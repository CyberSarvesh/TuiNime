//! Screens — one file per view. `ui::render` draws `browse` as the base layer and the
//! current view's popup (if any) on top. The view state machine is `state::AppView`.

pub mod browse;
pub mod detail;
pub mod help;
pub mod player;
pub mod search;
