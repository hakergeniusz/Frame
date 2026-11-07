//! Preferences window (docs/04): a single scrollable page ≈520 px wide with
//! a warning-style ⓘ icon in the header (left), close ✕ (right), rows
//! grouped into rounded card groups, and the full-width `Open Config Folder >`
//! footer button.

use adw::prelude::*;

use crate::config::{mpv_color, PrimaryClickAction, SecondaryClickAction, Settings};
use crate::rt::AppRuntime;
use gtk::prelude::*;
use gtk::{gdk, glib, Align, Orientation};
use std::sync::Arc;

pub fn show_preferences(rt: &Arc<AppRuntime>, parent: &impl IsA<gtk::Window>) {
    let window = adw::Window::builder()
        .title("Preferences")
        .transient_for(parent)
        .modal(false)
        .default_width(520)
        .default_height(640)
        .build();

    let header = adw::HeaderBar::new();
    // Warning-style ⓘ icon at the left of the header (docs/04).
    let info_icon = gtk::Image::builder()
        .icon_name("dialog-information-symbolic")
        .css_classes(["warning"])
        .build();
    header.pack_start(&info_icon);

    let page = adw::PreferencesPage::builder().build();

    build_behavior_group(&page, rt);
    build_performance_group(&page, rt);
    build_session_group(&page, rt);
    build_subtitles_group(&page, rt);
    build_mouse_group(&page, rt);
    build_language_group(&page, rt);
    build_footer(&page, rt);

    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&page));
    // AdwWindow only accepts a ToolbarView via its content property;
    // gtk_window_set_child() aborts here.
    window.set_content(Some(&view));
    crate::ui::util::install_quit_accel(&window);
    window.present();
}

fn apply_to_players(rt: &Arc<AppRuntime>, f: impl Fn(&crate::player::PlayerWindowCore) + 'static) {
    rt.for_each_player(|core| f(&core));
}

// Group 1 — Behavior ---------------------------------------------------------

fn build_behavior_group(page: &adw::PreferencesPage, rt: &Arc<AppRuntime>) {
    let group = adw::PreferencesGroup::builder().title("Behavior").build();

    let s = rt.settings.get();
    let row_new_window = adw::SwitchRow::builder()
        .title("Open New Window for New Files")
        .active(s.open_new_window)
        .build();
    row_new_window.connect_active_notify(glib::clone!(#[strong] rt, move |row| {
        rt.settings.update(|s| s.open_new_window = row.is_active());
    }));

    let row_thumbnail = adw::SwitchRow::builder()
        .title("Seek Bar Thumbnail")
        .active(s.seek_bar_thumbnail)
        .build();
    row_thumbnail.connect_active_notify(glib::clone!(#[strong] rt, move |row| {
        rt.settings.update(|s| s.seek_bar_thumbnail = row.is_active());
    }));

    let row_normalize = adw::SwitchRow::builder()
        .title("Normalize Volume")
        .active(s.normalize_volume)
        .build();
    row_normalize.connect_active_notify(glib::clone!(#[strong] rt, move |row| {
        let on = row.is_active();
        rt.settings.update(|s| s.normalize_volume = on);
        apply_to_players(&rt, move |core| core.player.set_volume_normalization(on));
    }));

    group.add(&row_new_window);
    group.add(&row_thumbnail);
    group.add(&row_normalize);
    page.add(&group);
}

// Group 2 — Performance --------------------------------------------------------

fn build_performance_group(page: &adw::PreferencesPage, rt: &Arc<AppRuntime>) {
    let group = adw::PreferencesGroup::builder().title("Performance").build();

    let s = rt.settings.get();
    let row_offload = adw::SwitchRow::builder()
        .title("Graphics Offload")
        .subtitle("Reduces resource usage; disable if you encounter playback issues.")
        .active(s.graphics_offload)
        .build();
    row_offload.connect_active_notify(glib::clone!(#[strong] rt, move |row| {
        let on = row.is_active();
        rt.settings.update(|s| s.graphics_offload = on);
        apply_to_players(&rt, move |core| core.set_graphics_offload(on));
    }));

    let row_hwaccel = adw::SwitchRow::builder()
        .title("Use Software Decoding")
        .subtitle("Decode on the CPU instead of the GPU. Slower, but avoids driver quirks.")
        .active(s.software_decoding)
        .build();
    row_hwaccel.connect_active_notify(glib::clone!(#[strong] rt, move |row| {
        let on = row.is_active();
        rt.settings.update(|s| s.software_decoding = on);
        apply_to_players(&rt, move |core| core.player.set_hwdec(!on));
    }));

    group.add(&row_offload);
    group.add(&row_hwaccel);
    page.add(&group);
}

