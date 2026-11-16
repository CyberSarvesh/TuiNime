use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::backend::{Anime, MiruBackend};
use crate::frontend::services::{self, BgEvent};
use crate::frontend::theme::Theme;
use crate::frontend::ui::carousel::CarouselState;
use crate::frontend::ui::hero::HeroState;
use crate::frontend::widgets::poster::PosterRenderer;

// re-export for `crate::frontend::app::AppView` compat (old import path)
pub use crate::frontend::state::AppView;
use crate::frontend::state::Status;

pub const ROW_CONTINUE: usize = 0;
pub const ROW_TRENDING: usize = 1;
pub const ROW_SEASON: usize = 2;
const TRENDING_TITLE: &str = "Trending Now";

/// Frontend state — only talks to `MiruBackend` interface, never scrapers directly.
pub struct App {
    pub running: bool,
    pub view: AppView,
    pub active_carousel: usize,
    pub carousels: Vec<CarouselState>,
    pub hero: HeroState,
    pub search_query: String,
    pub status: Status,
    /// Frame counter driving spinners; advanced on every tick.
    pub tick: u64,
    pub backend: MiruBackend,
    pub theme: Theme,
    pub posters: PosterRenderer,
    /// Title of what mpv is currently playing (shown in the player overlay).
    pub now_playing: Option<String>,
    /// Trending list kept aside while search results occupy its row (Esc restores it).
    trending: Vec<Anime>,
    /// Show whose streams are being looked up, so repeated `p` presses don't stack requests.
    resolving: Option<String>,
    /// Incremented per search so a slow, older search can't overwrite a newer one.
    search_seq: u64,
    bg_tx: UnboundedSender<BgEvent>,
    bg_rx: UnboundedReceiver<BgEvent>,
}

impl App {
    pub fn new(posters: PosterRenderer) -> Self {
        let backend = MiruBackend::new();
        let mut carousels = vec![
            CarouselState::new("Continue Watching", vec![]),
            CarouselState::new(TRENDING_TITLE, vec![]),
            CarouselState::new("Popular This Season", vec![]),
        ];
        carousels[ROW_TRENDING].loading = true;
        carousels[ROW_SEASON].loading = true;
        let (bg_tx, bg_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            running: true, view: AppView::Browse,
            active_carousel: ROW_TRENDING, carousels, hero: HeroState::default(),
            search_query: String::new(),
            status: Status::info("welcome"), tick: 0,
            backend, theme: Theme::cozy(), posters, now_playing: None,
            trending: Vec::new(), resolving: None, search_seq: 0, bg_tx, bg_rx,
        }
    }

    pub fn cycle_theme(&mut self) {
        let kinds = Theme::all_kinds();
        let i = kinds.iter().position(|k| *k == self.theme.kind).unwrap_or(0);
        let next = kinds[(i + 1) % kinds.len()];
        self.theme = Theme::from_kind(next);
        self.status = Status::info(format!("theme → {}", self.theme.name()));
    }

    pub fn spawn_initial_fetch(&mut self) {
        self.status = Status::busy("loading trending…");
        services::spawn_trending(self.backend.catalog(), self.bg_tx.clone());
        services::spawn_seasonal(self.backend.catalog(), self.bg_tx.clone());
    }

    /// Wait for the next background result. Never returns `None` while `App` holds a sender.
    pub async fn next_background(&mut self) -> Option<BgEvent> {
        self.bg_rx.recv().await
    }

