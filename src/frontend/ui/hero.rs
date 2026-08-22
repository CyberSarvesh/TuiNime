use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding, Paragraph},
    Frame,
};

use crate::backend::cache::ImageCache;
use crate::backend::providers::traits::Anime;
use crate::frontend::theme::Theme;

#[derive(Debug, Clone)]
pub struct HeroState {
    pub title: String,
    pub description: String,
    pub score: Option<u8>,
    pub year: Option<u16>,
    pub poster_url: Option<String>,
}

impl Default for HeroState {
    fn default() -> Self { Self::placeholder() }
}
impl HeroState {
    pub fn placeholder() -> Self {
        Self {
            title: "welcome home".to_string(),
            description: "your cozy corner for anime — trending, seasonal, and whatever you’re in the mood for.".to_string(),
            score: None, year: None, poster_url: None,
        }
    }
    pub fn from_anime(anime: &Anime) -> Self {
        Self {
            title: anime.title.clone(),
            description: anime.description.clone().unwrap_or_else(|| "no description — just vibes.".to_string()),
            score: anime.score, year: anime.year, poster_url: anime.cover_image.clone(),
        }
    }
}

pub fn render(f: &mut Frame, hero: &HeroState, area: Rect, cache: &ImageCache, theme: Theme) {
    let block = Block::default()
        .borders(Borders::ALL).border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.surface).fg(theme.text))
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width < 10 || inner.height < 3 { return; }

    let cover_w = 16.min(inner.width / 4).max(12);
    let text_area = Rect { x: inner.x + cover_w + 2, y: inner.y, width: inner.width.saturating_sub(cover_w + 2), height: inner.height };
    let cover_area = Rect { x: inner.x, y: inner.y, width: cover_w, height: inner.height };

    let rendered_image = try_render_poster(f, cover_area, hero.poster_url.as_deref(), cache);
    if !rendered_image {
        let cover_block = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(110, 95, 78)))
            .style(Style::default().bg(Color::Rgb(42, 38, 32)));
        let cover_inner = cover_block.inner(cover_area);
        f.render_widget(cover_block, cover_area);
        let status = hero.poster_url.as_deref().map(|u| if cache.is_cached(u) { "● cached" } else { "○ loading…" }).unwrap_or("—");
        let cover_text = Paragraph::new(vec![
            Line::from(Span::raw("")),
            Line::from(Span::styled("  ✦  cover  ✦  ", Style::default().fg(theme.text_dim).add_modifier(Modifier::ITALIC))),
            Line::from(Span::styled(status, Style::default().fg(if status.contains('●') { theme.accent } else { theme.text_muted }))),
            Line::from(Span::styled("  ───  ", Style::default().fg(theme.text_muted))),
            Line::from(Span::styled("  cozy  ", Style::default().fg(theme.text_muted))),
        ]).centered().style(Style::default().fg(theme.text_dim));
        f.render_widget(cover_text, cover_inner);
    }

    let title_line = Line::from(vec![
        Span::styled(hero.title.clone(), Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(hero.year.map(|y| y.to_string()).unwrap_or_default(), Style::default().fg(theme.text_muted)),
        Span::raw("  "),
        Span::styled(hero.score.map(|s| format!("★ {s}")).unwrap_or_default(), Style::default().fg(theme.accent)),
    ]);
    let desc_raw = hero.description.replace('\n', " ").trim().to_string();
    let desc_trim = if desc_raw.chars().count() > 220 { let mut s: String = desc_raw.chars().take(217).collect(); s.push_str("…"); s } else { desc_raw };
    let desc_para = Paragraph::new(vec![
        title_line,
        Line::from(Span::styled("  ─────────────────────────────────────", Style::default().fg(theme.text_muted))),
        Line::from(Span::styled(desc_trim, Style::default().fg(theme.text_dim))),
        Line::from(Span::raw("")),
        Line::from(Span::styled("  enter details  •  p play  •  / search", Style::default().fg(theme.text_muted).add_modifier(Modifier::ITALIC))),
    ]).wrap(ratatui::widgets::Wrap { trim: true }).style(Style::default().bg(theme.surface));
    f.render_widget(desc_para, text_area);
}

fn try_render_poster(f: &mut Frame, area: Rect, url: Option<&str>, cache: &ImageCache) -> bool {
    let Some(u) = url else { return false; };
    let Some(img) = cache.try_load_cached_image(u) else { return false; };
    use ratatui_image::{picker::Picker, StatefulImage};
    let mut picker = Picker::from_fontsize((8, 16));
    let protocol = picker.new_resize_protocol(img);
    let mut state = protocol;
    f.render_stateful_widget(StatefulImage::new(None), area, &mut state);
    true
}
