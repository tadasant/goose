//! Background ("dual-track") discovery.
//!
//! The inline sidecar reacts to domains a tool call actually touched. This track
//! runs a cheap one-shot reasoner beside the main agent at the start of each turn:
//! it infers what the user is trying to do (across their recent requests), names
//! the official domains whose AI Catalog could serve that intent, and probes each
//! `/.well-known/ai-catalog.json` through the same resolver. Anything found is
//! queued as an ordinary install nudge. Every step is recorded in a
//! [`ReasonerTrace`] so the UI can show exactly why something was (or was not)
//! suggested.

use std::collections::HashSet;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;

use futures::FutureExt;

use chrono::{DateTime, Utc};
use goose_providers::model::ModelConfig;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::conversation::message::Message;
use crate::providers::base::Provider;

use super::resolver::is_probeable_host;
use super::service::{normalize_host, DiscoveryService};
use super::types::{Discovery, DiscoveryKind, DiscoveryOrigin};

const CONFIG_KEY_MODEL: &str = "discovery_background_model";
const MAX_CANDIDATE_DOMAINS: usize = 3;
const MAX_REQUEST_CHARS: usize = 500;
const REASONER_TIMEOUT: Duration = Duration::from_secs(30);

pub const REASONER_SYSTEM_PROMPT: &str =
    "You are goose's background AI Catalog discovery reasoner. \
You run beside the main agent and never talk to the user. Given the user's recent requests, \
infer what they are trying to accomplish and decide whether an official MCP server or Agent \
Skills bundle published by some organisation would help. For each such organisation, name the \
official primary domain where it would publish /.well-known/ai-catalog.json (for example, work \
on the user's GitHub repositories -> github.com). Only name official domains you are confident \
about; never guess look-alike or third-party domains. Use at most 3 domains, and an empty list \
when nothing is relevant. Respond with ONLY a JSON object: \
{\"intent\": \"<one short sentence>\", \"domains\": [\"<domain>\"], \"rationale\": \"<why>\"}";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceStatus {
    Reasoning,
    Probing,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeRecord {
    pub domain: String,
    pub url: String,
    pub outcome: String,
    pub found: Vec<String>,
}

/// Everything the background reasoner did for one user turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasonerTrace {
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: TraceStatus,
    pub model: String,
    pub requests: Vec<String>,
    pub system_prompt: String,
    pub prompt: String,
    pub raw_output: String,
    pub intent: Option<String>,
    pub rationale: Option<String>,
    pub candidate_domains: Vec<String>,
    pub probes: Vec<ProbeRecord>,
    pub decision: String,
}

impl ReasonerTrace {
    pub fn is_running(&self) -> bool {
        matches!(self.status, TraceStatus::Reasoning | TraceStatus::Probing)
    }
}

#[derive(Debug, Deserialize)]
struct ReasonerOutput {
    #[serde(default)]
    intent: String,
    #[serde(default)]
    domains: Vec<String>,
    #[serde(default)]
    rationale: String,
}

fn parse_reasoner_output(text: &str) -> Option<ReasonerOutput> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    serde_json::from_str(text.get(start..=end)?).ok()
}

/// Policy hook for auto-install. An AI Catalog entry may carry a `trustManifest`;
/// a future policy check (publisher identity, signature, allow-listed domains)
/// would decide here whether a suggestion may be installed without asking.
/// Domains the reasoner *guessed* are never trusted, so today this always
/// declines and every background result is a nudge.
pub fn may_auto_install(_discovery: &Discovery) -> bool {
    false
}

fn entries(n: usize) -> String {
    if n == 1 {
        "1 entry".to_string()
    } else {
        format!("{n} entries")
    }
}

fn describe(d: &Discovery) -> String {
    let kind = match d.kind {
        DiscoveryKind::Server(_) => "MCP server",
        DiscoveryKind::Skills(_) => "Skills bundle",
    };
    format!("{kind}: {}", d.title())
}

impl DiscoveryService {
    /// Start the background reasoner for this turn. Returns immediately; the
    /// reasoner never blocks or cancels with the main agent's turn.
    pub fn spawn_background_reasoner(
        self: &Arc<Self>,
        provider: Arc<dyn Provider>,
        model_config: ModelConfig,
        session_id: String,
        user_text: String,
    ) {
        if !self.background_enabled() || user_text.trim().is_empty() {
            return;
        }
        let request: String = user_text.chars().take(MAX_REQUEST_CHARS).collect();
        let requests = self.note_user_request(&request);
        let mut model_config = model_config;
        if let Ok(model) = Config::global().get_param::<String>(CONFIG_KEY_MODEL) {
            model_config.model_name = model;
        }
        let prompt = format!(
            "Recent user requests (oldest first):\n{}\n\nWhich official domains' AI Catalogs are relevant?",
            requests
                .iter()
                .enumerate()
                .map(|(i, r)| format!("{}. {}", i + 1, r))
                .collect::<Vec<_>>()
                .join("\n")
        );
        let mut trace = ReasonerTrace {
            id: uuid::Uuid::new_v4().to_string(),
            started_at: Utc::now(),
            finished_at: None,
            status: TraceStatus::Reasoning,
            model: format!("{} / {}", provider.get_name(), model_config.model_name),
            requests,
            system_prompt: REASONER_SYSTEM_PROMPT.to_string(),
            prompt,
            raw_output: String::new(),
            intent: None,
            rationale: None,
            candidate_domains: Vec::new(),
            probes: Vec::new(),
            decision: "reasoning about intent…".to_string(),
        };
        self.upsert_trace(&trace);

        let service = Arc::clone(self);
        tokio::spawn(async move {
            let run = AssertUnwindSafe(tokio::time::timeout(
                REASONER_TIMEOUT,
                service.run_reasoner(&mut trace, provider, model_config, &session_id),
            ))
            .catch_unwind()
            .await;
            let failure = match run {
                Ok(Ok(())) => None,
                Ok(Err(_)) => Some("reasoner timed out; nothing to suggest"),
                Err(_) => Some("reasoner crashed; nothing to suggest"),
            };
            if let Some(reason) = failure {
                trace.status = TraceStatus::Failed;
                trace.decision = reason.to_string();
            }
            trace.finished_at = Some(Utc::now());
            service.upsert_trace(&trace);
        });
    }

