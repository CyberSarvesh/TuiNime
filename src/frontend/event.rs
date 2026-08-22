use std::time::Duration;

use anyhow::Result;
use crossterm::event::{Event as CrosstermEvent, KeyEvent, MouseEvent, MouseEventKind};
use futures::{FutureExt, StreamExt};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum Event {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
    Tick,
}

/// Async event handler — filters noisy mouse moves so channel doesn't flood.
pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<Result<Event, anyhow::Error>>,
    _handle: tokio::task::JoinHandle<()>,
}

impl EventHandler {
    pub fn new(tick_rate_ms: u64) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let tick_rate = Duration::from_millis(tick_rate_ms);
        let handle = tokio::spawn(async move {
            let mut reader = crossterm::event::EventStream::new();
            let mut tick = tokio::time::interval(tick_rate);
            loop {
                let tick_delay = tick.tick();
                let crossterm_event = reader.next().fuse();
                tokio::select! {
                    _ = tick_delay => {
                        if tx.send(Ok(Event::Tick)).is_err() { break; }
                    }
                    maybe_event = crossterm_event => {
                        match maybe_event {
                            Some(Ok(ev)) => {
                                let app_event = match ev {
                                    CrosstermEvent::Key(k) => Some(Event::Key(k)),
                                    CrosstermEvent::Resize(w, h) => Some(Event::Resize(w, h)),
                                    CrosstermEvent::FocusGained | CrosstermEvent::FocusLost | CrosstermEvent::Paste(_) => None,
                                    CrosstermEvent::Mouse(m) => {
                                        // ignore useless Move/Drag flood — only clicks/scroll matter
                                        match m.kind {
                                            MouseEventKind::Down(_) | MouseEventKind::Up(_) | MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => Some(Event::Mouse(m)),
                                            _ => None,
                                        }
                                    }
                                };
                                if let Some(e) = app_event {
                                    if tx.send(Ok(e)).is_err() { break; }
                                }
                            }
                            Some(Err(e)) => { let _ = tx.send(Err(anyhow::anyhow!(e))); }
                            None => break,
                        }
                    }
                }
            }
        });
        Self { rx, _handle: handle }
    }

    pub async fn next(&mut self) -> Option<Result<Event, anyhow::Error>> {
        self.rx.recv().await
    }
}
