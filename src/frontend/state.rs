//! App state — pure data, no IO. `App` in `app.rs` composes this.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppView {
    #[default]
    Browse,
    Detail,
    Search,
    Player,
}
