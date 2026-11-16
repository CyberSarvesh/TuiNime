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
use crate::frontend::widgets::poster::PosterRenderer;
use crate::frontend::widgets::scrollbar::ThemedScrollbar;

#[derive(Debug, Clone)]
pub struct CarouselState {
    pub title: String,
    pub items: Vec<Anime>,
    pub selected: usize,
    pub offset: usize,
}

impl CarouselState {
    pub fn new(title: impl Into<String>, items: Vec<Anime>) -> Self { Self { title: title.into(), items, selected: 0, offset: 0 } }
    pub fn selected_item(&self) -> Option<&Anime> { self.items.get(self.selected) }
    /// Replace the row's contents and reset the cursor to the first item.
    pub fn set_items(&mut self, items: Vec<Anime>) { self.items = items; self.selected = 0; self.offset = 0; }
    pub fn scroll_right(&mut self) { if self.items.is_empty() { return; } if self.selected + 1 < self.items.len() { self.selected += 1; if self.selected >= self.offset + 5 { self.offset += 1; } } }
    pub fn scroll_left(&mut self) { if self.selected > 0 { self.selected -= 1; if self.selected < self.offset { self.offset = self.selected; } } }
}

pub fn render(f: &mut Frame, state: &CarouselState, area: Rect, focused: bool, cache: &ImageCache, posters: &PosterRenderer, theme: Theme) {
    let border_style = if focused { Style::default().fg(theme.border_focus) } else { Style::default().fg(theme.border) };
    let title_style = if focused { Style::default().fg(theme.accent).add_modifier(Modifier::BOLD) } else { Style::default().fg(theme.text_dim) };
    let block = Block::default().title(format!("  {}  ", state.title.to_lowercase())).title_style(title_style)
        .borders(Borders::ALL).border_type(BorderType::Rounded).border_style(border_style)
        .style(Style::default().bg(theme.surface)).padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 { return; }
    if state.items.is_empty() {
        let msg = if state.title.to_lowercase().contains("continue") { "  ๋࣭  your list is empty — maybe rewatch something cozy?  " } else { "  — nothing here yet —  " };
        let p = Paragraph::new(Line::from(Span::styled(msg, Style::default().fg(theme.text_muted).add_modifier(Modifier::ITALIC)))).centered();
        let y = inner.y + inner.height / 2;
        let centered_area = Rect { x: inner.x, y, width: inner.width, height: 1 };
        f.render_widget(p, centered_area); return;
    }
    let card_w: u16 = 20;
    // leave the last row for the scrollbar; cards grow with the row so posters have room
    let card_h: u16 = inner.height.saturating_sub(1).min(16);
    if card_h < 3 { return; }
    let visible = (inner.width.saturating_sub(2) / (card_w + 1)).max(1) as usize;
    // `scroll_*` doesn't know the screen width, so clamp here to keep the selection on screen
    let offset = if state.selected >= state.offset + visible { state.selected + 1 - visible } else { state.offset.min(state.selected) };
    let end = (offset + visible).min(state.items.len());
    let left_more = offset > 0;
    let right_more = end < state.items.len();
    if left_more { let s = Paragraph::new(Span::styled("‹", Style::default().fg(theme.text_muted))).centered(); f.render_widget(s, Rect { x: inner.x, y: inner.y, width: 1, height: 1 }); }
    if right_more { let s = Paragraph::new(Span::styled("›", Style::default().fg(theme.text_muted))).centered(); f.render_widget(s, Rect { x: inner.x + inner.width - 1, y: inner.y, width: 1, height: 1 }); }
    let start_x = inner.x + if left_more { 2 } else { 0 };
    for (idx, anime) in state.items[offset..end].iter().enumerate() {
        let abs_idx = offset + idx;
        let is_selected = abs_idx == state.selected && focused;
        let x = start_x + (idx as u16) * (card_w + 1);
        if x + card_w > inner.x + inner.width { break; }
        let card_area = Rect { x, y: inner.y, width: card_w, height: card_h };
        render_card(f, anime, card_area, is_selected, cache, posters, theme);
    }
    // themed scrollbar — data-driven via Theme::scrollbar (future picker will reuse same colors)
    if state.items.len() > visible {
        let bar_area = Rect { x: inner.x, y: inner.y + inner.height.saturating_sub(1), width: inner.width, height: 1 };
        ThemedScrollbar { total: state.items.len(), visible, offset, focused }.render(f, bar_area, theme);
    }
}

