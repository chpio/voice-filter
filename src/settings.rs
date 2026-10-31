//! Settings kept between runs, in `$XDG_STATE_HOME/voice-filter/` (`~/.local/state/voice-filter/`
//! by default), one file per setting.

use ::std::fs;
use ::std::io;
use ::std::path::{Path, PathBuf};

use ::anyhow::{Context as _, Result};

/// The directory of the settings, `None` without `$XDG_STATE_HOME` and `$HOME`.
pub fn dir() -> Option<PathBuf> {
    let state = ::std::env::var_os("XDG_STATE_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| ::std::env::var_os("HOME").map(|home| Path::new(&home).join(".local/state")))?;
    Some(state.join("voice-filter"))
}

const INPUT: &str = "input";

/// The microphone chosen last, the `node.name` of a PipeWire source.
pub fn load_input(dir: &Path) -> Result<Option<String>> {
    let path = dir.join(INPUT);
    match fs::read_to_string(&path) {
        Ok(name) => Ok(Some(name.trim().to_owned()).filter(|name| !name.is_empty())),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("Could not read {}", path.display())),
    }
}

pub fn save_input(dir: &Path, name: &str) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("Could not create {}", dir.display()))?;
    let path = dir.join(INPUT);
    fs::write(&path, format!("{name}\n"))
        .with_context(|| format!("Could not write {}", path.display()))
}
