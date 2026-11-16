//! Settings and history kept between runs, in `<config dir>/tuinime/state.json`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::backend::Anime;
use crate::frontend::theme::ThemeKind;

/// Most shows kept in "Continue Watching".
pub const MAX_CONTINUE: usize = 20;

/// Everything saved. Every field has a default, so older or partial files still load.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    pub theme: ThemeKind,
    pub dub: bool,
    pub recent_searches: Vec<String>,
    pub continue_watching: Vec<Anime>,
}

/// `<config dir>/tuinime/state.json` (e.g. `%APPDATA%\tuinime\state.json` on Windows).
pub fn default_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("tuinime").join("state.json"))
}

/// Read saved state; a missing or unreadable file gives the defaults.
pub fn load(path: &Path) -> Saved {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            tracing::warn!("ignoring unreadable {}: {e}", path.display());
            Saved::default()
        }),
        Err(_) => Saved::default(),
    }
}

/// Write state via a temp file + rename, so a crash mid-write can't leave a broken file.
pub fn save(path: &Path, saved: &Saved) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(saved)?).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("replace {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::ui::testkit::anime;

    fn temp_path() -> PathBuf {
        std::env::temp_dir().join(format!("tuinime-test-{}", uuid::Uuid::new_v4())).join("state.json")
    }

    #[test]
    fn round_trips() {
        let path = temp_path();
        let saved = Saved {
            theme: ThemeKind::Paper,
            dub: true,
            recent_searches: vec!["frieren".into()],
            continue_watching: vec![anime("Frieren")],
        };
        save(&path, &saved).unwrap();
        assert_eq!(load(&path), saved);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn missing_partial_or_broken_files_fall_back_to_defaults() {
        let path = temp_path();
        assert_eq!(load(&path), Saved::default());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"dub": true}"#).unwrap();
        assert_eq!(load(&path), Saved { dub: true, ..Saved::default() });
        std::fs::write(&path, "not json {").unwrap();
        assert_eq!(load(&path), Saved::default());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
