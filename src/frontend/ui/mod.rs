//! Top-level render: header bar, the current screen, footer bar.
//! All colors come from `app.theme`.

pub mod carousel;
pub mod hero;

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};

use crate::frontend::app::{App, AppView};
use crate::frontend::screens;
use crate::frontend::state::{spinner_frame, StatusKind};
use crate::frontend::theme::Theme;

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();
    let t = app.theme;
    f.render_widget(Block::default().style(Style::default().bg(t.bg)), area);
    if area.height < 3 {
        return;
    }
    let header = Rect { height: 1, ..area };
    let footer = Rect { y: area.y + area.height - 1, height: 1, ..area };
    let body = Rect { y: area.y + 1, height: area.height - 2, ..area };

    render_header(f, app, header, t);
    // full-screen base: the player popup sits over whichever screen started playback
    let base = if app.view == AppView::Player { app.player_return } else { app.view };
    match base {
        AppView::Detail => screens::detail::render(f, app, body, t),
        _ => screens::browse::render(f, app, body, t),
    }
    match app.view {
        AppView::Search => screens::search::render(f, app, body, t),
        AppView::Player => screens::player::render(f, app, body, t),
        AppView::Browse | AppView::Detail => {}
    }
    render_footer(f, app, footer, t);
}

fn render_header(f: &mut Frame, app: &App, area: Rect, t: Theme) {
    let bar = Style::default().bg(t.surface).fg(t.text);
    f.render_widget(Block::default().style(bar), area);
    let brand = Line::from(vec![
        Span::styled(" ฅ tuinime ", Style::default().fg(t.accent).add_modifier(Modifier::BOLD)),
        Span::styled(format!(" {}", view_name(app.view)), Style::default().fg(t.text_muted)),
    ]);
    f.render_widget(Paragraph::new(brand), area);
    let theme = Span::styled(format!("{} theme ", t.name()), Style::default().fg(t.text_muted));
    f.render_widget(Paragraph::new(theme).right_aligned(), area);
}

fn view_name(v: AppView) -> &'static str {
    match v {
        AppView::Browse => "browse",
        AppView::Detail => "details",
        AppView::Search => "search",
        AppView::Player => "now playing",
    }
}

/// Key hints for the footer, per screen.
pub fn hints(v: AppView) -> &'static str {
    match v {
        AppView::Browse => "←→ move  ↑↓ rows  ⏎ details  p play  / search  t theme  q quit",
        AppView::Detail => "←↑↓→ episode  ⏎ play  s sub/dub  [ ] scroll text  esc back",
        AppView::Search => "⏎ search  esc cancel",
        AppView::Player => "esc stop",
    }
}

fn render_footer(f: &mut Frame, app: &App, area: Rect, t: Theme) {
    f.render_widget(Block::default().style(Style::default().bg(t.surface)), area);
    let (icon, color) = match app.status.kind {
        StatusKind::Busy => (spinner_frame(app.tick), t.accent),
        StatusKind::Error => ("✗", t.error),
        StatusKind::Info => ("•", t.text_muted),
    };
    let text_color = if app.status.kind == StatusKind::Error { t.error } else { t.text_dim };
    let status = Line::from(vec![
        Span::styled(format!(" {icon} "), Style::default().fg(color).add_modifier(Modifier::BOLD)),
        Span::styled(app.status.text.clone(), Style::default().fg(text_color)),
    ]);
    let hint = hints(app.view);
    let hint_w = hint.chars().count() as u16 + 1;
    let status_w = status.width() as u16;
    // hints only when they fit next to the status; the status always wins
    if area.width >= status_w + hint_w + 2 {
        f.render_widget(Paragraph::new(Span::styled(format!("{hint} "), Style::default().fg(t.text_muted))).right_aligned(), area);
    }
    f.render_widget(Paragraph::new(status), area);
}

/// Helpers for rendering the app off-screen in tests.
#[cfg(test)]
pub(crate) mod testkit {
    use ratatui::{backend::TestBackend, Terminal};

    use crate::backend::Anime;
    use crate::frontend::app::App;
    use crate::frontend::widgets::poster::PosterRenderer;

