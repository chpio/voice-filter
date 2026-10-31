//! The desktop entry and the icon of the app. GNOME Shell shows the app among the others and
//! takes the icon of its window from the desktop entry named after its app ID; `cargo install`
//! only installs the binary, so `voice-filter install-desktop` writes them.

use ::std::fs;
use ::std::path::{Path, PathBuf};
use ::std::process::Command;

use ::anyhow::{Context as _, Result, anyhow};

use super::APP_ID;

const DESKTOP: &str = include_str!("../../data/net.b128.VoiceFilter.desktop");
const ICON: &str = include_str!("../../data/net.b128.VoiceFilter.svg");

/// `$XDG_DATA_HOME`, `~/.local/share` by default.
pub fn data_home() -> Result<PathBuf> {
    ::std::env::var_os("XDG_DATA_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| ::std::env::var_os("HOME").map(|home| Path::new(&home).join(".local/share")))
        .ok_or_else(|| anyhow!("Neither XDG_DATA_HOME nor HOME is set"))
}

/// Writes the desktop entry, which starts `exe` by its full path, and the icon into `data_home`;
/// returns their paths.
pub fn install(data_home: &Path, exe: &Path) -> Result<[PathBuf; 2]> {
    let exe = exe
        .to_str()
        .ok_or_else(|| anyhow!("The path {} isn't UTF-8", exe.display()))?;
    let desktop: String = DESKTOP
        .lines()
        .map(|line| match line.strip_prefix("Exec=") {
            Some(_) => format!("Exec={} ui\n", quote(exe)),
            None => format!("{line}\n"),
        })
        .collect();
    let entry = data_home
        .join("applications")
        .join(format!("{APP_ID}.desktop"));
    let icon = data_home
        .join("icons/hicolor/scalable/apps")
        .join(format!("{APP_ID}.svg"));
    for (path, content) in [(&entry, desktop.as_str()), (&icon, ICON)] {
        let dir = path.parent().expect("a file in a directory");
        fs::create_dir_all(dir).with_context(|| format!("Could not create {}", dir.display()))?;
        fs::write(path, content).with_context(|| format!("Could not write {}", path.display()))?;
    }
    Ok([entry, icon])
}

/// Updates the icon cache of `data_home`'s hicolor theme, if there's a tool for it: with a cache
/// older than the icon, GTK and GNOME Shell may not find it. Best effort, a missing tool is fine.
pub fn update_icon_cache(data_home: &Path) {
    let theme = data_home.join("icons/hicolor");
    for tool in ["gtk4-update-icon-cache", "gtk-update-icon-cache"] {
        let updated = Command::new(tool)
            .args(["--force", "--ignore-theme-index", "--quiet"])
            .arg(&theme)
            .status();
        if updated.is_ok_and(|status| status.success()) {
            return;
        }
    }
}

/// A path as an argument of `Exec`: quoted if it has a character the desktop entry spec reserves,
/// with `"`, `` ` ``, `$` and `\` escaped inside the quotes.
pub fn quote(path: &str) -> String {
    let reserved = |c: char| " \t\n\"'\\><~|&;$*?#()`".contains(c);
    if !path.contains(reserved) {
        return path.to_owned();
    }
    let mut quoted = String::from('"');
    for c in path.chars() {
        if "\"`$\\".contains(c) {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    quoted
}
