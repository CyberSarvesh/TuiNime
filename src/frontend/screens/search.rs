//! Search — OVERLAY.
//!
//! Like `detail`, this is a popup overlay (60x20 centered) with `Clear`.
//! Keeps browse visible underneath.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding},
    Frame,
};

use crate::frontend::app::App;
use crate::frontend::screens::{Screen, ScreenKind};
use crate::frontend::theme::Theme;

pub struct SearchScreen;
impl Screen for SearchScreen {
    const KIND: ScreenKind = ScreenKind::Overlay;
    const NAME: &'static str = "search";
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
    let popup = centered_rect(60, 20, area);
    f.render_widget(ratatui::widgets::Clear, popup);
    let block = Block::default().title("  ⌕  search  —  enter to go  •  esc to close  ").title_style(Style::default().fg(t.text))
        .borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(t.border_focus))
        .style(Style::default().bg(t.surface)).padding(Padding::new(1, 1, 0, 0));
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    let input = ratatui::widgets::Paragraph::new(Line::from(vec![
        Span::styled(app.search_query.clone(), Style::default().fg(t.text)),
        Span::styled("▌", Style::default().fg(t.accent)),
    ]));
    f.render_widget(input, inner);
}
