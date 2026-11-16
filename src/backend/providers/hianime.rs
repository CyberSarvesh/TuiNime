//! hianime.at provider — the source current ani-cli (v5.1.x) uses.
//!
//! Flow, matching ani-cli's `hianime_search` / `hianime_episodes` / `hianime_m3u8`:
//!   1. `GET /search?keyword=…` → HTML; each `.film-detail h3.film-name a` gives the show's
//!      slug (`frieren-beyond-journeys-end-481`), title, and romaji name (`data-jname`).
//!   2. `GET /api/theme/episode/list/<numeric id>` → JSON `{html}`; each `a.ep-item` has
//!      `data-number` (label) and `data-id` (episode id).
//!   3. `GET /api/theme/episode/servers?episodeId=<id>` → JSON `{html}`; the `.server-item`
//!      with the wanted `data-type` (sub/dub) and `data-server-name="ZokoAnime"` carries a
//!      base64 `data-hash` = the embed page URL.
//!   4. The embed page holds `window.__P="<base64>"`: JSON XOR-ed with `otaku-embed-v1`.
//!      Its `src` is a master `.m3u8`; `subtitles` lists `.vtt` tracks.
//!   5. The master playlist lists one variant per quality; the stream host wants the
//!      embed site as Referer.
//!
//! Plain requests only: no TLS fingerprint tricks or Cloudflare workarounds. If the site
//! starts blocking, this provider reports it and the aggregator moves on.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use async_trait::async_trait;
use base64::Engine;
use scraper::{Html, Selector};

use super::traits::{Anime, AnimeProvider, Episode, StreamSource};

const BASE: &str = "https://hianime.at";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";
/// Key the embed page XORs its config with.
const EMBED_KEY: &[u8] = b"otaku-embed-v1";
/// The only embed server whose page format we understand (same as ani-cli).
const SERVER_NAME: &str = "ZokoAnime";

pub struct HiAnimeProvider {
    client: reqwest::Client,
    dub: AtomicBool,
}

impl HiAnimeProvider {
    pub fn new() -> Self {
        Self { client: super::http_client(UA), dub: AtomicBool::new(false) }
    }

    fn mode(&self) -> &'static str {
        if self.dub.load(Ordering::Relaxed) { "dub" } else { "sub" }
    }

    async fn get_text(&self, url: &str, referer: Option<&str>) -> Result<String> {
        let mut req = self.client.get(url);
        if let Some(r) = referer {
            req = req.header("Referer", r);
        }
        let resp = req.send().await.with_context(|| format!("hianime: fetch {url}"))?;
        let status = resp.status();
        let text = resp.text().await.context("hianime: read body")?;
        if is_cloudflare_challenge(&text) {
            anyhow::bail!("hianime.at is showing a Cloudflare check to apps right now");
        }
        if !status.is_success() {
            anyhow::bail!("hianime.at answered HTTP {status} for {url}");
        }
        Ok(text)
    }

    /// The `html` field of one of the site's JSON endpoints.
    async fn get_json_html(&self, url: &str) -> Result<String> {
        let v: serde_json::Value = serde_json::from_str(&self.get_text(url, None).await?).context("hianime: parse JSON")?;
        v.get("html").and_then(|h| h.as_str()).map(str::to_string).context("hianime: JSON has no html field")
    }
}

