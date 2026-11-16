use anyhow::Result;

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

    /// ani-cli style: search each provider by title → pick the best-matching show →
    /// episode list → stream links for episode 1.
    ///
    /// AniList ids mean nothing to stream providers, so we only search by name. The English
    /// title is tried first, then the alternate (romaji/native) titles, because providers
    /// like AllAnime usually list shows under romaji.
    pub async fn resolve_for_anime(&self, anime: &Anime) -> Result<Vec<StreamSource>> {
        let mut names: Vec<&str> = vec![anime.title.as_str()];
        names.extend(anime.alt_titles.iter().map(String::as_str));
        names.dedup();

        let mut last_err: Option<anyhow::Error> = None;
        for provider in &self.stream_providers {
            let mut chosen: Option<Anime> = None;
            let mut first_hit: Option<Anime> = None;
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
                    Err(e) => last_err = Some(e),
                }
            }
            let Some(show) = chosen.or(first_hit) else { continue };

            let eps = match provider.episodes(&show.id).await {
                Ok(eps) if !eps.is_empty() => eps,
                Ok(_) => {
                    last_err = Some(anyhow::anyhow!("no episodes listed for {}", show.title));
                    continue;
                }
                Err(e) => {
                    last_err = Some(e);
                    continue;
                }
            };
            match provider.streams(&eps[0].id).await {
                Ok(src) if !src.is_empty() => return Ok(src),
                Ok(_) => last_err = Some(anyhow::anyhow!("no streams for {}", show.title)),
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("no stream provider found {}", anime.title)))
    }

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
