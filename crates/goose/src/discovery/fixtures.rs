//! Demo fixtures for AI Catalog / Server Card discovery.
//!
//! Hand-authored cards for domains that do not (yet) publish a real
//! `/.well-known/ai-catalog.json`. Each fixture is clearly labeled
//! (`Discovery.fixture = true`) so demo content is never confused with a live
//! discovery. Domains that do publish a catalog (e.g. `github.com`,
//! `huggingface.co`) are resolved live by the HTTP resolver instead.
//!
//! Two fixtures are wired:
//!
//!   * `modelcontextprotocol.io` — a *minimal real* server card for the public,
//!     auth-less MCP server at `https://modelcontextprotocol.io/mcp`. The card is
//!     hand-authored here, but the endpoint and its `search_model_context_protocol`
//!     tool are real: installing actually connects and the tool actually works.
//!     A separate Skills bundle mirrors the real `mintlify://skills/*` resources
//!     that endpoint exposes.
//!
//!   * `api.github.com` — a mocked GitHub MCP server card with an explicit
//!     `Authorization` header-auth hint, used to drive the auth-entry install
//!     flow. (GitHub's real card, discovered live via `github.com`, carries no
//!     auth hint and relies on OAuth at connect time.)

use super::types::{
    AuthRequirement, Discovery, Icon, RemoteTransport, Repository, ServerCard, SkillRef,
    SkillsBundle,
};

/// A tiny inline SVG icon (rendered fine in both CLI metadata and Desktop card).
fn mcp_icon() -> String {
    // Simple "MCP" mark on a slate background.
    "data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='48' height='48'>\
     <rect width='48' height='48' rx='10' fill='%23111827'/>\
     <text x='24' y='31' font-family='monospace' font-size='14' fill='%2360a5fa' \
     text-anchor='middle'>MCP</text></svg>"
        .to_string()
}

fn github_icon() -> String {
    "data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='48' height='48'>\
     <rect width='48' height='48' rx='10' fill='%23111827'/>\
     <text x='24' y='31' font-family='monospace' font-size='20' fill='%23ffffff' \
     text-anchor='middle'>GH</text></svg>"
        .to_string()
}

/// The auth-less Model Context Protocol docs server. Endpoint is real.
pub fn modelcontextprotocol_card() -> ServerCard {
    ServerCard {
        name: "io.modelcontextprotocol/docs-server".to_string(),
        version: Some("1.0.0".to_string()),
        title: Some("Model Context Protocol Docs".to_string()),
        description: "Search and retrieval over the Model Context Protocol site. \
            Exposes search_model_context_protocol and a read-only docs filesystem."
            .to_string(),
        icons: vec![Icon {
            src: mcp_icon(),
            mime_type: Some("image/svg+xml".to_string()),
            theme: None,
        }],
        repository: Some(Repository {
            url: "https://github.com/modelcontextprotocol/modelcontextprotocol".to_string(),
            source: Some("github".to_string()),
        }),
        website_url: Some("https://modelcontextprotocol.io".to_string()),
        remotes: vec![RemoteTransport {
            transport_type: "streamable-http".to_string(),
            url: "https://modelcontextprotocol.io/mcp".to_string(),
            headers: Vec::new(),
            supported_protocol_versions: vec!["2025-06-18".to_string(), "2025-03-26".to_string()],
            auth: None,
        }],
        meta: serde_json::json!({
            "io.modelcontextprotocol/sep": 2127,
            "demo": "auth-less headline demo; endpoint is live"
        }),
    }
}

/// Skills bundle mirroring the real `mintlify://skills/*` resources the docs
/// server exposes.
pub fn modelcontextprotocol_skills() -> SkillsBundle {
    SkillsBundle {
        name: "MCP Authoring Skills".to_string(),
        description: "Guided skills for working with the Model Context Protocol: \
            drafting SEPs and searching MCP GitHub activity."
            .to_string(),
        source: Some("https://modelcontextprotocol.io/mcp".to_string()),
        skills: vec![
            SkillRef {
                name: "draft-sep".to_string(),
                description: "Research and draft a Specification Enhancement Proposal \
                    following the MCP SEP governance process."
                    .to_string(),
                uri: Some("mintlify://skills/draft-sep".to_string()),
            },
            SkillRef {
                name: "search-mcp-github".to_string(),
                description: "Search MCP PRs, issues, and discussions across the \
                    modelcontextprotocol GitHub org."
                    .to_string(),
                uri: Some("mintlify://skills/search-mcp-github".to_string()),
            },
        ],
    }
}

/// The GitHub MCP server card (mocked; auth-entry demo).
pub fn github_card() -> ServerCard {
    ServerCard {
        name: "io.github.github/github-mcp-server".to_string(),
        version: Some("1.4.0".to_string()),
        title: Some("GitHub MCP Server".to_string()),
        description: "GitHub's official MCP server: repositories, issues, pull \
            requests, Actions, and code search over the GitHub API."
            .to_string(),
        icons: vec![Icon {
            src: github_icon(),
            mime_type: Some("image/svg+xml".to_string()),
            theme: None,
        }],
        repository: Some(Repository {
            url: "https://github.com/github/github-mcp-server".to_string(),
            source: Some("github".to_string()),
        }),
        website_url: Some("https://github.com".to_string()),
        remotes: vec![RemoteTransport {
            transport_type: "streamable-http".to_string(),
            url: "https://api.githubcopilot.com/mcp/".to_string(),
            headers: Vec::new(),
            supported_protocol_versions: vec!["2025-06-18".to_string()],
            auth: Some(AuthRequirement {
                kind: "header".to_string(),
                header: Some("Authorization".to_string()),
                prefix: Some("Bearer ".to_string()),
                variable: Some("github_token".to_string()),
                instructions: Some(
                    "Paste a GitHub Personal Access Token (or GitHub App token) with \
                     repo scope. It is sent as the Authorization header to \
                     api.githubcopilot.com and stored only in this session."
                        .to_string(),
                ),
            }),
        }],
        meta: serde_json::json!({
            "io.modelcontextprotocol/sep": 2127,
            "demo": "mocked card; drives the auth-entry flow"
        }),
    }
}

/// Build the fixture discoveries for a (normalized) domain, if any.
pub fn fixtures_for(domain: &str) -> Vec<Discovery> {
    match domain {
        "modelcontextprotocol.io" => vec![
            Discovery::new_server(domain, modelcontextprotocol_card(), true),
            Discovery::new_skills(domain, modelcontextprotocol_skills(), true),
        ],
        "api.github.com" => vec![Discovery::new_server(domain, github_card(), true)],
        _ => vec![],
    }
}

/// Known catalog domains (fixture-backed or live). Used as extra "hint"
/// substrings so a bare hostname (no `https://`) in a tool call still triggers
/// a probe.
pub fn fixture_domains() -> &'static [&'static str] {
    &["modelcontextprotocol.io", "github.com", "api.github.com"]
}