const TEXT_LINES: u16 = 4; // title, rule, score/year, episodes

fn render_card(f: &mut Frame, anime: &Anime, area: Rect, selected: bool, cache: &ImageCache, posters: &PosterRenderer, t: Theme) {
    let border_style = if selected { Style::default().fg(t.border_focus).add_modifier(Modifier::BOLD) } else { Style::default().fg(t.border) };
    let bg = if selected { t.surface_hover } else { t.surface };
    let block = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(border_style).style(Style::default().bg(bg)).padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 2 { return; }
    let has_poster = anime.cover_image.as_deref().map(|u| cache.is_cached(u)).unwrap_or(false);
    // the poster gets whatever height is left above the text, if that is at least 3 rows
    let poster_h = inner.height.saturating_sub(TEXT_LINES);
    let drew_poster = has_poster && poster_h >= 3 && inner.width >= 8 && anime.cover_image.as_deref()
        .is_some_and(|url| posters.render(f, Rect { height: poster_h, ..inner }, url, cache));
    let indicator = if has_poster { "●" } else if anime.cover_image.is_some() { "○" } else { " " };
    let indicator_style = if has_poster { Style::default().fg(t.accent) } else { Style::default().fg(t.text_muted) };
    let title = cozy_truncate(&anime.title, (inner.width as usize).saturating_sub(2));
    let title_line = Line::from(vec![Span::styled(indicator, indicator_style), Span::raw(" "), Span::styled(title, if selected { Style::default().fg(t.text).add_modifier(Modifier::BOLD) } else { Style::default().fg(t.text_dim) })]);
    let score = anime.score.map(|s| format!("★ {s}")).unwrap_or_else(|| "☆ —".to_string());
    let year = anime.year.map(|y| y.to_string()).unwrap_or_else(|| "—".to_string());
    let ep = anime.episodes.map(|e| format!("{e} eps")).unwrap_or_default();
    let meta_line = Line::from(vec![Span::styled(score, Style::default().fg(t.accent)), Span::raw("  "), Span::styled(year, Style::default().fg(t.text_muted))]);
    let ep_line = if ep.is_empty() { Line::from("") } else { Line::from(Span::styled(ep, Style::default().fg(t.text_muted))) };
    let lines = vec![title_line, Line::from(Span::styled("─".repeat((inner.width as usize).min(12)), Style::default().fg(t.border))), meta_line, ep_line];
    let p = Paragraph::new(lines);
    let text_area = if drew_poster { Rect { y: inner.y + poster_h, height: inner.height - poster_h, ..inner } } else { inner };
    f.render_widget(p, text_area);
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

    fn row(n: usize) -> CarouselState {
        let items = (0..n).map(|i| Anime {
            id: i.to_string(), title: format!("show {i}"), description: None, cover_image: None,
            banner_image: None, score: None, year: None, episodes: None, status: None,
            genres: vec![], alt_titles: vec![],
        }).collect();
        CarouselState::new("t", items)
    }

    #[test]
    fn scroll_stays_in_bounds() {
        let mut c = row(3);
        c.scroll_left();
        assert_eq!(c.selected, 0);
        for _ in 0..10 { c.scroll_right(); }
        assert_eq!(c.selected, 2);
    }

    #[test]
    fn set_items_resets_cursor() {
        let mut c = row(8);
        for _ in 0..7 { c.scroll_right(); }
        c.set_items(row(2).items);
        assert_eq!((c.selected, c.offset), (0, 0));
    }

    #[test]
    fn truncate_is_unicode_safe() {
        assert_eq!(cozy_truncate("short", 10), "short");
        let t = cozy_truncate("葬送のフリーレン 第2期 スペシャル", 8);
        assert!(t.ends_with('…') && t.chars().count() <= 8);
    }
}
