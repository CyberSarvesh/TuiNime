use anyhow::Result;

use super::traits::{Anime, AnimeProvider, Episode, MetadataProvider, StreamSource};

/// Multi-provider resolution manager with automatic failover.
///
/// Metadata (search/trending) is served by `M` (AniList), while stream
/// resolution fans out across `AnimeProvider`s in order until one succeeds.
/// This keeps metadata fast and streaming resilient.
pub struct ProviderAggregator<M> {
    metadata: M,
    stream_providers: Vec<Box<dyn AnimeProvider>>,
}

impl<M> ProviderAggregator<M>
where
    M: MetadataProvider,
{
    pub fn new(metadata: M) -> Self {
        Self {
            metadata,
            stream_providers: Vec::new(),
        }
    }

    pub fn add_provider(&mut self, p: impl AnimeProvider + 'static) {
        self.stream_providers.push(Box::new(p));
    }
    // --- Metadata (delegated to AniList) ---

    pub async fn trending(&self, limit: u8) -> Result<Vec<Anime>> {
        self.metadata.trending(limit).await
    }

    pub async fn seasonal(&self, year: u16, season: &str, limit: u8) -> Result<Vec<Anime>> {
        self.metadata.seasonal(year, season, limit).await
    }

    pub async fn search(&self, query: &str, limit: u8) -> Result<Vec<Anime>> {
        // Try metadata provider first (richer data), fall back to stream providers
        match self.metadata.search(query, limit).await {
            Ok(v) if !v.is_empty() => Ok(v),
            Ok(_) | Err(_) => {
                for p in &self.stream_providers {
                    match p.search(query, limit).await {
                        Ok(v) if !v.is_empty() => return Ok(v),
                        _ => continue,
                    }
                }
                // Return original empty/metadata error if all fail
                self.metadata.search(query, limit).await
            }
        }
    }

    // --- Stream resolution with failover ---

    /// Subtitled or dubbed listings for every provider.
    pub fn set_dub(&self, dub: bool) {
        for p in &self.stream_providers {
            p.set_dub(dub);
        }
    }

    /// The names to search a show by: its title first, then alternate (romaji/native)
    /// titles, since providers like AllAnime list shows under romaji.
    fn names(anime: &Anime) -> Vec<&str> {
        let mut names: Vec<&str> = vec![anime.title.as_str()];
        names.extend(anime.alt_titles.iter().map(String::as_str));
        names.dedup();
        names
    }

    /// Find `anime` on one provider and list its episodes.
    ///
    /// AniList ids mean nothing to stream providers, so this searches by name and picks the
    /// best-matching result (falling back to the first non-empty one).
    async fn episodes_on(&self, index: usize, anime: &Anime) -> Result<EpisodeList> {
        let provider = &self.stream_providers[index];
        let names = Self::names(anime);
        let mut chosen: Option<Anime> = None;
        let mut first_hit: Option<Anime> = None;
        let mut search_err = None;
        for query in &names {
            match provider.search(query, 8).await {
                Ok(results) => {
                    if first_hit.is_none() {
                        first_hit = results.iter().find(|r| r.episodes != Some(0)).cloned();
                    }
                    if let Some(best) = best_match(&names, &results) {
                        chosen = Some(best.clone());
                        break;
                    }
                }
                Err(e) => search_err = Some(e),
            }
        }
        let Some(show) = chosen.or(first_hit) else {
            return Err(search_err.unwrap_or_else(|| anyhow::anyhow!("doesn't list {}", anime.title)));
        };
        let episodes = provider.episodes(&show.id).await?;
        if episodes.is_empty() {
            anyhow::bail!("lists no episodes for {}", show.title);
        }
        Ok(EpisodeList { provider: index, source: provider.name(), show: show.title, episodes })
    }

    /// ani-cli style: the first provider (in order) that can list the show's episodes wins.
    pub async fn episodes_for(&self, anime: &Anime) -> Result<EpisodeList> {
        let mut failures = Vec::new();
        for index in 0..self.stream_providers.len() {
            match self.episodes_on(index, anime).await {
                Ok(list) => return Ok(list),
                Err(e) => failures.push(format!("{}: {e:#}", self.stream_providers[index].name())),
            }
        }
        Err(all_failed(&format!("no source has episodes for {}", anime.title), failures))
    }

    /// Stream links for `episode` of `anime`, which `provider` listed.
    ///
    /// If that provider can't produce links (blocked, broken, no server), each other provider
    /// is tried in order: find the same show there, find the episode with the same label,
    /// and ask for its links. Reports which source the links came from.
    pub async fn streams_with_fallback(&self, anime: &Anime, provider: usize, episode: &Episode) -> Result<Streams> {
        let mut failures = Vec::new();
        let order = std::iter::once(provider).chain((0..self.stream_providers.len()).filter(|&i| i != provider));
        for index in order {
            let Some(p) = self.stream_providers.get(index) else { continue };
            let attempt = async {
                let episode_id = if index == provider {
                    episode.id.clone()
                } else {
                    let list = self.episodes_on(index, anime).await?;
                    list.episodes
                        .iter()
                        .find(|e| e.label == episode.label)
                        .map(|e| e.id.clone())
                        .ok_or_else(|| anyhow::anyhow!("has no episode {}", episode.label))?
                };
                let sources = p.streams(&episode_id).await?;
                if sources.is_empty() {
                    anyhow::bail!("gave no playable links");
                }
                Ok::<_, anyhow::Error>(sources)
            };
            match attempt.await {
                Ok(sources) => {
                    let tried: Vec<&'static str> = failures.iter().map(|(name, _)| *name).collect();
                    return Ok(Streams { sources, source: p.name(), failed: tried });
                }
                Err(e) => failures.push((p.name(), format!("{e:#}"))),
            }
        }
        let lines = failures.into_iter().map(|(name, e)| format!("{name}: {e}")).collect();
        Err(all_failed(&format!("no source could play {} episode {}", anime.title, episode.label), lines))
    }

    /// Streams for episode 1 — what `p` plays straight from browse.
    pub async fn resolve_for_anime(&self, anime: &Anime) -> Result<Streams> {
        let list = self.episodes_for(anime).await?;
        self.streams_with_fallback(anime, list.provider, &list.episodes[0]).await
    }
}

