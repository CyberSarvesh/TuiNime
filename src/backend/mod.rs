//! Backend — all scraping, caching, playback. Frontend only sees this interface.
pub mod cache;
pub mod player;
pub mod providers;

pub use cache::ImageCache;
pub use player::MpvPlayer;
pub use providers::traits::{Anime, StreamSource};
#[allow(unused_imports)]
pub use providers::aggregator::ProviderAggregator;
pub use providers::anilist::AniListClient;
pub use providers::allanime::AllAnimeProvider;
#[allow(unused_imports)]
pub use providers::scraper::ScraperProvider;

use anyhow::Result;

/// Clean interface that frontend consumes. No scraper details leak.
pub struct MiruBackend {
    pub cache: ImageCache,
    pub player: MpvPlayer,
    aggregator: ProviderAggregator<AniListClient>,
}

impl MiruBackend {
    pub fn new() -> Self {
        let cache = ImageCache::new().unwrap_or_else(|e| {
            tracing::warn!("cache init failed: {e}; temp fallback");
            ImageCache::new_temp()
        });
        let meta = AniListClient::new();
        let mut aggregator = ProviderAggregator::new(meta);
        aggregator.add_provider_front(AllAnimeProvider::new());
        Self { cache, player: MpvPlayer::new(), aggregator }
    }

    // --- metadata (AniList) with cozy fallback so UI never shows "nothing here yet" ---
    pub async fn trending(&self, limit: u8) -> Vec<Anime> {
        match self.aggregator.trending(limit).await {
            Ok(list) if !list.is_empty() => {
                // warm poster cache (cozy, not blocking)
                let urls: Vec<String> = list.iter().filter_map(|a| a.cover_image.clone()).collect();
                self.cache.prefetch_many(urls);
                list
            }
            Ok(_) | Err(_) => {
                tracing::warn!("trending offline, using cozy fallback");
                fallback_anime()
            }
        }
    }

    pub async fn search(&self, q: &str, limit: u8) -> Result<Vec<Anime>> {
        let res = self.aggregator.search(q, limit).await?;
        let urls: Vec<String> = res.iter().filter_map(|a| a.cover_image.clone()).collect();
        self.cache.prefetch_many(urls);
        Ok(res)
    }

    pub async fn resolve(&self, anime: &Anime) -> Result<Vec<StreamSource>> {
        self.aggregator.resolve_for_anime(anime).await
    }

    pub fn cache(&self) -> &ImageCache { &self.cache }
    pub fn player_mut(&mut self) -> &mut MpvPlayer { &mut self.player }
}

impl Default for MiruBackend {
    fn default() -> Self { Self::new() }
}

fn fallback_anime() -> Vec<Anime> {
    vec![
        Anime { id: "1".into(), title: "One Piece".into(), description: Some("Gold Roger was known as the Pirate King...".into()), cover_image: Some("https://s4.anilist.co/file/anilistcdn/media/anime/cover/medium/bx21-ELSYx3y2QNT0.jpg".into()), banner_image: None, score: Some(87), year: Some(1999), episodes: None, status: Some("FINISHED".into()), genres: vec!["Adventure".into()] },
        Anime { id: "20".into(), title: "Naruto".into(), description: Some("Naruto Uzumaki wants to be Hokage...".into()), cover_image: None, banner_image: None, score: Some(79), year: Some(2002), episodes: Some(220), status: None, genres: vec![] },
        Anime { id: "16498".into(), title: "Attack on Titan".into(), description: None, cover_image: None, banner_image: None, score: Some(84), year: Some(2013), episodes: Some(25), status: None, genres: vec![] },
        Anime { id: "11061".into(), title: "Hunter x Hunter (2011)".into(), description: None, cover_image: None, banner_image: None, score: Some(89), year: Some(2011), episodes: Some(148), status: None, genres: vec![] },
        Anime { id: "1535".into(), title: "Death Note".into(), description: None, cover_image: None, banner_image: None, score: Some(84), year: Some(2006), episodes: Some(37), status: None, genres: vec![] },
        Anime { id: "21".into(), title: "One Piece (Continue)".into(), description: None, cover_image: None, banner_image: None, score: Some(87), year: Some(1999), episodes: Some(1000), status: None, genres: vec![] },
    ]
}
