//! The icon in the tray, shown by the daemon: a StatusNotifierItem (through `ksni`, on a thread
//! and connection of its own), which KDE shows and GNOME Shell with the AppIndicator extension. A
//! click opens the app, its menu switches the whole filter on and off, the icon shows which (the
//! app icon's motif, a microphone between noise and a clean wave, dimmed and struck through
//! while the filter is off).

use ::std::fs;
use ::std::path::{Path, PathBuf};
use ::std::process::{Command, Stdio};
use ::std::sync::Arc;
use ::std::sync::atomic::{AtomicBool, Ordering};

use ::anyhow::{Context as _, Result};
use ::ksni::blocking::{Handle, TrayMethods as _};
use ::ksni::menu::{CheckmarkItem, StandardItem};
use ::ksni::{Category, MenuItem, OfflineReason, ToolTip};

use crate::i18n::tr;
use crate::param::Param;
use crate::settings;

/// The stage the menu switches besides the whole filter.
const ECHO: &str = "echo";

/// The icons, by the names the app ID gives them; `voice-filter install-desktop` installs them
/// into the icon theme too.
pub const ICON_ON: &str = "net.b128.VoiceFilter-symbolic";
pub const ICON_OFF: &str = "net.b128.VoiceFilter-off-symbolic";
pub const ICONS: [(&str, &str); 2] = [
    (
        ICON_ON,
        include_str!("../data/net.b128.VoiceFilter-symbolic.svg"),
    ),
    (
        ICON_OFF,
        include_str!("../data/net.b128.VoiceFilter-off-symbolic.svg"),
    ),
];

/// A setting the menu switches through someone else: reads it and sets it.
pub struct Toggle {
    pub get: Box<dyn Fn() -> bool + Send>,
    pub set: Box<dyn Fn(bool) + Send>,
}

