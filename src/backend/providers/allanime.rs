use std::sync::atomic::{AtomicBool, Ordering};

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
///      each byte XOR 0x38 (ani-cli spells this out as a sed table: `01→9 08→0 05→= …`),
///      then `/clock` → `/clock.json`, fetch `https://allanime.day{path}` and `get_links()` extracts
///      `link` + `resolutionStr` or `hls` url. Links are `quality > url`, sorted, then selected.
///
/// This file implements 1 + 2 fully, and 3 partially (decrypt + provider dispatch). The final
/// `get_links` HTTP fetch for m3u8/mp4 is wired but falls back gracefully when allanime is
/// unreachable — aggregator will try next provider (gogoanime via Consumet).
const ALLANIME_API: &str = "https://api.allanime.day/api";
const ALLANIME_REFR: &str = "https://allanime.to";
const ALLANIME_BASE: &str = "allanime.day";
const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:128.0) Gecko/20100101 Firefox/128.0";

pub struct AllAnimeProvider {
    client: reqwest::Client,
    /// `false` = subtitled listings, `true` = dubbed.
    dub: AtomicBool,
}

impl AllAnimeProvider {
    pub fn new() -> Self {
        Self {
            client: super::http_client(BROWSER_UA),
            dub: AtomicBool::new(false),
        }
    }
    /// AllAnime's `translationType`: "sub" or "dub".
    fn mode(&self) -> &'static str {
        if self.dub.load(Ordering::Relaxed) { "dub" } else { "sub" }
    }

    /// Decode an AllAnime `--<hex>` source URL.
    ///
    /// ani-cli's long sed table is just every byte XOR `0x38` (e.g. `01` → `'9'` = 0x39).
    /// Doing the XOR directly handles every character, not only the ~30 the table listed.
    fn decrypt_hex(hex: &str) -> String {
        let clean = hex.trim().trim_start_matches("--");
        if !clean.is_ascii() {
            return String::new();
        }
        let bytes: Vec<u8> = clean
            .as_bytes()
            .chunks_exact(2)
            .filter_map(|pair| std::str::from_utf8(pair).ok())
            .filter_map(|pair| u8::from_str_radix(pair, 16).ok())
            .map(|b| b ^ 0x38)
            .collect();
        let out = String::from_utf8_lossy(&bytes).into_owned();
        if out.contains("/clock.json") { out } else { out.replace("/clock", "/clock.json") }
    }

    fn referer_headers() -> Option<std::collections::HashMap<String, String>> {
        Some([("Referer".to_string(), ALLANIME_REFR.to_string())].into_iter().collect())
    }
}

/// Numeric rank for a quality label like "1080", "720p" or "1080p (HLS)"; unknown → 0.
pub(crate) fn quality_rank(q: Option<&str>) -> u32 {
    q.map(|q| q.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect::<String>())
        .and_then(|d| d.parse().ok())
        .unwrap_or(0)
}

#[async_trait]
impl AnimeProvider for AllAnimeProvider {
    fn name(&self) -> &'static str {
        "AllAnime"
    }

    async fn search(&self, query: &str, limit: u8) -> Result<Vec<Anime>> {
        let search_gql = r#"query($search: SearchInput $limit: Int $page: Int $translationType: VaildTranslationTypeEnumType $countryOrigin: VaildCountryOriginEnumType){shows(search: $search limit: $limit page: $page translationType: $translationType countryOrigin: $countryOrigin){edges{_id name availableEpisodes __typename}}}"#;
        let vars = json!({
            "search": {"allowAdult": false, "allowUnknown": false, "query": query},
            "limit": limit as i32,
            "page": 1,
            "translationType": self.mode(),
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
            let avail = e.get("availableEpisodes").and_then(|x| x.get(self.mode())).and_then(|x| x.as_u64()).unwrap_or(0) as u32;
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
                alt_titles: vec![],
            });
        }
        Ok(out)
    }

    async fn episodes(&self, anime_id: &str) -> Result<Vec<Episode>> {
        let gql = r#"query($showId: String!){show(_id: $showId){_id availableEpisodesDetail}}"#;
        let resp = self.client.post(ALLANIME_API)
            .header("Referer", ALLANIME_REFR)
            .header("Origin", ALLANIME_REFR)
            .json(&json!({"query": gql, "variables": {"showId": anime_id}}))
            .send().await.context("allanime episodes")?;
        if !resp.status().is_success() {
            anyhow::bail!("allanime episodes http {}", resp.status());
        }
        let v: serde_json::Value = resp.json().await?;
        // availableEpisodesDetail: { sub: ["1","2",…], dub: […] }
        let arr = v.pointer(&format!("/data/show/availableEpisodesDetail/{}", self.mode()))
            .and_then(|x| x.as_array()).cloned().unwrap_or_default();
        let mut eps: Vec<(f32, Episode)> = arr
            .iter()
            .filter_map(|val| {
                let label = val.as_str()?.to_string();
                let num = label.parse::<f32>().ok()?;
                Some((num, Episode { id: format!("{anime_id}:{label}"), number: num as u32, label, title: None, thumbnail: None }))
            })
            .collect();
        // sort by the real number so recaps like "12.5" land between 12 and 13
        eps.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(eps.into_iter().map(|(_, e)| e).collect())
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
            .json(&json!({"query": gql, "variables": {"showId": show_id, "translationType": self.mode(), "episodeString": ep_str}}))
            .send().await.context("allanime episode")?;

        let v: serde_json::Value = resp.json().await?;
        if let Some(errors) = v.get("errors") {
            anyhow::bail!("{}", explain_errors(errors));
        }
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
        sources.sort_by_key(|s| std::cmp::Reverse(quality_rank(s.quality.as_deref())));
        Ok(sources)
    }

    fn set_dub(&self, dub: bool) {
        self.set_dub_mode(dub);
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
                out.push(StreamSource { is_m3u8: url.contains(".m3u8"), url, quality: Some(res), headers: Self::referer_headers(), subtitles: Vec::new() });
            } else if chunk.contains("\"hls\"") {
                if let Some(url) = extract_json_field(chunk, "url") {
                    if url.contains("m3u8") {
                        let url = url.replace("\\u002F", "/");
                        // expand m3u8 master playlist to individual qualities like ani-cli does
                        match self.expand_m3u8(&url).await {
                            Ok(mut v) => out.append(&mut v),
                            Err(_) => out.push(StreamSource { url: url.clone(), quality: None, headers: Self::referer_headers(), is_m3u8: true, subtitles: Vec::new() }),
                        }
                    }
                }
            }
        }
        if out.is_empty() && text.contains("http") {
            // fallback: find any http url in response
            for token in text.split('"') {
                if token.starts_with("http") && (token.contains(".m3u8") || token.contains(".mp4")) {
                    out.push(StreamSource { url: token.to_string(), quality: None, headers: Self::referer_headers(), is_m3u8: token.contains(".m3u8"), subtitles: Vec::new() });
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
                    let res = line.split('x').next_back().unwrap_or("0").split(',').next().unwrap_or("0");
                    let full = if next.starts_with("http") { next.to_string() } else { format!("{base}{next}") };
                    out.push(StreamSource { url: full, quality: Some(format!("{}p", res.trim())), headers: Self::referer_headers(), is_m3u8: true, subtitles: Vec::new() });
                }
            }
        }
        Ok(out)
    }
}

