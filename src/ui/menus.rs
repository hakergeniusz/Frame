//! The player header's `Open ⌄` menu (docs/03).

use gtk::gio;
use gtk::glib::prelude::*;

fn item(label: &str, action: &str, accel: Option<&str>) -> gio::MenuItem {
    let it = gio::MenuItem::new(Some(label), Some(action));
    if let Some(a) = accel {
        it.set_attribute_value("accel", Some(&a.to_variant()));
    }
    it
}

/// Menu model for the player window header:
/// Open Files/Folder/URL, Add Files/Folder/URL, ─, Add Subtitle Track,
/// Add Audio Track, ─, Watch History.
pub fn open_menu() -> gio::Menu {
    let menu = gio::Menu::new();

    let open = gio::Menu::new();
    open.append_item(&item("Open Files", "win.open-files", Some("<Ctrl>O")));
    open.append_item(&item("Open Folder", "win.open-folder", Some("<Ctrl>I")));
    open.append_item(&item("Open URL", "win.open-url", Some("<Ctrl>U")));
    menu.append_section(None, &open);

    let add = gio::Menu::new();
    add.append_item(&item("Add Files", "win.add-files", Some("<Ctrl><Shift>O")));
    add.append_item(&item("Add Folder", "win.add-folder", Some("<Ctrl><Shift>I")));
    add.append_item(&item("Add URL", "win.add-url", Some("<Ctrl><Shift>U")));
    menu.append_section(None, &add);

    let tracks = gio::Menu::new();
    tracks.append_item(&item("Add Subtitle Track", "win.add-subtitle-track", None));
    tracks.append_item(&item("Add Audio Track", "win.add-audio-track", None));
    menu.append_section(None, &tracks);

    let history = gio::Menu::new();
    history.append_item(&item("Watch History", "win.watch-history", Some("<Ctrl>H")));
    menu.append_section(None, &history);

    menu
}

/// Context menu shown when Secondary Click is set to "Context Menu"
/// (option exists per docs/04; contents unobserved — sensible minimum,
/// recorded as an assumption).
pub fn context_menu() -> gio::Menu {
    let menu = gio::Menu::new();

    let play = gio::Menu::new();
    play.append_item(&item("Play/Pause", "win.toggle-play", None));
    play.append_item(&item("Seek 10s Backward", "win.seek-back-10", None));
    play.append_item(&item("Seek 10s Forward", "win.seek-forward-10", None));
    menu.append_section(None, &play);

    let open = gio::Menu::new();
    open.append_item(&item("Open Files", "win.open-files", Some("<Ctrl>O")));
    open.append_item(&item("Open URL", "win.open-url", Some("<Ctrl>U")));
    menu.append_section(None, &open);

    let menus = gio::Menu::new();
    menus.append_item(&item("Subtitles Menu", "win.show-subtitles-menu", Some("<Ctrl>S")));
    menus.append_item(&item("Audio Menu", "win.show-audio-menu", Some("<Ctrl>A")));
    menus.append_item(&item("Playlist", "win.show-playlist", None));
    menu.append_section(None, &menus);

    let window = gio::Menu::new();
    window.append_item(&item("Fullscreen", "win.toggle-fullscreen", Some("F11")));
    menu.append_section(None, &window);

    menu
}
