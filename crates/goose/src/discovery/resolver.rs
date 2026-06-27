//! Catalog / Server Card resolution.
//!
//! A [`CatalogResolver`] turns a domain Goose touched into zero or more
//! [`Discovery`] items. The real implementation ([`HttpResolver`]) fetches
//! `/.well-known/ai-catalog.json` and the referenced server cards over HTTP. The
//! demo path ([`FixtureResolver`]) returns hand-authored fixtures. [`DefaultResolver`]
//! composes them: fixtures win (so the demo is deterministic), otherwise it falls
//! back to a live HTTP probe — which is exactly how this works in production once
//! domains publish real catalogs.

use std::time::Duration;

use async_trait::async_trait;

use super::fixtures;
use super::types::{AiCatalog, Discovery, ServerCard};

/// Resolves a domain into discoveries.
#[async_trait]
pub trait CatalogResolver: Send + Sync {
    /// Probe `domain` (a bare host, already normalized) for an AI Catalog and the
    /// server cards / skills bundles it references. Returns an empty vec when the
    /// domain advertises nothing (the caller negative-caches that).
    async fn resolve(&self, domain: &str) -> Vec<Discovery>;
}

/// Returns hand-authored demo fixtures for known domains.
#[derive(Default)]
pub struct FixtureResolver;

#[async_trait]
impl CatalogResolver for FixtureResolver {
    async fn resolve(&self, domain: &str) -> Vec<Discovery> {
        fixtures::fixtures_for(domain)
    }
}

/// Fetches real `/.well-known/ai-catalog.json` documents and the server cards
/// they reference.
pub struct HttpResolver {
    client: reqwest::Client,
}

impl Default for HttpResolver {
    fn default() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("goose-ai-catalog-discovery/0.1")
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(6))
            .build()
            .unwrap_or_default();
        HttpResolver { client }
    }
}

impl HttpResolver {
    async fn fetch_catalog(&self, domain: &str) -> Option<AiCatalog> {
        let url = format!("https://{}/.well-known/ai-catalog.json", domain);
        let resp = self.client.get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        resp.json::<AiCatalog>().await.ok()
    }

    async fn fetch_card(&self, url: &str) -> Option<ServerCard> {
        let resp = self.client.get(url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        // Accept either the dedicated card content-type or plain json.
        resp.json::<ServerCard>().await.ok()
    }
}

#[async_trait]
impl CatalogResolver for HttpResolver {
    async fn resolve(&self, domain: &str) -> Vec<Discovery> {
        let Some(catalog) = self.fetch_catalog(domain).await else {
            return vec![];
        };
        let mut out = Vec::new();
        for server_ref in catalog.servers {
            if let Some(card) = server_ref.card {
                out.push(Discovery::new_server(domain, card, false));
            } else if let Some(card_url) = server_ref.server_card {
                if let Some(card) = self.fetch_card(&card_url).await {
                    out.push(Discovery::new_server(domain, card, false));
                }
            }
        }
        for bundle in catalog.skills {
            out.push(Discovery::new_skills(domain, bundle, false));
        }
        out
    }
}

/// Production resolver: try fixtures first (deterministic demo content), then a
/// live HTTP probe. `enable_http` lets tests / the demo disable real network egress.
pub struct DefaultResolver {
    fixture: FixtureResolver,
    http: Option<HttpResolver>,
}

impl DefaultResolver {
    pub fn new(enable_http: bool) -> Self {
        DefaultResolver {
            fixture: FixtureResolver,
            http: enable_http.then(HttpResolver::default),
        }
    }
}

#[async_trait]
impl CatalogResolver for DefaultResolver {
    async fn resolve(&self, domain: &str) -> Vec<Discovery> {
        let fixtures = self.fixture.resolve(domain).await;
        if !fixtures.is_empty() {
            return fixtures;
        }
        match &self.http {
            Some(http) => http.resolve(domain).await,
            None => vec![],
        }
    }
}
