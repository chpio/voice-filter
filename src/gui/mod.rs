//! The GNOME app of Voice Filter (`voice-filter ui`): switches and settings of the running
//! `voice-filter run` and its status, over its control interface on the session bus. A process of
//! its own, so closing or crashing it never touches the audio.

mod proxy;
mod window;

use ::adw::prelude::*;
use ::gtk::glib;

/// The app owns its ID as a bus name, so the daemon has another one
/// (`net.b128.VoiceFilter.Daemon`).
const APP_ID: &str = "net.b128.VoiceFilter";

/// Runs the app until its last window is closed.
pub fn run() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(|app| match app.active_window() {
        Some(window) => window.present(),
        None => window::build(app).present(),
    });
    // only the program name: the arguments were clap's, GApplication would reject them
    let argv0: Vec<_> = ::std::env::args_os().take(1).collect();
    app.run_with_args_os(&argv0)
}
