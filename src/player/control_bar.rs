//! Player control overlay (docs/03): button row (left group ⏮ ▶ ⏭ 🔊 Ⓒ🇨 🎚,
//! right group 🔀 ⒶⒷ 🗨 ☰ ⚙ ⤢) above a full-width seek bar with
//! `current | duration` labels and a hover timestamp bubble.

use super::PlayerWindowCore;
use crate::player::popovers::{
    ChaptersPopover, PlaylistPopover, SettingsPopover, TrackPopover, VolumePopover,
};
use gtk::prelude::*;
use gtk::{gdk, glib, Align, Orientation};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub struct ControlBar {
    pub container: gtk::Box,
    pub prev_btn: gtk::Button,
    pub play_btn: gtk::Button,
    pub next_btn: gtk::Button,
    pub volume_btn: gtk::MenuButton,
    pub subs_btn: gtk::MenuButton,
    pub audio_btn: gtk::MenuButton,
    pub shuffle_btn: gtk::ToggleButton,
    pub ab_btn: gtk::Button,
    pub chapters_btn: gtk::MenuButton,
    pub playlist_btn: gtk::MenuButton,
    pub settings_btn: gtk::MenuButton,
    pub fullscreen_btn: gtk::Button,
    pub volume_popover: Rc<VolumePopover>,
    pub subs_popover: Rc<TrackPopover>,
    pub audio_popover: Rc<TrackPopover>,
    pub chapters_popover: Rc<ChaptersPopover>,
    pub playlist_popover: Rc<PlaylistPopover>,
    pub settings_popover: Rc<SettingsPopover>,
    pub seek: gtk::Scale,
    time_current: gtk::Label,
    time_duration: gtk::Label,
    pub hover_bubble: gtk::Label,
    /// True while the UI positions the scale programmatically.
    seek_guard: Rc<Cell<bool>>,
    /// Coalesced seek target (ms) of the running seek session.
    seek_pending: Rc<RefCell<Option<f64>>>,
    /// Scrub pump timer (completion-paced while the pointer keeps moving).
    scrub_timer: Rc<RefCell<Option<glib::SourceId>>>,
    /// Settle timer (debounced; lands exactly on the final value).
    settle_timer: Rc<RefCell<Option<glib::SourceId>>>,
    /// A new scrub target arrived since the pump last issued a seek.
    scrub_dirty: Rc<Cell<bool>>,
    /// When the pump last issued a scrub seek (deadlock guard).
    last_scrub_issue: Rc<Cell<std::time::Instant>>,
    /// The last scrub target actually issued — identical targets are
    /// skipped (mpv's OSC dedupes the same way).
    last_scrub_target: Rc<Cell<f64>>,
    /// True while a scrub session has playback frozen (restored on settle).
    scrub_paused: Rc<Cell<bool>>,
    /// True while the primary button is held on the seek bar (raw event
    /// tracking — the settle timer may end the *seek session* on change
    /// quiescence while the user is still holding the bar; the bar then
    /// still belongs to the pointer, never to the decoder's reports).
    drag_active: Rc<Cell<bool>>,
    /// The pipeline's pause state before the scrub froze it.
    was_paused: Rc<Cell<bool>>,
}

impl ControlBar {
    /// Minimum spacing between drag scrub seeks. Keyframe landings are
    /// cheap, but issuing one per pointer pixel would queue decodes behind
    /// each other; 50 ms keeps the scan at mpv-OSC pace without flooding.
    const SCRUB_MIN_MS: u64 = 50;

