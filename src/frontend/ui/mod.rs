//! Layout manager — now just delegates to `screens` and `theme`.
//! All colors come from `app.theme` (data-defined). This file no longer hardcodes cozy palette.

pub mod carousel;
pub mod hero;

use ratatui::Frame;
use crate::frontend::app::{App, AppView};
use crate::frontend::screens;

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = app.theme;
    // Browse is the base separate screen; overlays sit on top — see screens/mod.rs docs.
    screens::browse::render(f, app, theme);
    match app.view {
        AppView::Search => screens::search::render(f, app, area, theme),
        AppView::Detail => screens::detail::render(f, app, area, theme),
        AppView::Player => screens::player::render(f, app, area, theme),
        AppView::Browse => {}
    }
}
