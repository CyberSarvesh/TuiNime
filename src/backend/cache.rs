use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result};
use bytes::Bytes;
use tokio::sync::Mutex;

/// In-memory + on-disk async image cache for poster thumbnails.
///
/// Posters are fetched from AniList `coverImage.extraLarge` (e.g. https://s4.anilist.co/…) via `reqwest`,
/// stored under `cache_dir` keyed by hash of URL, and kept in memory for the session.
/// Mirrors browser-style caching so the cozy TUI stays snappy offline.
///
/// Rendering uses `ratatui-image` — Kitty/Sixel if terminal supports it, else unicode halfblocks.
/// This cache only handles fetching/decoding; `ui::hero` / `ui::carousel` pick the protocol.
#[derive(Clone)]
pub struct ImageCache {
    client: reqwest::Client,
    cache_dir: PathBuf,
    memory: Arc<Mutex<HashMap<String, Bytes>>>,
    decoded: Arc<Mutex<HashMap<String, image::DynamicImage>>>,
}

impl ImageCache {
    pub fn new() -> Result<Self> {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| std::env::temp_dir().join("tuinime"))
            .join("tuinime")
            .join("posters");
        Self::with_dir(cache_dir)
    }

    pub fn new_temp() -> Self {
        let cache_dir = std::env::temp_dir().join(format!("tuinime-{}", uuid::Uuid::new_v4()));
        Self::with_dir(cache_dir).expect("temp cache dir must be creatable")
    }

    pub fn with_dir(cache_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&cache_dir).context("create poster cache dir")?;
        Ok(Self {
            client: crate::backend::providers::http_client("tuinime/0.1"),
            cache_dir,
            memory: Arc::new(Mutex::new(HashMap::new())),
            decoded: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    fn key_for(url: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        url.hash(&mut h);
        format!("{:016x}", h.finish())
    }

    fn path_for(&self, url: &str) -> PathBuf {
        let key = Self::key_for(url);
        let ext = Path::new(url)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("jpg");
        let ext = if ext.len() <= 4 { ext } else { "jpg" };
        // strip query string from ext
        let ext = ext.split('?').next().unwrap_or("jpg");
        self.cache_dir.join(format!("{key}.{ext}"))
    }

    pub fn is_cached(&self, url: &str) -> bool {
        self.path_for(url).exists()
    }

    /// Return cached bytes if present in memory or on disk, otherwise fetch, store and return.
    pub async fn get_or_fetch(&self, url: &str) -> Result<Bytes> {
        {
            let mem = self.memory.lock().await;
            if let Some(b) = mem.get(url) {
                return Ok(b.clone());
            }
        }
        let path = self.path_for(url);
        if path.exists() {
            if let Ok(data) = tokio::fs::read(&path).await {
                let bytes = Bytes::from(data);
                self.memory.lock().await.insert(url.to_string(), bytes.clone());
                return Ok(bytes);
            }
        }
        let bytes = self.download(url).await?;
        // write to a temp file then rename, so the render thread never decodes a half-written poster
        let tmp = path.with_extension(format!("part-{}", uuid::Uuid::new_v4()));
        if tokio::fs::write(&tmp, &bytes).await.is_ok() && tokio::fs::rename(&tmp, &path).await.is_err() {
            let _ = tokio::fs::remove_file(&tmp).await;
        }
        self.memory.lock().await.insert(url.to_string(), bytes.clone());
        Ok(bytes)
    }

    /// Try to load from disk synchronously (used in render without blocking async).
    /// Returns `None` if not yet cached — caller shows cozy placeholder.
    /// Caches decoded result in `decoded` so per-frame render doesn't re-decode.
    pub fn try_load_cached_image(&self, url: &str) -> Option<image::DynamicImage> {
        // fast path: decoded memory
        if let Ok(m) = self.decoded.try_lock() {
            if let Some(img) = m.get(url) { return Some(img.clone()); }
        }
        let path = self.path_for(url);
        if !path.exists() { return None; }
        let data = std::fs::read(&path).ok()?;
        let img = image::load_from_memory(&data).ok()?;
        // store decoded (best effort, ignore if lock poisoned)
        if let Ok(mut m) = self.decoded.try_lock() {
            m.insert(url.to_string(), img.clone());
            // cozy eviction: keep last 32
            if m.len() > 32 { m.clear(); }
        }
        Some(img)
    }

    async fn download(&self, url: &str) -> Result<Bytes> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .with_context(|| format!("fetch poster {url}"))?;
        if !resp.status().is_success() {
            anyhow::bail!("poster http {}", resp.status());
        }
        Ok(resp.bytes().await?)
    }

    /// Fire-and-forget prefetch; errors are traced.
    /// Call this for every `cover_image` from AniList so the cozy UI warms up.
    pub fn prefetch(&self, url: String) {
        let this = self.clone();
        tokio::spawn(async move {
            if let Err(e) = this.get_or_fetch(&url).await {
                tracing::debug!("prefetch failed for {url}: {e:#}");
            } else {
                tracing::trace!("prefetched poster {url}");
            }
        });
    }

    /// Prefetch many at once (hero + visible carousel).
    pub fn prefetch_many(&self, urls: Vec<String>) {
        for u in urls {
            if !u.is_empty() { self.prefetch(u); }
        }
    }

}
