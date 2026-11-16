//! Records the README demo: drives the real app with live data through a scripted tour
//! and saves every frame (characters + colors) to `target/demo/frames.json`.
//!
//! ```text
//! cargo test record_demo -- --ignored        # needs network
//! python docs/tools/render_demo.py           # frames → docs/media/*.gif / *.png
//! ```

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::TestBackend, buffer::Buffer, style::Color, style::Modifier, Terminal};
use serde_json::{json, Value};

use crate::frontend::app::App;
use crate::frontend::state::{Episodes, SearchResults};
use crate::frontend::widgets::poster::PosterRenderer;

const W: u16 = 128;
const H: u16 = 38;

struct Recorder {
    term: Terminal<TestBackend>,
    frames: Vec<Value>,
}

impl Recorder {
    fn new() -> Self {
        Self { term: Terminal::new(TestBackend::new(W, H)).unwrap(), frames: Vec::new() }
    }

    /// Draw the app and keep the frame on screen for `ms` milliseconds.
    /// `still` names frames worth exporting as README screenshots.
    fn snap(&mut self, app: &App, ms: u32, still: Option<&str>) {
        self.term.draw(|f| super::render(f, app)).unwrap();
        let cells = encode(self.term.backend().buffer());
        self.frames.push(json!({ "ms": ms, "still": still, "cells": cells }));
    }

    fn save(&self) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("demo");
        std::fs::create_dir_all(&dir).unwrap();
        let out = json!({ "width": W, "height": H, "frames": self.frames });
        std::fs::write(dir.join("frames.json"), out.to_string()).unwrap();
        println!("saved {} frames to {}", self.frames.len(), dir.display());
    }
}

fn hex(c: Color) -> Value {
    match c {
        Color::Rgb(r, g, b) => json!(format!("#{r:02x}{g:02x}{b:02x}")),
        Color::Reset => Value::Null,
        other => json!(format!("{other:?}")),
    }
}

/// Rows of `[symbol, fg, bg, bold]` cells.
fn encode(buf: &Buffer) -> Vec<Vec<Value>> {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| {
                    let c = &buf[(x, y)];
                    json!([c.symbol(), hex(c.fg), hex(c.bg), c.modifier.contains(Modifier::BOLD)])
                })
                .collect()
        })
        .collect()
}

fn key(c: KeyCode) -> KeyEvent {
    KeyEvent::new(c, KeyModifiers::NONE)
}

/// Wait for background results until `done` says the app has what it needs.
async fn pump(app: &mut App, rec: &mut Recorder, mut done: impl FnMut(&App) -> bool) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
    while !done(app) {
        tokio::select! {
            Some(ev) = app.next_background() => app.on_background(ev).await,
            _ = tokio::time::sleep(Duration::from_millis(120)) => {
                app.on_tick().await;
                rec.snap(app, 120, None); // spinner frames while waiting
            }
        }
        assert!(tokio::time::Instant::now() < deadline, "timed out waiting for live data");
    }
}

/// Download the posters the next frames will show, so they render instead of placeholders.
async fn warm_posters(app: &App, urls: impl IntoIterator<Item = String>) {
    for u in urls {
        let _ = app.backend.cache.get_or_fetch(&u).await;
    }
}

async fn press(app: &mut App, rec: &mut Recorder, code: KeyCode, ms: u32) {
    app.on_key(key(code)).await;
    rec.snap(app, ms, None);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "network; records the README demo"]
async fn record_demo() {
    let mut app = App::new(PosterRenderer::new(ratatui_image::picker::Picker::from_fontsize((10, 20))));
    let mut rec = Recorder::new();

    // 1. startup: loading spinners, then live trending + seasonal rows
    app.spawn_initial_fetch();
    for _ in 0..6 {
        app.on_tick().await;
        rec.snap(&app, 120, None);
    }
    pump(&mut app, &mut rec, |a| !a.carousels[1].loading && !a.carousels[2].loading).await;
    let covers: Vec<String> = app.carousels.iter().flat_map(|c| c.items.iter().take(8)).filter_map(|a| a.cover_image.clone()).collect();
    warm_posters(&app, covers).await;
    rec.snap(&app, 2200, Some("browse"));

    // 2. browse along the trending row, then down to this season
    for _ in 0..4 {
        press(&mut app, &mut rec, KeyCode::Right, 650).await;
    }
    press(&mut app, &mut rec, KeyCode::Down, 900).await;
    for _ in 0..2 {
        press(&mut app, &mut rec, KeyCode::Right, 650).await;
    }

    // 3. search for a show, letter by letter, with live results
    press(&mut app, &mut rec, KeyCode::Char('/'), 900).await;
    for c in "frieren".chars() {
        press(&mut app, &mut rec, KeyCode::Char(c), 140).await;
    }
    app.search.due = Some(std::time::Instant::now());
    app.on_tick().await;
    pump(&mut app, &mut rec, |a| !matches!(a.search.results, SearchResults::Loading)).await;
    if let SearchResults::Loaded(r) = &app.search.results {
        let urls: Vec<String> = r.iter().filter_map(|a| a.cover_image.clone()).collect();
        warm_posters(&app, urls).await;
    }
    rec.snap(&app, 2200, Some("search"));

    // 4. open it: details page with the real episode list
    press(&mut app, &mut rec, KeyCode::Enter, 300).await;
    pump(&mut app, &mut rec, |a| a.detail.as_ref().is_some_and(|d| !matches!(d.episodes, Episodes::Loading))).await;
    rec.snap(&app, 1600, None);
    for code in [KeyCode::Right, KeyCode::Right, KeyCode::Right, KeyCode::Down, KeyCode::Right] {
        press(&mut app, &mut rec, code, 450).await;
    }
    rec.snap(&app, 2000, Some("details"));

    // 5. help popup
    press(&mut app, &mut rec, KeyCode::Char('?'), 2600).await;
    rec.snap(&app, 1, Some("help"));
    press(&mut app, &mut rec, KeyCode::Esc, 400).await;

    // 6. back to browse and through the themes
    press(&mut app, &mut rec, KeyCode::Esc, 500).await;
    press(&mut app, &mut rec, KeyCode::Esc, 500).await;
    press(&mut app, &mut rec, KeyCode::Esc, 900).await;
    press(&mut app, &mut rec, KeyCode::Char('t'), 1600).await;
    rec.snap(&app, 1, Some("theme-midnight"));
    press(&mut app, &mut rec, KeyCode::Char('t'), 1600).await;
    rec.snap(&app, 1, Some("theme-paper"));
    press(&mut app, &mut rec, KeyCode::Char('t'), 1800).await;

    rec.save();
}
