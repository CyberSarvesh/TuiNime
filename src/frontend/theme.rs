//! Data-defined theme path — single source of truth for colors.
//!
//! All frontend rendering reads from `Theme`. To add a new theme or a selector/scrollbar,
//! just add a variant to `ThemeKind` and a constructor here; no widget file needs to be touched.
//! This is the "data defined path across the thing" you asked for.
use ratatui::style::Color;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeKind {
    Cozy,   // warm paper / cafe — current default
    Midnight, // cooler, higher contrast
    Paper,  // light cozy
}

impl Default for ThemeKind {
    fn default() -> Self { Self::Cozy }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollbarTheme {
    pub track: Color,
    pub thumb: Color,
    pub thumb_focused: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectorTheme {
    /// indicator for selected item (e.g. "▸ " or "● ")
    pub indicator: &'static str,
    pub indicator_color: Color,
    pub selected_bg: Color,
    pub selected_fg: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub bg: Color,
    pub surface: Color,
    pub surface_hover: Color,
    pub border: Color,
    pub border_focus: Color,
    pub text: Color,
    pub text_dim: Color,
    pub text_muted: Color,
    pub accent: Color,
    pub accent_green: Color,
    pub scrollbar: ScrollbarTheme,
    pub selector: SelectorTheme,
    pub kind: ThemeKind,
}

impl Theme {
    /// Cozy — the current warm look from screenshot tweaks.
    pub fn cozy() -> Self {
        Self {
            bg: Color::Rgb(26, 24, 22),
            surface: Color::Rgb(36, 32, 28),
            surface_hover: Color::Rgb(48, 42, 36),
            border: Color::Rgb(92, 80, 65),
            border_focus: Color::Rgb(214, 167, 98),
            text: Color::Rgb(213, 196, 161),
            text_dim: Color::Rgb(168, 153, 132),
            text_muted: Color::Rgb(124, 111, 100),
            accent: Color::Rgb(214, 167, 98),
            accent_green: Color::Rgb(142, 192, 124),
            scrollbar: ScrollbarTheme {
                track: Color::Rgb(48, 42, 36),
                thumb: Color::Rgb(92, 80, 65),
                thumb_focused: Color::Rgb(214, 167, 98),
            },
            selector: SelectorTheme {
                indicator: "▸ ",
                indicator_color: Color::Rgb(214, 167, 98),
                selected_bg: Color::Rgb(48, 42, 36),
                selected_fg: Color::Rgb(213, 196, 161),
            },
            kind: ThemeKind::Cozy,
        }
    }

    pub fn midnight() -> Self {
        Self {
            bg: Color::Rgb(14, 14, 18),
            surface: Color::Rgb(22, 22, 28),
            surface_hover: Color::Rgb(32, 32, 42),
            border: Color::Rgb(60, 60, 75),
            border_focus: Color::Rgb(138, 180, 255),
            text: Color::Rgb(220, 220, 235),
            text_dim: Color::Rgb(160, 160, 180),
            text_muted: Color::Rgb(120, 120, 140),
            accent: Color::Rgb(138, 180, 255),
            accent_green: Color::Rgb(160, 220, 160),
            scrollbar: ScrollbarTheme {
                track: Color::Rgb(32, 32, 42),
                thumb: Color::Rgb(60, 60, 75),
                thumb_focused: Color::Rgb(138, 180, 255),
            },
            selector: SelectorTheme {
                indicator: "▸ ",
                indicator_color: Color::Rgb(138, 180, 255),
                selected_bg: Color::Rgb(32, 32, 42),
                selected_fg: Color::Rgb(220, 220, 235),
            },
            kind: ThemeKind::Midnight,
        }
    }

    /// Paper — light, warm off-white with ink-brown text.
    pub fn paper() -> Self {
        Self {
            bg: Color::Rgb(242, 236, 224),
            surface: Color::Rgb(250, 246, 238),
            surface_hover: Color::Rgb(232, 222, 204),
            border: Color::Rgb(196, 182, 160),
            border_focus: Color::Rgb(166, 98, 42),
            text: Color::Rgb(52, 44, 36),
            text_dim: Color::Rgb(92, 80, 68),
            text_muted: Color::Rgb(128, 116, 102),
            accent: Color::Rgb(166, 98, 42),
            accent_green: Color::Rgb(62, 120, 64),
            scrollbar: ScrollbarTheme {
                track: Color::Rgb(232, 222, 204),
                thumb: Color::Rgb(196, 182, 160),
                thumb_focused: Color::Rgb(166, 98, 42),
            },
            selector: SelectorTheme {
                indicator: "▸ ",
                indicator_color: Color::Rgb(166, 98, 42),
                selected_bg: Color::Rgb(232, 222, 204),
                selected_fg: Color::Rgb(52, 44, 36),
            },
            kind: ThemeKind::Paper,
        }
    }

    pub fn from_kind(kind: ThemeKind) -> Self {
        match kind {
            ThemeKind::Cozy => Self::cozy(),
            ThemeKind::Midnight => Self::midnight(),
            ThemeKind::Paper => Self::paper(),
        }
    }

    /// All available themes — drives the future selector/scrollbar you mentioned.
    pub fn all_kinds() -> &'static [ThemeKind] {
        &[ThemeKind::Cozy, ThemeKind::Midnight, ThemeKind::Paper]
    }

    pub fn name(&self) -> &'static str {
        match self.kind {
            ThemeKind::Cozy => "cozy",
            ThemeKind::Midnight => "midnight",
            ThemeKind::Paper => "paper",
        }
    }
}

impl Default for Theme {
    fn default() -> Self { Self::cozy() }
}
