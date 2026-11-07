//! Open URL / Add URL modal dialogs (docs/04): a small modal with a single
//! floating-label `URL` entry and a pill action button (`Open` / `Add`)
//! disabled while the entry is empty.

use adw::prelude::*;
use gtk::prelude::*;
use gtk::glib;

/// Show the dialog. `add` selects the Add variant; `on_confirm` receives the
/// entered URL when the action button is pressed.
pub fn show_url_dialog<W: IsA<gtk::Widget>>(
    parent: &W,
    add: bool,
    on_confirm: impl Fn(String) + 'static,
) {
    let title = if add { "Add URL" } else { "Open URL" };
    let dialog = adw::Dialog::builder()
        .title(title)
        .content_width(360)
        .content_height(140)
        .build();

    let entry = adw::EntryRow::builder().title("URL").build();
    entry.add_css_class("card");

    let action_label = if add { "Add" } else { "Open" };
    let button = gtk::Button::builder()
        .label(action_label)
        .css_classes(["pill", "suggested-action"])
        .halign(gtk::Align::Center)
        .margin_top(12)
        // Disabled while the entry is empty (observed on Add URL; Open
        // presumably matches — docs/04).
        .sensitive(false)
        .build();

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .spacing(6)
        .valign(gtk::Align::Start)
        .build();
    // AdwEntryRow may only be placed in a GtkListBox (boxed-list style).
    let entry_list = gtk::ListBox::builder()
        .css_classes(["boxed-list"])
        .selection_mode(gtk::SelectionMode::None)
        .build();
    entry_list.append(&entry);
    content.append(&entry_list);
    content.append(&button);

    dialog.set_child(Some(&content));
    dialog.present(Some(parent));

    let enable = glib::clone!(#[strong] entry, #[strong] button, move |_: &adw::EntryRow| {
        button.set_sensitive(!entry.text().trim().is_empty());
    });
    entry.connect_changed(enable);

    let on_confirm = std::cell::RefCell::new(Some(on_confirm));
    button.connect_clicked(glib::clone!(
        #[strong(rename_to = dialog)]
        dialog,
        #[strong]
        entry,
        move |_| {
            let url = entry.text().trim().to_string();
            if url.is_empty() {
                return;
            }
            dialog.force_close();
            if let Some(cb) = on_confirm.borrow_mut().take() {
                cb(url);
            }
        }
    ));
    entry.connect_entry_activated(glib::clone!(
        #[strong]
        button,
        move |_| {
            let _ = button.activate();
        }
    ));
}

pub fn show_open_url_dialog<W: IsA<gtk::Widget>>(parent: &W, on_confirm: impl Fn(String) + 'static) {
    show_url_dialog(parent, false, on_confirm);
}

pub fn show_add_url_dialog<W: IsA<gtk::Widget>>(parent: &W, on_confirm: impl Fn(String) + 'static) {
    show_url_dialog(parent, true, on_confirm);
}