    pub fn app() -> App {
        App::new(PosterRenderer::new(ratatui_image::picker::Picker::from_fontsize((8, 16))))
    }

    pub fn anime(title: &str) -> Anime {
        Anime {
            id: title.into(), title: title.into(), description: None, cover_image: None,
            banner_image: None, score: None, year: None, episodes: None, status: None,
            genres: vec![], alt_titles: vec![],
        }
    }

    /// A fully filled-in show for screen tests.
    pub fn rich(title: &str, i: u32) -> Anime {
        Anime {
            description: Some(format!("{title} is a show about a lot of things. It has a long description that should wrap nicely across several lines in the hero panel.")),
            score: Some(70 + (i % 20) as u8),
            year: Some(2026),
            episodes: Some(12 + i),
            status: Some(if i.is_multiple_of(2) { "RELEASING" } else { "FINISHED" }.into()),
            genres: vec!["Action".into(), "Fantasy".into()],
            ..anime(title)
        }
    }

    /// Draw the whole app into a `w`×`h` buffer and return it as text, one line per row.
    pub fn draw(app: &App, w: u16, h: u16) -> String {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| super::render(f, app)).unwrap();
        let buf = term.backend().buffer();
        (0..h)
            .map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>().trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Print a screen when `TUINIME_SNAPSHOTS=1`, so a human can look at it.
    pub fn show(name: &str, screen: &str) {
        if std::env::var_os("TUINIME_SNAPSHOTS").is_some() {
            println!("\n===== {name} =====\n{screen}\n");
        }
    }
}

#[cfg(test)]
mod screen_tests {
    use super::testkit::{app, draw, rich, show};
    use crate::frontend::services::BgEvent;
    use crate::frontend::state::Status;

    async fn loaded_app() -> crate::frontend::app::App {
        let mut a = app();
        let trending = (0..12).map(|i| rich(&format!("Trending Show {i}"), i)).collect();
        a.on_background(BgEvent::Trending(trending)).await;
        let season = (0..8).map(|i| rich(&format!("Season Pick {i}"), i)).collect();
        a.on_background(BgEvent::Seasonal { label: "fall 2026".into(), result: Ok(season) }).await;
        a
    }

    #[tokio::test]
    async fn browse_while_loading() {
        let a = app();
        let s = draw(&a, 100, 30);
        show("browse while loading (100x30)", &s);
        assert!(s.contains("welcome home"));
        assert!(s.contains("loading…"));
        assert!(!s.contains("continue watching"), "empty continue row should be hidden");
    }

    #[tokio::test]
    async fn browse_loaded_wide() {
        let a = loaded_app().await;
        let s = draw(&a, 140, 42);
        show("browse loaded (140x42)", &s);
        assert!(s.contains("Trending Show 0"), "hero shows the selected show");
        assert!(s.contains("airing"));
        assert!(s.contains("Action · Fantasy"));
        assert!(s.contains("trending now 1/12"));
        assert!(s.contains("popular this season (fall 2026)"));
        assert!(s.contains("⏎ details"), "hints fit on a wide window");
    }

    #[tokio::test]
    async fn browse_small_window_keeps_status_over_hints() {
        let mut a = loaded_app().await;
        a.status = Status::error("search failed: connection refused");
        let s = draw(&a, 70, 20);
        show("browse small (70x20) with error", &s);
        assert!(s.contains("✗ search failed"));
        assert!(!s.contains("⏎ details"), "hints drop out before the status does");
        assert!(!s.contains("welcome"), "hero hides on short windows");
    }

    #[tokio::test]
    async fn any_window_size_renders_without_panicking() {
        let a = loaded_app().await;
        for w in [1, 2, 5, 10, 20, 33, 60, 80, 120, 250] {
            for h in [1, 2, 3, 5, 8, 12, 19, 20, 24, 40, 80] {
                draw(&a, w, h);
            }
        }
    }

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use crate::backend::{Episode, EpisodeList};
    use crate::frontend::app::AppView;
    use crate::frontend::state::Episodes;

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    fn episodes(n: u32) -> EpisodeList {
        EpisodeList {
            provider: 0,
            show: "Trending Show Zero (romaji)".into(),
            episodes: (1..=n)
                .map(|i| Episode { id: format!("x:{i}"), number: i, label: i.to_string(), title: None, thumbnail: None })
                .collect(),
        }
    }

