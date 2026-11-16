//! App state — pure data, no IO. `App` in `app.rs` composes this.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppView {
    #[default]
    Browse,
    Detail,
    Search,
    Player,
}

/// How the status bar should present a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    /// Plain information.
    Info,
    /// Work in progress — drawn with a spinner.
    Busy,
    /// Something failed — drawn in the error color.
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub text: String,
    pub kind: StatusKind,
}

impl Status {
    pub fn info(text: impl Into<String>) -> Self { Self { text: text.into(), kind: StatusKind::Info } }
    pub fn busy(text: impl Into<String>) -> Self { Self { text: text.into(), kind: StatusKind::Busy } }
    pub fn error(text: impl Into<String>) -> Self { Self { text: text.into(), kind: StatusKind::Error } }
}

/// Braille spinner frames, advanced once per tick.
pub const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn spinner_frame(tick: u64) -> &'static str {
    SPINNER[(tick % SPINNER.len() as u64) as usize]
}
