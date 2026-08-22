use std::path::PathBuf;

use anyhow::{Context, Result};
use tokio::process::Child;

/// Thin wrapper around an `mpv` child process with optional IPC socket.
///
/// Uses `tokio::process::Command` so spawning never blocks the TUI.
/// When `mpv` is not installed the error is surfaced as status text rather
/// than panicking.
pub struct MpvPlayer {
    child: Option<Child>,
    ipc_path: Option<PathBuf>,
}

impl MpvPlayer {
    pub fn new() -> Self {
        Self {
            child: None,
            ipc_path: None,
        }
    }

    /// Spawn mpv to play `url`. Stops any existing playback first.
    pub async fn play(&mut self, url: &str, title: &str) -> Result<()> {
        self.stop().await?;

        let ipc_path = std::env::temp_dir().join(format!("miru-mpv-{}.sock", uuid::Uuid::new_v4()));

        let mut cmd = tokio::process::Command::new("mpv");
        cmd.arg(format!("--title=miru — {title}"))
            .arg("--force-window=immediate")
            .arg(format!("--input-ipc-server={}", ipc_path.display()))
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);

        let child = cmd.spawn().context(
            "failed to spawn mpv — is it installed and on PATH? (https://mpv.io/install/)",
        )?;

        self.child = Some(child);
        self.ipc_path = Some(ipc_path);
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            // Try graceful kill; SIGTERM on unix, TerminateProcess on windows
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        if let Some(path) = self.ipc_path.take() {
            let _ = tokio::fs::remove_file(path).await;
        }
        Ok(())
    }

    /// Non-blocking poll: reap exited mpv so `child` does not stay `Some` forever.
    pub async fn poll(&mut self) -> Result<bool> {
        if let Some(child) = self.child.as_mut() {
            match child.try_wait()? {
                Some(_status) => {
                    self.child = None;
                    if let Some(p) = self.ipc_path.take() {
                        let _ = tokio::fs::remove_file(p).await;
                    }
                    return Ok(true); // just exited
                }
                None => return Ok(false), // still running
            }
        }
        Ok(false)
    }

    pub fn is_playing(&self) -> bool {
        self.child.is_some()
    }

    /// Send a JSON IPC command to mpv if the socket exists (best-effort).
    #[allow(dead_code)]
    pub async fn send_ipc(&self, json_cmd: &str) -> Result<()> {
        if let Some(path) = &self.ipc_path {
            use tokio::io::AsyncWriteExt;
            #[cfg(unix)]
            {
                let mut stream = tokio::net::UnixStream::connect(path).await?;
                stream.write_all(json_cmd.as_bytes()).await?;
                stream.write_all(b"\n").await?;
            }
            #[cfg(not(unix))]
            {
                let _ = (path, json_cmd);
                anyhow::bail!("mpv IPC via Unix socket is only available on Unix");
            }
        }
        Ok(())
    }
}

impl Default for MpvPlayer {
    fn default() -> Self {
        Self::new()
    }
}
