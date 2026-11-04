//! The control interface on the session bus, for the GNOME app and `busctl`: the settings, which
//! go to the audio path through atomics, and the status of the live filter.

use ::std::collections::HashMap;
use ::std::path::{Path, PathBuf};
use ::std::sync::atomic::{AtomicBool, Ordering};
use ::std::sync::{Arc, Mutex, mpsc};

use ::anyhow::{Context as _, Result, anyhow};
use ::zbus::{connection, fdo, interface};

use crate::bridge::Stats;
use crate::echo::EchoStatus;
use crate::live::Device;
use crate::param::Param;
use crate::{samples_ms, settings};

/// The well-known name; only one process can own it, so it also keeps a second instance from
/// running.
/// It isn't `net.b128.VoiceFilter`: that's the ID of the GNOME app, which owns its ID as a bus name.
pub const NAME: &str = "net.b128.VoiceFilter.Daemon";
pub const PATH: &str = "/net/b128/VoiceFilter";

/// What the control interface asks the PipeWire loop to do, which owns the streams.
#[derive(Debug, PartialEq)]
pub enum Control {
    /// Reads from another microphone, the empty name for the default input.
    SetInput(String),
    /// Takes the reference of the echo canceller from another output device, the empty name for
    /// the default output.
    SetOutput(String),
    /// Plays what "Voice Filter" outputs on the default output, or stops it.
    SetListen(bool),
}

