//! Safe wrapper around the libmpv client + render APIs.
//!
//! One `MpvPlayer` per player window. libmpv API calls are thread-safe except
//! `mpv_wait_event` (drained on a dedicated thread here) and the render
//! functions (called on the main thread from the `GtkGLArea` render handler).

use super::ffi;
use super::MediaSource;
use gtk::prelude::Cast;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_double, c_int, c_void};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};


// ---------------------------------------------------------------------------
// Public data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    Audio,
    Video,
    Sub,
}

impl TrackKind {
    pub fn as_mpv_str(self) -> &'static str {
        match self {
            TrackKind::Audio => "audio",
            TrackKind::Video => "video",
            TrackKind::Sub => "sub",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrackInfo {
    pub id: i64,
    pub kind: TrackKind,
    pub selected: bool,
    pub title: Option<String>,
    pub lang: Option<String>,
    pub codec: Option<String>,
    pub external: bool,
    pub default: bool,
    pub forced: bool,
}

impl TrackInfo {
    /// Human-readable row label, e.g. `Track 2 [eng] (aac) — external`.
    pub fn label(&self) -> String {
        let mut parts = vec![format!("Track {}", self.id)];
        if let Some(lang) = &self.lang {
            parts.push(format!("[{lang}]"));
        }
        if let Some(title) = &self.title {
            parts.push(title.clone());
        }
        if let Some(codec) = &self.codec {
            parts.push(format!("({codec})"));
        }
        let mut s = parts.join(" ");
        if self.external {
            s.push_str(" — external");
        }
        s
    }

    /// Row label for the track popovers, matching the reference chrome:
    /// `Title – lang` (falling back to `Track N` when untitled).
    pub fn picker_label(&self) -> String {
        let name = self
            .title
            .clone()
            .unwrap_or_else(|| format!("Track {}", self.id));
        let mut s = match &self.lang {
            Some(lang) => format!("{name} – {lang}"),
            None => name,
        };
        if self.external {
            s.push_str(" — external");
        }
        s
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChapterInfo {
    pub title: Option<String>,
    pub time: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EndReason {
    Eof,
    Stop,
    Quit,
    Error,
    Redirect,
    Other(c_int),
}

impl EndReason {
    fn from_mpv(reason: c_int) -> Self {
        match reason {
            ffi::MPV_END_FILE_REASON_EOF => EndReason::Eof,
            ffi::MPV_END_FILE_REASON_STOP => EndReason::Stop,
            ffi::MPV_END_FILE_REASON_QUIT => EndReason::Quit,
            ffi::MPV_END_FILE_REASON_ERROR => EndReason::Error,
            ffi::MPV_END_FILE_REASON_REDIRECT => EndReason::Redirect,
            other => EndReason::Other(other),
        }
    }
}

/// Events forwarded from the engine thread to the UI.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    MediaLoaded {
        duration: Option<f64>,
        has_video: bool,
        has_audio: bool,
    },
    EndOfFile {
        reason: EndReason,
        engine_message: Option<String>,
    },
    Idle,
    PauseChanged(bool),
    PositionChanged(Option<f64>),
    DurationChanged(Option<f64>),
    TracksChanged(Vec<TrackInfo>),
    ChaptersChanged(Vec<ChapterInfo>),
    VolumeChanged(f64),
    MuteChanged(bool),
    SpeedChanged(f64),
    AbLoopChanged(Option<f64>, Option<f64>),
    SubDelayChanged(f64),
    AudioDelayChanged(f64),
    SubPosChanged(i64),
    /// A seek-bar thumbnail frame rendered by the hidden thumbnailer.
    ThumbnailReady {
        path: String,
        pos: f64,
        width: i32,
        height: i32,
        rgba: std::sync::Arc<Vec<u8>>,
    },
    SubScaleChanged(f64),
    SubVisibilityChanged(bool),
    ZoomChanged(f64),
    ColorChanged {
        contrast: i64,
        brightness: i64,
        gamma: i64,
        saturation: i64,
    },
    TrackSelectionChanged {
        sid: i64,
        secondary_sid: i64,
        aid: i64,
    },
    LoopFileChanged(bool),
    SeekableChanged(bool),
    /// Screenshot written to disk (path) or failed (message).
    ScreenshotSaved(Result<PathBuf, String>),
    Shutdown,
}

pub struct ScreenshotRequest {
    pub with_subs: bool,
    pub path: PathBuf,
}

struct Shared {
    stop: AtomicBool,
    screenshot: Mutex<Option<ScreenshotRequest>>,
    render_wakeup: Mutex<Option<async_channel::Sender<()>>>,
    /// Last parsed track-list snapshot (A/Shift+A and Z/Shift+Z cycle over
    /// this; kept in Shared so the event thread can update it).
    tracks: Mutex<Vec<TrackInfo>>,
}

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

fn err_string(rc: c_int) -> String {
    unsafe {
        let p = ffi::mpv_error_string(rc);
        if p.is_null() {
            format!("mpv error {rc}")
        } else {
            CStr::from_ptr(p).to_string_lossy().into_owned()
        }
    }
}

// ---------------------------------------------------------------------------
// GL helpers for screenshots (resolved through libglvnd at runtime)
// ---------------------------------------------------------------------------

struct Gl {
    read_pixels: unsafe extern "C" fn(c_int, c_int, c_int, c_int, u32, u32, *mut c_void),
    pixel_storei: unsafe extern "C" fn(u32, c_int),
    get_integerv: unsafe extern "C" fn(u32, *mut c_int),
    bind_framebuffer: unsafe extern "C" fn(u32, u32),
    get_error: unsafe extern "C" fn() -> u32,
    gen_buffers: unsafe extern "C" fn(c_int, *mut u32),
    delete_buffers: unsafe extern "C" fn(c_int, *const u32),
    bind_buffer: unsafe extern "C" fn(u32, u32),
    buffer_data: unsafe extern "C" fn(u32, isize, *const c_void, u32),
    map_buffer_range: unsafe extern "C" fn(u32, isize, isize, u32) -> *mut c_void,
    unmap_buffer: unsafe extern "C" fn(u32) -> u8,
    disable: unsafe extern "C" fn(u32),
    active_texture: unsafe extern "C" fn(u32),
    gen_framebuffers: unsafe extern "C" fn(c_int, *mut u32),
    delete_framebuffers: unsafe extern "C" fn(c_int, *const u32),
    gen_textures: unsafe extern "C" fn(c_int, *mut u32),
    delete_textures: unsafe extern "C" fn(c_int, *const u32),
    bind_texture: unsafe extern "C" fn(u32, u32),
    tex_image_2d: unsafe extern "C" fn(u32, c_int, c_int, c_int, c_int, c_int, u32, u32, *const c_void),
    tex_parameteri: unsafe extern "C" fn(u32, u32, c_int),
    framebuffer_texture_2d: unsafe extern "C" fn(u32, u32, u32, u32, c_int),
    viewport: unsafe extern "C" fn(c_int, c_int, c_int, c_int),
    check_framebuffer: unsafe extern "C" fn(u32) -> u32,
}

const GL_RGBA: u32 = 0x1908;
const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_PACK_ALIGNMENT: u32 = 0x0D05;
const GL_FRAMEBUFFER_BINDING: u32 = 0x8CA6;
const GL_VIEWPORT: u32 = 0x0BA2;
const GL_FRAMEBUFFER: u32 = 0x8D40;
// Pixel-buffer objects for the video publish path's asynchronous readback.
const GL_PIXEL_PACK_BUFFER: u32 = 0x88EB;
const GL_PIXEL_PACK_BUFFER_BINDING: u32 = 0x88ED;
const GL_STREAM_READ: u32 = 0x88E9;
const GL_MAP_READ_BIT: u32 = 0x0001;
const GL_BLEND: u32 = 0x0BE2;
const GL_SCISSOR_TEST: u32 = 0x0C11;
const GL_DEPTH_TEST: u32 = 0x0B71;
const GL_STENCIL_TEST: u32 = 0x0B90;
const GL_CULL_FACE: u32 = 0x0B44;
const GL_TEXTURE0: u32 = 0x84C0;
const GL_TEXTURE_2D: u32 = 0x0DE1;
const GL_RGBA8: u32 = 0x8058;
const GL_NEAREST: u32 = 0x2600;
const GL_LINEAR: u32 = 0x2601;
const GL_TEXTURE_MIN_FILTER: u32 = 0x2801;
const GL_TEXTURE_MAG_FILTER: u32 = 0x2800;
const GL_TEXTURE_WRAP_S: u32 = 0x2802;
const GL_TEXTURE_WRAP_T: u32 = 0x2803;
const GL_CLAMP_TO_EDGE: u32 = 0x812F;
const GL_COLOR_ATTACHMENT0: u32 = 0x8CE0;
const GL_FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;
const GL_READ_FRAMEBUFFER: u32 = 0x8CA8;
const GL_DRAW_FRAMEBUFFER: u32 = 0x8CA9;
const GL_COLOR_BUFFER_BIT: u32 = 0x4000;

fn gl_resolve(name: &std::ffi::CStr) -> *mut c_void {
    // EGL first (GDK 4 contexts are EGL-based on Wayland and modern X11);
    // GLX covers desktop-GL functions EGL's lookup refuses.
    let p = unsafe { ffi::eglGetProcAddress(name.as_ptr()) };
    if !p.is_null() {
        return p;
    }
    unsafe { ffi::glXGetProcAddress(name.as_ptr()) }
}

unsafe extern "C" fn glvnd_proc(ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    let _ = ctx;
    gl_resolve(unsafe { std::ffi::CStr::from_ptr(name) })
}

static GL: OnceLock<Option<Gl>> = OnceLock::new();

fn gl() -> Option<&'static Gl> {
    GL.get_or_init(|| {
        unsafe {
            let rp = gl_resolve(c"glReadPixels");
            let ps = gl_resolve(c"glPixelStorei");
            let gi = gl_resolve(c"glGetIntegerv");
            let bf = gl_resolve(c"glBindFramebuffer");
            let ge = gl_resolve(c"glGetError");
            let gb = gl_resolve(c"glGenBuffers");
            let db = gl_resolve(c"glDeleteBuffers");
            let bb = gl_resolve(c"glBindBuffer");
            let bd = gl_resolve(c"glBufferData");
            let mbr = gl_resolve(c"glMapBufferRange");
            let ub = gl_resolve(c"glUnmapBuffer");
            let di = gl_resolve(c"glDisable");
            let at = gl_resolve(c"glActiveTexture");
            let gf = gl_resolve(c"glGenFramebuffers");
            let df = gl_resolve(c"glDeleteFramebuffers");
            let gt = gl_resolve(c"glGenTextures");
            let dt = gl_resolve(c"glDeleteTextures");
            let bt = gl_resolve(c"glBindTexture");
            let ti = gl_resolve(c"glTexImage2D");
            let tp = gl_resolve(c"glTexParameteri");
            let ft = gl_resolve(c"glFramebufferTexture2D");
            let vp = gl_resolve(c"glViewport");
            let cf = gl_resolve(c"glCheckFramebufferStatus");
            if rp.is_null() || ps.is_null() || gi.is_null() || bf.is_null() || ge.is_null() || gb.is_null()
                || db.is_null() || bb.is_null() || bd.is_null() || mbr.is_null() || ub.is_null() || di.is_null()
                || at.is_null() || gf.is_null() || df.is_null() || gt.is_null() || dt.is_null()
                || bt.is_null() || ti.is_null() || tp.is_null() || ft.is_null()
                || vp.is_null() || cf.is_null()
            {
                return None;
            }
            macro_rules! cast {
                ($p:expr, $t:ty) => {
                    std::mem::transmute::<*mut c_void, $t>($p)
                };
            }
            Some(Gl {
                read_pixels: cast!(rp, unsafe extern "C" fn(c_int, c_int, c_int, c_int, u32, u32, *mut c_void)),
                pixel_storei: cast!(ps, unsafe extern "C" fn(u32, c_int)),
                get_integerv: cast!(gi, unsafe extern "C" fn(u32, *mut c_int)),
                bind_framebuffer: cast!(bf, unsafe extern "C" fn(u32, u32)),
                get_error: cast!(ge, unsafe extern "C" fn() -> u32),
                gen_buffers: cast!(gb, unsafe extern "C" fn(c_int, *mut u32)),
                delete_buffers: cast!(db, unsafe extern "C" fn(c_int, *const u32)),
                bind_buffer: cast!(bb, unsafe extern "C" fn(u32, u32)),
                buffer_data: cast!(bd, unsafe extern "C" fn(u32, isize, *const c_void, u32)),
                map_buffer_range: cast!(mbr, unsafe extern "C" fn(u32, isize, isize, u32) -> *mut c_void),
                unmap_buffer: cast!(ub, unsafe extern "C" fn(u32) -> u8),
                disable: cast!(di, unsafe extern "C" fn(u32)),
                active_texture: cast!(at, unsafe extern "C" fn(u32)),
                gen_framebuffers: cast!(gf, unsafe extern "C" fn(c_int, *mut u32)),
                delete_framebuffers: cast!(df, unsafe extern "C" fn(c_int, *const u32)),
                gen_textures: cast!(gt, unsafe extern "C" fn(c_int, *mut u32)),
                delete_textures: cast!(dt, unsafe extern "C" fn(c_int, *const u32)),
                bind_texture: cast!(bt, unsafe extern "C" fn(u32, u32)),
                tex_image_2d: cast!(ti, unsafe extern "C" fn(u32, c_int, c_int, c_int, c_int, c_int, u32, u32, *const c_void)),
                tex_parameteri: cast!(tp, unsafe extern "C" fn(u32, u32, c_int)),
                framebuffer_texture_2d: cast!(ft, unsafe extern "C" fn(u32, u32, u32, u32, c_int)),
                viewport: cast!(vp, unsafe extern "C" fn(c_int, c_int, c_int, c_int)),
                check_framebuffer: cast!(cf, unsafe extern "C" fn(u32) -> u32),
            })
        }
    })
    .as_ref()
}

/// Reset the GL state GTK's renderer leaves behind; libmpv's render API
/// assumes default state (scissor/blend/stencil would corrupt the frame).
fn reset_gl_state() {
    let Some(glf) = gl() else { return };
    unsafe {
        (glf.disable)(GL_BLEND);
        (glf.disable)(GL_SCISSOR_TEST);
        (glf.disable)(GL_DEPTH_TEST);
        (glf.disable)(GL_STENCIL_TEST);
        (glf.disable)(GL_CULL_FACE);
        (glf.active_texture)(GL_TEXTURE0);
    }
}

/// Reverse the row order of a tightly-packed RGBA buffer in place (GL reads
/// bottom-up; PNG/thumbnail bytes must be top-down).
fn flip_rows_vertical(buf: &mut [u8], stride: usize, rows: usize) {
    let mut tmp = vec![0u8; stride];
    for row in 0..rows / 2 {
        let a = row * stride;
        let b = (rows - 1 - row) * stride;
        tmp.copy_from_slice(&buf[a..a + stride]);
        buf.copy_within(b..b + stride, a);
        buf[b..b + stride].copy_from_slice(&tmp);
    }
}

// ---------------------------------------------------------------------------
// MpvPlayer
// ---------------------------------------------------------------------------

pub struct MpvPlayer {
    ctx: *mut ffi::mpv_handle,
    shared: Arc<Shared>,
    events: async_channel::Sender<EngineEvent>,
    render_ctx: Mutex<Option<*mut ffi::mpv_render_context>>,
    // Private offscreen target (FBO + texture) the video is rendered into
    // before being read back and handed to GSK as a CPU-side MemoryTexture
    // (see render_frame()).
    video_off: Mutex<Option<Offscreen>>,
    // Resize debounce for the video target (see render_frame): deadline at
    // which a widget-size mismatch is allowed to rebuild the offscreen
    // target. Drag-resizes otherwise pay an FBO+PBO realloc storm plus a
    // synchronous seed readback per size event.
    resize_settle: Mutex<Option<std::time::Instant>>,
    // Keep render passes coming until this instant: scrub seeks run while
    // the pipeline is paused, and a paused pipeline raises no render-update
    // flags on its own (same trap as the thumbnailer). Safety deadline —
    // the window closes itself early once the picture is current (see the
    // render-pass tail).
    scrub_until: Mutex<Option<std::time::Instant>>,
    // One landing render pass is still owed (a seek finished decoding).
    seek_grace: std::cell::Cell<bool>,
    // Rate cap for in-flight nudges: a full render+readback+publish per
    // frame-clock tick starves the UI on 180 Hz desktops.
    last_window_nudge: Mutex<std::time::Instant>,
    // Rate cap for mid-decode publishes (aborted seeks surface their
    // intermediate position; see FORCED_PUBLISH_MIN).
    last_forced_publish: Mutex<std::time::Instant>,
    // FRAME_DEBUG: the current render window already logged its first
    // forced pass (one line per window, not per pass).
    render_window_logged: std::sync::atomic::AtomicBool,
    // Hidden second libmpv instance for seek-bar thumbnails; created lazily
    // with the GL context current and driven from render passes.
    thumb: Mutex<Option<ThumbEngine>>,
    // Requests waiting for the next render pass (UI thread → render pass).
    thumb_request: Mutex<Option<(String, f64, i32, i32)>>,
    // Raw Arc<Shared> handed to libmpv as the callback ctx; reconstructed and
    // dropped in free_render_context().
    callback_raw: Mutex<Option<*mut Shared>>,
    event_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

/// A private RGBA8 texture + FBO used as a render target. Used by the hidden
/// thumbnailer, whose textures are read back and never handed to GSK — reuse
/// is safe there. The video path uses one FBO with a fresh texture per frame
/// plus two STREAM_READ pixel buffers for the publish path's ping-pong
/// readback (see `read_pixels_pbo`).
#[derive(Clone, Copy)]
struct Offscreen {
    fbo: u32,
    tex: u32,
    w: i32,
    h: i32,
    /// Publish-path pixel buffers: one receives the async GPU read of the
    /// current frame while the other's previous read is copied out.
    pbos: [u32; 2],
    /// Which PBO receives the next async read (the other is copied out).
    pbo_index: usize,
    /// False until both PBOs hold a real frame (the first publish reads
    /// synchronously and seeds them).
    pbo_seeded: bool,
}

impl Offscreen {
    unsafe fn ensure(glf: &Gl, existing: Option<Self>, w: i32, h: i32) -> Option<Self> {
        let mut o = match existing {
            Some(o) if o.w == w && o.h == h => return Some(o),
            Some(o) => {
                o.destroy(glf);
                None
            }
            None => None,
        };
        if o.is_none() {
            let (mut fbo, mut tex) = (0u32, 0u32);
            (glf.gen_framebuffers)(1, &mut fbo);
            (glf.gen_textures)(1, &mut tex);
            (glf.bind_texture)(GL_TEXTURE_2D, tex);
            (glf.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR as c_int);
            (glf.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as c_int);
            (glf.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE as c_int);
            (glf.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE as c_int);
            (glf.tex_image_2d)(GL_TEXTURE_2D, 0, GL_RGBA8 as c_int, w, h, 0, GL_RGBA, GL_UNSIGNED_BYTE, std::ptr::null());
            (glf.bind_framebuffer)(GL_FRAMEBUFFER, fbo);
            (glf.framebuffer_texture_2d)(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, tex, 0);
            if (glf.check_framebuffer)(GL_FRAMEBUFFER) != GL_FRAMEBUFFER_COMPLETE {
                (glf.bind_texture)(GL_TEXTURE_2D, 0);
                (glf.delete_framebuffers)(1, &fbo);
                (glf.delete_textures)(1, &tex);
                return None;
            }
            (glf.bind_texture)(GL_TEXTURE_2D, 0);
            let mut pbos = [0u32; 2];
            (glf.gen_buffers)(2, pbos.as_mut_ptr());
            for p in pbos {
                (glf.bind_buffer)(GL_PIXEL_PACK_BUFFER, p);
                (glf.buffer_data)(GL_PIXEL_PACK_BUFFER, (w.max(1) * h.max(1) * 4) as isize, std::ptr::null(), GL_STREAM_READ);
            }
            (glf.bind_buffer)(GL_PIXEL_PACK_BUFFER, 0);
            o = Some(Self { fbo, tex, w, h, pbos, pbo_index: 0, pbo_seeded: false });
        }
        o
    }

    unsafe fn destroy(self, glf: &Gl) {
        (glf.delete_buffers)(2, self.pbos.as_ptr());
        (glf.delete_framebuffers)(1, &self.fbo);
        (glf.delete_textures)(1, &self.tex);
    }
}

/// Hidden second libmpv instance producing seek-bar thumbnail frames.
struct ThumbEngine {
    handle: *mut ffi::mpv_handle,
    rctx: *mut ffi::mpv_render_context,
    offscreen: Option<Offscreen>,
    /// File currently loaded into the hidden instance.
    path: String,
    /// Request being worked across render passes (seek issued → frame
    /// rendered → pixels read). Cleared once the pixels are sent.
    pending: Option<ThumbPending>,
}

struct ThumbPending {
    path: String,
    pos: f64,
    w: i32,
    h: i32,
    /// Render passes waited on since the seek was issued.
    tries: u32,
    /// True until the seek command is accepted (the loadfile may still be
    /// running when the request arrives — retry each pass until it lands).
    seeking: bool,
    /// Wall-clock budget: decoding toward the target on heavy files takes
    /// seconds, and pass counts are meaningless at 180 fps.
    started: std::time::Instant,
}

unsafe impl Send for ThumbEngine {}

impl ThumbEngine {
    unsafe fn new() -> Option<Self> {
        let handle = ffi::mpv_create();
        if handle.is_null() {
            return None;
        }
        let opts: &[(&str, &str)] = &[
            ("vo", "libmpv"),
            // Hardware decode for the hidden instance too: keyframe seeks in
            // 180 fps 10-bit files otherwise steal noticeable CPU from the
            // main decoder while scrubbing (the owner's "big lag" report).
            // mpv's render API always produces RGBA in the target FBO either
            // way.
            ("hwdec", "auto"),
            ("ao", "null"),
            ("pause", "yes"),
            ("keep-open", "yes"),
            ("force-window", "no"),
            ("input-default-bindings", "no"),
            ("input-vo-keyboard", "no"),
            ("input-cursor", "no"),
            ("osc", "no"),
            ("osd-level", "0"),
            ("config", "no"),
            ("terminal", "no"),
            ("audio-display", "no"),
            ("sub-auto", "fuzzy"),
            ("volume", "0"),
        ];
        for (k, v) in opts {
            let ck = CString::new(*k).unwrap();
            let cv = CString::new(*v).unwrap();
            // The osc option may not exist in a cplayer-less libmpv; ignore.
            ffi::mpv_set_option_string(handle, ck.as_ptr(), cv.as_ptr());
        }
        if ffi::mpv_initialize(handle) < 0 {
            ffi::mpv_terminate_destroy(handle);
            return None;
        }
        let mut init = ffi::mpv_opengl_init_params {
            get_proc_address: Some(glvnd_proc),
            get_proc_address_ctx: std::ptr::null_mut(),
        };
        let api = CString::new(ffi::MPV_RENDER_API_TYPE_OPENGL).unwrap();
        let mut params = [
            ffi::mpv_render_param {
                param_type: ffi::MPV_RENDER_PARAM_API_TYPE,
                data: api.as_ptr() as *mut c_void,
            },
            ffi::mpv_render_param {
                param_type: ffi::MPV_RENDER_PARAM_OPENGL_INIT_PARAMS,
                data: &mut init as *mut _ as *mut c_void,
            },
            ffi::mpv_render_param {
                param_type: ffi::MPV_RENDER_PARAM_INVALID,
                data: std::ptr::null_mut(),
            },
        ];
        let mut rctx: *mut ffi::mpv_render_context = std::ptr::null_mut();
        if ffi::mpv_render_context_create(&mut rctx, handle, params.as_mut_ptr()) < 0 {
            ffi::mpv_terminate_destroy(handle);
            return None;
        }
        Some(Self {
            handle,
            rctx,
            offscreen: None,
            path: String::new(),
            pending: None,
        })
    }

    unsafe fn destroy(self, glf: &Gl) {
        ffi::mpv_render_context_set_update_callback(self.rctx, None, std::ptr::null_mut());
        ffi::mpv_render_context_free(self.rctx);
        ffi::mpv_terminate_destroy(self.handle);
        if let Some(o) = self.offscreen {
            o.destroy(glf);
        }
    }

    /// Load `path` if needed and issue the seek for a new request.
    unsafe fn start_request(&mut self, path: &str, pos: f64, w: i32, h: i32) {
        if self.path != path {
            self.path = path.to_string();
            let cpath = CString::new(path).unwrap();
            let cmd = [c"loadfile".as_ptr(), cpath.as_ptr(), c"replace".as_ptr(), std::ptr::null()];
            ffi::mpv_command(self.handle, cmd.as_ptr());
            // loadfile unpauses; the thumbnailer must never play (it would
            // crawl through the file and read back arbitrary frames).
            let cpause = CString::new("pause").unwrap();
            let cyes = CString::new("yes").unwrap();
            ffi::mpv_set_property_string(self.handle, cpause.as_ptr(), cyes.as_ptr());
        }
        let seeking = self.seek_pending(pos) < 0;
        self.pending = Some(ThumbPending {
            path: path.to_string(),
            pos,
            w,
            h,
            tries: 0,
            seeking,
            started: std::time::Instant::now(),
        });
    }

    /// Issue the seek for the pending position; < 0 if mpv rejected it (e.g.
    /// the file is still loading). Keyframe-only: a hover preview does not
    /// need frame-exact seeks, and exact seeks on heavy files (10-bit AV1 at
    /// 180 fps) decode from the last keyframe anyway — the visual result is
    /// the same snapped keyframe at a fraction of the cost.
    unsafe fn seek_pending(&mut self, pos: f64) -> c_int {
        let cpos = CString::new(format!("{pos:.3}")).unwrap();
        let cmd = [c"seek".as_ptr(), cpos.as_ptr(), c"absolute+keyframes".as_ptr(), std::ptr::null()];
        ffi::mpv_command(self.handle, cmd.as_ptr())
    }

    /// Read a property as string (works for flags and numbers alike).
    unsafe fn prop_string(&self, name: &str) -> Option<String> {
        let cname = CString::new(name).unwrap();
        let p = ffi::mpv_get_property_string(self.handle, cname.as_ptr());
        if p.is_null() {
            return None;
        }
        let s = CStr::from_ptr(p).to_string_lossy().into_owned();
        ffi::mpv_free(p as *mut c_void);
        Some(s)
    }

    /// True while the hidden instance is still working toward `pos`. The
    /// seek *command* returning success only means the seek was queued, and
    /// `time-pos` reports the target immediately — reading back at that
    /// point captured the seek's start keyframe (a scene from before the
    /// request). Wait until no seek is running, the core is idle (not
    /// decoding) and the reported position has reached the target. With
    /// keyframe-only seeks the landed position is the keyframe at or before
    /// the target (up to a GOP early) — that is the accepted result.
    unsafe fn seek_in_flight(&self, pos: f64) -> bool {
        // Flag properties read back as "yes"/"no" through the string API.
        if matches!(self.prop_string("seeking").as_deref(), Some("true") | Some("yes")) {
            return true;
        }
        if self.prop_string("core-idle").as_deref() == Some("false") {
            return true; // still decoding toward the target
        }
        match self.prop_string("time-pos") {
            None => true,
            // Keyframe-only seeks land at the keyframe at or before the
            // target — accept anything within a generous GOP (the owner's
            // 180 fps file is ~5 s, but x264 defaults give 10 s GOPs at
            // 25 fps; too tight here means an endless re-seek loop that
            // lands on the same keyframe every time). Mid-seek states are
            // already caught by the `seeking`/`core-idle` flags above.
            Some(s) => s
                .parse::<f64>()
                .map(|v| !(v <= pos + 0.1 && v >= pos - 15.0))
                .unwrap_or(true),
        }
    }

    /// Try to finish the pending request during this render pass. Returns
    /// Some((path, pos, w, h, rgba)) once a fresh frame has been rendered.
    /// Gives up after ~600 passes so a failed load/seek cannot wedge the
    /// thumbnailer; the UI simply re-requests on the next hover.
    unsafe fn poll(&mut self, glf: &Gl) -> Option<(String, f64, i32, i32, Vec<u8>)> {
        if self.pending.is_none() {
            return None;
        }
        {
            let (seeking, pos, tries, started) = {
                let p = self.pending.as_ref().unwrap();
                (p.seeking, p.pos, p.tries, p.started)
            };
            let give_up = if seeking {
                // Still loading? Retry until the seek is accepted.
                if self.seek_pending(pos) >= 0 {
                    self.pending.as_mut().unwrap().seeking = false;
                    if std::env::var_os("FRAME_DEBUG").is_some() {
                        eprintln!("frame: thumb seek accepted after {tries} tries");
                    }
                }
                tries + 1 > 600 || started.elapsed() > std::time::Duration::from_secs(15)
            } else {
                false
            };
            if give_up {
                if std::env::var_os("FRAME_DEBUG").is_some() {
                    eprintln!("frame: thumb request gave up after {tries} tries");
                }
                self.pending = None;
                return None;
            }
        }
        if self.pending.as_ref().unwrap().seeking {
            return None;
        }
        // The seek was queued but may still be decoding toward the target —
        // hold the readback until mpv reports the requested position.
        if self.seek_in_flight(self.pending.as_ref().unwrap().pos) {
            let dbg = std::env::var_os("FRAME_DEBUG").is_some();
            let flight_info = if dbg {
                Some((
                    self.prop_string("seeking"),
                    self.prop_string("core-idle"),
                    self.prop_string("time-pos"),
                ))
            } else {
                None
            };
            let give_up = {
                let p = self.pending.as_mut().unwrap();
                p.tries += 1;
                if let Some((seeking, idle, tp)) = flight_info {
                    if p.tries % 10 == 0 {
                        eprintln!(
                            "frame: thumb seek in flight, tries={} seeking={seeking:?} core-idle={idle:?} time-pos={tp:?} (want {:.2})",
                            p.tries, p.pos
                        );
                    }
                }
                p.started.elapsed() > std::time::Duration::from_secs(15)
            };
            if give_up {
                if std::env::var_os("FRAME_DEBUG").is_some() {
                    eprintln!("frame: thumb request gave up (position never reached)");
                }
                self.pending = None;
            }
            return None;
        }
        // The position has been reached: render the vo's current frame
        // unconditionally. A paused instance never raises the render API's
        // FRAME flag (observed: update() returns 0 indefinitely after a
        // paused seek lands), so waiting for it would hang the request.
        let (w, h) = {
            let p = self.pending.as_ref().unwrap();
            (p.w, p.h)
        };
        let Some(ours) = Offscreen::ensure(glf, self.offscreen.take(), w.max(1), h.max(1)) else {
            return None;
        };
        self.offscreen = Some(ours);
        // State to restore after the pass (queried before mpv renders).
        let mut prev_vp = [0 as c_int; 4];
        (glf.get_integerv)(GL_VIEWPORT, prev_vp.as_mut_ptr());
        let mut prev: c_int = 0;
        (glf.get_integerv)(GL_FRAMEBUFFER_BINDING, &mut prev);
        (glf.bind_framebuffer)(GL_FRAMEBUFFER, ours.fbo);
        (glf.viewport)(0, 0, w, h);
        let mut skip_flag: c_int = 0;
        let mut fbo = ffi::mpv_opengl_fbo {
            fbo: ours.fbo as i32,
            w: w.max(1),
            h: h.max(1),
            internal_format: 0,
        };
        let mut flip: c_int = 1; // top-left RGBA out, same as screenshots
        let mut params = [
            ffi::mpv_render_param {
                param_type: ffi::MPV_RENDER_PARAM_OPENGL_FBO,
                data: &mut fbo as *mut _ as *mut c_void,
            },
            ffi::mpv_render_param {
                param_type: ffi::MPV_RENDER_PARAM_FLIP_Y,
                data: &mut flip as *mut _ as *mut c_void,
            },
            ffi::mpv_render_param {
                param_type: ffi::MPV_RENDER_PARAM_SKIP_RENDERING,
                data: &mut skip_flag as *mut _ as *mut c_void,
            },
            ffi::mpv_render_param {
                param_type: ffi::MPV_RENDER_PARAM_INVALID,
                data: std::ptr::null_mut(),
            },
        ];
        ffi::mpv_render_context_render(self.rctx, params.as_mut_ptr());
        ffi::mpv_render_context_report_swap(self.rctx);

        // Read back from the thumbnail FBO, then restore the caller's state.
        // GL reads bottom-up; with FLIP_Y=1 the image's top row lands at the
        // END of the read buffer — flip rows, exactly like read_pixels()
        // does for screenshots (the preview was mirrored without this).
        let bytes = {
            (glf.bind_framebuffer)(GL_FRAMEBUFFER, ours.fbo);
            let mut buf = vec![0u8; (w.max(1) * h.max(1) * 4) as usize];
            (glf.pixel_storei)(GL_PACK_ALIGNMENT, 1);
            (glf.read_pixels)(0, 0, w.max(1), h.max(1), GL_RGBA, GL_UNSIGNED_BYTE, buf.as_mut_ptr() as *mut c_void);
            flip_rows_vertical(&mut buf, (w.max(1) * 4) as usize, h.max(1) as usize);
            buf
        };
        (glf.viewport)(prev_vp[0], prev_vp[1], prev_vp[2], prev_vp[3]);
        (glf.bind_framebuffer)(GL_FRAMEBUFFER, prev as u32);
        let done = self.pending.take().unwrap();
        if std::env::var_os("FRAME_DEBUG").is_some() {
            let prop = |n: &str| {
                let mut v: c_double = -1.0;
                let name = CString::new(n).unwrap();
                if ffi::mpv_get_property(self.handle, name.as_ptr(), ffi::MPV_FORMAT_DOUBLE, &mut v as *mut _ as *mut c_void) < 0 {
                    "?".into()
                } else {
                    format!("{v:.2}")
                }
            };
            eprintln!(
                "frame: thumb readback at time-pos={} (requested {:.2})",
                prop("time-pos"),
                done.pos
            );
        }
        Some((done.path, done.pos, w, h, bytes))
    }
}

unsafe impl Send for MpvPlayer {}
unsafe impl Sync for MpvPlayer {}

struct ThreadHandle(*mut ffi::mpv_handle);
unsafe impl Send for ThreadHandle {}

impl MpvPlayer {
    pub fn new(events: async_channel::Sender<EngineEvent>, hwdec: bool, volume_normalization: bool) -> Result<Arc<Self>, String> {
        unsafe {
            let ctx = ffi::mpv_create();
            if ctx.is_null() {
                return Err("mpv_create failed".into());
            }
            let opts: &[(&str, String)] = &[
                ("vo", "libmpv".into()),
                ("hwdec", if hwdec { "auto" } else { "no" }.into()),
                ("keep-open", "no".into()),
                ("force-window", "no".into()),
                ("input-default-bindings", "no".into()),
                ("input-vo-keyboard", "no".into()),
                ("input-cursor", "no".into()),
                ("osc", "no".into()),
                ("osd-level", "0".into()),
                ("osd-on-seek", "no".into()),
                ("config", "no".into()),
                ("terminal", "no".into()),
                ("audio-display", "no".into()),
                ("sub-auto", "fuzzy".into()),
                ("pause", "yes".into()),
                ("volume", "100".into()),
            ];
            for (k, v) in opts {
                // "osc" only exists when libmpv ships its CLI player / lua
                // (distro builds); a -Dcplayer=false libmpv (Flatpak) rejects
                // it, which is fine — mpv's OSC must stay off wherever it
                // does exist, and Frame draws its own controls.
                let ck = CString::new(*k).unwrap();
                let cv = CString::new(v.as_str()).unwrap();
                let rc = ffi::mpv_set_option_string(ctx, ck.as_ptr(), cv.as_ptr());
                if rc < 0 && !(k == &"osc" && rc == ffi::MPV_ERROR_OPTION_NOT_FOUND) {
                    eprintln!("mpv: option {k}={v} rejected: {}", err_string(rc));
                }
            }
            let clevel = CString::new("warn").unwrap();
            ffi::mpv_request_log_messages(ctx, clevel.as_ptr());
            let rc = ffi::mpv_initialize(ctx);
            if rc < 0 {
                ffi::mpv_terminate_destroy(ctx);
                return Err(format!("mpv_initialize failed: {}", err_string(rc)));
            }
            if volume_normalization {
                // mpv has no "volume-normalization" option; the loudness
                // filter is the mechanism behind the documented preference.
                let af = CString::new("af").unwrap();
                let val = CString::new("loudnorm").unwrap();
                ffi::mpv_set_property_string(ctx, af.as_ptr(), val.as_ptr());
            }

            let player = Arc::new(MpvPlayer {
                ctx,
                shared: Arc::new(Shared {
                    stop: AtomicBool::new(false),
                    screenshot: Mutex::new(None),
                    render_wakeup: Mutex::new(None),
                    tracks: Mutex::new(Vec::new()),
                }),
                events: events.clone(),
                render_ctx: Mutex::new(None),
                video_off: Mutex::new(None),
                resize_settle: Mutex::new(None),
                scrub_until: Mutex::new(None),
                seek_grace: std::cell::Cell::new(false),
                last_window_nudge: Mutex::new(std::time::Instant::now()),
                last_forced_publish: Mutex::new(std::time::Instant::now()),
                render_window_logged: std::sync::atomic::AtomicBool::new(false),
                thumb: Mutex::new(None),
                thumb_request: Mutex::new(None),
                callback_raw: Mutex::new(None),
                event_thread: Mutex::new(None),
            });

            // Observe everything the UI reacts to.
            let observed: &[(&str, c_int)] = &[
                ("time-pos", ffi::MPV_FORMAT_DOUBLE),
                ("duration", ffi::MPV_FORMAT_DOUBLE),
                ("pause", ffi::MPV_FORMAT_FLAG),
                ("volume", ffi::MPV_FORMAT_DOUBLE),
                ("mute", ffi::MPV_FORMAT_FLAG),
                ("speed", ffi::MPV_FORMAT_DOUBLE),
                ("ab-loop-a", ffi::MPV_FORMAT_DOUBLE),
                ("ab-loop-b", ffi::MPV_FORMAT_DOUBLE),
                ("sub-delay", ffi::MPV_FORMAT_DOUBLE),
                ("audio-delay", ffi::MPV_FORMAT_DOUBLE),
                ("sub-pos", ffi::MPV_FORMAT_INT64),
                ("sub-scale", ffi::MPV_FORMAT_DOUBLE),
                ("sub-visibility", ffi::MPV_FORMAT_FLAG),
                ("video-zoom", ffi::MPV_FORMAT_DOUBLE),
                ("contrast", ffi::MPV_FORMAT_INT64),
                ("brightness", ffi::MPV_FORMAT_INT64),
                ("gamma", ffi::MPV_FORMAT_INT64),
                ("saturation", ffi::MPV_FORMAT_INT64),
                ("sid", ffi::MPV_FORMAT_INT64),
                ("secondary-sid", ffi::MPV_FORMAT_INT64),
                ("aid", ffi::MPV_FORMAT_INT64),
                ("loop-file", ffi::MPV_FORMAT_STRING),
                ("seekable", ffi::MPV_FORMAT_FLAG),
                ("idle-active", ffi::MPV_FORMAT_FLAG),
                ("track-list", ffi::MPV_FORMAT_NODE),
                ("chapter-list", ffi::MPV_FORMAT_NODE),
            ];
            for (name, fmt) in observed {
                let cname = CString::new(*name).unwrap();
                ffi::mpv_observe_property(ctx, 0, cname.as_ptr(), *fmt);
            }

            // Event pump thread.
            let tctx = ThreadHandle(ctx);
            let shared = Arc::downgrade(&player.shared);
            let events_tx = events.clone();
            let handle = std::thread::Builder::new()
                .name("mpv-events".into())
                .spawn(move || event_loop(tctx, shared, events_tx))
                .map_err(|e| format!("failed to spawn engine thread: {e}"))?;
            *player.event_thread.lock().unwrap() = Some(handle);

            Ok(player)
        }
    }

    // -- render context (main thread, GL context current) -------------------

    pub fn create_render_context(self: &Arc<Self>, wakeup: async_channel::Sender<()>) -> Result<(), String> {
        unsafe {
            let mut init = ffi::mpv_opengl_init_params {
                get_proc_address: Some(glvnd_proc),
                get_proc_address_ctx: std::ptr::null_mut(),
            };
            let mut advanced: c_int = 1;
            let api = CString::new(ffi::MPV_RENDER_API_TYPE_OPENGL).unwrap();
            let mut params = [
                ffi::mpv_render_param {
                    param_type: ffi::MPV_RENDER_PARAM_API_TYPE,
                    data: api.as_ptr() as *mut c_void,
                },
                ffi::mpv_render_param {
                    param_type: ffi::MPV_RENDER_PARAM_OPENGL_INIT_PARAMS,
                    data: &mut init as *mut _ as *mut c_void,
                },
                ffi::mpv_render_param {
                    param_type: ffi::MPV_RENDER_PARAM_ADVANCED_CONTROL,
                    data: &mut advanced as *mut _ as *mut c_void,
                },
                ffi::mpv_render_param {
                    param_type: ffi::MPV_RENDER_PARAM_INVALID,
                    data: std::ptr::null_mut(),
                },
            ];
            let mut rctx: *mut ffi::mpv_render_context = std::ptr::null_mut();
            let rc = ffi::mpv_render_context_create(&mut rctx, self.ctx, params.as_mut_ptr());
            if rc < 0 {
                return Err(format!("mpv_render_context_create failed: {}", err_string(rc)));
            }
            *self.render_ctx.lock().unwrap() = Some(rctx);

            // Update callback: keep an owned Arc alive for the callback's
            // lifetime; reconstructed and dropped in free_render_context().
            let cb_arc: Arc<Shared> = Arc::clone(&self.shared);
            let raw = Arc::into_raw(cb_arc);
            *self.callback_raw.lock().unwrap() = Some(raw as *mut Shared);
            ffi::mpv_render_context_set_update_callback(rctx, Some(Self::update_cb), raw as *mut c_void);

            *self.shared.render_wakeup.lock().unwrap() = Some(wakeup);
            Ok(())
        }
    }

    /// mpv render update callback: nudges the GTK main loop to redraw.
    /// `user_data` is the `Arc<Shared>` installed by create_render_context().
    unsafe extern "C" fn update_cb(user_data: *mut c_void) {
        let shared = unsafe { &*(user_data as *const Shared) };
        if let Some(tx) = shared.render_wakeup.lock().unwrap().as_ref() {
            let _ = tx.send_blocking(());
        }
    }

    pub fn free_render_context(&self) {
        unsafe {
            let rctx = self.render_ctx.lock().unwrap().take();
            if let Some(rctx) = rctx {
                ffi::mpv_render_context_set_update_callback(rctx, None, std::ptr::null_mut());
                ffi::mpv_render_context_free(rctx);
                if let Some(raw) = self.callback_raw.lock().unwrap().take() {
                    drop(Arc::from_raw(raw));
                }
            }
            if let Some(glf) = gl() {
                if let Some(off) = self.video_off.lock().unwrap().take() {
                    off.destroy(glf);
                }
                if let Some(t) = self.thumb.lock().unwrap().take() {
                    t.destroy(glf);
                }
            }
        }
    }

    /// Render one frame and hand it to GSK as a `GdkTexture`.
    ///
    /// mpv always renders into a private offscreen RGBA8 FBO sized exactly to
    /// the video widget's pixel size. The frame is then **read back to the
    /// CPU** and published as a `gdk::MemoryTexture`; the caller puts it on a
    /// Paintable and GSK composites it like any other image. Frame does **not**
    /// draw into GTK's render target at all, and no GL texture id is ever
    /// placed in GSK's hands: two earlier revisions published the offscreen
    /// texture itself (first blitting into GTK's framebuffer, then wrapping it
    /// in a `GdkGLTexture`), and both corrupted on the owner's NVIDIA desktop
    /// — GSK caches texture ids beyond the `GdkGLTexture` lifetime (tile
    /// caching / damage tracking), so re-rendering into a reused name races
    /// its sampling (displaced stale bands), and recycling deleted names via
    /// `glGenTextures` samples outright garbage. A CPU-side texture has no
    /// lifetime games: correctness by construction on every GL stack. The
    /// readback runs as a double-buffered PBO ping-pong (`read_pixels_pbo`):
    /// no CPU stall per publish, one constant frame of latency. Publishes
    /// themselves are paced to the display by the UI's frame-clock tick (the
    /// engine's wakeup only marks the picture dirty) — mpv's update callback
    /// fires per decoded frame, and an 180 fps file on a 180 Hz panel would
    /// otherwise drive a publish per decoded frame.
    ///
    /// Returns `None` when there was nothing to publish. Called on the main
    /// thread with the engine GL context current.
    /// How long the widget size must stay unchanged before the offscreen
    /// video target is rebuilt at the new size.
    const RESIZE_SETTLE: std::time::Duration = std::time::Duration::from_millis(150);

    /// Minimum spacing between render-window nudges while a seek decodes —
    /// completion is noticed within this bound without a per-tick churn.
    const WINDOW_NUDGE_MIN: std::time::Duration = std::time::Duration::from_millis(25);

    /// Minimum spacing between mid-decode publishes while a seek is in
    /// flight: aborted-and-reissued seeks leave the decoder partway, and
    /// rendering mid-seek shows that intermediate position (how mpv's own
    /// OSC drag "follows" the bar) — but a full readback+upload per tick
    /// would starve the UI on 180 Hz desktops. Keyframe drag landings are
    /// one-shot (a landing pass publishes, then the window clears), so the
    /// cap only spaces the bursts; 50 ms keeps the drag scan smooth.
    const FORCED_PUBLISH_MIN: std::time::Duration = std::time::Duration::from_millis(50);

    pub fn render_frame(&self, width: i32, height: i32) -> Option<gtk::gdk::Texture> {
        let rctx = *self.render_ctx.lock().unwrap();
        let Some(rctx) = rctx else { return None };
        let Some(glf) = gl() else { return None };
        let w = width.max(1);
        let h = height.max(1);
        reset_gl_state();
        let mut published: Option<gtk::gdk::Texture> = None;
        unsafe {
            let flags = ffi::mpv_render_context_update(rctx);
            let has_frame = flags & ffi::MPV_RENDER_UPDATE_FRAME != 0;
            let screenshot = self.shared.screenshot.lock().unwrap().take();

            // Ambient state to restore around the hidden thumbnailer pass.
            let mut vp = [0 as c_int; 4];
            (glf.get_integerv)(GL_VIEWPORT, vp.as_mut_ptr());
            let mut bound: c_int = 0;
            (glf.get_integerv)(GL_FRAMEBUFFER_BINDING, &mut bound);

            // The persistent video target: (re)created on size changes — but
            // debounced. Drag-resizing otherwise pays an FBO+PBO realloc
            // storm plus a synchronous seed readback on every size event;
            // while the widget size keeps changing we keep rendering at the
            // existing target size (GSK scales the published texture) and
            // rebuild once it has settled briefly.
            let mut off = self.video_off.lock().unwrap().as_ref().copied();
            let (tw, th) = match off {
                Some(o) if o.w != w || o.h != h => {
                    let now = std::time::Instant::now();
                    let mut settle = self.resize_settle.lock().unwrap();
                    // Hold (render at the existing target size) while size
                    // events keep arriving within the settle window; rebuild
                    // when it elapses. A drag gesture then pays two rebuilds
                    // (first + final) instead of one per size event, and a
                    // one-off resize (maximize) is still immediate.
                    let rebuild = match *settle {
                        Some(t) => now >= t,
                        None => true,
                    };
                    *settle = Some(now + Self::RESIZE_SETTLE);
                    if rebuild { (w, h) } else { (o.w, o.h) }
                }
                _ => {
                    *self.resize_settle.lock().unwrap() = None;
                    (w, h)
                }
            };
            let mut have_target = false;
            // A paused pipeline raises no FRAME flag — during an open render
            // window (scrub seeks, keyboard seeks/track/subtitle/color
            // changes while paused) render unconditionally, or the nudged
            // passes would all skip the one thing they exist for.
            let forced = self.scrub_active();
            // Mid-decode passes DO have something to show with abort-and-
            // reissue scrubbing: the vo holds the last decoded position of
            // the aborted seek — the drag's intermediate frame. Publish
            // those rate-capped; the landing pass publishes uncapped.
            let seeking_now = self.is_seeking();
            let forced_publish = forced
                && (!seeking_now
                    || self
                        .last_forced_publish
                        .lock()
                        .unwrap()
                        .elapsed()
                        >= Self::FORCED_PUBLISH_MIN);
            if seeking_now && forced_publish {
                *self.last_forced_publish.lock().unwrap() = std::time::Instant::now();
            }
            if has_frame || screenshot.is_some() || forced_publish {
                if std::env::var_os("FRAME_DEBUG").is_some()
                    && forced_publish
                    && !has_frame
                {
                    if !self
                        .render_window_logged
                        .swap(true, std::sync::atomic::Ordering::Relaxed)
                    {
                        eprintln!("frame: forced pass (no FRAME flag) — rendering unconditionally");
                    }
                }
                if let Some(o) = Offscreen::ensure(glf, off.take(), tw, th) {
                    off = Some(o);
                    (glf.bind_framebuffer)(GL_FRAMEBUFFER, o.fbo);
                    (glf.viewport)(0, 0, tw, th);
                    self.render_once(rctx, tw, th, o.fbo as i32, false);
                    ffi::mpv_render_context_report_swap(rctx);
                    have_target = true;
                } else {
                    // Target unavailable (no GL): consume the update.
                    self.render_once(rctx, tw, th, 0, true);
                }
            } else {
                self.render_once(rctx, tw, th, 0, true);
            }

            // Screenshot handling: grab from our own FBO (always the full
            // area-sized frame, independent of anything GTK composites).
            if let Some(req) = screenshot {
                if have_target {
                    let fbo = off.unwrap().fbo as i32;
                    if !req.with_subs {
                        self.set_string("sub-visibility", "no");
                        self.render_once(rctx, tw, th, fbo, false);
                    }
                    let pixels = self.read_pixels(tw, th, fbo);
                    if !req.with_subs {
                        self.set_string("sub-visibility", "yes");
                        self.render_once(rctx, tw, th, fbo, false);
                    }
                    let result = match pixels {
                        Some(rgba) => save_png(&req.path, tw, th, &rgba).map(|_| req.path.clone()).map_err(|e| e.to_string()),
                        None => Err("no GL framebuffer available".into()),
                    };
                    self.send(EngineEvent::ScreenshotSaved(result));
                } else {
                    *self.shared.screenshot.lock().unwrap() = Some(req);
                }
            }

            // Thumbnail request handling (separate hidden mpv context). It
            // shares this GL context — save and restore everything it touches
            // so nothing leaks into the next pass.
            self.process_thumbnail(glf);
            (glf.viewport)(vp[0], vp[1], vp[2], vp[3]);
            (glf.bind_framebuffer)(GL_FRAMEBUFFER, bound as u32);

            // Publish: read the frame back and wrap it as a CPU texture.
            if have_target {
                let mut o = off.unwrap();
                let rgba = self.read_pixels_pbo(tw, th, o.fbo, &mut o);
                off = Some(o);
                if let Some(rgba) = rgba {
                    published = Some(
                        gtk::gdk::MemoryTexture::new(
                            tw,
                            th,
                            gtk::gdk::MemoryFormat::R8g8b8a8Premultiplied,
                            &gtk::glib::Bytes::from_owned(rgba),
                            (tw * 4) as usize,
                        )
                        .upcast(),
                    );
                    if forced && std::env::var_os("FRAME_DEBUG").is_some() {
                        // One line per scrub-window publish: the log itself
                        // then measures the drag's visible update cadence.
                        eprintln!(
                            "frame: scrub publish pos={:?} seeking={}",
                            self.get_double("time-pos"),
                            self.is_seeking()
                        );
                    }
                }
            }
            // Persist the PBO ping-pong state (index/seed) for the next pass.
            *self.video_off.lock().unwrap() = off;
        }
        // Keep passes coming while the hidden thumbnailer works — when
        // the video is paused nothing else schedules draws, and the
        // thumbnailer needs several (load, seek, render).
        if self.thumb_busy() {
            self.nudge_render();
        }
        // Seeks and picture changes that happen while the pipeline is
        // paused raise no render-update flags of their own. The render
        // window keeps passes coming ONLY while they can matter: while a
        // seek is decoding (rate-capped) plus one landing pass afterwards.
        // An always-on window would self-sustain at the frame-clock rate —
        // a render+readback+publish of a full-frame texture per tick, which
        // on a 180 Hz desktop starves the UI thread (the owner's "progress
        // bar still lags while moving it").
        if self.scrub_active() {
            if self.is_seeking() {
                if self
                    .last_window_nudge
                    .lock()
                    .unwrap()
                    .elapsed()
                    >= Self::WINDOW_NUDGE_MIN
                {
                    *self.last_window_nudge.lock().unwrap() = std::time::Instant::now();
                    self.nudge_render();
                }
            } else if self.seek_grace.replace(false) {
                // The seek finished decoding — this pass (or the next) is
                // the one that publishes the landed frame, then stop.
                *self.last_window_nudge.lock().unwrap() = std::time::Instant::now();
                self.nudge_render();
            } else {
                // Picture is current; close the window instead of churning.
                *self.scrub_until.lock().unwrap() = None;
            }
        }
        published
    }

    fn scrub_active(&self) -> bool {
        self.scrub_until
            .lock()
            .unwrap()
            .map_or(false, |t| std::time::Instant::now() < t)
    }

    /// Ask the engine to keep render passes coming for a short window so a
    /// change made while the pipeline is paused actually shows: scrub seeks,
    /// keyboard seeks, frame steps, chapter jumps, subtitle/track/zoom/color
    /// changes — a paused pipeline raises no render-update flags on its own.
    /// While playing, passes happen naturally and the request is a no-op.
    pub fn request_render_passes(&self) {
        if self.get_flag("pause").unwrap_or(false) {
            if std::env::var_os("FRAME_DEBUG").is_some() {
                eprintln!("frame: render passes requested (paused)");
            }
            *self.scrub_until.lock().unwrap() =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(900));
            self.render_window_logged
                .store(false, std::sync::atomic::Ordering::Relaxed);
            self.seek_grace.set(true);
            *self.last_window_nudge.lock().unwrap() = std::time::Instant::now();
            // Bootstrap: nothing else wakes a paused pipeline — without this
            // first nudge the window would sit there with no pass ever
            // entering it (wakeup pump idle, tick never dirty).
            self.nudge_render();
        }
    }

    /// True while mpv is working on a seek (the property reads "yes"/"no").
    pub fn is_seeking(&self) -> bool {
        matches!(self.get_string("seeking").as_deref(), Some("yes") | Some("true"))
    }

    /// Queue a seek-bar thumbnail: the hidden thumbnailer loads `path` (if
    /// it is not already loaded), seeks to `pos` and renders a `w×h` frame
    /// that arrives as `EngineEvent::ThumbnailReady`. Called on the UI
    /// thread; the GL work happens in the next render pass.
    pub fn request_thumbnail(&self, path: String, pos: f64, w: i32, h: i32) {
        *self.thumb_request.lock().unwrap() = Some((path, pos, w.max(1), h.max(1)));
    }

    /// Drive the thumbnailer from the current render pass (GL context
    /// current): pick up queued requests, seek the hidden instance and read
    /// back finished frames.
    fn process_thumbnail(&self, glf: &Gl) {
        let debug = std::env::var_os("FRAME_DEBUG").is_some();
        let mut engine = self.thumb.lock().unwrap();
        if engine.is_none() {
            *engine = unsafe { ThumbEngine::new() };
            if engine.is_none() {
                if debug {
                    eprintln!("frame: thumbnail engine failed to start");
                }
                return;
            }
            if debug {
                eprintln!("frame: thumbnail engine started");
            }
        }
        let engine = engine.as_mut().unwrap();
        if engine.pending.is_none() {
            if let Some((path, pos, w, h)) = self.thumb_request.lock().unwrap().take() {
                if debug {
                    eprintln!("frame: thumbnail request pos={pos:.1} {w}x{h}");
                }
                unsafe { engine.start_request(&path, pos, w, h) };
            }
        }
        if let Some((path, pos, w, h, rgba)) = unsafe { engine.poll(glf) } {
            self.send(EngineEvent::ThumbnailReady {
                path,
                pos,
                width: w,
                height: h,
                rgba: std::sync::Arc::new(rgba),
            });
        }
    }

    #[allow(unsafe_op_in_unsafe_fn)]
    unsafe fn render_once(&self, rctx: *mut ffi::mpv_render_context, w: i32, h: i32, fbo_id: i32, skip: bool) {
        let mut skip_flag: c_int = skip as c_int;
        let mut fbo = ffi::mpv_opengl_fbo {
            fbo: fbo_id,
            w: w.max(1),
            h: h.max(1),
            internal_format: 0,
        };
        let mut flip: c_int = 1;
        let mut params = [
            ffi::mpv_render_param {
                param_type: ffi::MPV_RENDER_PARAM_OPENGL_FBO,
                data: &mut fbo as *mut _ as *mut c_void,
            },
            ffi::mpv_render_param {
                param_type: ffi::MPV_RENDER_PARAM_FLIP_Y,
                data: &mut flip as *mut _ as *mut c_void,
            },
            ffi::mpv_render_param {
                param_type: ffi::MPV_RENDER_PARAM_SKIP_RENDERING,
                data: &mut skip_flag as *mut _ as *mut c_void,
            },
            ffi::mpv_render_param {
                param_type: ffi::MPV_RENDER_PARAM_INVALID,
                data: std::ptr::null_mut(),
            },
        ];
        ffi::mpv_render_context_render(rctx, params.as_mut_ptr());
    }

    #[allow(unsafe_op_in_unsafe_fn)]
    unsafe fn read_pixels(&self, w: i32, h: i32, fbo: i32) -> Option<Vec<u8>> {
        let glf = gl()?;
        // Read from the render target GTK composites, not whatever is bound.
        let mut prev: c_int = 0;
        (glf.get_integerv)(GL_FRAMEBUFFER_BINDING, &mut prev);
        (glf.bind_framebuffer)(GL_FRAMEBUFFER, fbo as u32);
        (glf.pixel_storei)(GL_PACK_ALIGNMENT, 1);
        let mut buf = vec![0u8; (w.max(1) * h.max(1) * 4) as usize];
        (glf.read_pixels)(0, 0, w.max(1), h.max(1), GL_RGBA, GL_UNSIGNED_BYTE, buf.as_mut_ptr() as *mut c_void);
        (glf.bind_framebuffer)(GL_FRAMEBUFFER, prev as u32);
        // GL reads bottom-up; flip rows for the PNG.
        flip_rows_vertical(&mut buf, (w.max(1) * 4) as usize, h.max(1) as usize);
        Some(buf)
    }

    /// Publish-path readback with double-buffered pixel-buffer objects:
    /// issue the async GPU→PBO read of the current frame, then copy out the
    /// *previous* frame's buffer, whose read has long completed. A plain
    /// `glReadPixels` stalls the CPU until the GPU finishes everything
    /// before it — one stall per publish, which eats the frame budget at
    /// high display refresh rates (the owner's panel is 180 Hz, matching
    /// this file's 180 fps). The ping-pong costs one constant video frame
    /// of display latency and needs no fences: GL executes in order, so by
    /// the time the older read is copied out, every command issued before
    /// it has completed. The first call after (re)creation reads
    /// synchronously and seeds both PBOs so a copy-out never sees
    /// uninitialized storage; screenshots and the thumbnailer keep the
    /// synchronous `read_pixels` (rare passes that must see the current
    /// frame).
    #[allow(unsafe_op_in_unsafe_fn)]
    unsafe fn read_pixels_pbo(
        &self,
        w: i32,
        h: i32,
        fbo: u32,
        off: &mut Offscreen,
    ) -> Option<Vec<u8>> {
        let glf = gl()?;
        let len = (w.max(1) * h.max(1) * 4) as isize;
        unsafe {
            let mut prev: c_int = 0;
            (glf.get_integerv)(GL_PIXEL_PACK_BUFFER_BINDING, &mut prev);
            (glf.bind_framebuffer)(GL_FRAMEBUFFER, fbo);
            (glf.pixel_storei)(GL_PACK_ALIGNMENT, 1);
            if !off.pbo_seeded {
                for p in &off.pbos {
                    (glf.bind_buffer)(GL_PIXEL_PACK_BUFFER, *p);
                    (glf.read_pixels)(0, 0, w.max(1), h.max(1), GL_RGBA, GL_UNSIGNED_BYTE, std::ptr::null_mut());
                }
                (glf.bind_buffer)(GL_PIXEL_PACK_BUFFER, 0);
                off.pbo_seeded = true;
                // Same frame the seeds captured, read synchronously once.
                let buf = self.read_pixels(w, h, fbo as i32);
                (glf.bind_framebuffer)(GL_FRAMEBUFFER, prev as u32);
                return buf;
            }
            let idx = off.pbo_index;
            // Async: queue the current frame's read into PBO[idx].
            (glf.bind_buffer)(GL_PIXEL_PACK_BUFFER, off.pbos[idx]);
            (glf.read_pixels)(0, 0, w.max(1), h.max(1), GL_RGBA, GL_UNSIGNED_BYTE, std::ptr::null_mut());
            (glf.bind_buffer)(GL_PIXEL_PACK_BUFFER, 0);
            // Copy out PBO[1-idx] — the previous frame, long completed.
            // MapBufferRange, not GetBufferSubData: the latter is desktop-GL
            // only and errors on GLES contexts.
            let mut buf = vec![0u8; len as usize];
            (glf.bind_buffer)(GL_PIXEL_PACK_BUFFER, off.pbos[1 - idx]);
            let ptr = (glf.map_buffer_range)(GL_PIXEL_PACK_BUFFER, 0, len, GL_MAP_READ_BIT);
            let ok = if ptr.is_null() {
                false
            } else {
                std::ptr::copy_nonoverlapping(ptr as *const u8, buf.as_mut_ptr(), len as usize);
                (glf.unmap_buffer)(GL_PIXEL_PACK_BUFFER) != 0
            };
            (glf.bind_buffer)(GL_PIXEL_PACK_BUFFER, 0);
            (glf.bind_framebuffer)(GL_FRAMEBUFFER, prev as u32);
            if !ok {
                return None;
            }
            off.pbo_index = 1 - idx;
            flip_rows_vertical(&mut buf, (w.max(1) * 4) as usize, h.max(1) as usize);
            Some(buf)
        }
    }

    pub fn request_screenshot(&self, req: ScreenshotRequest) {
        *self.shared.screenshot.lock().unwrap() = Some(req);
        // Force a redraw so the render handler runs even while paused/idle.
        self.nudge_render();
    }

    /// Queue one more render pass (used while paused/idle, when nothing else
    /// draws — e.g. so the hidden thumbnailer can make progress, or to
    /// re-render after a video-surface resize). Note: async_channel's
    /// future-returning send drops the item if never polled — use
    /// send_blocking.
    pub fn nudge_render(&self) {
        if let Some(tx) = self.shared.render_wakeup.lock().unwrap().as_ref() {
            let _ = tx.send_blocking(());
        }
    }

    /// True while thumbnail work is outstanding (a request waiting or a
    /// pending seek/render in the hidden instance).
    fn thumb_busy(&self) -> bool {
        if self.thumb_request.lock().unwrap().is_some() {
            return true;
        }
        self.thumb
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|e| e.pending.is_some())
    }

    // -- low-level property/command helpers ---------------------------------

    fn command(&self, args: &[&str]) -> Result<(), String> {
        let cargs: Vec<CString> = args
            .iter()
            .map(|s| CString::new(*s).map_err(|_| "NUL in argument".to_string()))
            .collect::<Result<_, _>>()?;
        let mut ptrs: Vec<*const c_char> = cargs.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        unsafe {
            let rc = ffi::mpv_command(self.ctx, ptrs.as_ptr());
            if rc < 0 {
                Err(err_string(rc))
            } else {
                Ok(())
            }
        }
    }

    pub fn set_string(&self, name: &str, val: &str) -> Result<(), String> {
        let cn = CString::new(name).unwrap();
        let cv = CString::new(val).map_err(|_| "NUL".to_string())?;
        unsafe {
            let rc = ffi::mpv_set_property_string(self.ctx, cn.as_ptr(), cv.as_ptr());
            if rc < 0 {
                Err(err_string(rc))
            } else {
                Ok(())
            }
        }
    }

    pub fn set_double(&self, name: &str, v: f64) -> Result<(), String> {
        let cn = CString::new(name).unwrap();
        unsafe {
            let rc = ffi::mpv_set_property(self.ctx, cn.as_ptr(), ffi::MPV_FORMAT_DOUBLE, &v as *const _ as *mut c_void);
            if rc < 0 {
                Err(err_string(rc))
            } else {
                Ok(())
            }
        }
    }

    pub fn set_i64(&self, name: &str, v: i64) -> Result<(), String> {
        let cn = CString::new(name).unwrap();
        unsafe {
            let rc = ffi::mpv_set_property(self.ctx, cn.as_ptr(), ffi::MPV_FORMAT_INT64, &v as *const _ as *mut c_void);
            if rc < 0 {
                Err(err_string(rc))
            } else {
                Ok(())
            }
        }
    }

    fn set_flag(&self, name: &str, v: bool) -> Result<(), String> {
        let cn = CString::new(name).unwrap();
        let vi: c_int = v as c_int; // MPV_FORMAT_FLAG is an int pointer, not bool
        unsafe {
            let rc = ffi::mpv_set_property(self.ctx, cn.as_ptr(), ffi::MPV_FORMAT_FLAG, &vi as *const _ as *mut c_void);
            if rc < 0 {
                Err(err_string(rc))
            } else {
                Ok(())
            }
        }
    }

    pub fn get_string(&self, name: &str) -> Option<String> {
        let cn = CString::new(name).ok()?;
        unsafe {
            let p = ffi::mpv_get_property_string(self.ctx, cn.as_ptr());
            if p.is_null() {
                None
            } else {
                let s = CStr::from_ptr(p).to_string_lossy().into_owned();
                ffi::mpv_free(p as *mut c_void);
                Some(s)
            }
        }
    }

    pub fn get_double(&self, name: &str) -> Option<f64> {
        let cn = CString::new(name).ok()?;
        unsafe {
            let mut v: f64 = 0.0;
            let rc = ffi::mpv_get_property(self.ctx, cn.as_ptr(), ffi::MPV_FORMAT_DOUBLE, &mut v as *mut _ as *mut c_void);
            if rc < 0 {
                None
            } else {
                Some(v)
            }
        }
    }

    pub fn get_i64(&self, name: &str) -> Option<i64> {
        let cn = CString::new(name).ok()?;
        unsafe {
            let mut v: i64 = 0;
            let rc = ffi::mpv_get_property(self.ctx, cn.as_ptr(), ffi::MPV_FORMAT_INT64, &mut v as *mut _ as *mut c_void);
            if rc < 0 {
                None
            } else {
                Some(v)
            }
        }
    }

    pub fn get_flag(&self, name: &str) -> Option<bool> {
        let cn = CString::new(name).ok()?;
        unsafe {
            let mut v: c_int = 0;
            let rc = ffi::mpv_get_property(self.ctx, cn.as_ptr(), ffi::MPV_FORMAT_FLAG, &mut v as *mut _ as *mut c_void);
            if rc < 0 {
                None
            } else {
                Some(v != 0)
            }
        }
    }

    fn snapshot_tracks(&self) -> Vec<TrackInfo> {
        self.shared.tracks.lock().unwrap().clone()
    }

    fn send(&self, ev: EngineEvent) {
        let _ = self.events.send_blocking(ev);
    }

    // -- playback control ----------------------------------------------------

    pub fn load(&self, source: &MediaSource) {
        let _ = self.set_string("ab-loop-a", "no");
        let _ = self.set_string("ab-loop-b", "no");
        let _ = self.set_flag("pause", false);
        if let Err(e) = self.command(&["loadfile", source.resource(), "replace"]) {
            self.send(EngineEvent::EndOfFile {
                reason: EndReason::Error,
                engine_message: Some(e),
            });
        }
    }

    pub fn stop(&self) {
        let _ = self.command(&["stop"]);
    }

    pub fn set_pause(&self, pause: bool) {
        let _ = self.set_flag("pause", pause);
    }

    pub fn toggle_play(&self) {
        let cur = self.get_flag("pause").unwrap_or(true);
        let _ = self.set_flag("pause", !cur);
    }

    pub fn is_paused(&self) -> bool {
        self.get_flag("pause").unwrap_or(true)
    }

    pub fn position(&self) -> Option<f64> {
        self.get_double("time-pos")
    }

    pub fn seek_absolute(&self, t: f64) {
        if std::env::var_os("FRAME_DEBUG").is_some() {
            eprintln!("frame: seek exact t={t:.2}");
        }
        let _ = self.command(&["seek", &format!("{t:.3}"), "absolute"]);
        // Paused seeks publish no frame on their own — keep passes coming
        // until this one has landed.
        self.request_render_passes();
    }

    /// Keyframe seek: lands on the keyframe at or before `t` at a fraction
    /// of an exact seek's decode cost. This is what mpv's own OSC drags the
    /// seek bar with (its `seekbarkeyframes` defaults to true — exact seeks
    /// only on press and final landing). The scrub pump issues these while
    /// the bar is being dragged; the settle timer lands exactly.
    pub fn seek_absolute_keyframes(&self, t: f64) {
        if std::env::var_os("FRAME_DEBUG").is_some() {
            eprintln!("frame: seek kf t={t:.2}");
        }
        let _ = self.command(&["seek", &format!("{t:.3}"), "absolute+keyframes"]);
        // Paused seeks publish no frame on their own — keep passes coming
        // until this one has landed.
        self.request_render_passes();
    }

    pub fn seek_relative(&self, delta: f64) {
        if std::env::var_os("FRAME_DEBUG").is_some() {
            eprintln!(
                "frame: seek relative d={delta:.2} from={:?}",
                self.get_double("time-pos")
            );
        }
        let _ = self.command(&["seek", &format!("{delta:.3}"), "relative"]);
        self.request_render_passes();
    }

    pub fn frame_step(&self) {
        let _ = self.command(&["frame-step"]);
        self.request_render_passes();
    }

    pub fn frame_back_step(&self) {
        let _ = self.command(&["frame-back-step"]);
        self.request_render_passes();
    }

    pub fn set_speed(&self, speed: f64) {
        let _ = self.set_double("speed", speed.clamp(0.1, 10.0));
    }

    pub fn speed(&self) -> f64 {
        self.get_double("speed").unwrap_or(1.0)
    }

    pub fn set_ab_loop(&self, a: Option<f64>, b: Option<f64>) {
        match a {
            Some(t) => {
                let _ = self.set_double("ab-loop-a", t);
            }
            None => {
                let _ = self.set_string("ab-loop-a", "no");
            }
        }
        match b {
            Some(t) => {
                let _ = self.set_double("ab-loop-b", t);
            }
            None => {
                let _ = self.set_string("ab-loop-b", "no");
            }
        }
    }

    pub fn set_loop_file(&self, on: bool) {
        let _ = self.set_string("loop-file", if on { "inf" } else { "no" });
    }

    pub fn loop_file(&self) -> bool {
        matches!(self.get_string("loop-file").as_deref(), Some("inf") | Some("yes"))
    }

    // -- tracks ---------------------------------------------------------------

    pub fn set_audio_track(&self, id: i64) {
        let _ = self.set_i64("aid", id);
    }

    pub fn set_subtitle_track(&self, id: i64) {
        let _ = self.set_i64("sid", id);
        self.request_render_passes();
    }

    pub fn set_secondary_subtitle_track(&self, id: i64) {
        let _ = self.set_i64("secondary-sid", id);
        self.request_render_passes();
    }

    /// Cycle audio tracks (A / Shift+A). Direction +1 or -1.
    pub fn cycle_audio_track(&self, dir: i32) {
        let audio: Vec<i64> = self
            .snapshot_tracks()
            .iter()
            .filter(|t| t.kind == TrackKind::Audio)
            .map(|t| t.id)
            .collect();
        if audio.is_empty() {
            if std::env::var_os("FRAME_DEBUG").is_some() {
                eprintln!("frame: cycle_audio_track: no audio tracks known");
            }
            return;
        }
        let cur = self.get_i64("aid").unwrap_or(0);
        let pos = audio.iter().position(|&id| id == cur);
        let next = match pos {
            Some(p) => {
                let np = (p as i64 + dir as i64).rem_euclid(audio.len() as i64) as usize;
                audio[np]
            }
            None => audio[0],
        };
        if std::env::var_os("FRAME_DEBUG").is_some() {
            eprintln!("frame: cycle_audio_track dir={dir} cur={cur} next={next}");
        }
        self.set_audio_track(next);
    }

    /// Cycle subtitle tracks, including "None" (id 0) (Z / Shift+Z).
    pub fn cycle_subtitle_track(&self, dir: i32) {
        let mut ids: Vec<i64> = self
            .snapshot_tracks()
            .iter()
            .filter(|t| t.kind == TrackKind::Sub)
            .map(|t| t.id)
            .collect();
        ids.sort_unstable();
        ids.insert(0, 0); // None
        let cur = self.get_i64("sid").unwrap_or(0);
        let pos = ids.iter().position(|&id| id == cur).unwrap_or(0);
        let np = (pos as i64 + dir as i64).rem_euclid(ids.len() as i64) as usize;
        self.set_subtitle_track(ids[np]);
    }

    /// Cycle the *secondary* subtitle track, including "None" (id 0).
    pub fn cycle_secondary_subtitle_track(&self, dir: i32) {
        let mut ids: Vec<i64> = self
            .snapshot_tracks()
            .iter()
            .filter(|t| t.kind == TrackKind::Sub)
            .map(|t| t.id)
            .collect();
        ids.sort_unstable();
        ids.insert(0, 0); // None
        let cur = self.get_i64("secondary-sid").unwrap_or(0);
        let pos = ids.iter().position(|&id| id == cur).unwrap_or(0);
        let np = (pos as i64 + dir as i64).rem_euclid(ids.len() as i64) as usize;
        self.set_secondary_subtitle_track(ids[np]);
    }

    pub fn add_external_track(&self, source: &MediaSource, kind: TrackKind) -> Result<(), String> {
        match kind {
            TrackKind::Sub => self.command(&["sub-add", source.resource(), "select"]),
            TrackKind::Audio => self.command(&["audio-add", source.resource(), "select"]),
            TrackKind::Video => self.command(&["video-add", source.resource(), "select"]),
        }
    }

    // -- subtitle & audio adjustments -----------------------------------------

    pub fn adjust_sub_delay(&self, delta: f64) {
        let cur = self.get_double("sub-delay").unwrap_or(0.0);
        let _ = self.set_double("sub-delay", cur + delta);
        self.request_render_passes();
    }

    pub fn set_sub_delay(&self, v: f64) {
        let _ = self.set_double("sub-delay", v);
        self.request_render_passes();
    }

    pub fn adjust_sub_pos(&self, delta: i64) {
        let cur = self.get_i64("sub-pos").unwrap_or(100);
        let _ = self.set_i64("sub-pos", (cur + delta).clamp(0, 100));
        self.request_render_passes();
    }

    pub fn adjust_sub_scale(&self, factor: f64) {
        let cur = self.get_double("sub-scale").unwrap_or(1.0);
        let _ = self.set_double("sub-scale", (cur * factor).clamp(0.1, 10.0));
        self.request_render_passes();
    }

    pub fn set_sub_scale(&self, v: f64) {
        let _ = self.set_double("sub-scale", v.clamp(0.1, 10.0));
        self.request_render_passes();
    }

    pub fn set_sub_visibility(&self, visible: bool) {
        let _ = self.set_flag("sub-visibility", visible);
        self.request_render_passes();
    }

    pub fn adjust_audio_delay(&self, delta: f64) {
        let cur = self.get_double("audio-delay").unwrap_or(0.0);
        let _ = self.set_double("audio-delay", cur + delta);
    }

    pub fn set_audio_delay(&self, v: f64) {
        let _ = self.set_double("audio-delay", v);
    }

    // -- volume ----------------------------------------------------------------

    pub fn set_volume(&self, v: f64) {
        let _ = self.set_double("volume", v.clamp(0.0, 130.0));
    }

    pub fn adjust_volume(&self, delta: f64) {
        let cur = self.get_double("volume").unwrap_or(100.0);
        self.set_volume(cur + delta);
    }

    pub fn toggle_mute(&self) {
        let cur = self.get_flag("mute").unwrap_or(false);
        let _ = self.set_flag("mute", !cur);
    }

    pub fn set_mute(&self, mute: bool) {
        let _ = self.set_flag("mute", mute);
    }

    // -- video picture -----------------------------------------------------------

    pub fn adjust_zoom(&self, delta: f64) {
        let cur = self.get_double("video-zoom").unwrap_or(0.0);
        let _ = self.set_double("video-zoom", (cur + delta).clamp(-1.0, 20.0));
        self.request_render_passes();
    }

    pub fn adjust_color(&self, prop: &str, delta: i64) {
        let cur = self.get_i64(prop).unwrap_or(0);
        let _ = self.set_i64(prop, (cur + delta).clamp(-100, 100));
        self.request_render_passes();
    }

    pub fn set_hwdec(&self, on: bool) {
        let _ = self.set_string("hwdec", if on { "auto" } else { "no" });
    }

    pub fn set_volume_normalization(&self, on: bool) {
        let _ = self.set_string("af", if on { "loudnorm" } else { "" });
    }

    /// Apply subtitle styling from preferences. `color`/`background` are
    /// `#RRGGBBAA` strings; background `None` disables the background box.
    pub fn apply_subtitle_style(&self, font: &str, color: &str, background: Option<&str>) {
        let _ = self.set_string("sub-font", font);
        let _ = self.set_string("sub-color", color);
        match background {
            Some(bg) => {
                let _ = self.set_string("sub-back-color", bg);
            }
            None => {
                let _ = self.set_string("sub-back-color", "#00000000");
            }
        }
    }

    pub fn set_chapter(&self, idx: i64) {
        let _ = self.set_i64("chapter", idx);
        // A chapter change is a seek — its frame must show when paused too.
        self.request_render_passes();
    }

    pub fn current_chapter(&self) -> Option<i64> {
        self.get_i64("chapter")
    }
}

impl Drop for MpvPlayer {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        unsafe {
            ffi::mpv_wakeup(self.ctx);
        }
        if let Some(h) = self.event_thread.lock().unwrap().take() {
            let _ = h.join();
        }
        // The render context must already have been freed on the GL thread
        // (free_render_context); if not, this leaks it by design rather than
        // risking a cross-thread free.
        unsafe {
            ffi::mpv_terminate_destroy(self.ctx);
        }
    }
}

