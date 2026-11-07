//! Hand-written FFI bindings for the subset of the libmpv client and render
//! APIs that Frame uses. Written against the upstream public headers
//! (client.h / render.h / render_gl.h of mpv 0.41, API version 2.5) — these
//! are stable, documented C APIs, not an implementation dependency.
//!
//! Only what the player needs is declared; everything else stays out.

#![allow(non_camel_case_types, dead_code)]

use std::os::raw::{c_char, c_int, c_ulonglong, c_void};

// ---------------------------------------------------------------------------
// client.h — core API
// ---------------------------------------------------------------------------

pub type mpv_handle = c_void;

pub const MPV_FORMAT_NONE: c_int = 0;
pub const MPV_FORMAT_STRING: c_int = 1;
pub const MPV_FORMAT_OSD_STRING: c_int = 2;
pub const MPV_FORMAT_FLAG: c_int = 3;
pub const MPV_FORMAT_INT64: c_int = 4;
pub const MPV_FORMAT_DOUBLE: c_int = 5;
pub const MPV_FORMAT_NODE: c_int = 6;
pub const MPV_FORMAT_NODE_ARRAY: c_int = 7;
pub const MPV_FORMAT_NODE_MAP: c_int = 8;
pub const MPV_FORMAT_BYTE_ARRAY: c_int = 9;

// Events
pub const MPV_EVENT_NONE: c_int = 0;
pub const MPV_EVENT_SHUTDOWN: c_int = 1;
pub const MPV_EVENT_LOG_MESSAGE: c_int = 2;
pub const MPV_EVENT_GET_PROPERTY_REPLY: c_int = 3;
pub const MPV_EVENT_SET_PROPERTY_REPLY: c_int = 4;
pub const MPV_EVENT_COMMAND_REPLY: c_int = 5;
pub const MPV_EVENT_START_FILE: c_int = 6;
pub const MPV_EVENT_END_FILE: c_int = 7;
pub const MPV_EVENT_FILE_LOADED: c_int = 8;
pub const MPV_EVENT_IDLE: c_int = 11;
pub const MPV_EVENT_TICK: c_int = 14;
pub const MPV_EVENT_VIDEO_RECONFIG: c_int = 17;
pub const MPV_EVENT_AUDIO_RECONFIG: c_int = 18;
pub const MPV_EVENT_SEEK: c_int = 20;
pub const MPV_EVENT_PLAYBACK_RESTART: c_int = 21;
pub const MPV_EVENT_PROPERTY_CHANGE: c_int = 22;
pub const MPV_EVENT_QUEUE_OVERFLOW: c_int = 24;

// End-file reasons
pub const MPV_END_FILE_REASON_EOF: c_int = 0;
pub const MPV_END_FILE_REASON_STOP: c_int = 2;
pub const MPV_END_FILE_REASON_QUIT: c_int = 3;
pub const MPV_END_FILE_REASON_ERROR: c_int = 4;
pub const MPV_END_FILE_REASON_REDIRECT: c_int = 5;

