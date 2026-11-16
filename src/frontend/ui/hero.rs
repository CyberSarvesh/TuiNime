//! Hero — the large panel at the top of browse showing the highlighted show.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding, Paragraph, Wrap},
    Frame,
};

use crate::backend::cache::ImageCache;
use crate::backend::providers::traits::Anime;
use crate::frontend::theme::Theme;
use crate::frontend::widgets::meta;
use crate::frontend::widgets::poster::PosterRenderer;

/// The show the hero panel describes; `None` until something loads.
#[derive(Debug, Clone, Default)]
pub struct HeroState {
    pub anime: Option<Anime>,
}

impl HeroState {
    pub fn from_anime(anime: &Anime) -> Self {
        Self { anime: Some(anime.clone()) }
    }
    pub fn poster_url(&self) -> Option<&str> {
        self.anime.as_ref().and_then(|a| a.cover_image.as_deref())
    }
}

/// Poster width (cells) that keeps a 2:3 cover undistorted at `height` rows
/// (terminal cells are about twice as tall as wide).
pub fn poster_width_for(height: u16) -> u16 {
    height.saturating_mul(4) / 3
}

pub fn render(f: &mut Frame, hero: &HeroState, area: Rect, cache: &ImageCache, posters: &PosterRenderer, t: Theme) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.surface).fg(t.text))
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width < 20 || inner.height < 3 {
        return;
    }

    let Some(anime) = &hero.anime else {
        let lines = vec![
            Line::from(Span::styled("welcome home", Style::default().fg(t.accent).add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from(Span::styled("trending, seasonal, and whatever you're in the mood for.", Style::default().fg(t.text_dim))),
        ];
        f.render_widget(Paragraph::new(lines), inner);
        return;
    };

    let cover_w = poster_width_for(inner.height).clamp(8, inner.width / 3);
    let cover = Rect { width: cover_w, ..inner };
    let text = Rect { x: inner.x + cover_w + 2, width: inner.width.saturating_sub(cover_w + 2), ..inner };

    let drew = hero.poster_url().is_some_and(|u| posters.render(f, cover, u, cache));
    if !drew {
        render_cover_placeholder(f, cover, hero.poster_url().is_some(), t);
    }

    let mut lines = vec![
        Line::from(Span::styled(anime.title.clone(), Style::default().fg(t.accent).add_modifier(Modifier::BOLD))),
        meta::meta_line(anime, t),
    ];
    if !anime.genres.is_empty() {
        lines.push(meta::genre_line(anime, t));
    }
    lines.push(Line::from(""));
    let desc = anime.description.as_deref().unwrap_or("no description yet.").replace('\n', " ");
    lines.push(Line::from(Span::styled(desc, Style::default().fg(t.text_dim))));
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), text);
}

/// Framed box shown where a poster will appear (or "no cover" when there is none).
pub fn render_cover_placeholder(f: &mut Frame, area: Rect, loading: bool, t: Theme) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.surface_hover));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let label = if loading { "loading…" } else { "no cover" };
    let y = inner.y + inner.height / 2;
    f.render_widget(
        Paragraph::new(Span::styled(label, Style::default().fg(t.text_muted).add_modifier(Modifier::ITALIC))).centered(),
        Rect { y, height: 1, ..inner },
    );
}