    pub fn new(core: &Rc<PlayerWindowCore>) -> Rc<Self> {
        // The scrim (video-scrim-controls) must span the window edge to
        // edge: margins on the overlay child would leave raw video showing
        // in a band at the bottom (glaring white spots on bright frames), so
        // the padding lives on this inner box instead.
        let container = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .css_classes(["controls-overlay", "video-scrim-controls"])
            .build();
        let content = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(6)
            .margin_top(6)
            .margin_bottom(12)
            .margin_start(16)
            .margin_end(16)
            .build();

        // --- button row ---------------------------------------------------
        let row = gtk::Box::builder().orientation(Orientation::Horizontal).spacing(12).build();

        let prev_btn = icon_button("media-skip-backward-symbolic", "Previous");
        let play_btn = icon_button("media-playback-start-symbolic", "Play/Pause");
        let next_btn = icon_button("media-skip-forward-symbolic", "Next");
        for b in [&prev_btn, &play_btn, &next_btn] {
            row.append(b);
        }

        let volume_popover = Rc::new(VolumePopover::new(core));
        let volume_btn = menu_button(&volume_popover.popover, "audio-volume-high-symbolic", "Volume");

        let subs_popover = Rc::new(TrackPopover::new(core, crate::engine::TrackKind::Sub));
        let subs_btn = menu_button(&subs_popover.popover, "media-view-subtitles-symbolic", "Subtitles (Ctrl+S)");

        let audio_popover = Rc::new(TrackPopover::new(core, crate::engine::TrackKind::Audio));
        let audio_btn = menu_button(&audio_popover.popover, "frame-audio-track-symbolic", "Audio (Ctrl+A)");

        for b in [&volume_btn, &subs_btn, &audio_btn] {
            row.append(b);
        }

        // Right group is right-aligned via a spacer.
        let spacer = gtk::Box::new(Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        row.append(&spacer);

        let shuffle_btn = gtk::ToggleButton::builder()
            .icon_name("media-playlist-shuffle-symbolic")
            .css_classes(["flat"])
            .tooltip_text("Shuffle")
            .visible(false) // only with ≥2 playlist items (docs/03)
            .build();
        let ab_btn = gtk::Button::builder()
            .icon_name("frame-ab-loop-symbolic")
            .css_classes(["flat"])
            .tooltip_text("Set/Clear A-B Loop Points (Ctrl+L)")
            .build();
        let chapters_popover = Rc::new(ChaptersPopover::new(core));
        let chapters_btn = menu_button(&chapters_popover.popover, "frame-chapters-symbolic", "Chapters (Ctrl+C)");
        let playlist_popover = Rc::new(PlaylistPopover::new(core));
        let playlist_btn = menu_button(&playlist_popover.popover, "view-list-symbolic", "Playlist");
        let settings_popover = Rc::new(SettingsPopover::new(core));
        let settings_btn = menu_button(&settings_popover.popover, "emblem-system-symbolic", "Playback settings");
        let fullscreen_btn = icon_button("view-fullscreen-symbolic", "Fullscreen (F11)");
        for b in [
            shuffle_btn.upcast_ref::<gtk::Widget>(),
            ab_btn.upcast_ref::<gtk::Widget>(),
            chapters_btn.upcast_ref::<gtk::Widget>(),
            playlist_btn.upcast_ref::<gtk::Widget>(),
            settings_btn.upcast_ref::<gtk::Widget>(),
            fullscreen_btn.upcast_ref::<gtk::Widget>(),
        ] {
            row.append(b);
        }
        content.append(&row);

        // --- seek row ------------------------------------------------------
        let seek_row = gtk::Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(10)
            .build();
        let seek = gtk::Scale::builder()
            .orientation(Orientation::Horizontal)
            .draw_value(false)
            .hexpand(true)
            .css_classes(["video-seek"])
            .build();
        seek.set_range(0.0, 1_000.0);
        let time_current = gtk::Label::builder().label("0:00").css_classes(["numeric"]).build();
        let sep = gtk::Label::new(Some("|"));
        let time_duration = gtk::Label::builder().label("0:00").css_classes(["numeric", "dim-label"]).build();
        seek_row.append(&seek);
        seek_row.append(&time_current);
        seek_row.append(&sep);
        seek_row.append(&time_duration);
        content.append(&seek_row);
        container.append(&content);

        // Hover timestamp chip (docs/03) — anchored bottom-start inside the
        // window's overlay; the player window positions it under the hover
        // thumbnail (or above the bar when no thumbnail is showing).
        let hover_bubble = crate::ui::util::hover_bubble_label();
        hover_bubble.add_css_class("time-chip");
        hover_bubble.set_halign(Align::Start);
        hover_bubble.set_valign(Align::End);
        hover_bubble.set_visible(false);

        let seek_guard = Rc::new(Cell::new(false));
        let seek_pending = Rc::new(RefCell::new(None::<f64>));
        // Scrub pump: armed once per interaction burst and re-arming itself
        // while a seek is in flight; it issues the next exact seek only when
        // the engine has finished the previous one, so the video tracks the
        // bar as fast as the file allows without ever queueing seeks.
        let scrub_timer = Rc::new(RefCell::new(None::<glib::SourceId>));
        let scrub_dirty = Rc::new(Cell::new(false));
        let last_scrub_issue = Rc::new(Cell::new(std::time::Instant::now()));
        let last_scrub_target = Rc::new(Cell::new(-1.0_f64));
        // Settle timer: re-armed (debounced) on every change; fires ~250 ms
        // after the last change of a session and lands exactly.
        let settle_timer = Rc::new(RefCell::new(None::<glib::SourceId>));

        // User-driven seeking, entirely timer-driven — no click gesture. The
        // Scale's own drag gesture claims the pointer sequence, so a
        // GestureClick on the bar never sees the release (it gets cancelled
        // instead) and cannot anchor the exact landing.
        let scrub_paused = Rc::new(Cell::new(false));
        let was_paused = Rc::new(Cell::new(false));
        // Raw button tracking (EventControllerLegacy only *observes*; the
        // scale's internal drag gesture keeps owning the sequence). The
        // settle timer fires on change quiescence — while the user is
        // simply holding the bar still — and after it the decoder's
        // position reports must never yank the thumb away from the pointer.
        let drag_active = Rc::new(Cell::new(false));
        {
            let drag_active2 = drag_active.clone();
            let drag_active3 = drag_active.clone();
            let legacy = gtk::EventControllerLegacy::new();
            legacy.connect_event(move |_, event| {
                match event.event_type() {
                    gdk::EventType::ButtonPress => {
                        if event
                            .downcast_ref::<gdk::ButtonEvent>()
                            .map(|b| b.button() == 1)
                            .unwrap_or(false)
                        {
                            if std::env::var_os("FRAME_DEBUG").is_some() {
                                eprintln!("frame: bar press");
                            }
                            drag_active2.set(true);
                        }
                    }
                    gdk::EventType::ButtonRelease => {
                        if event
                            .downcast_ref::<gdk::ButtonEvent>()
                            .map(|b| b.button() == 1)
                            .unwrap_or(false)
                        {
                            if std::env::var_os("FRAME_DEBUG").is_some() {
                                eprintln!("frame: bar release");
                            }
                            drag_active3.set(false);
                        }
                    }
                    _ => {}
                }
                glib::Propagation::Proceed
            });
            seek.add_controller(legacy);
        }
        seek.connect_change_value(
            glib::clone!(#[strong] core, #[strong] seek_guard, #[strong] seek_pending, #[strong] scrub_timer, #[strong] scrub_dirty, #[strong] last_scrub_issue, #[strong] last_scrub_target, #[strong] settle_timer, #[strong] scrub_paused, #[strong] was_paused, #[strong] drag_active, move |scale, _scroll, value| {
                let _ = scale;
                if seek_guard.get() {
                    return glib::Propagation::Proceed;
                }
                // First change of a session (settle timer not yet armed):
                // scrub stays off and the exact seek fires right away, so a
                // plain trough click lands instantly.
                let scrub = settle_timer.borrow().is_some();
                *seek_pending.borrow_mut() = Some(value);
                scrub_dirty.set(true);
                // The settle timer debounces: when it fires ~250 ms after
                // the last change, one exact seek lands on the final value —
                // for drags, wheel bursts and keyboard steps alike — and
                // playback resumes from there.
                if let Some(old) = settle_timer.borrow_mut().take() {
                    old.remove();
                }
                let s_pending = seek_pending.clone();
                let s_settle = settle_timer.clone();
                let s_settle_dirty = scrub_dirty.clone();
                let core_settle = core.clone();
                let s_scrub_paused = scrub_paused.clone();
                let s_was_paused = was_paused.clone();
                let s_drag = drag_active.clone();
                settle_timer.borrow_mut().replace(glib::timeout_add_local(
                    std::time::Duration::from_millis(250),
                    move || {
                        // A human drag has micro-pauses longer than the
                        // debounce; ending the session (and paying an exact
                        // seek's decode) while the button is still down read
                        // as a stall mid-scrub. While the pointer is held,
                        // the landing waits for the release.
                        if s_drag.get() {
                            return glib::ControlFlow::Continue;
                        }
                        // The drag's keyframe steps never land precisely —
                        // this exact seek is the landing. None here means the
                        // session was a single press (already seeked exactly
                        // at press time) and there is nothing to land.
                        if let Some(v) = s_pending.borrow_mut().take() {
                            core_settle.player.seek_absolute(v / 1000.0);
                            // The landing frame must show even when the
                            // scrub's pause stays (scrubbed from paused).
                            core_settle.player.request_render_passes();
                        }
                        s_settle_dirty.set(false);
                        if s_scrub_paused.replace(false) && !s_was_paused.get() {
                            core_settle.player.set_pause(false);
                        }
                        *s_settle.borrow_mut() = None;
                        glib::ControlFlow::Break
                    },
                ));
                if scrub {
                    // Freeze playback for the scrub: a playing 180 fps
                    // decode flood fights the seeks for the main loop
                    // (the owner's "still laggy"); paused, each seek decodes
                    // and shows exactly one frame. The settle timer restores
                    // the previous pause state.
                    if !scrub_paused.get() {
                        scrub_paused.set(true);
                        was_paused.set(core.player.is_paused());
                        if !was_paused.get() {
                            core.player.set_pause(true);
                        }
                    }
                    // mpv's own OSC issues the seek straight from the
                    // mouse-move event (its only rate control is skipping
                    // identical targets) and drags with KEYFRAME seeks,
                    // which land at a fraction of an exact seek's decode
                    // cost — that is why its drag scans smoothly on this
                    // file. (An earlier keyframe attempt here read as "the
                    // picture did not move" only because it predated the
                    // paused render window: the seeks landed, but nothing
                    // published them.) The cap below keeps one seek from
                    // queueing behind another; the timer catches bursts
                    // arriving faster than the cap.
                    if last_scrub_issue.get().elapsed()
                        >= std::time::Duration::from_millis(Self::SCRUB_MIN_MS)
                    {
                        // Peek, don't take: the settle timer must still find
                        // the final value to land on it EXACTLY (the drag
                        // steps only ever reach keyframes). Identical targets
                        // are skipped — mpv's OSC dedupes the same way.
                        if let Some(v) = *seek_pending.borrow() {
                            if (v - last_scrub_target.get()).abs() > 1e-9 {
                                core.player.seek_absolute_keyframes(v / 1000.0);
                                last_scrub_target.set(v);
                                last_scrub_issue.set(std::time::Instant::now());
                            }
                            scrub_dirty.set(false);
                        }
                    }
                    if scrub_timer.borrow().is_some() {
                        return glib::Propagation::Stop;
                    }
                    let s_pending = seek_pending.clone();
                    let s_dirty = scrub_dirty.clone();
                    let s_last = last_scrub_issue.clone();
                    let s_target = last_scrub_target.clone();
                    let s_scrub = scrub_timer.clone();
                    let core_scrub = core.clone();
                    scrub_timer.borrow_mut().replace(glib::timeout_add_local(
                        std::time::Duration::from_millis(Self::SCRUB_MIN_MS),
                        move || {
                            // Keyframe steps never wait for the previous
                            // seek to land — a new seek aborts the in-flight
                            // one, so a drag costs at most one decode per
                            // step (waiting for completion measured
                            // ~500 ms per step on the owner's file).
                            if s_dirty.get() && s_last.get().elapsed()
                                >= std::time::Duration::from_millis(Self::SCRUB_MIN_MS)
                            {
                                // Peek only — the settle timer owns the
                                // pending slot and lands on it exactly.
                                if let Some(v) = *s_pending.borrow() {
                                    if (v - s_target.get()).abs() > 1e-9 {
                                        core_scrub.player.seek_absolute_keyframes(v / 1000.0);
                                        s_target.set(v);
                                        s_last.set(std::time::Instant::now());
                                    }
                                }
                                s_dirty.set(false);
                            }
                            if s_dirty.get() {
                                glib::ControlFlow::Continue
                            } else {
                                *s_scrub.borrow_mut() = None;
                                glib::ControlFlow::Break
                            }
                        },
                    ));
                } else {
                    // Session start (the press): land exactly on the pressed
                    // position right away, like mpv's OSC does on mousedown,
                    // so a plain trough click gives instant feedback and a
                    // drag starts with the picture at the pointer. The
                    // settle timer stays armed only to close the session if
                    // nothing further happens (the landing seek is already
                    // issued, so it will find no pending value).
                    if let Some(v) = seek_pending.borrow_mut().take() {
                        core.player.seek_absolute(v / 1000.0);
                    }
                }
                glib::Propagation::Stop
            },
        ));

        // Hover chip position: the player window owns the overlay placement
        // (chip under the thumbnail when one is visible).
        let motion = gtk::EventControllerMotion::new();
        {
            let core = core.clone();
            let scale = seek.clone();
            let bubble = hover_bubble.clone();
            let scrubbing = scrub_paused.clone();
            let scrub_thumb_at = Rc::new(Cell::new(std::time::Instant::now()));
            motion.connect_motion(move |_, x, y| {
                let duration = core.player.get_double("duration").unwrap_or(0.0);
                let width = scale.width().max(1) as f64;
                let frac = (x / width).clamp(0.0, 1.0);
                let t = frac * duration;
                bubble.set_text(&crate::ui::util::format_time(t));
                bubble.set_visible(duration > 0.0);
                core.hover_x.set(x);
                core.reposition_time_chip();
                // The visible preview glides with the pointer between
                // content updates (its frames land on decode time, the
                // position must not wait for them).
                core.reposition_thumb_preview();
                // While a scrub session is active the hover preview's seeks
                // on the hidden second instance still compete with the scrub
                // seeks for decode — throttle them to one every 150 ms
                // instead of suppressing them entirely (owner: more frequent
                // thumbnails; the thumbnailer seeks keyframe-only, so its
                // steps land fast even on heavy files).
                if !scrubbing.get() {
                    core.queue_seek_thumbnail(t);
                } else if scrub_thumb_at.get().elapsed()
                    >= std::time::Duration::from_millis(150)
                {
                    scrub_thumb_at.set(std::time::Instant::now());
                    core.queue_seek_thumbnail(t);
                }
                let _ = y;
            });
        }
        {
            let core = core.clone();
            let bubble = hover_bubble.clone();
            motion.connect_leave(move |_| {
                bubble.set_visible(false);
                if let Some(picture) = core.thumb_picture.borrow().as_ref() {
                    picture.set_visible(false);
                }
                // Cancel any pending thumbnail timer so the window does not
                // light up after the pointer left.
                if let Some(t) = core.thumb_timer.borrow_mut().take() {
                    t.remove();
                }
            });
        }
        seek.add_controller(motion);

        // Clicks.
        play_btn.connect_clicked(glib::clone!(#[strong] core, move |_| {
            core.toggle_play();
        }));
        prev_btn.connect_clicked(glib::clone!(#[strong] core, move |_| {
            core.previous_item();
        }));
        next_btn.connect_clicked(glib::clone!(#[strong] core, move |_| {
            core.next_item();
        }));
        shuffle_btn.connect_toggled(glib::clone!(#[strong] core, move |btn| {
            core.set_shuffle(btn.is_active());
        }));
        ab_btn.connect_clicked(glib::clone!(#[strong] core, move |_| {
            core.cycle_ab_loop();
        }));
        fullscreen_btn.connect_clicked(glib::clone!(#[strong] core, move |_| {
            core.toggle_fullscreen();
        }));

        Rc::new(Self {
            container,
            prev_btn,
            play_btn,
            next_btn,
            volume_btn,
            subs_btn,
            audio_btn,
            shuffle_btn,
            ab_btn,
            chapters_btn,
            playlist_btn,
            settings_btn,
            fullscreen_btn,
            volume_popover,
            subs_popover,
            audio_popover,
            chapters_popover,
            playlist_popover,
            settings_popover,
            seek,
            time_current,
            time_duration,
            hover_bubble,
            seek_guard,
            seek_pending,
            scrub_timer,
            scrub_dirty,
            last_scrub_issue,
            last_scrub_target,
            settle_timer,
            scrub_paused,
            drag_active,
            was_paused,
        })
    }

    pub fn update_play_icon(&self, paused: bool) {
        self.play_btn.set_icon_name(if paused {
            "media-playback-start-symbolic"
        } else {
            "media-playback-pause-symbolic"
        });
    }

    pub fn update_playlist_buttons(&self, len: usize) {
        // Disabled/dimmed with a single-item playlist (docs/03).
        self.prev_btn.set_sensitive(len > 1);
        self.next_btn.set_sensitive(len > 1);
        // Shuffle only visible with ≥2 items (docs/03).
        self.shuffle_btn.set_visible(len > 1);
    }

    pub fn update_time(&self, pos: Option<f64>, duration: Option<f64>) {
        let dur = duration.unwrap_or(0.0);
        let pos = pos.unwrap_or(0.0).min(dur);
        self.time_duration.set_text(&crate::ui::util::format_time(dur));
        if self.scrub_paused.get() || self.drag_active.get() {
            // Scrub session or bar held: the pointer is the truth. The
            // engine's reported position is the last *landed* seek —
            // snapping the bar back to it mid-drag is exactly the
            // choppiness the owner described. The thumb already sits at
            // the pointer; keep the label on the bar's value and leave
            // the bar alone.
            if std::env::var_os("FRAME_DEBUG").is_some() {
                eprintln!(
                    "frame: update_time guarded (scrub={} drag={}) pos={pos:.2}",
                    self.scrub_paused.get(),
                    self.drag_active.get()
                );
            }
            let v = self.seek.value() / 1000.0;
            self.time_current.set_text(&crate::ui::util::format_time(v));
            return;
        }
        self.time_current.set_text(&crate::ui::util::format_time(pos));
        if dur > 0.0 {
            self.seek_guard.set(true);
            self.seek.set_range(0.0, (dur * 1000.0).max(1.0));
            self.seek.set_value(pos * 1000.0);
            self.seek_guard.set(false);
            self.seek.set_sensitive(true);
        } else {
            self.seek_guard.set(true);
            self.seek.set_value(0.0);
            self.seek_guard.set(false);
            self.seek.set_sensitive(false);
        }
    }

    pub fn update_ab_state(&self, a: Option<f64>, b: Option<f64>) {
        let active = a.is_some() || b.is_some();
        self.ab_btn
            .set_css_classes(if active { &["flat", "accent"][..] } else { &["flat"][..] });
    }

    pub fn update_volume_icon(&self, volume: f64, muted: bool) {
        let icon = if muted || volume <= 0.0 {
            "audio-volume-muted-symbolic"
        } else if volume < 34.0 {
            "audio-volume-low-symbolic"
        } else if volume < 67.0 {
            "audio-volume-medium-symbolic"
        } else {
            "audio-volume-high-symbolic"
        };
        self.volume_btn.set_icon_name(icon);
    }

    /// Icons showing a "captions off" state when no subtitle track is active
    /// (docs/03) — dimmed via opacity.
    pub fn update_subs_state(&self, has_active_sub: bool) {
        self.subs_btn.set_opacity(if has_active_sub { 1.0 } else { 0.55 });
    }

    pub fn set_seekable(&self, seekable: bool) {
        self.seek.set_sensitive(seekable);
    }
}

fn icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    gtk::Button::builder()
        .icon_name(icon)
        .css_classes(["flat", "circular"])
        .tooltip_text(tooltip)
        .valign(Align::Center)
        .build()
}

fn menu_button(popover: &gtk::Popover, icon: &str, tooltip: &str) -> gtk::MenuButton {
    gtk::MenuButton::builder()
        .icon_name(icon)
        .popover(popover)
        .css_classes(["flat", "circular"])
        .tooltip_text(tooltip)
        .valign(Align::Center)
        .build()
}