// ---------------------------------------------------------------------------
// Event pump
// ---------------------------------------------------------------------------

fn event_loop(tctx: ThreadHandle, shared: std::sync::Weak<Shared>, events: async_channel::Sender<EngineEvent>) {
    let ctx = tctx.0;
    loop {
        let Some(strong) = shared.upgrade() else { break };
        if strong.stop.load(Ordering::SeqCst) {
            break;
        }
        drop(strong);
        let ev = unsafe { ffi::mpv_wait_event(ctx, 1.0) };
        let (id, _error, data) = unsafe {
            let e = &*ev;
            (e.event_id, e.error, e.data)
        };
        match id {
            ffi::MPV_EVENT_NONE => {}
            ffi::MPV_EVENT_SHUTDOWN => {
                let _ = events.send_blocking(EngineEvent::Shutdown);
                break;
            }
            ffi::MPV_EVENT_LOG_MESSAGE => unsafe {
                let m = &*(data as *mut ffi::mpv_event_log_message);
                let prefix = CStr::from_ptr(m.prefix).to_string_lossy();
                let level = CStr::from_ptr(m.level).to_string_lossy();
                let text = CStr::from_ptr(m.text).to_string_lossy();
                eprintln!("mpv [{prefix}/{level}] {text}");
            },
            ffi::MPV_EVENT_FILE_LOADED => {
                // Re-upgrade only while handling: no strong ref is held
                // across mpv_wait_event (the player must stay droppable).
                if let Some(s) = shared.upgrade() {
                    let snapshot = read_file_snapshot(ctx, &s);
                    let _ = events.send_blocking(snapshot);
                }
            }
            ffi::MPV_EVENT_END_FILE => unsafe {
                let ef = &*(data as *mut ffi::mpv_event_end_file);
                let reason = EndReason::from_mpv(ef.reason);
                let msg = if ef.error < 0 { Some(err_string(ef.error)) } else { None };
                let _ = events.send_blocking(EngineEvent::EndOfFile {
                    reason,
                    engine_message: msg,
                });
            },
            ffi::MPV_EVENT_IDLE => {
                let _ = events.send_blocking(EngineEvent::Idle);
            }
            ffi::MPV_EVENT_PROPERTY_CHANGE => unsafe {
                let prop = &*(data as *mut ffi::mpv_event_property);
                let name = CStr::from_ptr(prop.name).to_string_lossy().into_owned();
                if let Some(s) = shared.upgrade() {
                    if let Some(ev) = convert_property(ctx, &s, &name, prop.format, prop.data) {
                        let _ = events.send_blocking(ev);
                    }
                }
            },
            _ => {}
        }
    }
}

