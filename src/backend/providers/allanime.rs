use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::json;

use super::traits::{Anime, AnimeProvider, Episode, StreamSource};

/// AllAnime provider — mirrors how `pystardust/ani-cli` pulls episodes.
///
/// ani-cli flow (see `ani-cli` + `hacking.md`):
///   1. `search_anime()`  — POST GraphQL `shows(search: {query, allowAdult:false})` to `https://api.allanime.day/api`
///      → parses `edges { _id, name, availableEpisodes }`
///   2. `episodes_list()` — POST `show(_id)` → `availableEpisodesDetail { sub, dub, raw }`
///      → newline-separated episode numbers, user picks one
///   3. `get_episode_url()` — POST `episode(showId, translationType, episodeString)` → `sourceUrls`
///      each `sourceUrl` is a hex obfuscated `--<hex>` string that needs decrypting:
///        `01→9 08→0 05→= 0a→2 0b→3 0c→4 07→? 00→8 5c→d 0f→7 5e→f 17→/ 54→l 09→1 48→p 4f→w 0e→6 5b→c 5d→e 0d→5 53→k 1e→& 5a→b 59→a 4a→r 4c→t 4e→v 57→o 51→i`
///      then `/clock` → `/clock.json`, fetch `https://allanime.day{path}` and `get_links()` extracts
///      `link` + `resolutionStr` or `hls` url. Links are `quality > url`, sorted, then selected.
///
/// This file implements 1 + 2 fully, and 3 partially (decrypt + provider dispatch). The final
/// `get_links` HTTP fetch for m3u8/mp4 is wired but falls back gracefully when allanime is
/// unreachable — aggregator will try next provider (gogoanime via Consumet).
const ALLANIME_API: &str = "https://api.allanime.day/api";
const ALLANIME_REFR: &str = "https://allanime.to";
const ALLANIME_BASE: &str = "allanime.day";

#[derive(Clone)]
pub struct AllAnimeProvider {
    client: reqwest::Client,
    mode: String, // "sub" or "dub"
}

impl AllAnimeProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("tuinime/0.1")
                .build().expect("reqwest"),
            mode: "sub".to_string(),
        }
    }
    pub fn with_mode(mode: impl Into<String>) -> Self {
        Self { client: reqwest::Client::builder().user_agent("tuinime/0.1").build().expect("reqwest"), mode: mode.into() }
    }

    fn decrypt_hex(hex: &str) -> String {
        // ani-cli: `sed 's/../&\n/g' | sed 's/^01$/9/g; …' | tr -d '\n'`
        let table: std::collections::HashMap<&str, &str> = [
            ("01","9"),("08","0"),("05","="),("0a","2"),("0b","3"),("0c","4"),("07","?"),("00","8"),
            ("5c","d"),("0f","7"),("5e","f"),("17","/"),("54","l"),("09","1"),("48","p"),("4f","w"),
            ("0e","6"),("5b","c"),("5d","e"),("0d","5"),("53","k"),("1e","&"),("5a","b"),("59","a"),
            ("4a","r"),("4c","t"),("4e","v"),("57","o"),("51","i"),
        ].into_iter().collect();
        let mut out = String::new();
        let clean = hex.trim().trim_start_matches("--");
        for i in (0..clean.len()).step_by(2) {
            if i + 2 > clean.len() { break; }
            let pair = &clean[i..i+2].to_lowercase();
            if let Some(mapped) = table.get(pair.as_str()) {
                out.push_str(mapped);
            } else {
                // fallback: try parse as hex char
                if let Ok(b) = u8::from_str_radix(pair, 16) {
                    out.push(b as char);
                }
            }
        }
        out.replace("/clock", "/clock.json")
    }
}

