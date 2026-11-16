//! Carousel — one titled row of show cards.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding, Paragraph},
    Frame,
};

use crate::backend::cache::ImageCache;
use crate::backend::providers::traits::Anime;
use crate::frontend::theme::Theme;
use crate::frontend::ui::hero::poster_width_for;
use crate::frontend::widgets::poster::PosterRenderer;
use crate::frontend::widgets::scrollbar::ThemedScrollbar;

#[derive(Debug, Clone)]
pub struct CarouselState {
    pub title: String,
    pub items: Vec<Anime>,
    pub selected: usize,
    pub offset: usize,
    /// Items are being fetched; the row shows a spinner until `set_items`.
    pub loading: bool,
}

impl CarouselState {
    pub fn new(title: impl Into<String>, items: Vec<Anime>) -> Self {
        Self { title: title.into(), items, selected: 0, offset: 0, loading: false }
    }
    pub fn selected_item(&self) -> Option<&Anime> { self.items.get(self.selected) }
    /// Replace the row's contents, reset the cursor and clear `loading`.
    pub fn set_items(&mut self, items: Vec<Anime>) {
        self.items = items;
        self.selected = 0;
        self.offset = 0;
        self.loading = false;
    }
    pub fn scroll_right(&mut self) {
        if self.selected + 1 < self.items.len() {
            self.selected += 1;
        }
    }
    pub fn scroll_left(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }
}

/// Rows needed for a card's text: title, rule, score/year, episodes.
const TEXT_LINES: u16 = 4;
const MIN_CARD_W: u16 = 18;
const MAX_CARD_W: u16 = 30;

/// Card width for a given card height: wide enough for an undistorted poster above the text.
pub fn card_width_for(card_h: u16) -> u16 {
    let poster_h = card_h.saturating_sub(2 + TEXT_LINES);
    if poster_h < 3 {
        return MIN_CARD_W;
    }
    (poster_width_for(poster_h) + 4).clamp(MIN_CARD_W, MAX_CARD_W)
}

pub struct RowStyle<'a> {
    pub focused: bool,
    pub spinner: &'a str,
}

pub fn render(f: &mut Frame, state: &CarouselState, area: Rect, style: RowStyle, cache: &ImageCache, posters: &PosterRenderer, t: Theme) {
    let RowStyle { focused, spinner } = style;
    let border_style = Style::default().fg(if focused { t.border_focus } else { t.border });
    let title_style = if focused { Style::default().fg(t.accent).add_modifier(Modifier::BOLD) } else { Style::default().fg(t.text_dim) };
    let count = if state.items.is_empty() { String::new() } else { format!("{}/{}", state.selected + 1, state.items.len()) };
    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(format!(" {} ", state.title.to_lowercase()), title_style),
            Span::styled(count, Style::default().fg(t.text_muted)),
            Span::raw(" "),
        ]))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .style(Style::default().bg(t.surface))
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    if state.items.is_empty() {
        let msg = if state.loading { format!("{spinner} loading…") } else { "nothing here yet".to_string() };
        let p = Paragraph::new(Span::styled(msg, Style::default().fg(t.text_muted).add_modifier(Modifier::ITALIC))).centered();
        f.render_widget(p, Rect { y: inner.y + inner.height / 2, height: 1, ..inner });
        return;
    }

    // leave the last row for the scrollbar; cards grow with the row so posters keep their shape
    let card_h = inner.height.saturating_sub(1).min(20);
    if card_h < 3 {
        return;
    }
    // as many cards as fit at their natural width, then stretched to fill the row evenly
    let avail = inner.width.saturating_sub(4); // 2 columns each side for the ‹ › arrows
    let visible = ((avail + 1) / (card_width_for(card_h) + 1)).max(1) as usize;
    let card_w = ((avail + 1) / visible as u16).saturating_sub(1).clamp(MIN_CARD_W.min(avail), MAX_CARD_W + 8);
    // `scroll_*` doesn't know the screen width, so clamp here to keep the selection on screen
    let offset = if state.selected >= state.offset + visible { state.selected + 1 - visible } else { state.offset.min(state.selected) };
    let end = (offset + visible).min(state.items.len());
    let left_more = offset > 0;
    let right_more = end < state.items.len();
    let arrow = Style::default().fg(t.accent);
    if left_more {
        f.render_widget(Paragraph::new(Span::styled("‹", arrow)), Rect { x: inner.x, y: inner.y + card_h / 2, width: 1, height: 1 });
    }
    if right_more {
        f.render_widget(Paragraph::new(Span::styled("›", arrow)), Rect { x: inner.x + inner.width - 1, y: inner.y + card_h / 2, width: 1, height: 1 });
    }
    let used = (end - offset) as u16 * (card_w + 1);
    let start_x = inner.x + 2 + avail.saturating_sub(used.saturating_sub(1)) / 2; // center the cards
    for (idx, anime) in state.items[offset..end].iter().enumerate() {
        let x = start_x + (idx as u16) * (card_w + 1);
        if x + card_w > inner.x + inner.width.saturating_sub(1) {
            break;
        }
        let selected = offset + idx == state.selected && focused;
        render_card(f, anime, Rect { x, y: inner.y, width: card_w, height: card_h }, selected, cache, posters, t);
    }
    if state.items.len() > visible {
        let bar = Rect { x: inner.x + 2, y: inner.y + inner.height - 1, width: inner.width.saturating_sub(4), height: 1 };
        ThemedScrollbar { total: state.items.len(), visible, offset, focused }.render(f, bar, t);
    }
}

