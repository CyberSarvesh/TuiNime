use std::path::PathBuf;

use anyhow::{Context, Result};
use tokio::process::Child;

use super::providers::traits::StreamSource;

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

    /// mpv's `--input-ipc-server` takes a Unix socket path on Unix and a named pipe on Windows.
    fn new_ipc_path() -> PathBuf {
        let name = format!("miru-mpv-{}", uuid::Uuid::new_v4());
        if cfg!(windows) {
            PathBuf::from(format!(r"\\.\pipe\{name}"))
        } else {
            std::env::temp_dir().join(format!("{name}.sock"))
        }
    }

    /// Build mpv's arguments for a source, passing along any HTTP headers the CDN needs
    /// (most stream hosts reject requests without the right `Referer`).
    fn args_for(source: &StreamSource, title: &str, ipc_path: &std::path::Path) -> Vec<String> {
        let mut args = vec![
            format!("--force-media-title={title}"),
            format!("--title=tuinime — {title}"),
            "--force-window=immediate".to_string(),
            format!("--input-ipc-server={}", ipc_path.display()),
        ];
        if let Some(headers) = &source.headers {
            let mut extra = Vec::new();
            for (k, v) in headers {
                if k.eq_ignore_ascii_case("referer") {
                    args.push(format!("--referrer={v}"));
                } else if k.eq_ignore_ascii_case("user-agent") {
                    args.push(format!("--user-agent={v}"));
                } else {
                    // mpv splits this list on commas, so commas inside values must be escaped
                    extra.push(format!("{k}: {}", v.replace(',', "\\,")));
                }
            }
            if !extra.is_empty() {
                extra.sort();
                args.push(format!("--http-header-fields={}", extra.join(",")));
            }
        }
        args.push(source.url.clone());
        args
    }

    /// Spawn mpv to play `source`. Stops any existing playback first.
    pub async fn play(&mut self, source: &StreamSource, title: &str) -> Result<()> {
        self.stop().await?;

        let ipc_path = Self::new_ipc_path();
        let mut cmd = tokio::process::Command::new("mpv");
        cmd.args(Self::args_for(source, title, &ipc_path))
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

    async fn cleanup_ipc(&mut self) {
        if let Some(path) = self.ipc_path.take() {
            // named pipes on Windows vanish with the process; only Unix leaves a socket file
            if cfg!(unix) {
                let _ = tokio::fs::remove_file(path).await;
            }
        }
    }

    pub async fn stop(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            // SIGKILL on unix, TerminateProcess on windows
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        self.cleanup_ipc().await;
        Ok(())
    }

    /// Non-blocking poll: reap exited mpv. Returns `true` exactly once, when mpv has just exited.
    pub async fn poll(&mut self) -> Result<bool> {
        if let Some(child) = self.child.as_mut() {
            if child.try_wait()?.is_some() {
                self.child = None;
                self.cleanup_ipc().await;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Send a JSON IPC command to mpv if the socket/pipe exists (best-effort).
    #[allow(dead_code)]
    pub async fn send_ipc(&self, json_cmd: &str) -> Result<()> {
        use tokio::io::AsyncWriteExt;
        let Some(path) = &self.ipc_path else { return Ok(()) };
        #[cfg(unix)]
        let mut stream = tokio::net::UnixStream::connect(path).await?;
        #[cfg(windows)]
        let mut stream = tokio::net::windows::named_pipe::ClientOptions::new().open(path)?;
        stream.write_all(json_cmd.as_bytes()).await?;
        stream.write_all(b"\n").await?;
        Ok(())
    }
}

impl Default for MpvPlayer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_referer_and_extra_headers_to_mpv() {
        let src = StreamSource {
            url: "https://cdn.example/ep1.m3u8".into(),
            quality: None,
            headers: Some(
                [("Referer".to_string(), "https://allanime.to".to_string()), ("X-Key".to_string(), "a,b".to_string())]
                    .into_iter()
                    .collect(),
            ),
            is_m3u8: true,
        };
        let args = MpvPlayer::args_for(&src, "Show", std::path::Path::new("ipc"));
        assert!(args.contains(&"--referrer=https://allanime.to".to_string()));
        assert!(args.contains(&"--http-header-fields=X-Key: a\\,b".to_string()));
        assert_eq!(args.last().unwrap(), "https://cdn.example/ep1.m3u8");
    }
}
