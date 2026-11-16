//! Help — popup listing every key, opened with `?` (F1 on the search screen).

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::frontend::layout;
use crate::frontend::theme::Theme;

/// (section, [(keys, what they do)])
pub const SECTIONS: &[(&str, &[(&str, &str)])] = &[
    ("browse", &[
        ("← → / h l", "move along a row"),
        ("↑ ↓ / k j", "switch rows"),
        ("enter", "open details"),
        ("p", "play episode 1"),
        ("/", "search"),
        ("t", "next theme"),
        ("q", "quit"),
    ]),
    ("details", &[
        ("arrows / hjkl", "pick an episode"),
        ("pgup pgdn  g G", "jump / first / last"),
        ("enter / p", "play the episode"),
        ("s", "switch sub ↔ dub"),
        ("[ ]", "scroll the description"),
        ("r", "retry loading episodes"),
        ("esc", "back"),
    ]),
    ("search", &[
        ("type", "results appear as you type"),
        ("↑ ↓ / tab", "pick a result"),
        ("enter", "open it (or re-run a recent search)"),
        ("esc", "clear, then leave"),
        ("F1", "this help"),
    ]),
    ("anywhere", &[
        ("mouse wheel", "scroll rows"),
        ("?", "this help"),
        ("ctrl+c", "quit"),
    ]),
];

pub fn render(f: &mut Frame, area: Rect, t: Theme) {
    let key_w = SECTIONS.iter().flat_map(|(_, keys)| keys.iter()).map(|(k, _)| k.chars().count()).max().unwrap_or(8);
    let mut lines = Vec::new();
    for (i, (section, keys)) in SECTIONS.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(*section, Style::default().fg(t.accent).add_modifier(Modifier::BOLD))));
        for (k, what) in keys.iter() {
            lines.push(Line::from(vec![
                Span::styled(format!("  {k:<key_w$}  "), Style::default().fg(t.text).add_modifier(Modifier::BOLD)),
                Span::styled(*what, Style::default().fg(t.text_dim)),
            ]));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("esc or ? to close", Style::default().fg(t.text_muted).add_modifier(Modifier::ITALIC))));

    let h = (lines.len() as u16 + 2).min(area.height);
    let inner = layout::popup(f, layout::centered(area, 66, h), t, "keys", t.border_focus);
    f.render_widget(Paragraph::new(lines), inner);
}
