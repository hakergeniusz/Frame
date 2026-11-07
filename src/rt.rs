//! Application-level runtime shared by all windows: settings, history,
//! resume/session stores, screensaver inhibit, the player-window registry,
//! and the open/add flows (window-per-file preference, docs/01 + docs/06).

use gtk::prelude::*;

use crate::config::{AppRuntimePaths, SettingsStore};
use crate::engine::MediaSource;
use crate::history::HistoryStore;
use crate::inhibit::InhibitManager;
use crate::player::PlayerWindowCore;
use crate::resume::{ResumeStore, SavedSession, SessionStore, SessionWindow};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

pub struct AppRuntime {
    pub settings: SettingsStore,
    pub history: HistoryStore,
    pub resume: ResumeStore,
    pub session: SessionStore,
    pub inhibit: InhibitManager,
    players: RefCell<Vec<std::rc::Weak<PlayerWindowCore>>>,
}

impl AppRuntime {
    pub fn new(paths: &AppRuntimePaths) -> Arc<Self> {
        Arc::new(Self {
            settings: SettingsStore::load(&paths.config_dir),
            history: HistoryStore::load(&paths.config_dir),
            resume: ResumeStore::load(&paths.config_dir),
            session: SessionStore::load(&paths.config_dir),
            inhibit: InhibitManager::new(),
            players: RefCell::new(Vec::new()),
        })
    }

    pub fn register_player(&self, core: &Rc<PlayerWindowCore>) {
        self.players.borrow_mut().push(Rc::downgrade(core));
    }

    pub fn for_each_player(&self, mut f: impl FnMut(Rc<PlayerWindowCore>)) {
        self.players.borrow_mut().retain(|w| w.upgrade().is_some());
        for w in self.players.borrow().iter() {
            if let Some(core) = w.upgrade() {
                f(core);
            }
        }
    }

    pub fn player_count(&self) -> usize {
        self.players.borrow().iter().filter(|w| w.upgrade().is_some()).count()
    }

    /// Apply an open/add decision (see IMPLEMENTATION_DECISIONS for the
    /// recorded assumptions).
    pub fn apply_open(
        self: &Arc<Self>,
        sources: Vec<MediaSource>,
        add: bool,
        target: Option<Rc<PlayerWindowCore>>,
    ) {
        if sources.is_empty() {
            return;
        }
        for s in &sources {
            self.history.add(s);
        }
        if add {
            if let Some(target) = target {
                target.add_items(sources);
                return;
            }
            // "Add …" from a welcome window: nothing to append to — open.
        }
        let open_new_window = self.settings.get().open_new_window;
        if open_new_window || target.is_none() {
            if self.create_player_window(sources).is_none() {
                return; // engine failed; already logged
            }
        } else if let Some(target) = target {
            target.replace_items(sources);
        }
    }

    /// Create a player window; if `sources` is non-empty the first item
    /// starts playing, otherwise the window stays empty (docs/06: a failed
    /// chooser leaves an empty player window behind).
    pub fn create_player_window(
        self: &Arc<Self>,
        sources: Vec<MediaSource>,
    ) -> Option<Rc<PlayerWindowCore>> {
        crate::player::create_player_window(self.clone(), sources)
    }

    /// "Open Files" / "Add Files" chooser flow.
    ///
    /// docs/06: with the new-window preference on, the (empty) player window
    /// is created first and the chooser attaches to it — cancelling leaves
    /// an empty player window behind (observed behavior, chosen for Frame).
    pub fn begin_open_files(
        self: &Arc<Self>,
        parent: &impl gtk::prelude::IsA<gtk::Window>,
        add: bool,
        target: Option<Rc<PlayerWindowCore>>,
    ) {
        if add {
            // Add: chooser directly; results append to the target window.
            let rt = self.clone();
            let target = target.clone();
            crate::ui::file_dialogs::choose_files(parent, move |files| {
                let sources: Vec<MediaSource> = files.iter().filter_map(file_to_source).collect();
                rt.apply_open(sources, true, target.clone());
            });
            return;
        }
        if self.settings.get().open_new_window {
            let Some(window) = self.create_player_window(Vec::new()) else { return };
            let rt = self.clone();
            let window2 = window.clone();
            crate::ui::file_dialogs::choose_files(&window.window, move |files| {
                let sources: Vec<MediaSource> = files.iter().filter_map(file_to_source).collect();
                if sources.is_empty() {
                    // Cancelled — the empty player window remains (docs/06).
                    return;
                }
                window2.replace_items(sources);
                let _ = rt;
            });
        } else {
            let rt = self.clone();
            let target = target.clone();
            crate::ui::file_dialogs::choose_files(parent, move |files| {
                let sources: Vec<MediaSource> = files.iter().filter_map(file_to_source).collect();
                rt.apply_open(sources, false, target.clone());
            });
        }
    }

