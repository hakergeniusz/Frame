//! Player window (docs/03, docs/06): one player window per media item, video
//! surface with overlaid chrome, control bar with popovers, toasts, full
//! keyboard handling, click mapping, and drag-and-drop.

pub mod control_bar;
pub mod playlist;
pub mod popovers;
pub mod stats;
pub mod video_paintable;

use crate::config::{PrimaryClickAction, SecondaryClickAction};
use crate::engine::{ChapterInfo, EndReason, EngineEvent, MediaSource, MpvPlayer, TrackInfo, TrackKind};
use crate::rt::AppRuntime;
use crate::resume::VideoAdjustments;
use adw::prelude::*;
use gtk::{gdk, gio, glib};
use gtk::{Align, Orientation};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use control_bar::ControlBar;
use playlist::Playlist;
use stats::Stats;
use video_paintable::VideoPaintable;

/// Unified UI event: engine events + window-logic events.
#[derive(Debug, Clone)]
pub enum UiEvent {
    Engine(EngineEvent),
    PlaylistChanged,
    TitleChanged,
}

// ---------------------------------------------------------------------------
// Core: engine + playlist + window state
// ---------------------------------------------------------------------------

pub struct PlayerWindowCore {
    pub rt: Arc<AppRuntime>,
    pub window: adw::ApplicationWindow,
    pub player: Arc<MpvPlayer>,
    pub toast_overlay: adw::ToastOverlay,
    pub events: async_channel::Sender<UiEvent>,
    pub engine_rx: async_channel::Receiver<UiEvent>,
    render_wakeup: RefCell<Option<async_channel::Sender<()>>>,
    playlist: RefCell<Playlist>,
    pub tracks: RefCell<Vec<TrackInfo>>,
    pub chapters: RefCell<Vec<ChapterInfo>>,
    pending_position: Cell<Option<f64>>,
    pending_adjustments: Cell<Option<VideoAdjustments>>,
    render_ready: Cell<bool>,
    pending_play: RefCell<Option<(usize, Option<f64>)>>,
    resume_applied: Cell<bool>,
    ab_loop: Cell<(Option<f64>, Option<f64>)>,
    inhibiting: Cell<bool>,
    title: RefCell<String>,
    offload: RefCell<Option<gtk::GraphicsOffload>>,
    // Video surface: a Picture backed by VideoPaintable (frames arrive as
    // GdkTextures) plus the GdkGLContext the engine renders with.
    video_picture: RefCell<Option<gtk::Picture>>,
    gl_context: RefCell<Option<gdk::GLContext>>,
    // Video widget's pixel size (logical size × scale), tracked on resize.
    video_size: Cell<(i32, i32)>,
    // Coalescing flag between the engine's render wakeup and the video
    // picture's frame-clock tick: mpv signals per decoded frame (an 180 fps
    // file nudges 180×/s), but a publish costs a full offscreen render plus a
    // synchronous readback, so at most one happens per display frame. The
    // tick consumes it; the wakeup side only sets it and requests a redraw.
    frame_dirty: Cell<bool>,
    // Seek-bar thumbnails: registered control widgets, result cache, and the
    // hover position awaiting the next engine event.
    thumb_picture: RefCell<Option<gtk::Picture>>,
    seek_widget: RefCell<Option<gtk::Scale>>,
    control_container: RefCell<Option<gtk::Box>>,
    thumb_cache: RefCell<HashMap<(String, i64), gdk::MemoryTexture>>,
    thumb_timer: RefCell<Option<glib::SourceId>>,
    hover_time: Cell<f64>,
    hover_x: Cell<f64>,
    // True while the pointer is on the seek bar: a thumbnail decode that
    // was already in flight when the pointer left must not present itself
    // afterwards (leaving cancels the debounce timer, not the decode).
    bar_hovering: Cell<bool>,
    // Duration cache for the hot hover/position paths: reading the property
    // out of mpv takes the core lock, and doing that per motion event or per
    // time-pos report (an 180 fps file reports ~180×/s) contends with the
    // decode loop on the UI thread. Maintained by the engine events.
    cached_duration: Cell<f64>,
    // Set once PlayerWindow::build finished (weak: the window owns the core).
    player_window: RefCell<std::rc::Weak<PlayerWindow>>,
}

impl PlayerWindowCore {
    /// Hover debounce before a thumbnail request goes out — short enough
    /// that sliding along the bar keeps the preview following (owner
    /// request for more frequent thumbnails).
    const THUMB_DEBOUNCE_MS: u64 = 120;
    /// Thumbnail cache entries (half-second slots per file; ~186 KB each,
    /// clearing wholesale instead of LRU keeps the code simple).
    const THUMB_CACHE_CAP: usize = 128;

    fn new(rt: Arc<AppRuntime>) -> Result<Rc<Self>, String> {
        let window = adw::ApplicationWindow::builder()
            .title("Frame")
            .default_width(1070)
            .default_height(600)
            .build();
        if let Some(app) = gio::Application::default() {
            window.set_application(Some(app.downcast_ref::<adw::Application>().unwrap()));
        }

        let (tx, rx) = async_channel::unbounded::<UiEvent>();
        let (etx, erx) = async_channel::unbounded::<EngineEvent>();
        let s = rt.settings.get();
        let player = MpvPlayer::new(etx, !s.software_decoding, s.normalize_volume)?;

        // Bridge engine events into the unified channel.
        {
            let tx = tx.clone();
            glib::spawn_future_local(async move {
                while let Ok(e) = erx.recv().await {
                    let _ = tx.send_blocking(UiEvent::Engine(e));
                }
            });
        }

        let toast_overlay = adw::ToastOverlay::new();

        let core = Rc::new(Self {
            rt,
            window: window.clone(),
            player,
            toast_overlay,
            events: tx,
            engine_rx: rx,
            render_wakeup: RefCell::new(None),
            playlist: RefCell::new(Playlist::default()),
            tracks: RefCell::new(Vec::new()),
            chapters: RefCell::new(Vec::new()),
            pending_position: Cell::new(None),
            pending_adjustments: Cell::new(None),
            render_ready: Cell::new(false),
            pending_play: RefCell::new(None),
            resume_applied: Cell::new(false),
            ab_loop: Cell::new((None, None)),
            inhibiting: Cell::new(false),
            title: RefCell::new("Frame".into()),
            offload: RefCell::new(None),
            video_picture: RefCell::new(None),
            gl_context: RefCell::new(None),
            video_size: Cell::new((0, 0)),
            frame_dirty: Cell::new(false),
            thumb_picture: RefCell::new(None),
            seek_widget: RefCell::new(None),
            control_container: RefCell::new(None),
            thumb_cache: RefCell::new(HashMap::new()),
            thumb_timer: RefCell::new(None),
            hover_time: Cell::new(0.0),
            hover_x: Cell::new(0.0),
            bar_hovering: Cell::new(false),
            cached_duration: Cell::new(0.0),
            player_window: RefCell::new(std::rc::Weak::new()),
        });

        core.rt.register_player(&core);
        Ok(core)
    }

    // -- playlist -------------------------------------------------------------

    pub fn playlist_item_sources(&self) -> Vec<MediaSource> {
        self.playlist.borrow().items().iter().map(|i| i.source.clone()).collect()
    }

    pub fn playlist_current_index(&self) -> Option<usize> {
        self.playlist.borrow().current_index()
    }

    pub fn playlist_current_source(&self) -> Option<MediaSource> {
        self.playlist.borrow().current_source()
    }

    pub fn playlist_len(&self) -> usize {
        self.playlist.borrow().len()
    }

    // -- seek-bar thumbnails (docs/03) ---------------------------------------

    /// A hover position on the seek bar: remember it and (debounced) ask the
    /// engine's hidden thumbnailer for a frame — only when the preference is
    /// enabled and a local file is playing.
    pub fn queue_seek_thumbnail(self: &Rc<Self>, t: f64) {
        self.hover_time.set(t);
        if !self.rt.settings.get().seek_bar_thumbnail {
            return;
        }
        if self.thumb_timer.borrow().is_some() {
            return; // already scheduled; it picks up the newest hover_time
        }
        let core = Rc::downgrade(self);
        self.thumb_timer.borrow_mut().replace(glib::timeout_add_local(
            std::time::Duration::from_millis(Self::THUMB_DEBOUNCE_MS),
            move || {
                // The source ends here; drop the id before dispatching so a
                // later timer can be scheduled immediately.
                if let Some(core) = core.upgrade() {
                    *core.thumb_timer.borrow_mut() = None;
                    core.dispatch_seek_thumbnail();
                }
                glib::ControlFlow::Break
            },
        ));
    }