pub struct Tray {
    /// The switch of the whole filter, see [`crate::chain::Chain::enabled`].
    pub enabled: Arc<AtomicBool>,
    /// The switches of the stages, as `Chain::switches` gives them; the menu switches the echo
    /// cancellation (if the chain has it) and keeps them with the parameters, as the control
    /// interface does.
    pub switches: Vec<(&'static str, Arc<AtomicBool>)>,
    pub params: Vec<(&'static str, Param)>,
    /// Where switching is kept for the next runs.
    pub settings_dir: Option<PathBuf>,
    /// Listen, through the control interface, which starts and stops its streams and keeps it if
    /// asked to; `None` without one.
    pub listen: Option<Toggle>,
    /// A directory with the icons (see [`write_icons`]), for hosts that look there first; empty
    /// if there's none.
    pub icon_theme_path: String,
    /// The binary that opens the app with `ui`, `None` if it's built without the app.
    pub app: Option<PathBuf>,
}

impl Tray {
    pub fn is_on(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// The switch of the echo cancellation, if the chain has it.
    pub fn echo(&self) -> Option<&Arc<AtomicBool>> {
        self.switches
            .iter()
            .find(|(stage, _)| *stage == ECHO)
            .map(|(_, enabled)| enabled)
    }

    /// Switches the echo cancellation and keeps the stages, as the control interface does.
    pub fn set_echo(&mut self, on: bool) {
        let Some(echo) = self.echo() else {
            return;
        };
        echo.store(on, Ordering::Relaxed);
        if let Some(dir) = &self.settings_dir
            && let Err(err) = settings::save_chain(dir, &self.switches, &self.params)
        {
            eprintln!("{err:#}");
        }
    }

    /// Switches the filter and keeps it, as the control interface does.
    pub fn set(&mut self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
        if let Some(dir) = &self.settings_dir
            && let Err(err) = settings::save_enabled(dir, on)
        {
            eprintln!("{err:#}");
        }
    }

    /// Starts the app (`app ui`), which brings its window to the front if it's open already. A
    /// thread waits for it, so it doesn't stay behind as a zombie. With the activation token of the
    /// click (see [`activation_token`]): without one, GNOME Shell only says "Voice Filter is ready"
    /// instead of raising an open window, GApplication hands it on to the running instance.
    fn open_app(&self) {
        let Some(app) = &self.app else {
            return;
        };
        let mut command = Command::new(app);
        command.arg("ui").stdin(Stdio::null());
        // never one the daemon was started with, it's used up
        command.env_remove("XDG_ACTIVATION_TOKEN");
        command.env_remove("DESKTOP_STARTUP_ID");
        if let Some(token) = activation_token() {
            command.env("XDG_ACTIVATION_TOKEN", &token);
            command.env("DESKTOP_STARTUP_ID", &token);
        }
        match command.spawn() {
            Ok(mut child) => {
                let _ = ::std::thread::Builder::new()
                    .name("app".into())
                    .spawn(move || child.wait());
            }
            Err(err) => eprintln!("Could not start {} ui: {err}", app.display()),
        }
    }
}

/// The XDG activation token the tray's host gave with the click, which lets the app raise its
/// window on Wayland. GNOME Shell's AppIndicator extension sends it before the click itself,
/// others may send it a little after it, so it's asked for a few times, up to 20 ms.
fn activation_token() -> Option<String> {
    for _ in 0..20 {
        if let Some(token) = ::ksni::current_xdg_activation_token() {
            return Some(token);
        }
        ::std::thread::sleep(::std::time::Duration::from_millis(1));
    }
    None
}

impl ::ksni::Tray for Tray {
    fn id(&self) -> String {
        "voice-filter".into()
    }

    fn title(&self) -> String {
        tr("tray-title")
    }

    fn category(&self) -> Category {
        Category::Hardware
    }

    fn icon_name(&self) -> String {
        if self.is_on() { ICON_ON } else { ICON_OFF }.into()
    }

    fn icon_theme_path(&self) -> String {
        self.icon_theme_path.clone()
    }

    fn tool_tip(&self) -> ToolTip {
        let description = tr(if self.is_on() { "tray-on" } else { "tray-off" });
        ToolTip {
            title: tr("tray-title"),
            description,
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.open_app();
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut menu = vec![
            CheckmarkItem {
                label: tr("tray-filter"),
                checked: self.is_on(),
                activate: Box::new(|tray: &mut Self| tray.set(!tray.is_on())),
                ..Default::default()
            }
            .into(),
        ];
        if let Some(echo) = self.echo() {
            menu.push(
                CheckmarkItem {
                    label: tr("stage-echo"),
                    checked: echo.load(Ordering::Relaxed),
                    // its own switch, which counts while the filter is on
                    enabled: self.is_on(),
                    activate: Box::new(|tray: &mut Self| {
                        let on = tray.echo().is_some_and(|echo| echo.load(Ordering::Relaxed));
                        tray.set_echo(!on);
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }
        if let Some(listen) = &self.listen {
            menu.push(
                CheckmarkItem {
                    label: tr("listen"),
                    checked: (listen.get)(),
                    activate: Box::new(|tray: &mut Self| {
                        if let Some(listen) = &tray.listen {
                            (listen.set)(!(listen.get)());
                        }
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }
        if self.app.is_some() {
            menu.push(MenuItem::Separator);
            menu.push(
                StandardItem {
                    label: tr("tray-open"),
                    activate: Box::new(|tray: &mut Self| tray.open_app()),
                    ..Default::default()
                }
                .into(),
            );
        }
        menu
    }

    fn watcher_offline(&self, reason: OfflineReason) -> bool {
        match reason {
            OfflineReason::No => eprintln!(
                "No tray to show the icon in; on GNOME it takes the AppIndicator extension"
            ),
            reason => eprintln!("The icon left the tray: {reason:?}"),
        }
        // keeps running, the icon shows when a tray appears
        true
    }
}

/// Shows `tray` from a thread of its own, also when there's no tray yet: it shows the icon when
/// one appears. The handle updates the icon after the filter was switched elsewhere.
pub fn spawn(tray: Tray) -> Result<Handle<Tray>> {
    tray.assume_sni_available(true)
        .spawn()
        .context("Could not show the icon in the tray")
}

/// Writes the icons into `dir` (one file each, by its name), for [`Tray::icon_theme_path`].
pub fn write_icons(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("Could not create {}", dir.display()))?;
    for (name, svg) in ICONS {
        let path = dir.join(format!("{name}.svg"));
        fs::write(&path, svg).with_context(|| format!("Could not write {}", path.display()))?;
    }
    Ok(())
}

/// Where the daemon writes the icons: `$XDG_RUNTIME_DIR/voice-filter/icons`, else the directory
/// `icons` in the settings.
pub fn icons_dir(settings_dir: Option<&Path>) -> Option<PathBuf> {
    ::std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|dir| !dir.is_empty())
        .map(|dir| PathBuf::from(dir).join("voice-filter"))
        .or_else(|| settings_dir.map(Path::to_owned))
        .map(|dir| dir.join("icons"))
}