    /// "Open Folder" / "Add Folder" chooser flow (docs/02: the folder's video
    /// contents become the playlist).
    pub fn begin_open_folder(
        self: &Arc<Self>,
        parent: &impl gtk::prelude::IsA<gtk::Window>,
        add: bool,
        target: Option<Rc<PlayerWindowCore>>,
    ) {
        let rt = self.clone();
        let window_first = !add && self.settings.get().open_new_window;
        let pre_window = if window_first { self.create_player_window(Vec::new()) } else { None };
        crate::ui::file_dialogs::choose_folder(parent, move |file| {
            let sources = file.and_then(|f| f.path()).and_then(|p| scan_folder(&p)).unwrap_or_default();
            match pre_window.clone() {
                Some(window) => {
                    if sources.is_empty() {
                        return; // cancelled / empty → empty window remains
                    }
                    window.replace_items(sources);
                }
                None => rt.apply_open(sources, add, target.clone()),
            }
        });
    }

    /// Open a folder's video files (sorted by name, non-recursive — recorded
    /// as an assumption; docs/07 §2). Returns None when the path is missing.
    pub fn scan_folder_public(dir: &std::path::Path) -> Option<Vec<MediaSource>> {
        scan_folder(dir)
    }

    /// Save Session (menu): snapshot all player windows.
    pub fn save_session_now(&self) {
        let mut windows = Vec::new();
        self.for_each_player(|core| {
            windows.push(SessionWindow {
                items: core.playlist_item_sources(),
                current: core.playlist_current_index(),
                position: core.player.position(),
            });
        });
        self.session.put(&SavedSession { windows });
    }

    /// Restore Saved Session (at startup, when the preference is on).
    pub fn restore_session_if_enabled(self: &Arc<Self>) {
        if !self.settings.get().restore_session {
            return;
        }
        let Some(session) = self.session.get() else { return };
        for w in session.windows {
            if w.items.is_empty() {
                continue;
            }
            let Some(core) = self.create_player_window(w.items) else { continue };
            if let (Some(idx), Some(pos)) = (w.current, w.position) {
                core.play_index_with_position(idx, Some(pos));
            }
        }
    }

    /// Directory for screenshots (`~/Pictures/Frame`), created on demand.
    pub fn screenshot_dir(&self) -> Option<PathBuf> {
        let pictures = glib::user_special_dir(glib::UserDirectory::Pictures)
            .unwrap_or_else(|| glib::home_dir().join("Pictures"));
        let dir = pictures.join("Frame");
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir)
    }
}

fn file_to_source(file: &gio::File) -> Option<MediaSource> {
    let path = file.path()?;
    Some(MediaSource::Path(path.to_string_lossy().into_owned()))
}

/// The video files of a folder (non-recursive, name-sorted — see the
/// recorded assumption for docs/07 §2 "Open Folder semantics").
pub fn scan_folder(dir: &std::path::Path) -> Option<Vec<MediaSource>> {
    let mut sources = Vec::new();
    let entries = std::fs::read_dir(dir).ok()?;
    let mut names: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter(|e| crate::inhibit::is_video_file(&e.file_name().to_string_lossy()))
        .collect();
    names.sort_by_key(|e| e.file_name());
    for e in names {
        sources.push(MediaSource::Path(e.path().to_string_lossy().into_owned()));
    }
    Some(sources)
}
