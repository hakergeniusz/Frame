//! Welcome window (docs/02): centered stack with the app icon, bold title,
//! "Drag and drop files here" subtitle, the Open…/Open Folder/Open URL
//! buttons with the circular history button (badge), the ☰ primary menu,
//! and watch-history popover access.

use crate::engine::MediaSource;
use crate::rt::AppRuntime;
use adw::prelude::*;
use gtk::{gdk, gio, glib, Align, Orientation};
use std::rc::Rc;
use std::sync::Arc;

pub struct WelcomeWindow {
    pub rt: Arc<AppRuntime>,
    pub window: adw::ApplicationWindow,
    pub toast_overlay: adw::ToastOverlay,
    history_btn: gtk::MenuButton,
    badge: gtk::Label,
    menu_btn: gtk::MenuButton,
}

pub fn create_welcome_window(rt: Arc<AppRuntime>) -> Rc<WelcomeWindow> {
    let window = adw::ApplicationWindow::builder()
        .title("Frame")
        .default_width(1070)
        .default_height(600)
        .build();
    if let Some(app) = gio::Application::default() {
        window.set_application(Some(app.downcast_ref::<adw::Application>().unwrap()));
    }

    let toast_overlay = adw::ToastOverlay::new();

    // Header: ☰ left; min/max/close right (CSD default).
    let header = adw::HeaderBar::new();
    let menu_btn = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&crate::ui::util::primary_menu())
        .tooltip_text("Main Menu")
        .build();
    header.pack_start(&menu_btn);

    // Centered stack: icon, title, subtitle, buttons.
    let icon = crate::ui::util::app_icon_image();
    let title = gtk::Label::builder().label("Frame").css_classes(["title-1"]).build();
    let subtitle = gtk::Label::builder()
        .label("Drag and drop files here")
        .css_classes(["dim-label"])
        .build();

    let buttons = gtk::Box::builder()
        .orientation(Orientation::Vertical)
        .spacing(10)
        .halign(Align::Center)
        .valign(Align::Center)
        .margin_top(18)
        .build();

    // First row: Open… (filled pill) + circular history button with badge.
    let first_row = gtk::Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(10)
        .halign(Align::Center)
        .build();
    let open_btn = gtk::Button::builder()
        .label("Open…")
        .css_classes(["pill", "suggested-action"])
        .build();
    let badge = gtk::Label::builder()
        .label("0")
        .css_classes(["osd", "caption", "numeric"])
        .width_request(18)
        .halign(Align::Center)
        .build();
    let badge_overlay = gtk::Overlay::builder()
        .halign(Align::Center)
        .valign(Align::Center)
        .build();
    let history_btn = gtk::MenuButton::builder()
        .icon_name("document-open-recent-symbolic")
        .css_classes(["flat", "circular"])
        .tooltip_text("Watch History (Ctrl+H)")
        .build();
    badge_overlay.set_child(Some(&history_btn));
    badge.set_halign(Align::End);
    badge.set_valign(Align::End);
    badge_overlay.add_overlay(&badge);
    first_row.append(&open_btn);
    first_row.append(&badge_overlay);

    let open_folder_btn = gtk::Button::builder()
        .label("Open Folder")
        .css_classes(["pill"])
        .halign(Align::Center)
        .build();
    let open_url_btn = gtk::Button::builder()
        .label("Open URL")
        .css_classes(["pill"])
        .halign(Align::Center)
        .build();

    buttons.append(&first_row);
    buttons.append(&open_folder_btn);
    buttons.append(&open_url_btn);

    let stack = gtk::Box::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .valign(Align::Center)
        .halign(Align::Center)
        .build();
    stack.append(&icon);
    stack.append(&title);
    stack.append(&subtitle);
    stack.append(&buttons);

    toast_overlay.set_child(Some(&stack));

    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&toast_overlay));
    window.set_content(Some(&view));

    let this = Rc::new(WelcomeWindow {
        rt: rt.clone(),
        window: window.clone(),
        toast_overlay,
        history_btn,
        badge,
        menu_btn,
    });

    // Watch History popover on the 🕐 button.
    let popover = crate::ui::history_popover::build_history_popover(
        &rt.history,
        &this.toast_overlay,
        glib::clone!(#[strong] this, move |source| {
            this.rt.apply_open(vec![source], false, None);
        }),
    );
    this.history_btn.set_popover(Some(&popover));

    // Actions.
    let a = gio::SimpleAction::new("open-files", None);
    a.connect_activate(glib::clone!(#[strong] this, move |_, _| {
        this.rt.begin_open_files(&this.window, false, None);
    }));
    this.window.add_action(&a);

    let a = gio::SimpleAction::new("open-folder", None);
    a.connect_activate(glib::clone!(#[strong] this, move |_, _| {
        this.rt.begin_open_folder(&this.window, false, None);
    }));
    this.window.add_action(&a);

    let a = gio::SimpleAction::new("open-url", None);
    a.connect_activate(glib::clone!(#[strong] this, move |_, _| {
        crate::ui::show_open_url_dialog(&this.window, glib::clone!(#[strong] this, move |url| {
            this.rt.apply_open(vec![MediaSource::Url(url)], false, None);
        }));
    }));
    this.window.add_action(&a);

    // "Add …" from a welcome window behaves like Open (nothing to append to;
    // recorded assumption).
    let a = gio::SimpleAction::new("add-files", None);
    a.connect_activate(glib::clone!(#[strong] this, move |_, _| {
        this.rt.begin_open_files(&this.window, false, None);
    }));
    this.window.add_action(&a);

    let a = gio::SimpleAction::new("add-folder", None);
    a.connect_activate(glib::clone!(#[strong] this, move |_, _| {
        this.rt.begin_open_folder(&this.window, false, None);
    }));
    this.window.add_action(&a);

    let a = gio::SimpleAction::new("add-url", None);
    a.connect_activate(glib::clone!(#[strong] this, move |_, _| {
        crate::ui::show_add_url_dialog(&this.window, glib::clone!(#[strong] this, move |url| {
            this.rt.apply_open(vec![MediaSource::Url(url)], false, None);
        }));
    }));
    this.window.add_action(&a);

    let a = gio::SimpleAction::new("add-subtitle-track", None);
    a.connect_activate(glib::clone!(#[strong] this, move |_, _| {
        crate::ui::util::toast(&this.toast_overlay, "Open a video first to add subtitle tracks");
    }));
    this.window.add_action(&a);

    let a = gio::SimpleAction::new("add-audio-track", None);
    a.connect_activate(glib::clone!(#[strong] this, move |_, _| {
        crate::ui::util::toast(&this.toast_overlay, "Open a video first to add audio tracks");
    }));
    this.window.add_action(&a);

    let a = gio::SimpleAction::new("watch-history", None);
    a.connect_activate(glib::clone!(#[strong] this, move |_, _| {
        this.history_btn.popup();
    }));
    this.window.add_action(&a);

    let a = gio::SimpleAction::new("save-session", None);
    a.connect_activate(glib::clone!(#[strong] this, move |_, _| {
        this.rt.save_session_now();
        crate::ui::util::toast(&this.toast_overlay, "Session saved");
    }));
    this.window.add_action(&a);

    let a = gio::SimpleAction::new("save-session-close", None);
    a.connect_activate(glib::clone!(#[strong] this, move |_, _| {
        this.rt.save_session_now();
        this.window.close();
    }));
    this.window.add_action(&a);

    // Open… button.
    open_btn.connect_clicked(glib::clone!(#[strong] this, move |_| {
        this.rt.begin_open_files(&this.window, false, None);
    }));
    open_folder_btn.connect_clicked(glib::clone!(#[strong] this, move |_| {
        this.rt.begin_open_folder(&this.window, false, None);
    }));
    open_url_btn.connect_clicked(glib::clone!(#[strong] this, move |_| {
        crate::ui::show_open_url_dialog(&this.window, glib::clone!(#[strong] this, move |url| {
            this.rt.apply_open(vec![MediaSource::Url(url)], false, None);
        }));
    }));

    // Refresh the badge whenever the history popover opens.
    let this2 = this.clone();
    this.history_btn
        .popover()
        .and_downcast::<gtk::Popover>()
        .map(|p| {
            p.connect_map(glib::clone!(#[strong] this2, move |_| {
                this2.update_badge();
            }))
        });
    this.update_badge();

    // Drop target: the whole welcome window invites file drops (docs/02).
    let this3 = this.clone();
    let file_target = gtk::DropTarget::builder()
        .actions(gdk::DragAction::COPY)
        .formats(&gdk::ContentFormats::for_type(gdk::FileList::static_type()))
        .build();
    file_target.connect_drop(glib::clone!(#[strong] this3, move |_, value, _, _| {
        if let Ok(list) = value.get::<gdk::FileList>() {
            let mut media = Vec::new();
            for file in list.files() {
                if let Some(path) = file.path() {
                    if path.is_dir() {
                        if let Some(items) = crate::rt::scan_folder(&path) {
                            media.extend(items);
                        }
                    } else if crate::inhibit::is_video_file(&path.to_string_lossy()) {
                        media.push(MediaSource::Path(path.to_string_lossy().into_owned()));
                    }
                }
            }
            if media.is_empty() {
                crate::ui::util::toast(&this3.toast_overlay, "No supported media files found");
            } else {
                this3.rt.apply_open(media, false, None);
            }
            return true;
        }
        false
    }));
    window.add_controller(file_target);

    let this4 = this.clone();
    let text_target = gtk::DropTarget::builder()
        .actions(gdk::DragAction::COPY)
        .formats(&gdk::ContentFormats::for_type(glib::types::Type::STRING))
        .build();
    text_target.connect_drop(glib::clone!(#[strong] this4, move |_, value, _, _| {
        if let Ok(text) = value.get::<String>() {
            let trimmed = text.trim();
            if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
                this4.rt.apply_open(vec![MediaSource::Url(trimmed.to_string())], false, None);
                return true;
            }
        }
        false
    }));
    window.add_controller(text_target);

    // Keyboard shortcuts (welcome subset; docs/05 General — the media keys
    // have no target here).
    this.install_keys();

    // Clipboard scan at startup (docs/01): offer to open a copied media URL.
    this.check_clipboard();

    window.present();
    this
}