    pub async fn on_background(&mut self, ev: BgEvent) {
        match ev {
            BgEvent::Trending(list) => {
                self.trending = list.clone();
                if let Some(row) = self.carousels.get_mut(ROW_TRENDING) {
                    // don't clobber search results that arrived first
                    if row.title == TRENDING_TITLE { row.set_items(list); }
                }
                self.ensure_active_visible();
                self.sync_hero();
                self.status = Status::info("ready");
            }
            BgEvent::Seasonal { label, result } => {
                if let Some(row) = self.carousels.get_mut(ROW_SEASON) {
                    match result {
                        Ok(list) => { row.title = format!("Popular This Season ({label})"); row.set_items(list); }
                        Err(e) => {
                            tracing::warn!("seasonal fetch failed: {e}");
                            row.set_items(Vec::new()); // stop the spinner; the empty row hides
                        }
                    }
                }
                self.ensure_active_visible();
            }
            BgEvent::Search { seq, query, result } => {
                if seq != self.search_seq { return; } // superseded by a newer search
                match result {
                    Ok(results) => {
                        let n = results.len();
                        if let Some(row) = self.carousels.get_mut(ROW_TRENDING) {
                            row.title = format!("Results for \"{query}\"");
                            row.set_items(results);
                        }
                        self.active_carousel = ROW_TRENDING;
                        self.sync_hero();
                        self.status = Status::info(format!("found {n} results for \"{query}\" — esc for trending"));
                    }
                    Err(e) => self.status = Status::error(format!("search failed: {}", short(&e))),
                }
            }
            BgEvent::Resolved { anime, result } => {
                self.resolving = None;
                match result {
                    Ok(src) if !src.is_empty() => {
                        // sources arrive sorted best-quality first
                        match self.backend.player_mut().play(&src[0], &anime.title).await {
                            Ok(()) => {
                                self.view = AppView::Player;
                                self.now_playing = Some(anime.title.clone());
                                self.status = Status::info(format!("playing {}", anime.title));
                                self.add_continue_watching(anime);
                            }
                            Err(e) => self.status = Status::error(format!("mpv failed: {}", short(&format!("{e:#}")))),
                        }
                    }
                    Ok(_) => self.status = Status::error(format!("no streams found for {}", anime.title)),
                    Err(e) => self.status = Status::error(format!("couldn't get streams: {}", short(&e))),
                }
            }
        }
    }

    fn add_continue_watching(&mut self, anime: Anime) {
        if let Some(row) = self.carousels.get_mut(ROW_CONTINUE) {
            let mut items = std::mem::take(&mut row.items);
            items.retain(|a| a.id != anime.id);
            items.insert(0, anime);
            row.set_items(items);
        }
    }

