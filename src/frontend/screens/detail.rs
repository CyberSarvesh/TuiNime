//! Detail — OVERLAY (not a separate full-screen).
//!
//! Rendered with `Clear` + centered `Block::Rounded` on top of `browse`.
//! Stays as overlay so Esc returns instantly without rebuild.
//! If you want it as separate screen, change this to take full `area` and skip `Clear`.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, BorderType, Borders, Padding},
    Frame,
};

use crate::frontend::app::App;
use crate::frontend::screens::{Screen, ScreenKind};
use crate::frontend::theme::Theme;

pub struct DetailScreen;
impl Screen for DetailScreen {
    const KIND: ScreenKind = ScreenKind::Overlay;
    const NAME: &'static str = "detail";
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
    let popup = centered_rect(70, 60, area);
    f.render_widget(ratatui::widgets::Clear, popup);
    let anime = app.carousels.get(app.active_carousel).and_then(|c| c.selected_item());
    let (title, body) = if let Some(a) = anime {
        (format!("  {}  ", a.title), format!(
            "{}\n\n  ★ {}   •   {} eps   •   {}\n\n{}\n\n  p play   •   esc close",
            a.title, a.score.map(|s| s.to_string()).unwrap_or_else(|| "—".into()),
            a.episodes.map(|e| e.to_string()).unwrap_or_else(|| "—".into()),
            a.status.clone().unwrap_or_else(|| "—".into()),
            a.description.clone().unwrap_or_default()
        ))
    } else { ("  tuinime  ".into(), "  no selection  ".into()) };
    let block = Block::default().title(title).title_style(Style::default().fg(t.accent).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(t.border_focus))
        .style(Style::default().bg(t.surface)).padding(Padding::new(1, 1, 1, 1));
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    let p = ratatui::widgets::Paragraph::new(body).style(Style::default().fg(t.text)).wrap(ratatui::widgets::Wrap { trim: true });
    f.render_widget(p, inner);
}
