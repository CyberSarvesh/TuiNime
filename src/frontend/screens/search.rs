//! Search — full screen: input box on top, live results (or recent searches) below.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding, Paragraph},
    Frame,
};

use crate::backend::Anime;
use crate::frontend::app::App;
use crate::frontend::state::{add_hit, spinner_frame, HitTarget, SearchResults};
use crate::frontend::theme::Theme;
use crate::frontend::ui::hero::render_cover_placeholder;
use crate::frontend::widgets::meta;

/// Height of one result row (poster thumbnail + three text lines).
const RESULT_H: u16 = 4;
const THUMB_W: u16 = 6;

pub fn render(f: &mut Frame, app: &App, area: Rect, t: Theme) {
    let area = Rect { x: area.x + 1, width: area.width.saturating_sub(2), ..area };
    if area.height < 4 || area.width < 10 {
        return;
    }
    render_input(f, app, Rect { height: 3, ..area }, t);
    let list = Rect { y: area.y + 4, height: area.height.saturating_sub(4), ..area };
    match &app.search.results {
        SearchResults::Idle => render_recent(f, app, list, t),
        SearchResults::Loading => message(f, list, format!("{} searching…", spinner_frame(app.tick)), t.text_muted),
        SearchResults::Failed(e) => message(f, list, format!("search failed: {e}"), t.error),
        SearchResults::Loaded(r) if r.is_empty() => message(f, list, format!("no shows match \"{}\"", app.search.sent), t.text_muted),
        SearchResults::Loaded(r) => render_results(f, app, r, list, t),
    }
}

fn render_input(f: &mut Frame, app: &App, area: Rect, t: Theme) {
    let block = Block::default()
        .title(Span::styled(" ⌕ search ", Style::default().fg(t.accent).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border_focus))
        .style(Style::default().bg(t.surface))
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let line = if app.search.query.is_empty() {
        Line::from(vec![
            Span::styled("▌", Style::default().fg(t.accent)),
            Span::styled("type a show name…", Style::default().fg(t.text_muted).add_modifier(Modifier::ITALIC)),
        ])
    } else {
        Line::from(vec![
            Span::styled(app.search.query.clone(), Style::default().fg(t.text)),
            Span::styled("▌", Style::default().fg(t.accent)),
        ])
    };
    f.render_widget(Paragraph::new(line), inner);
}

fn message(f: &mut Frame, area: Rect, text: String, color: ratatui::style::Color) {
    if area.height == 0 {
        return;
    }
    f.render_widget(Paragraph::new(Span::styled(text, Style::default().fg(color))).centered(), Rect { y: area.y + area.height / 3, height: 1, ..area });
}

fn render_recent(f: &mut Frame, app: &App, area: Rect, t: Theme) {
    if app.recent_searches.is_empty() {
        message(f, area, "results appear as you type".into(), t.text_muted);
        return;
    }
    let mut lines = vec![Line::from(Span::styled("recent searches", Style::default().fg(t.text_muted).add_modifier(Modifier::BOLD))), Line::from("")];
    for (i, q) in app.recent_searches.iter().enumerate() {
        let selected = i == app.search.selected;
        let (bar, style) = if selected {
            ("▌ ", Style::default().fg(t.text).bg(t.surface_hover).add_modifier(Modifier::BOLD))
        } else {
            ("  ", Style::default().fg(t.text_dim))
        };
        lines.push(Line::from(vec![Span::styled(bar, Style::default().fg(t.accent)), Span::styled(format!("↺ {q}"), style)]));
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn render_results(f: &mut Frame, app: &App, results: &[Anime], area: Rect, t: Theme) {
    let fit = (area.height / RESULT_H).max(1) as usize;
    let first = app.search.selected.saturating_sub(fit - 1);
    for (row, anime) in results.iter().enumerate().skip(first).take(fit) {
        let y = area.y + ((row - first) as u16) * RESULT_H;
        let rect = Rect { y, height: RESULT_H.min(area.y + area.height - y), ..area };
        render_result(f, app, anime, rect, row == app.search.selected, t);
        add_hit(&app.hits, rect, HitTarget::SearchResult(row));
    }
    let shown = format!(" {}/{} ", app.search.selected + 1, results.len());
    f.render_widget(
        Paragraph::new(Span::styled(shown, Style::default().fg(t.text_muted))).right_aligned(),
        Rect { y: area.y.saturating_sub(1), height: 1, ..area },
    );
}

fn render_result(f: &mut Frame, app: &App, anime: &Anime, area: Rect, selected: bool, t: Theme) {
    let bg = if selected { t.surface_hover } else { t.bg };
    f.render_widget(Block::default().style(Style::default().bg(bg)), area);
    if selected {
        for dy in 0..area.height.saturating_sub(1) {
            f.render_widget(Paragraph::new(Span::styled("▌", Style::default().fg(t.accent))), Rect { y: area.y + dy, width: 1, height: 1, ..area });
        }
    }
    let thumb = Rect { x: area.x + 2, width: THUMB_W, height: area.height.saturating_sub(1), ..area };
    let url = anime.cover_image.as_deref();
    if !url.is_some_and(|u| app.backend.cache.is_cached(u) && app.posters.render(f, thumb, u, &app.backend.cache)) {
        render_cover_placeholder(f, thumb, false, t);
    }
    let text = Rect { x: thumb.x + THUMB_W + 2, width: area.width.saturating_sub(THUMB_W + 4), height: area.height.saturating_sub(1), ..area };
    let title_style = if selected { Style::default().fg(t.accent).add_modifier(Modifier::BOLD) } else { Style::default().fg(t.text) };
    let lines = vec![
        Line::from(Span::styled(anime.title.clone(), title_style)),
        meta::meta_line(anime, t),
        meta::genre_line(anime, t),
    ];
    f.render_widget(Paragraph::new(lines), text);
}
