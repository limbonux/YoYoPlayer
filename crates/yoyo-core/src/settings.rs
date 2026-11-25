//! Settings saved in `%APPDATA%\YoYoPlayer\settings.toml`, or `.\data\` next to
//! the executable when `portable.txt` is there.
//! Fix X04: the playback speed is remembered across files and restarts.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::PlayMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub volume: f64,
    pub muted: bool,
    pub speed: f64,
    pub remember_speed: bool,
    pub play_mode: PlayMode,
    pub playlist_visible: bool,
    /// Fix X08: hide the controls in a window too, after the idle delay.
    pub autohide_controls_windowed: bool,
    pub controls_autohide_ms: u64,
    pub subtitle_scale: f64,
    pub always_on_top: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            volume: 80.0,
            muted: false,
            speed: 1.0,
            remember_speed: true,
            play_mode: PlayMode::Order,
            playlist_visible: false,
            autohide_controls_windowed: false,
            controls_autohide_ms: 3000,
            subtitle_scale: 1.0,
            always_on_top: false,
        }
    }
}

impl Settings {
    pub fn data_dir() -> Option<PathBuf> {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                if dir.join("portable.txt").exists() {
                    return Some(dir.join("data"));
                }
            }
        }
        directories::ProjectDirs::from("", "", "YoYoPlayer").map(|d| d.config_dir().to_path_buf())
    }

    fn path() -> Option<PathBuf> {
        Self::data_dir().map(|d| d.join("settings.toml"))
    }

    /// Loads the settings; a missing or broken file gives the defaults.
    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else { return Ok(()) };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }
}
