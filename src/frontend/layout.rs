//! Shared layout helpers so every screen and popup agrees on geometry.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding};
use ratatui::Frame;

use crate::frontend::theme::Theme;

/// A `w`×`h` rect centered in `area` (clamped to fit).
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

/// Popup shell: clears `area`, draws a rounded themed border with `title` in `accent`,
/// and returns the inner rect for content.
pub fn popup(f: &mut Frame, area: Rect, theme: Theme, title: &str, accent: Color) -> Rect {
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(format!(" {title} "))
        .title_style(Style::default().fg(accent).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .style(Style::default().bg(theme.surface).fg(theme.text))
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_clamps_to_area() {
        let area = Rect::new(0, 0, 10, 4);
        assert_eq!(centered(area, 20, 20), area);
        assert_eq!(centered(area, 4, 2), Rect::new(3, 1, 4, 2));
    }
}
