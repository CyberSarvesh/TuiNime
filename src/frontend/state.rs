//! App state — pure data, no IO. `App` in `app.rs` composes this.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppView {
    #[default]
    Browse, // SEPARATE SCREEN — owns full layout
    Detail, // OVERLAY
    Search, // OVERLAY
    Player, // OVERLAY today (swap to Separate by flipping `ScreenKind` in `screens/player.rs`)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Hero,
    Carousel,
    SearchInput,
}

impl Default for Focus {
    fn default() -> Self {
        Self::Carousel
    }
}
