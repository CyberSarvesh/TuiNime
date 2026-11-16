use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::traits::{Anime, MetadataProvider};

const ANILIST_API: &str = "https://graphql.anilist.co";

#[derive(Clone)]
pub struct AniListClient {
    client: reqwest::Client,
}

impl AniListClient {
    pub fn new() -> Self {
        Self {
            client: super::http_client("tuinime/0.1"),
        }
    }

    async fn query(&self, query: &str, variables: serde_json::Value) -> Result<serde_json::Value> {
        let resp = self
            .client
            .post(ANILIST_API)
            .json(&json!({ "query": query, "variables": variables }))
            .send()
            .await
            .context("AniList request")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("AniList http {status}: {body:.500}");
        }

        let v: serde_json::Value = resp.json().await.context("AniList json")?;
        if let Some(errors) = v.get("errors") {
            anyhow::bail!("AniList GraphQL errors: {errors}");
        }
        Ok(v)
    }

    fn map_media(m: &AnilistMedia) -> Anime {
        let title = m
            .title
            .english
            .clone()
            .or(m.title.romaji.clone())
            .or(m.title.native.clone())
            .unwrap_or_else(|| "Untitled".to_string());
        Anime {
            id: m.id.to_string(),
            title: title.clone(),
            description: m.description.clone().map(|d| strip_html(&d)),
            cover_image: m.cover_image.extra_large.clone().or(m.cover_image.large.clone()),
            banner_image: m.banner_image.clone(),
            score: m.average_score.map(|s| s.clamp(0, 100) as u8),
            year: m.season_year,
            episodes: m.episodes,
            status: m.status.clone(),
            genres: m.genres.clone().unwrap_or_default(),
            alt_titles: [&m.title.romaji, &m.title.english, &m.title.native]
                .into_iter()
                .flatten()
                .filter(|t| **t != title)
                .cloned()
                .collect(),
        }
    }
}

#[async_trait]
impl MetadataProvider for AniListClient {
    async fn trending(&self, limit: u8) -> Result<Vec<Anime>> {
        let q = r#"
            query ($perPage: Int) {
                Page(perPage: $perPage) {
                    media(sort: TRENDING_DESC, type: ANIME, isAdult: false) {
                        id
                        title { romaji english native }
                        description
                        coverImage { extraLarge large }
                        bannerImage
                        averageScore
                        seasonYear
                        episodes
                        status
                        genres
                    }
                }
            }
        "#;
        let v = self.query(q, json!({ "perPage": limit })).await?;
        let media: Vec<AnilistMedia> = serde_json::from_value(v["data"]["Page"]["media"].clone())
            .context("parse trending")?;
        Ok(media.iter().map(Self::map_media).collect())
    }

    async fn seasonal(&self, year: u16, season: &str, limit: u8) -> Result<Vec<Anime>> {
        let q = r#"
            query ($perPage: Int, $season: MediaSeason, $year: Int) {
                Page(perPage: $perPage) {
                    media(season: $season, seasonYear: $year, type: ANIME, sort: POPULARITY_DESC, isAdult: false) {
                        id
                        title { romaji english native }
                        description
                        coverImage { extraLarge large }
                        bannerImage
                        averageScore
                        seasonYear
                        episodes
                        status
                        genres
                    }
                }
            }
        "#;
        let v = self
            .query(q, json!({ "perPage": limit, "season": season.to_uppercase(), "year": year }))
            .await?;
        let media: Vec<AnilistMedia> = serde_json::from_value(v["data"]["Page"]["media"].clone())
            .context("parse seasonal")?;
        Ok(media.iter().map(Self::map_media).collect())
    }

    async fn search(&self, query: &str, limit: u8) -> Result<Vec<Anime>> {
        let q = r#"
            query ($search: String, $perPage: Int) {
                Page(perPage: $perPage) {
                    media(search: $search, type: ANIME, isAdult: false) {
                        id
                        title { romaji english native }
                        description
                        coverImage { extraLarge large }
                        bannerImage
                        averageScore
                        seasonYear
                        episodes
                        status
                        genres
                    }
                }
            }
        "#;
        let v = self
            .query(q, json!({ "search": query, "perPage": limit }))
            .await?;
        let media: Vec<AnilistMedia> = serde_json::from_value(v["data"]["Page"]["media"].clone())
            .context("parse search")?;
        Ok(media.iter().map(Self::map_media).collect())
    }

    async fn details(&self, id: &str) -> Result<Anime> {
        let q = r#"
            query ($id: Int) {
                Media(id: $id, type: ANIME) {
                    id
                    title { romaji english native }
                    description
                    coverImage { extraLarge large }
                    bannerImage
                    averageScore
                    seasonYear
                    episodes
                    status
                    genres
                }
            }
        "#;
        let id_int: i64 = id.parse().context("invalid AniList id")?;
        let v = self.query(q, json!({ "id": id_int })).await?;
        let m: AnilistMedia =
            serde_json::from_value(v["data"]["Media"].clone()).context("parse details")?;
        Ok(Self::map_media(&m))
    }
}

impl Default for AniListClient {
    fn default() -> Self {
        Self::new()
    }
}

// --- AniList response shapes ---

#[derive(Debug, Deserialize)]
struct AnilistMedia {
    id: i64,
    title: AnilistTitle,
    description: Option<String>,
    #[serde(rename = "coverImage")]
    cover_image: AnilistCover,
    #[serde(rename = "bannerImage")]
    banner_image: Option<String>,
    #[serde(rename = "averageScore")]
    average_score: Option<i32>,
    #[serde(rename = "seasonYear")]
    season_year: Option<u16>,
    episodes: Option<u32>,
    status: Option<String>,
    genres: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct AnilistTitle {
    romaji: Option<String>,
    english: Option<String>,
    native: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnilistCover {
    #[serde(rename = "extraLarge")]
    extra_large: Option<String>,
    large: Option<String>,
}

fn strip_html(s: &str) -> String {
    // Cheap tag strip. `<br>` tags become newlines (they must be handled while stripping:
    // once tags are removed there is nothing left to replace).
    let mut out = String::with_capacity(s.len());
    let mut tag: Option<String> = None;
    for c in s.chars() {
        match (&mut tag, c) {
            (None, '<') => tag = Some(String::new()),
            (Some(t), '>') => {
                let name = t.trim_start_matches('/').trim().to_ascii_lowercase();
                if name == "br" || name.starts_with("br ") || name.starts_with("br/") {
                    out.push('\n');
                }
                tag = None;
            }
            (Some(t), c) => t.push(c),
            (None, c) => out.push(c),
        }
    }
    // Decode a few common entities (`&amp;` last so `&amp;lt;` stays literal)
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::strip_html;

    #[test]
    fn br_tags_become_newlines() {
        assert_eq!(strip_html("a<br>b<br/>c<BR />d"), "a\nb\nc\nd");
    }

    #[test]
    fn strips_tags_and_decodes_entities() {
        assert_eq!(strip_html("<i>Tom &amp; Jerry</i> &quot;hi&quot;"), "Tom & Jerry \"hi\"");
    }
}
