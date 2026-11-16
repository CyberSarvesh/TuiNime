//! Show metadata lines shared by the hero panel and the details screen.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::backend::Anime;
use crate::frontend::theme::Theme;

/// AniList status enum → words people use.
pub fn pretty_status(status: &str) -> &'static str {
    match status {
        "RELEASING" => "airing",
        "FINISHED" => "finished",
        "NOT_YET_RELEASED" => "upcoming",
        "CANCELLED" => "cancelled",
        "HIATUS" => "on hiatus",
        _ => "",
    }
}

/// `2026  ★ 76  12 eps  airing` — only the parts that are known.
pub fn meta_line(anime: &Anime, t: Theme) -> Line<'static> {
    let mut parts: Vec<Span<'static>> = Vec::new();
    let mut push = |s: String, style: Style| {
        if !parts.is_empty() {
            parts.push(Span::styled("  ·  ", Style::default().fg(t.text_muted)));
        }
        parts.push(Span::styled(s, style));
    };
    if let Some(y) = anime.year {
        push(y.to_string(), Style::default().fg(t.text_dim));
    }
    if let Some(s) = anime.score {
        push(format!("★ {s}"), Style::default().fg(t.accent).add_modifier(Modifier::BOLD));
    }
    if let Some(e) = anime.episodes.filter(|e| *e > 0) {
        push(format!("{e} eps"), Style::default().fg(t.text_dim));
    }
    if let Some(st) = anime.status.as_deref().map(pretty_status).filter(|s| !s.is_empty()) {
        let color = if st == "airing" { t.accent_green } else { t.text_dim };
        push(st.to_string(), Style::default().fg(color));
    }
    Line::from(parts)
}

/// Genres as `Action · Fantasy · Comedy`, or an empty line when unknown.
pub fn genre_line(anime: &Anime, t: Theme) -> Line<'static> {
    Line::from(Span::styled(anime.genres.join(" · "), Style::default().fg(t.text_muted)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_line_skips_unknown_parts() {
        let mut a = crate::frontend::ui::testkit::anime("x");
        a.year = Some(2026);
        a.score = None;
        a.episodes = Some(12);
        a.status = Some("RELEASING".into());
        let text: String = meta_line(&a, Theme::cozy()).spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "2026  ·  12 eps  ·  airing");
    }
}
