//! Screensaver inhibition while playing (docs/01, docs/06: the desktop
//! inhibit call happens when playback starts).
//!
//! - Inside Flatpak: `org.freedesktop.portal.Inhibit` (the portal is the
//!   sanctioned path in the sandbox).
//! - Outside: `org.freedesktop.ScreenSaver` (the freedesktop standard,
//!   honored by GNOME/KDE).
//!
//! Reference-counted: any playing window acquires, pause/close releases.

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use gtk::glib;
use gtk::glib::prelude::*;

#[derive(Clone)]
pub struct InhibitManager {
    count: Arc<AtomicU32>,
    cookie: Arc<std::sync::Mutex<Option<u32>>>,
    flatpak: bool,
}

impl InhibitManager {
    pub fn new() -> Self {
        Self {
            count: Arc::new(AtomicU32::new(0)),
            cookie: Arc::new(std::sync::Mutex::new(None)),
            flatpak: glib::getenv("FLATPAK_ID").is_some(),
        }
    }

    pub fn acquire(&self) {
        let prev = self.count.fetch_add(1, Ordering::SeqCst);
        if prev == 0 {
            glib::spawn_future_local(glib::clone!(
                #[strong(rename_to = this)]
                self,
                async move {
                    this.inhibit().await;
                }
            ));
        }
    }

    pub fn release(&self) {
        let prev = self.count.fetch_sub(1, Ordering::SeqCst);
        if prev == 1 {
            glib::spawn_future_local(glib::clone!(
                #[strong(rename_to = this)]
                self,
                async move {
                    this.uninhibit().await;
                }
            ));
        }
    }

    fn session_bus(&self) -> Option<gio::DBusConnection> {
        gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()
    }

    async fn inhibit(&self) {
        let Some(bus) = self.session_bus() else { return };
        if self.flatpak {
            // org.freedesktop.portal.Inhibit.Inhibit(reason, options, window)
            let dict = glib::VariantDict::new(None);
            dict.insert("handle_token", "frame_inhibit");
            dict.insert("reason", "media is playing");
            let options = dict.end();
            let r = bus
                .call_future(
                    Some("org.freedesktop.portal.Desktop"),
                    "/org/freedesktop/portal/desktop",
                    "org.freedesktop.portal.Inhibit",
                    "Inhibit",
                    Some(&("", options, 8u32).to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    5_000,
                )
                .await;
            if let Ok(reply) = r {
                // Portal returns an ObjectPath handle; keep it for uninhibit.
                let handle = reply
                    .try_get::<glib::Variant>()
                    .ok()
                    .and_then(|v| v.str().map(|s| s.to_string()));
                if let Some(h) = handle {
                    *PORTAL_HANDLE.lock().unwrap() = Some(h);
                }
            }
        } else {
            // org.freedesktop.ScreenSaver.Inhibit(application_name, reason) → u32
            let r = bus
                .call_future(
                    Some("org.freedesktop.ScreenSaver"),
                    "/org/freedesktop/ScreenSaver",
                    "org.freedesktop.ScreenSaver",
                    "Inhibit",
                    Some(&("Frame", "media is playing").to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    5_000,
                )
                .await;
            if let Ok(reply) = r {
                if let Ok(cookie) = reply.try_get::<u32>() {
                    *self.cookie.lock().unwrap() = Some(cookie);
                }
            }
        }
    }

    async fn uninhibit(&self) {
        let cookie = self.cookie.lock().unwrap().take();
        let Some(bus) = self.session_bus() else { return };
        if self.flatpak {
            if let Some(handle) = PORTAL_HANDLE.lock().unwrap().take() {
                // Close the portal request object.
                let obj_path = handle.clone();
                let _ = bus
                    .call_future(
                        Some("org.freedesktop.portal.Desktop"),
                        &obj_path,
                        "org.freedesktop.portal.Request",
                        "Close",
                        None,
                        None,
                        gio::DBusCallFlags::NONE,
                        5_000,
                    )
                    .await;
            }
        } else if let Some(cookie) = cookie {
            let _ = bus
                .call_future(
                    Some("org.freedesktop.ScreenSaver"),
                    "/org/freedesktop/ScreenSaver",
                    "org.freedesktop.ScreenSaver",
                    "UnInhibit",
                    Some(&cookie.to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    5_000,
                )
                .await;
        }
    }
}

static PORTAL_HANDLE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Media file extensions used for folder scans and drop classification
/// (choice recorded in IMPLEMENTATION_DECISIONS — not specified in docs).
pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "avi", "webm", "mov", "flv", "wmv", "m4v", "mpg", "mpeg", "ts", "ogv", "3gp", "m2ts", "mts", "divx", "vob", "ogm",
];

pub const SUBTITLE_EXTENSIONS: &[&str] = &["srt", "ass", "ssa", "vtt", "sub"];

pub const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "oga", "opus", "wav", "m4a", "aac", "wma"];

pub fn extension_of(name: &str) -> String {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub fn is_video_file(name: &str) -> bool {
    let ext = extension_of(name);
    VIDEO_EXTENSIONS.contains(&ext.as_str()) || AUDIO_EXTENSIONS.contains(&ext.as_str())
}

pub fn is_subtitle_file(name: &str) -> bool {
    SUBTITLE_EXTENSIONS.contains(&extension_of(name).as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification() {
        assert!(is_video_file("movie.mkv"));
        assert!(is_video_file("SONG.MP3")); // audio counts as playable media
        assert!(is_subtitle_file("subs.srt"));
        assert!(is_subtitle_file("subs.en.ass"));
        assert!(!is_video_file("image.png"));
        assert!(!is_subtitle_file("movie.mkv"));
    }
}
