//! Settings kept between runs, in `$XDG_STATE_HOME/voice-filter/` (`~/.local/state/voice-filter/`
//! by default): the devices one file each, the stages and their parameters in `chain`, the whole
//! filter switched off in `enabled`.

use ::std::fmt::Write as _;
use ::std::fs;
use ::std::io;
use ::std::path::{Path, PathBuf};
use ::std::sync::Arc;
use ::std::sync::atomic::{AtomicBool, Ordering};

use ::anyhow::{Context as _, Result};

use crate::param::Param;

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
const CHAIN: &str = "chain";
const LISTEN: &str = "listen";
const LISTEN_VOLUME: &str = "listen-volume";
const ENABLED: &str = "enabled";

/// Whether the whole filter was on (see [`crate::chain::Chain::enabled`]); on unless it was
/// switched off.
pub fn load_enabled(dir: &Path) -> Result<bool> {
    match load(dir, ENABLED)?.as_deref() {
        None | Some("on") => Ok(true),
        Some("off") => Ok(false),
        Some(value) => Err(::anyhow::anyhow!("{value} isn't on or off")),
    }
}

/// Keeps whether the whole filter is on; only off is written, as for the stages in `chain`.
pub fn save_enabled(dir: &Path, on: bool) -> Result<()> {
    if on {
        remove(dir, ENABLED)
    } else {
        save(dir, ENABLED, "off")
    }
}

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

/// Whether Listen is kept for the next runs, and if so whether it was on: `None` if it isn't.
pub fn load_listen(dir: &Path) -> Result<Option<bool>> {
    Ok(load(dir, LISTEN)?.map(|listen| listen == "on"))
}

/// Keeps whether Listen is on for the next runs, or with `None` forgets it, so it starts off.
pub fn save_listen(dir: &Path, on: Option<bool>) -> Result<()> {
    match on {
        Some(on) => save(dir, LISTEN, if on { "on" } else { "off" }),
        None => remove(dir, LISTEN),
    }
}

/// The volume of Listen chosen last, in dB.
pub fn load_listen_volume(dir: &Path) -> Result<Option<f32>> {
    load(dir, LISTEN_VOLUME)?
        .map(|volume| {
            volume
                .parse()
                .with_context(|| format!("{volume} isn't a number"))
        })
        .transpose()
}

pub fn save_listen_volume(dir: &Path, db: f32) -> Result<()> {
    save(dir, LISTEN_VOLUME, &db.to_string())
}

/// Keeps what differs from the defaults: the stages switched off (`denoise off`) and the
/// parameters not at their default (`denoise.limit 30`), one per line. A default changed in a
/// later version applies to everything the user didn't change.
pub fn save_chain(
    dir: &Path,
    switches: &[(&'static str, Arc<AtomicBool>)],
    params: &[(&'static str, Param)],
) -> Result<()> {
    let mut chain = String::new();
    for (stage, enabled) in switches {
        if !enabled.load(Ordering::Relaxed) {
            writeln!(chain, "{stage} off")?;
        }
    }
    for (stage, param) in params {
        let value = param.get();
        if value != param.default {
            writeln!(chain, "{stage}.{} {value}", param.name)?;
        }
    }
    save(dir, CHAIN, chain.trim_end())
}

/// Switches the stages and sets the parameters as [`save_chain`] kept them. What doesn't fit the
/// chain (a stage or parameter that's gone, a value out of range, a line it can't read) is skipped
/// and returned as a warning each, the rest still applies.
pub fn restore_chain(
    dir: &Path,
    switches: &[(&'static str, Arc<AtomicBool>)],
    params: &[(&'static str, Param)],
) -> Result<Vec<String>> {
    let Some(chain) = load(dir, CHAIN)? else {
        return Ok(Vec::new());
    };
    let mut warnings = Vec::new();
    for line in chain.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Err(warning) = restore_line(line, switches, params) {
            warnings.push(format!("Skipped “{line}” of the saved settings: {warning}"));
        }
    }
    Ok(warnings)
}

fn restore_line(
    line: &str,
    switches: &[(&'static str, Arc<AtomicBool>)],
    params: &[(&'static str, Param)],
) -> Result<(), String> {
    let Some((key, value)) = line.split_once(char::is_whitespace) else {
        return Err("no value".to_owned());
    };
    let value = value.trim();
    match key.split_once('.') {
        Some((stage, name)) => {
            let param = Param::find(params, stage, name)
                .ok_or_else(|| format!("no parameter {name} of {stage}"))?;
            let value = value
                .parse()
                .map_err(|_| format!("{value} isn't a number"))?;
            param.set(value).map_err(|err| err.to_string())
        }
        None => {
            let (_, enabled) = switches
                .iter()
                .find(|(stage, _)| *stage == key)
                .ok_or_else(|| format!("no stage {key}"))?;
            let on = match value {
                "on" => true,
                "off" => false,
                _ => return Err(format!("{value} isn't on or off")),
            };
            enabled.store(on, Ordering::Relaxed);
            Ok(())
        }
    }
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

fn remove(dir: &Path, name: &str) -> Result<()> {
    let path = dir.join(name);
    match fs::remove_file(&path) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => {
            Err(err).with_context(|| format!("Could not remove {}", path.display()))
        }
        _ => Ok(()),
    }
}

/// Writes a file next to it and renames that over it, so a crash never leaves half a setting
/// (a slider in the app saves many times while it's dragged).
fn save(dir: &Path, name: &str, value: &str) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("Could not create {}", dir.display()))?;
    let path = dir.join(name);
    let temporary = dir.join(format!(".{name}.tmp"));
    fs::write(&temporary, format!("{value}\n"))
        .with_context(|| format!("Could not write {}", temporary.display()))?;
    fs::rename(&temporary, &path).with_context(|| format!("Could not write {}", path.display()))
}
