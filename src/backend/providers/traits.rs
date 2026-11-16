use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Core anime metadata used throughout the TUI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Anime {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub cover_image: Option<String>,
    pub banner_image: Option<String>,
    pub score: Option<u8>,
    pub year: Option<u16>,
    pub episodes: Option<u32>,
    pub status: Option<String>,
    pub genres: Vec<String>,
    /// Other names for the show (e.g. romaji / native). Stream providers often list a show
    /// under its romaji title, so resolution searches these too.
    #[serde(default)]
    pub alt_titles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub id: String,
    pub number: u32,
    /// The provider's own episode string, e.g. "12" or "12.5" for a recap.
    pub label: String,
    pub title: Option<String>,
    pub thumbnail: Option<String>,
}

/// Resolved playable source for an episode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamSource {
    /// Direct media URL (HLS/DASH/MP4)
    pub url: String,
    pub quality: Option<String>,
    pub headers: Option<std::collections::HashMap<String, String>>,
    pub is_m3u8: bool,
    /// External subtitle files (e.g. `.vtt`), preferred track first.
    #[serde(default)]
    pub subtitles: Vec<String>,
}

/// Provider that can search and resolve streams.
#[async_trait]
pub trait AnimeProvider: Send + Sync {
    async fn search(&self, query: &str, limit: u8) -> Result<Vec<Anime>>;
    async fn episodes(&self, anime_id: &str) -> Result<Vec<Episode>>;
    async fn streams(&self, episode_id: &str) -> Result<Vec<StreamSource>>;
    /// Switch between subtitled and dubbed listings (providers without the concept ignore it).
    fn set_dub(&self, _dub: bool) {}
}

/// Provider that supplies metadata (AniList, MAL, etc.).
#[async_trait]
pub trait MetadataProvider: Send + Sync {
    async fn trending(&self, limit: u8) -> Result<Vec<Anime>>;
    async fn seasonal(&self, year: u16, season: &str, limit: u8) -> Result<Vec<Anime>>;
    async fn search(&self, query: &str, limit: u8) -> Result<Vec<Anime>>;
}
