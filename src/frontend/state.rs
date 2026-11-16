//! App state — pure data, no IO. `App` in `app.rs` composes this.

use std::cell::Cell;

use crate::backend::{Anime, EpisodeList};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppView {
    #[default]
    Browse,
    Detail,
    Search,
    Player,
}

/// Something on screen that a mouse click can select.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitTarget {
    /// Card `index` in carousel `row` (browse).
    Card { row: usize, index: usize },
    /// Row `n` of the search results.
    SearchResult(usize),
    /// Episode `n` in the details grid.
    Episode(usize),
}

/// Clickable areas from the last frame; rebuilt on every draw.
pub type Hits = Cell<Vec<(ratatui::layout::Rect, HitTarget)>>;

/// Record a clickable area for this frame.
pub fn add_hit(hits: &Hits, rect: ratatui::layout::Rect, target: HitTarget) {
    let mut v = hits.take();
    v.push((rect, target));
    hits.set(v);
}

/// How the status bar should present a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    /// Plain information.
    Info,
    /// Work in progress — drawn with a spinner.
    Busy,
    /// Something failed — drawn in the error color.
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub text: String,
    pub kind: StatusKind,
}

impl Status {
    pub fn info(text: impl Into<String>) -> Self { Self { text: text.into(), kind: StatusKind::Info } }
    pub fn busy(text: impl Into<String>) -> Self { Self { text: text.into(), kind: StatusKind::Busy } }
    pub fn error(text: impl Into<String>) -> Self { Self { text: text.into(), kind: StatusKind::Error } }
}

/// Braille spinner frames, advanced once per tick.
pub const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn spinner_frame(tick: u64) -> &'static str {
    SPINNER[(tick % SPINNER.len() as u64) as usize]
}

/// Episode list on the details screen.
#[derive(Debug, Clone)]
pub enum Episodes {
    Loading,
    Loaded(EpisodeList),
    Failed(String),
}

/// Everything the details screen shows and remembers.
#[derive(Debug)]
pub struct DetailState {
    pub anime: Anime,
    pub episodes: Episodes,
    /// Index of the highlighted episode.
    pub selected: usize,
    /// Lines the description is scrolled down by.
    pub desc_scroll: u16,
    /// Furthest the description can scroll; written by the renderer, which knows the width.
    pub desc_max: Cell<u16>,
    /// Episodes per row in the grid; written by the renderer, read by up/down.
    pub grid_cols: Cell<usize>,
}

impl DetailState {
    pub fn new(anime: Anime) -> Self {
        Self { anime, episodes: Episodes::Loading, selected: 0, desc_scroll: 0, desc_max: Cell::new(0), grid_cols: Cell::new(1) }
    }

    pub fn episode_count(&self) -> usize {
        match &self.episodes {
            Episodes::Loaded(list) => list.episodes.len(),
            _ => 0,
        }
    }

    /// Move the highlight by `delta` episodes, staying in range.
    pub fn move_by(&mut self, delta: isize) {
        let n = self.episode_count();
        if n == 0 {
            return;
        }
        self.selected = (self.selected as isize + delta).clamp(0, n as isize - 1) as usize;
    }
}

/// What the search screen's result list is showing.
#[derive(Debug, Clone)]
pub enum SearchResults {
    /// Nothing searched yet (recent searches are shown instead).
    Idle,
    Loading,
    Loaded(Vec<Anime>),
    Failed(String),
}

/// How long typing must pause before a live search is sent.
pub const SEARCH_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(350);
/// Most recent searches remembered.
pub const MAX_RECENT: usize = 8;

#[derive(Debug)]
pub struct SearchState {
    pub query: String,
    pub results: SearchResults,
    /// Highlighted row: a result, or a recent search while the box is empty.
    pub selected: usize,
    /// When the pending live search should fire (reset on every keystroke).
    pub due: Option<std::time::Instant>,
    /// Query of the last search sent, so the same text isn't searched twice.
    pub sent: String,
    /// Incremented per search so a slow, older search can't overwrite a newer one.
    pub seq: u64,
}

impl Default for SearchState {
    fn default() -> Self {
        Self { query: String::new(), results: SearchResults::Idle, selected: 0, due: None, sent: String::new(), seq: 0 }
    }
}

impl SearchState {
    /// Rows the list currently has (results, or recents when idle).
    pub fn row_count(&self, recent: &[String]) -> usize {
        match &self.results {
            SearchResults::Loaded(r) => r.len(),
            SearchResults::Idle => recent.len(),
            _ => 0,
        }
    }

    pub fn move_by(&mut self, delta: isize, recent: &[String]) {
        let n = self.row_count(recent);
        self.selected = if n == 0 { 0 } else { (self.selected as isize + delta).clamp(0, n as isize - 1) as usize };
    }

    /// Text changed: schedule a live search and go back to recents if the box is empty.
    pub fn edited(&mut self, now: std::time::Instant) {
        self.selected = 0;
        if self.query.trim().is_empty() {
            self.results = SearchResults::Idle;
            self.due = None;
            self.sent.clear();
        } else {
            self.due = Some(now + SEARCH_DEBOUNCE);
        }
    }
}

/// Put `query` at the front of `recent`, without duplicates, keeping at most `MAX_RECENT`.
pub fn remember_search(recent: &mut Vec<String>, query: &str) {
    let q = query.trim();
    if q.is_empty() {
        return;
    }
    recent.retain(|r| !r.eq_ignore_ascii_case(q));
    recent.insert(0, q.to_string());
    recent.truncate(MAX_RECENT);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_searches_dedupe_and_cap() {
        let mut r = Vec::new();
        for q in ["naruto", "bleach", "Naruto", "  ", "a", "b", "c", "d", "e", "f", "g"] {
            remember_search(&mut r, q);
        }
        assert_eq!(r.len(), MAX_RECENT);
        assert_eq!(r[0], "g");
        assert_eq!(r.iter().filter(|q| q.eq_ignore_ascii_case("naruto")).count(), 1);
    }

    #[test]
    fn editing_schedules_or_clears() {
        let now = std::time::Instant::now();
        let mut s = SearchState { query: "fri".into(), ..Default::default() };
        s.edited(now);
        assert_eq!(s.due, Some(now + SEARCH_DEBOUNCE));
        s.query.clear();
        s.edited(now);
        assert!(s.due.is_none() && matches!(s.results, SearchResults::Idle));
    }
}