/// One error that names every source and why it failed.
fn all_failed(summary: &str, failures: Vec<String>) -> anyhow::Error {
    if failures.is_empty() {
        anyhow::anyhow!("{summary} (no stream sources are enabled)")
    } else {
        anyhow::anyhow!("{summary} — {}", failures.join("; "))
    }
}

/// A show's episodes as listed by one stream provider.
#[derive(Debug, Clone)]
pub struct EpisodeList {
    /// Index of the provider that listed them (pass back to `streams_with_fallback`).
    pub provider: usize,
    /// That provider's name, e.g. "hianime.at".
    pub source: &'static str,
    /// The provider's name for the show (may differ from the AniList title).
    pub show: String,
    pub episodes: Vec<Episode>,
}

/// Playable links and where they came from.
#[derive(Debug, Clone)]
pub struct Streams {
    /// Best quality first.
    pub sources: Vec<StreamSource>,
    /// Provider that produced them.
    pub source: &'static str,
    /// Providers tried first that couldn't (empty when the first choice worked).
    pub failed: Vec<&'static str>,
}

/// Lowercase, keep only letters/digits, collapse everything else to single spaces.
fn normalize(s: &str) -> String {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Pick the provider result that best matches any of `names`.
/// Exact match beats prefix match beats substring; results with zero episodes are skipped.
/// Returns `None` when nothing matches at all, so the caller can try another query.
pub(crate) fn best_match<'a>(names: &[&str], candidates: &'a [Anime]) -> Option<&'a Anime> {
    let targets: Vec<String> = names.iter().map(|n| normalize(n)).filter(|n| !n.is_empty()).collect();
    let mut best: Option<(&Anime, (u8, std::cmp::Reverse<usize>))> = None;
    for c in candidates.iter().filter(|c| c.episodes != Some(0)) {
        let cand = normalize(&c.title);
        let score = targets
            .iter()
            .map(|t| {
                let rank = if cand == *t {
                    3
                } else if cand.starts_with(t.as_str()) || t.starts_with(cand.as_str()) {
                    2
                } else if cand.contains(t.as_str()) || t.contains(cand.as_str()) {
                    1
                } else {
                    0
                };
                (rank, std::cmp::Reverse(cand.len().abs_diff(t.len())))
            })
            .max()
            .unwrap_or((0, std::cmp::Reverse(usize::MAX)));
        if score.0 == 0 {
            continue;
        }
        // strict `>` keeps the provider's own ordering on ties
        if best.as_ref().is_none_or(|(_, s)| score > *s) {
            best = Some((c, score));
        }
    }
    best.map(|(c, _)| c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    /// A stream source that lists one show, numbering its episodes by `labels`,
    /// and either gives a link or fails with `stream_err`.
    struct Fake {
        name: &'static str,
        show: &'static str,
        labels: Vec<&'static str>,
        stream_err: Option<&'static str>,
    }

    #[async_trait]
    impl AnimeProvider for Fake {
        fn name(&self) -> &'static str { self.name }
        async fn search(&self, _q: &str, _l: u8) -> Result<Vec<Anime>> {
            Ok(vec![show(self.show, 12)])
        }
        async fn episodes(&self, _id: &str) -> Result<Vec<Episode>> {
            Ok(self.labels.iter().map(|l| Episode {
                id: format!("{}:{l}", self.name), number: 0, label: l.to_string(), title: None, thumbnail: None,
            }).collect())
        }
        async fn streams(&self, episode_id: &str) -> Result<Vec<StreamSource>> {
            match self.stream_err {
                Some(e) => anyhow::bail!("{e}"),
                None => Ok(vec![StreamSource { url: format!("https://{episode_id}"), quality: None, headers: None, is_m3u8: true, subtitles: vec![] }]),
            }
        }
    }

    struct NoMeta;
    #[async_trait]
    impl MetadataProvider for NoMeta {
        async fn trending(&self, _l: u8) -> Result<Vec<Anime>> { Ok(vec![]) }
        async fn seasonal(&self, _y: u16, _s: &str, _l: u8) -> Result<Vec<Anime>> { Ok(vec![]) }
        async fn search(&self, _q: &str, _l: u8) -> Result<Vec<Anime>> { Ok(vec![]) }
    }

    fn agg(providers: Vec<Fake>) -> ProviderAggregator<NoMeta> {
        let mut a = ProviderAggregator::new(NoMeta);
        for p in providers {
            a.add_provider(p);
        }
        a
    }

    fn fake(name: &'static str, labels: &[&'static str], stream_err: Option<&'static str>) -> Fake {
        Fake { name, show: "Frieren", labels: labels.to_vec(), stream_err }
    }

    #[tokio::test]
    async fn falls_back_to_the_same_episode_on_the_next_source() {
        let a = agg(vec![
            fake("first", &["1", "2", "3"], Some("NEED_CAPTCHA")),
            fake("second", &["1", "2", "3"], None),
        ]);
        let target = show("Frieren", 3);
        let list = a.episodes_for(&target).await.unwrap();
        assert_eq!(list.source, "first");
        let s = a.streams_with_fallback(&target, list.provider, &list.episodes[1]).await.unwrap();
        assert_eq!(s.source, "second");
        assert_eq!(s.failed, ["first"]);
        assert_eq!(s.sources[0].url, "https://second:2", "same episode label on the other source");
    }

    #[tokio::test]
    async fn first_choice_working_reports_no_failures() {
        let a = agg(vec![fake("first", &["1"], None), fake("second", &["1"], None)]);
        let s = a.resolve_for_anime(&show("Frieren", 1)).await.unwrap();
        assert_eq!((s.source, s.failed.len()), ("first", 0));
    }

    #[tokio::test]
    async fn skips_a_source_missing_that_episode_and_explains_total_failure() {
        let a = agg(vec![
            fake("first", &["1", "2"], Some("blocked")),
            fake("second", &["1"], None), // has no episode 2
            fake("third", &["1", "2"], Some("down")),
        ]);
        let target = show("Frieren", 2);
        let list = a.episodes_for(&target).await.unwrap();
        let err = a.streams_with_fallback(&target, list.provider, &list.episodes[1]).await.unwrap_err().to_string();
        assert!(err.contains("no source could play Frieren episode 2"), "{err}");
        assert!(err.contains("first: blocked") && err.contains("second: has no episode 2") && err.contains("third: down"), "{err}");
    }

    #[tokio::test]
    async fn no_sources_enabled_is_explained() {
        let a = agg(vec![]);
        let err = a.episodes_for(&show("Frieren", 1)).await.unwrap_err().to_string();
        assert!(err.contains("no stream sources are enabled"), "{err}");
    }

    fn show(title: &str, eps: u32) -> Anime {
        Anime {
            id: title.into(), title: title.into(), description: None, cover_image: None,
            banner_image: None, score: None, year: None, episodes: Some(eps), status: None,
            genres: vec![], alt_titles: vec![],
        }
    }

    #[test]
    fn prefers_exact_romaji_match_over_first_result() {
        let results = vec![
            show("Sousou no Frieren: ●● no Mahou Part 3", 6),
            show("Sousou no Frieren 2nd Season", 10),
            show("Sousou no Frieren", 28),
        ];
        let names = ["Frieren: Beyond Journey's End", "Sousou no Frieren"];
        assert_eq!(best_match(&names, &results).unwrap().title, "Sousou no Frieren");
    }

    #[test]
    fn skips_shows_without_episodes() {
        let results = vec![show("Naruto", 0), show("Naruto Shippuden", 500)];
        assert_eq!(best_match(&["Naruto"], &results).unwrap().title, "Naruto Shippuden");
    }

    #[test]
    fn no_match_returns_none() {
        assert!(best_match(&["Bleach"], &[show("One Piece", 1000)]).is_none());
    }
}

