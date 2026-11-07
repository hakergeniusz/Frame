//! Frame — independent desktop media player.
//!
//! Application/UI code licensed MIT; the mpv playback engine is dynamically
//! linked and keeps its own licence — see THIRD_PARTY_LICENSES.md.

mod app;
mod config;
mod engine;
mod history;
mod inhibit;
mod player;
mod resume;
mod rt;
mod ui;

fn main() -> glib::ExitCode {
    app::run()
}
