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
    AiCatalog, AuthRequirement, CatalogEntry, Discovery, DiscoveryKind, RemoteTransport,
    ServerCard, SkillRef, SkillsBundle,
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
            card.primary_remote().unwrap().url,
            "https://modelcontextprotocol.io/mcp"
        );
        assert!(!server.requires_auth(), "mcp docs server is auth-less");
    }

    #[tokio::test]
    async fn github_fixture_requires_auth() {
        let r = FixtureResolver;
        let found = r.resolve("api.github.com").await;
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

    async fn settle(svc: &DiscoveryService) {
        for _ in 0..50 {
            if svc.probes_in_flight() == 0 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn light_mode_ignores_non_fetch_tools() {
        let svc = DiscoveryService::new(DiscoveryMode::Light, Arc::new(FixtureResolver));
        svc.observe_tool_call(
            "developer__shell",
            r#"{"command":"curl https://modelcontextprotocol.io"}"#,
        );
        settle(&svc).await;
        assert_eq!(svc.pending_count(), 0);

        svc.observe_tool_call(
            "fetch__fetch",
            r#"{"url":"https://modelcontextprotocol.io"}"#,
        );
        settle(&svc).await;
        assert_eq!(
            svc.pending_count(),
            2,
            "fetch-like tool is probed in light mode"
        );
    }

    #[tokio::test]
    async fn dismissed_discoveries_do_not_resurface() {
        let svc = DiscoveryService::new(DiscoveryMode::Full, Arc::new(FixtureResolver));
        svc.observe_domain("modelcontextprotocol.io");
        settle(&svc).await;
        let first = svc.pending()[0].id.clone();
        svc.mark_dismissed(&first);
        svc.observe_domain("modelcontextprotocol.io");
        settle(&svc).await;
        assert!(svc.pending().iter().all(|d| d.id != first));
    }

    #[test]
    fn refuses_private_and_literal_hosts() {
        use super::resolver::is_probeable_host;
        for bad in [
            "localhost",
            "169.254.169.254",
            "10.0.0.5",
            "metadata.google.internal",
            "printer.local",
            "intranet",
        ] {
            assert!(!is_probeable_host(bad), "{bad} must not be probed");
        }
        assert!(is_probeable_host("github.com"));
        assert!(is_probeable_host("huggingface.co"));
    }

    #[test]
    fn bare_domain_hint_respects_boundaries() {
        assert!(extract_hosts("see notmodelcontextprotocol.io").is_empty());
        assert!(!extract_hosts("see modelcontextprotocol.io now").is_empty());
    }

    // Verbatim from https://github.com/.well-known/ai-catalog.json (2026-10-19).
    const GITHUB_CATALOG: &str = r#"{"specVersion":"1.0","entries":[{"identifier":"urn:air:github.com:mcp:github-mcp-server","type":"application/mcp-server-card+json","url":"https://api.githubcopilot.com/mcp/server-card"}]}"#;

    // Verbatim from https://api.githubcopilot.com/mcp/server-card (2026-10-19).
    const GITHUB_CARD: &str = r#"{"$schema":"https://static.modelcontextprotocol.io/schemas/v1/server-card.schema.json","name":"io.github.github/github-mcp-server","version":"github-mcp-server/remote-c91af86fab613fa50f4f34159186e6a61e89c381","description":"Connect AI assistants to GitHub - manage repos, issues, PRs, and workflows through natural language.","title":"GitHub","websiteUrl":"https://github.com/github/github-mcp-server","repository":{"url":"https://github.com/github/github-mcp-server","source":"github","id":"942771284"},"remotes":[{"type":"streamable-http","url":"https://api.githubcopilot.com/mcp/"}]}"#;

    #[test]
    fn parses_live_github_catalog_and_card() {
        let catalog: super::AiCatalog = serde_json::from_str(GITHUB_CATALOG).unwrap();
        assert_eq!(catalog.spec_version.as_deref(), Some("1.0"));
        assert_eq!(catalog.entries.len(), 1);
        assert_eq!(
            catalog.entries[0].media_type,
            super::types::SERVER_CARD_MEDIA_TYPE
        );

        let card: super::ServerCard = serde_json::from_str(GITHUB_CARD).unwrap();
        assert_eq!(card.title.as_deref(), Some("GitHub"));
        assert_eq!(
            card.primary_remote().unwrap().url,
            "https://api.githubcopilot.com/mcp/"
        );
        assert!(card.primary_remote().unwrap().auth_requirement().is_none());
        match server_card_to_extension(&card, None).unwrap() {
            ExtensionConfig::StreamableHttp { name, uri, .. } => {
                assert_eq!(name, "github-mcp-server");
                assert_eq!(uri, "https://api.githubcopilot.com/mcp/");
            }
            _ => panic!("expected streamable_http extension"),
        }
    }

    #[test]
    fn secret_header_input_requires_auth() {
        let card: super::ServerCard = serde_json::from_str(
            r#"{"name":"com.example/api","version":"1.0.0","description":"x","remotes":[{"type":"streamable-http","url":"https://mcp.example.com/mcp","headers":[{"name":"X-Api-Key","isRequired":true,"isSecret":true,"description":"Your API key"},{"name":"X-Client","value":"goose"}]}]}"#,
        )
        .unwrap();
        let auth = card.primary_remote().unwrap().auth_requirement().unwrap();
        assert_eq!(auth.header.as_deref(), Some("X-Api-Key"));
        assert!(server_card_to_extension(&card, None).is_err());
        match server_card_to_extension(&card, Some("k")).unwrap() {
            ExtensionConfig::StreamableHttp { headers, .. } => {
                assert_eq!(headers.get("X-Api-Key").unwrap(), "k");
                assert_eq!(headers.get("X-Client").unwrap(), "goose");
            }
            _ => panic!("expected streamable_http extension"),
        }
    }

    #[test]
    fn generic_card_names_fall_back_to_publisher() {
        let card: super::ServerCard = serde_json::from_str(
            r#"{"name":"huggingface.co/mcp","version":"0.4.31","description":"x","remotes":[{"type":"streamable-http","url":"https://huggingface.co/mcp"}]}"#,
        )
        .unwrap();
        assert_eq!(super::extension_name_for(&card), "huggingface");
    }
}
