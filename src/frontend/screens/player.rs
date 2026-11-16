//! Player — SEPARATE SCREEN (currently overlay, easy to flip to full-screen).
//!
//! Doc: this is conceptually a *separate screen* — it takes over input (Esc stops mpv).
//! Currently rendered as centered overlay for cozy preview; to make it full-screen
//! just render `Block` over `area` instead of `centered_rect`.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::{Block, BorderType, Borders, Padding},
    Frame,
};

use crate::frontend::app::App;
use crate::frontend::screens::{Screen, ScreenKind};
use crate::frontend::theme::Theme;

pub struct PlayerScreen;
impl Screen for PlayerScreen {
    // flip to ScreenKind::Separate to make this full-screen takeover instead of overlay
    const KIND: ScreenKind = ScreenKind::Overlay;
    const NAME: &'static str = "player";
    fn render(f: &mut ratatui::Frame, app: &App, area: ratatui::layout::Rect, theme: Theme) { render(f, app, area, theme); }
}
pub const KIND: ScreenKind = ScreenKind::Overlay;

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let v = Layout::default().direction(Direction::Vertical).constraints([
        Constraint::Percentage((100 - percent_y) / 2), Constraint::Percentage(percent_y), Constraint::Percentage((100 - percent_y) / 2),
    ]).split(r);
    Layout::default().direction(Direction::Horizontal).constraints([
        Constraint::Percentage((100 - percent_x) / 2), Constraint::Percentage(percent_x), Constraint::Percentage((100 - percent_x) / 2),
    ]).split(v[1])[1]
}

pub fn render(f: &mut Frame, app: &App, area: Rect, t: Theme) {
    // For true separate-screen, use `let popup = area;` — keeping overlay for now.
    let popup = centered_rect(56, 18, area);
    f.render_widget(ratatui::widgets::Clear, popup);
    let title = app.now_playing.as_deref().unwrap_or("playback");
    let block = Block::default().title(format!("  ▶  {}  ", title)).title_style(Style::default().fg(t.accent_green))
        .borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(t.accent_green))
        .style(Style::default().bg(t.surface)).padding(Padding::new(1, 1, 0, 0));
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    let p = ratatui::widgets::Paragraph::new("  playing in mpv  —  esc to stop  ♡  ").style(Style::default().fg(t.text)).centered();
    f.render_widget(p, inner);
}
