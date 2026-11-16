//! Browse — hero panel on top, rows of shows below. Sizes everything from the window.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::Span,
    widgets::Paragraph,
    Frame,
};

use crate::frontend::app::{App, AppView};
use crate::frontend::state::spinner_frame;
use crate::frontend::theme::Theme;
use crate::frontend::ui::carousel::{self, RowStyle};
use crate::frontend::ui::hero;

/// Minimum height for a row of cards (border + card + scrollbar).
const MIN_ROW_H: u16 = 7;

pub fn render(f: &mut Frame, app: &App, area: Rect, t: Theme) {
    // the hero gets about a third of the height, and disappears on very short windows
    let hero_h = if area.height >= 20 { (area.height / 3).clamp(8, 16) } else { 0 };
    if hero_h > 0 {
        let hero_area = Rect { x: area.x + 1, width: area.width.saturating_sub(2), height: hero_h, ..area };
        hero::render(f, &app.hero, hero_area, &app.backend.cache, &app.posters, t);
    }
    let rows_area = Rect { y: area.y + hero_h, height: area.height - hero_h, ..area };
    render_rows(f, app, rows_area, t);
}

/// Indices of `rows` to draw so the active row is always among them.
fn window(rows: &[usize], active: usize, fit: usize) -> &[usize] {
    if rows.len() <= fit {
        return rows;
    }
    let pos = rows.iter().position(|r| *r == active).unwrap_or(0);
    let start = pos.saturating_sub(fit - 1).min(rows.len() - fit);
    &rows[start..start + fit]
}

fn render_rows(f: &mut Frame, app: &App, area: Rect, t: Theme) {
    let rows = app.visible_rows();
    if rows.is_empty() {
        let p = Paragraph::new(Span::styled("couldn't load anything — check your connection", Style::default().fg(t.text_muted).add_modifier(Modifier::ITALIC))).centered();
        f.render_widget(p, Rect { y: area.y + area.height / 2, height: 1, ..area });
        return;
    }
    let fit = ((area.height / MIN_ROW_H) as usize).max(1);
    let shown = window(&rows, app.active_carousel, fit);
    let n = shown.len() as u16;
    let base_h = area.height / n;
    let mut y = area.y;
    for (i, &row) in shown.iter().enumerate() {
        // the last row takes any leftover lines
        let h = if i as u16 == n - 1 { area.y + area.height - y } else { base_h };
        let row_area = Rect { x: area.x + 1, y, width: area.width.saturating_sub(2), height: h };
        let style = RowStyle {
            focused: app.active_carousel == row && app.view == AppView::Browse,
            spinner: spinner_frame(app.tick),
        };
        carousel::render(f, &app.carousels[row], row_area, style, &app.backend.cache, &app.posters, t);
        y += h;
    }
}

#[cfg(test)]
mod tests {
    use super::window;

    #[test]
    fn window_keeps_active_row_visible() {
        let rows = [0, 1, 2];
        assert_eq!(window(&rows, 2, 3), &[0, 1, 2]);
        assert_eq!(window(&rows, 0, 2), &[0, 1]);
        assert_eq!(window(&rows, 2, 2), &[1, 2]);
        assert_eq!(window(&rows, 2, 1), &[2]);
    }
}
