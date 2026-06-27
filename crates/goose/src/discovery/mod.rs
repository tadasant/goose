//! AI Catalog / Server Card auto-discovery (SEP-2127, experimental).
//!
//! A background sidecar observes the domains Goose touches, probes them for an
//! `/.well-known/ai-catalog.json` and the Server Cards / Skills bundles it
//! references, and surfaces a non-intrusive install nudge — without ever blocking
//! the agent loop. The user can one-click install a discovered MCP server (running
//! auth if the card requires it) and it becomes active in the *same* session.
//!
//! See [`service::DiscoveryService`] for the entry point.

pub mod fixtures;
pub mod resolver;
pub mod service;
pub mod types;

pub use service::{
    discovery_to_extension, extension_name_for, server_card_to_extension, DiscoveryMode,
    DiscoveryService,
};
pub use types::{
    AiCatalog, AuthRequirement, Discovery, DiscoveryKind, RemoteTransport, ServerCard, SkillRef,
    SkillsBundle,
};

#[cfg(test)]
mod tests {
    use super::resolver::{CatalogResolver, FixtureResolver};
    use super::service::{
        extract_hosts, server_card_to_extension, DiscoveryMode, DiscoveryService,
    };
    use super::types::DiscoveryKind;
    use crate::agents::extension::ExtensionConfig;
    use std::sync::Arc;

    #[test]
    fn extracts_scheme_qualified_hosts() {
        let hosts =
            extract_hosts(r#"{"command":"curl -s https://modelcontextprotocol.io/llms.txt"}"#);
        assert!(hosts.contains(&"modelcontextprotocol.io".to_string()));
    }

    #[test]
    fn extracts_bare_fixture_domain() {
        let hosts = extract_hosts(r#"{"url":"modelcontextprotocol.io"}"#);
        assert!(hosts.contains(&"modelcontextprotocol.io".to_string()));
    }

    #[test]
    fn strips_www_and_port() {
        let hosts = extract_hosts("see https://www.github.com:443/foo");
        assert!(hosts.contains(&"github.com".to_string()));
    }

    #[tokio::test]
    async fn fixture_resolver_returns_mcp_server_and_skills() {
        let r = FixtureResolver;
        let found = r.resolve("modelcontextprotocol.io").await;
        assert_eq!(found.len(), 2, "expected one server + one skills bundle");
        assert!(found.iter().any(|d| d.is_server()));
        assert!(found
            .iter()
            .any(|d| matches!(d.kind, DiscoveryKind::Skills(_))));
        let server = found.iter().find(|d| d.is_server()).unwrap();
        let card = server.server_card().unwrap();
        assert_eq!(
            card.remote.as_ref().unwrap().url,
            "https://modelcontextprotocol.io/mcp"
        );
        assert!(!server.requires_auth(), "mcp docs server is auth-less");
    }

    #[tokio::test]
    async fn github_fixture_requires_auth() {
        let r = FixtureResolver;
        let found = r.resolve("github.com").await;
        let server = found.iter().find(|d| d.is_server()).unwrap();
        assert!(server.requires_auth());
        let card = server.server_card().unwrap();
        // Without a secret, conversion fails because auth is required.
        assert!(server_card_to_extension(card, None).is_err());
        // With a secret, the Authorization header is injected.
        let ext = server_card_to_extension(card, Some("ghp_demo")).unwrap();
        match ext {
            ExtensionConfig::StreamableHttp { uri, headers, .. } => {
                assert_eq!(uri, "https://api.githubcopilot.com/mcp/");
                assert_eq!(headers.get("Authorization").unwrap(), "Bearer ghp_demo");
            }
            _ => panic!("expected streamable_http extension"),
        }
    }

    #[tokio::test]
    async fn light_mode_ignores_non_fetch_tools() {
        let svc = DiscoveryService::new(DiscoveryMode::Light, Arc::new(FixtureResolver));
        // A shell tool is not fetch-like → ignored in light mode.
        svc.observe_tool_call(
            "developer__shell",
            r#"{"command":"curl https://modelcontextprotocol.io"}"#,
        );
        assert_eq!(svc.pending_count(), 0);
    }
}