pub struct Service {
    /// The switch of the whole filter, see [`crate::chain::Chain::enabled`].
    pub enabled: Arc<AtomicBool>,
    /// Called after `enabled` changed, to update the icon in the tray.
    pub enabled_changed: Box<dyn Fn() + Send + Sync>,
    pub switches: Vec<(&'static str, Arc<AtomicBool>)>,
    /// The settings of the stages, with the name of their stage.
    pub params: Vec<(&'static str, Param)>,
    pub stats: Arc<Stats>,
    /// How the echo canceller does, if the chain has one.
    pub echo: Option<Arc<EchoStatus>>,
    /// In samples.
    pub chain_latency: usize,
    /// The `node.name` of the microphone, empty for the default input.
    pub input: String,
    /// The microphones to choose from, kept up to date by the PipeWire loop.
    pub inputs: Arc<Mutex<Vec<Device>>>,
    /// The `node.name` of the output device whose monitor is the reference of the echo
    /// canceller, empty for the default output.
    pub output: String,
    /// The output devices to choose from, kept up to date by the PipeWire loop.
    pub outputs: Arc<Mutex<Vec<Device>>>,
    /// Sends a [`Control`] to the PipeWire loop.
    pub send: Box<dyn Fn(Control) -> Result<()> + Send + Sync>,
    /// Whether the output is played on the default output, see [`Control::SetListen`].
    pub listening: bool,
    /// Whether `listening` is kept for the next runs.
    pub keep_listen: bool,
    /// The volume of Listen, in dB, see [`crate::volume`].
    pub listen_volume: Param,
    /// Where a chosen microphone and output device, the stages switched on or off and the
    /// parameters are kept for the next runs.
    pub settings_dir: Option<PathBuf>,
}

impl Service {
    /// Keeps a setting for the next runs with `save`, if there's a directory for the settings.
    fn save(&self, save: impl FnOnce(&Path) -> Result<()>) -> fdo::Result<()> {
        match &self.settings_dir {
            Some(dir) => save(dir).map_err(failed),
            None => Ok(()),
        }
    }

    /// Keeps whether Listen is on, if [`Self::keep_listen`], or forgets it.
    fn save_listen(&self) -> fdo::Result<()> {
        self.save(|dir| settings::save_listen(dir, self.keep_listen.then_some(self.listening)))
    }

    /// Keeps the stages and parameters as they are now for the next runs.
    fn save_chain(&self) -> fdo::Result<()> {
        self.save(|dir| settings::save_chain(dir, &self.switches, &self.params))
    }
}

/// Fails unless `name` is one of `devices` or empty, for the default device.
fn check_device(name: &str, devices: &Mutex<Vec<Device>>) -> fdo::Result<()> {
    let known = devices
        .lock()
        .unwrap()
        .iter()
        .any(|device| device.name == name);
    if name.is_empty() || known {
        Ok(())
    } else {
        Err(fdo::Error::InvalidArgs(format!("no device {name}")))
    }
}

/// A failure as a D-Bus error, with its causes.
fn failed(err: ::anyhow::Error) -> fdo::Error {
    fdo::Error::Failed(format!("{err:#}"))
}

/// A parameter on the bus: stage, name, title, unit, minimum, maximum, step, default and value.
pub type Parameter = (String, String, String, String, f64, f64, f64, f64, f64);

/// `node.name`, description and priority of the devices.
fn listed(devices: &Mutex<Vec<Device>>) -> Vec<(String, String, i32)> {
    devices
        .lock()
        .unwrap()
        .iter()
        .map(|device| {
            (
                device.name.clone(),
                device.description.clone(),
                device.priority,
            )
        })
        .collect()
}

/// The settings change the live filter immediately, the status properties (latency, counters)
/// don't emit change signals: they change every cycle, clients poll them.
#[interface(name = "net.b128.VoiceFilter1")]
impl Service {
    /// The settings of the stages, in the order of the chain: stage, name, title, unit, minimum,
    /// maximum, step, default and value. Set with [`Self::set_parameter`], they don't emit change
    /// signals either.
    #[zbus(property(emits_changed_signal = "false"))]
    pub fn parameters(&self) -> Vec<Parameter> {
        self.params
            .iter()
            .map(|(stage, param)| {
                (
                    stage.to_string(),
                    param.name.to_owned(),
                    param.title.to_owned(),
                    param.unit.to_owned(),
                    param.min as f64,
                    param.max as f64,
                    param.step as f64,
                    param.default as f64,
                    param.get() as f64,
                )
            })
            .collect()
    }

    /// Sets a setting of a stage, within its range; the stage applies it with the next frame. Kept
    /// for the next runs.
    pub fn set_parameter(&mut self, stage: &str, name: &str, value: f64) -> fdo::Result<()> {
        let param = Param::find(&self.params, stage, name)
            .ok_or_else(|| fdo::Error::InvalidArgs(format!("no parameter {name} of {stage}")))?;
        param
            .set(value as f32)
            .map_err(|err| fdo::Error::InvalidArgs(err.to_string()))?;
        self.save_chain()
    }

    /// Whether what "Voice Filter" outputs is played on the default output, to hear the filter
    /// while tuning it; with headphones, speakers feed back into the microphone. It's off at the
    /// start, unless [`Self::keep_listen`] kept it on.
    #[zbus(property)]
    pub fn listen(&self) -> bool {
        self.listening
    }

    #[zbus(property)]
    pub fn set_listen(&mut self, on: bool) -> fdo::Result<()> {
        (self.send)(Control::SetListen(on)).map_err(failed)?;
        self.listening = on;
        if self.keep_listen {
            self.save_listen()?;
        }
        Ok(())
    }

    /// Whether [`Self::listen`] is kept for the next runs. Off by default: while listening, the
    /// filter runs and the microphone is in use all the time.
    #[zbus(property)]
    pub fn keep_listen(&self) -> bool {
        self.keep_listen
    }

    #[zbus(property)]
    pub fn set_keep_listen(&mut self, keep: bool) -> fdo::Result<()> {
        self.keep_listen = keep;
        self.save_listen()
    }

    /// The volume of Listen in dB, −40 to 0. Kept for the next runs.
    #[zbus(property)]
    pub fn listen_volume(&self) -> f64 {
        self.listen_volume.get() as f64
    }

    #[zbus(property)]
    pub fn set_listen_volume(&mut self, db: f64) -> fdo::Result<()> {
        self.listen_volume
            .set(db as f32)
            .map_err(|err| fdo::Error::InvalidArgs(err.to_string()))?;
        self.save(|dir| settings::save_listen_volume(dir, db as f32))
    }

    /// Whether the filter is on. Off, every stage is, as if switched off by
    /// [`Self::set_stage_enabled`] (crossfaded, at the same latency), while [`Self::stages`] keeps
    /// what each stage's own switch says. Kept for the next runs.
    #[zbus(property(emits_changed_signal = "false"))]
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    #[zbus(property)]
    pub fn set_enabled(&mut self, on: bool) -> fdo::Result<()> {
        self.enabled.store(on, Ordering::Relaxed);
        (self.enabled_changed)();
        self.save(|dir| settings::save_enabled(dir, on))
    }

    /// The stages in the order of the chain and whether they're on.
    #[zbus(property(emits_changed_signal = "false"))]
    pub fn stages(&self) -> Vec<(String, bool)> {
        self.switches
            .iter()
            .map(|(name, enabled)| (name.to_string(), enabled.load(Ordering::Relaxed)))
            .collect()
    }

    /// Switches a stage on or off; switching crossfades and doesn't change the latency. Kept for
    /// the next runs.
    pub fn set_stage_enabled(&mut self, name: &str, enabled: bool) -> fdo::Result<()> {
        let (_, switch) = self
            .switches
            .iter()
            .find(|(stage, _)| *stage == name)
            .ok_or_else(|| fdo::Error::InvalidArgs(format!("no stage {name}")))?;
        switch.store(enabled, Ordering::Relaxed);
        self.save_chain()
    }

    /// The `node.name` of the microphone, empty for the default input. Setting it switches the
    /// filter to that microphone and keeps it for the next runs.
    #[zbus(property)]
    pub fn input(&self) -> String {
        self.input.clone()
    }

    #[zbus(property)]
    pub fn set_input(&mut self, name: String) -> fdo::Result<()> {
        check_device(&name, &self.inputs)?;
        (self.send)(Control::SetInput(name.clone())).map_err(failed)?;
        // it reads from it now, even if keeping it fails
        self.input = name;
        self.save(|dir| settings::save_input(dir, &self.input))
    }

    /// The microphones to choose from: `node.name`, description and priority (the session
    /// manager falls back to the highest one), the highest first.
    #[zbus(property(emits_changed_signal = "false"))]
    pub fn inputs(&self) -> Vec<(String, String, i32)> {
        listed(&self.inputs)
    }

    /// The output device whose monitor is the reference of the echo canceller, its `node.name`,
    /// empty for the default output. Setting it switches the reference and keeps it for the next
    /// runs.
    #[zbus(property)]
    pub fn output(&self) -> String {
        self.output.clone()
    }

    #[zbus(property)]
    pub fn set_output(&mut self, name: String) -> fdo::Result<()> {
        check_device(&name, &self.outputs)?;
        (self.send)(Control::SetOutput(name.clone())).map_err(failed)?;
        self.output = name;
        self.save(|dir| settings::save_output(dir, &self.output))
    }

    /// The output devices to choose from, like [`Self::inputs`].
    #[zbus(property(emits_changed_signal = "false"))]
    pub fn outputs(&self) -> Vec<(String, String, i32)> {
        listed(&self.outputs)
    }

    /// The latency in ms from the microphone to the app: `total` and its parts `microphone`,
    /// `bridge`, `chain` and `app`.
    #[zbus(property(emits_changed_signal = "false"))]
    pub fn latency(&self) -> HashMap<String, f64> {
        let parts = self.stats.latency(self.chain_latency);
        let total = parts.iter().map(|(_, samples)| samples).sum();
        parts
            .into_iter()
            .chain([("total", total)])
            .map(|(name, samples)| (name.to_owned(), samples_ms(samples as f64)))
            .collect()
    }

    /// How the echo cancellation does: `capture-frames` and `reference-frames` (counters; the
    /// first stops while it's off or no app records, the second while the speakers are idle),
    /// `reference` (the peak level of what the speakers play in dBFS, falling by 60 dB/s), `erle`
    /// and `erl` (as AEC3 reports them, of its linear filter only: they say little about how much
    /// echo it removes in all, see [`EchoStatus::erle_db`]) and `delay` (from the speakers to the
    /// microphone, in ms, in steps of 4 ms). Empty without echo cancellation.
    #[zbus(property(emits_changed_signal = "false"))]
    pub fn echo(&self) -> HashMap<String, f64> {
        let Some(echo) = &self.echo else {
            return HashMap::new();
        };
        [
            ("capture-frames", echo.capture_frames() as f64),
            ("reference-frames", echo.reference_frames() as f64),
            ("reference", echo.reference_db() as f64),
            ("erle", echo.erle_db() as f64),
            ("erl", echo.erl_db() as f64),
            ("delay", echo.delay_ms() as f64),
        ]
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect()
    }

    /// The samples per graph cycle.
    #[zbus(property(emits_changed_signal = "false"))]
    pub fn quantum(&self) -> u64 {
        self.stats.quantum.load(Ordering::Relaxed)
    }

    /// The longest processing time of a frame in ms, over the last 10 s (updated every second);
    /// the `max` of `total` in
    /// [`Self::frame_times`].
    #[zbus(property(emits_changed_signal = "false"))]
    pub fn max_frame_time(&self) -> f64 {
        self.stats.frame_times[0].last().max_ns as f64 / 1e6
    }

    /// The processing time of a frame over the last 10 s (updated every second), in ms: part,
    /// mean, 95th and 99th
    /// percentile (rounded up by at most 1.6 %) and the longest. The parts are `total`,
    /// `echo`, `denoise` (0 when the chain doesn't have them) and `other`, the stages
    /// after them and what's measured around the chain.
    #[zbus(property(emits_changed_signal = "false"))]
    pub fn frame_times(&self) -> Vec<(String, f64, f64, f64, f64)> {
        let ms = |ns: u64| ns as f64 / 1e6;
        self.stats
            .frame_times()
            .map(|(part, time)| {
                (
                    part.to_owned(),
                    ms(time.mean_ns),
                    ms(time.p95_ns),
                    ms(time.p99_ns),
                    ms(time.max_ns),
                )
            })
            .collect()
    }

    /// The levels in dBFS: `input` and `output`, of the microphone and of what the filter outputs
    /// (RMS per frame, rising at once and falling by 20 dB/s), and `input-floor`, the noise floor
    /// of the microphone (its quietest frame of the last 2 s). They only change while the filter
    /// runs, see the `frames` of [`Self::counters`].
    #[zbus(property(emits_changed_signal = "false"))]
    pub fn levels(&self) -> HashMap<String, f64> {
        let stats = &self.stats;
        [
            ("input", &stats.input_db),
            ("output", &stats.output_db),
            ("input-floor", &stats.input_floor_db),
        ]
        .into_iter()
        .map(|(name, level)| (name.to_owned(), level.load() as f64))
        .collect()
    }

    /// Since the start: frames processed, samples of silence played because the chain wasn't
    /// ahead, samples dropped to bring the latency down and frames the chain failed on.
    #[zbus(property(emits_changed_signal = "false"))]
    pub fn counters(&self) -> HashMap<String, u64> {
        let stats = &self.stats;
        [
            ("frames", &stats.frames),
            ("silence", &stats.underruns),
            ("dropped", &stats.dropped),
            ("errors", &stats.errors),
        ]
        .into_iter()
        .map(|(name, counter)| (name.to_owned(), counter.load(Ordering::Relaxed)))
        .collect()
    }
}

/// Serves `service` under [`NAME`] on the session bus from a thread of its own, for as long as
/// the program runs. Fails if the name is taken, i.e. another instance runs.
pub fn serve(service: Service) -> Result<()> {
    let (result_tx, result_rx) = mpsc::channel();
    ::std::thread::Builder::new()
        .name("dbus".into())
        .spawn(move || {
            ::zbus::block_on(async move {
                // zbus requests names with "allow replacement" and "replace existing" by
                // default: a second instance would take the name from the first
                let connection = connection::Builder::session()
                    .allow_name_replacements(false)
                    .replace_existing_names(false)
                    .name(NAME)
                    .serve_at(PATH, service)
                    .build()
                    .await;
                match connection {
                    Ok(connection) => {
                        let _ = result_tx.send(Ok(()));
                        // serves until the program ends
                        ::std::future::pending::<()>().await;
                        drop(connection);
                    }
                    Err(err) => {
                        let _ = result_tx.send(Err(err));
                    }
                }
            })
        })
        .context("Could not start the D-Bus thread")?;
    match result_rx.recv()? {
        Ok(()) => Ok(()),
        Err(::zbus::Error::NameTaken) => Err(anyhow!("Voice Filter is already running")),
        Err(err) => Err(err).context("Could not provide the D-Bus interface"),
    }
}
