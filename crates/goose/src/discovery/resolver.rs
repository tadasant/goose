//! Catalog / Server Card resolution.
//!
//! A [`CatalogResolver`] turns a domain Goose touched into zero or more
//! [`Discovery`] items. The real implementation ([`HttpResolver`]) fetches
//! `/.well-known/ai-catalog.json` and the referenced server cards over HTTP. The
//! demo path ([`FixtureResolver`]) returns hand-authored fixtures. [`DefaultResolver`]
//! composes them: fixtures win (so the demo is deterministic), otherwise it falls
//! back to a live HTTP probe — which is exactly how this works in production once
//! domains publish real catalogs.

use std::net::IpAddr;
use std::time::Duration;

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use url::Url;

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

const MAX_BODY_BYTES: usize = 256 * 1024;
const MAX_CATALOG_ENTRIES: usize = 16;

/// Probes are driven by hosts seen in untrusted tool arguments and by URLs in
/// third-party catalogs, so only public, named hosts are ever contacted.
pub fn is_probeable_host(host: &str) -> bool {
    const PRIVATE_SUFFIXES: &[&str] = &[
        ".localhost",
        ".local",
        ".internal",
        ".lan",
        ".home.arpa",
        ".intranet",
        ".corp",
    ];
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    !host.is_empty()
        && host.contains('.')
        && host.parse::<IpAddr>().is_err()
        && !host.starts_with('[')
        && host != "localhost"
        && !PRIVATE_SUFFIXES.iter().any(|s| host.ends_with(s))
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                || a == 0
                || (a == 100 && (64..128).contains(&b)))
        }
        IpAddr::V6(v6) => {
            let first = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
                || v6
                    .to_ipv4_mapped()
                    .is_some_and(|v4| !is_public_ip(IpAddr::V4(v4))))
        }
    }
}

/// True if `url` is https on a probeable host that resolves only to public IPs.
async fn is_safe_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    if parsed.scheme() != "https" || !is_probeable_host(host) {
        return false;
    }
    match tokio::net::lookup_host((host, parsed.port_or_known_default().unwrap_or(443))).await {
        Ok(addrs) => {
            let addrs: Vec<_> = addrs.collect();
            !addrs.is_empty() && addrs.iter().all(|a| is_public_ip(a.ip()))
        }
        Err(_) => false,
    }
}

/// Fetches real `/.well-known/ai-catalog.json` documents and the server cards
/// they reference.
pub struct HttpResolver {
    client: reqwest::Client,
}

impl Default for HttpResolver {
    fn default() -> Self {
        let redirects = reqwest::redirect::Policy::custom(|attempt| {
            let ok = attempt.previous().len() < 3
                && attempt.url().scheme() == "https"
                && attempt.url().host_str().is_some_and(is_probeable_host);
            if ok {
                attempt.follow()
            } else {
                attempt.stop()
            }
        });
        let client = reqwest::Client::builder()
            .user_agent("goose-ai-catalog-discovery/0.1")
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(6))
            .redirect(redirects)
            .build()
            .expect("static reqwest client configuration is valid");
        HttpResolver { client }
    }
}

impl HttpResolver {
    async fn get_json<T: DeserializeOwned>(&self, url: &str, accept: &str) -> Option<T> {
        if !is_safe_url(url).await {
            return None;
        }
        let mut resp = self
            .client
            .get(url)
            .header(reqwest::header::ACCEPT, accept)
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let mut body = Vec::new();
        while let Some(chunk) = resp.chunk().await.ok()? {
            if body.len() + chunk.len() > MAX_BODY_BYTES {
                return None;
            }
            body.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&body).ok()
    }

    async fn fetch_catalog(&self, domain: &str) -> Option<AiCatalog> {
        let url = format!("https://{}/.well-known/ai-catalog.json", domain);
        self.get_json(&url, "application/ai-catalog+json, application/json")
            .await
    }

    async fn fetch_card(&self, url: &str) -> Option<ServerCard> {
        self.get_json(url, SERVER_CARD_MEDIA_TYPE).await
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
        for entry in catalog.entries.into_iter().take(MAX_CATALOG_ENTRIES) {
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
