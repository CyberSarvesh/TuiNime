//! Backend — all scraping, caching, playback. Frontend only sees this interface.
pub mod cache;
pub mod player;
pub mod providers;

pub use cache::ImageCache;
pub use player::MpvPlayer;
pub use providers::traits::{Anime, Episode, StreamSource};
pub use providers::aggregator::{EpisodeList, ProviderAggregator};
pub use providers::anilist::AniListClient;
pub use providers::allanime::AllAnimeProvider;
pub use providers::hianime::HiAnimeProvider;
pub use providers::scraper::ScraperProvider;

use std::sync::Arc;

use anyhow::Result;

/// Clean interface that frontend consumes. No scraper details leak.
pub struct MiruBackend {
    pub cache: ImageCache,
    pub player: MpvPlayer,
    catalog: Catalog,
}

/// Cheap-to-clone handle for the network side (metadata + stream resolution), so the UI can
/// run searches/resolves on background tasks instead of blocking the render loop.
#[derive(Clone)]
pub struct Catalog {
    cache: ImageCache,
    aggregator: Arc<ProviderAggregator<AniListClient>>,
}

impl MiruBackend {
    pub fn new() -> Self {
        let cache = ImageCache::new().unwrap_or_else(|e| {
            tracing::warn!("cache init failed: {e}; temp fallback");
            ImageCache::new_temp()
        });
        let mut aggregator = ProviderAggregator::new(AniListClient::new());
        // tried in this order: hianime.at (what ani-cli uses today), then AllAnime
        aggregator.add_provider(HiAnimeProvider::new());
        aggregator.add_provider(AllAnimeProvider::new());
        // The public Consumet instance is dead; only use one the user runs themselves.
        if std::env::var_os("MIRU_CONSUMET_URL").is_some() {
            aggregator.add_provider(ScraperProvider::new());
        }
        let catalog = Catalog { cache: cache.clone(), aggregator: Arc::new(aggregator) };
        Self { cache, player: MpvPlayer::new(), catalog }
    }

    pub fn catalog(&self) -> Catalog { self.catalog.clone() }
    pub fn player_mut(&mut self) -> &mut MpvPlayer { &mut self.player }
}

impl Catalog {
    fn warm(&self, list: &[Anime]) {
        let urls: Vec<String> = list.iter().filter_map(|a| a.cover_image.clone()).collect();
        self.cache.prefetch_many(urls);
    }

    // --- metadata (AniList) with fallback so UI never shows "nothing here yet" ---
    pub async fn trending(&self, limit: u8) -> Vec<Anime> {
        match self.aggregator.trending(limit).await {
            Ok(list) if !list.is_empty() => {
                self.warm(&list);
                list
            }
            Ok(_) | Err(_) => {
                tracing::warn!("trending offline, using fallback");
                fallback_anime()
            }
        }
    }

    pub async fn seasonal(&self, year: u16, season: &str, limit: u8) -> Result<Vec<Anime>> {
        let list = self.aggregator.seasonal(year, season, limit).await?;
        self.warm(&list);
        Ok(list)
    }

    pub async fn search(&self, q: &str, limit: u8) -> Result<Vec<Anime>> {
        let res = self.aggregator.search(q, limit).await?;
        self.warm(&res);
        Ok(res)
    }

    pub async fn resolve(&self, anime: &Anime) -> Result<Vec<StreamSource>> {
        self.aggregator.resolve_for_anime(anime).await
    }

    pub async fn episodes(&self, anime: &Anime) -> Result<EpisodeList> {
        self.aggregator.episodes_for(anime).await
    }

    pub async fn streams(&self, provider: usize, episode_id: &str) -> Result<Vec<StreamSource>> {
        self.aggregator.streams_for(provider, episode_id).await
    }

    /// Subtitled (`false`) or dubbed (`true`) listings from now on.
    pub fn set_dub(&self, dub: bool) {
        self.aggregator.set_dub(dub);
    }
}

impl Default for MiruBackend {
    fn default() -> Self { Self::new() }
}

fn fallback_anime() -> Vec<Anime> {
    vec![
        Anime { id: "1".into(), title: "One Piece".into(), description: Some("Gold Roger was known as the Pirate King...".into()), cover_image: Some("https://s4.anilist.co/file/anilistcdn/media/anime/cover/medium/bx21-ELSYx3y2QNT0.jpg".into()), banner_image: None, score: Some(87), year: Some(1999), episodes: None, status: Some("FINISHED".into()), genres: vec!["Adventure".into()], alt_titles: vec![] },
        Anime { id: "20".into(), title: "Naruto".into(), description: Some("Naruto Uzumaki wants to be Hokage...".into()), cover_image: None, banner_image: None, score: Some(79), year: Some(2002), episodes: Some(220), status: None, genres: vec![], alt_titles: vec![] },
        Anime { id: "16498".into(), title: "Attack on Titan".into(), description: None, cover_image: None, banner_image: None, score: Some(84), year: Some(2013), episodes: Some(25), status: None, genres: vec![], alt_titles: vec![] },
        Anime { id: "11061".into(), title: "Hunter x Hunter (2011)".into(), description: None, cover_image: None, banner_image: None, score: Some(89), year: Some(2011), episodes: Some(148), status: None, genres: vec![], alt_titles: vec![] },
        Anime { id: "1535".into(), title: "Death Note".into(), description: None, cover_image: None, banner_image: None, score: Some(84), year: Some(2006), episodes: Some(37), status: None, genres: vec![], alt_titles: vec![] },
        Anime { id: "21".into(), title: "One Piece (Continue)".into(), description: None, cover_image: None, banner_image: None, score: Some(87), year: Some(1999), episodes: Some(1000), status: None, genres: vec![], alt_titles: vec![] },
    ]
}
