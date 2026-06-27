//! Data model for the AI Catalog / Server Card auto-discovery feature.
//!
//! Implements the document shapes described in SEP-2127 ("Server Cards"):
//!   - An **AI Catalog** served at `GET https://<domain>/.well-known/ai-catalog.json`
//!     that lists the MCP servers (and Skills bundles) a domain advertises.
//!   - A **Server Card** (`application/mcp-server-card+json`) describing a single
//!     MCP server: identity, presentation, and *remote transport* configuration.
//!
//! The card intentionally excludes tool/resource/prompt listings — those are
//! discovered at runtime over the MCP protocol once the server is connected. The
//! card is advisory and MUST NOT be treated as authoritative for access control.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// The `/.well-known/ai-catalog.json` document.
///
/// A catalog references zero or more server cards (by URL or inline) and zero or
/// more Skills bundles. Fields are permissive so a partial/real-world catalog
/// still parses.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiCatalog {
    #[serde(default)]
    pub version: Option<String>,
    /// Servers advertised by this domain.
    #[serde(default)]
    pub servers: Vec<CatalogServerRef>,
    /// Skills bundles advertised by this domain.
    #[serde(default)]
    pub skills: Vec<SkillsBundle>,
}

/// A reference to a server card from inside a catalog. Either an inline `card`
/// or a `server_card` URL pointing at the `application/mcp-server-card+json`
/// document (recommended location: `<streamable-http-url>/server-card`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogServerRef {
    #[serde(default)]
    pub name: Option<String>,
    /// URL of the server card document to fetch.
    #[serde(default, alias = "serverCard", alias = "card_url", alias = "url")]
    pub server_card: Option<String>,
    /// Inline server card (lets a catalog ship cards without a second fetch).
    #[serde(default)]
    pub card: Option<ServerCard>,
}

/// A Server Card: advisory metadata for a single MCP server.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
    pub icons: Option<Icons>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub website: Option<String>,
    /// Remote transport configuration (streamable-http endpoint + auth).
    #[serde(default)]
    pub remote: Option<RemoteTransport>,
    /// Namespaced extension metadata. Advisory only.
    #[serde(default, rename = "_meta")]
    pub meta: serde_json::Value,
}

/// Light/dark icons. Values may be `https?://` URLs or `data:` URIs.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
pub struct Icons {
    #[serde(default)]
    pub light: Option<String>,
    #[serde(default)]
    pub dark: Option<String>,
}

impl Icons {
    /// Best icon to show on a (typically light) UI surface.
    pub fn any(&self) -> Option<&str> {
        self.light.as_deref().or(self.dark.as_deref())
    }
}

/// Remote transport block of a server card.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RemoteTransport {
    /// Transport kind. Only `streamable-http` is supported for install today.
    #[serde(default = "default_transport_type", rename = "type")]
    pub transport_type: String,
    /// The streamable-http endpoint URL.
    pub url: String,
    /// Static headers to send with every request.
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Protocol versions the server advertises support for.
    #[serde(default)]
    pub protocol_versions: Vec<String>,
    /// Auth requirement, if any. Absence means the server is auth-less.
    #[serde(default)]
    pub auth: Option<AuthRequirement>,
}

fn default_transport_type() -> String {
    "streamable-http".to_string()
}

/// Describes how a client must authenticate to a remote server.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SkillRef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub uri: Option<String>,
}

/// What a single discovery surfaces to the UI: either an installable MCP server
/// or a bundle of Skills.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiscoveryKind {
    Server(ServerCard),
    Skills(SkillsBundle),
}

/// A discovery surfaced by the sidecar: a server card or skills bundle found by
/// probing a domain Goose touched. This is the object the CLI and Desktop render
/// as an install nudge.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
}

impl Discovery {
    pub fn new_server(source_domain: &str, card: ServerCard, fixture: bool) -> Self {
        Discovery {
            id: format!("{}#{}", source_domain, card.name),
            source_domain: source_domain.to_string(),
            kind: DiscoveryKind::Server(card),
            fixture,
        }
    }

    pub fn new_skills(source_domain: &str, bundle: SkillsBundle, fixture: bool) -> Self {
        Discovery {
            id: format!("{}#skills:{}", source_domain, bundle.name),
            source_domain: source_domain.to_string(),
            kind: DiscoveryKind::Skills(bundle),
            fixture,
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
                .remote
                .as_ref()
                .map(|r| r.auth.is_some())
                .unwrap_or(false),
            DiscoveryKind::Skills(_) => false,
        }
    }

    pub fn server_card(&self) -> Option<&ServerCard> {
        match &self.kind {
            DiscoveryKind::Server(c) => Some(c),
            _ => None,
        }
    }
}
