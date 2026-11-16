//! Browse — SEPARATE SCREEN (full-screen base layer).
//!
//! This is the only screen that owns the full layout: header + hero + carousels + footer.
//! Overlays (`detail`/`search`/`player`) are rendered *on top* of this via `Clear`.
//! Keep this as the background so overlays can assume browse is already drawn.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::frontend::app::App;
use crate::frontend::screens::{Screen, ScreenKind};
use crate::frontend::theme::Theme;
use crate::frontend::ui::{carousel, hero};

pub struct BrowseScreen;
impl Screen for BrowseScreen {
    const KIND: ScreenKind = ScreenKind::Separate;
    const NAME: &'static str = "browse";
    fn render(f: &mut ratatui::Frame, app: &App, area: ratatui::layout::Rect, theme: Theme) {
        render(f, app, theme);
        let _ = area;
    }
}
pub const KIND: ScreenKind = ScreenKind::Separate;

pub fn render(f: &mut Frame, app: &App, theme: Theme) {
    let area = f.area();
    // background fill from theme
    f.render_widget(Block::default().style(Style::default().bg(theme.bg)), area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // header
            Constraint::Length(8),  // hero
            Constraint::Min(12),    // carousels
            Constraint::Length(1),  // footer
        ])
        .split(area);

    render_header(f, app, chunks[0], theme);
    render_hero(f, app, chunks[1], theme);
    render_carousels(f, app, chunks[2], theme);
    render_footer(f, app, chunks[3], theme);
}

fn render_header(f: &mut Frame, app: &App, area: Rect, t: Theme) {
    let left = Span::styled("  ฅ  tuinime  ", Style::default().fg(t.accent).add_modifier(Modifier::BOLD));
    let right = Span::styled("  / search  •  enter details  •  p play  •  q quit  ", Style::default().fg(t.text_muted));
    let p = Paragraph::new(Line::from(vec![
        left.clone(), Span::raw(" "),
        Span::styled(format!("— {}", app.status.trim()), Style::default().fg(t.text_muted)),
        Span::raw("  "), right,
    ])).style(Style::default().bg(t.surface).fg(t.text));
    f.render_widget(p, area);
}

fn render_hero(f: &mut Frame, app: &App, area: Rect, theme: Theme) {
    let padded = Rect { x: area.x + 1, y: area.y, width: area.width.saturating_sub(2), height: area.height };
    hero::render(f, &app.hero, padded, &app.backend.cache, &app.posters, theme);
}

fn render_carousels(f: &mut Frame, app: &App, area: Rect, t: Theme) {
    if app.carousels.is_empty() {
        let block = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border)).style(Style::default().bg(t.surface));
        let p = Paragraph::new("  no content — cozy offline mode  ").style(Style::default().fg(t.text_dim)).block(block).centered();
        f.render_widget(p, area);
        return;
    }
    let is_continue_empty = app.carousels.first().map(|c| c.items.is_empty()).unwrap_or(true);
    let constraints = if is_continue_empty && app.carousels.len() == 3 {
        vec![Constraint::Length(3), Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]
    } else {
        let n = app.carousels.len() as u32;
        (0..n).map(|_| Constraint::Ratio(1, n)).collect()
    };
    let rows = Layout::default().direction(Direction::Vertical).constraints(constraints).split(area);
    for (i, (carousel, row_area)) in app.carousels.iter().zip(rows.iter()).enumerate() {
        let focused = app.active_carousel == i && matches!(app.view, crate::frontend::app::AppView::Browse);
        let inset = Rect { x: row_area.x + 1, y: row_area.y, width: row_area.width.saturating_sub(2), height: row_area.height };
        carousel::render(f, carousel, inset, focused, &app.backend.cache, &app.posters, t);
    }
}

fn render_footer(f: &mut Frame, app: &App, area: Rect, t: Theme) {
    // show current theme name so future selector has visible feedback
    let line = Line::from(vec![
        Span::styled("  ♡  ", Style::default().fg(t.accent)),
        Span::styled(format!("{} mode", t.name()), Style::default().fg(t.text_dim)),
        Span::raw("  •  "), Span::styled("h/l scroll  j/k rows  enter open", Style::default().fg(t.text_muted)),
        Span::raw("  •  "), Span::styled(format!("t theme: {}", app.theme.name()), Style::default().fg(t.text_muted)),
    ]);
    let p = Paragraph::new(line).style(Style::default().bg(t.surface).fg(t.text)).centered();
    f.render_widget(p, area);
}
