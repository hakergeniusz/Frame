//! Watch History popover (docs/02): red-toned clear-all icon, title, ✕;
//! date-group headers in locale format; rows with the media name (full URL
//! for URL sources, file name without extension for local files) and a
//! per-row ⊗ remove button; clicking a row re-opens the media.

use crate::engine::{display_title, MediaSource};
use crate::history::{group_by_date, HistoryEntry, HistoryStore};
use gtk::prelude::*;
use gtk::{glib, Align};
use std::rc::Rc;

/// Build the popover. Its content refreshes from the store every time the
/// popover is shown.
pub fn build_history_popover(
    store: &HistoryStore,
    toast_overlay: &adw::ToastOverlay,
    on_open: impl Fn(MediaSource) + 'static,
) -> gtk::Popover {
    let popover = gtk::Popover::new();

    let refresh: Rc<dyn Fn()> = build_content(
        &popover,
        store,
        toast_overlay,
        Rc::new(on_open),
    );
    refresh();

    let refresh_show = refresh.clone();
    popover.connect_show(move |_| refresh_show());

    popover
}

fn build_content(
    popover: &gtk::Popover,
    store: &HistoryStore,
    toast_overlay: &adw::ToastOverlay,
    on_open: Rc<dyn Fn(MediaSource)>,
) -> Rc<dyn Fn()> {
    let container = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .width_request(380)
        .height_request(220)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();

    // Header: clear-all (red-toned), title, close ✕.
    let clear_btn = gtk::Button::builder()
        .icon_name("edit-clear-all-symbolic")
        .css_classes(["flat", "error"])
        .tooltip_text("Clear Watch History")
        .build();
    let title = gtk::Label::builder()
        .label("Watch History")
        .css_classes(["heading"])
        .build();
    let close_btn = gtk::Button::builder()
        .icon_name("window-close-symbolic")
        .css_classes(["flat"])
        .halign(Align::End)
        .hexpand(true)
        .build();
    let header = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .build();
    header.append(&clear_btn);
    header.append(&title);
    header.append(&close_btn);

    let list_box = gtk::ListBox::builder()
        .css_classes(["boxed-list"])
        .selection_mode(gtk::SelectionMode::None)
        .build();
    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .child(&list_box)
        .vexpand(true)
        .height_request(280)
        .build();
    container.append(&header);
    container.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    container.append(&scrolled);

    popover.set_child(Some(&container));

    let refresh: Rc<dyn Fn()> = {
        let list_box = list_box.clone();
        let store = store.clone();
        let on_open = on_open.clone();
        Rc::new(move || {
            while let Some(row) = list_box.row_at_index(0) {
                list_box.remove(&row);
            }
            let entries = store.entries();
            if entries.is_empty() {
                let empty = gtk::Label::builder()
                    .label("No watch history yet")
                    .css_classes(["dim-label"])
                    .margin_top(18)
                    .margin_bottom(18)
                    .build();
                list_box.append(
                    &gtk::ListBoxRow::builder()
                        .child(&empty)
                        .selectable(false)
                        .activatable(false)
                        .build(),
                );
                return;
            }
            for group in group_by_date(&entries) {
                let header_label = gtk::Label::builder()
                    .label(&group.label)
                    .halign(Align::Start)
                    .css_classes(["heading"])
                    .margin_top(10)
                    .margin_start(6)
                    .build();
                list_box.append(
                    &gtk::ListBoxRow::builder()
                        .child(&header_label)
                        .selectable(false)
                        .activatable(false)
                        .build(),
                );

                for entry in group.entries {
                    let name = display_title(&entry.source);
                    let label = gtk::Label::builder()
                        .label(&name)
                        .halign(Align::Start)
                        .ellipsize(gtk::pango::EllipsizeMode::End)
                        .hexpand(true)
                        .build();
                    let remove_btn = gtk::Button::builder()
                        .icon_name("window-close-symbolic")
                        .css_classes(["flat"])
                        .tooltip_text("Remove entry")
                        .valign(Align::Center)
                        .build();
                    let row_box = gtk::Box::builder()
                        .orientation(gtk::Orientation::Horizontal)
                        .spacing(8)
                        .margin_top(4)
                        .margin_bottom(4)
                        .margin_start(10)
                        .margin_end(6)
                        .build();
                    row_box.append(&label);
                    row_box.append(&remove_btn);
                    let row = gtk::ListBoxRow::builder().child(&row_box).build();

                    let entry_for_remove = entry.clone();
                    let store_remove = store.clone();
                    remove_btn.connect_clicked(move |_| {
                        store_remove.remove(&entry_for_remove);
                    });
                    let entry_for_open = entry.clone();
                    let cb = on_open.clone();
                    row.connect_activate(move |_| {
                        cb(entry_for_open.source.clone());
                    });
                    list_box.append(&row);
                }
            }
        })
    };

    // Clear-all: unobserved in the original (docs/02, docs/06); recorded
    // assumption — immediate clear with an Undo toast.
    clear_btn.connect_clicked(glib::clone!(
        #[strong]
        store,
        #[strong]
        toast_overlay,
        move |_| {
            let removed = store.entries();
            if removed.is_empty() {
                return;
            }
            store.clear();
            crate::ui::util::toast_with_button(
                &toast_overlay,
                "Watch History cleared",
                "Undo",
                glib::clone!(
                    #[strong]
                    store,
                    move |_| {
                        for e in removed.iter().rev() {
                            store.add(&e.source);
                        }
                    }
                ),
            );
        }
    ));
    close_btn.connect_clicked(glib::clone!(#[strong] popover, move |_| popover.popdown()));

    refresh
}
