//! Themed scrollbar — data-driven via `Theme::scrollbar`.
//! Pass `theme.scrollbar` so switching `ThemeKind` automatically recolors it.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::frontend::theme::{ScrollbarTheme, Theme};

/// Renders a horizontal scrollbar track + thumb purely with theme colors.
/// `total` = item count, `visible` = viewport, `offset` = scroll offset.
pub fn scrollbar_track(area: Rect) -> Rect {
    // reserve last row of `area` for the bar
    Rect { x: area.x, y: area.y + area.height.saturating_sub(1), width: area.width, height: 1 }
}

pub struct ThemedScrollbar {
    pub total: usize,
    pub visible: usize,
    pub offset: usize,
    pub focused: bool,
}

impl ThemedScrollbar {
    pub fn render(self, f: &mut Frame, area: Rect, theme: Theme) {
        render_scrollbar(f, area, self, theme.scrollbar);
    }
}

fn render_scrollbar(f: &mut Frame, area: Rect, s: ThemedScrollbar, t: ScrollbarTheme) {
    if s.total == 0 || area.width == 0 {
        return;
    }
    let bar = scrollbar_track(area);
    let thumb_w = ((s.visible as f32 / s.total as f32) * bar.width as f32).ceil() as u16;
    let thumb_w = thumb_w.clamp(1, bar.width);
    let max_offset = s.total.saturating_sub(s.visible) as u16;
    let thumb_x = if max_offset == 0 {
        bar.x
    } else {
        bar.x + ((s.offset as u16 as f32 / max_offset as f32) * (bar.width - thumb_w) as f32) as u16
    };
    let mut spans = Vec::new();
    for x in 0..bar.width {
        let is_thumb = x >= thumb_x.saturating_sub(bar.x) && x < thumb_x.saturating_sub(bar.x) + thumb_w;
        // thin line glyphs: a heavy thumb over a light track, matching the rounded borders
        let ch = if is_thumb { "━" } else { "─" };
        let col = if is_thumb {
            if s.focused { t.thumb_focused } else { t.thumb }
        } else {
            t.track
        };
        spans.push(Span::styled(ch.to_string(), Style::default().fg(col)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), bar);
}
