use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};

use crate::backend::{Anime, MiruBackend};
use crate::frontend::services;
use crate::frontend::theme::{Theme, ThemeKind};
use crate::frontend::ui::carousel::CarouselState;
use crate::frontend::ui::hero::HeroState;

// re-export for `crate::frontend::app::AppView` compat (old import path)
pub use crate::frontend::state::{AppView, Focus};
#[allow(unused_imports)]
pub use crate::frontend::state::{AppView as AppViewState, Focus as FocusState};

/// Frontend state — only talks to `MiruBackend` interface, never scrapers directly.
pub struct App {
    pub running: bool,
    pub view: AppView,
    pub focus: Focus,
    pub active_carousel: usize,
    pub carousels: Vec<CarouselState>,
    pub hero: HeroState,
    pub search_query: String,
    pub status: String,
    pub backend: MiruBackend,
    pub theme: Theme,
    trending_rx: Option<tokio::sync::mpsc::UnboundedReceiver<Result<Vec<Anime>, String>>>,
}

impl App {
    pub async fn new() -> Self {
        let backend = MiruBackend::new();
        let carousels = vec![
            CarouselState::new("Continue Watching", vec![]),
            CarouselState::new("Trending Now", vec![]),
            CarouselState::new("Popular This Season", vec![]),
        ];
        Self {
            running: true, view: AppView::Browse, focus: Focus::Carousel,
            active_carousel: 1, carousels, hero: HeroState::default(),
            search_query: String::new(),
            status: "cozy • / search • q quit • ←→ move • enter open".to_string(),
            backend, theme: Theme::cozy(), trending_rx: None,
        }
    }

    pub fn cycle_theme(&mut self) {
        let next = match self.theme.kind {
            ThemeKind::Cozy => ThemeKind::Midnight,
            ThemeKind::Midnight => ThemeKind::Cozy,
            ThemeKind::Paper => ThemeKind::Cozy,
        };
        self.theme = Theme::from_kind(next);
        self.status = format!("theme → {}", self.theme.name());
    }

    // keep old name for main.rs compat
    pub async fn fetch_initial_data(&mut self) { self.spawn_initial_fetch(); self.poll_trending().await; }
    async fn poll_trending(&mut self) {}

