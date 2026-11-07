//! Control-bar popovers (docs/03 + docs/07): volume, subtitles, audio,
//! chapters, playlist, settings.
//!
//! Fidelity notes: docs/03 observed the Subtitles popover as a radio list
//! with `None`; the owner's 2026-11-04 reference screenshots settled the
//! exact chrome — bold `Add … Track` title (no pill), plain rows with round
//! radio indicators (no boxed list, no section header). The others mirror
//! that shape (docs/03 "inferred to mirror"); playlist rows additionally
//! offer per-item removal (assumption, docs/06/07).

use super::PlayerWindowCore;
use crate::engine::{MediaSource, TrackInfo, TrackKind};
use adw::prelude::*;
use gtk::prelude::*;
use gtk::{glib, Align, Orientation};
use std::cell::Cell;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Volume
// ---------------------------------------------------------------------------

pub struct VolumePopover {
    pub popover: gtk::Popover,
    scale: gtk::Scale,
    mute: gtk::ToggleButton,
    dragging: Rc<Cell<bool>>,
}

impl VolumePopover {
    pub fn new(core: &Rc<PlayerWindowCore>) -> Self {
        let dragging: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let popover = gtk::Popover::builder().autohide(true).build();
        let box_ = gtk::Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .margin_top(10)
            .margin_bottom(10)
            .margin_start(10)
            .margin_end(10)
            .build();
        let mute = gtk::ToggleButton::builder()
            .icon_name("audio-volume-muted-symbolic")
            .css_classes(["flat"])
            .build();
        let scale = gtk::Scale::builder()
            .orientation(Orientation::Horizontal)
            .hexpand(true)
            .width_request(180)
            .draw_value(false)
            .build();
        scale.set_range(0.0, 100.0);
        scale.set_value(100.0);
        box_.append(&mute);
        box_.append(&scale);
        popover.set_child(Some(&box_));

        scale.connect_value_changed(glib::clone!(#[strong] core, #[strong] dragging, move |s| {
            if !dragging.get() {
                core.player.set_volume(s.value());
            }
        }));
        // Track drags so engine-driven updates don't fight the user's drag.
        let press = gtk::GestureClick::new();
        press.set_button(0);
        press.connect_pressed(glib::clone!(#[strong] dragging, move |_, _, _, _| dragging.set(true)));
        press.connect_released(glib::clone!(#[strong] dragging, move |_, _, _, _| {
            dragging.set(false);
        }));
        scale.add_controller(press);

        mute.connect_toggled(glib::clone!(#[strong] core, move |btn| {
            core.player.set_mute(btn.is_active());
        }));

        Self { popover, scale, mute, dragging }
    }

    pub fn update(&self, core: &PlayerWindowCore) {
        let volume = core.player.get_double("volume").unwrap_or(100.0);
        let muted = core.player.get_flag("mute").unwrap_or(false);
        if !self.dragging.get() {
            self.scale.set_value(volume.min(100.0));
        }
        self.mute.set_active(muted);
    }
}

// ---------------------------------------------------------------------------
// Subtitles & audio (same shape per docs/03)
// ---------------------------------------------------------------------------

pub struct TrackPopover {
    pub popover: gtk::Popover,
    list: gtk::ListBox,
    kind: TrackKind,
}

impl TrackPopover {
    /// The track pickers follow the owner's reference chrome: a bold
    /// `Add … Track` title (still opening the portal chooser) above a plain
    /// radio list — no pill button, no boxed list, no section header.
    pub fn new(core: &Rc<PlayerWindowCore>, kind: TrackKind) -> Self {
        // `track-popover` lets the stylesheet melt the radio rows into the
        // popover background (the list node's default view background reads
        // as a mismatched darker panel otherwise).
        let popover = gtk::Popover::builder()
            .autohide(true)
            .css_classes(["track-popover"])
            .build();
        let box_ = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(2)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(6)
            .margin_end(6)
            .width_request(300)
            .build();

        let add_btn = gtk::Button::builder()
            .label(if kind == TrackKind::Sub { "Add Subtitle Track" } else { "Add Audio Track" })
            .css_classes(["flat", "heading"])
            .halign(Align::Start)
            .build();
        add_btn.connect_clicked(glib::clone!(#[strong] core, #[strong] popover, move |_| {
            popover.popdown();
            core.request_add_external_track(kind);
        }));

        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .build();