// Group 3 — Session & resume -----------------------------------------------------

fn build_session_group(page: &adw::PreferencesPage, rt: &Arc<AppRuntime>) {
    let group = adw::PreferencesGroup::builder().title("Session").build();

    let s = rt.settings.get();
    let row_restore = adw::SwitchRow::builder()
        .title("Restore Saved Session")
        .subtitle("Pick up where you left off after saving from the menu.")
        .active(s.restore_session)
        .build();
    row_restore.add_suffix(&crate::ui::util::info_button(
        "When enabled, windows saved with “Save Session” from the menu are reopened at the saved positions on the next launch.",
    ));
    row_restore.connect_active_notify(glib::clone!(#[strong] rt, move |row| {
        rt.settings.update(|s| s.restore_session = row.is_active());
    }));

    let row_position = adw::SwitchRow::builder()
        .title("Save Video Position on Close")
        .subtitle("Also save options like brightness, subtitle delay, etc.")
        .active(s.save_video_position)
        .build();
    row_position.add_suffix(&crate::ui::util::info_button(
        "When closing a video, its playback position and the picture adjustments and subtitle delay active at that moment are remembered and restored the next time the video is opened.",
    ));
    row_position.connect_active_notify(glib::clone!(#[strong] rt, move |row| {
        rt.settings.update(|s| s.save_video_position = row.is_active());
    }));

    group.add(&row_restore);
    group.add(&row_position);
    page.add(&group);
}

// Group 4 — Subtitles --------------------------------------------------------------

fn build_subtitles_group(page: &adw::PreferencesPage, rt: &Arc<AppRuntime>) {
    let group = adw::PreferencesGroup::builder().title("Subtitles").build();

    let s = rt.settings.get();

    // Subtitle Color: color swatch + reset ↩.
    let color_row = adw::ActionRow::builder().title("Subtitle Color").build();
    let color_btn = gtk::ColorButton::builder()
        .rgba(&gdk::RGBA::parse(&s.subtitle_color).unwrap_or_else(|_| gdk::RGBA::WHITE))
        .build();
    let reset_color = gtk::Button::builder()
        .icon_name("edit-undo-symbolic")
        .css_classes(["flat"])
        .tooltip_text("Reset to white")
        .valign(Align::Center)
        .build();
    color_row.add_suffix(&color_btn);
    color_row.add_suffix(&reset_color);
    color_btn.connect_color_set(glib::clone!(#[strong] rt, move |btn| {
        let hex = to_hex(&btn.rgba());
        rt.settings.update(|s| s.subtitle_color = hex.clone());
        apply_subtitle_style(&rt);
    }));
    reset_color.connect_clicked(glib::clone!(#[strong] rt, #[strong] color_btn, move |_| {
        color_btn.set_rgba(&gdk::RGBA::WHITE);
        rt.settings.update(|s| s.subtitle_color = "#ffffffff".into());
        apply_subtitle_style(&rt);
    }));

    // Subtitle Font: value + ⓘ + reset ↩; font picker behind the value.
    let font_row = adw::ActionRow::builder()
        .title("Subtitle Font")
        .subtitle(&s.subtitle_font)
        .build();
    let font_dialog = gtk::FontDialog::builder().title("Subtitle Font").build();
    let font_btn = gtk::FontDialogButton::builder()
        .dialog(&font_dialog)
        .level(gtk::FontLevel::Font)
        .font_desc(&gtk::pango::FontDescription::from_string(&s.subtitle_font))
        .build();
    let reset_font = gtk::Button::builder()
        .icon_name("edit-undo-symbolic")
        .css_classes(["flat"])
        .tooltip_text("Reset to default")
        .valign(Align::Center)
        .build();
    font_row.add_suffix(&font_btn);
    font_row.add_suffix(&reset_font);
    font_btn.connect_notify_local(Some("font-desc"), glib::clone!(#[strong] rt, #[strong] font_row, move |btn, _| {
        if let Some(desc) = btn.font_desc() {
            let family = desc.family().unwrap_or_default().to_string();
            if !family.is_empty() {
                rt.settings.update(|s| s.subtitle_font = family.clone());
                font_row.set_subtitle(&family);
                apply_subtitle_style(&rt);
            }
        }
    }));
    reset_font.connect_clicked(glib::clone!(#[strong] rt, #[strong] font_btn, #[strong] font_row, move |_| {
        font_btn.set_font_desc(&gtk::pango::FontDescription::from_string("Adwaita Sans SemiBold"));
        rt.settings.update(|s| s.subtitle_font = "Adwaita Sans SemiBold".into());
        font_row.set_subtitle("Adwaita Sans SemiBold");
        apply_subtitle_style(&rt);
    }));

    // Subtitle Scale: numeric stepper row.
    let scale_row = adw::SpinRow::builder()
        .title("Subtitle Scale")
        .digits(2)
        .numeric(true)
        .build();
    let adj = gtk::Adjustment::new(s.subtitle_scale, 0.5, 3.0, 0.1, 0.5, 0.0);
    scale_row.set_adjustment(Some(&adj));
    scale_row.connect_output(glib::clone!(#[strong] rt, move |row| {
        let v = row.value();
        rt.settings.update(|s| s.subtitle_scale = v);
        apply_subtitle_style(&rt);
        true
    }));

    // Subtitle Background: toggle + checkerboard preview.
    let bg_row = adw::SwitchRow::builder()
        .title("Subtitle Background")
        .active(s.subtitle_background)
        .build();
    let preview = gtk::Picture::builder()
        .paintable(&crate::ui::util::checkerboard_texture())
        .content_fit(gtk::ContentFit::Fill)
        .width_request(40)
        .height_request(24)
        .valign(Align::Center)
        .build();
    bg_row.add_suffix(&preview);
    bg_row.connect_active_notify(glib::clone!(#[strong] rt, move |row| {
        let on = row.is_active();
        rt.settings.update(|s| s.subtitle_background = on);
        apply_subtitle_style(&rt);
    }));

    group.add(&color_row);
    group.add(&font_row);
    group.add(&scale_row);
    group.add(&bg_row);
    page.add(&group);
}