    pub fn spawn_initial_fetch(&mut self) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.trending_rx = Some(rx);
        self.status = "brewing tea… loading trending".to_string();
        services::spawn_trending_into(tx);
    }

    fn poll_trending_sync(&mut self) {
        if let Some(rx) = &mut self.trending_rx {
            if let Ok(Ok(list)) = rx.try_recv() {
                if let Some(first) = list.first().cloned() {
                    self.hero = HeroState::from_anime(&first);
                    self.backend.cache.prefetch(first.cover_image.as_deref().unwrap_or("").to_string());
                }
                if let Some(row) = self.carousels.get_mut(1) { row.items = list.clone(); }
                if let Some(row) = self.carousels.get_mut(2) { row.items = list; }
                self.status = "cozy & ready — h/l scroll • j/k switch".to_string();
                self.trending_rx = None;
            }
        }
    }

    pub async fn on_key(&mut self, key: KeyEvent) {
        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Char('q'), _) => { self.running = false; return; }
            _ => {}
        }
        match self.view {
            AppView::Browse => self.on_key_browse(key).await,
            AppView::Detail => self.on_key_detail(key).await,
            AppView::Search => self.on_key_search(key).await,
            AppView::Player => self.on_key_player(key).await,
        }
    }

    async fn on_key_browse(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('/') => { self.view = AppView::Search; self.focus = Focus::SearchInput; self.status = "⌕ search — enter to go • esc to stay cozy".to_string(); }
            KeyCode::Esc => { self.focus = Focus::Carousel; }
            KeyCode::Up | KeyCode::Char('k') => { if self.active_carousel > 0 { self.active_carousel -= 1; self.focus = Focus::Carousel; } }
            KeyCode::Down | KeyCode::Char('j') => { if self.active_carousel + 1 < self.carousels.len() { self.active_carousel += 1; self.focus = Focus::Carousel; } }
            KeyCode::Left | KeyCode::Char('h') => { if let Some(c) = self.carousels.get_mut(self.active_carousel) { c.scroll_left(); self.sync_hero(); } }
            KeyCode::Right | KeyCode::Char('l') => { if let Some(c) = self.carousels.get_mut(self.active_carousel) { c.scroll_right(); self.sync_hero(); } }
            KeyCode::Enter => { self.view = AppView::Detail; self.status = "details — esc to go back • p to play ♡".to_string(); }
            KeyCode::Char('p') => { self.play_selected().await; }
            KeyCode::Char('t') => { self.cycle_theme(); }
            _ => {}
        }
    }
    async fn on_key_detail(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => { self.view = AppView::Browse; self.status = "cozy & ready".to_string(); }
            KeyCode::Char('p') => self.play_selected().await,
            _ => {}
        }
    }
    async fn on_key_search(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => { self.view = AppView::Browse; self.focus = Focus::Carousel; self.search_query.clear(); }
            KeyCode::Enter => {
                let q = self.search_query.clone();
                if q.trim().is_empty() { return; }
                self.status = format!(" Searching \"{q}\"… ");
                match self.backend.search(&q, 10).await {
                    Ok(results) => {
                        if let Some(row) = self.carousels.get_mut(1) { row.items = results; row.offset = 0; row.selected = 0; }
                        self.sync_hero(); self.view = AppView::Browse; self.focus = Focus::Carousel;
                        self.status = format!(" Found {} results for \"{q}\" ", self.carousels[1].items.len());
                    }
                    Err(e) => self.status = format!(" Search failed: {e:.60} "),
                }
            }
            KeyCode::Backspace => { self.search_query.pop(); }
            KeyCode::Char(c) => { self.search_query.push(c); }
            _ => {}
        }
    }
    async fn on_key_player(&mut self, key: KeyEvent) {
        match key.code { KeyCode::Esc | KeyCode::Char('q') => { let _ = self.backend.player_mut().stop().await; self.view = AppView::Browse; } _ => {} }
    }
    pub async fn on_mouse(&mut self, event: MouseEvent) {
        use crossterm::event::MouseEventKind;
        if self.view != AppView::Browse { return; }
        match event.kind {
            MouseEventKind::ScrollDown => { if let Some(c) = self.carousels.get_mut(self.active_carousel) { c.scroll_right(); self.sync_hero(); } }
            MouseEventKind::ScrollUp => { if let Some(c) = self.carousels.get_mut(self.active_carousel) { c.scroll_left(); self.sync_hero(); } }
            _ => {}
        }
    }
    pub async fn on_tick(&mut self) { let _ = self.backend.player_mut().poll().await; self.poll_trending_sync(); }
    fn sync_hero(&mut self) {
        if let Some(c) = self.carousels.get(self.active_carousel) {
            if let Some(anime) = c.selected_item() {
                self.hero = HeroState::from_anime(anime);
                if let Some(u) = &self.hero.poster_url { self.backend.cache.prefetch(u.clone()); }
            }
        }
    }
    async fn play_selected(&mut self) {
        let Some(anime) = self.carousels.get(self.active_carousel).and_then(|c| c.selected_item().cloned()) else {
            self.status = " Nothing selected ".to_string(); return;
        };
        self.status = format!(" Resolving streams for {}… ", anime.title);
        match self.backend.resolve(&anime).await {
            Ok(src) if !src.is_empty() => {
                let url = src[0].url.clone();
                match self.backend.player_mut().play(&url, &anime.title).await {
                    Ok(()) => { self.view = AppView::Player; self.status = " Playing — Esc to stop ".to_string(); }
                    Err(e) => self.status = format!(" mpv failed: {e:.60} "),
                }
            }
            Ok(_) => self.status = " No streams found ".to_string(),
            Err(e) => self.status = format!(" Resolve failed: {e:.60} "),
        }
    }
    pub fn quit(&mut self) { self.running = false; }
    // cozy compat for ui: `app.cache` used in ui, expose via method
    pub fn cache(&self) -> &crate::backend::ImageCache { &self.backend.cache }
}
