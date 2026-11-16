//! Player — small popup while mpv plays in its own window.

use ratatui::{layout::Rect, style::Style, widgets::Paragraph, Frame};

use crate::frontend::app::App;
use crate::frontend::layout;
use crate::frontend::theme::Theme;

pub fn render(f: &mut Frame, app: &App, area: Rect, t: Theme) {
    let title = format!("▶ {}", app.now_playing.as_deref().unwrap_or("playback"));
    let inner = layout::popup(f, layout::centered(area, 56, 3), t, &title, t.accent_green);
    let p = Paragraph::new("playing in mpv — esc to stop").style(Style::default().fg(t.text)).centered();
    f.render_widget(p, inner);
}