#[async_trait]
impl AnimeProvider for AllAnimeProvider {
    async fn search(&self, query: &str, limit: u8) -> Result<Vec<Anime>> {
        let search_gql = r#"query($search: SearchInput $limit: Int $page: Int $translationType: VaildTranslationTypeEnumType $countryOrigin: VaildCountryOriginEnumType){shows(search: $search limit: $limit page: $page translationType: $translationType countryOrigin: $countryOrigin){edges{_id name availableEpisodes __typename}}}"#;
        let vars = json!({
            "search": {"allowAdult": false, "allowUnknown": false, "query": query},
            "limit": limit as i32,
            "page": 1,
            "translationType": self.mode,
            "countryOrigin": "ALL"
        });
        let resp = self.client.post(ALLANIME_API)
            .header("Referer", ALLANIME_REFR)
            .header("Origin", ALLANIME_REFR)
            .json(&json!({"query": search_gql, "variables": vars}))
            .send().await.context("allanime search")?;

        if !resp.status().is_success() {
            anyhow::bail!("allanime search http {}", resp.status());
        }
        let v: serde_json::Value = resp.json().await.context("allanime search json")?;
        let edges = v.pointer("/data/shows/edges").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        let mut out = Vec::new();
        for e in edges {
            let id = e.get("_id").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if id.is_empty() { continue; }
            let name = e.get("name").and_then(|x| x.as_str()).unwrap_or("Untitled").to_string();
            let avail = e.get("availableEpisodes").and_then(|x| x.get(&self.mode)).and_then(|x| x.as_u64()).unwrap_or(0) as u32;
            out.push(Anime {
                id,
                title: name,
                description: None,
                cover_image: None, // allanime search doesn't return cover; aggregator merges with AniList later
                banner_image: None,
                score: None,
                year: None,
                episodes: Some(avail),
                status: None,
                genres: vec![],
            });
        }
        Ok(out)
    }

    async fn episodes(&self, anime_id: &str) -> Result<Vec<Episode>> {
        let gql = r#"query($showId: String!){show(_id: $showId){_id availableEpisodesDetail}}"#;
        let resp = self.client.post(ALLANIME_API)
            .header("Referer", ALLANIME_REFR)
            .json(&json!({"query": gql, "variables": {"showId": anime_id}}))
            .send().await.context("allanime episodes")?;
        if !resp.status().is_success() {
            anyhow::bail!("allanime episodes http {}", resp.status());
        }
        let v: serde_json::Value = resp.json().await?;
        // availableEpisodesDetail: { sub: ["1","2",…], dub: […] }
        let arr = v.pointer(&format!("/data/show/availableEpisodesDetail/{}", self.mode))
            .and_then(|x| x.as_array()).cloned().unwrap_or_default();
        let mut eps = Vec::new();
        for val in arr {
            let num_str = val.as_str().unwrap_or("").to_string();
            if let Ok(num) = num_str.parse::<f32>() {
                let n = num as u32;
                eps.push(Episode {
                    id: format!("{}:{}", anime_id, num_str),
                    number: n,
                    title: None,
                    thumbnail: None,
                });
            }
        }
        eps.sort_by_key(|e| e.number);
        Ok(eps)
    }

    async fn streams(&self, episode_id: &str) -> Result<Vec<StreamSource>> {
        // episode_id is "showId:episodeString" (e.g. "abc123:12")
        let parts: Vec<&str> = episode_id.splitn(2, ':').collect();
        if parts.len() != 2 {
            anyhow::bail!("invalid allanime episode_id, expected showId:episodeString");
        }
        let (show_id, ep_str) = (parts[0], parts[1]);

        // Step 1: get sourceUrls (ani-cli episode_embed_gql)
        let gql = r#"query($showId: String! $translationType: VaildTranslationTypeEnumType! $episodeString: String!){episode(showId: $showId translationType: $translationType episodeString: $episodeString){episodeString sourceUrls}}"#;
        let resp = self.client.post(ALLANIME_API)
            .header("Referer", ALLANIME_REFR)
            .json(&json!({"query": gql, "variables": {"showId": show_id, "translationType": self.mode, "episodeString": ep_str}}))
            .send().await.context("allanime episode")?;

        let v: serde_json::Value = resp.json().await?;
        let source_urls = v.pointer("/data/episode/sourceUrls")
            .and_then(|x| x.as_array()).cloned().unwrap_or_default();

        if source_urls.is_empty() {
            anyhow::bail!("no sourceUrls for {episode_id}");
        }

        // Step 2: decrypt each --hex to /clock.json path and fetch get_links
        let mut sources = Vec::new();
        for entry in source_urls {
            let url_hex = entry.get("sourceUrl").and_then(|x| x.as_str()).unwrap_or("");
            if !url_hex.starts_with("--") { continue; }
            let decrypted = Self::decrypt_hex(url_hex);
            // decrypted like "/apiv2/clock.json?id=…"
            let embed_url = format!("https://{ALLANIME_BASE}{decrypted}");
            match self.get_links(&embed_url).await {
                Ok(mut links) => sources.append(&mut links),
                Err(e) => { tracing::debug!("allanime get_links failed for {embed_url}: {e:#}"); continue; }
            }
        }

        if sources.is_empty() {
            anyhow::bail!("no playable links for {episode_id}");
        }
        // sort by quality desc if present
        sources.sort_by(|a,b| b.quality.cmp(&a.quality));
        Ok(sources)
    }
}

