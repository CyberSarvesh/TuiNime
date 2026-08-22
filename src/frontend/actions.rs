//! Actions — pure input handlers extracted from `App` so `app.rs` stays thin.
//! Each fn is a reducer: `(state, input) -> state delta + optional effect`.
//! For now just helpers; full async effects stay in `App::play_selected` / `services`.

use crate::frontend::ui::carousel::CarouselState;
use crate::frontend::ui::hero::HeroState;

pub fn sync_hero(active: usize, carousels: &[CarouselState]) -> Option<HeroState> {
    carousels.get(active).and_then(|c| c.selected_item().map(HeroState::from_anime))
}

pub fn next_carousel(current: usize, total: usize) -> usize {
    (current + 1).min(total.saturating_sub(1))
}

pub fn prev_carousel(current: usize) -> usize {
    current.saturating_sub(1)
}