fn render_card(f: &mut Frame, anime: &Anime, area: Rect, selected: bool, cache: &ImageCache, posters: &PosterRenderer, t: Theme) {
    let border_style = if selected { Style::default().fg(t.border_focus).add_modifier(Modifier::BOLD) } else { Style::default().fg(t.border) };
    let bg = if selected { t.surface_hover } else { t.surface };
    let block = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(border_style).style(Style::default().bg(bg)).padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 2 {
        return;
    }
    let poster_h = inner.height.saturating_sub(TEXT_LINES);
    let drew_poster = poster_h >= 3
        && inner.width >= 8
        && anime.cover_image.as_deref().is_some_and(|url| cache.is_cached(url) && posters.render(f, Rect { height: poster_h, ..inner }, url, cache));
    if poster_h >= 3 && !drew_poster {
        crate::frontend::ui::hero::render_cover_placeholder(f, Rect { height: poster_h, ..inner }, anime.cover_image.is_some(), t);
    }

    let title_style = if selected { Style::default().fg(t.text).add_modifier(Modifier::BOLD) } else { Style::default().fg(t.text_dim) };
    let title = cozy_truncate(&anime.title, inner.width as usize);
    let score = anime.score.map(|s| format!("★ {s}")).unwrap_or_else(|| "☆ —".to_string());
    let year = anime.year.map(|y| y.to_string()).unwrap_or_default();
    let ep = anime.episodes.filter(|e| *e > 0).map(|e| format!("{e} eps")).unwrap_or_default();
    let lines = vec![
        Line::from(Span::styled(title, title_style)),
        Line::from(Span::styled("─".repeat((inner.width as usize).min(12)), Style::default().fg(t.border))),
        Line::from(vec![Span::styled(score, Style::default().fg(t.accent)), Span::raw("  "), Span::styled(year, Style::default().fg(t.text_muted))]),
        Line::from(Span::styled(ep, Style::default().fg(t.text_muted))),
    ];
    let text_area = if poster_h >= 3 { Rect { y: inner.y + poster_h, height: inner.height - poster_h, ..inner } } else { inner };
    f.render_widget(Paragraph::new(lines), text_area);
}

fn cozy_truncate(s: &str, max: usize) -> String {
    let max = max.max(4);
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max { return s.to_string(); }
    let mut cut = max.saturating_sub(1);
    while cut > max.saturating_sub(6) && cut < chars.len() && chars[cut] != ' ' { cut -= 1; }
    if cut <= 4 { cut = max - 1; }
    let mut t: String = chars[..cut].iter().collect(); t = t.trim_end().to_string(); t.push('…'); t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::ui::testkit::anime;

    fn row(n: usize) -> CarouselState {
        CarouselState::new("t", (0..n).map(|i| anime(&format!("show {i}"))).collect())
    }

    #[test]
    fn scroll_stays_in_bounds() {
        let mut c = row(3);
        c.scroll_left();
        assert_eq!(c.selected, 0);
        for _ in 0..10 { c.scroll_right(); }
        assert_eq!(c.selected, 2);
        let mut empty = row(0);
        empty.scroll_right();
        assert_eq!(empty.selected, 0);
    }

    #[test]
    fn set_items_resets_cursor_and_loading() {
        let mut c = row(8);
        c.loading = true;
        for _ in 0..7 { c.scroll_right(); }
        c.set_items(row(2).items);
        assert_eq!((c.selected, c.offset, c.loading), (0, 0, false));
    }

    #[test]
    fn card_width_grows_with_height() {
        assert_eq!(card_width_for(6), MIN_CARD_W); // no room for a poster
        assert!(card_width_for(20) > card_width_for(12));
        assert_eq!(card_width_for(60), MAX_CARD_W);
    }

    #[test]
    fn truncate_is_unicode_safe() {
        assert_eq!(cozy_truncate("short", 10), "short");
        let t = cozy_truncate("葬送のフリーレン 第2期 スペシャル", 8);
        assert!(t.ends_with('…') && t.chars().count() <= 8);
    }
}
