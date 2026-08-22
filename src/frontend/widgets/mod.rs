//! Reusable themed widgets — all read `Theme`, never hardcode colors.
//! These are the extension points for the future scrollbar/selector you asked about.

pub mod poster;
pub mod scrollbar;
pub mod selector;

pub use poster::Poster;
pub use scrollbar::{ThemedScrollbar, scrollbar_track};
pub use selector::{Selector, SelectorItem};