impl Default for HiAnimeProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AnimeProvider for HiAnimeProvider {
    fn name(&self) -> &'static str {
        "hianime.at"
    }

    async fn search(&self, query: &str, limit: u8) -> Result<Vec<Anime>> {
        let url = format!("{BASE}/search?keyword={}", urlencode(query));
        Ok(parse_search(&self.get_text(&url, None).await?, limit as usize))
    }

    async fn episodes(&self, slug: &str) -> Result<Vec<Episode>> {
        let id = numeric_id(slug).with_context(|| format!("hianime: no numeric id in {slug}"))?;
        let html = self.get_json_html(&format!("{BASE}/api/theme/episode/list/{id}")).await?;
        Ok(parse_episodes(&html, slug))
    }

    async fn streams(&self, episode_id: &str) -> Result<Vec<StreamSource>> {
        let mode = self.mode();
        let servers = self.get_json_html(&format!("{BASE}/api/theme/episode/servers?episodeId={episode_id}")).await?;
        let embed = pick_server(&servers, mode)
            .with_context(|| format!("hianime.at has no {SERVER_NAME} {mode} server for this episode"))?;
        let referer = site_root(&embed).context("hianime: embed URL has no host")?;

        let page = self.get_text(&embed, None).await?;
        let config = decode_embed_config(&page)?;
        let master = config.get("src").and_then(|s| s.as_str()).filter(|s| s.contains(".m3u8")).context("hianime: embed config has no playlist")?;
        let subtitles = default_subtitles(&config);
        let headers: HashMap<String, String> = [("Referer".to_string(), referer.clone())].into_iter().collect();
        let source = |url: String, quality: Option<String>| StreamSource {
            url,
            quality,
            headers: Some(headers.clone()),
            is_m3u8: true,
            subtitles: subtitles.clone(),
        };

        let mut out: Vec<StreamSource> = match self.get_text(master, Some(&referer)).await {
            Ok(playlist) => parse_master_playlist(master, &playlist).into_iter().map(|(q, u)| source(u, Some(q))).collect(),
            Err(e) => {
                tracing::debug!("hianime: master playlist unreadable ({e:#}); handing it to mpv as-is");
                Vec::new()
            }
        };
        if out.is_empty() {
            out.push(source(master.to_string(), None));
        }
        out.sort_by_key(|s| std::cmp::Reverse(super::allanime::quality_rank(s.quality.as_deref())));
        Ok(out)
    }

    fn set_dub(&self, dub: bool) {
        self.dub.store(dub, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------- parsing (pure, tested)

fn is_cloudflare_challenge(html: &str) -> bool {
    html.contains("<title>Just a moment")
}

/// `frieren-beyond-journeys-end-481` → `481`.
fn numeric_id(slug: &str) -> Option<&str> {
    let id = slug.rsplit('-').next()?;
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_digit())).then_some(id)
}

/// `https://zokoanime.video/stream/…` → `https://zokoanime.video/`.
fn site_root(url: &str) -> Option<String> {
    let after_scheme = url.find("://")? + 3;
    let end = url[after_scheme..].find('/').map(|i| after_scheme + i).unwrap_or(url.len());
    Some(format!("{}/", &url[..end]))
}

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("static selector")
}

fn parse_search(page: &str, limit: usize) -> Vec<Anime> {
    // the "top 10" sidebar repeats the result markup; ignore everything from it on
    let main = page.split("id=\"main-sidebar\"").next().unwrap_or(page);
    let doc = Html::parse_document(main);
    doc.select(&sel(".film-detail .film-name a"))
        .filter_map(|a| {
            let slug = a.value().attr("href")?.trim_end_matches('/').rsplit('/').next()?.to_string();
            numeric_id(&slug)?;
            let title = a.value().attr("title").map(str::to_string).unwrap_or_else(|| a.text().collect::<String>().trim().to_string());
            let romaji = a.value().attr("data-jname").map(str::trim).filter(|j| !j.is_empty() && *j != title);
            Some(Anime {
                id: slug,
                title,
                description: None,
                cover_image: None,
                banner_image: None,
                score: None,
                year: None,
                episodes: None,
                status: None,
                genres: vec![],
                alt_titles: romaji.map(|j| vec![j.to_string()]).unwrap_or_default(),
            })
        })
        .take(limit)
        .collect()
}

fn parse_episodes(html: &str, slug: &str) -> Vec<Episode> {
    let doc = Html::parse_fragment(html);
    // the list can include other seasons; keep only links into this show
    let marker = format!("/watch/{slug}?ep=");
    let mut eps: Vec<(f32, Episode)> = doc
        .select(&sel("a.ep-item"))
        .filter(|a| a.value().attr("href").is_some_and(|h| h.contains(&marker)))
        .filter_map(|a| {
            let label = a.value().attr("data-number")?.trim().to_string();
            let id = a.value().attr("data-id")?.trim().to_string();
            let num = label.parse::<f32>().ok()?;
            let title = a.value().attr("title").map(str::to_string).filter(|t| !t.is_empty());
            Some((num, Episode { id, number: num as u32, label, title, thumbnail: None }))
        })
        .collect();
    eps.sort_by(|a, b| a.0.total_cmp(&b.0));
    eps.into_iter().map(|(_, e)| e).collect()
}

/// The embed URL of the ZokoAnime server for `mode` ("sub"/"dub"), decoded from `data-hash`.
fn pick_server(html: &str, mode: &str) -> Option<String> {
    let doc = Html::parse_fragment(html);
    let hash = doc.select(&sel(".server-item")).find_map(|s| {
        let v = s.value();
        (v.attr("data-type") == Some(mode) && v.attr("data-server-name") == Some(SERVER_NAME)).then(|| v.attr("data-hash")).flatten()
    })?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(hash.trim()).ok()?;
    String::from_utf8(bytes).ok().filter(|u| u.starts_with("http"))
}

