//! The discovery sidecar.
//!
//! [`DiscoveryService`] watches the domains Goose touches and, in the background,
//! probes them for AI Catalogs / Server Cards without ever blocking the agent
//! loop. Captured domains are de-duplicated and negative-cached; each probe runs
//! on its own detached task. Results land in a `pending` list the CLI and Desktop
//! surface as install nudges.
//!
//! Two capture modes (a user-facing config setting, `discovery_mode`):
//!   * **Full** — every domain that appears in *any* tool call's arguments.
//!   * **Light-touch** — only domains in fetch/search/browse-style tool calls
//!     (pre-tool probing on fetch/browse tools, per SEP-2127).

use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::agents::extension::ExtensionConfig;
use crate::config::Config;

use super::fixtures;
use super::resolver::{CatalogResolver, DefaultResolver};
use super::types::{Discovery, DiscoveryKind, ServerCard};

/// How aggressively the sidecar captures domains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryMode {
    /// Discovery disabled.
    Off,
    /// Capture only domains touched by fetch/search/browse tools.
    Light,
    /// Capture every domain that appears in any tool call.
    Full,
}

impl DiscoveryMode {
    pub fn from_config_str(s: &str) -> DiscoveryMode {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "disabled" | "none" | "false" => DiscoveryMode::Off,
            "full" | "all" | "egress" => DiscoveryMode::Full,
            // Default to the conservative mode for anything else (incl. "light").
            _ => DiscoveryMode::Light,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            DiscoveryMode::Off => "off",
            DiscoveryMode::Light => "light",
            DiscoveryMode::Full => "full",
        }
    }
}

const CONFIG_KEY_MODE: &str = "discovery_mode";
const CONFIG_KEY_HTTP: &str = "discovery_http_probe";

#[derive(Default)]
struct DiscoveryState {
    /// Domains already enqueued/probed (positive or negative) — dedup guard.
    seen_domains: HashSet<String>,
    /// Domains probed that advertised nothing.
    negative: HashSet<String>,
    /// Discoveries found, awaiting user action.
    pending: Vec<Discovery>,
    /// Discovery ids already surfaced to the CLI (so we don't re-nudge inline).
    surfaced: HashSet<String>,
    /// Discovery ids the user installed.
    installed: HashSet<String>,
    /// Discovery ids the user dismissed.
    dismissed: HashSet<String>,
}

/// Background domain-watcher + catalog prober. Cheap to clone the `Arc`.
pub struct DiscoveryService {
    mode: RwLock<DiscoveryMode>,
    resolver: Arc<dyn CatalogResolver>,
    state: Arc<RwLock<DiscoveryState>>,
}

impl DiscoveryService {
    /// Build a service from global config (`discovery_mode`, `discovery_http_probe`).
    pub fn from_config() -> Self {
        let config = Config::global();
        let mode = config
            .get_param::<String>(CONFIG_KEY_MODE)
            .ok()
            .map(|s| DiscoveryMode::from_config_str(&s))
            .unwrap_or(DiscoveryMode::Light);
        let enable_http = config.get_param::<bool>(CONFIG_KEY_HTTP).unwrap_or(true);
        DiscoveryService::new(mode, Arc::new(DefaultResolver::new(enable_http)))
    }

    pub fn new(mode: DiscoveryMode, resolver: Arc<dyn CatalogResolver>) -> Self {
        DiscoveryService {
            mode: RwLock::new(mode),
            resolver,
            state: Arc::new(RwLock::new(DiscoveryState::default())),
        }
    }

    pub fn mode(&self) -> DiscoveryMode {
        *self.mode.read().unwrap()
    }

    pub fn set_mode(&self, mode: DiscoveryMode) {
        *self.mode.write().unwrap() = mode;
    }

    /// Observe a tool call. Based on the active mode, extracts candidate domains
    /// from the (already serialized) tool arguments and kicks off background probes.
    /// Never blocks: returns immediately.
    pub fn observe_tool_call(&self, tool_name: &str, args_text: &str) {
        match self.mode() {
            DiscoveryMode::Off => return,
            DiscoveryMode::Light if !is_fetch_like_tool(tool_name) => return,
            _ => {}
        }
        for host in extract_hosts(args_text) {
            self.observe_domain(&host);
        }
    }

    /// Observe a raw destination host (any direct egress hook can call this).
    /// De-dups, then spawns a detached probe task so the agent loop is unaffected.
    pub fn observe_domain(&self, raw_host: &str) {
        if self.mode() == DiscoveryMode::Off {
            return;
        }
        let host = normalize_host(raw_host);
        if host.is_empty() {
            return;
        }
        {
            let mut st = self.state.write().unwrap();
            // insert returns false if already present → already probed/queued.
            if !st.seen_domains.insert(host.clone()) {
                return;
            }
        }
        let resolver = self.resolver.clone();
        let state = self.state.clone();
        tokio::spawn(async move {
            let discoveries = resolver.resolve(&host).await;
            let mut st = state.write().unwrap();
            if discoveries.is_empty() {
                st.negative.insert(host);
                return;
            }
            for d in discoveries {
                if st.installed.contains(&d.id)
                    || st.dismissed.contains(&d.id)
                    || st.pending.iter().any(|p| p.id == d.id)
                {
                    continue;
                }
                tracing::info!(
                    discovery_id = %d.id,
                    domain = %d.source_domain,
                    fixture = d.fixture,
                    "ai-catalog discovery surfaced"
                );
                st.pending.push(d);
            }
        });
    }

    /// All currently-pending discoveries (not installed, not dismissed).
    pub fn pending(&self) -> Vec<Discovery> {
        self.state.read().unwrap().pending.clone()
    }

