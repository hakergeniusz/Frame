//! About window (docs/04): standard Adw about window with icon, name,
//! developer, version chip and the Details / Report an Issue / Legal /
//! Acknowledgements subpages.

use crate::config::{APP_ID, APP_NAME, DEVELOPER, ISSUE_URL, VERSION, WEBSITE};
use gtk::prelude::*;

pub fn show_about(parent: &impl IsA<gtk::Window>) {
    let about = adw::AboutWindow::builder()
        .transient_for(parent)
        .modal(true)
        .application_name(APP_NAME)
        .application_icon(APP_ID)
        .developer_name(DEVELOPER)
        .version(VERSION)
        .website(WEBSITE)
        .issue_url(ISSUE_URL)
        .license_type(gtk::License::MitX11)
        .copyright("© 2026 hakergeniusz")
        .build();
    crate::ui::util::install_quit_accel(&about);
    about.present();
}