impl WelcomeWindow {
    /// Badge count (assumption recorded in IMPLEMENTATION_DECISIONS, docs/02:
    /// meaning unresolved — chosen: number of entries added today, shown
    /// always, `0` included, matching the observed `0` badge).
    fn update_badge(&self) {
        let now = glib::DateTime::now_local().map(|d| d.to_unix()).unwrap_or(0);
        let today_start = now - (now % 86_400);
        let count = self
            .rt
            .history
            .entries()
            .iter()
            .filter(|e| e.timestamp >= today_start)
            .count();
        self.badge.set_label(&count.to_string());
    }

    fn check_clipboard(self: &Rc<Self>) {
        let this = self.clone();
        gdk::Display::default()
            .and_then(|d| Some(d.clipboard()))
            .map(|clipboard| {
                clipboard.read_text_async(gio::Cancellable::NONE, move |result| {
                    if let Ok(Some(text)) = result {
                        let trimmed = text.trim().to_string();
                        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
                            // A plausible media URL was copied — offer to open it
                            // (purpose inferred in docs/01; toast chosen as the
                            // minimal non-modal offer).
                            crate::ui::util::toast_with_button(
                                &this.toast_overlay,
                                "Open media from the clipboard?",
                                "Open",
                                glib::clone!(#[strong] this, move |_| {
                                    this.rt.apply_open(
                                        vec![MediaSource::Url(trimmed.to_string())],
                                        false,
                                        None,
                                    );
                                }),
                            );
                        }
                    }
                });
            });
    }

    fn install_keys(self: &Rc<Self>) {
        let this = self.clone();
        let controller = gtk::EventControllerKey::new();
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        controller.connect_key_pressed(glib::clone!(#[strong] this, move |_, keyval, _code, state| {
            use gdk::ModifierType as M;
            if focus_is_editable(&this.window) {
                return glib::Propagation::Proceed;
            }
            let ctrl = state.contains(M::CONTROL_MASK);
            let shift = state.contains(M::SHIFT_MASK);
            let name = keyval.name().map(|s| s.to_string()).unwrap_or_default();
            let name = name.as_str();
            if ctrl && !shift {
                match name {
                    "n" => {
                        gtk::prelude::WidgetExt::activate_action(&this.window, "app.new-window", None);
                        return glib::Propagation::Stop;
                    }
                    "o" => {
                        this.rt.begin_open_files(&this.window, false, None);
                        return glib::Propagation::Stop;
                    }
                    "i" => {
                        this.rt.begin_open_folder(&this.window, false, None);
                        return glib::Propagation::Stop;
                    }
                    "u" => {
                        crate::ui::show_open_url_dialog(&this.window, glib::clone!(#[strong] this, move |url| {
                            this.rt.apply_open(vec![MediaSource::Url(url)], false, None);
                        }));
                        return glib::Propagation::Stop;
                    }
                    "h" => {
                        this.history_btn.popup();
                        return glib::Propagation::Stop;
                    }
                    "question" => {
                        gtk::prelude::WidgetExt::activate_action(&this.window, "app.shortcuts", None);
                        return glib::Propagation::Stop;
                    }
                    "comma" => {
                        gtk::prelude::WidgetExt::activate_action(&this.window, "app.preferences", None);
                        return glib::Propagation::Stop;
                    }
                    "w" => {
                        this.window.close();
                        return glib::Propagation::Stop;
                    }
                    _ => {}
                }
            }
            if ctrl && shift && name == "S" {
                this.rt.save_session_now();
                crate::ui::util::toast(&this.toast_overlay, "Session saved");
                return glib::Propagation::Stop;
            }
            if !ctrl && !shift && name == "q" {
                this.window.close();
                return glib::Propagation::Stop;
            }
            if !ctrl && shift && name == "Q" {
                this.rt.save_session_now();
                this.window.close();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        }));
        self.window.add_controller(controller);
    }
}

fn focus_is_editable(window: &adw::ApplicationWindow) -> bool {
    window
        .property::<Option<gtk::Widget>>("focus-widget")
        .map(|f| f.is::<gtk::Text>() || f.is::<gtk::TextView>())
        .unwrap_or(false)
}

/// Keeps the welcome window alive for its widget lifetime (see player/mod.rs
/// lifetime note).
pub fn connect_keepalive(win: &Rc<WelcomeWindow>) {
    let clone = win.clone();
    win.window.connect_destroy(move |_| {
        let _ = &clone;
    });
}