    async fn run_reasoner(
        &self,
        trace: &mut ReasonerTrace,
        provider: Arc<dyn Provider>,
        model_config: ModelConfig,
        session_id: &str,
    ) {
        let messages = [Message::user().with_text(&trace.prompt)];
        let response = crate::model_config::complete_one_shot(
            provider.as_ref(),
            &model_config,
            session_id,
            REASONER_SYSTEM_PROMPT,
            &messages,
            &[],
        )
        .await;
        let text = match response {
            Ok((message, _usage)) => message.as_concat_text(),
            Err(e) => {
                trace.status = TraceStatus::Failed;
                trace.decision = format!("reasoner call failed: {e}");
                return;
            }
        };
        trace.raw_output = text.clone();
        let Some(output) = parse_reasoner_output(&text) else {
            trace.status = TraceStatus::Failed;
            trace.decision = "reasoner output was not the expected JSON; nothing to suggest".into();
            return;
        };
        trace.intent = Some(output.intent.clone()).filter(|s| !s.is_empty());
        trace.rationale = Some(output.rationale).filter(|s| !s.is_empty());
        let mut seen = HashSet::new();
        let domains: Vec<String> = output
            .domains
            .iter()
            .map(|d| normalize_host(d))
            .filter(|d| {
                d.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
                    && is_probeable_host(d)
            })
            .filter(|d| seen.insert(d.clone()))
            .take(MAX_CANDIDATE_DOMAINS)
            .collect();
        trace.candidate_domains = domains.clone();
        if domains.is_empty() {
            trace.status = TraceStatus::Done;
            trace.decision =
                "no relevant AI Catalog domains for this intent; nothing to suggest".into();
            return;
        }

        trace.status = TraceStatus::Probing;
        trace.decision = format!("probing {} candidate domain(s)…", domains.len());
        self.upsert_trace(trace);

        let intent = trace.intent.clone().unwrap_or_default();
        let mut found = Vec::new();
        for domain in domains {
            let url = format!("https://{domain}/.well-known/ai-catalog.json");
            if !self.claim_background_domain(&domain) {
                trace.probes.push(ProbeRecord {
                    domain,
                    url,
                    outcome: "skipped: already probed earlier in this session".into(),
                    found: Vec::new(),
                });
                continue;
            }
            let discoveries = self.resolver.resolve(&domain).await;
            trace.probes.push(ProbeRecord {
                domain: domain.clone(),
                url,
                outcome: if discoveries.is_empty() {
                    "no catalog (or no installable entries)".into()
                } else if discoveries.iter().all(|d| d.fixture) {
                    format!("{} from demo fixture", entries(discoveries.len()))
                } else {
                    format!("{} in live ai-catalog.json", entries(discoveries.len()))
                },
                found: discoveries.iter().map(describe).collect(),
            });
            found.extend(discoveries.into_iter().map(|mut d| {
                d.origin = DiscoveryOrigin::Background {
                    intent: intent.clone(),
                    trace_id: trace.id.clone(),
                };
                d
            }));
            self.upsert_trace(trace);
        }

        trace.status = TraceStatus::Done;
        if found.is_empty() {
            let all_skipped = trace
                .probes
                .iter()
                .all(|p| p.outcome.starts_with("skipped"));
            trace.decision = if all_skipped {
                "candidate domains were already probed earlier in this session; nothing new to suggest"
                    .into()
            } else {
                "no catalog entries found on candidate domains; nothing to suggest".into()
            };
            return;
        }
        let auto = found.iter().filter(|d| may_auto_install(d)).count();
        let titles: Vec<String> = found.iter().map(describe).collect();
        let queued = self.queue_background(&intent, found);
        trace.decision = if queued.is_empty() {
            format!(
                "found {} but already suggested, installed or dismissed for this intent; nothing new to suggest",
                titles.join(", ")
            )
        } else {
            format!(
                "suggest installing {} (nudge only: auto-install declined for {} from guessed domains, no trustManifest policy)",
                titles.join(", "),
                entries(titles.len() - auto)
            )
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fenced_reasoner_json() {
        let out = parse_reasoner_output(
            "```json\n{\"intent\":\"summarise my GitHub PRs\",\"domains\":[\"github.com\"],\"rationale\":\"GitHub MCP\"}\n```",
        )
        .unwrap();
        assert_eq!(out.intent, "summarise my GitHub PRs");
        assert_eq!(out.domains, vec!["github.com"]);
    }

    #[test]
    fn rejects_non_json_output() {
        assert!(parse_reasoner_output("I think GitHub").is_none());
    }
}