// A few error codes we translate.
pub const MPV_ERROR_SUCCESS: c_int = 0;
pub const MPV_ERROR_OPTION_NOT_FOUND: c_int = -5;
pub const MPV_ERROR_PROPERTY_UNAVAILABLE: c_int = -10;
pub const MPV_ERROR_PROPERTY_ERROR: c_int = -11;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct mpv_node {
    pub u: mpv_node_u,
    pub format: c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union mpv_node_u {
    pub string: *mut c_char,
    pub flag: c_int,
    pub int64: i64,
    pub double_: f64,
    pub list: *mut mpv_node_list,
    pub ba: *mut c_void,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct mpv_node_list {
    pub num: c_int,
    pub values: *mut mpv_node,
    pub keys: *mut *mut c_char,
}

#[repr(C)]
pub struct mpv_event_property {
    pub name: *const c_char,
    pub format: c_int,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct mpv_event_end_file {
    pub reason: c_int,
    pub error: c_int,
    pub playlist_entry_id: c_ulonglong,
    pub playlist_insert_id: c_ulonglong,
    pub playlist_insert_num_entries: c_int,
}

#[repr(C)]
pub struct mpv_event_log_message {
    pub prefix: *const c_char,
    pub level: *const c_char,
    pub text: *const c_char,
    pub log_level: c_int,
}

#[repr(C)]
pub struct mpv_event {
    pub event_id: c_int,
    pub error: c_int,
    pub reply_userdata: c_ulonglong,
    pub data: *mut c_void,
}

pub type mpv_set_wakeup_callback_fn = Option<unsafe extern "C" fn(*mut c_void)>;

unsafe extern "C" {
    pub fn mpv_create() -> *mut mpv_handle;
    pub fn mpv_initialize(ctx: *mut mpv_handle) -> c_int;
    pub fn mpv_terminate_destroy(ctx: *mut mpv_handle);
    pub fn mpv_set_option_string(ctx: *mut mpv_handle, name: *const c_char, data: *const c_char) -> c_int;
    pub fn mpv_command(ctx: *mut mpv_handle, args: *const *const c_char) -> c_int;
    pub fn mpv_set_property_string(ctx: *mut mpv_handle, name: *const c_char, data: *const c_char) -> c_int;
    pub fn mpv_set_property(ctx: *mut mpv_handle, name: *const c_char, format: c_int, data: *mut c_void) -> c_int;
    pub fn mpv_get_property(ctx: *mut mpv_handle, name: *const c_char, format: c_int, data: *mut c_void) -> c_int;
    pub fn mpv_get_property_string(ctx: *mut mpv_handle, name: *const c_char) -> *mut c_char;
    pub fn mpv_observe_property(ctx: *mut mpv_handle, reply_userdata: c_ulonglong, name: *const c_char, format: c_int) -> c_int;
    pub fn mpv_wakeup(ctx: *mut mpv_handle);
    pub fn mpv_wait_event(ctx: *mut mpv_handle, timeout: f64) -> *mut mpv_event;
    pub fn mpv_free(data: *mut c_void);
    pub fn mpv_free_node_contents(node: *mut mpv_node);
    pub fn mpv_set_wakeup_callback(ctx: *mut mpv_handle, cb: mpv_set_wakeup_callback_fn, d: *mut c_void);
    pub fn mpv_request_log_messages(ctx: *mut mpv_handle, min_level: *const c_char) -> c_int;
    pub fn mpv_error_string(error: c_int) -> *const c_char;
    pub fn mpv_client_api_version() -> c_ulonglong;
}

// ---------------------------------------------------------------------------
// render.h / render_gl.h — OpenGL render API
// ---------------------------------------------------------------------------

pub type mpv_render_context = c_void;

pub const MPV_RENDER_API_TYPE_OPENGL: &str = "opengl";

pub const MPV_RENDER_PARAM_INVALID: c_int = 0;
pub const MPV_RENDER_PARAM_API_TYPE: c_int = 1;
pub const MPV_RENDER_PARAM_OPENGL_INIT_PARAMS: c_int = 2;
pub const MPV_RENDER_PARAM_OPENGL_FBO: c_int = 3;
pub const MPV_RENDER_PARAM_FLIP_Y: c_int = 4;
pub const MPV_RENDER_PARAM_DEPTH: c_int = 5;
pub const MPV_RENDER_PARAM_ICC_PROFILE: c_int = 6;
pub const MPV_RENDER_PARAM_ADVANCED_CONTROL: c_int = 10;
pub const MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME: c_int = 12;
pub const MPV_RENDER_PARAM_SKIP_RENDERING: c_int = 13;

pub const MPV_RENDER_UPDATE_FRAME: u64 = 1 << 0;
pub const MPV_RENDER_UPDATE_REDRAW: u64 = 1 << 1;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct mpv_render_param {
    pub param_type: c_int,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct mpv_opengl_init_params {
    pub get_proc_address: Option<unsafe extern "C" fn(ctx: *mut c_void, name: *const c_char) -> *mut c_void>,
    pub get_proc_address_ctx: *mut c_void,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct mpv_opengl_fbo {
    pub fbo: c_int,
    pub w: c_int,
    pub h: c_int,
    pub internal_format: c_int,
}

pub type mpv_render_update_fn = Option<unsafe extern "C" fn(*mut c_void)>;

unsafe extern "C" {
    pub fn mpv_render_context_create(
        res: *mut *mut mpv_render_context,
        ctx: *mut mpv_handle,
        params: *mut mpv_render_param,
    ) -> c_int;
    pub fn mpv_render_context_render(ctx: *mut mpv_render_context, params: *mut mpv_render_param) -> c_int;
    pub fn mpv_render_context_update(ctx: *mut mpv_render_context) -> u64;
    pub fn mpv_render_context_set_update_callback(
        ctx: *mut mpv_render_context,
        cb: mpv_render_update_fn,
        cb_ctx: *mut c_void,
    );
    pub fn mpv_render_context_report_swap(ctx: *mut mpv_render_context);
    pub fn mpv_render_context_free(ctx: *mut mpv_render_context);
}

/// libglvnd's vendor-neutral GL symbol resolution (libEGL/libGLX are part of
/// GTK 4's GL stack on Linux): eglGetProcAddress covers EGL contexts and GLVND
/// core/extension functions, glXGetProcAddress covers the desktop-GL GLX path.
unsafe extern "C" {
    pub fn eglGetProcAddress(name: *const c_char) -> *mut c_void;
    pub fn glXGetProcAddress(name: *const c_char) -> *mut c_void;
}