fn read_file_snapshot(ctx: *mut ffi::mpv_handle, shared: &Arc<Shared>) -> EngineEvent {
    unsafe {
        let duration = get_prop_double(ctx, "duration");
        let tracks = get_tracks_node(ctx);
        *shared.tracks.lock().unwrap() = tracks.clone();
        let has_video = tracks.iter().any(|t| t.kind == TrackKind::Video && t.selected);
        let has_audio = tracks.iter().any(|t| t.kind == TrackKind::Audio && t.selected);
        if std::env::var_os("FRAME_DEBUG").is_some() {
            eprintln!(
                "frame: media loaded, hwdec-current={:?}",
                get_prop_string(ctx, "hwdec-current")
            );
        }
        EngineEvent::MediaLoaded {
            duration,
            has_video,
            has_audio,
        }
    }
}

/// Translate one observed property change into an engine event. Returns None
/// for properties the UI derives elsewhere or for unavailable values that
/// need no reaction. `shared` receives the parsed track/chapter snapshots so
/// the engine-side cycle helpers see current data.
#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn convert_property(
    ctx: *mut ffi::mpv_handle,
    shared: &Arc<Shared>,
    name: &str,
    format: c_int,
    data: *mut c_void,
) -> Option<EngineEvent> {
    if format == ffi::MPV_FORMAT_NONE {
        return match name {
            "time-pos" => Some(EngineEvent::PositionChanged(None)),
            "duration" => Some(EngineEvent::DurationChanged(None)),
            "track-list" => {
                *shared.tracks.lock().unwrap() = Vec::new();
                Some(EngineEvent::TracksChanged(Vec::new()))
            }
            "chapter-list" => Some(EngineEvent::ChaptersChanged(Vec::new())),
            "ab-loop-a" => Some(EngineEvent::AbLoopChanged(None, None)),
            _ => None,
        };
    }
    match name {
        "time-pos" => Some(EngineEvent::PositionChanged(Some(*(data as *mut f64)))),
        "duration" => Some(EngineEvent::DurationChanged(Some(*(data as *mut f64)))),
        "pause" => Some(EngineEvent::PauseChanged(*(data as *mut c_int) != 0)),
        "volume" => Some(EngineEvent::VolumeChanged(*(data as *mut f64))),
        "mute" => Some(EngineEvent::MuteChanged(*(data as *mut c_int) != 0)),
        "speed" => Some(EngineEvent::SpeedChanged(*(data as *mut f64))),
        "ab-loop-a" | "ab-loop-b" => {
            // Doubles are per-property; read both to send a consistent pair.
            let a = get_prop_double(ctx, "ab-loop-a");
            let b = get_prop_double(ctx, "ab-loop-b");
            Some(EngineEvent::AbLoopChanged(a, b))
        }
        "sub-delay" => Some(EngineEvent::SubDelayChanged(*(data as *mut f64))),
        "audio-delay" => Some(EngineEvent::AudioDelayChanged(*(data as *mut f64))),
        "sub-pos" => Some(EngineEvent::SubPosChanged(*(data as *mut i64))),
        "sub-scale" => Some(EngineEvent::SubScaleChanged(*(data as *mut f64))),
        "sub-visibility" => Some(EngineEvent::SubVisibilityChanged(*(data as *mut c_int) != 0)),
        "video-zoom" => Some(EngineEvent::ZoomChanged(*(data as *mut f64))),
        "contrast" | "brightness" | "gamma" | "saturation" => Some(EngineEvent::ColorChanged {
            contrast: get_prop_i64(ctx, "contrast").unwrap_or(0),
            brightness: get_prop_i64(ctx, "brightness").unwrap_or(0),
            gamma: get_prop_i64(ctx, "gamma").unwrap_or(0),
            saturation: get_prop_i64(ctx, "saturation").unwrap_or(0),
        }),
        "sid" | "secondary-sid" | "aid" => Some(EngineEvent::TrackSelectionChanged {
            sid: get_prop_i64(ctx, "sid").unwrap_or(0),
            secondary_sid: get_prop_i64(ctx, "secondary-sid").unwrap_or(0),
            aid: get_prop_i64(ctx, "aid").unwrap_or(0),
        }),
        "loop-file" => {
            let s = CStr::from_ptr(*(data as *mut *const c_char)).to_string_lossy().into_owned();
            Some(EngineEvent::LoopFileChanged(s == "inf" || s == "yes"))
        }
        "seekable" => Some(EngineEvent::SeekableChanged(*(data as *mut c_int) != 0)),
        "track-list" => {
            let node = data as *mut ffi::mpv_node;
            let tracks = parse_track_list(&*node);
            *shared.tracks.lock().unwrap() = tracks.clone();
            if std::env::var_os("FRAME_DEBUG").is_some() {
                eprintln!("frame: engine track-list change -> {} tracks", tracks.len());
            }
            Some(EngineEvent::TracksChanged(tracks))
        }
        "chapter-list" => {
            let node = data as *mut ffi::mpv_node;
            let chapters = parse_chapter_list(&*node);
            Some(EngineEvent::ChaptersChanged(chapters))
        }
        _ => None,
    }
}

