//! Keyboard Shortcuts window (docs/04 + docs/05).
//!
//! GNOME-standard layout: search field on top, rounded category chips that
//! jump to their section, and one scrollable list of `action … keycaps`
//! rows. GtkShortcutsWindow paginates sections instead of showing chips, so
//! Frame builds the documented layout directly.

use adw::prelude::*;
use gtk::prelude::*;
use gtk::{glib, Align, Orientable, Orientation};

/// (category, [(action, accelerator-display)])
/// Accelerator strings use gtk_accelerator_parse syntax, with " / " separating
/// alternative bindings; ShortcutLabel renders each parsed key.
const SECTIONS: &[(&str, &[(&str, &str)])] = &[
    (
        "General",
        &[
            ("New Window", "<Ctrl>N"),
            ("Open Files", "<Ctrl>O"),
            ("Open Folder", "<Ctrl>I"),
            ("Open URL", "<Ctrl>U"),
            ("Add Files to Playlist", "<Ctrl><Shift>O"),
            ("Add Folder to Playlist", "<Ctrl><Shift>I"),
            ("Add URL to Playlist", "<Ctrl><Shift>U"),
            ("Open Subtitles Menu", "<Ctrl>S"),
            ("Open Audio Menu", "<Ctrl>A"),
            ("Open Chapters Menu", "<Ctrl>C"),
            ("Watch History", "<Ctrl>H"),
            ("Show Shortcuts", "<Ctrl>question"),
            ("Close Window", "Q / <Ctrl>W"),
            ("Save Session", "<Ctrl><Shift>S"),
            ("Save Session and Close", "<Shift>Q"),
            ("Quit", "<Ctrl>Q"),
        ],
    ),
    (
        "Subtitles",
        &[
            ("Show/Hide Subtitles", "C"),
            ("Next Subtitle Track", "Z"),
            ("Previous Subtitle Track", "<Shift>Z"),
            ("Next Secondary Subtitle Track", "<Alt>Z"),
            ("Previous Secondary Subtitle Track", "<Ctrl>Z"),
            ("Decrease Subtitle Delay", "comma"),
            ("Increase Subtitle Delay", "period"),
            ("Move Subtitles Up", "Page_Up"),
            ("Move Subtitles Down", "Page_Down"),
            ("Increase Subtitle Scale", "<Shift>G"),
            ("Decrease Subtitle Scale", "<Shift>F"),
        ],
    ),
    (
        "Audio & Volume",
        &[
            ("Volume Increase", "Up"),
            ("Volume Decrease", "Down"),
            ("Next Audio Track", "A"),
            ("Previous Audio Track", "<Shift>A"),
            ("Mute/Unmute", "M"),
            ("Decrease Audio Delay", "<Ctrl>minus"),
            ("Increase Audio Delay", "<Ctrl>equal / <Ctrl>plus"),
        ],
    ),
    (
        "Navigation",
        &[
            ("Seek 10s Backward", "J"),
            ("Seek 10s Forward", "L"),
            ("Seek 5s Backward", "Left"),
            ("Seek 5s Forward", "Right"),
            ("Go Back One Frame", "<Ctrl>bracketleft"),
            ("Advance One Frame", "<Ctrl>bracketright"),
            ("Seek to the Previous Chapter", "<Ctrl>Left"),
            ("Seek to the Next Chapter", "<Ctrl>Right"),
        ],
    ),
    (
        "Display & Video",
        &[
            ("Fullscreen", "F11 / F"),
            ("Exit Fullscreen", "Escape"),
            ("Zoom In", "equal / plus"),
            ("Zoom Out", "minus"),
            ("Decrease Contrast", "1"),
            ("Increase Contrast", "2"),
            ("Decrease Brightness", "3"),
            ("Increase Brightness", "4"),
            ("Decrease Gamma", "5"),
            ("Increase Gamma", "6"),
            ("Decrease Saturation", "7"),
            ("Increase Saturation", "8"),
        ],
    ),
    (
        "Playback",
        &[
            ("Play/Pause", "K / P / space"),
            ("Set/Clear A-B Loop Points", "<Ctrl>L"),
            ("Loop File", "<Shift>L"),
            ("Decrease Playback Speed", "bracketleft"),
            ("Increase Playback Speed", "bracketright"),
            ("Reset Playback Speed", "BackSpace"),
        ],
    ),
    (
        "Miscellaneous",
        &[
            ("Take Screenshot With Subtitles", "S"),
            ("Take Screenshot Without Subtitles", "<Shift>S"),
            ("Statistics", "I"),
            ("Statistics Overlay", "<Shift>I"),
        ],
    ),
];

