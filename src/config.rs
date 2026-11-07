//! Application settings + persistence (JSON under `XDG_CONFIG_HOME/Frame`).
//!
//! The docs (01, 06) show the original exposes an "Open Config Folder"
//! button, i.e. user-visible file-based configuration. Frame mirrors that
//! with plain JSON files rather than GSettings, so the folder really contains
//! the configuration on every distribution.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

pub const APP_ID: &str = "io.github.hakergeniusz.Frame";
pub const APP_NAME: &str = "Frame";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEVELOPER: &str = "hakergeniusz";
pub const WEBSITE: &str = "https://github.com/hakergeniusz/Frame";
pub const ISSUE_URL: &str = "https://github.com/hakergeniusz/Frame/issues";

/// Mouse-click mapping for the video surface (docs/04, group 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrimaryClickAction {
    PlayPause,
    FocusPlayPause,
    Bypass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecondaryClickAction {
    PlayPause,
    ContextMenu,
    Bypass,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // Behavior
    pub open_new_window: bool,
    pub seek_bar_thumbnail: bool,
    pub normalize_volume: bool,
    // Performance
    pub graphics_offload: bool,
    pub software_decoding: bool,
    // Session & resume
    pub restore_session: bool,
    pub save_video_position: bool,
    // Subtitles
    pub subtitle_color: String,
    pub subtitle_font: String,
    pub subtitle_scale: f64,
    pub subtitle_background: bool,
    // Mouse behavior
    pub primary_click: PrimaryClickAction,
    pub secondary_click: SecondaryClickAction,
    // Language preferences
    pub preferred_subtitle_languages: String,
    pub preferred_audio_languages: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            open_new_window: true,
            seek_bar_thumbnail: false,
            normalize_volume: false,
            graphics_offload: false,
            software_decoding: false,
            restore_session: false,
            save_video_position: false,
            subtitle_color: "#ffffffff".into(),
            subtitle_font: "Adwaita Sans SemiBold".into(),
            subtitle_scale: 1.0,
            subtitle_background: false,
            primary_click: PrimaryClickAction::Bypass,
            secondary_click: SecondaryClickAction::PlayPause,
            preferred_subtitle_languages: String::new(),
            preferred_audio_languages: String::new(),
        }
    }
}

/// `#RRGGBBAA` → mpv color string (`#AARRGGBB`). Returns None for unparsable
/// values (the caller keeps the previous setting then).
pub fn mpv_color(rgba: &str) -> Option<String> {
    let hex = rgba.strip_prefix('#')?;
    let (r, g, b, a) = match hex.len() {
        8 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
            u8::from_str_radix(&hex[6..8], 16).ok()?,
        ),
        6 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
            0xff,
        ),
        _ => return None,
    };
    Some(format!("#{a:02x}{r:02x}{g:02x}{b:02x}"))
}

/// Handle to the settings file. Cheap to clone; writes are saved atomically.
#[derive(Clone)]
pub struct SettingsStore {
    inner: Arc<RwLock<Settings>>,
    path: PathBuf,
}

impl SettingsStore {
    pub fn load(config_dir: &std::path::Path) -> Self {
        let path = config_dir.join("config.json");
        let settings = std::fs::read(&path)
            .ok()
            .and_then(|data| match serde_json::from_slice::<Settings>(&data) {
                Ok(s) => Some(s),
                Err(e) => {
                    eprintln!("frame: invalid config.json ({e}); using defaults");
                    None
                }
            })
            .unwrap_or_default();
        let store = Self {
            inner: Arc::new(RwLock::new(settings)),
            path,
        };
        // Ensure the config folder exists so "Open Config Folder" shows it.
        let _ = std::fs::create_dir_all(config_dir);
        store.save();
        store
    }

    pub fn get(&self) -> Settings {
        self.inner.read().unwrap().clone()
    }

    /// Apply a mutation and persist.
    pub fn update<R>(&self, f: impl FnOnce(&mut Settings) -> R) -> R {
        let mut s = self.inner.write().unwrap();
        let r = f(&mut s);
        drop(s);
        self.save();
        r
    }

    fn save(&self) {
        let settings = self.inner.read().unwrap().clone();
        if let Err(e) = write_atomic(&self.path, &serde_json::to_vec_pretty(&settings).unwrap()) {
            eprintln!("frame: failed to save config: {e}");
        }
    }
}

pub(crate) fn write_atomic(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

/// The user-facing configuration directory (`…/Frame` under XDG config home).
pub fn config_dir() -> PathBuf {
    let base = glib::user_config_dir();
    base.join("Frame")
}

/// Locations of all persisted state, resolved once at startup.
#[derive(Debug, Clone)]
pub struct AppRuntimePaths {
    pub config_dir: PathBuf,
}

impl AppRuntimePaths {
    pub fn detect() -> Self {
        Self {
            config_dir: config_dir(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_documented_preferences() {
        let s = Settings::default();
        // docs/04 — exact defaults
        assert!(s.open_new_window);
        assert!(!s.seek_bar_thumbnail);
        assert!(!s.normalize_volume);
        assert!(!s.graphics_offload);
        assert!(!s.software_decoding);
        assert!(!s.restore_session);
        assert!(!s.save_video_position);
        assert_eq!(s.subtitle_color, "#ffffffff");
        assert_eq!(s.subtitle_font, "Adwaita Sans SemiBold");
        assert!((s.subtitle_scale - 1.0).abs() < 1e-9);
        assert!(!s.subtitle_background);
        assert_eq!(s.primary_click, PrimaryClickAction::Bypass);
        assert_eq!(s.secondary_click, SecondaryClickAction::PlayPause);
        assert_eq!(s.preferred_subtitle_languages, "");
        assert_eq!(s.preferred_audio_languages, "");
    }

    #[test]
    fn settings_roundtrip() {
        let dir = std::env::temp_dir().join(format!("frame-test-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = SettingsStore::load(&dir);
        store.update(|s| s.normalize_volume = true);
        let store2 = SettingsStore::load(&dir);
        assert!(store2.get().normalize_volume);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn invalid_config_falls_back_to_defaults() {
        let dir = std::env::temp_dir().join(format!("frame-test-config-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.json"), b"{ not json").unwrap();
        let store = SettingsStore::load(&dir);
        assert_eq!(store.get(), Settings::default());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn color_conversion() {
        assert_eq!(mpv_color("#ffffffff").as_deref(), Some("#ffffffff"));
        assert_eq!(mpv_color("#ff8000cc").as_deref(), Some("#ccff8000"));
        assert_eq!(mpv_color("#ff8000").as_deref(), Some("#ffff8000"));
        assert_eq!(mpv_color("nonsense"), None);
    }
}
