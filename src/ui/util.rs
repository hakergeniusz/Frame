//! Small shared widgets & formatting helpers.

use gtk::prelude::*;
use gtk::{gdk, gio, glib};

/// Format seconds as `m:ss` (or `h:mm:ss` above one hour) — docs/03.
pub fn format_time(t: f64) -> String {
    let t = t.max(0.0);
    let total = t.round() as i64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Honor the app.quit accelerator on a plain `adw::Window`. GTK4 only wires
/// `set_accels_for_action` into GtkApplicationWindow, and a NamedAction here
/// would not even resolve — plain windows never join the application's
/// action muxer — so the shortcut carries a callback that quits directly.
pub fn install_quit_accel<W: IsA<gtk::Window> + IsA<gtk::Widget>>(window: &W) {
    let controller = gtk::ShortcutController::new();
    if let Some(trigger) = gtk::ShortcutTrigger::parse_string("<Primary>q") {
        let app = gio::Application::default();
        controller.add_shortcut(gtk::Shortcut::new(
            Some(trigger),
            Some(
                gtk::CallbackAction::new(move |_, _| {
                    if let Some(app) = app.as_ref() {
                        app.quit();
                    }
                    glib::Propagation::Stop
                })
                .upcast::<gtk::ShortcutAction>(),
            ),
        ));
    }
    window.add_controller(controller);
}

/// Attach a bottom-center toast (docs/03: dark pill, ✕ dismiss, timeout).
pub fn toast(overlay: &adw::ToastOverlay, title: &str) {
    let t = adw::Toast::builder().title(title).timeout(5).build();
    overlay.add_toast(t);
}

/// Toast with a trailing action button (used for history clear-undo).
pub fn toast_with_button(overlay: &adw::ToastOverlay, title: &str, button_label: &str, on_click: impl Fn(&adw::Toast) + 'static) {
    let t = adw::Toast::builder()
        .title(title)
        .button_label(button_label)
        .timeout(10)
        .build();
    t.connect_button_clicked(on_click);
    overlay.add_toast(t);
}

fn menu_item(label: &str, action: &str, accel: Option<&str>) -> gio::MenuItem {
    let item = gio::MenuItem::new(Some(label), Some(action));
    if let Some(a) = accel {
        item.set_attribute_value("accel", Some(&a.to_variant()));
    }
    item
}

/// The primary application menu (docs/02): general section + session section.
/// "About Frame" is re-authored per the clean wall.
pub fn primary_menu() -> gio::Menu {
    let menu = gio::Menu::new();

    let general = gio::Menu::new();
    general.append_item(&menu_item("New Window", "app.new-window", Some("<Ctrl>N")));
    general.append_item(&menu_item("Preferences", "app.preferences", Some("<Ctrl>comma")));
    general.append_item(&menu_item("Keyboard Shortcuts", "app.shortcuts", Some("<Ctrl>question")));
    general.append_item(&menu_item("About Frame", "app.about", None));
    menu.append_section(None, &general);

    let session = gio::Menu::new();
    session.append_item(&menu_item("Save Session", "win.save-session", Some("<Shift><Ctrl>S")));
    session.append_item(&menu_item("Save Session and Close", "win.save-session-close", Some("<Shift>Q")));
    menu.append_section(None, &session);

    menu
}

/// Small rounded pill label used for the seek-bar hover timestamp bubble.
pub fn hover_bubble_label() -> gtk::Label {
    let label = gtk::Label::new(None);
    label.add_css_class("osd");
    label.add_css_class("caption");
    label.set_halign(gtk::Align::Start);
    label.set_valign(gtk::Align::Start);
    label.set_margin_top(6);
    label.set_margin_bottom(6);
    label.set_margin_start(10);
    label.set_margin_end(10);
    label
}

/// Generate a small checkerboard texture (subtitle-background preview).
pub fn checkerboard_texture() -> gdk::Texture {
    const N: usize = 16; // 16×16 px, 4 px squares
    let mut data = vec![0u8; N * N * 4];
    for y in 0..N {
        for x in 0..N {
            let on = ((x / 4) + (y / 4)) % 2 == 0;
            let i = (y * N + x) * 4;
            let (r, g, b, a) = if on { (120, 120, 120, 255) } else { (200, 200, 200, 255) };
            data[i] = r;
            data[i + 1] = g;
            data[i + 2] = b;
            data[i + 3] = a;
        }
    }
    let bytes = glib::Bytes::from(&data);
    gdk::MemoryTexture::new(N as i32, N as i32, gdk::MemoryFormat::R8g8b8a8, &bytes, (N * 4) as usize).upcast()
}

/// A suffix button showing an ⓘ whose click reveals `text` in a popover.
pub fn info_button(text: &str) -> gtk::MenuButton {
    let label = gtk::Label::builder()
        .label(text)
        .wrap(true)
        .max_width_chars(40)
        .build();
    let popover = gtk::Popover::builder().child(&label).build();
    let btn = gtk::MenuButton::builder()
        .icon_name("dialog-information-symbolic")
        .popover(&popover)
        .css_classes(["flat"])
        .tooltip_text(text)
        .build();
    btn
}

/// The window-icon used on the welcome screen.
pub fn app_icon_image() -> gtk::Image {
    gtk::Image::builder()
        .icon_name(crate::config::APP_ID)
        .pixel_size(128)
        .build()
}