/// Extract `link`/`resolutionStr` and `hls` from embed JSON — mirrors ani-cli `get_links()`
impl AllAnimeProvider {
    async fn get_links(&self, embed_url: &str) -> Result<Vec<StreamSource>> {
        let resp = self.client.get(embed_url)
            .header("Referer", ALLANIME_REFR)
            .header("Origin", ALLANIME_REFR)
            .send().await.context("fetch embed")?;
        if !resp.status().is_success() {
            anyhow::bail!("embed http {}", resp.status());
        }
        let text = resp.text().await?;
        // ani-cli: sed 's|},{|\n|g' | sed -nE 's|.*link":"([^"]*)".*"resolutionStr":"([^"]*)".*|\2 >\1|p; s|.*hls","url":"([^"]*)".*"hardsub_lang":"en-US".*|\1|p'
        // We do it in Rust: look for JSON-ish fragments
        let mut out = Vec::new();
        for chunk in text.split("},{") {
            // try link + resolutionStr
            if let Some(link) = extract_json_field(chunk, "link") {
                let res = extract_json_field(chunk, "resolutionStr").unwrap_or_else(|| "0".to_string());
                let url = link.replace("\\u002F", "/").replace('\\', "");
                out.push(StreamSource { url: url.clone(), quality: Some(res), headers: None, is_m3u8: url.contains(".m3u8") });
            } else if chunk.contains("\"hls\"") {
                if let Some(url) = extract_json_field(chunk, "url") {
                    if url.contains("m3u8") {
                        let url = url.replace("\\u002F", "/");
                        // expand m3u8 master playlist to individual qualities like ani-cli does
                        match self.expand_m3u8(&url).await {
                            Ok(mut v) => out.append(&mut v),
                            Err(_) => out.push(StreamSource { url: url.clone(), quality: None, headers: None, is_m3u8: true }),
                        }
                    }
                }
            }
        }
        if out.is_empty() && text.contains("http") {
            // fallback: find any http url in response
            for token in text.split('"') {
                if token.starts_with("http") && (token.contains(".m3u8") || token.contains(".mp4")) {
                    out.push(StreamSource { url: token.to_string(), quality: None, headers: None, is_m3u8: token.contains(".m3u8") });
                }
            }
        }
        Ok(out)
    }

    async fn expand_m3u8(&self, url: &str) -> Result<Vec<StreamSource>> {
        let resp = self.client.get(url).header("Referer", ALLANIME_REFR).send().await?;
        let text = resp.text().await?;
        let base = url.rsplitn(2, '/').collect::<Vec<_>>().get(1).map(|s| format!("{}/", s)).unwrap_or_default();
        let mut out = Vec::new();
        // ani-cli: sed 's|^#.*x||g; s|,.*|p|g; /^#/d; $!N; s|\n| >|'  → lines like "1080p >https://…/1080.m3u8"
        let mut lines = text.lines().peekable();
        while let Some(line) = lines.next() {
            if line.starts_with("#") && line.contains("x") {
                if let Some(next) = lines.next() {
                    let res = line.split('x').last().unwrap_or("0").split(',').next().unwrap_or("0");
                    let full = if next.starts_with("http") { next.to_string() } else { format!("{base}{next}") };
                    out.push(StreamSource { url: full, quality: Some(format!("{}p", res.trim())), headers: None, is_m3u8: true });
                }
            }
        }
        Ok(out)
    }
}

fn extract_json_field(s: &str, field: &str) -> Option<String> {
    let pat = format!("\"{}\":\"", field);
    let start = s.find(&pat)? + pat.len();
    let rest = &s[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

impl Default for AllAnimeProvider {
    fn default() -> Self { Self::new() }
}
