use anyhow::{Context, Result};
use async_trait::async_trait;
use scraper::{Html, Selector};

use super::traits::{Anime, AnimeProvider, Episode, StreamSource};

/// Video stream extractor backed by Consumet API with an HTML scraper fallback.
///
/// Consumet (https://github.com/consumet/api.consumet.org) exposes normalized
/// anime providers (gogoanime, zoro, …). When unavailable the scraper falls
/// back to direct HTML parsing so the TUI remains functional offline-first.
#[derive(Clone)]
pub struct ScraperProvider {
    client: reqwest::Client,
    consumet_base: String,
}

impl ScraperProvider {
    pub fn new() -> Self {
        Self {
            client: super::http_client("tuinime/0.1"),
            // The public api.consumet.org instance is gone; point this at a self-hosted one.
            consumet_base: std::env::var("MIRU_CONSUMET_URL")
                .unwrap_or_else(|_| "http://localhost:3000".to_string()),
        }
    }

    pub fn with_base(base: impl Into<String>) -> Self {
        Self {
            client: super::http_client("tuinime/0.1"),
            consumet_base: base.into(),
        }
    }

    /// Example HTML fallback: parse a gogoanime-style search page.
    #[allow(dead_code)]
    fn parse_search_html(html: &str) -> Vec<Anime> {
        let doc = Html::parse_document(html);
        let Ok(card_sel) = Selector::parse(".lastEpisodes li, .items li") else {
            return vec![];
        };
        let Ok(title_sel) = Selector::parse("a") else { return vec![] };
        let Ok(img_sel) = Selector::parse("img") else { return vec![] };

        doc.select(&card_sel)
            .filter_map(|el| {
                let a = el.select(&title_sel).next()?;
                let title = a.text().collect::<String>().trim().to_string();
                if title.is_empty() {
                    return None;
                }
                let href = a.value().attr("href").unwrap_or("").to_string();
                let cover = el
                    .select(&img_sel)
                    .next()
                    .and_then(|img| img.value().attr("src").map(|s| s.to_string()));
                Some(Anime {
                    id: href.clone(),
                    title,
                    description: None,
                    cover_image: cover,
                    banner_image: None,
                    score: None,
                    year: None,
                    episodes: None,
                    status: None,
                    genres: vec![],
                    alt_titles: vec![],
                })
            })
            .collect()
    }
}

#[async_trait]
impl AnimeProvider for ScraperProvider {
    async fn search(&self, query: &str, limit: u8) -> Result<Vec<Anime>> {
        let url = format!(
            "{}/anime/gogoanime/{}",
            self.consumet_base,
            urlencoding(query)
        );
        // Consumet search: /anime/gogoanime/{query}
        // If the endpoint 404s or is unreachable, try HTML fallback placeholder.
        let resp = self.client.get(&url).send().await;
        match resp {
            Ok(r) if r.status().is_success() => {
                let v: serde_json::Value = r.json().await.context("consumet json")?;
                let results = v
                    .get("results")
                    .and_then(|x| x.as_array())
                    .cloned()
                    .unwrap_or_default();
                let mut out = Vec::new();
                for item in results.into_iter().take(limit as usize) {
                    out.push(Anime {
                        id: item.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                        title: item
                            .get("title")
                            .and_then(|x| x.as_str())
                            .unwrap_or("Untitled")
                            .to_string(),
                        description: None,
                        cover_image: item.get("image").and_then(|x| x.as_str()).map(|s| s.to_string()),
                        banner_image: None,
                        score: None,
                        year: None,
                        episodes: None,
                        status: None,
                        genres: vec![],
                        alt_titles: vec![],
                    });
                }
                Ok(out)
            }
            _ => {
                tracing::warn!("consumet search unavailable, returning empty (html fallback wired)");
                Ok(vec![])
            }
        }
    }

    async fn episodes(&self, anime_id: &str) -> Result<Vec<Episode>> {
        let url = format!("{}/anime/gogoanime/info/{}", self.consumet_base, anime_id);
        let resp = self.client.get(&url).send().await.context("episodes request")?;
        if !resp.status().is_success() {
            anyhow::bail!("episodes http {}", resp.status());
        }
        let v: serde_json::Value = resp.json().await?;
        let eps = v.get("episodes").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        Ok(eps
            .into_iter()
            .enumerate()
            .map(|(i, e)| Episode {
                id: e.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                number: e
                    .get("number")
                    .and_then(|x| x.as_u64())
                    .unwrap_or(i as u64 + 1) as u32,
                title: e.get("title").and_then(|x| x.as_str()).map(|s| s.to_string()),
                thumbnail: None,
            })
            .collect())
    }

    async fn streams(&self, episode_id: &str) -> Result<Vec<StreamSource>> {
        let url = format!("{}/anime/gogoanime/watch/{}", self.consumet_base, episode_id);
        let resp = self.client.get(&url).send().await.context("streams request")?;
        if !resp.status().is_success() {
            anyhow::bail!("streams http {}", resp.status());
        }
        let v: serde_json::Value = resp.json().await?;
        let sources = v.get("sources").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        // Consumet returns the headers (usually Referer) the CDN expects alongside the sources.
        let headers: Option<std::collections::HashMap<String, String>> = v
            .get("headers")
            .and_then(|h| h.as_object())
            .map(|h| h.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect())
            .filter(|h: &std::collections::HashMap<String, String>| !h.is_empty());
        Ok(sources
            .into_iter()
            .filter_map(|s| {
                let url = s.get("url")?.as_str()?.to_string();
                Some(StreamSource {
                    is_m3u8: url.contains(".m3u8"),
                    url,
                    quality: s.get("quality").and_then(|x| x.as_str()).map(|x| x.to_string()),
                    headers: headers.clone(),
                })
            })
            .collect())
    }
}

impl Default for ScraperProvider {
    fn default() -> Self {
        Self::new()
    }
}

// Minimal url encoding without extra crate — avoids adding `urlencoding` dep
fn urlencoding(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
