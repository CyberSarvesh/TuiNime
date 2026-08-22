//! Themed selector — vertical list with theme-driven highlight.
//! Used by future theme-picker, episode list, etc.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::frontend::theme::Theme;

pub struct SelectorItem {
    pub label: String,
    pub selected: bool,
}

pub struct Selector {
    pub items: Vec<SelectorItem>,
    pub focused: bool,
}

impl Selector {
    pub fn new(labels: Vec<String>, selected_idx: usize) -> Self {
        let items = labels.into_iter().enumerate().map(|(i, l)| SelectorItem { label: l, selected: i == selected_idx }).collect();
        Self { items, focused: true }
    }

    pub fn render(self, f: &mut Frame, area: Rect, theme: Theme) {
        let sel = theme.selector;
        let lines: Vec<Line> = self
            .items
            .into_iter()
            .map(|it| {
                if it.selected {
                    Line::from(vec![
                        Span::styled(sel.indicator, Style::default().fg(sel.indicator_color).add_modifier(Modifier::BOLD)),
                        Span::styled(it.label, Style::default().fg(sel.selected_fg).bg(sel.selected_bg).add_modifier(Modifier::BOLD)),
                    ])
                } else {
                    Line::from(Span::styled(format!("  {}", it.label), Style::default().fg(theme.text_dim)))
                }
            })
            .collect();
        f.render_widget(Paragraph::new(lines), area);
    }
}