unsafe fn get_prop_double(ctx: *mut ffi::mpv_handle, name: &str) -> Option<f64> {
    let cn = CString::new(name).ok()?;
    let mut v: f64 = 0.0;
    let rc = ffi::mpv_get_property(ctx, cn.as_ptr(), ffi::MPV_FORMAT_DOUBLE, &mut v as *mut _ as *mut c_void);
    if rc < 0 {
        None
    } else {
        Some(v)
    }
}

unsafe fn get_prop_i64(ctx: *mut ffi::mpv_handle, name: &str) -> Option<i64> {
    let cn = CString::new(name).ok()?;
    let mut v: i64 = 0;
    let rc = ffi::mpv_get_property(ctx, cn.as_ptr(), ffi::MPV_FORMAT_INT64, &mut v as *mut _ as *mut c_void);
    if rc < 0 {
        None
    } else {
        Some(v)
    }
}

unsafe fn get_prop_string(ctx: *mut ffi::mpv_handle, name: &str) -> Option<String> {
    let cn = CString::new(name).ok()?;
    let p = ffi::mpv_get_property_string(ctx, cn.as_ptr());
    if p.is_null() {
        None
    } else {
        let s = CStr::from_ptr(p).to_string_lossy().into_owned();
        ffi::mpv_free(p as *mut c_void);
        Some(s)
    }
}