        box_.append(&add_btn);
        box_.append(&list);
        popover.set_child(Some(&box_));

        Self { popover, list, kind }
    }

    /// Rebuild the radio list. `current` is the active track id (0 = None).
    pub fn update(&self, tracks: &[TrackInfo], current: i64, core: &Rc<PlayerWindowCore>) {
        while let Some(row) = self.list.row_at_index(0) {
            self.list.remove(&row);
        }
        let relevant: Vec<&TrackInfo> = tracks.iter().filter(|t| t.kind == self.kind).collect();

        // "None" row (observed in the Subtitles popover), then the tracks —
        // all chained into one radio group so the indicators read as
        // (•)/( ) rows.
        let mut group: Option<gtk::CheckButton> = None;
        let (row, radio) = track_row("None", 0, current == 0, group.as_ref(), core, self.kind);
        self.list.append(&row);
        group = Some(radio);
        for t in relevant {
            let (row, radio) =
                track_row(&t.picker_label(), t.id, t.id == current, group.as_ref(), core, self.kind);
            self.list.append(&row);
            group = Some(radio);
        }
    }
}

/// GTK4 emits a ListBoxRow's ::activate on Enter/double-click only — make a
/// single click activate too (docs/03 popovers).
fn row_click_activate(
    row: &gtk::ListBoxRow,
    core: &Rc<PlayerWindowCore>,
    activate: impl Fn(&Rc<PlayerWindowCore>) + 'static,
) {
    let click = gtk::GestureClick::new();
    let core = core.clone();
    click.connect_released(move |_, _, _, _| activate(&core));
    row.add_controller(click);
}

/// One radio row of the track popovers (docs/03: `(•) None` radio list;
/// owner's reference: round indicators, flat rows). Returns the row and its
/// radio so the caller can chain the group.
fn track_row(
    label: &str,
    id: i64,
    selected: bool,
    group: Option<&gtk::CheckButton>,
    core: &Rc<PlayerWindowCore>,
    kind: TrackKind,
) -> (gtk::ListBoxRow, gtk::CheckButton) {
    let radio = gtk::CheckButton::builder().active(selected).build();
    if let Some(g) = group {
        radio.set_group(Some(g));
    }
    let label = gtk::Label::builder().label(label).halign(Align::Start).hexpand(true).build();
    let box_ = gtk::Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(10)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(10)
        .margin_end(10)
        .build();
    box_.append(&radio);
    box_.append(&label);
    let row = gtk::ListBoxRow::builder().child(&box_).activatable(true).build();

    // Single selection: every activation path funnels through
    // `radio.set_active(true)` so the indicator and the engine can never
    // disagree; the `toggled` handler then selects (ignoring the
    // deactivation toggled on the previously active radio of the group).
    let click_radio = radio.clone();
    row_click_activate(&row, core, move |_| click_radio.set_active(true));

    let activate_radio = radio.clone();
    row.connect_activate(move |_| activate_radio.set_active(true));

    let core = core.clone();
    radio.connect_toggled(move |r| {
        if r.is_active() {
            match kind {
                TrackKind::Sub => core.player.set_subtitle_track(id),
                TrackKind::Audio => core.player.set_audio_track(id),
                TrackKind::Video => {}
            }
        }
    });
    (row, radio)
}

// ---------------------------------------------------------------------------
// Chapters
// ---------------------------------------------------------------------------

