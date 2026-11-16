use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::backend::{Anime, MiruBackend};
use crate::frontend::persist::{self, Saved};
use crate::frontend::services::{self, BgEvent};
use crate::frontend::theme::Theme;
use crate::frontend::ui::carousel::CarouselState;
use crate::frontend::ui::hero::HeroState;
use crate::frontend::widgets::poster::PosterRenderer;

// re-export for `crate::frontend::app::AppView` compat (old import path)
pub use crate::frontend::state::AppView;
use crate::frontend::state::{remember_search, DetailState, Episodes, HitTarget, Hits, SearchResults, SearchState, Status};

pub const ROW_CONTINUE: usize = 0;
pub const ROW_TRENDING: usize = 1;
pub const ROW_SEASON: usize = 2;

/// Frontend state — only talks to `MiruBackend` interface, never scrapers directly.
pub struct App {
    pub running: bool,
    pub view: AppView,
    pub active_carousel: usize,
    pub carousels: Vec<CarouselState>,
    pub hero: HeroState,
    pub search: SearchState,
    /// Newest first; shown on the search screen while the box is empty.
    pub recent_searches: Vec<String>,
    pub status: Status,
    /// Frame counter driving spinners; advanced on every tick.
    pub tick: u64,
    pub backend: MiruBackend,
    pub theme: Theme,
    pub posters: PosterRenderer,
    /// Title of what mpv is currently playing (shown in the player overlay).
    pub now_playing: Option<String>,
    /// The show open on the details screen.
    pub detail: Option<DetailState>,
    /// Dubbed (`true`) or subtitled (`false`) episode listings.
    pub dub: bool,
    /// Screen to return to when playback stops.
    pub player_return: AppView,
    /// The key-help popup is open (it takes all keys until closed).
    pub show_help: bool,
    /// Clickable areas from the last frame (filled in while rendering).
    pub hits: Hits,
    /// Show whose streams are being looked up, so repeated `p` presses don't stack requests.
    resolving: Option<String>,
    /// Screen to return to when leaving details (browse or search).
    detail_return: AppView,
    /// Where settings and history are saved; `None` (as in tests) means don't save.
    state_path: Option<std::path::PathBuf>,
    bg_tx: UnboundedSender<BgEvent>,
    bg_rx: UnboundedReceiver<BgEvent>,
}

impl App {
    pub fn new(posters: PosterRenderer) -> Self {
        let backend = MiruBackend::new();
        let mut carousels = vec![
            CarouselState::new("Continue Watching", vec![]),
            CarouselState::new("Trending Now", vec![]),
            CarouselState::new("Popular This Season", vec![]),
        ];
        carousels[ROW_TRENDING].loading = true;
        carousels[ROW_SEASON].loading = true;
        let (bg_tx, bg_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            running: true, view: AppView::Browse,
            active_carousel: ROW_TRENDING, carousels, hero: HeroState::default(),
            search: SearchState::default(), recent_searches: Vec::new(),
            status: Status::info("welcome"), tick: 0,
            backend, theme: Theme::cozy(), posters, now_playing: None,
            detail: None, dub: false, player_return: AppView::Browse, show_help: false, hits: Hits::default(),
            resolving: None, detail_return: AppView::Browse, state_path: None, bg_tx, bg_rx,
        }
    }

    /// Load saved settings/history from `path` and save there from now on.
    pub fn restore(&mut self, path: std::path::PathBuf) {
        let saved = persist::load(&path);
        self.theme = Theme::from_kind(saved.theme);
        self.dub = saved.dub;
        self.backend.catalog().set_dub(saved.dub);
        self.recent_searches = saved.recent_searches;
        if let Some(row) = self.carousels.get_mut(ROW_CONTINUE) {
            row.set_items(saved.continue_watching);
        }
        self.state_path = Some(path);
    }