/// Live checks against the real services. Run with `cargo test -- --ignored`.
#[cfg(test)]
mod live {
    use super::*;
    use crate::backend::{AllAnimeProvider, AniListClient};

    #[tokio::test]
    #[ignore = "network"]
    async fn finds_frieren_episodes_by_romaji_title() {
        let mut agg = ProviderAggregator::new(AniListClient::new());
        agg.add_provider(AllAnimeProvider::new());
        let anime = Anime {
            id: "154587".into(), title: "Frieren: Beyond Journey's End".into(), description: None,
            cover_image: None, banner_image: None, score: None, year: None, episodes: Some(28),
            status: None, genres: vec![], alt_titles: vec!["Sousou no Frieren".into()],
        };
        let list = agg.episodes_for(&anime).await.expect("episodes");
        println!("listed as {:?}: {} episodes, first {:?}", list.show, list.episodes.len(), list.episodes.first().map(|e| &e.label));
        assert_eq!(list.show, "Sousou no Frieren");
        assert!(list.episodes.len() >= 28);
        assert_eq!(list.episodes[0].label, "1");
        agg.set_dub(true);
        let dub = agg.episodes_for(&anime).await.expect("dub episodes");
        println!("dub: {} episodes", dub.episodes.len());
    }

    #[tokio::test]
    #[ignore = "network"]
    async fn allanime_captcha_falls_back_to_hianime() {
        use crate::backend::HiAnimeProvider;
        // AllAnime first on purpose: it lists episodes but refuses links
        let mut agg = ProviderAggregator::new(AniListClient::new());
        agg.add_provider(AllAnimeProvider::new());
        agg.add_provider(HiAnimeProvider::new());
        let anime = Anime {
            id: "154587".into(), title: "Frieren: Beyond Journey's End".into(), description: None,
            cover_image: None, banner_image: None, score: None, year: None, episodes: Some(28),
            status: None, genres: vec![], alt_titles: vec!["Sousou no Frieren".into()],
        };
        let list = agg.episodes_for(&anime).await.expect("episodes");
        println!("episodes from {} as {:?}", list.source, list.show);
        let ep = &list.episodes[4];
        let s = agg.streams_with_fallback(&anime, list.provider, ep).await.expect("some source plays it");
        println!("episode {} → {} (failed: {:?}), best {:?} {}", ep.label, s.source, s.failed, s.sources[0].quality, s.sources[0].url);
        assert_eq!(s.source, "hianime.at");
    }
}