unsafe fn get_tracks_node(ctx: *mut ffi::mpv_handle) -> Vec<TrackInfo> {
    let cn = CString::new("track-list").unwrap();
    let mut node: ffi::mpv_node = std::mem::zeroed();
    let rc = ffi::mpv_get_property(ctx, cn.as_ptr(), ffi::MPV_FORMAT_NODE, &mut node as *mut _ as *mut c_void);
    if rc < 0 {
        return Vec::new();
    }
    let tracks = parse_track_list(&node);
    ffi::mpv_free_node_contents(&mut node);
    tracks
}

// ---------------------------------------------------------------------------
// Node parsing (track-list / chapter-list)
// ---------------------------------------------------------------------------

fn node_str(node: &ffi::mpv_node) -> Option<String> {
    unsafe {
        if node.format != ffi::MPV_FORMAT_STRING {
            return None;
        }
        Some(CStr::from_ptr(node.u.string).to_string_lossy().into_owned())
    }
}

fn node_flag(node: &ffi::mpv_node) -> bool {
    unsafe { node.format == ffi::MPV_FORMAT_FLAG && node.u.flag != 0 }
}

fn node_i64(node: &ffi::mpv_node) -> Option<i64> {
    unsafe {
        match node.format {
            ffi::MPV_FORMAT_INT64 => Some(node.u.int64),
            ffi::MPV_FORMAT_DOUBLE => Some(node.u.double_ as i64),
            _ => None,
        }
    }
}

