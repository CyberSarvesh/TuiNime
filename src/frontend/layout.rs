//! Shared layout constants + overlay helpers.
//! Every screen's geometry lives here so `browse` + overlays agree on spacing.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding};
use ratatui::Frame;

use crate::frontend::theme::Theme;

/// Standard browse split: header | hero | carousels | footer.
pub fn browse_chunks(area: Rect) -> [Rect; 4] {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(8),
            Constraint::Min(12),
            Constraint::Length(1),
        ])
        .split(area)
        .as_ref()
        .try_into()
        .expect("browse_chunks")
}

/// Centered popup. `w,h` are absolute cell counts.
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect { x, y, width: w, height: h }
}

/// Overlay shell: `Clear` + themed rounded block. Returns inner rect for content.
pub fn overlay_block(f: &mut Frame, area: Rect, theme: Theme, title: &str) -> Rect {
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focus))
        .style(Style::default().bg(theme.surface).fg(theme.text))
        .padding(Padding::uniform(1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    inner
}
