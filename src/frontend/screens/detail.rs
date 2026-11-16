//! Details — full screen: poster on the left; info, description and episodes on the right.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding, Paragraph, Wrap},
    Frame,
};

use crate::frontend::app::App;
use crate::frontend::state::{add_hit, spinner_frame, DetailState, Episodes, HitTarget, Hits};
use crate::frontend::theme::Theme;
use crate::frontend::ui::hero::{poster_width_for, render_cover_placeholder};
use crate::frontend::widgets::meta;

/// Width of one cell in the episode grid, e.g. `  12  `.
const EP_CELL_W: u16 = 7;

pub fn render(f: &mut Frame, app: &App, area: Rect, t: Theme) {
    let Some(d) = &app.detail else { return };
    let area = Rect { x: area.x + 1, width: area.width.saturating_sub(2), ..area };
    if area.width < 10 || area.height < 4 {
        return;
    }

    // poster column only when there is room for it next to the text
    let poster_w = if area.width >= 70 { poster_width_for(area.height).clamp(14, area.width / 4) } else { 0 };
    if poster_w > 0 {
        let poster_h = (poster_w * 3 / 4).min(area.height);
        let cover = Rect { width: poster_w, height: poster_h, ..area };
        let url = d.anime.cover_image.as_deref();
        if !url.is_some_and(|u| app.posters.render(f, cover, u, &app.backend.cache)) {
            render_cover_placeholder(f, cover, url.is_some(), t);
        }
    }
    let gap = if poster_w > 0 { 2 } else { 0 };
    let right = Rect { x: area.x + poster_w + gap, width: area.width - poster_w - gap, ..area };
    render_info(f, d, right, app, t);
}

fn render_info(f: &mut Frame, d: &DetailState, area: Rect, app: &App, t: Theme) {
    let a = &d.anime;
    let mut head = vec![
        Line::from(Span::styled(a.title.clone(), Style::default().fg(t.accent).add_modifier(Modifier::BOLD))),
        meta::meta_line(a, t),
    ];
    if !a.genres.is_empty() {
        head.push(meta::genre_line(a, t));
    }
    if !a.alt_titles.is_empty() {
        let aka = a.alt_titles.iter().take(2).cloned().collect::<Vec<_>>().join(" / ");
        head.push(Line::from(Span::styled(format!("also known as {aka}"), Style::default().fg(t.text_muted).add_modifier(Modifier::ITALIC))));
    }
    let head_h = (head.len() as u16).min(area.height);
    f.render_widget(Paragraph::new(head), Rect { height: head_h, ..area });

    let rest = Rect { y: area.y + head_h + 1, height: area.height.saturating_sub(head_h + 1), ..area };
    if rest.height < 3 {
        return;
    }
    // description takes the lines it needs (up to a third of what's left); episodes get the rest
    let needed = wrapped_lines(desc_text(d), rest.width) as u16;
    let desc_h = needed.clamp(1, (rest.height / 3).clamp(2, 10)).min(rest.height.saturating_sub(4));
    let desc_area = Rect { height: desc_h, ..rest };
    render_description(f, d, desc_area, t);

    let eps_area = Rect { y: rest.y + desc_h + 1, height: rest.height.saturating_sub(desc_h + 1), ..rest };
    render_episodes(f, d, eps_area, app.dub, app.tick, &app.hits, t);
}

fn desc_text(d: &DetailState) -> &str {
    d.anime.description.as_deref().unwrap_or("no description yet.")
}

/// Rough wrapped line count (so `]` stops at the end); exact wrapping is ratatui's job.
fn wrapped_lines(text: &str, width: u16) -> usize {
    let w = width.max(1) as usize;
    text.lines().map(|l| l.chars().count().div_ceil(w).max(1)).sum()
}

fn render_description(f: &mut Frame, d: &DetailState, area: Rect, t: Theme) {
    if area.height == 0 {
        return;
    }
    let text = desc_text(d);
    let max = (wrapped_lines(text, area.width) as u16).saturating_sub(area.height);
    d.desc_max.set(max);
    let scroll = d.desc_scroll.min(max);
    let p = Paragraph::new(text.to_string()).style(Style::default().fg(t.text_dim)).wrap(Wrap { trim: true }).scroll((scroll, 0));
    f.render_widget(p, area);
    if max > 0 {
        let more = if scroll < max { "▼ ] more" } else { "▲ [ back" };
        let tag = Paragraph::new(Span::styled(more, Style::default().fg(t.text_muted))).right_aligned();
        f.render_widget(tag, Rect { y: area.y + area.height - 1, height: 1, ..area });
    }
}

