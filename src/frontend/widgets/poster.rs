//! Poster rendering shared by `hero` and `carousel`.
//!
//! The `Picker` is created once at startup (it knows whether the terminal speaks Kitty,
//! Sixel, iTerm2 or only unicode halfblocks). Encoded image states are cached per
//! (url, size), so a poster is decoded and encoded once rather than on every frame.
use std::cell::RefCell;
use std::collections::HashMap;

use ratatui::layout::Rect;
use ratatui::Frame;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::StatefulImage;

use crate::backend::cache::ImageCache;

const MAX_CACHED_STATES: usize = 64;

pub struct PosterRenderer {
    picker: Picker,
    states: RefCell<HashMap<(String, u16, u16), StatefulProtocol>>,
}

impl PosterRenderer {
    pub fn new(picker: Picker) -> Self {
        Self { picker, states: RefCell::new(HashMap::new()) }
    }

    /// Draw the poster for `url` into `area` if it is already on disk.
    /// Returns `false` (draw a placeholder instead) when it isn't downloaded yet.
    pub fn render(&self, f: &mut Frame, area: Rect, url: &str, cache: &ImageCache) -> bool {
        if area.width == 0 || area.height == 0 {
            return false;
        }
        let key = (url.to_string(), area.width, area.height);
        let mut states = self.states.borrow_mut();
        if !states.contains_key(&key) {
            let Some(img) = cache.try_load_cached_image(url) else { return false };
            if states.len() >= MAX_CACHED_STATES {
                states.clear();
            }
            let mut picker = self.picker;
            states.insert(key.clone(), picker.new_resize_protocol(img));
        }
        if let Some(state) = states.get_mut(&key) {
            f.render_stateful_widget(StatefulImage::new(None), area, state);
            true
        } else {
            false
        }
    }
}
