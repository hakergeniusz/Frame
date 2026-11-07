//! Shared UI helpers.

pub mod about;
pub mod file_dialogs;
pub mod history_popover;
pub mod menus;
pub mod preferences;
pub mod shortcuts;
pub mod url_dialog;
pub mod util;
pub mod welcome;

pub use url_dialog::{show_add_url_dialog, show_open_url_dialog};