/// Turn AllAnime's GraphQL `errors` into one readable sentence.
fn explain_errors(errors: &serde_json::Value) -> String {
    let messages: Vec<&str> = errors
        .as_array()
        .map(|a| a.iter().filter_map(|e| e.get("message").and_then(|m| m.as_str())).collect())
        .unwrap_or_default();
    if messages.contains(&"NEED_CAPTCHA") {
        "AllAnime wants a CAPTCHA solved before it gives out video links, so it can't play this right now".to_string()
    } else if messages.contains(&"AA_CRYPTO_MISSING") {
        "AllAnime now requires a signed request token it doesn't give to apps, so it can't play this right now".to_string()
    } else if messages.is_empty() {
        format!("AllAnime returned an error: {errors}")
    } else {
        format!("AllAnime returned an error: {}", messages.join(", "))
    }
}

fn extract_json_field(s: &str, field: &str) -> Option<String> {
    let pat = format!("\"{}\":\"", field);
    let start = s.find(&pat)? + pat.len();
    let rest = &s[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

impl AllAnimeProvider {
    fn set_dub_mode(&self, dub: bool) {
        self.dub.store(dub, Ordering::Relaxed);
    }
}

impl Default for AllAnimeProvider {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(plain: &str) -> String {
        format!("--{}", plain.bytes().map(|b| format!("{:02x}", b ^ 0x38)).collect::<String>())
    }

    #[test]
    fn decrypt_handles_every_character() {
        // `m`, `s`, `n`, `u`, `.`, `:`, `-`, `_` were all missing from the old lookup table.
        let plain = "/apivn/clock?id=Mx_9.sun-ma";
        assert_eq!(AllAnimeProvider::decrypt_hex(&encode(plain)), "/apivn/clock.json?id=Mx_9.sun-ma");
    }

    #[test]
    fn decrypt_does_not_double_json_suffix() {
        assert_eq!(AllAnimeProvider::decrypt_hex(&encode("/clock.json?x=1")), "/clock.json?x=1");
    }

    #[test]
    fn decrypt_matches_ani_cli_table_samples() {
        // a few pairs straight from ani-cli's sed table
        assert_eq!(AllAnimeProvider::decrypt_hex("--01080517"), "90=/");
    }

    #[test]
    fn errors_become_readable_sentences() {
        let captcha = serde_json::json!([{"message": "NEED_CAPTCHA", "extensions": {"code": "INTERNAL_SERVER_ERROR"}}]);
        assert!(explain_errors(&captcha).contains("CAPTCHA"));
        let crypto = serde_json::json!([{"message": "AA_CRYPTO_MISSING"}]);
        assert!(explain_errors(&crypto).contains("signed request token"));
        let other = serde_json::json!([{"message": "boom"}]);
        assert_eq!(explain_errors(&other), "AllAnime returned an error: boom");
    }

    #[test]
    fn quality_sorts_numerically() {
        assert!(quality_rank(Some("1080")) > quality_rank(Some("720")));
        assert_eq!(quality_rank(Some("1080p (HLS)")), 1080);
        assert_eq!(quality_rank(Some("auto")), 0);
        assert_eq!(quality_rank(None), 0);
    }
}