    fn dispatch_seek_thumbnail(self: &Rc<Self>) {
        let dur = self.player.get_double("duration").unwrap_or(0.0);
        if dur <= 0.0 {
            return;
        }
        // Local files only — a hidden instance fetching network media per
        // hover would be indistinguishable from a stall.
        let Some(MediaSource::Path(path)) = self.playlist_current_source() else {
            return;
        };
        let pos = self.hover_time.get().clamp(0.0, dur);
        if std::env::var_os("FRAME_DEBUG").is_some() {
            eprintln!("frame: dispatch thumbnail pos={pos:.1} paused={}", self.player.is_paused());
        }
        // Half-second cache granularity — the preview follows the pointer
        // more closely while sliding along the bar (owner request).
        let key = (path.clone(), (pos * 2.0).round() as i64);
        if let Some(tex) = self.thumb_cache.borrow().get(&key) {
            self.present_thumbnail(&tex, pos);
            return;
        }
        let vw = self.player.get_i64("width").unwrap_or(0);
        let vh = self.player.get_i64("height").unwrap_or(0);
        let (tw, th) = if vw > 0 && vh > 0 {
            (288, ((288.0 * vh as f64 / vw as f64).round() as i32).max(1))
        } else {
            (288, 162)
        };
        self.player.request_thumbnail(path, pos, tw, th);
        // While paused there are no spontaneous render passes — ask for one.
        if self.player.is_paused() {
            self.player.nudge_render();
        }
    }

    /// Engine produced a thumbnail frame: cache it (per file + second) and
    /// show it over the seek bar.
    fn on_thumbnail_ready(self: &Rc<Self>, path: String, pos: f64, width: i32, height: i32, rgba: &[u8]) {
        if std::env::var_os("FRAME_DEBUG").is_some() {
            // Centre patch average (corners are letterbox bars in most files).
            let (w, h) = (width.max(1) as usize, height.max(1) as usize);
            let cx = w / 2;
            let cy = h / 2;
            let mut acc = 0u64;
            let mut n = 0u64;
            for yy in cy.saturating_sub(8)..(cy + 8).min(h) {
                for xx in cx.saturating_sub(8)..(cx + 8).min(w) {
                    acc += rgba[(yy * w + xx) * 4] as u64;
                    n += 1;
                }
            }
            eprintln!(
                "frame: thumbnail ready pos={pos} {width}x{height} centre_r={}",
                if n > 0 { acc / n } else { 0 }
            );
        }
        let bytes = glib::Bytes::from(&rgba[..(width.max(1) * height.max(1) * 4) as usize]);
        let tex = gdk::MemoryTexture::new(
            width.max(1),
            height.max(1),
            gdk::MemoryFormat::R8g8b8a8,
            &bytes,
            (width.max(1) * 4) as usize,
        );
        {
            let mut cache = self.thumb_cache.borrow_mut();
            if cache.len() > Self::THUMB_CACHE_CAP {
                cache.clear();
            }
            cache.insert((path, (pos * 2.0).round() as i64), tex.clone());
        }
        self.present_thumbnail(&tex, pos);
        // While paused, that queue_render already produced this frame; if the
        // pointer kept moving a new request is already queued.
    }

    /// Centre `widget` (a small overlay: thumbnail or timestamp chip) above
    /// the control bar at the pointer's position on the seek bar. `lift`
    /// raises the widget further (the thumbnail is lifted above the chip).
    fn place_above_controls(&self, widget: &gtk::Widget, x_on_seek: f64, width: f64, lift: f64) {
        let container_cell = self.control_container.borrow();
        let Some(container) = container_cell.as_ref() else {
            return;
        };
        let seek_cell = self.seek_widget.borrow();
        let Some(seek) = seek_cell.as_ref() else {
            return;
        };
        // Map the pointer's position on the seek bar into window coordinates.
        let px = match seek.compute_point(&self.window, &gtk::graphene::Point::new(x_on_seek as f32, 0.0)) {
            Some(p) => p.x() as f64,
            None => x_on_seek,
        };
        let win_w = self.window.width() as f64;
        let margin = ((px - width / 2.0).round() as i32).clamp(4, (win_w - width - 4.0).max(4.0) as i32);
        widget.set_margin_start(margin);
        widget.set_margin_bottom(container.height() + 12 + lift.round() as i32);
    }

    /// Show a thumbnail centred on the pointer above the control bar, lifted
    /// so the timestamp chip fits between it and the bar (docs/03 popup).
    fn present_thumbnail(self: &Rc<Self>, tex: &gdk::MemoryTexture, _pos: f64) {
        // The decode finished after the pointer left the bar: drop the
        // frame instead of lighting the overlay up on its own.
        if !self.bar_hovering.get() {
            return;
        }
        let picture_cell = self.thumb_picture.borrow();
        let Some(picture) = picture_cell.as_ref() else {
            return;
        };
        let (tw, th) = (tex.width() as f64, tex.height() as f64);
        let disp_w = 256.0_f64.min(tw);
        let disp_h = (disp_w * th / tw).round().max(1.0);
        picture.set_paintable(Some(tex));
        picture.set_size_request(disp_w as i32, disp_h as i32);
        // Chip height + gap; the chip itself sits at the base position.
        let lift = 26.0 + 8.0;
        self.place_above_controls(picture.upcast_ref::<gtk::Widget>(), self.hover_x.get(), disp_w, lift);
        picture.set_visible(true);
        drop(picture_cell);
        if std::env::var_os("FRAME_DEBUG").is_some() {
            eprintln!("frame: thumbnail shown {tw}x{th} at x={}", self.hover_x.get());
        }
    }

    /// Re-place the timestamp chip for the current hover position (used by
    /// the hover motion handler and whenever a thumbnail appears).
    pub fn reposition_time_chip(self: &Rc<Self>) {
        // Handled by PlayerWindow (owns the bubble); no-op until built.
        if let Some(win) = self.player_window.borrow().upgrade() {
            win.reposition_time_chip();
        }
    }

    /// Re-place the visible thumbnail preview for the current hover
    /// position. The preview's *content* only changes when a frame lands
    /// (decodes take time); its *position* must glide with the pointer
    /// between those landings — the UI never shows the waiting.
    pub fn reposition_thumb_preview(self: &Rc<Self>) {
        if let Some(win) = self.player_window.borrow().upgrade() {
            win.reposition_thumb_preview();
        }
    }

    pub fn title(&self) -> String {
        self.title.borrow().clone()
    }

    fn update_title(&self, source: &MediaSource) {
        let t = crate::engine::display_title(source);
        *self.title.borrow_mut() = t.clone();
        self.window.set_title(Some(&t));
    }

    fn update_title_none(&self) {
        *self.title.borrow_mut() = "Frame".into();
        self.window.set_title(Some("Frame"));
    }

    pub fn add_items(self: &Rc<Self>, sources: Vec<MediaSource>) {
        if sources.is_empty() {
            return;
        }
        let first_new = self.playlist.borrow_mut().add(sources);
        if let Some(idx) = first_new {
            // Adding to the playlist focuses/plays the new item (docs/06).
            self.play_index(idx);
        } else {
            let _ = self.events.send_blocking(UiEvent::PlaylistChanged);
        }
    }

    pub fn replace_items(self: &Rc<Self>, sources: Vec<MediaSource>) {
        let first = self.playlist.borrow_mut().replace(sources);
        match first {
            Some(idx) => self.play_index(idx),
            None => {
                self.player.stop();
                self.update_title_none();
                let _ = self.events.send_blocking(UiEvent::PlaylistChanged);
                let _ = self.events.send_blocking(UiEvent::TitleChanged);
            }
        }
    }

    pub fn play_index(self: &Rc<Self>, idx: usize) {
        self.play_index_with_position(idx, None);
    }

    pub fn play_index_with_position(self: &Rc<Self>, idx: usize, position: Option<f64>) {
        // mpv's vo=libmpv fatally errors on video before the render context
        // exists; queue the start until the GL area has been realized.
        if !self.render_ready.get() {
            *self.pending_play.borrow_mut() = Some((idx, position));
            let _ = self.events.send_blocking(UiEvent::PlaylistChanged);
            return;
        }
        let source = {
            let mut pl = self.playlist.borrow_mut();
            let idx = pl.play_index(idx);
            pl.items().get(idx).map(|i| i.source.clone())
        };
        self.resume_applied.set(false);
        self.pending_position.set(position);
        self.pending_adjustments.set(None);
        if let Some(source) = source {
            self.update_title(&source);
            // Consult per-video resume (docs/04: position + adjustments).
            if position.is_none() && self.rt.settings.get().save_video_position {
                if let Some(entry) = self.rt.resume.take(&source) {
                    if entry.position > 1.0 {
                        self.pending_position.set(Some(entry.position));
                    }
                    self.pending_adjustments.set(Some(entry.adjustments));
                }
            }
            self.player.load(&source);
        }
        let _ = self.events.send_blocking(UiEvent::PlaylistChanged);
        let _ = self.events.send_blocking(UiEvent::TitleChanged);
    }

    /// Called from the GLArea realize handler once the mpv render context
    /// exists; flushes any playback queued before the surface was ready.
    pub fn set_render_ready(self: &Rc<Self>) {
        let first = self.render_ready.replace(true);
        if first {
            return;
        }
        let pending = self.pending_play.borrow_mut().take();
        if let Some((idx, pos)) = pending {
            self.play_index_with_position(idx, pos);
        }
    }

    /// GL surface lost (unrealize): further loads must queue again.
    pub fn set_render_unready(&self) {
        self.render_ready.set(false);
    }

    pub fn next_item(self: &Rc<Self>) {
        if let Some(idx) = self.playlist.borrow_mut().next() {
            self.play_index(idx);
        }
        // No wraparound at the end (docs/06) — Next on the last item is a no-op.
    }

    pub fn previous_item(self: &Rc<Self>) {
        if let Some(idx) = self.playlist.borrow_mut().previous() {
            self.play_index(idx);
        }
        // Robust on error states (docs/06): nothing to go back to → no-op.
    }

