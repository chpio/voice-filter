//! The GNOME app of Voice Filter: switches and settings of the running `voice-filter run` and its
//! status, over its control interface on the session bus.

mod proxy;
mod window;

use ::adw::prelude::*;
use ::gtk::glib;

/// The app owns its ID as a bus name, so the daemon has another one
/// (`net.b128.VoiceFilter.Daemon`).
const APP_ID: &str = "net.b128.VoiceFilter";

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(|app| match app.active_window() {
        Some(window) => window.present(),
        None => window::build(app).present(),
    });
    app.run()
}