    /// Save settings/history (best effort — a failure is logged, never shown as a crash).
    fn save_state(&self) {
        let Some(path) = &self.state_path else { return };
        let saved = Saved {
            theme: self.theme.kind,
            dub: self.dub,
            recent_searches: self.recent_searches.clone(),
            continue_watching: self.carousels.get(ROW_CONTINUE).map(|r| r.items.clone()).unwrap_or_default(),
        };
        if let Err(e) = persist::save(path, &saved) {
            tracing::warn!("couldn't save state: {e:#}");
        }
    }

    pub fn cycle_theme(&mut self) {
        let kinds = Theme::all_kinds();
        let i = kinds.iter().position(|k| *k == self.theme.kind).unwrap_or(0);
        let next = kinds[(i + 1) % kinds.len()];
        self.theme = Theme::from_kind(next);
        self.status = Status::info(format!("theme → {}", self.theme.name()));
        self.save_state();
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
                if let Some(row) = self.carousels.get_mut(ROW_TRENDING) {
                    row.set_items(list);
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
                if seq != self.search.seq {
                    return; // superseded by a newer search
                }
                self.search.selected = 0;
                match result {
                    Ok(results) => {
                        self.status = Status::info(format!("{} results for \"{query}\"", results.len()));
                        self.search.results = SearchResults::Loaded(results);
                    }
                    Err(e) => {
                        self.status = Status::error(format!("search failed: {}", short(&e)));
                        self.search.results = SearchResults::Failed(short(&e));
                    }
                }
            }
            BgEvent::Episodes { anime_id, dub, result } => {
                let Some(d) = self.detail.as_mut() else { return };
                if d.anime.id != anime_id || dub != self.dub {
                    return; // the user moved on to another show or language
                }
                d.selected = 0;
                d.episodes = match result {
                    Ok(list) => {
                        self.status = Status::info(format!("{} episodes ({})", list.episodes.len(), if dub { "dub" } else { "sub" }));
                        Episodes::Loaded(list)
                    }
                    Err(e) => {
                        self.status = Status::error(format!("couldn't load episodes: {}", short(&e)));
                        Episodes::Failed(short(&e))
                    }
                };
            }
            BgEvent::Resolved { anime, episode, result } => {
                self.resolving = None;
                let title = match &episode {
                    Some(ep) => format!("{} — episode {ep}", anime.title),
                    None => anime.title.clone(),
                };
                match result {
                    Ok(streams) if !streams.sources.is_empty() => {
                        // sources arrive sorted best-quality first
                        match self.backend.player_mut().play(&streams.sources[0], &title).await {
                            Ok(()) => {
                                if self.view != AppView::Player {
                                    self.player_return = self.view;
                                }
                                self.view = AppView::Player;
                                self.now_playing = Some(title.clone());
                                self.status = Status::info(match streams.failed.as_slice() {
                                    [] => format!("playing {title} · {}", streams.source),
                                    failed => format!("playing {title} · {} ({} didn't work)", streams.source, failed.join(", ")),
                                });
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
            items.truncate(persist::MAX_CONTINUE);
            row.set_items(items);
        }
        self.save_state();
    }

    pub async fn on_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.running = false;
            return;
        }
        if self.show_help {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') | KeyCode::F(1) | KeyCode::Enter) {
                self.show_help = false;
            }
            return;
        }
        // `?` is a letter on the search screen, so F1 opens help there
        let help_key = key.code == KeyCode::F(1) || (key.code == KeyCode::Char('?') && self.view != AppView::Search);
        if help_key {
            self.show_help = true;
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
            KeyCode::Up | KeyCode::Char('k') => self.move_row(false),
            KeyCode::Down | KeyCode::Char('j') => self.move_row(true),
            KeyCode::Left | KeyCode::Char('h') => self.scroll(false),
            KeyCode::Right | KeyCode::Char('l') => self.scroll(true),
            KeyCode::Enter => {
                if let Some(anime) = self.selected().cloned() {
                    self.open_detail(anime);
                }
            }
            KeyCode::Char('p') => self.play_selected(),
            KeyCode::Char('t') => self.cycle_theme(),
            _ => {}
        }
    }

    async fn on_key_detail(&mut self, key: KeyEvent) {
        let Some(d) = self.detail.as_mut() else {
            self.view = AppView::Browse;
            return;
        };
        let cols = d.grid_cols.get().max(1) as isize;
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace => self.view = self.detail_return,
            KeyCode::Left | KeyCode::Char('h') => d.move_by(-1),
            KeyCode::Right | KeyCode::Char('l') => d.move_by(1),
            KeyCode::Up | KeyCode::Char('k') => d.move_by(-cols),
            KeyCode::Down | KeyCode::Char('j') => d.move_by(cols),
            KeyCode::PageUp => d.move_by(-cols * 5),
            KeyCode::PageDown => d.move_by(cols * 5),
            KeyCode::Home | KeyCode::Char('g') => d.selected = 0,
            KeyCode::End | KeyCode::Char('G') => d.selected = d.episode_count().saturating_sub(1),
            KeyCode::Char(']') => d.desc_scroll = (d.desc_scroll + 1).min(d.desc_max.get()),
            KeyCode::Char('[') => d.desc_scroll = d.desc_scroll.min(d.desc_max.get()).saturating_sub(1),
            KeyCode::Char('s') => {
                self.dub = !self.dub;
                self.backend.catalog().set_dub(self.dub);
                self.status = Status::busy(format!("switched to {} — loading episodes…", if self.dub { "dub" } else { "sub" }));
                self.reload_episodes();
                self.save_state();
            }
            KeyCode::Char('r') if matches!(d.episodes, Episodes::Failed(_)) => self.reload_episodes(),
            KeyCode::Enter | KeyCode::Char('p') => self.play_detail_episode(),
            _ => {}
        }
    }