/// Pull `window.__P` out of the embed page and undo base64 + XOR(`otaku-embed-v1`).
fn decode_embed_config(page: &str) -> Result<serde_json::Value> {
    let start = page.find("window.__P=\"").context("hianime: embed page has no player config")? + "window.__P=\"".len();
    let end = page[start..].find('"').context("hianime: player config is cut off")? + start;
    let raw = base64::engine::general_purpose::STANDARD.decode(&page[start..end]).context("hianime: player config isn't base64")?;
    let json: Vec<u8> = raw.iter().zip(EMBED_KEY.iter().cycle()).map(|(b, k)| b ^ k).collect();
    serde_json::from_slice(&json).context("hianime: player config isn't JSON after decoding")
}

/// Subtitle URLs, default track first.
fn default_subtitles(config: &serde_json::Value) -> Vec<String> {
    let mut tracks: Vec<(bool, String)> = config
        .get("subtitles")
        .and_then(|s| s.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|t| Some((t.get("default").and_then(|d| d.as_bool()).unwrap_or(false), t.get("src")?.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default();
    tracks.sort_by_key(|(default, _)| !*default);
    tracks.into_iter().map(|(_, src)| src).collect()
}

/// `(quality, url)` for each variant in an HLS master playlist; relative URLs are resolved
/// against the master's directory.
fn parse_master_playlist(master_url: &str, text: &str) -> Vec<(String, String)> {
    let dir = master_url.rsplit_once('/').map(|(d, _)| d).unwrap_or(master_url);
    let mut out = Vec::new();
    let mut lines = text.lines().map(str::trim);
    while let Some(line) = lines.next() {
        let Some(attrs) = line.strip_prefix("#EXT-X-STREAM-INF:") else { continue };
        let Some(uri) = lines.next().filter(|u| !u.is_empty() && !u.starts_with('#')) else { continue };
        let height = attrs
            .split(',')
            .find_map(|a| a.strip_prefix("RESOLUTION="))
            .and_then(|r| r.split('x').nth(1))
            .map(|h| format!("{h}p"))
            .unwrap_or_else(|| "auto".to_string());
        let url = if uri.starts_with("http") { uri.to_string() } else { format!("{dir}/{uri}") };
        out.push((height, url));
    }
    out
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b' ' => "+".to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str = r#"<div id="main-content">
      <div class="flw-item"><div class="film-detail"><h3 class="film-name">
        <a href="https://hianime.at/frieren-beyond-journeys-end-481" title="Frieren: Beyond Journey&#039;s End" class="dynamic-name" data-jname="Sousou no Frieren">Frieren</a>
      </h3></div></div>
      <div class="flw-item"><div class="film-detail"><h3 class="film-name">
        <a href="https://hianime.at/sousou-no-frieren-3rd-7220" title="Sousou no Frieren 3rd Season" data-jname="Sousou no Frieren 3rd Season">x</a>
      </h3></div></div>
    </div>
    <div id="main-sidebar"><div class="film-detail"><h3 class="film-name"><a href="/one-piece-100" title="One Piece">x</a></h3></div></div>"#;

    #[test]
    fn search_reads_slug_title_and_romaji_and_skips_sidebar() {
        let r = parse_search(SEARCH, 10);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].id, "frieren-beyond-journeys-end-481");
        assert_eq!(r[0].title, "Frieren: Beyond Journey's End");
        assert_eq!(r[0].alt_titles, vec!["Sousou no Frieren".to_string()]);
        assert!(r[1].alt_titles.is_empty(), "romaji equal to the title is not repeated");
        assert_eq!(parse_search(SEARCH, 1).len(), 1);
    }

    #[test]
    fn episodes_keep_only_this_show_and_sort_numerically() {
        let html = r#"
          <a title="Two" class="ssl-item ep-item" data-number="2" data-id="9228" href="https://hianime.at/watch/frieren-beyond-journeys-end-481?ep=9228">2</a>
          <a title="Recap" class="ssl-item ep-item" data-number="1.5" data-id="9300" href="https://hianime.at/watch/frieren-beyond-journeys-end-481?ep=9300">1.5</a>
          <a title="The Journey&#039;s End" class="ssl-item ep-item" data-number="1" data-id="9227" href="https://hianime.at/watch/frieren-beyond-journeys-end-481?ep=9227">1</a>
          <a class="ssl-item ep-item" data-number="1" data-id="1" href="https://hianime.at/watch/other-show-99?ep=1">1</a>"#;
        let eps = parse_episodes(html, "frieren-beyond-journeys-end-481");
        let labels: Vec<&str> = eps.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels, ["1", "1.5", "2"]);
        assert_eq!(eps[0].id, "9227");
        assert_eq!(eps[0].title.as_deref(), Some("The Journey's End"));
    }

    #[test]
    fn picks_the_zokoanime_server_for_the_language() {
        let sub = base64::engine::general_purpose::STANDARD.encode("https://zokoanime.video/stream/mal/52991/1/sub");
        let dub = base64::engine::general_purpose::STANDARD.encode("https://zokoanime.video/stream/mal/52991/1/dub");
        let html = format!(
            r#"<div class="item server-item" data-type="sub" data-server-name="HD-1" data-hash="eA==">HD-1</div>
               <div class="item server-item" data-type="sub" data-server-name="ZokoAnime" data-hash="{sub}">Z</div>
               <div class="item server-item" data-type="dub" data-server-name="ZokoAnime" data-hash="{dub}">Z</div>"#
        );
        assert_eq!(pick_server(&html, "sub").as_deref(), Some("https://zokoanime.video/stream/mal/52991/1/sub"));
        assert_eq!(pick_server(&html, "dub").as_deref(), Some("https://zokoanime.video/stream/mal/52991/1/dub"));
        assert_eq!(pick_server(&html, "raw"), None);
    }

    #[test]
    fn decodes_the_xor_scrambled_player_config() {
        let json = br#"{"src":"https://hls.example/p/x/master.m3u8","subtitles":[{"label":"Spanish","src":"https://s/es.vtt"},{"label":"English","default":true,"src":"https://s/en.vtt"}]}"#;
        let scrambled: Vec<u8> = json.iter().zip(EMBED_KEY.iter().cycle()).map(|(b, k)| b ^ k).collect();
        let page = format!(r#"<script>window.__P="{}";</script>"#, base64::engine::general_purpose::STANDARD.encode(scrambled));
        let config = decode_embed_config(&page).unwrap();
        assert_eq!(config["src"], "https://hls.example/p/x/master.m3u8");
        assert_eq!(default_subtitles(&config), ["https://s/en.vtt", "https://s/es.vtt"]);
        assert!(decode_embed_config("<html>nothing here</html>").is_err());
    }

    #[test]
    fn master_playlist_variants_resolve_relative_urls() {
        let text = "#EXTM3U\n#EXT-X-VERSION:4\n#EXT-X-STREAM-INF:BANDWIDTH=900000,RESOLUTION=640x360\n360/index.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=5300000,RESOLUTION=1920x1080\nhttps://cdn/abs/1080.m3u8\n";
        let v = parse_master_playlist("https://hls.example/p/x/master.m3u8", text);
        assert_eq!(v, [
            ("360p".to_string(), "https://hls.example/p/x/360/index.m3u8".to_string()),
            ("1080p".to_string(), "https://cdn/abs/1080.m3u8".to_string()),
        ]);
    }

    #[test]
    fn small_helpers() {
        assert_eq!(numeric_id("frieren-beyond-journeys-end-481"), Some("481"));
        assert_eq!(numeric_id("no-number"), None);
        assert_eq!(site_root("https://zokoanime.video/stream/mal/1").as_deref(), Some("https://zokoanime.video/"));
        assert_eq!(urlencode("black clover!"), "black+clover%21");
        assert!(is_cloudflare_challenge("<html><title>Just a moment...</title>"));
    }
}

/// Live checks against hianime.at. Run with `cargo test -- --ignored`.
#[cfg(test)]
mod live {
    use super::*;

    #[tokio::test]
    #[ignore = "network"]
    async fn search_to_playable_stream() {
        let p = HiAnimeProvider::new();
        let shows = p.search("frieren", 5).await.expect("search");
        println!("search: {:?}", shows.iter().map(|s| (&s.id, &s.title)).collect::<Vec<_>>());
        let show = shows.iter().find(|s| s.id == "frieren-beyond-journeys-end-481").expect("main series listed");
        let eps = p.episodes(&show.id).await.expect("episodes");
        println!("episodes: {} (first {:?})", eps.len(), eps.first().map(|e| (&e.label, &e.id, &e.title)));
        assert!(eps.len() >= 28);

        let sources = p.streams(&eps[0].id).await.expect("streams");
        for s in &sources {
            println!("source: {:?} {} subs={}", s.quality, s.url, s.subtitles.len());
        }
        let best = &sources[0];
        assert!(best.is_m3u8 && !best.subtitles.is_empty());
        let referer = best.headers.as_ref().unwrap()["Referer"].clone();
        let playlist = p.get_text(&best.url, Some(&referer)).await.expect("variant playlist fetchable with Referer");
        assert!(playlist.starts_with("#EXTM3U"), "got: {}", &playlist[..playlist.len().min(200)]);
        println!("best variant playlist OK ({} lines); referer {referer}", playlist.lines().count());
        println!("PLAY {}|{}|{}", best.url, referer, best.subtitles[0]);

        p.set_dub(true);
        let dub = p.streams(&eps[0].id).await.expect("dub streams");
        println!("dub: {} sources, best {:?}", dub.len(), dub[0].quality);
    }
}