    pub fn remove_index(self: &Rc<Self>, idx: usize) {
        let was_current = self.playlist.borrow().current_index() == Some(idx);
        self.playlist.borrow_mut().remove(idx);
        if was_current {
            if self.playlist.borrow().is_empty() {
                self.player.stop();
                self.update_title_none();
            } else {
                let new_idx = self.playlist.borrow().current_index().unwrap_or(0);
                self.play_index(new_idx);
            }
        }
        let _ = self.events.send_blocking(UiEvent::PlaylistChanged);
    }

    pub fn set_shuffle(self: &Rc<Self>, on: bool) {
        self.playlist.borrow_mut().set_shuffle(on);
        let _ = self.events.send_blocking(UiEvent::PlaylistChanged);
    }

    pub fn is_shuffle(&self) -> bool {
        self.playlist.borrow().is_shuffle()
    }

    // -- playback helpers --------------------------------------------------------

    pub fn toggle_play(self: &Rc<Self>) {
        self.player.toggle_play();
    }

    pub fn cycle_ab_loop(self: &Rc<Self>) {
        // Press semantics never observed (docs/03); chosen cycle:
        // set A → set B → clear (recorded assumption).
        let pos = self.player.position().unwrap_or(0.0);
        let (a, b) = self.ab_loop.get();
        let next = match (a, b) {
            (None, _) => (Some(pos), None),
            (Some(_), None) => (a, Some(pos)),
            _ => (None, None),
        };
        self.ab_loop.set(next);
        self.player.set_ab_loop(next.0, next.1);
        self.control_bar_ab_state(next);
    }

    fn control_bar_ab_state(self: &Rc<Self>, state: (Option<f64>, Option<f64>)) {
        let _ = self
            .events
            .send_blocking(UiEvent::Engine(EngineEvent::AbLoopChanged(state.0, state.1)));
    }

    pub fn toggle_fullscreen(self: &Rc<Self>) {
        if self.window.is_fullscreen() {
            self.window.unfullscreen();
        } else {
            self.window.fullscreen();
        }
    }

    pub fn request_add_external_track(self: &Rc<Self>, kind: TrackKind) {
        let core = self.clone();
        let window = self.window.clone();
        let toast_overlay = self.toast_overlay.clone();
        match kind {
            TrackKind::Sub => {
                crate::ui::file_dialogs::choose_subtitle_file(&window, move |file| {
                    if let Some(f) = file.and_then(|f| f.path()) {
                        let source = MediaSource::Path(f.to_string_lossy().into_owned());
                        if let Err(e) = core.player.add_external_track(&source, TrackKind::Sub) {
                            crate::ui::util::toast(&toast_overlay, &format!("Could not add subtitle track: {e}"));
                        }
                    }
                });
            }
            TrackKind::Audio => {
                crate::ui::file_dialogs::choose_audio_file(&window, move |file| {
                    if let Some(f) = file.and_then(|f| f.path()) {
                        let source = MediaSource::Path(f.to_string_lossy().into_owned());
                        if let Err(e) = core.player.add_external_track(&source, TrackKind::Audio) {
                            crate::ui::util::toast(&toast_overlay, &format!("Could not add audio track: {e}"));
                        }
                    }
                });
            }
            TrackKind::Video => {}
        }
    }

    pub fn take_screenshot(self: &Rc<Self>, with_subs: bool) {
        let Some(dir) = self.rt.screenshot_dir() else {
            crate::ui::util::toast(&self.toast_overlay, "Could not access the Pictures folder");
            return;
        };
        let ts = glib::DateTime::now_local().map(|d| d.to_unix()).unwrap_or(0);
        let path = dir.join(format!("frame-{ts}.png"));
        self.player.request_screenshot(crate::engine::ScreenshotRequest { with_subs, path });
    }

    pub fn set_graphics_offload(&self, on: bool) {
        if let Some(offload) = self.offload.borrow().as_ref() {
            offload.set_enabled(if on {
                gtk::GraphicsOffloadEnabled::Enabled
            } else {
                gtk::GraphicsOffloadEnabled::Disabled
            });
        }
    }

    pub fn refresh_from_settings(self: &Rc<Self>) {
        let s = self.rt.settings.get();
        self.player.set_hwdec(!s.software_decoding);
        self.player.set_volume_normalization(s.normalize_volume);
        let color = crate::config::mpv_color(&s.subtitle_color).unwrap_or_else(|| "#ffffffff".into());
        let background = if s.subtitle_background { Some("#a0000000".to_string()) } else { None };
        self.player.apply_subtitle_style(&s.subtitle_font, &color, background.as_deref());
        self.player.set_sub_scale(s.subtitle_scale);
    }

    /// Save per-video resume on close (pref "Save Video Position on Close").
    pub fn save_resume_if_enabled(&self) {
        if !self.rt.settings.get().save_video_position {
            return;
        }
        let Some(source) = self.playlist_current_source() else { return };
        let Some(position) = self.player.position() else { return };
        let adjustments = VideoAdjustments {
            contrast: self.player.get_i64("contrast").unwrap_or(0),
            brightness: self.player.get_i64("brightness").unwrap_or(0),
            gamma: self.player.get_i64("gamma").unwrap_or(0),
            saturation: self.player.get_i64("saturation").unwrap_or(0),
            zoom: self.player.get_double("video-zoom").unwrap_or(0.0),
            sub_delay: self.player.get_double("sub-delay").unwrap_or(0.0),
        };
        let timestamp = glib::DateTime::now_local().map(|d| d.to_unix()).unwrap_or(0);
        self.rt
            .resume
            .put(&source, crate::resume::ResumeEntry { position, adjustments, timestamp });
    }

    pub fn release_inhibit(&self) {
        if self.inhibiting.take() {
            self.rt.inhibit.release();
        }
    }

    pub fn close_window(self: &Rc<Self>) {
        self.save_resume_if_enabled();
        self.release_inhibit();
        self.window.close();
    }

    pub fn save_session_and_close(self: &Rc<Self>) {
        self.rt.save_session_now();
        self.close_window();
    }
}

// ---------------------------------------------------------------------------
// Player window (UI)
// ---------------------------------------------------------------------------

/// One engine-driven render pass: mpv draws into a private offscreen FBO and
/// the result is published on the paintable as a GdkTexture for GSK to
/// composite (see video_paintable). FRAME_DEBUG prints the geometry whenever
/// it changes — field diagnostics for the video surface.
fn render_video_frame(core: &Rc<PlayerWindowCore>) {
    if !core.render_ready.get() {
        return;
    }
    let Some(picture) = core.video_picture.borrow().clone() else {
        return;
    };
    let Some(ctx) = core.gl_context.borrow().clone() else {
        return;
    };
    // GSK renders frames with its own context — make ours current for the
    // engine pass (the switch flushes the outgoing context, ordering our
    // texture writes against GSK's sampling).
    ctx.make_current();
    let (w, h) = core.video_size.get();
    if w < 1 || h < 1 {
        return;
    }
    static DBG_SIG: std::sync::Mutex<(i32, i32)> = std::sync::Mutex::new((0, 0));
    if std::env::var_os("FRAME_DEBUG").is_some() {
        let mut sig = DBG_SIG.lock().unwrap();
        if *sig != (w, h) {
            *sig = (w, h);
            eprintln!("frame: render texture {w}x{h}");
        }
    }
    if let Some(paintable) = picture.paintable().and_downcast::<VideoPaintable>() {
        let t0 = std::time::Instant::now();
        if let Some(tex) = core.player.render_frame(w, h) {
            paintable.set_texture(Some(&tex));
        }
        if std::env::var_os("FRAME_DEBUG").is_some() {
            static STATS: std::sync::Mutex<(Option<std::time::Instant>, u32, u64)> =
                std::sync::Mutex::new((None, 0, 0));
            let mut s = STATS.lock().unwrap();
            if s.0.is_none() {
                s.0 = Some(std::time::Instant::now());
            }
            s.1 += 1;
            s.2 += t0.elapsed().as_micros() as u64;
            let secs = s.0.unwrap().elapsed().as_secs_f64();
            if secs >= 5.0 {
                eprintln!(
                    "frame: publish rate {}/s, avg render+publish {} ms ({}x{})",
                    (s.1 as f64 / secs).round(),
                    s.2 / (s.1.max(1) as u64) / 1000,
                    w,
                    h,
                );
                *s = (Some(std::time::Instant::now()), 0, 0);
            }
        }
    }
}

pub struct PlayerWindow {
    pub core: Rc<PlayerWindowCore>,
    pub control_bar: Rc<ControlBar>,
    pub stats: Rc<Stats>,
    header: adw::HeaderBar,
    header_title: gtk::Label,
    menu_btn: gtk::MenuButton,
    hide_timer: RefCell<Option<glib::SourceId>>,
    pointer_on_controls: Cell<bool>,
}