    /// Open the details screen for `anime` and start loading its episodes.
    fn open_detail(&mut self, anime: Anime) {
        if let Some(u) = &anime.cover_image {
            self.backend.cache.prefetch(u.clone());
        }
        self.detail = Some(DetailState::new(anime));
        if self.view != AppView::Detail {
            self.detail_return = self.view;
        }
        self.view = AppView::Detail;
        self.reload_episodes();
    }

    fn reload_episodes(&mut self) {
        if let Some(d) = self.detail.as_mut() {
            d.episodes = Episodes::Loading;
            d.selected = 0;
            services::spawn_episodes(self.backend.catalog(), self.bg_tx.clone(), d.anime.clone(), self.dub);
        }
    }

    /// Play the highlighted episode on the details screen.
    fn play_detail_episode(&mut self) {
        let Some(d) = &self.detail else { return };
        let Episodes::Loaded(list) = &d.episodes else {
            self.status = Status::error("no episodes to play yet");
            return;
        };
        let Some(ep) = list.episodes.get(d.selected).cloned() else { return };
        if let Some(t) = &self.resolving {
            self.status = Status::busy(format!("still looking up streams for {t}…"));
            return;
        }
        let label = format!("{} episode {}", d.anime.title, ep.label);
        self.status = Status::busy(format!("looking up streams for {label}…"));
        self.resolving = Some(label);
        services::spawn_play_episode(self.backend.catalog(), self.bg_tx.clone(), d.anime.clone(), list.provider, ep);
    }

