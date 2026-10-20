use agent_client_protocol::{JsonRpcRequest, JsonRpcResponse};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::EmptyResponse;

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverySkillEntry {
    pub name: String,
    pub description: String,
}

/// A flat, UI-friendly projection of an AI Catalog / Server Card discovery.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryCardDto {
    pub id: String,
    /// "server" or "skills"
    pub kind: String,
    pub title: String,
    pub description: String,
    pub source_domain: String,
    /// True when the discovery came from the built-in demo fixtures rather than
    /// a live `/.well-known/ai-catalog.json` fetch.
    pub fixture: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub requires_auth: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub skills: Vec<DiscoverySkillEntry>,
    /// "inline" (a domain a tool call touched) or "background" (inferred intent).
    pub track: String,
    /// The intent the background reasoner inferred (background track only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryProbeDto {
    pub domain: String,
    pub url: String,
    pub outcome: String,
    #[serde(default)]
    pub found: Vec<String>,
}

/// One run of the background discovery reasoner, for the trace viewer.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryTraceDto {
    pub id: String,
    /// "reasoning", "probing", "done" or "failed".
    pub status: String,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    pub model: String,
    pub requests: Vec<String>,
    pub system_prompt: String,
    pub prompt: String,
    pub raw_output: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    pub candidate_domains: Vec<String>,
    pub probes: Vec<DiscoveryProbeDto>,
    pub decision: String,
}

/// List discoveries the background sidecar surfaced for a session.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(
    method = "_goose/unstable/discovery/pending",
    response = DiscoveryPendingResponse
)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryPendingRequest {
    pub session_id: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryPendingResponse {
    /// Active capture mode: "off", "light" or "full".
    pub mode: String,
    pub discoveries: Vec<DiscoveryCardDto>,
    /// Whether the background (dual-track) reasoner is enabled.
    #[serde(default)]
    pub background_enabled: bool,
    /// True while a background reasoner run is in flight.
    #[serde(default)]
    pub background_running: bool,
    /// Background reasoner runs for this session, oldest first.
    #[serde(default)]
    pub traces: Vec<DiscoveryTraceDto>,
}

/// Install a discovered MCP server into the session (or acknowledge a Skills bundle).
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(
    method = "_goose/unstable/discovery/install",
    response = DiscoveryInstallResponse
)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryInstallRequest {
    pub session_id: String,
    pub id: String,
    /// Auth secret for cards that require one (e.g. a GitHub token).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcResponse)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryInstallResponse {
    /// "server" or "skills"
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension_name: Option<String>,
    /// Tools the newly installed extension exposes in this session.
    pub tools: Vec<String>,
    pub message: String,
}

/// Dismiss a discovery so it is not surfaced again in this session.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcRequest)]
#[request(method = "_goose/unstable/discovery/dismiss", response = EmptyResponse)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryDismissRequest {
    pub session_id: String,
    pub id: String,
}
