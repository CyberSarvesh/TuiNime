//! Screens — logical separation, not separate apps.
//!
//! Each file is one screen. Frontend is single `App` with `AppView` state machine in `crate::frontend::state`.
//! - `browse`   — SEPARATE SCREEN (full layout: header + hero + carousels + footer). Base layer.
//! - `detail`   — OVERLAY (centered popup on top of browse, `Clear` + rounded block).
//! - `search`   — OVERLAY (same, input popup).
//! - `player`   — OVERLAY today (swap to SEPARATE by changing `KIND` in `player.rs` and `AppView::Player` routing).
//!
//! To add a scrollbar/selector (theme picker, episode list): add `settings.rs` or `episode.rs` here and a variant to `AppView`.
//! Theme is data-defined via `crate::frontend::theme::Theme` so screens never hardcode colors.
//!
//! `ScreenKind` + `Screen` trait make the overlay/separate contract explicit so `ui::render` can
//! decide `Clear` vs full takeover just by reading `KIND`.

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::frontend::app::App;
use crate::frontend::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenKind {
    Separate,
    Overlay,
}

pub trait Screen {
    const KIND: ScreenKind;
    const NAME: &'static str;
    fn render(f: &mut Frame, app: &App, area: Rect, theme: Theme);
}

pub mod browse;
pub mod detail;
pub mod player;
pub mod search;