fn apply_subtitle_style(rt: &Arc<AppRuntime>) {
    let s = rt.settings.get();
    let color = mpv_color(&s.subtitle_color).unwrap_or_else(|| "#ffffffff".into());
    let background = if s.subtitle_background {
        Some("#a0000000".to_string())
    } else {
        None
    };
    let (font, scale) = (s.subtitle_font.clone(), s.subtitle_scale);
    apply_to_players(rt, move |core| {
        core.player.apply_subtitle_style(&font, &color, background.as_deref());
        core.player.set_sub_scale(scale);
    });
}

fn to_hex(rgba: &gdk::RGBA) -> String {
    let to8 = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        to8(rgba.red()),
        to8(rgba.green()),
        to8(rgba.blue()),
        to8(rgba.alpha())
    )
}

// Group 5 — Mouse behavior -----------------------------------------------------------

fn build_mouse_group(page: &adw::PreferencesPage, rt: &Arc<AppRuntime>) {
    let group = adw::PreferencesGroup::builder().title("Mouse behavior").build();

    let s = rt.settings.get();
    let primary = gtk::StringList::new(&["Play/Pause", "Focus/Play/Pause", "Bypass"]);
    let row_primary = adw::ComboRow::builder().title("Primary Click").model(&primary).build();
    row_primary.set_selected(match s.primary_click {
        PrimaryClickAction::PlayPause => 0,
        PrimaryClickAction::FocusPlayPause => 1,
        PrimaryClickAction::Bypass => 2,
    });
    row_primary.connect_selected_notify(glib::clone!(#[strong] rt, move |row| {
        let action = match row.selected() {
            0 => PrimaryClickAction::PlayPause,
            1 => PrimaryClickAction::FocusPlayPause,
            _ => PrimaryClickAction::Bypass,
        };
        rt.settings.update(|s| s.primary_click = action);
    }));

    let secondary = gtk::StringList::new(&["Play/Pause", "Context Menu", "Bypass"]);
    let row_secondary = adw::ComboRow::builder().title("Secondary Click").model(&secondary).build();
    row_secondary.set_selected(match s.secondary_click {
        SecondaryClickAction::PlayPause => 0,
        SecondaryClickAction::ContextMenu => 1,
        SecondaryClickAction::Bypass => 2,
    });
    row_secondary.connect_selected_notify(glib::clone!(#[strong] rt, move |row| {
        let action = match row.selected() {
            0 => SecondaryClickAction::PlayPause,
            1 => SecondaryClickAction::ContextMenu,
            _ => SecondaryClickAction::Bypass,
        };
        rt.settings.update(|s| s.secondary_click = action);
    }));

    group.add(&row_primary);
    group.add(&row_secondary);
    page.add(&group);
}