pub struct ChaptersPopover {
    pub popover: gtk::Popover,
    list: gtk::ListBox,
}

impl ChaptersPopover {
    pub fn new(core: &Rc<PlayerWindowCore>) -> Self {
        let popover = gtk::Popover::builder().autohide(true).build();
        let list = gtk::ListBox::builder()
            .css_classes(["boxed-list"])
            .selection_mode(gtk::SelectionMode::None)
            .width_request(300)
            .build();
        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .max_content_height(360)
            .propagate_natural_height(true)
            .child(&list)
            .build();
        let box_ = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(8)
            .margin_end(8)
            .build();
        box_.append(&scrolled);
        popover.set_child(Some(&box_));
        Self { popover, list }
    }

    pub fn update(&self, chapters: &[crate::engine::ChapterInfo], core: &Rc<PlayerWindowCore>) {
        while let Some(row) = self.list.row_at_index(0) {
            self.list.remove(&row);
        }
        if chapters.is_empty() {
            let empty = gtk::Label::builder()
                .label("No chapters")
                .css_classes(["dim-label"])
                .margin_top(12)
                .margin_bottom(12)
                .build();
            self.list.append(
                &gtk::ListBoxRow::builder().child(&empty).selectable(false).activatable(false).build(),
            );
            return;
        }
        for (i, ch) in chapters.iter().enumerate() {
            let title = ch.title.clone().unwrap_or_else(|| format!("Chapter {}", i + 1));
            let label = gtk::Label::builder()
                .label(format!("{}  ·  {}", title, crate::ui::util::format_time(ch.time)))
                .halign(Align::Start)
                .hexpand(true)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            let row = gtk::ListBoxRow::builder().child(&label).activatable(true).build();
            let core = core.clone();
            let idx = i as i64;
            row_click_activate(&row, &core, move |core| {
                core.player.set_chapter(idx);
            });
            row.connect_activate(move |_| {
                core.player.set_chapter(idx);
            });
            self.list.append(&row);
        }
    }
}

// ---------------------------------------------------------------------------
// Playlist panel
// ---------------------------------------------------------------------------

pub struct PlaylistPopover {
    pub popover: gtk::Popover,
    list: gtk::ListBox,
}

impl PlaylistPopover {
    pub fn new(core: &Rc<PlayerWindowCore>) -> Self {
        let popover = gtk::Popover::builder().autohide(true).build();
        let list = gtk::ListBox::builder()
            .css_classes(["boxed-list"])
            .selection_mode(gtk::SelectionMode::None)
            .width_request(340)
            .build();
        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .max_content_height(380)
            .propagate_natural_height(true)
            .child(&list)
            .build();
        let box_ = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(8)
            .margin_end(8)
            .build();
        box_.append(&scrolled);
        popover.set_child(Some(&box_));
        Self { popover, list }
    }

    pub fn update(&self, core: &Rc<PlayerWindowCore>) {
        while let Some(row) = self.list.row_at_index(0) {
            self.list.remove(&row);
        }
        let current = core.playlist_current_index();
        let items = core.playlist_item_sources();
        if items.is_empty() {
            let empty = gtk::Label::builder()
                .label("Playlist is empty")
                .css_classes(["dim-label"])
                .margin_top(12)
                .margin_bottom(12)
                .build();
            self.list.append(
                &gtk::ListBoxRow::builder().child(&empty).selectable(false).activatable(false).build(),
            );
            return;
        }
        for (i, source) in items.iter().enumerate() {
            let is_current = current == Some(i);
            let name = crate::engine::display_title(source);
            let label = gtk::Label::builder()
                .label(if is_current { format!("▶ {name}") } else { name })
                .halign(Align::Start)
                .hexpand(true)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .css_classes(if is_current { vec!["heading".to_string()] } else { vec![] })
                .build();
            let remove_btn = gtk::Button::builder()
                .icon_name("window-close-symbolic")
                .css_classes(["flat"])
                .valign(Align::Center)
                .tooltip_text("Remove from playlist")
                .build();
            let box_ = gtk::Box::builder()
                .orientation(Orientation::Horizontal)
                .spacing(6)
                .margin_top(4)
                .margin_bottom(4)
                .margin_start(10)
                .margin_end(6)
                .build();
            box_.append(&label);
            box_.append(&remove_btn);
            let row = gtk::ListBoxRow::builder().child(&box_).activatable(true).build();

            // Clicking the row plays that item.
            let core_play = core.clone();
            let idx = i;
            row_click_activate(&row, &core_play, move |core| {
                core.play_index(idx);
            });
            row.connect_activate(move |_| {
                core_play.play_index(idx);
            });
            // The remove button must not also trigger row activation.
            let core_remove = core.clone();
            remove_btn.connect_clicked(move |_| {
                core_remove.remove_index(idx);
            });
            self.list.append(&row);
        }
    }
}