/// Style the video chrome once per process: the video surface fills the
/// whole window (docs/03), so the header bar and the control bar are
/// transparent overlays with gradient scrims and light foreground — they
/// must stay readable over dark *and* light video frames (the owner's
/// "Bad Apple" report: white frames made theme-coloured UI invisible).
fn install_player_css() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if ONCE.get().is_some() {
        return;
    }
    let css = "
    headerbar.video-scrim-top {
        background: linear-gradient(to bottom, rgba(0,0,0,0.72), rgba(0,0,0,0));
        color: white;
        box-shadow: none;
        outline: none;
    }
    .video-scrim-controls {
        background: linear-gradient(to top, rgba(0,0,0,0.85), rgba(0,0,0,0.45) 55%, rgba(0,0,0,0));
        color: white;
    }
    /* Seek bar, matching the reference chrome: white trough fill that takes
     * the accent colour only while hovered/dragged, translucent white track,
     * plain white handle. */
    scale.video-seek trough {
        background: rgba(255, 255, 255, 0.30);
        border: none;
        border-radius: 999px;
        min-height: 4px;
        box-shadow: none;
    }
    scale.video-seek trough highlight {
        background: white;
        border-radius: 999px;
        min-height: 4px;
    }
    scale.video-seek:hover trough highlight,
    scale.video-seek:active trough highlight {
        background: @accent_color;
    }
    scale.video-seek slider {
        background: white;
        border: none;
        border-radius: 999px;
        min-height: 14px;
        min-width: 14px;
        box-shadow: 0 1px 3px rgba(0, 0, 0, 0.45);
        margin: -5px;
    }
    /* Timestamp chip under the hover thumbnail. */
    .time-chip {
        background: rgba(0, 0, 0, 0.75);
        color: white;
        border-radius: 8px;
        padding: 3px 10px;
        font-weight: bold;
        font-size: 0.85em;
    }
    /* Seek-bar hover preview, rounded like the reference chrome: the clip
     * comes from set_overflow(Hidden) on the picture (GTK4 has no `overflow`
     * CSS property); the light border reads over dark scenes and the drop
     * shadow over bright ones. */
    .seek-thumbnail {
        border-radius: 10px;
        border: 1px solid rgba(255, 255, 255, 0.35);
        box-shadow: 0 2px 10px rgba(0, 0, 0, 0.55);
    }
    /* Track pickers: the list node's default view background paints the rows
     * as a mismatched darker panel inside the popover; melt them into it and
     * tint hover/press from the popover's own colors (theme-agnostic). */
    .track-popover list,
    .track-popover list > row {
        background: none;
    }
    .track-popover list > row:hover {
        background: alpha(@popover_fg_color, 0.08);
    }
    .track-popover list > row:active {
        background: alpha(@popover_fg_color, 0.16);
    }
    ";
    let provider = gtk::CssProvider::new();
    provider.load_from_string(css);
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        ONCE.set(()).ok();
    }
}

/// Create a player window; `sources` non-empty → start playing the first
/// item; empty → an empty player window (docs/06 chooser behavior).
pub fn create_player_window(
    rt: Arc<AppRuntime>,
    sources: Vec<MediaSource>,
) -> Option<Rc<PlayerWindowCore>> {
    let core = match PlayerWindowCore::new(rt) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("frame: failed to initialize the playback engine: {e}");
            return None;
        }
    };
    PlayerWindow::build(core.clone());
    if !sources.is_empty() {
        core.replace_items(sources);
    }
    core.window.present();
    Some(core)
}

