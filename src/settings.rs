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
const OUTPUT: &str = "output";

/// The microphone chosen last, the `node.name` of a PipeWire source.
pub fn load_input(dir: &Path) -> Result<Option<String>> {
    load(dir, INPUT)
}

pub fn save_input(dir: &Path, name: &str) -> Result<()> {
    save(dir, INPUT, name)
}

/// The output device chosen last for the reference of the echo canceller, the `node.name` of a
/// PipeWire sink.
pub fn load_output(dir: &Path) -> Result<Option<String>> {
    load(dir, OUTPUT)
}

pub fn save_output(dir: &Path, name: &str) -> Result<()> {
    save(dir, OUTPUT, name)
}

/// The setting in the file `name`, `None` if there's none or it's empty.
fn load(dir: &Path, name: &str) -> Result<Option<String>> {
    let path = dir.join(name);
    match fs::read_to_string(&path) {
        Ok(name) => Ok(Some(name.trim().to_owned()).filter(|name| !name.is_empty())),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("Could not read {}", path.display())),
    }
}

fn save(dir: &Path, name: &str, value: &str) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("Could not create {}", dir.display()))?;
    let path = dir.join(name);
    fs::write(&path, format!("{value}\n"))
        .with_context(|| format!("Could not write {}", path.display()))
}
