//! Persistent application settings.

use crate::ai::AiSettings;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub ai: AiSettings,
    /// `system`, `light` or `dark`.
    pub theme: String,
    /// Read the clipboard automatically when the window gains focus.
    pub auto_paste_on_focus: bool,
    /// Poll the clipboard and convert new content automatically.
    pub watch_clipboard: bool,
    /// Copy the result to the clipboard automatically after each conversion.
    pub auto_copy_result: bool,
    /// Output format id used before the user picks one.
    pub default_output: Option<String>,
    /// UI language: `system`, `en` or `tr`.
    pub language: String,
    pub settings_version: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            ai: AiSettings::default(),
            theme: "system".into(),
            auto_paste_on_focus: false,
            watch_clipboard: false,
            auto_copy_result: false,
            default_output: None,
            language: "system".into(),
            settings_version: 1,
        }
    }
}

impl AppSettings {
    pub fn load(path: &Path) -> Result<AppSettings> {
        if !path.exists() {
            return Ok(AppSettings::default());
        }
        let text = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&text).unwrap_or_default())
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_missing_gives_default_and_round_trips() {
        let dir =
            std::env::temp_dir().join(format!("pastecompiler-settings-{}", uuid::Uuid::new_v4()));
        let path = dir.join("settings.json");
        let s = AppSettings::load(&path).unwrap();
        assert_eq!(s, AppSettings::default());
        let mut changed = s.clone();
        changed.theme = "dark".into();
        changed.ai.model = "llama3".into();
        changed.save(&path).unwrap();
        assert_eq!(AppSettings::load(&path).unwrap(), changed);
        // Corrupt files fall back to defaults instead of failing.
        fs::write(&path, "{not json").unwrap();
        assert_eq!(AppSettings::load(&path).unwrap(), AppSettings::default());
        let _ = fs::remove_dir_all(dir);
    }
}
