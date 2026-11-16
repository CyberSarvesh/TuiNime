//! Search — input popup over browse.

use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::frontend::app::App;
use crate::frontend::layout;
use crate::frontend::theme::Theme;

pub fn render(f: &mut Frame, app: &App, area: Rect, t: Theme) {
    let inner = layout::popup(f, layout::centered(area, 60, 3), t, "⌕ search — enter to go • esc to close", t.border_focus);
    let input = Paragraph::new(Line::from(vec![
        Span::styled(app.search_query.clone(), Style::default().fg(t.text)),
        Span::styled("▌", Style::default().fg(t.accent)),
    ]));
    f.render_widget(input, inner);
}