    pub async fn on_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.running = false;
            return;
        }
        match self.view {
            AppView::Browse => self.on_key_browse(key).await,
            AppView::Detail => self.on_key_detail(key).await,
            AppView::Search => self.on_key_search(key),
            AppView::Player => self.on_key_player(key).await,
        }
    }

    async fn on_key_browse(&mut self, key: KeyEvent) {
        match key.code {
            // `q` quits only from browse — in search it is a letter, in overlays it means "back"
            KeyCode::Char('q') => { self.running = false; }
            KeyCode::Char('/') => self.view = AppView::Search,
            KeyCode::Esc => self.restore_trending(),
            KeyCode::Up | KeyCode::Char('k') => self.move_row(false),
            KeyCode::Down | KeyCode::Char('j') => self.move_row(true),
            KeyCode::Left | KeyCode::Char('h') => self.scroll(false),
            KeyCode::Right | KeyCode::Char('l') => self.scroll(true),
            KeyCode::Enter => {
                if self.selected().is_some() {
                    self.view = AppView::Detail;
                }
            }
            KeyCode::Char('p') => self.play_selected(),
            KeyCode::Char('t') => self.cycle_theme(),
            _ => {}
        }
    }

    async fn on_key_detail(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.view = AppView::Browse,
            KeyCode::Char('p') => self.play_selected(),
            _ => {}
        }
    }

    fn on_key_search(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => { self.view = AppView::Browse; self.search_query.clear(); }
            KeyCode::Enter => {
                let q = self.search_query.trim().to_string();
                if q.is_empty() { return; }
                self.search_seq += 1;
                services::spawn_search(self.backend.catalog(), self.bg_tx.clone(), self.search_seq, q.clone());
                self.status = Status::busy(format!("searching \"{q}\"…"));
                self.search_query.clear();
                self.view = AppView::Browse;
            }
            KeyCode::Backspace => { self.search_query.pop(); }
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                self.search_query.push(c);
            }
            _ => {}
        }
    }

    async fn on_key_player(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
            let _ = self.backend.player_mut().stop().await;
            self.now_playing = None;
            self.view = AppView::Browse;
            self.status = Status::info("stopped");
        }
    }

    pub async fn on_mouse(&mut self, event: MouseEvent) {
        use crossterm::event::MouseEventKind;
        if self.view != AppView::Browse { return; }
        match event.kind {
            MouseEventKind::ScrollDown => self.scroll(true),
            MouseEventKind::ScrollUp => self.scroll(false),
            _ => {}
        }
    }

    pub async fn on_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        // mpv closed on its own (window closed / episode ended) → leave the player overlay
        if let Ok(true) = self.backend.player_mut().poll().await {
            if self.view == AppView::Player {
                self.view = AppView::Browse;
                self.status = Status::info("playback finished");
            }
            self.now_playing = None;
        }
    }

    fn scroll(&mut self, right: bool) {
        if let Some(c) = self.carousels.get_mut(self.active_carousel) {
            if right { c.scroll_right() } else { c.scroll_left() }
        }
        self.sync_hero();
    }

    fn restore_trending(&mut self) {
        if let Some(row) = self.carousels.get_mut(ROW_TRENDING) {
            if row.title != TRENDING_TITLE {
                row.title = TRENDING_TITLE.to_string();
                row.set_items(self.trending.clone());
                self.sync_hero();
                self.status = Status::info("back to trending");
            }
        }
    }

    /// Rows worth drawing: anything with items or still loading. "Continue Watching" only
    /// appears once something has been played.
    pub fn visible_rows(&self) -> Vec<usize> {
        (0..self.carousels.len())
            .filter(|&i| {
                let c = &self.carousels[i];
                !c.items.is_empty() || (c.loading && i != ROW_CONTINUE)
            })
            .collect()
    }

    /// Move to the next/previous visible row.
    fn move_row(&mut self, down: bool) {
        let rows = self.visible_rows();
        let Some(pos) = rows.iter().position(|r| *r == self.active_carousel) else {
            self.ensure_active_visible();
            return;
        };
        let next = if down { rows.get(pos + 1) } else { pos.checked_sub(1).and_then(|p| rows.get(p)) };
        if let Some(&r) = next {
            self.active_carousel = r;
            self.sync_hero();
        }
    }

    /// If the active row was hidden (e.g. it came back empty), jump to the first visible one.
    fn ensure_active_visible(&mut self) {
        let rows = self.visible_rows();
        if !rows.contains(&self.active_carousel) {
            if let Some(&r) = rows.first() {
                self.active_carousel = r;
                self.sync_hero();
            }
        }
    }

    fn selected(&self) -> Option<&Anime> {
        self.carousels.get(self.active_carousel).and_then(|c| c.selected_item())
    }

    fn sync_hero(&mut self) {
        if let Some(anime) = self.selected() {
            self.hero = HeroState::from_anime(anime);
            if let Some(u) = self.hero.poster_url() { self.backend.cache.prefetch(u.to_string()); }
        }
    }

    fn play_selected(&mut self) {
        let Some(anime) = self.selected().cloned() else {
            self.status = Status::error("nothing selected");
            return;
        };
        if let Some(t) = &self.resolving {
            self.status = Status::busy(format!("still looking up streams for {t}…"));
            return;
        }
        self.status = Status::busy(format!("looking up streams for {}…", anime.title));
        self.resolving = Some(anime.title.clone());
        services::spawn_resolve(self.backend.catalog(), self.bg_tx.clone(), anime);
    }
}

/// Keep status-bar errors to one readable line.
fn short(s: &str) -> String {
    let line = s.lines().next().unwrap_or("");
    if line.chars().count() > 90 { format!("{}…", line.chars().take(89).collect::<String>()) } else { line.to_string() }
}
