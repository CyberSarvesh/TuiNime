//! Services — background IO that `App` spawns. No widget code.

use tokio::sync::mpsc::UnboundedSender;

use crate::backend::{Anime, MiruBackend};

/// Spawns a one-shot trending fetch on a fresh `MiruBackend` (cheap `Client` clone).
/// Returns the receiver that `App::poll_trending_sync` drains.
pub fn spawn_trending(
    tx: UnboundedSender<Result<Vec<Anime>, String>>,
) -> tokio::sync::mpsc::UnboundedReceiver<Result<Vec<Anime>, String>> {
    let (tx2, rx) = tokio::sync::mpsc::unbounded_channel();
    // keep caller’s tx for compat if needed
    let _ = tx;
    let backend = MiruBackend::new();
    tokio::spawn(async move {
        let list = backend.trending(12).await;
        let _ = tx2.send(Ok(list));
    });
    rx
}

pub fn spawn_trending_into(tx: UnboundedSender<Result<Vec<Anime>, String>>) {
    let backend = MiruBackend::new();
    tokio::spawn(async move {
        let list = backend.trending(12).await;
        let _ = tx.send(Ok(list));
    });
}
