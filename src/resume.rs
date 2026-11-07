//! Session & resume persistence (docs/04, docs/06).
//!
//! - `resume.json` — per-video resume ("Save Video Position on Close"):
//!   stores the playback position *and* the picture adjustments + subtitle
//!   delay active at close time, keyed by media source.
//! - `session.json` — "Save Session" / "Save Session and Close" snapshots
//!   all player windows (playlists + current items + positions); restored
//!   at startup when "Restore Saved Session" is enabled.

use crate::config::write_atomic;
use crate::engine::MediaSource;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Picture adjustments + subtitle delay saved alongside the position, per
/// the "Save Video Position on Close" copy in docs/04.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VideoAdjustments {
    pub contrast: i64,
    pub brightness: i64,
    pub gamma: i64,
    pub saturation: i64,
    pub zoom: f64,
    pub sub_delay: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResumeEntry {
    pub position: f64,
    pub adjustments: VideoAdjustments,
    pub timestamp: i64,
}

#[derive(Clone)]
pub struct ResumeStore {
    inner: std::sync::Arc<std::sync::RwLock<HashMap<String, ResumeEntry>>>,
    path: PathBuf,
}

impl ResumeStore {
    pub fn load(config_dir: &Path) -> Self {
        let path = config_dir.join("resume.json");
        let map = std::fs::read(&path)
            .ok()
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default();
        Self {
            inner: std::sync::Arc::new(std::sync::RwLock::new(map)),
            path,
        }
    }

    pub fn take(&self, source: &MediaSource) -> Option<ResumeEntry> {
        self.inner.read().unwrap().get(source.resource()).cloned()
    }

    pub fn put(&self, source: &MediaSource, entry: ResumeEntry) {
        self.inner
            .write()
            .unwrap()
            .insert(source.resource().to_string(), entry);
        self.save();
    }

    /// Called when playback reaches the natural end: the resume point is no
    /// longer meaningful.
    pub fn clear(&self, source: &MediaSource) {
        self.inner.write().unwrap().remove(source.resource());
        self.save();
    }

    fn save(&self) {
        let map = self.inner.read().unwrap().clone();
        if let Err(e) = write_atomic(&self.path, &serde_json::to_vec_pretty(&map).unwrap()) {
            eprintln!("frame: failed to save resume data: {e}");
        }
    }
}

/// One player window inside a saved session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionWindow {
    pub items: Vec<MediaSource>,
    pub current: Option<usize>,
    /// Position (seconds) of the current item at save time.
    pub position: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedSession {
    pub windows: Vec<SessionWindow>,
}

#[derive(Clone)]
pub struct SessionStore {
    inner: std::sync::Arc<std::sync::RwLock<Option<SavedSession>>>,
    path: PathBuf,
}

impl SessionStore {
    pub fn load(config_dir: &Path) -> Self {
        let path = config_dir.join("session.json");
        let session = std::fs::read(&path)
            .ok()
            .and_then(|data| serde_json::from_slice(&data).ok());
        Self {
            inner: std::sync::Arc::new(std::sync::RwLock::new(session)),
            path,
        }
    }

    pub fn get(&self) -> Option<SavedSession> {
        self.inner.read().unwrap().clone()
    }

    pub fn put(&self, session: &SavedSession) {
        *self.inner.write().unwrap() = Some(session.clone());
        self.save();
    }

    fn save(&self) {
        let s = self.inner.read().unwrap().clone();
        if let Err(e) = write_atomic(&self.path, &serde_json::to_vec_pretty(&s).unwrap()) {
            eprintln!("frame: failed to save session: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_roundtrip() {
        let dir = std::env::temp_dir().join(format!("frame-test-resume-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let s = ResumeStore::load(&dir);
        let src = MediaSource::Url("http://x/v.mp4".into());
        assert!(s.take(&src).is_none());
        s.put(
            &src,
            ResumeEntry {
                position: 42.5,
                adjustments: VideoAdjustments { contrast: 5, brightness: -3, gamma: 0, saturation: 10, zoom: 0.2, sub_delay: -0.5 },
                timestamp: 1,
            },
        );
        let e = s.take(&src).unwrap();
        assert!((e.position - 42.5).abs() < 1e-9);
        assert_eq!(e.adjustments.contrast, 5);
        assert_eq!(e.adjustments.sub_delay, -0.5);
        s.clear(&src);
        assert!(s.take(&src).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn session_roundtrip() {
        let dir = std::env::temp_dir().join(format!("frame-test-session-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let s = SessionStore::load(&dir);
        assert!(s.get().is_none());
        let session = SavedSession {
            windows: vec![SessionWindow {
                items: vec![MediaSource::Url("http://x/a.mp4".into()), MediaSource::Path("/b.mkv".into())],
                current: Some(1),
                position: Some(12.0),
            }],
        };
        s.put(&session);
        assert_eq!(s.get().unwrap(), session);
        std::fs::remove_dir_all(&dir).ok();
    }
}