    async fn detail_app(n: u32) -> crate::frontend::app::App {
        let mut a = loaded_app().await;
        a.on_key(key(KeyCode::Enter)).await;
        let id = a.detail.as_ref().unwrap().anime.id.clone();
        a.on_background(BgEvent::Episodes { anime_id: id, dub: false, result: Ok(episodes(n)) }).await;
        a
    }

    #[tokio::test]
    async fn enter_opens_details_and_shows_loading() {
        let mut a = loaded_app().await;
        a.on_key(key(KeyCode::Enter)).await;
        assert_eq!(a.view, AppView::Detail);
        let s = draw(&a, 120, 36);
        show("details while loading episodes (120x36)", &s);
        assert!(s.contains("Trending Show 0"));
        assert!(s.contains("finding episodes…"));
        assert!(s.contains("episodes · sub"));
    }

    #[tokio::test]
    async fn details_episode_grid_and_keys() {
        let mut a = detail_app(30).await;
        let s = draw(&a, 120, 36);
        show("details with 30 episodes (120x36)", &s);
        assert!(s.contains("episodes · sub · 30"));
        assert!(s.contains("▶ episode 1"));
        assert!(s.contains("listed as Trending Show Zero (romaji)"));
        assert!(s.contains("⏎ play"));

        a.on_key(key(KeyCode::Right)).await;
        assert_eq!(a.detail.as_ref().unwrap().selected, 1);
        let cols = a.detail.as_ref().unwrap().grid_cols.get();
        assert!(cols > 1, "grid should have several columns at 120 wide");
        a.on_key(key(KeyCode::Down)).await;
        assert_eq!(a.detail.as_ref().unwrap().selected, 1 + cols);
        a.on_key(key(KeyCode::End)).await;
        assert!(draw(&a, 120, 36).contains("▶ episode 30"));
        a.on_key(key(KeyCode::Down)).await; // already at the end: stays put
        assert_eq!(a.detail.as_ref().unwrap().selected, 29);

        a.on_key(key(KeyCode::Esc)).await;
        assert_eq!(a.view, AppView::Browse);
    }

    #[tokio::test]
    async fn switching_to_dub_reloads_and_ignores_stale_results() {
        let mut a = detail_app(12).await;
        a.on_key(key(KeyCode::Char('s'))).await;
        assert!(a.dub);
        assert!(matches!(a.detail.as_ref().unwrap().episodes, Episodes::Loading));
        // a late "sub" answer must not replace the pending "dub" list
        let id = a.detail.as_ref().unwrap().anime.id.clone();
        a.on_background(BgEvent::Episodes { anime_id: id.clone(), dub: false, result: Ok(episodes(12)) }).await;
        assert!(matches!(a.detail.as_ref().unwrap().episodes, Episodes::Loading));
        a.on_background(BgEvent::Episodes { anime_id: id, dub: true, result: Ok(episodes(5)) }).await;
        assert_eq!(a.detail.as_ref().unwrap().episode_count(), 5);
        assert!(draw(&a, 120, 36).contains("episodes · dub · 5"));
    }

    #[tokio::test]
    async fn details_failure_message() {
        let mut a = loaded_app().await;
        a.on_key(key(KeyCode::Enter)).await;
        let id = a.detail.as_ref().unwrap().anime.id.clone();
        a.on_background(BgEvent::Episodes { anime_id: id, dub: false, result: Err("no stream provider has Trending Show 0".into()) }).await;
        let s = draw(&a, 100, 30);
        show("details when episodes fail (100x30)", &s);
        assert!(s.contains("couldn't load episodes"));
        assert!(s.contains("r to retry"));
        assert!(s.contains("✗ couldn't load episodes"));
    }

    #[tokio::test]
    async fn details_any_size_renders_without_panicking() {
        let a = detail_app(200).await;
        for w in [1, 5, 20, 40, 69, 70, 100, 200] {
            for h in [1, 3, 6, 10, 20, 50] {
                draw(&a, w, h);
            }
        }
    }
}