/// " listed as Sousou no Frieren " when the provider's name differs from AniList's.
fn listed_as(d: &DetailState) -> String {
    match &d.episodes {
        Episodes::Loaded(list) if !list.show.eq_ignore_ascii_case(&d.anime.title) => format!(" from {} as {} ", list.source, list.show),
        Episodes::Loaded(list) => format!(" from {} ", list.source),
        _ => String::new(),
    }
}

fn render_episodes(f: &mut Frame, d: &DetailState, area: Rect, dub: bool, tick: u64, hits: &Hits, t: Theme) {
    if area.height < 3 {
        return;
    }
    let lang = if dub { "dub" } else { "sub" };
    let count = d.episode_count();
    let title = match &d.episodes {
        Episodes::Loaded(_) => format!(" episodes · {lang} · {count} "),
        _ => format!(" episodes · {lang} "),
    };
    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(t.accent).add_modifier(Modifier::BOLD)))
        .title_bottom(Line::from(Span::styled(format!(" s: switch to {} ", if dub { "sub" } else { "dub" }), Style::default().fg(t.text_muted))).right_aligned())
        .title_bottom(Line::from(Span::styled(listed_as(d), Style::default().fg(t.text_muted))))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border_focus))
        .style(Style::default().bg(t.surface))
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    let center = |f: &mut Frame, line: Line| f.render_widget(Paragraph::new(line).centered(), Rect { y: inner.y + inner.height / 2, height: 1, ..inner });

    let list = match &d.episodes {
        Episodes::Loading => {
            center(f, Line::from(Span::styled(format!("{} finding episodes…", spinner_frame(tick)), Style::default().fg(t.text_muted))));
            return;
        }
        Episodes::Failed(e) => {
            let msg = Paragraph::new(vec![
                Line::from(Span::styled("couldn't load episodes", Style::default().fg(t.error).add_modifier(Modifier::BOLD))),
                Line::from(Span::styled(e.clone(), Style::default().fg(t.text_dim))),
                Line::from(""),
                Line::from(Span::styled("r to retry  ·  s to try the other language", Style::default().fg(t.text_muted))),
            ])
            .centered()
            .wrap(Wrap { trim: true });
            f.render_widget(msg, inner);
            return;
        }
        Episodes::Loaded(list) => list,
    };

    let cols = ((inner.width / EP_CELL_W) as usize).max(1);
    d.grid_cols.set(cols);
    // last line shows which episode is highlighted
    let grid_h = inner.height.saturating_sub(1).max(1) as usize;
    let sel_row = d.selected / cols;
    let first_row = sel_row.saturating_sub(grid_h - 1);
    for row in 0..grid_h {
        let start = (first_row + row) * cols;
        if start >= list.episodes.len() {
            break;
        }
        let spans: Vec<Span> = list.episodes[start..(start + cols).min(list.episodes.len())]
            .iter()
            .enumerate()
            .map(|(i, ep)| {
                let label = format!("{:^w$}", ep.label, w = EP_CELL_W as usize - 1);
                let style = if start + i == d.selected {
                    Style::default().fg(t.bg).bg(t.accent).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(t.text_dim)
                };
                Span::styled(format!("{label} "), style)
            })
            .collect();
        let y = inner.y + row as u16;
        f.render_widget(Paragraph::new(Line::from(spans)), Rect { y, height: 1, ..inner });
        for i in start..(start + cols).min(list.episodes.len()) {
            let x = inner.x + ((i - start) as u16) * EP_CELL_W;
            add_hit(hits, Rect { x, y, width: EP_CELL_W - 1, height: 1 }, HitTarget::Episode(i));
        }
    }
    if inner.height >= 2 {
        let ep = &list.episodes[d.selected.min(list.episodes.len() - 1)];
        let more = if (first_row + grid_h) * cols < list.episodes.len() { "  ▼ more below" } else { "" };
        let status = Line::from(vec![
            Span::styled(format!("▶ episode {}", ep.label), Style::default().fg(t.accent_green).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  ·  ⏎ to play{more}"), Style::default().fg(t.text_muted)),
        ]);
        f.render_widget(Paragraph::new(status), Rect { y: inner.y + inner.height - 1, height: 1, ..inner });
    }
}