fn node_f64(node: &ffi::mpv_node) -> Option<f64> {
    unsafe {
        match node.format {
            ffi::MPV_FORMAT_DOUBLE => Some(node.u.double_),
            ffi::MPV_FORMAT_INT64 => Some(node.u.int64 as f64),
            _ => None,
        }
    }
}

/// Parse `track-list` (NODE_MAP array) into TrackInfos.
pub fn parse_track_list(node: &ffi::mpv_node) -> Vec<TrackInfo> {
    let mut out = Vec::new();
    unsafe {
        if node.format != ffi::MPV_FORMAT_NODE_ARRAY {
            return out;
        }
        let list = &*node.u.list;
        for i in 0..list.num {
            let entry = &*list.values.add(i as usize);
            if entry.format != ffi::MPV_FORMAT_NODE_MAP {
                continue;
            }
            let map = &*entry.u.list;
            let mut track = TrackInfo {
                id: 0,
                kind: TrackKind::Video,
                selected: false,
                title: None,
                lang: None,
                codec: None,
                external: false,
                default: false,
                forced: false,
            };
            for k in 0..map.num {
                let key = match map.keys.add(k as usize).read() {
                    p if p.is_null() => continue,
                    p => CStr::from_ptr(p).to_string_lossy().into_owned(),
                };
                let val = &*map.values.add(k as usize);
                match key.as_str() {
                    "id" => track.id = node_i64(val).unwrap_or(0),
                    "type" => {
                        track.kind = match node_str(val).as_deref() {
                            Some("audio") => TrackKind::Audio,
                            Some("sub") => TrackKind::Sub,
                            _ => TrackKind::Video,
                        }
                    }
                    "selected" => track.selected = node_flag(val),
                    "title" => track.title = node_str(val),
                    "lang" => track.lang = node_str(val),
                    "codec" => track.codec = node_str(val),
                    "external" => track.external = node_flag(val),
                    "default" => track.default = node_flag(val),
                    "forced" => track.forced = node_flag(val),
                    _ => {}
                }
            }
            out.push(track);
        }
    }
    out
}