// Group 6 — Language preferences --------------------------------------------------------

fn build_language_group(page: &adw::PreferencesPage, rt: &Arc<AppRuntime>) {
    let group = adw::PreferencesGroup::builder().title("Language preferences").build();

    let s = rt.settings.get();
    inline_edit_row(
        &group,
        "Preferred Subtitle Languages",
        &s.preferred_subtitle_languages,
        glib::clone!(#[strong] rt, move |v| {
            rt.settings.update(|s| s.preferred_subtitle_languages = v);
        }),
    );
    inline_edit_row(
        &group,
        "Preferred Audio Languages",
        &s.preferred_audio_languages,
        glib::clone!(#[strong] rt, move |v| {
            rt.settings.update(|s| s.preferred_audio_languages = v);
        }),
    );
    page.add(&group);
}

/// Inline editor row (docs/04): clicking the pencil ✎ turns the row into a
/// text entry (placeholder = the row title); Enter commits. Implemented as a
/// view row + entry row pair toggled by visibility.
fn inline_edit_row(
    group: &adw::PreferencesGroup,
    title: &str,
    initial: &str,
    on_commit: impl Fn(String) + 'static,
) {
    let view_row = adw::ActionRow::builder().title(title).build();
    let value_label = gtk::Label::builder()
        .label(if initial.is_empty() { "—" } else { initial })
        .css_classes(["dim-label"])
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    let edit_btn = gtk::Button::builder()
        .icon_name("document-edit-symbolic")
        .css_classes(["flat"])
        .valign(Align::Center)
        .build();
    view_row.add_suffix(&value_label);
    view_row.add_suffix(&edit_btn);

    let edit_row = adw::EntryRow::builder()
        .title(title)
        .text(initial)
        .visible(false)
        .show_apply_button(true)
        .build();

    group.add(&view_row);
    group.add(&edit_row);

    edit_btn.connect_clicked(glib::clone!(#[strong] view_row, #[strong] edit_row, move |_| {
        view_row.set_visible(false);
        edit_row.set_visible(true);
        edit_row.grab_focus();
    }));

    let commit_cell = std::rc::Rc::new(std::cell::RefCell::new(Some(on_commit)));
    edit_row.connect_apply(glib::clone!(
        #[strong] view_row,
        #[strong] edit_row,
        #[strong] value_label,
        #[strong] commit_cell,
        move |_| {
            let text = edit_row.text().trim().to_string();
            value_label.set_label(&if text.is_empty() { "—".to_string() } else { text.clone() });
            edit_row.set_visible(false);
            view_row.set_visible(true);
            if let Some(cb) = commit_cell.borrow_mut().as_mut() {
                cb(text);
            }
        }
    ));
}

// Footer --------------------------------------------------------------------------------

fn build_footer(page: &adw::PreferencesPage, rt: &Arc<AppRuntime>) {
    let group = adw::PreferencesGroup::builder().build();
    let label = gtk::Label::new(Some("Open Config Folder"));
    let arrow = gtk::Image::builder().icon_name("go-next-symbolic").build();
    let box_ = gtk::Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .halign(Align::Center)
        .build();
    box_.append(&label);
    box_.append(&arrow);
    let btn = gtk::Button::builder()
        .child(&box_)
        .css_classes(["flat"])
        .build();
    btn.connect_clicked(glib::clone!(#[strong] rt, move |_| {
        let dir = gio::File::for_path(crate::config::config_dir());
        let launcher = gtk::FileLauncher::new(Some(&dir));
        launcher.launch(None::<&gtk::Window>, gio::Cancellable::NONE, |result| {
            if let Err(e) = result {
                eprintln!("frame: failed to open config folder: {e}");
            }
        });
    }));
    group.add(&btn);
    page.add(&group);
}
