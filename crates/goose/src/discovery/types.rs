//! Data model for the AI Catalog / Server Card auto-discovery feature.
//!
//! Implements two published document shapes:
//!   - An **AI Catalog** (`specVersion` 1.0, <https://github.com/Agent-Card/ai-catalog>)
//!     served at `GET https://<domain>/.well-known/ai-catalog.json`, whose typed
//!     `entries` point at MCP Server Cards, Agent Skills, and other artifacts.
//!   - A **Server Card** (SEP-2127, final; <https://github.com/modelcontextprotocol/ext-server-card>,
//!     `application/mcp-server-card+json`) describing a single MCP server:
//!     identity, presentation, and its `remotes` (transport endpoints).
//!
//! The card intentionally excludes tool/resource/prompt listings — those are
//! discovered at runtime over the MCP protocol once the server is connected. The
//! card is advisory and MUST NOT be treated as authoritative for access control.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Media type of an MCP Server Card entry in an AI Catalog.
pub const SERVER_CARD_MEDIA_TYPE: &str = "application/mcp-server-card+json";
/// Media type of an Agent Skills metadata entry in an AI Catalog.
pub const AGENT_SKILLS_MEDIA_TYPE: &str = "application/agent-skills+json";

/// The `/.well-known/ai-catalog.json` document (AI Catalog `specVersion` 1.0).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AiCatalog {
    #[serde(default)]
    pub spec_version: Option<String>,
    #[serde(default)]
    pub entries: Vec<CatalogEntry>,
}

/// One catalog entry. Exactly one of `url` (fetch the artifact) or `data` (the
/// artifact inline) is set; `type` is the artifact's media type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub identifier: String,
    #[serde(rename = "type")]
    pub media_type: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// A Server Card (SEP-2127, `application/mcp-server-card+json`): advisory
/// metadata for a single MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCard {
    /// Reverse-DNS style identity, e.g. `io.github.github/github-mcp-server`.
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub icons: Vec<Icon>,
    #[serde(default)]
    pub repository: Option<Repository>,
    #[serde(default)]
    pub website_url: Option<String>,
    #[serde(default)]
    pub remotes: Vec<RemoteTransport>,
    /// Namespaced extension metadata. Advisory only.
    #[serde(default, rename = "_meta")]
    pub meta: serde_json::Value,
}

impl ServerCard {
    /// The first remote goose can install (streamable-http).
    pub fn primary_remote(&self) -> Option<&RemoteTransport> {
        self.remotes
            .iter()
            .find(|r| r.transport_type.eq_ignore_ascii_case("streamable-http"))
    }

    /// Best icon for a (typically light) UI surface.
    pub fn icon_url(&self) -> Option<&str> {
        self.icons
            .iter()
            .find(|i| i.theme.as_deref() != Some("dark"))
            .or(self.icons.first())
            .map(|i| i.src.as_str())
    }
}

/// An icon; `src` may be an `https://` URL or a `data:` URI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Icon {
    pub src: String,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub theme: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub url: String,
    #[serde(default)]
    pub source: Option<String>,
}

/// One entry of a card's `remotes` array.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteTransport {
    /// `streamable-http` or `sse`. Only `streamable-http` is installable today.
    #[serde(rename = "type")]
    pub transport_type: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<HeaderInput>,
    #[serde(default)]
    pub supported_protocol_versions: Vec<String>,
    /// Non-standard demo extension: explicit header-auth hint (used by the
    /// mocked GitHub fixture). Spec cards express secrets via `headers`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthRequirement>,
}

impl RemoteTransport {
    /// The secret the user must supply before connecting, if any: the explicit
    /// `auth` hint, else the first required secret header without a value.
    pub fn auth_requirement(&self) -> Option<AuthRequirement> {
        if let Some(auth) = &self.auth {
            return Some(auth.clone());
        }
        self.headers
            .iter()
            .find(|h| h.is_required && h.is_secret && h.value.is_none())
            .map(|h| AuthRequirement {
                kind: "header".to_string(),
                header: Some(h.name.clone()),
                prefix: None,
                variable: Some(h.name.clone()),
                instructions: h.description.clone(),
            })
    }

    /// Headers with a fixed, non-templated value.
    pub fn static_headers(&self) -> HashMap<String, String> {
        self.headers
            .iter()
            .filter_map(|h| {
                let value = h.value.as_ref()?;
                (!value.contains('{')).then(|| (h.name.clone(), value.clone()))
            })
            .collect()
    }
}

