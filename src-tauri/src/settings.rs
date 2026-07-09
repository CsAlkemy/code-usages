use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub show_number: bool,
    #[serde(default = "default_theme")]
    pub theme: String, // "system" | "light" | "dark"
    // One-time flag: autostart is enabled by default on first launch, but only
    // once — so a user who later turns it off stays off.
    #[serde(default)]
    pub autostart_configured: bool,
}

fn default_theme() -> String {
    "system".into()
}

impl Default for Settings {
    fn default() -> Self {
        Self { show_number: false, theme: default_theme(), autostart_configured: false }
    }
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_config_dir().ok()?;
    let _ = fs::create_dir_all(&dir);
    Some(dir.join("settings.json"))
}

pub fn load(app: &AppHandle) -> Settings {
    path(app)
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(app: &AppHandle, s: &Settings) {
    if let (Some(p), Ok(body)) = (path(app), serde_json::to_string_pretty(s)) {
        let _ = fs::write(p, body);
    }
}
