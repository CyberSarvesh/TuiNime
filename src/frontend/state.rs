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