/// One accelerator spec (single binding, gtk_accelerator_parse syntax) as a
/// key-cap label.
fn accel_label(spec: &str) -> gtk::ShortcutLabel {
    gtk::ShortcutLabel::builder().accelerator(spec).build()
}

/// A " / "-separated group of bindings ("K / P / space"), rendered as
/// key-cap labels separated by slashes, matching the documented presentation.
fn accel_group(spec: &str) -> gtk::Widget {
    let box_ = gtk::Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(4)
        .build();
    for (i, part) in spec.split(" / ").enumerate() {
        if i > 0 {
            box_.append(&gtk::Label::new(Some("/")));
        }
        box_.append(&accel_label(part));
    }
    box_.upcast()
}

pub fn show_shortcuts(parent: &impl IsA<gtk::Window>) {
    let window = adw::Window::builder()
        .title("Keyboard Shortcuts")
        .transient_for(parent)
        .modal(false)
        .default_width(640)
        .default_height(700)
        .build();

    let root = gtk::Box::builder()
        .orientation(Orientation::Vertical)
        .spacing(12)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();

    // Search field with leading magnifier icon.
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search shortcuts"));
    root.append(&search);

    // Category chips.
    let chips = gtk::Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .halign(Align::Start)
        .build();
    root.append(&chips);

    // Body: all sections in one scrollable list.
    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();
    let body = gtk::Box::builder().orientation(Orientation::Vertical).spacing(18).build();
    scrolled.set_child(Some(&body));
    root.append(&scrolled);

    let mut chip_buttons: Vec<(String, gtk::Button)> = Vec::new();
    for (category, rows) in SECTIONS {
        let heading = gtk::Label::builder()
            .label(*category)
            .halign(Align::Start)
            .css_classes(["title-3"])
            .build();
        body.append(&heading);

        let list = gtk::ListBox::builder()
            .css_classes(["boxed-list"])
            .selection_mode(gtk::SelectionMode::None)
            .build();
        for (title, accel) in *rows {
            let title_label = gtk::Label::builder().label(*title).halign(Align::Start).hexpand(true).build();
            let keys = accel_group(accel);
            let row_box = gtk::Box::builder()
                .orientation(Orientation::Horizontal)
                .spacing(12)
                .margin_top(8)
                .margin_bottom(8)
                .margin_start(12)
                .margin_end(12)
                .build();
            row_box.append(&title_label);
            row_box.append(&keys);
            list.append(&gtk::ListBoxRow::builder().child(&row_box).selectable(false).activatable(false).build());
        }
        body.append(&list);

        let chip = gtk::Button::builder().label(*category).css_classes(["pill", "flat"]).build();
        chip.connect_clicked(glib::clone!(#[strong] scrolled, #[strong] heading, move |_| {
            // Scroll the section heading into view.
            let adj = scrolled.vadjustment();
            let allocation_top = compute_offset(&scrolled, &heading);
            adj.set_value(allocation_top - 12.0);
        }));
        chip_buttons.push(((*category).to_string(), chip));
    }
    for (_, chip) in &chip_buttons {
        chips.append(chip);
    }

    // Search: filter shortcut rows by title.
    search.connect_search_changed(glib::clone!(#[strong] body, move |entry| {
        let query = entry.text().to_lowercase();
        for row in body.observe_children().iter::<gtk::Widget>().flatten() {
            if let Ok(list) = row.downcast::<gtk::ListBox>() {
                let mut i = 0;
                while let Some(row) = list.row_at_index(i) {
                    let visible = query.is_empty()
                        || row
                            .first_child()
                            .and_then(|w| w.first_child())
                            .and_then(|w| w.downcast::<gtk::Label>().ok())
                            .map(|l| l.text().to_lowercase().contains(&query))
                            .unwrap_or(false);
                    row.set_visible(visible);
                    i += 1;
                }
            }
        }
    }));

    // AdwWindow requires content, not set_child (which aborts).
    window.set_content(Some(&root));
    crate::ui::util::install_quit_accel(&window);
    window.present();
}

/// Vertical offset of a widget inside a scrolled window's content.
fn compute_offset(scrolled: &gtk::ScrolledWindow, target: &impl IsA<gtk::Widget>) -> f64 {
    let mut offset = 0.0;
    let mut widget = target.upcast_ref::<gtk::Widget>().clone();
    loop {
        let Some(parent) = widget.parent() else { break };
        if parent.is::<gtk::ScrolledWindow>() {
            break;
        }
        let alloc = widget.allocation();
        offset += alloc.y() as f64;
        widget = parent;
        let _ = scrolled;
    }
    offset
}