    /// Discoveries that have not yet been shown inline by the CLI. Marks them
    /// surfaced so the same nudge isn't printed every turn.
    pub fn take_unsurfaced(&self) -> Vec<Discovery> {
        let mut st = self.state.write().unwrap();
        let fresh: Vec<Discovery> = st
            .pending
            .iter()
            .filter(|d| !st.surfaced.contains(&d.id))
            .cloned()
            .collect();
        for d in &fresh {
            st.surfaced.insert(d.id.clone());
        }
        fresh
    }

    pub fn get(&self, id: &str) -> Option<Discovery> {
        self.state
            .read()
            .unwrap()
            .pending
            .iter()
            .find(|d| d.id == id)
            .cloned()
    }

    pub fn mark_installed(&self, id: &str) {
        let mut st = self.state.write().unwrap();
        st.installed.insert(id.to_string());
        st.pending.retain(|d| d.id != id);
    }

    pub fn mark_dismissed(&self, id: &str) {
        let mut st = self.state.write().unwrap();
        st.dismissed.insert(id.to_string());
        st.pending.retain(|d| d.id != id);
    }

    /// Number of pending discoveries — cheap check for the UI loop.
    pub fn pending_count(&self) -> usize {
        self.state.read().unwrap().pending.len()
    }
}

/// Tool-name heuristic for light-touch mode: matches fetch/search/browse tools
/// (names are namespaced, e.g. `computercontroller__web_scrape`).
fn is_fetch_like_tool(tool_name: &str) -> bool {
    let n = tool_name.to_ascii_lowercase();
    const HINTS: &[&str] = &[
        "fetch", "search", "scrape", "browse", "crawl", "web", "url", "open_url", "navigate",
        "read_web", "http_get", "wget", "curl",
    ];
    HINTS.iter().any(|h| n.contains(h))
}

static URL_HOST_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)https?://([a-z0-9.\-]+)").expect("valid url host regex"));

/// Extract candidate hosts from arbitrary text (serialized tool args). Captures
/// scheme-qualified URLs plus any bare occurrence of a known fixture domain.
pub fn extract_hosts(text: &str) -> Vec<String> {
    let mut hosts: Vec<String> = Vec::new();
    let mut push = |h: String| {
        let h = normalize_host(&h);
        if !h.is_empty() && !hosts.contains(&h) {
            hosts.push(h);
        }
    };
    for cap in URL_HOST_RE.captures_iter(text) {
        if let Some(m) = cap.get(1) {
            push(m.as_str().to_string());
        }
    }
    // Bare-domain hint: a tool arg like {"url":"modelcontextprotocol.io"} or a
    // mention without a scheme should still trigger the demo fixtures.
    let lower = text.to_ascii_lowercase();
    for d in fixtures::fixture_domains() {
        if lower.contains(d) {
            push(d.to_string());
        }
    }
    hosts
}

/// Normalize a host: lowercase, strip trailing dot, drop a leading `www.`.
fn normalize_host(host: &str) -> String {
    let mut h = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if let Some(stripped) = h.strip_prefix("www.") {
        h = stripped.to_string();
    }
    // Guard against obvious non-hosts.
    if !h.contains('.') || h.contains(' ') {
        return String::new();
    }
    h
}

/// A short, filesystem/key-safe extension name derived from a card's reverse-DNS
/// identity (e.g. `io.github.github/github-mcp-server` → `github-mcp-server`).
pub fn extension_name_for(card: &ServerCard) -> String {
    let base = card.name.rsplit('/').next().unwrap_or(&card.name);
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "discovered-server".to_string()
    } else {
        cleaned
    }
}

/// Build a live `StreamableHttp` extension config from a server card, injecting
/// the auth secret as the configured header when the card requires one.
pub fn server_card_to_extension(
    card: &ServerCard,
    secret: Option<&str>,
) -> Result<ExtensionConfig, String> {
    let remote = card
        .remote
        .as_ref()
        .ok_or_else(|| "server card has no remote transport".to_string())?;
    if !remote
        .transport_type
        .eq_ignore_ascii_case("streamable-http")
    {
        return Err(format!(
            "unsupported transport type: {}",
            remote.transport_type
        ));
    }

    let mut headers = remote.headers.clone();
    if let Some(auth) = &remote.auth {
        if auth.is_oauth() {
            // OAuth handled by goose's existing streamable-http OAuth flow on connect.
        } else {
            let header_name = auth
                .header
                .clone()
                .unwrap_or_else(|| "Authorization".to_string());
            let secret = secret
                .ok_or_else(|| format!("auth required: provide a value for {}", header_name))?;
            let prefix = auth.prefix.clone().unwrap_or_default();
            headers.insert(header_name, format!("{}{}", prefix, secret));
        }
    }

    Ok(ExtensionConfig::StreamableHttp {
        name: extension_name_for(card),
        description: card.description.clone(),
        uri: remote.url.clone(),
        envs: Default::default(),
        env_keys: Vec::new(),
        headers,
        timeout: Some(300),
        socket: None,
        bundled: None,
        available_tools: Vec::new(),
    })
}

/// Convenience: turn a discovery into an extension config (server discoveries only).
pub fn discovery_to_extension(
    discovery: &Discovery,
    secret: Option<&str>,
) -> Result<ExtensionConfig, String> {
    match &discovery.kind {
        DiscoveryKind::Server(card) => server_card_to_extension(card, secret),
        DiscoveryKind::Skills(_) => {
            Err("skills bundles are not installed as MCP extensions".to_string())
        }
    }
}
