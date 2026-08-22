//! Poster — shared image helper so `hero`/`carousel`/`detail` don't duplicate `ImageCache` logic.
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::backend::cache::ImageCache;
use crate::frontend::theme::Theme;

/// Renders a poster placeholder / cached thumbnail indicator.
/// Real `ratatui-image` protocol rendering stays in `hero` for now; this is the seam.
pub struct Poster<'a> {
    pub url: Option<&'a str>,
    pub cache: &'a ImageCache,
    pub theme: Theme,
}

impl<'a> Poster<'a> {
    pub fn render(self, f: &mut Frame, area: Rect) {
        let filled = self.url.map(|u| self.cache.is_cached(u)).unwrap_or(false);
        let (glyph, color) = if filled {
            ("▓ poster cached ▓", self.theme.accent_green)
        } else if self.url.is_some() {
            ("◇ loading… ◇", self.theme.text_muted)
        } else {
            ("— no image —", self.theme.text_muted)
        };
        let p = Paragraph::new(glyph)
            .style(Style::default().fg(color).bg(self.theme.surface_hover))
            .block(Block::default().borders(Borders::NONE))
            .centered();
        f.render_widget(p, area);
    }
}
