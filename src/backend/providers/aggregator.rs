use anyhow::Result;

use super::scraper::ScraperProvider;
use super::traits::{Anime, AnimeProvider, MetadataProvider, StreamSource};

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
            stream_providers: vec![Box::new(ScraperProvider::new())],
        }
    }

    pub fn with_providers(metadata: M, providers: Vec<Box<dyn AnimeProvider>>) -> Self {
        Self {
            metadata,
            stream_providers: providers,
        }
    }

    pub fn add_provider(&mut self, p: impl AnimeProvider + 'static) {
        self.stream_providers.push(Box::new(p));
    }
    pub fn add_provider_front(&mut self, p: impl AnimeProvider + 'static) {
        self.stream_providers.insert(0, Box::new(p));
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

    pub async fn details(&self, id: &str) -> Result<Anime> {
        self.metadata.details(id).await
    }

    // --- Stream resolution with failover ---

    /// Resolve playable sources for an anime by trying each provider in order.
    /// `anime_id` is the AniList ID; providers map it via search internally.
    pub async fn resolve_streams(&self, anime_id: &str) -> Result<Vec<StreamSource>> {
        let mut last_err: Option<anyhow::Error> = None;
        for provider in &self.stream_providers {
            match provider.episodes(anime_id).await {
                Ok(episodes) if !episodes.is_empty() => {
                    let ep_id = &episodes[0].id;
                    match provider.streams(ep_id).await {
                        Ok(sources) if !sources.is_empty() => return Ok(sources),
                        Ok(_) => continue,
                        Err(e) => {
                            last_err = Some(e);
                            continue;
                        }
                    }
                }
                Ok(_) => continue,
                Err(e) => {
                    last_err = Some(e);
                    continue;
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("no stream providers available")))
    }

    /// Cozy, ani-cli style: resolve via Anime struct so AllAnime can search by title.
    /// Mirrors `ani-cli`'s `search_anime` → `episodes_list` → `get_episode_url` chain.
    /// Tries each provider: first search by id, then fallback to search by title (for allanime),
    /// then episodes -> streams.
    pub async fn resolve_for_anime(&self, anime: &Anime) -> Result<Vec<StreamSource>> {
        let mut last_err: Option<anyhow::Error> = None;
        for provider in &self.stream_providers {
            // 1) direct id attempt
            if let Ok(eps) = provider.episodes(&anime.id).await {
                if !eps.is_empty() {
                    if let Ok(src) = provider.streams(&eps[0].id).await {
                        if !src.is_empty() { return Ok(src); }
                    }
                }
            }
            // 2) title fallback — crucial for AllAnime where AniList id != allanime _id
            //    this mirrors ani-cli `search_anime "$title"` then `episodes_list "$id"`
            if let Ok(search_res) = provider.search(&anime.title, 3).await {
                if let Some(best) = search_res.first() {
                    if let Ok(eps) = provider.episodes(&best.id).await {
                        if !eps.is_empty() {
                            // prefer episode 1 for cozy preview, like ani-cli default
                            let ep = &eps[0];
                            match provider.streams(&ep.id).await {
                                Ok(src) if !src.is_empty() => return Ok(src),
                                Ok(_) => {},
                                Err(e) => last_err = Some(e),
                            }
                        }
                    }
                }
            }
            last_err = Some(anyhow::anyhow!("no streams via provider for {}", anime.title));
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("no stream providers available")))
    }

    /// Resolve streams for a concrete provider episode ID directly.
    pub async fn resolve_episode_streams(&self, episode_id: &str) -> Result<Vec<StreamSource>> {
        let mut last_err: Option<anyhow::Error> = None;
        for p in &self.stream_providers {
            match p.streams(episode_id).await {
                Ok(s) if !s.is_empty() => return Ok(s),
                Ok(_) => continue,
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("no streams found for episode {episode_id}")))
    }
}
