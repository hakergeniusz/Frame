//! Application shell: single-instance GtkApplication, CLI/open routing,
//! app-level actions and windows.

use crate::config::{AppRuntimePaths, APP_ID};
use crate::engine::MediaSource;
use crate::rt::AppRuntime;
use adw::prelude::*;
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

pub fn run() -> glib::ExitCode {
    gio::resources_register_include!("resources.gresource")
        .expect("failed to register built-in resources");
    gtk::init().expect("failed to initialize GTK");
    adw::init().expect("failed to initialize libadwaita");
    // libmpv refuses to initialize when LC_NUMERIC is not "C" (it parses
    // numbers with the C locale). GTK's init calls setlocale(LC_ALL, ""), so
    // the numeric-only override goes here — the UI keeps the user's language.
    unsafe {
        libc::setlocale(libc::LC_NUMERIC, b"C\0".as_ptr() as *const libc::c_char);
    }

    // Built-in icons (also installed system-wide by the packages).
    gtk::IconTheme::default()
        .add_resource_path("/io/github/hakergeniusz/Frame/icons");
    gtk::Window::set_default_icon_name(crate::config::APP_ID);

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    let paths = AppRuntimePaths::detect();
    let rt_cell: Rc<RefCell<Option<Arc<AppRuntime>>>> = Rc::new(RefCell::new(None));
    let windows: Rc<RefCell<Vec<glib::WeakRef<gtk::Window>>>> = Rc::new(RefCell::new(Vec::new()));
    let restored = Rc::new(Cell::new(false));

    {
        let rt_cell = rt_cell.clone();
        let windows = windows.clone();
        let restored = restored.clone();
        let paths = paths.clone();
        let app2 = app.clone();
        app.connect_activate(move |_| {
            let rt = rt_with(&rt_cell, &paths);

            // A second argument-less launch while running presents the most
            // recent window (assumption recorded; docs/07 §3).
            if present_recent(&windows) {
                return;
            }

            // Restore Saved Session (once, at startup; docs/04).
            if !restored.get() {
                restored.set(true);
                rt.restore_session_if_enabled();
            }

            if rt.player_count() == 0 {
                let w = crate::ui::welcome::create_welcome_window(rt.clone());
                crate::ui::welcome::connect_keepalive(&w);
                register_window(&windows, w.window.upcast_ref());
            }
        });
    }

    {
        let rt_cell = rt_cell.clone();
        let windows = windows.clone();
        let paths = paths.clone();
        app.connect_open(move |_, files, _| {
            let rt = rt_with(&rt_cell, &paths);
            let mut sources = Vec::new();
            let mut missing = Vec::new();
            for file in files {
                match file.path() {
                    Some(path) => {
                        if path.exists() {
                            sources.push(MediaSource::Path(path.to_string_lossy().into_owned()));
                        } else {
                            // Sandbox-invisible or missing: visible error
                            // (docs/06 suggestion) instead of silent retries.
                            missing.push(
                                path.file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| path.to_string_lossy().into_owned()),
                            );
                        }
                    }
                    None => {
                        // Non-local file (e.g. an http URL passed via the
                        // desktop "Open With" flow).
                        let uri = file.uri().to_string();
                        if uri.starts_with("http://") || uri.starts_with("https://") {
                            sources.push(MediaSource::Url(uri.clone()));
                        } else {
                            missing.push(uri);
                        }
                    }
                }
            }
            if !sources.is_empty() {
                rt.apply_open(sources, false, None);
            }
            if !missing.is_empty() {
                let welcome = ensure_welcome(&rt, &windows);
                crate::ui::util::toast(
                    &welcome.toast_overlay,
                    &format!("File Error: cannot access {}", missing.join(", ")),
                );
            }
        });
    }

    // App-level actions.
    {
        let rt_cell = rt_cell.clone();
        let windows = windows.clone();
        let paths = paths.clone();
        let a = gio::SimpleAction::new("new-window", None);
        a.connect_activate(move |_, _| {
            let rt = rt_with(&rt_cell, &paths);
            let w = crate::ui::welcome::create_welcome_window(rt);
            crate::ui::welcome::connect_keepalive(&w);
            register_window(&windows, w.window.upcast_ref());
        });
        app.add_action(&a);
    }
    {
        let rt_cell = rt_cell.clone();
        let paths = paths.clone();
        let app2 = app.clone();
        let a = gio::SimpleAction::new("preferences", None);
        a.connect_activate(move |_, _| {
            let rt = rt_with(&rt_cell, &paths);
            if let Some(parent) = app2.active_window() {
                crate::ui::preferences::show_preferences(&rt, &parent);
            }
        });
        app.add_action(&a);
    }
    {
        let app2 = app.clone();
        let a = gio::SimpleAction::new("shortcuts", None);
        a.connect_activate(move |_, _| {
            if let Some(parent) = app2.active_window() {
                crate::ui::shortcuts::show_shortcuts(&parent);
            }
        });
        app.add_action(&a);
    }
    {
        let app2 = app.clone();
        let a = gio::SimpleAction::new("about", None);
        a.connect_activate(move |_, _| {
            if let Some(parent) = app2.active_window() {
                crate::ui::about::show_about(&parent);
            }
        });
        app.add_action(&a);
    }
    {
        let app2 = app.clone();
        let a = gio::SimpleAction::new("quit", None);
        a.connect_activate(move |_, _| {
            app2.quit();
        });
        app.add_action(&a);
    }
    // A real accelerator, not a window key handler: Ctrl+Q must quit from
    // every window, including the Shortcuts window (an adw::Window with no
    // key controller — the manual handlers could never see it there).
    app.set_accels_for_action("app.quit", &["<Primary>Q"]);

    app.run()
}

fn rt_with(cell: &RefCell<Option<Arc<AppRuntime>>>, paths: &AppRuntimePaths) -> Arc<AppRuntime> {
    cell.borrow_mut().get_or_insert_with(|| AppRuntime::new(paths)).clone()
}

fn register_window(windows: &RefCell<Vec<glib::WeakRef<gtk::Window>>>, window: &gtk::Window) {
    let mut weak = glib::WeakRef::new();
    weak.set(Some(window));
    windows.borrow_mut().push(weak);
}

/// Present the most recently opened live window; returns true when one was
/// found (dead references are pruned).
fn present_recent(windows: &RefCell<Vec<glib::WeakRef<gtk::Window>>>) -> bool {
    let mut list = windows.borrow_mut();
    list.retain(|w| w.upgrade().is_some());
    for w in list.iter().rev() {
        if let Some(win) = w.upgrade() {
            win.present();
            return true;
        }
    }
    false
}

fn ensure_welcome(
    rt: &Arc<AppRuntime>,
    windows: &RefCell<Vec<glib::WeakRef<gtk::Window>>>,
) -> Rc<crate::ui::welcome::WelcomeWindow> {
    // The CLI-open error case has no window of its own — create a fresh
    // welcome window to carry the toast.
    let w = crate::ui::welcome::create_welcome_window(rt.clone());
    crate::ui::welcome::connect_keepalive(&w);
    register_window(windows, w.window.upcast_ref());
    w
}
