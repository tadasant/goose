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
use super::types::{
    AiCatalog, CatalogEntry, Discovery, ServerCard, SkillsBundle, AGENT_SKILLS_MEDIA_TYPE,
    SERVER_CARD_MEDIA_TYPE,
};

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
        let resp = self
            .client
            .get(url)
            .header(reqwest::header::ACCEPT, SERVER_CARD_MEDIA_TYPE)
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        resp.json::<ServerCard>().await.ok()
    }

    async fn resolve_entry(&self, domain: &str, entry: CatalogEntry) -> Option<Discovery> {
        match entry.media_type.as_str() {
            SERVER_CARD_MEDIA_TYPE => {
                let mut card = match (&entry.data, &entry.url) {
                    (Some(data), _) => serde_json::from_value::<ServerCard>(data.clone()).ok()?,
                    (None, Some(url)) => self.fetch_card(url).await?,
                    (None, None) => return None,
                };
                // Entry-level presentation fields take precedence over the card's.
                if entry.display_name.is_some() {
                    card.title = entry.display_name;
                }
                if let Some(description) = entry.description {
                    card.description = description;
                }
                Some(Discovery::new_server(domain, card, false))
            }
            AGENT_SKILLS_MEDIA_TYPE => {
                let bundle = SkillsBundle {
                    name: entry.display_name.unwrap_or(entry.identifier),
                    description: entry.description.unwrap_or_default(),
                    source: entry.url,
                    skills: Vec::new(),
                };
                Some(Discovery::new_skills(domain, bundle, false))
            }
            _ => None,
        }
    }
}

#[async_trait]
impl CatalogResolver for HttpResolver {
    async fn resolve(&self, domain: &str) -> Vec<Discovery> {
        let Some(catalog) = self.fetch_catalog(domain).await else {
            return vec![];
        };
        let mut out = Vec::new();
        for entry in catalog.entries {
            if let Some(discovery) = self.resolve_entry(domain, entry).await {
                out.push(discovery);
            }
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
