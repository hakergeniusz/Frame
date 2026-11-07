//! Watch History (docs/02, docs/06): persisted, date-grouped, newest first.
//!
//! Documented behaviors: URL opens are recorded as soon as they are opened,
//! even if playback fails; entries persist across restarts; rows show the
//! full URL for URL sources and the file name without extension for local
//! files; entries are grouped by date with a locale-formatted header.

use crate::config::write_atomic;
use crate::engine::MediaSource;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub source: MediaSource,
    /// Unix timestamp (seconds) of when the media was opened.
    pub timestamp: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct HistoryFile {
    entries: Vec<HistoryEntry>,
}

const MAX_ENTRIES: usize = 500;

#[derive(Clone)]
pub struct HistoryStore {
    inner: std::sync::Arc<std::sync::RwLock<Vec<HistoryEntry>>>,
    path: PathBuf,
}

impl HistoryStore {
    pub fn load(config_dir: &Path) -> Self {
        let path = config_dir.join("history.json");
        let entries = std::fs::read(&path)
            .ok()
            .and_then(|data| match serde_json::from_slice::<HistoryFile>(&data) {
                Ok(f) => Some(f.entries),
                Err(e) => {
                    eprintln!("frame: invalid history.json ({e}); starting empty");
                    None
                }
            })
            .unwrap_or_default();
        Self {
            inner: std::sync::Arc::new(std::sync::RwLock::new(entries)),
            path,
        }
    }

    pub fn entries(&self) -> Vec<HistoryEntry> {
        self.inner.read().unwrap().clone()
    }

    /// Record an open (called as soon as opening is attempted — see docs/06).
    pub fn add(&self, source: &MediaSource) {
        let now = glib::DateTime::now_local().map(|d| d.to_unix()).unwrap_or(0);
        {
            let mut entries = self.inner.write().unwrap();
            // Avoid immediate consecutive duplicates (re-opening the same item).
            if entries.first().map(|e| &e.source) == Some(source) {
                return;
            }
            entries.insert(0, HistoryEntry { source: source.clone(), timestamp: now });
            entries.truncate(MAX_ENTRIES);
        }
        self.save();
    }

    pub fn remove(&self, entry: &HistoryEntry) {
        self.inner.write().unwrap().retain(|e| e != entry);
        self.save();
    }

    pub fn clear(&self) {
        self.inner.write().unwrap().clear();
        self.save();
    }

    fn save(&self) {
        let file = HistoryFile {
            entries: self.inner.read().unwrap().clone(),
        };
        if let Err(e) = write_atomic(&self.path, &serde_json::to_vec_pretty(&file).unwrap()) {
            eprintln!("frame: failed to save history: {e}");
        }
    }
}

/// One date group for the popover UI: locale-formatted label + its entries
/// (newest first — the store keeps global newest-first order).
pub struct HistoryGroup {
    pub label: String,
    pub entries: Vec<HistoryEntry>,
}

/// Group entries by local calendar day, newest day first.
pub fn group_by_date(entries: &[HistoryEntry]) -> Vec<HistoryGroup> {
    let mut groups: Vec<(String, i64, Vec<HistoryEntry>)> = Vec::new();
    for entry in entries {
        let dt = glib::DateTime::from_unix_local(entry.timestamp);
        let (label, sort_day) = match &dt {
            Ok(dt) => {
                let label = dt.format("%x").map(|s| s.to_string()).unwrap_or_default();
                // Sort key: yyyymmdd for stable day ordering.
                let y = dt.year();
                let m = dt.month();
                let d = dt.day_of_month();
                (label, y as i64 * 10_000 + m as i64 * 100 + d as i64)
            }
            Err(_) => (String::new(), 0),
        };
        match groups.iter_mut().find(|(l, _, _)| *l == label) {
            Some(group) => group.2.push(entry.clone()),
            None => groups.push((label, sort_day, vec![entry.clone()])),
        }
    }
    groups.sort_by_key(|(_, day, _)| std::cmp::Reverse(*day));
    groups
        .into_iter()
        .map(|(label, _, entries)| HistoryGroup { label, entries })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(dir: &Path) -> HistoryStore {
        std::fs::create_dir_all(dir).unwrap();
        HistoryStore::load(dir)
    }

    #[test]
    fn add_remove_clear_roundtrip() {
        let dir = std::env::temp_dir().join(format!("frame-test-hist-{}", std::process::id()));
        let s = store(&dir);
        let url = MediaSource::Url("http://127.0.0.1:8069/Sample%20B.mp4".into());
        s.add(&url);
        assert_eq!(s.entries().len(), 1);
        assert_eq!(s.entries()[0].source, url);
        // consecutive duplicate not added
        s.add(&url);
        assert_eq!(s.entries().len(), 1);
        let path = MediaSource::Path("/movies/Sample A.mp4".into());
        s.add(&path);
        assert_eq!(s.entries().len(), 2);
        // newest first
        assert_eq!(s.entries()[0].source, path);

        // persisted
        let s2 = HistoryStore::load(&dir);
        assert_eq!(s2.entries().len(), 2);

        let entry = s2.entries()[0].clone();
        s2.remove(&entry);
        assert_eq!(s2.entries().len(), 1);
        s2.clear();
        assert!(s2.entries().is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn date_grouping_newest_first() {
        // Two entries: yesterday and today → two groups, today first.
        let now = glib::DateTime::now_local().unwrap().to_unix();
        let yesterday = now - 86_400;
        let entries = vec![
            HistoryEntry { source: MediaSource::Url("u1".into()), timestamp: now },
            HistoryEntry { source: MediaSource::Url("u2".into()), timestamp: yesterday },
            HistoryEntry { source: MediaSource::Url("u3".into()), timestamp: now },
        ];
        let groups = group_by_date(&entries);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].entries.len(), 2); // u3, u1 (newest first within day)
        assert_eq!(groups[1].entries.len(), 1);
        assert!(!groups[0].label.is_empty());
    }
}
