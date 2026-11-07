//! Statistics page + pinned overlay (I / Shift+I, docs/05).
//!
//! The original's statistics UI was never visually exercised (docs/07 §2),
//! so Frame renders its own page from engine properties. `I` shows it
//! temporarily (auto-hides); `Shift+I` pins it until pressed again.

use super::PlayerWindowCore;
use gtk::prelude::*;
use gtk::{glib, Align};
use std::cell::Cell;
use std::rc::Rc;

const MONO_CSS: &str = "statistics-overlay";

pub struct Stats {
    label: gtk::Label,
    pinned: Cell<bool>,
    temp_source: Cell<Option<glib::SourceId>>,
    poll_source: Cell<Option<glib::SourceId>>,
}

impl Stats {
    pub fn new(overlay: &gtk::Overlay) -> Rc<Self> {
        let label = gtk::Label::builder()
            .halign(Align::Start)
            .valign(Align::Start)
            .margin_top(12)
            .margin_start(12)
            .visible(false)
            .css_classes(["osd", MONO_CSS])
            .wrap(true)
            .build();
        label.set_use_markup(false);
        overlay.add_overlay(&label);

        let this = Rc::new(Self {
            label,
            pinned: Cell::new(false),
            temp_source: Cell::new(None),
            poll_source: Cell::new(None),
        });
        this
    }

    pub fn is_visible(&self) -> bool {
        self.label.is_visible()
    }

    pub fn toggle_temporary(self: &Rc<Self>, core: &Rc<PlayerWindowCore>) {
        if self.pinned.get() {
            return; // pinned wins
        }
        if self.is_visible() {
            self.hide();
        } else {
            self.show(core);
            if let Some(old) = self.temp_source.replace(None) {
                old.remove();
            }
            let this = Rc::downgrade(self);
            self.temp_source.set(Some(glib::timeout_add_local(
                std::time::Duration::from_secs(4),
                move || {
                    if let Some(this) = this.upgrade() {
                        this.hide();
                        // The source ends here; drop the stale id so a later
                        // replace(None) never removes an already-freed source.
                        this.temp_source.set(None);
                    }
                    glib::ControlFlow::Break
                },
            )));
        }
    }

    pub fn toggle_pinned(self: &Rc<Self>, core: &Rc<PlayerWindowCore>) {
        let pin = !self.pinned.get();
        self.pinned.set(pin);
        if let Some(old) = self.temp_source.replace(None) {
            old.remove();
        }
        if pin {
            self.show(core);
        } else {
            self.hide();
        }
    }

    fn show(self: &Rc<Self>, core: &Rc<PlayerWindowCore>) {
        // The header bar overlays the video (docs/03): clear it in windowed
        // mode; in fullscreen the chrome hides, so sit at the corner.
        let top = if core.window.is_fullscreen() { 12 } else { 56 };
        self.label.set_margin_top(top);
        self.label.set_visible(true);
        self.refresh(core);
        // Poll once per second while visible.
        if self.poll_source.replace(None).is_none() {
            let this = Rc::downgrade(self);
            let core_weak = Rc::downgrade(core);
            self.poll_source.set(Some(glib::timeout_add_local(
                std::time::Duration::from_secs(1),
                move || {
                    match (this.upgrade(), core_weak.upgrade()) {
                        (Some(this), Some(core)) => {
                            if !this.label.is_visible() {
                                return glib::ControlFlow::Break;
                            }
                            this.refresh(&core);
                            glib::ControlFlow::Continue
                        }
                        _ => glib::ControlFlow::Break,
                    }
                },
            )));
        }
    }

    pub fn hide(&self) {
        self.label.set_visible(false);
        if let Some(p) = self.poll_source.take() {
            p.remove();
        }
    }

    fn refresh(&self, core: &PlayerWindowCore) {
        let p = &core.player;
        let lines = [
            format!("File: {}", core.title()),
            format!(
                "Position: {} / {}",
                crate::ui::util::format_time(p.position().unwrap_or(0.0)),
                crate::ui::util::format_time(p.get_double("duration").unwrap_or(0.0))
            ),
            format!("Paused: {}", if p.is_paused() { "yes" } else { "no" }),
            format!("Speed: {:.2}×", p.speed()),
            format!("Container: {}", p.get_string("container-format").unwrap_or_else(|| "n/a".into())),
            format!("Video codec: {}", p.get_string("video-format").unwrap_or_else(|| "none".into())),
            format!(
                "Video size: {}×{}",
                p.get_i64("width").unwrap_or(0),
                p.get_i64("height").unwrap_or(0)
            ),
            format!("FPS: {:.3}", p.get_double("container-fps").unwrap_or(0.0)),
            format!("Hardware decode: {}", p.get_string("hwdec-current").unwrap_or_else(|| "no".into())),
            format!("Audio codec: {}", p.get_string("audio-codec-name").unwrap_or_else(|| "none".into())),
            format!(
                "Audio: {} Hz, {} ch",
                p.get_i64("audio-params/samplerate").unwrap_or(0),
                p.get_i64("audio-params/channel-count").unwrap_or(0)
            ),
            format!(
                "A/V sync: {:+.3}",
                p.get_double("avsync").unwrap_or(f64::NAN)
            ),
            format!(
                "Dropped frames: decoder {}, output {}",
                p.get_i64("frame-drop-count").unwrap_or(0),
                p.get_i64("vo-drop-frame-count").unwrap_or(0)
            ),
            format!(
                "Cache: {:.1} s buffered",
                p.get_double("demuxer-cache-time").unwrap_or(0.0)
            ),
            format!(
                "Volume: {:.0}%{}",
                p.get_double("volume").unwrap_or(0.0),
                if p.get_flag("mute").unwrap_or(false) { " (muted)" } else { "" }
            ),
        ];
        self.label.set_text(&lines.join("\n"));
    }
}