/// Parse `chapter-list` (NODE_MAP array with `title` + `time`).
pub fn parse_chapter_list(node: &ffi::mpv_node) -> Vec<ChapterInfo> {
    let mut out = Vec::new();
    unsafe {
        if node.format != ffi::MPV_FORMAT_NODE_ARRAY {
            return out;
        }
        let list = &*node.u.list;
        for i in 0..list.num {
            let entry = &*list.values.add(i as usize);
            if entry.format != ffi::MPV_FORMAT_NODE_MAP {
                continue;
            }
            let map = &*entry.u.list;
            let mut title = None;
            let mut time = 0.0;
            for k in 0..map.num {
                let key = match map.keys.add(k as usize).read() {
                    p if p.is_null() => continue,
                    p => CStr::from_ptr(p).to_string_lossy().into_owned(),
                };
                let val = &*map.values.add(k as usize);
                match key.as_str() {
                    "title" => title = node_str(val),
                    "time" => time = node_f64(val).unwrap_or(0.0),
                    _ => {}
                }
            }
            out.push(ChapterInfo { title, time });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// PNG encoding for screenshots
// ---------------------------------------------------------------------------

fn save_png(path: &PathBuf, w: i32, h: i32, rgba: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w.max(1) as u32, h.max(1) as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an mpv_node map from (key, value) pairs. Values limited to
    /// strings, flags, int64s and doubles — enough for track-list entries.
    unsafe fn make_map<'a>(entries: &[(&str, MapVal)], storage: &'a mut NodeStorage) -> ffi::mpv_node {
        // Strings live in boxed storage so the pointers handed to the parser
        // stay valid regardless of later pushes (Vec buffers would move).
        let mut keys: Vec<*mut c_char> = Vec::new();
        let mut values: Vec<ffi::mpv_node> = Vec::new();
        for (k, v) in entries {
            let ks: Box<CStr> = Box::from(CString::new(*k).unwrap().as_c_str());
            keys.push(Box::into_raw(ks) as *mut c_char);
            values.push(match v {
                MapVal::Str(s) => {
                    let vs: Box<CStr> = Box::from(CString::new(*s).unwrap().as_c_str());
                    ffi::mpv_node {
                        format: ffi::MPV_FORMAT_STRING,
                        u: ffi::mpv_node_u { string: Box::into_raw(vs) as *mut c_char },
                    }
                }
                MapVal::Flag(b) => ffi::mpv_node {
                    format: ffi::MPV_FORMAT_FLAG,
                    u: ffi::mpv_node_u { flag: *b as c_int },
                },
                MapVal::Int(i) => ffi::mpv_node {
                    format: ffi::MPV_FORMAT_INT64,
                    u: ffi::mpv_node_u { int64: *i },
                },
                MapVal::Double(d) => ffi::mpv_node {
                    format: ffi::MPV_FORMAT_DOUBLE,
                    u: ffi::mpv_node_u { double_: *d },
                },
            });
        }
        storage.keys_bufs.push(keys);
        storage.value_bufs.push(values);
        storage.lists.push(Box::new(ffi::mpv_node_list {
            num: entries.len() as c_int,
            values: storage.value_bufs.last_mut().unwrap().as_mut_ptr(),
            keys: storage.keys_bufs.last_mut().unwrap().as_mut_ptr(),
        }));
        ffi::mpv_node {
            format: ffi::MPV_FORMAT_NODE_MAP,
            u: ffi::mpv_node_u { list: &mut **storage.lists.last_mut().unwrap() },
        }
    }

    unsafe fn make_array(items: &mut [ffi::mpv_node], storage: &mut NodeStorage) -> ffi::mpv_node {
        storage.value_bufs.push(items.to_vec());
        storage.lists.push(Box::new(ffi::mpv_node_list {
            num: items.len() as c_int,
            values: storage.value_bufs.last_mut().unwrap().as_mut_ptr(),
            keys: std::ptr::null_mut(),
        }));
        ffi::mpv_node {
            format: ffi::MPV_FORMAT_NODE_ARRAY,
            u: ffi::mpv_node_u { list: &mut **storage.lists.last_mut().unwrap() },
        }
    }

    /// All buffers are heap-allocated with stable addresses (Box / owned Vecs
    /// kept for the test's lifetime), so the raw node graph stays valid.
    #[derive(Default)]
    struct NodeStorage {
        keys_bufs: Vec<Vec<*mut c_char>>,
        value_bufs: Vec<Vec<ffi::mpv_node>>,
        lists: Vec<Box<ffi::mpv_node_list>>,
    }

    enum MapVal {
        Str(&'static str),
        Flag(bool),
        Int(i64),
        Double(f64),
    }

    #[test]
    fn parse_track_list_two_tracks() {
        unsafe {
            let mut storage = NodeStorage::default();
            let t1 = make_map(
                &[
                    ("id", MapVal::Int(1)),
                    ("type", MapVal::Str("audio")),
                    ("selected", MapVal::Flag(true)),
                    ("codec", MapVal::Str("aac")),
                    ("lang", MapVal::Str("eng")),
                    ("default", MapVal::Flag(true)),
                ],
                &mut storage,
            );
            let t2 = make_map(
                &[
                    ("id", MapVal::Int(2)),
                    ("type", MapVal::Str("sub")),
                    ("selected", MapVal::Flag(false)),
                    ("external", MapVal::Flag(true)),
                    ("title", MapVal::Str("Commentary")),
                ],
                &mut storage,
            );
            let mut items = vec![t1, t2];
            let node = make_array(&mut items, &mut storage);
            let tracks = parse_track_list(&node);
            assert_eq!(tracks.len(), 2);
            assert_eq!(tracks[0].kind, TrackKind::Audio);
            assert_eq!(tracks[0].id, 1);
            assert!(tracks[0].selected);
            assert_eq!(tracks[0].codec.as_deref(), Some("aac"));
            assert!(tracks[0].default);
            assert_eq!(tracks[1].kind, TrackKind::Sub);
            assert!(!tracks[1].selected);
            assert!(tracks[1].external);
            assert_eq!(tracks[1].title.as_deref(), Some("Commentary"));
            assert!(tracks[1].label().contains("external"));
        }
    }

    #[test]
    fn parse_chapter_list() {
        unsafe {
            let mut storage = NodeStorage::default();
            let c1 = make_map(&[("title", MapVal::Str("Intro")), ("time", MapVal::Double(0.0))], &mut storage);
            let c2 = make_map(&[("title", MapVal::Str("Main")), ("time", MapVal::Double(91.5))], &mut storage);
            let mut items = vec![c1, c2];
            let node = make_array(&mut items, &mut storage);
            let chapters = super::parse_chapter_list(&node);
            assert_eq!(chapters.len(), 2);
            assert_eq!(chapters[0].title.as_deref(), Some("Intro"));
            assert!((chapters[1].time - 91.5).abs() < 1e-9);
        }
    }

    #[test]
    fn end_reason_mapping() {
        assert_eq!(EndReason::from_mpv(ffi::MPV_END_FILE_REASON_EOF), EndReason::Eof);
        assert_eq!(EndReason::from_mpv(ffi::MPV_END_FILE_REASON_ERROR), EndReason::Error);
        assert_eq!(EndReason::from_mpv(99), EndReason::Other(99));
    }
}