    fn on_key_search(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            // first Esc clears the box, the second leaves the screen
            KeyCode::Esc if !self.search.query.is_empty() => {
                self.search.query.clear();
                self.search.edited(std::time::Instant::now());
            }
            KeyCode::Esc => self.view = AppView::Browse,
            KeyCode::Up => self.search.move_by(-1, &self.recent_searches),
            KeyCode::Down | KeyCode::Tab => self.search.move_by(1, &self.recent_searches),
            KeyCode::Char('p') if ctrl => self.search.move_by(-1, &self.recent_searches),
            KeyCode::Char('n') if ctrl => self.search.move_by(1, &self.recent_searches),
            KeyCode::Char('u') if ctrl => {
                self.search.query.clear();
                self.search.edited(std::time::Instant::now());
            }
            KeyCode::Enter => self.search_enter(),
            KeyCode::Backspace => {
                self.search.query.pop();
                self.search.edited(std::time::Instant::now());
            }
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                self.search.query.push(c);
                self.search.edited(std::time::Instant::now());
            }
            _ => {}
        }
    }

    /// Enter on the search screen: open the highlighted result, re-run a recent search,
    /// or search right away if results aren't in yet.
    fn search_enter(&mut self) {
        match &self.search.results {
            SearchResults::Loaded(r) if !r.is_empty() && self.search.sent == self.search.query.trim() => {
                let Some(anime) = r.get(self.search.selected).cloned() else { return };
                remember_search(&mut self.recent_searches, &self.search.query);
                self.save_state();
                self.open_detail(anime);
            }
            SearchResults::Idle if self.search.query.trim().is_empty() => {
                if let Some(q) = self.recent_searches.get(self.search.selected).cloned() {
                    self.search.query = q;
                    self.start_search();
                }
            }
            _ => self.start_search(),
        }
    }

    /// Send a search for the current query now.
    fn start_search(&mut self) {
        let q = self.search.query.trim().to_string();
        self.search.due = None;
        if q.is_empty() {
            return;
        }
        self.search.seq += 1;
        self.search.sent = q.clone();
        self.search.results = SearchResults::Loading;
        self.search.selected = 0;
        self.status = Status::busy(format!("searching \"{q}\"…"));
        services::spawn_search(self.backend.catalog(), self.bg_tx.clone(), self.search.seq, q);
    }

    async fn on_key_player(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
            let _ = self.backend.player_mut().stop().await;
            self.now_playing = None;
            self.view = self.player_return;
            self.status = Status::info("stopped");
        }
    }

    pub async fn on_mouse(&mut self, event: MouseEvent) {
        use crossterm::event::{MouseButton, MouseEventKind};
        if self.show_help || self.view == AppView::Player {
            return;
        }
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let at = ratatui::layout::Position::new(event.column, event.row);
                let hits = self.hits.take();
                let target = hits.iter().find(|(r, _)| r.contains(at)).map(|(_, t)| *t);
                self.hits.set(hits);
                if let Some(t) = target {
                    self.click(t);
                }
            }
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                let down = matches!(event.kind, MouseEventKind::ScrollDown);
                match self.view {
                    AppView::Browse => self.scroll(down),
                    AppView::Search => self.search.move_by(if down { 1 } else { -1 }, &self.recent_searches),
                    AppView::Detail => {
                        if let Some(d) = self.detail.as_mut() {
                            let cols = d.grid_cols.get().max(1) as isize;
                            d.move_by(if down { cols } else { -cols });
                        }
                    }
                    AppView::Player => {}
                }
            }
            _ => {}
        }
    }

    /// First click selects; clicking the selected thing again opens (or plays) it.
    fn click(&mut self, target: HitTarget) {
        match target {
            HitTarget::Card { row, index } => {
                let already = self.active_carousel == row && self.carousels.get(row).is_some_and(|c| c.selected == index);
                if already {
                    if let Some(anime) = self.selected().cloned() {
                        self.open_detail(anime);
                    }
                } else if let Some(c) = self.carousels.get_mut(row) {
                    c.selected = index.min(c.items.len().saturating_sub(1));
                    self.active_carousel = row;
                    self.sync_hero();
                }
            }
            HitTarget::SearchResult(i) => {
                if self.search.selected == i {
                    self.search_enter();
                } else {
                    self.search.selected = i;
                }
            }
            HitTarget::Episode(i) => {
                let Some(d) = self.detail.as_mut() else { return };
                if d.selected == i {
                    self.play_detail_episode();
                } else {
                    d.selected = i;
                }
            }
        }
    }

    pub async fn on_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        // live search: fire once typing has paused, if the text actually changed
        if self.search.due.is_some_and(|due| std::time::Instant::now() >= due) {
            self.search.due = None;
            if self.search.query.trim().chars().count() >= 2 && self.search.query.trim() != self.search.sent {
                self.start_search();
            }
        }
        // mpv closed on its own (window closed / episode ended) → leave the player overlay
        if let Ok(true) = self.backend.player_mut().poll().await {
            if self.view == AppView::Player {
                self.view = self.player_return;
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
