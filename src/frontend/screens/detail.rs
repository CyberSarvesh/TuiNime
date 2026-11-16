//! Detail — popup over browse with the selected show's info.

use ratatui::{layout::Rect, style::Style, widgets::Paragraph, Frame};

use crate::frontend::app::App;
use crate::frontend::layout;
use crate::frontend::theme::Theme;

pub fn render(f: &mut Frame, app: &App, area: Rect, t: Theme) {
    let anime = app.carousels.get(app.active_carousel).and_then(|c| c.selected_item());
    let title = anime.map(|a| a.title.as_str()).unwrap_or("tuinime");
    let inner = layout::popup(f, layout::centered_pct(area, 70, 60), t, title, t.border_focus);
    let body = match anime {
        Some(a) => format!(
            "★ {}   •   {} eps   •   {}\n\n{}\n\np play   •   esc close",
            a.score.map(|s| s.to_string()).unwrap_or_else(|| "—".into()),
            a.episodes.map(|e| e.to_string()).unwrap_or_else(|| "—".into()),
            a.status.as_deref().unwrap_or("—"),
            a.description.as_deref().unwrap_or_default()
        ),
        None => "no selection".into(),
    };
    let p = Paragraph::new(body).style(Style::default().fg(t.text)).wrap(ratatui::widgets::Wrap { trim: true });
    f.render_widget(p, inner);
}