// ---------------------------------------------------------------------------
// Settings (⚙) — speed + loop (docs/03: "speed/loop candidates")
// ---------------------------------------------------------------------------

pub struct SettingsPopover {
    pub popover: gtk::Popover,
    speed_label: gtk::Label,
    loop_switch: gtk::Switch,
}

impl SettingsPopover {
    pub fn new(core: &Rc<PlayerWindowCore>) -> Self {
        let popover = gtk::Popover::builder().autohide(true).build();
        let list = gtk::ListBox::builder()
            .css_classes(["boxed-list"])
            .selection_mode(gtk::SelectionMode::None)
            .width_request(300)
            .build();

        // Speed row: − value +
        let speed_label = gtk::Label::new(Some("1.00×"));
        speed_label.set_width_chars(7);
        let minus = gtk::Button::builder()
            .icon_name("list-remove-symbolic")
            .css_classes(["flat"])
            .valign(Align::Center)
            .build();
        let plus = gtk::Button::builder()
            .icon_name("list-add-symbolic")
            .css_classes(["flat"])
            .valign(Align::Center)
            .build();
        let reset = gtk::Button::builder()
            .icon_name("edit-undo-symbolic")
            .css_classes(["flat"])
            .valign(Align::Center)
            .tooltip_text("Reset Playback Speed")
            .build();
        let speed_box = gtk::Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(4)
            .halign(Align::End)
            .valign(Align::Center)
            .build();
        speed_box.append(&minus);
        speed_box.append(&speed_label);
        speed_box.append(&plus);
        speed_box.append(&reset);
        let speed_row = adw::ActionRow::builder().title("Playback Speed").build();
        speed_row.add_suffix(&speed_box);

        minus.connect_clicked(glib::clone!(#[strong] core, move |_| {
            core.player.set_speed(core.player.speed() / 1.1);
        }));
        plus.connect_clicked(glib::clone!(#[strong] core, move |_| {
            core.player.set_speed(core.player.speed() * 1.1);
        }));
        reset.connect_clicked(glib::clone!(#[strong] core, move |_| {
            core.player.set_speed(1.0);
        }));

        // Loop file row.
        let loop_switch = gtk::Switch::builder().valign(Align::Center).build();
        let loop_row = adw::ActionRow::builder().title("Loop File").build();
        loop_row.add_suffix(&loop_switch);
        loop_switch.connect_state_set(glib::clone!(#[strong] core, move |_, state| {
            core.player.set_loop_file(state);
            glib::Propagation::Proceed
        }));

        list.append(&speed_row);
        list.append(&loop_row);

        let box_ = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(8)
            .margin_end(8)
            .build();
        box_.append(&list);
        popover.set_child(Some(&box_));

        Self { popover, speed_label, loop_switch }
    }

    pub fn update(&self, core: &PlayerWindowCore) {
        self.speed_label.set_text(&format!("{:.2}×", core.player.speed()));
        self.loop_switch.set_active(core.player.loop_file());
    }
}
