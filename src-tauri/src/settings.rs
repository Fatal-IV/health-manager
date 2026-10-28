//! The one user setting: how the waiting screen shows progress.

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Settings {
    /// `"live"` (the helper reports each operation) or `"staged"` (the app shows stages only).
    pub progress_mode: String,
}

const MODES: [&str; 2] = ["live", "staged"];

fn defaults() -> Settings {
    Settings { progress_mode: "live".into() }
}

/// A missing, broken or unknown value never produces an error: it means `live`.
pub fn parse(text: &str) -> Settings {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| v.get("progress_mode")?.as_str().map(String::from))
        .filter(|m| MODES.contains(&m.as_str()))
        .map(|progress_mode| Settings { progress_mode })
        .unwrap_or_else(defaults)
}

fn dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("SaglikDenetimi"))
}

fn load_from(dir: &Path) -> Settings {
    std::fs::read_to_string(dir.join("settings.json"))
        .map(|t| parse(&t))
        .unwrap_or_else(|_| defaults())
}

fn save_in(dir: &Path, mode: &str) -> Result<Settings, String> {
    if !MODES.contains(&mode) {
        return Err("unknown progress mode".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let s = Settings { progress_mode: mode.into() };
    let json = serde_json::json!({ "progress_mode": s.progress_mode });
    std::fs::write(dir.join("settings.json"), json.to_string()).map_err(|e| e.to_string())?;
    Ok(s)
}

pub fn load() -> Settings {
    dir().map(|d| load_from(&d)).unwrap_or_else(defaults)
}

pub fn save(mode: &str) -> Result<Settings, String> {
    let d = dir().ok_or_else(|| "no settings folder".to_string())?;
    save_in(&d, mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_defaults_to_live() {
        for text in ["", "{}", "not json", r#"{"progress_mode":"x"}"#, r#"{"progress_mode":3}"#] {
            assert_eq!(parse(text).progress_mode, "live", "{text:?}");
        }
    }

    #[test]
    fn parse_accepts_staged() {
        assert_eq!(parse(r#"{"progress_mode":"staged"}"#).progress_mode, "staged");
        assert_eq!(parse(r#"{"progress_mode":"live"}"#).progress_mode, "live");
    }

    #[test]
    fn save_rejects_unknown_mode_without_touching_the_disk() {
        let dir = std::env::temp_dir().join(format!("settings-reject-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(save_in(&dir, "x").is_err());
        assert!(!dir.exists());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = std::env::temp_dir().join(format!("settings-roundtrip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(save_in(&dir, "staged").unwrap().progress_mode, "staged");
        assert_eq!(load_from(&dir).progress_mode, "staged");
    }
}