/// A `KeyValueInput` from the Server Card schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderInput {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub is_required: bool,
    #[serde(default)]
    pub is_secret: bool,
}

/// Describes how a client must authenticate to a remote server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthRequirement {
    /// `header` (a static secret placed in a request header) or `oauth`.
    pub kind: String,
    /// For `header` auth: the header name, e.g. `Authorization`.
    #[serde(default)]
    pub header: Option<String>,
    /// For `header` auth: an optional value prefix, e.g. `Bearer `.
    #[serde(default)]
    pub prefix: Option<String>,
    /// Logical name of the secret to collect from the user, e.g. `github_token`.
    #[serde(default)]
    pub variable: Option<String>,
    /// Human-readable guidance shown when collecting the secret.
    #[serde(default)]
    pub instructions: Option<String>,
}

impl AuthRequirement {
    pub fn is_oauth(&self) -> bool {
        self.kind.eq_ignore_ascii_case("oauth")
    }
}

/// A bundle of Skills advertised alongside a domain's servers. Surfaced as its
/// own discovery entry so the install nudge can offer Skills separately from the
/// MCP server itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsBundle {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Where the skills can be loaded from (catalog/site URL or MCP endpoint).
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub skills: Vec<SkillRef>,
}

/// A single Skill inside a bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub uri: Option<String>,
}

/// What a single discovery surfaces to the UI: either an installable MCP server
/// or a bundle of Skills.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiscoveryKind {
    // Boxed: `ServerCard` is much larger than `SkillsBundle`, so an unboxed
    // variant bloats every `DiscoveryKind`/`Discovery` (clippy::large_enum_variant).
    Server(Box<ServerCard>),
    Skills(SkillsBundle),
}

/// A discovery surfaced by the sidecar: a server card or skills bundle found by
/// probing a domain Goose touched. This is the object the CLI and Desktop render
/// as an install nudge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Discovery {
    /// Stable id (`<source_domain>#<name>`) used to install / dismiss.
    pub id: String,
    /// The domain whose catalog produced this discovery.
    pub source_domain: String,
    pub kind: DiscoveryKind,
    /// True when this came from the built-in demo fixture layer rather than a
    /// live `/.well-known/ai-catalog.json` fetch. Surfaced in the UI so demo
    /// content is never mistaken for a real, authenticated discovery.
    #[serde(default)]
    pub fixture: bool,
    #[serde(default)]
    pub origin: DiscoveryOrigin,
}

/// Which track surfaced a discovery: the inline sidecar (a domain a tool call
/// touched) or the background reasoner (a domain inferred from user intent).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "track", rename_all = "snake_case")]
pub enum DiscoveryOrigin {
    #[default]
    Inline,
    Background {
        intent: String,
        trace_id: String,
    },
}

impl Discovery {
    pub fn new_server(source_domain: &str, card: ServerCard, fixture: bool) -> Self {
        Discovery {
            id: format!("{}#{}", source_domain, card.name),
            source_domain: source_domain.to_string(),
            kind: DiscoveryKind::Server(Box::new(card)),
            fixture,
            origin: DiscoveryOrigin::Inline,
        }
    }

    pub fn new_skills(source_domain: &str, bundle: SkillsBundle, fixture: bool) -> Self {
        Discovery {
            id: format!("{}#skills:{}", source_domain, bundle.name),
            source_domain: source_domain.to_string(),
            kind: DiscoveryKind::Skills(bundle),
            fixture,
            origin: DiscoveryOrigin::Inline,
        }
    }

    /// Display title for the nudge.
    pub fn title(&self) -> String {
        match &self.kind {
            DiscoveryKind::Server(c) => c.title.clone().unwrap_or_else(|| c.name.clone()),
            DiscoveryKind::Skills(b) => b.name.clone(),
        }
    }

    pub fn description(&self) -> &str {
        match &self.kind {
            DiscoveryKind::Server(c) => &c.description,
            DiscoveryKind::Skills(b) => &b.description,
        }
    }

    pub fn is_server(&self) -> bool {
        matches!(self.kind, DiscoveryKind::Server(_))
    }

    /// True if installing this server requires the user to supply a secret.
    pub fn requires_auth(&self) -> bool {
        match &self.kind {
            DiscoveryKind::Server(c) => c
                .primary_remote()
                .and_then(|r| r.auth_requirement())
                .is_some(),
            DiscoveryKind::Skills(_) => false,
        }
    }

    pub fn server_card(&self) -> Option<&ServerCard> {
        match &self.kind {
            DiscoveryKind::Server(c) => Some(c.as_ref()),
            _ => None,
        }
    }
}
