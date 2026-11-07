//! File/folder choosers.
//!
//! docs/01: file and folder selection use the desktop file chooser — under
//! GTK 4 `FileDialog` transparently delegates to the portal when sandboxed,
//! matching the observed behavior. Filters: video files / folders / external
//! subtitle & audio tracks.

use gtk::prelude::*;
use gtk::{gio, glib};

fn video_filter() -> gio::ListStore {
    let f = gtk::FileFilter::new();
    f.set_name(Some("Video files"));
    f.add_mime_type("video/*");
    // Also accept plain audio files opened as media.
    let f2 = gtk::FileFilter::new();
    f2.set_name(Some("Audio files"));
    f2.add_mime_type("audio/*");
    let store = gio::ListStore::new::<gtk::FileFilter>();
    store.append(&f);
    store.append(&f2);
    store
}

fn subtitle_filter() -> gio::ListStore {
    let f = gtk::FileFilter::new();
    f.set_name(Some("Subtitle files"));
    for mime in [
        "application/x-subrip",
        "text/vtt",
        "text/x-ssa",
        "application/x-ssa",
        "application/octet-stream",
    ] {
        f.add_mime_type(mime);
    }
    let store = gio::ListStore::new::<gtk::FileFilter>();
    store.append(&f);
    store
}

fn audio_filter() -> gio::ListStore {
    let f = gtk::FileFilter::new();
    f.set_name(Some("Audio files"));
    f.add_mime_type("audio/*");
    let store = gio::ListStore::new::<gtk::FileFilter>();
    store.append(&f);
    store
}

/// Pick one or more video files.
pub fn choose_files<W: IsA<gtk::Window>>(
    parent: &W,
    on_result: impl FnOnce(Vec<gio::File>) + 'static,
) {
    let dialog = gtk::FileDialog::builder()
        .title("Open Files")
        .filters(&video_filter())
        .modal(true)
        .build();
    dialog.open_multiple(
        Some(parent),
        gio::Cancellable::NONE,
        move |result| match result {
            Ok(files) => {
                let mut out: Vec<gio::File> = Vec::new();
                for i in 0..files.n_items() {
                    if let Some(f) = files.item(i).and_downcast::<gio::File>() {
                        out.push(f);
                    }
                }
                on_result(out);
            }
            Err(e) => {
                if !e.matches(gtk::DialogError::Dismissed) {
                    eprintln!("frame: file chooser failed: {e}");
                }
                on_result(Vec::new());
            }
        },
    );
}

/// Pick a folder.
pub fn choose_folder<W: IsA<gtk::Window>>(
    parent: &W,
    on_result: impl FnOnce(Option<gio::File>) + 'static,
) {
    let dialog = gtk::FileDialog::builder()
        .title("Open Folder")
        .modal(true)
        .build();
    dialog.select_folder(
        Some(parent),
        gio::Cancellable::NONE,
        move |result| match result {
            Ok(file) => on_result(Some(file)),
            Err(e) => {
                if !e.matches(gtk::DialogError::Dismissed) {
                    eprintln!("frame: folder chooser failed: {e}");
                }
                on_result(None);
            }
        },
    );
}

/// Pick an external subtitle file.
pub fn choose_subtitle_file<W: IsA<gtk::Window>>(
    parent: &W,
    on_result: impl FnOnce(Option<gio::File>) + 'static,
) {
    let dialog = gtk::FileDialog::builder()
        .title("Add Subtitle Track")
        .filters(&subtitle_filter())
        .modal(true)
        .build();
    dialog.open(
        Some(parent),
        gio::Cancellable::NONE,
        move |result| match result {
            Ok(file) => on_result(Some(file)),
            Err(e) => {
                if !e.matches(gtk::DialogError::Dismissed) {
                    eprintln!("frame: subtitle chooser failed: {e}");
                }
                on_result(None);
            }
        },
    );
}

/// Pick an external audio file.
pub fn choose_audio_file<W: IsA<gtk::Window>>(
    parent: &W,
    on_result: impl FnOnce(Option<gio::File>) + 'static,
) {
    let dialog = gtk::FileDialog::builder()
        .title("Add Audio Track")
        .filters(&audio_filter())
        .modal(true)
        .build();
    dialog.open(
        Some(parent),
        gio::Cancellable::NONE,
        move |result| match result {
            Ok(file) => on_result(Some(file)),
            Err(e) => {
                if !e.matches(gtk::DialogError::Dismissed) {
                    eprintln!("frame: audio chooser failed: {e}");
                }
                on_result(None);
            }
        },
    );
}