impl PlayerWindow {
    fn build(core: Rc<PlayerWindowCore>) -> Rc<Self> {
        // --- video surface ---------------------------------------------------
        // A Picture backed by VideoPaintable: the engine renders mpv into a
        // private offscreen FBO and hands over GdkTextures; GSK composites
        // them like any other texture node. Frame never draws into GTK's own
        // render target (the earlier blit-into-GTK approach produced
        // displaced stale bands on the owner's NVIDIA desktop — GSK's damage
        // tracking cannot be fought from outside).
        let paintable = VideoPaintable::new();
        let picture = gtk::Picture::builder()
            .paintable(&paintable)
            .content_fit(gtk::ContentFit::Fill)
            .build();

        let core_realize = core.clone();
        picture.connect_realize(glib::clone!(#[strong] core_realize, move |_pic| {
            // The window (a Native) owns the GdkSurface the GL context is
            // created from.
            let Some(surface) = core_realize.window.surface() else {
                eprintln!("frame: video surface has no GdkSurface yet");
                return;
            };
            let ctx = match surface.create_gl_context() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("frame: no GL context — video surface disabled ({e}); controls still work");
                    return;
                }
            };
            // make_current realizes the context; a failed realization shows
            // up as render errors later (surfaced in the logs).
            ctx.make_current();
            *core_realize.gl_context.borrow_mut() = Some(ctx);
            let sender = core_realize.render_wakeup.borrow().clone();
            match sender {
                Some(tx) => match core_realize.player.create_render_context(tx) {
                    Ok(()) => {
                        if std::env::var_os("FRAME_DEBUG").is_some() {
                            eprintln!("frame: render context ready");
                        }
                        core_realize.set_render_ready();
                    }
                    Err(e) => eprintln!("frame: mpv render context failed: {e}"),
                },
                None => eprintln!("frame: render wakeup channel missing"),
            }
        }));
        let core_unrealize = core.clone();
        picture.connect_unrealize(glib::clone!(#[strong] core_unrealize, move |_pic| {
            if let Some(ctx) = core_unrealize.gl_context.borrow().as_ref() {
                let _ = ctx.make_current();
            }
            core_unrealize.player.free_render_context();
            core_unrealize.set_render_unready();
        }));

        // Track the video widget's pixel size on the frame clock (a Picture
        // has no resize signal), and do the actual frame publish here — one
        // per display frame at most — when the engine's wakeup pump marked
        // the picture dirty. A change nudges a render pass, so resizes also
        // repaint while paused — nothing else schedules a pass then.
        let core_tick = core.clone();
        picture.add_tick_callback(move |pic, _| {
            let (w, h) = (pic.width() * pic.scale_factor(), pic.height() * pic.scale_factor());
            if std::env::var_os("FRAME_DEBUG").is_some() {
                static TICKS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                let n = TICKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                if n % 300 == 1 {
                    eprintln!(
                        "frame: tick #{n} size {w}x{h} dirty={}",
                        core_tick.frame_dirty.get()
                    );
                }
            }
            let old = core_tick.video_size.replace((w, h));
            if old != (w, h) {
                core_tick.player.nudge_render();
            }
            if core_tick.frame_dirty.get() {
                core_tick.frame_dirty.set(false);
                render_video_frame(&core_tick);
            }
            glib::ControlFlow::Continue
        });

        let offload = gtk::GraphicsOffload::builder()
            .child(&picture)
            .enabled(gtk::GraphicsOffloadEnabled::Disabled)
            .build();
        *core.offload.borrow_mut() = Some(offload.clone());
        core.set_graphics_offload(core.rt.settings.get().graphics_offload);

        // --- overlay: video + controls + bubble + stats -----------------------
        let overlay = gtk::Overlay::builder().child(&offload).build();

        // Seek-bar thumbnail (docs/03 hover preview; only shown when the
        // preference is on and a thumbnail has arrived). Start-aligned so the
        // margins (not a Fill) determine its rectangle. The rounded clip is
        // the widget property, not CSS — GTK4 has no `overflow` CSS property
        // and every packaged build warned "No property named overflow",
        // silently skipping the clip.
        let thumb_picture = gtk::Picture::builder()
            .css_classes(["seek-thumbnail", "osd"])
            .content_fit(gtk::ContentFit::Fill)
            .visible(false)
            .build();
        thumb_picture.set_overflow(gtk::Overflow::Hidden);
        thumb_picture.set_halign(Align::Start);
        thumb_picture.set_valign(Align::End);

        let control_bar = ControlBar::new(&core);
        control_bar.container.set_valign(Align::End);
        control_bar.container.set_halign(Align::Fill);
        overlay.add_overlay(&control_bar.container);
        overlay.add_overlay(&control_bar.hover_bubble);
        overlay.add_overlay(&thumb_picture);

        *core.video_picture.borrow_mut() = Some(picture.clone());
        *core.thumb_picture.borrow_mut() = Some(thumb_picture);
        *core.seek_widget.borrow_mut() = Some(control_bar.seek.clone());
        *core.control_container.borrow_mut() = Some(control_bar.container.clone());

        let stats = Stats::new(&overlay);

        // --- header bar (docs/03: the video surface fills the whole window;
        // the header bar and the control bar are chrome overlaid on it, kept
        // readable on any content by gradient scrims) ---------------------------
        let header = adw::HeaderBar::new();
        header.add_css_class("video-scrim-top");
        header.set_halign(Align::Fill);
        header.set_valign(Align::Start);
        let open_btn = gtk::MenuButton::builder()
            .css_classes(["flat"])
            .tooltip_text("Open")
            .menu_model(&crate::ui::menus::open_menu())
            .build();
        let open_box = gtk::Box::builder().orientation(Orientation::Horizontal).spacing(4).build();
        open_box.append(&gtk::Label::new(Some("Open")));
        open_box.append(&gtk::Image::builder().icon_name("pan-down-symbolic").build());
        open_btn.set_child(Some(&open_box));
        header.pack_start(&open_btn);

        let header_title = gtk::Label::builder()
            .label("Frame")
            .single_line_mode(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .width_chars(4)
            .build();
        header.set_title_widget(Some(&header_title));

        let menu_btn = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(&crate::ui::util::primary_menu())
            .tooltip_text("Main Menu")
            .build();
        header.pack_end(&menu_btn);

        overlay.add_overlay(&header);

        let toast_overlay = core.toast_overlay.clone();
        toast_overlay.set_child(Some(&overlay));
        core.window.set_content(Some(&toast_overlay));
        install_player_css();

        let this = Rc::new(Self {
            core: core.clone(),
            control_bar,
            stats,
            header: header.clone(),
            header_title,
            menu_btn,
            hide_timer: RefCell::new(None),
            pointer_on_controls: Cell::new(false),
        });
        *core.player_window.borrow_mut() = std::rc::Weak::new();
        *core.player_window.borrow_mut() = Rc::downgrade(&this);

    this.spawn_render_wakeup();
    this.spawn_engine_receiver();
    this.install_actions();
    this.connect_controls_motion();
    this.connect_video_clicks();
    this.connect_dnd();
    this.connect_close();
    this.connect_keepalive();
        this.connect_popover_tracking();

        this
    }

    /// Position the hover timestamp chip just above the control bar, centred
    /// on the pointer's position on the seek bar (the thumbnail lifts above
    /// it once it arrives).
    pub fn reposition_time_chip(&self) {
        let core = &self.core;
        let bubble = self.control_bar.hover_bubble.clone();
        if !bubble.is_visible() {
            return;
        }
        let (_, nat, _, _) = bubble.measure(gtk::Orientation::Horizontal, -1);
        core.place_above_controls(
            bubble.upcast_ref::<gtk::Widget>(),
            core.hover_x.get(),
            nat as f64 + 16.0,
            0.0,
        );
    }

    /// Glide the visible thumbnail preview to the current hover position
    /// (same lift as `present_thumbnail`: chip height + gap).
    pub fn reposition_thumb_preview(&self) {
        let core = &self.core;
        let picture_cell = core.thumb_picture.borrow();
        let Some(picture) = picture_cell.as_ref() else {
            return;
        };
        if !picture.is_visible() {
            return;
        }
        let (w, _) = picture.size_request();
        core.place_above_controls(
            picture.upcast_ref::<gtk::Widget>(),
            core.hover_x.get(),
            (w.max(1)) as f64,
            34.0,
        );
    }

    /// Keeps the `PlayerWindow` alive for the window's lifetime and frees the
    /// mpv render context at destroy time (while the GL context still exists).
    fn connect_keepalive(self: &Rc<Self>) {
        let this = self.clone();
        let core = self.core.clone();
        self.core.window.connect_destroy(move |_| {
            if let Some(ctx) = core.gl_context.borrow().as_ref() {
                let _ = ctx.make_current();
            }
            core.player.free_render_context();
            // Releasing this Rc (after the closure returns) drops the engine.
            let _ = &this;
        });
    }

    /// The engine's render-update pump: mpv's update callback nudges this
    /// channel once per decoded frame — 180 times a second for an 180 fps
    /// file — but a publish is expensive (offscreen render + synchronous
    /// readback), so the pump only marks the video picture dirty and asks
    /// for a redraw. The actual render runs from the picture's frame-clock
    /// tick, pacing publishes to the display rate and letting the channel
    /// backlog collapse instead of the renderer falling behind.
    fn spawn_render_wakeup(&self) {
        let (tx, rx) = async_channel::unbounded::<()>();
        *self.core.render_wakeup.borrow_mut() = Some(tx);
        let core = self.core.clone();
        glib::spawn_future_local(async move {
            while let Ok(()) = rx.recv().await {
                // Drain everything queued behind this wakeup; one mark and
                // one redraw request covers them all.
                while rx.try_recv().is_ok() {}
                if std::env::var_os("FRAME_DEBUG").is_some() {
                    static WAKES: std::sync::atomic::AtomicU32 =
                        std::sync::atomic::AtomicU32::new(0);
                    let n = WAKES.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    if n % 900 == 1 {
                        eprintln!("frame: render wakeup #{n}");
                    }
                }
                if !core.frame_dirty.replace(true) {
                    if let Some(pic) = core.video_picture.borrow().as_ref() {
                        pic.queue_draw();
                    }
                }
            }
        });
    }

    fn spawn_engine_receiver(self: &Rc<Self>) {
        let this = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            while let Some(this) = this.upgrade() {
                match this.core.engine_rx.recv().await {
                    Ok(ev) => this.handle_event(ev),
                    Err(_) => break,
                }
            }
        });
    }

    fn handle_event(self: &Rc<Self>, ev: UiEvent) {
        match ev {
            UiEvent::Engine(e) => self.handle_engine_event(e),
            UiEvent::PlaylistChanged => {
                let len = self.core.playlist_len();
                self.control_bar.update_playlist_buttons(len);
                self.control_bar.playlist_popover.update(&self.core);
                // Keep the shuffle toggle in sync with the model.
                if self.control_bar.shuffle_btn.is_active() != self.core.is_shuffle() {
                    self.control_bar.shuffle_btn.set_active(self.core.is_shuffle());
                }
            }
            UiEvent::TitleChanged => {
                self.header_title.set_text(&self.core.title());
            }
        }
    }

    fn handle_engine_event(self: &Rc<Self>, ev: EngineEvent) {
        match ev {
            EngineEvent::MediaLoaded { duration, has_video: _, has_audio: _ } => {
                // The header shows the media title. mpv's `media-title`
                // prefers the file's metadata title but falls back to the
                // raw file name — extension included. The original player
                // shows the bare name in that case (verified black-box
                // against Cine 1.8.8 on Xvfb), so only a real metadata
                // title may override the stripped load-time title.
                if let Some(t) = self.core.player.get_string("media-title") {
                    let t = t.trim().to_string();
                    if !t.is_empty() {
                        let has_metadata_title = self
                            .core
                            .player
                            .get_string("metadata/title")
                            .map(|m| !m.trim().is_empty())
                            .unwrap_or(false);
                        let t = if has_metadata_title {
                            t
                        } else {
                            self.core
                                .playlist_current_source()
                                .map(|s| crate::engine::display_title(&s))
                                .unwrap_or(t)
                        };
                        if t != *self.core.title.borrow() {
                            *self.core.title.borrow_mut() = t.clone();
                            self.core.window.set_title(Some(&t));
                            self.header_title.set_text(&t);
                        }
                    }
                }
                self.core.cached_duration.set(duration.unwrap_or(0.0));
                self.control_bar.update_time(Some(0.0), duration);
                self.apply_pending_resume();
                self.show_controls();
                self.acquire_inhibit();
            }
            EngineEvent::EndOfFile { reason, engine_message } => match reason {
                EndReason::Eof => {
                    // Natural end: clear any resume point, then advance.
                    if let Some(source) = self.core.playlist_current_source() {
                        self.core.rt.resume.clear(&source);
                    }
                    self.core.release_inhibit();
                    self.core.next_item();
                }
                EndReason::Stop | EndReason::Quit | EndReason::Redirect | EndReason::Other(_) => {
                    // Our own loadfile switches — nothing to surface.
                }
                EndReason::Error => {
                    // Generic documented error surface (docs/03, docs/06):
                    // the engine detail is logged, not shown.
                    if let Some(m) = &engine_message {
                        eprintln!("frame: engine error: {m}");
                    }
                    crate::ui::util::toast(
                        &self.core.toast_overlay,
                        "File Error: no audio or video data played",
                    );
                    self.core.release_inhibit();
                    self.show_controls();
                }
            },
            EngineEvent::Idle => {
                self.show_controls();
                self.core.release_inhibit();
                self.core.cached_duration.set(0.0);
                self.control_bar.update_time(Some(0.0), None);
            }
            EngineEvent::PauseChanged(paused) => {
                self.control_bar.update_play_icon(paused);
                if paused {
                    self.show_controls();
                    self.core.release_inhibit();
                } else {
                    self.acquire_inhibit();
                    self.schedule_hide();
                }
            }
            EngineEvent::PositionChanged(pos) => {
                // ~1 report per decoded frame; the duration comes from the
                // engine's cache (the property read takes the mpv core lock,
                // contended with the decode loop at high frame rates).
                let dur = self.core.cached_duration.get();
                let dur = if dur > 0.0 {
                    Some(dur)
                } else {
                    self.core.player.get_double("duration")
                };
                self.control_bar.update_time(pos, dur);
            }
            EngineEvent::DurationChanged(dur) => {
                self.core.cached_duration.set(dur.unwrap_or(0.0));
                let pos = self.core.player.position();
                self.control_bar.update_time(pos, dur);
            }
            EngineEvent::TracksChanged(tracks) => {
                if std::env::var_os("FRAME_DEBUG").is_some() {
                    eprintln!(
                        "frame: ui tracks update: {} entries (audio: {})",
                        tracks.len(),
                        tracks.iter().filter(|t| t.kind == TrackKind::Audio).count()
                    );
                }
                *self.core.tracks.borrow_mut() = tracks.clone();
                let sid = self.core.player.get_i64("sid").unwrap_or(0);
                let aid = self.core.player.get_i64("aid").unwrap_or(0);
                self.control_bar.subs_popover.update(&tracks, sid, &self.core);
                self.control_bar.audio_popover.update(&tracks, aid, &self.core);
                self.control_bar
                    .update_subs_state(tracks.iter().any(|t| t.kind == TrackKind::Sub && t.id == sid));
            }
            EngineEvent::ChaptersChanged(chapters) => {
                *self.core.chapters.borrow_mut() = chapters.clone();
                self.control_bar.chapters_popover.update(&chapters, &self.core);
            }
            EngineEvent::VolumeChanged(_) | EngineEvent::MuteChanged(_) => {
                let v = self.core.player.get_double("volume").unwrap_or(100.0);
                let muted = self.core.player.get_flag("mute").unwrap_or(false);
                self.control_bar.update_volume_icon(v, muted);
                self.control_bar.volume_popover.update(&self.core);
            }
            EngineEvent::SpeedChanged(_) | EngineEvent::LoopFileChanged(_) => {
                self.control_bar.settings_popover.update(&self.core);
            }
            EngineEvent::AbLoopChanged(a, b) => {
                self.core.ab_loop.set((a, b));
                self.control_bar.update_ab_state(a, b);
            }
            EngineEvent::TrackSelectionChanged { sid, secondary_sid: _, aid } => {
                if std::env::var_os("FRAME_DEBUG").is_some() {
                    eprintln!("frame: track selection sid={sid} aid={aid}");
                }
                let tracks = self.core.tracks.borrow().clone();
                self.control_bar.subs_popover.update(&tracks, sid, &self.core);
                self.control_bar.audio_popover.update(&tracks, aid, &self.core);
                self.control_bar
                    .update_subs_state(tracks.iter().any(|t| t.kind == TrackKind::Sub && t.id == sid));
            }
            EngineEvent::SeekableChanged(seekable) => {
                self.control_bar.set_seekable(seekable);
            }
            EngineEvent::ScreenshotSaved(Ok(path)) => {
                crate::ui::util::toast(
                    &self.core.toast_overlay,
                    &format!("Screenshot saved: {}", path.display()),
                );
            }
            EngineEvent::ScreenshotSaved(Err(e)) => {
                crate::ui::util::toast(&self.core.toast_overlay, &format!("Screenshot failed: {e}"));
            }
            EngineEvent::ThumbnailReady { path, pos, width, height, rgba } => {
                self.core.on_thumbnail_ready(path, pos, width, height, &rgba);
            }
            _ => {}
        }
    }

    fn acquire_inhibit(&self) {
        if !self.core.inhibiting.get() {
            self.core.inhibiting.set(true);
            self.core.rt.inhibit.acquire();
        }
    }

    fn apply_pending_resume(&self) {
        if self.core.resume_applied.get() {
            return;
        }
        self.core.resume_applied.set(true);
        if let Some(pos) = self.core.pending_position.take() {
            self.core.player.seek_absolute(pos);
        }
        if let Some(adj) = self.core.pending_adjustments.take() {
            let _ = self.core.player.set_i64("contrast", adj.contrast);
            let _ = self.core.player.set_i64("brightness", adj.brightness);
            let _ = self.core.player.set_i64("gamma", adj.gamma);
            let _ = self.core.player.set_i64("saturation", adj.saturation);
            let _ = self.core.player.set_double("video-zoom", adj.zoom);
            self.core.player.set_sub_delay(adj.sub_delay);
        }
    }

    // -- controls auto-hide (docs/03) ------------------------------------------

    fn connect_controls_motion(self: &Rc<Self>) {
        // Motion fires at the mouse's polling rate — up to ~1000 events/s —
        // and the old handlers cancelled and re-created the hide timer on
        // every one of them. Coalesced like the seek-bar hover work: the
        // event only arms a one-shot frame-clock tick, the actual show +
        // re-schedule runs once per display frame.
        let chrome_tick: Rc<std::cell::RefCell<Option<gtk::TickCallbackId>>> =
            Rc::new(RefCell::new(None));
        let arm_chrome = {
            let window = self.core.window.clone();
            let chrome_tick = chrome_tick.clone();
            let this = self.clone();
            move || {
                if chrome_tick.borrow().is_some() {
                    return;
                }
                let slot = chrome_tick.clone();
                let this_tick = this.clone();
                *chrome_tick.borrow_mut() = Some(window.add_tick_callback(move |_, _| {
                    *slot.borrow_mut() = None;
                    // show_controls() cancels the pending hide; the
                    // schedule re-arms 2 s past this frame — the same
                    // semantics the per-event handlers had.
                    this_tick.show_controls();
                    this_tick.schedule_hide();
                    glib::ControlFlow::Break
                }));
            }
        };
        let motion = gtk::EventControllerMotion::new();
        {
            let arm = arm_chrome.clone();
            motion.connect_motion(move |_, _, _| arm());
        }
        self.core.window.add_controller(motion);

        // Pointer resting on the control bar — or the header chrome — keeps
        // the controls visible.
        let cb_motion = gtk::EventControllerMotion::new();
        {
            let this2 = self.clone();
            let arm = arm_chrome.clone();
            cb_motion.connect_motion(move |_, _, _| {
                this2.pointer_on_controls.set(true);
                arm();
            });
        }
        {
            let this3 = self.clone();
            cb_motion.connect_leave(move |_| {
                this3.pointer_on_controls.set(false);
                this3.schedule_hide();
            });
        }
        self.control_bar.container.add_controller(cb_motion);
        // A controller lives on exactly one widget — the header gets its own
        // with the same handlers.
        let hdr_motion = gtk::EventControllerMotion::new();
        {
            let this4 = self.clone();
            let arm = arm_chrome.clone();
            hdr_motion.connect_motion(move |_, _, _| {
                this4.pointer_on_controls.set(true);
                arm();
            });
        }
        {
            let this5 = self.clone();
            hdr_motion.connect_leave(move |_| {
                this5.pointer_on_controls.set(false);
                this5.schedule_hide();
            });
        }
        self.header.add_controller(hdr_motion);
    }

    fn show_controls(&self) {
        self.header.set_visible(true);
        self.control_bar.container.set_visible(true);
        self.cancel_hide();
    }

    fn cancel_hide(&self) {
        if let Some(t) = self.hide_timer.borrow_mut().take() {
            t.remove();
        }
    }

    fn schedule_hide(self: &Rc<Self>) {
        self.cancel_hide();
        let this = Rc::downgrade(self);
        self.hide_timer.borrow_mut().replace(glib::timeout_add_local(
            std::time::Duration::from_secs(2),
            move || {
                let Some(this) = this.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                let paused = this.core.player.is_paused();
                if paused || this.pointer_on_controls.get() || this.any_popover_open() {
                    // Stay visible while paused, while the pointer rests on
                    // the control bar (docs/03), or while a popover is open.
                    return glib::ControlFlow::Continue;
                }
                this.control_bar.container.set_visible(false);
                // The header bar is overlay chrome just like the bottom
                // controls — it hides with them everywhere (owner request:
                // the upper UI should disappear like the lower one); pointer
                // movement brings both back.
                this.header.set_visible(false);
                // The source ends here; drop the stale id so a later
                // cancel_hide() does not remove an already-freed source.
                *this.hide_timer.borrow_mut() = None;
                glib::ControlFlow::Break
            },
        ));
    }

    fn any_popover_open(&self) -> bool {
        [
            self.control_bar.volume_popover.popover.clone(),
            self.control_bar.subs_popover.popover.clone(),
            self.control_bar.audio_popover.popover.clone(),
            self.control_bar.chapters_popover.popover.clone(),
            self.control_bar.playlist_popover.popover.clone(),
            self.control_bar.settings_popover.popover.clone(),
        ]
        .iter()
        .any(|p| p.is_visible())
    }

    fn connect_popover_tracking(self: &Rc<Self>) {
        // Refresh popovers from live state right before they show.
        let core = self.core.clone();
        let pop = self.control_bar.volume_popover.clone();
        pop.popover.clone().connect_map(move |_| pop.update(&core));
        let core = self.core.clone();
        let subs = self.control_bar.subs_popover.clone();
        subs.popover.clone().connect_map(move |_| {
            let tracks = core.tracks.borrow().clone();
            let sid = core.player.get_i64("sid").unwrap_or(0);
            subs.update(&tracks, sid, &core);
        });
        let core = self.core.clone();
        let audio = self.control_bar.audio_popover.clone();
        audio.popover.clone().connect_map(move |_| {
            let tracks = core.tracks.borrow().clone();
            let aid = core.player.get_i64("aid").unwrap_or(0);
            audio.update(&tracks, aid, &core);
        });
        let core = self.core.clone();
        let chapters = self.control_bar.chapters_popover.clone();
        chapters.popover.clone().connect_map(move |_| {
            let ch = core.chapters.borrow().clone();
            chapters.update(&ch, &core);
        });
        let core = self.core.clone();
        let playlist = self.control_bar.playlist_popover.clone();
        playlist.popover.clone().connect_map(move |_| playlist.update(&core));
        let core = self.core.clone();
        let settings = self.control_bar.settings_popover.clone();
        settings.popover.clone().connect_map(move |_| settings.update(&core));
    }

    // -- video click mapping (docs/03 + docs/04 prefs) --------------------------

    fn connect_video_clicks(self: &Rc<Self>) {
        let this = self.clone();
        let gesture = gtk::GestureClick::new();
        gesture.set_button(0); // any button; dispatch by number
        gesture.connect_pressed(glib::clone!(#[strong] this, move |g, n, x, y| {
            let button = g.current_button();
            if button == 1 && n == 1 {
                match this.core.rt.settings.get().primary_click {
                    PrimaryClickAction::Bypass => {}
                    PrimaryClickAction::PlayPause => this.core.toggle_play(),
                    PrimaryClickAction::FocusPlayPause => {
                        if !this.core.window.is_active() {
                            this.core.window.activate();
                        } else {
                            this.core.toggle_play();
                        }
                    }
                }
            } else if button == 3 && n == 1 {
                match this.core.rt.settings.get().secondary_click {
                    SecondaryClickAction::Bypass => {}
                    SecondaryClickAction::PlayPause => this.core.toggle_play(),
                    SecondaryClickAction::ContextMenu => this.popup_context_menu(x as i32, y as i32),
                }
            }
        }));
        self.core
            .video_picture
            .borrow()
            .as_ref()
            .map(|p| p.add_controller(gesture));
    }

    fn popup_context_menu(&self, x: i32, y: i32) {
        let popover = gtk::PopoverMenu::builder()
            .menu_model(&crate::ui::menus::context_menu())
            .has_arrow(false)
            .build();
        popover.set_parent(&self.core.window);
        popover.set_pointing_to(Some(&gdk::Rectangle::new(x, y, 1, 1)));
        popover.connect_closed(|p| p.unparent());
        popover.popup();
    }

    // -- drag and drop (docs/01 welcome invite; rules recorded as assumptions) --

    fn connect_dnd(self: &Rc<Self>) {
        let core = self.core.clone();
        let file_target = gtk::DropTarget::builder()
            .actions(gdk::DragAction::COPY)
            .formats(&gdk::ContentFormats::for_type(gdk::FileList::static_type()))
            .build();
        file_target.connect_drop(glib::clone!(#[strong] core, move |_, value, _, _| {
            if let Ok(list) = value.get::<gdk::FileList>() {
                handle_dropped_files(&core, list.files());
                return true;
            }
            false
        }));
        self.core.window.add_controller(file_target);

        let core = self.core.clone();
        let text_target = gtk::DropTarget::builder()
            .actions(gdk::DragAction::COPY)
            .formats(&gdk::ContentFormats::for_type(glib::types::Type::STRING))
            .build();
        text_target.connect_drop(glib::clone!(#[strong] core, move |_, value, _, _| {
            if let Ok(text) = value.get::<String>() {
                let trimmed = text.trim();
                if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
                    core.rt
                        .apply_open(vec![MediaSource::Url(trimmed.to_string())], false, Some(core.clone()));
                    return true;
                }
            }
            false
        }));
        self.core.window.add_controller(text_target);
    }

    // -- close ---------------------------------------------------------------------

    fn connect_close(self: &Rc<Self>) {
        let core = self.core.clone();
        self.core.window.connect_close_request(move |_| {
            core.save_resume_if_enabled();
            core.release_inhibit();
            core.player.stop();
            // Allow close; widget destruction releases the closures that keep
            // the Rc<PlayerWindow> alive, dropping the engine afterwards.
            glib::Propagation::Proceed
        });
    }

    fn install_actions(self: &Rc<Self>) {
        use gio::SimpleAction;
        let core = self.core.clone();
        let this = self.clone();

        let open_media = |name: &'static str, add: bool| {
            let action_core = core.clone();
            let a = SimpleAction::new(name, None);
            a.connect_activate(move |_, _| {
                if add {
                    action_core.rt.begin_open_files(&action_core.window, true, Some(action_core.clone()));
                } else {
                    action_core.rt.begin_open_files(&action_core.window, false, Some(action_core.clone()));
                }
            });
            core.window.add_action(&a);
        };
        open_media("open-files", false);
        open_media("add-files", true);

        let open_folder = |name: &'static str, add: bool| {
            let action_core = core.clone();
            let a = SimpleAction::new(name, None);
            a.connect_activate(move |_, _| {
                action_core.rt.begin_open_folder(&action_core.window, add, Some(action_core.clone()));
            });
            core.window.add_action(&a);
        };
        open_folder("open-folder", false);
        open_folder("add-folder", true);

        let a = SimpleAction::new("open-url", None);
        a.connect_activate(glib::clone!(#[strong] core, move |_, _| {
            crate::ui::show_open_url_dialog(&core.window, glib::clone!(#[strong] core, move |url| {
                core.rt
                    .apply_open(vec![MediaSource::Url(url)], false, Some(core.clone()));
            }));
        }));
        core.window.add_action(&a);

        let a = SimpleAction::new("add-url", None);
        a.connect_activate(glib::clone!(#[strong] core, move |_, _| {
            crate::ui::show_add_url_dialog(&core.window, glib::clone!(#[strong] core, move |url| {
                core.add_items(vec![MediaSource::Url(url)]);
            }));
        }));
        core.window.add_action(&a);

        let a = SimpleAction::new("add-subtitle-track", None);
        a.connect_activate(glib::clone!(#[strong] core, move |_, _| {
            core.request_add_external_track(TrackKind::Sub);
        }));
        core.window.add_action(&a);

        let a = SimpleAction::new("add-audio-track", None);
        a.connect_activate(glib::clone!(#[strong] core, move |_, _| {
            core.request_add_external_track(TrackKind::Audio);
        }));
        core.window.add_action(&a);

        let a = SimpleAction::new("watch-history", None);
        a.connect_activate(glib::clone!(#[strong] this, move |_, _| {
            // Anchor under the header's Open button (assumption recorded).
            let popover = crate::ui::history_popover::build_history_popover(
                &this.core.rt.history,
                &this.core.toast_overlay,
                glib::clone!(#[strong] this, move |source| {
                    this.core
                        .rt
                        .apply_open(vec![source], false, Some(this.core.clone()));
                }),
            );
            popover.set_parent(&this.menu_btn);
            popover.popup();
        }));
        core.window.add_action(&a);

        let a = SimpleAction::new("save-session", None);
        a.connect_activate(glib::clone!(#[strong] core, move |_, _| {
            core.rt.save_session_now();
            crate::ui::util::toast(&core.toast_overlay, "Session saved");
        }));
        core.window.add_action(&a);

        let a = SimpleAction::new("save-session-close", None);
        a.connect_activate(glib::clone!(#[strong] core, move |_, _| {
            core.save_session_and_close();
        }));
        core.window.add_action(&a);

        // Player-only actions (context menu + key handling).
        let a = SimpleAction::new("toggle-play", None);
        a.connect_activate(glib::clone!(#[strong] core, move |_, _| core.toggle_play()));
        core.window.add_action(&a);

        let a = SimpleAction::new("seek-back-10", None);
        a.connect_activate(glib::clone!(#[strong] core, move |_, _| core.player.seek_relative(-10.0)));
        core.window.add_action(&a);

        let a = SimpleAction::new("seek-forward-10", None);
        a.connect_activate(glib::clone!(#[strong] core, move |_, _| core.player.seek_relative(10.0)));
        core.window.add_action(&a);

        let a = SimpleAction::new("show-subtitles-menu", None);
        a.connect_activate(glib::clone!(#[strong] this, move |_, _| {
            this.control_bar.subs_btn.popup();
        }));
        core.window.add_action(&a);

        let a = SimpleAction::new("show-audio-menu", None);
        a.connect_activate(glib::clone!(#[strong] this, move |_, _| {
            this.control_bar.audio_btn.popup();
        }));
        core.window.add_action(&a);

        let a = SimpleAction::new("show-playlist", None);
        a.connect_activate(glib::clone!(#[strong] this, move |_, _| {
            this.control_bar.playlist_btn.popup();
        }));
        core.window.add_action(&a);

        let a = SimpleAction::new("toggle-fullscreen", None);
        a.connect_activate(glib::clone!(#[strong] core, move |_, _| core.toggle_fullscreen()));
        core.window.add_action(&a);

        self.install_key_controller();
    }

    // -- keyboard shortcuts (docs/05) --------------------------------------------------

    fn install_key_controller(self: &Rc<Self>) {
        let this = self.clone();
        let controller = gtk::EventControllerKey::new();
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        controller.connect_key_pressed(move |_, keyval, _code, state| {
            this.on_key_pressed(keyval, state)
        });
        self.core.window.add_controller(controller);
    }

    fn focus_is_editable(&self) -> bool {
        let Some(focus) = self.core.window.property::<Option<gtk::Widget>>("focus-widget") else {
            return false;
        };
        // GtkText is the entry widget inside GtkEntry/AdwEntryRow/SpinRow.
        focus.is::<gtk::Text>() || focus.is::<gtk::TextView>()
    }

    fn on_key_pressed(self: &Rc<Self>, keyval: gdk::Key, state: gdk::ModifierType) -> glib::Propagation {
        use gdk::ModifierType as M;

        // Text entries handle their own keys (docs/04 note; S8).
        if self.focus_is_editable() {
            return glib::Propagation::Proceed;
        }
        // Popovers (menus) handle Escape/activation themselves.
        if self.any_popover_open() {
            return glib::Propagation::Proceed;
        }

        let ctrl = state.contains(M::CONTROL_MASK);
        let shift = state.contains(M::SHIFT_MASK);
        let alt = state.contains(M::ALT_MASK);
        let name = keyval.name().map(|s| s.to_string()).unwrap_or_default();
        let name = name.as_str();
        if std::env::var_os("FRAME_DEBUG").is_some() {
            eprintln!(
                "frame: key {name} ctrl={ctrl} shift={shift} alt={alt} popover_open={}",
                self.any_popover_open()
            );
        }
        let core = &self.core;
        let cb = &self.control_bar;

        // Ctrl+? arrives as the "question" keysym, and on most layouts the X
        // server also sets SHIFT (question is shift+slash there). GTK
        // accelerator matching ignores shift for shift-implied keysyms, so
        // match it before the strict modifier blocks.
        if ctrl && !alt && name == "question" {
            return self.activate_app("app.shortcuts");
        }

        // --- General (docs/05 §General) ------------------------------------
        if ctrl && !shift && !alt {
            match name {
                "n" => return self.activate_app("app.new-window"),
                "o" => return self.activate_win("open-files"),
                "i" => return self.activate_win("open-folder"),
                "u" => return self.activate_win("open-url"),
                "s" => {
                    cb.subs_btn.popup();
                    return glib::Propagation::Stop;
                }
                "a" => {
                    cb.audio_btn.popup();
                    return glib::Propagation::Stop;
                }
                "c" => {
                    cb.chapters_btn.popup();
                    return glib::Propagation::Stop;
                }
                "h" => return self.activate_win("watch-history"),
                "question" => return self.activate_app("app.shortcuts"),
                "comma" => return self.activate_app("app.preferences"),
                "w" => {
                    core.close_window();
                    return glib::Propagation::Stop;
                }
                "l" => {
                    core.cycle_ab_loop();
                    return glib::Propagation::Stop;
                }
                "bracketleft" => {
                    core.player.frame_back_step();
                    return glib::Propagation::Stop;
                }
                "bracketright" => {
                    core.player.frame_step();
                    return glib::Propagation::Stop;
                }
                "Left" => return self.chapter_step(-1),
                "Right" => return self.chapter_step(1),
                "minus" => {
                    core.player.adjust_audio_delay(-0.1);
                    return glib::Propagation::Stop;
                }
                "equal" | "plus" => {
                    core.player.adjust_audio_delay(0.1);
                    return glib::Propagation::Stop;
                }
                "z" => {
                    core.player.cycle_secondary_subtitle_track(-1);
                    return glib::Propagation::Stop;
                }
                _ => {}
            }
        }
        if ctrl && shift && !alt {
            match name {
                "O" => return self.activate_win("add-files"),
                "I" => return self.activate_win("add-folder"),
                "U" => return self.activate_win("add-url"),
                "S" => return self.activate_win("save-session"),
                _ => {}
            }
        }
        if !ctrl && shift && !alt {
            match name {
                // Save Session and Close (Shift+Q)
                "Q" => {
                    core.save_session_and_close();
                    return glib::Propagation::Stop;
                }
                "L" => {
                    let on = !core.player.loop_file();
                    core.player.set_loop_file(on);
                    return glib::Propagation::Stop;
                }
                "Z" => {
                    core.player.cycle_subtitle_track(-1);
                    return glib::Propagation::Stop;
                }
                "A" => {
                    core.player.cycle_audio_track(-1);
                    return glib::Propagation::Stop;
                }
                "G" => {
                    core.player.adjust_sub_scale(1.1);
                    return glib::Propagation::Stop;
                }
                "F" => {
                    core.player.adjust_sub_scale(1.0 / 1.1);
                    return glib::Propagation::Stop;
                }
                "S" => {
                    core.take_screenshot(false);
                    return glib::Propagation::Stop;
                }
                "I" => {
                    self.stats.toggle_pinned(&core.clone());
                    return glib::Propagation::Stop;
                }
                _ => {}
            }
        }
        if alt && !ctrl && !shift && name == "z" {
            core.player.cycle_secondary_subtitle_track(1);
            return glib::Propagation::Stop;
        }
        if !ctrl && !shift && !alt {
            match name {
                // Close Window (Q)
                "q" => {
                    core.close_window();
                    return glib::Propagation::Stop;
                }
                // Subtitles
                "c" => {
                    let vis = core.player.get_flag("sub-visibility").unwrap_or(true);
                    core.player.set_sub_visibility(!vis);
                    return glib::Propagation::Stop;
                }
                "z" => {
                    core.player.cycle_subtitle_track(1);
                    return glib::Propagation::Stop;
                }
                "comma" => {
                    core.player.adjust_sub_delay(-0.1);
                    return glib::Propagation::Stop;
                }
                "period" => {
                    core.player.adjust_sub_delay(0.1);
                    return glib::Propagation::Stop;
                }
                "Page_Up" => {
                    core.player.adjust_sub_pos(-5);
                    return glib::Propagation::Stop;
                }
                "Page_Down" => {
                    core.player.adjust_sub_pos(5);
                    return glib::Propagation::Stop;
                }
                // Audio & volume
                "Up" => {
                    core.player.adjust_volume(5.0);
                    return glib::Propagation::Stop;
                }
                "Down" => {
                    core.player.adjust_volume(-5.0);
                    return glib::Propagation::Stop;
                }
                "a" => {
                    core.player.cycle_audio_track(1);
                    return glib::Propagation::Stop;
                }
                "m" => {
                    core.player.toggle_mute();
                    return glib::Propagation::Stop;
                }
                // Navigation
                "j" => {
                    core.player.seek_relative(-10.0);
                    return glib::Propagation::Stop;
                }
                "l" => {
                    core.player.seek_relative(10.0);
                    return glib::Propagation::Stop;
                }
                "Left" => {
                    core.player.seek_relative(-5.0);
                    return glib::Propagation::Stop;
                }
                "Right" => {
                    core.player.seek_relative(5.0);
                    return glib::Propagation::Stop;
                }
                // Display & video
                "F11" | "f" => {
                    core.toggle_fullscreen();
                    return glib::Propagation::Stop;
                }
                "Escape" => {
                    if core.window.is_fullscreen() {
                        core.window.unfullscreen();
                        return glib::Propagation::Stop;
                    }
                }
                "equal" | "plus" => {
                    core.player.adjust_zoom(0.1);
                    return glib::Propagation::Stop;
                }
                "minus" => {
                    core.player.adjust_zoom(-0.1);
                    return glib::Propagation::Stop;
                }
                // Playback
                "space" | "k" | "p" => {
                    core.toggle_play();
                    return glib::Propagation::Stop;
                }
                "bracketleft" => {
                    core.player.set_speed(core.player.speed() / 1.1);
                    return glib::Propagation::Stop;
                }
                "bracketright" => {
                    core.player.set_speed(core.player.speed() * 1.1);
                    return glib::Propagation::Stop;
                }
                "Backspace" => {
                    core.player.set_speed(1.0);
                    return glib::Propagation::Stop;
                }
                // Miscellaneous
                "s" => {
                    core.take_screenshot(true);
                    return glib::Propagation::Stop;
                }
                "i" => {
                    self.stats.toggle_temporary(&core.clone());
                    return glib::Propagation::Stop;
                }
                _ => {}
            }
            // Color keys 1–8 (Display & Video).
            if let Some(digit) = name.chars().next().filter(|c| c.is_ascii_digit()) {
                if name.len() == 1 {
                    let step = |delta: i64| core.player.adjust_color(COLOR_PROPS[(digit as u8 - b'1') as usize / 2], delta);
                    // 1/2 contrast, 3/4 brightness, 5/6 gamma, 7/8 saturation;
                    // odd = decrease, even = increase.
                    let delta = if digit as u8 % 2 == 1 { -5 } else { 5 };
                    step(delta);
                    return glib::Propagation::Stop;
                }
            }
        }
        glib::Propagation::Proceed
    }

    fn activate_win(&self, action: &str) -> glib::Propagation {
        gtk::prelude::WidgetExt::activate_action(&self.core.window, action, None);
        glib::Propagation::Stop
    }

    fn activate_app(&self, action: &str) -> glib::Propagation {
        gtk::prelude::WidgetExt::activate_action(&self.core.window, action, None);
        glib::Propagation::Stop
    }

    fn chapter_step(&self, delta: i64) -> glib::Propagation {
        let count = self.core.chapters.borrow().len() as i64;
        if count == 0 {
            return glib::Propagation::Stop;
        }
        let cur = self.core.player.current_chapter().unwrap_or(0);
        let next = (cur + delta).clamp(0, count - 1);
        self.core.player.set_chapter(next);
        glib::Propagation::Stop
    }
}

const COLOR_PROPS: [&str; 4] = ["contrast", "brightness", "gamma", "saturation"];

/// Drop payload handling (assumption recorded in IMPLEMENTATION_DECISIONS):
/// video/audio files open; folders become playlists; subtitle files add an
/// external subtitle track; text URLs open.
fn handle_dropped_files(core: &Rc<PlayerWindowCore>, files: Vec<gio::File>) {
    let mut media = Vec::new();
    let mut subtitles = Vec::new();
    for file in files {
        if let Some(path) = file.path() {
            let name = path.to_string_lossy().into_owned();
            if path.is_dir() {
                if let Some(items) = crate::rt::scan_folder(&path) {
                    media.extend(items);
                }
            } else if crate::inhibit::is_subtitle_file(&name) {
                subtitles.push(MediaSource::Path(name));
            } else if crate::inhibit::is_video_file(&name) {
                media.push(MediaSource::Path(name));
            }
        }
    }
    for sub in subtitles {
        let _ = core.player.add_external_track(&sub, TrackKind::Sub);
    }
    if !media.is_empty() {
        core.rt.apply_open(media, false, Some(core.clone()));
    }
}
