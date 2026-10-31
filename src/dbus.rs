//! The control interface on the session bus, for the GNOME app and `busctl`: the settings, which
//! go to the audio path through atomics, and the status of the live filter.

use ::std::collections::HashMap;
use ::std::path::PathBuf;
use ::std::sync::atomic::{AtomicBool, Ordering};
use ::std::sync::{Arc, Mutex, mpsc};

use ::anyhow::{Context as _, Result, anyhow};
use ::zbus::{connection, fdo, interface};

use crate::SAMPLE_RATE;
use crate::atomic::AtomicF32;
use crate::bridge::Stats;
use crate::denoise::UNLIMITED;
use crate::live::Input;
use crate::settings;

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
    /// Plays what "Voice Filter" outputs on the default output, or stops it.
    SetListen(bool),
}

pub struct Service {
    pub switches: Vec<(&'static str, Arc<AtomicBool>)>,
    pub atten_lim: Arc<AtomicF32>,
    pub stats: Arc<Stats>,
    /// In samples.
    pub chain_latency: usize,
    /// The `node.name` of the microphone, empty for the default input.
    pub input: String,
    /// The microphones to choose from, kept up to date by the PipeWire loop.
    pub inputs: Arc<Mutex<Vec<Input>>>,
    /// Sends a [`Control`] to the PipeWire loop.
    pub send: Box<dyn Fn(Control) -> Result<()> + Send + Sync>,
    /// Whether the output is played on the default output, see [`Control::SetListen`].
    pub listening: bool,
    /// Where a chosen microphone is kept for the next runs.
    pub settings_dir: Option<PathBuf>,
}

fn ms(samples: i64) -> f64 {
    samples as f64 * 1e3 / SAMPLE_RATE as f64
}

/// The settings change the live filter immediately, the status properties (latency, counters)
/// don't emit change signals: they change every cycle, clients poll them.
#[interface(name = "net.b128.VoiceFilter1")]
impl Service {
    /// How much the noise suppression may attenuate, in dB, 0 to 100; 100 doesn't limit it.
    #[zbus(property)]
    pub fn attenuation_limit(&self) -> f64 {
        self.atten_lim.load() as f64
    }

    #[zbus(property)]
    pub fn set_attenuation_limit(&mut self, db: f64) -> fdo::Result<()> {
        if !(0. ..=UNLIMITED as f64).contains(&db) {
            return Err(fdo::Error::InvalidArgs(format!(
                "the attenuation limit must be 0 to {UNLIMITED} dB, not {db}"
            )));
        }
        self.atten_lim.store(db as f32);
        Ok(())
    }

    /// Whether what "Voice Filter" outputs is played on the default output, to hear the filter
    /// while tuning it; with headphones, speakers feed back into the microphone. It's off at the
    /// start.
    #[zbus(property)]
    pub fn listen(&self) -> bool {
        self.listening
    }

    #[zbus(property)]
    pub fn set_listen(&mut self, on: bool) -> fdo::Result<()> {
        (self.send)(Control::SetListen(on)).map_err(|err| fdo::Error::Failed(err.to_string()))?;
        self.listening = on;
        Ok(())
    }

    /// The stages in the order of the chain and whether they're on.
    #[zbus(property(emits_changed_signal = "false"))]
    pub fn stages(&self) -> Vec<(String, bool)> {
        self.switches
            .iter()
            .map(|(name, enabled)| (name.to_string(), enabled.load(Ordering::Relaxed)))
            .collect()
    }

    /// Switches a stage on or off; switching crossfades and doesn't change the latency.
    pub fn set_stage_enabled(&mut self, name: &str, enabled: bool) -> fdo::Result<()> {
        let (_, switch) = self
            .switches
            .iter()
            .find(|(stage, _)| *stage == name)
            .ok_or_else(|| fdo::Error::InvalidArgs(format!("no stage {name}")))?;
        switch.store(enabled, Ordering::Relaxed);
        Ok(())
    }

    /// The `node.name` of the microphone, empty for the default input. Setting it switches the
    /// filter to that microphone and keeps it for the next runs.
    #[zbus(property)]
    pub fn input(&self) -> String {
        self.input.clone()
    }

    #[zbus(property)]
    pub fn set_input(&mut self, name: String) -> fdo::Result<()> {
        let known = self
            .inputs
            .lock()
            .unwrap()
            .iter()
            .any(|input| input.name == name);
        if !name.is_empty() && !known {
            return Err(fdo::Error::InvalidArgs(format!("no input {name}")));
        }
        (self.send)(Control::SetInput(name.clone()))
            .map_err(|err| fdo::Error::Failed(err.to_string()))?;
        if let Some(dir) = &self.settings_dir {
            settings::save_input(dir, &name).map_err(|err| fdo::Error::Failed(err.to_string()))?;
        }
        self.input = name;
        Ok(())
    }

    /// The microphones to choose from: `node.name`, description and priority (the session
    /// manager falls back to the highest one), the highest first.
    #[zbus(property(emits_changed_signal = "false"))]
    pub fn inputs(&self) -> Vec<(String, String, i32)> {
        self.inputs
            .lock()
            .unwrap()
            .iter()
            .map(|input| {
                (
                    input.name.clone(),
                    input.description.clone(),
                    input.priority,
                )
            })
            .collect()
    }

    /// The latency in ms from the microphone to the app: `total` and its parts `microphone`,
    /// `bridge`, `chain` and `app`.
    #[zbus(property(emits_changed_signal = "false"))]
    pub fn latency(&self) -> HashMap<String, f64> {
        let stats = &self.stats;
        let parts = [
            ("microphone", stats.capture_delay.load(Ordering::Relaxed)),
            ("bridge", stats.bridge_delay.load(Ordering::Relaxed)),
            ("chain", self.chain_latency as i64),
            ("app", stats.playback_delay.load(Ordering::Relaxed)),
        ];
        let total = parts.iter().map(|(_, samples)| samples).sum();
        parts
            .into_iter()
            .chain([("total", total)])
            .map(|(name, samples)| (name.to_owned(), ms(samples)))
            .collect()
    }

    /// The samples per graph cycle.
    #[zbus(property(emits_changed_signal = "false"))]
    pub fn quantum(&self) -> u64 {
        self.stats.quantum.load(Ordering::Relaxed)
    }

    /// The longest processing time of a frame in ms, over the last 10 s.
    #[zbus(property(emits_changed_signal = "false"))]
    pub fn max_frame_time(&self) -> f64 {
        self.stats.last_max_frame_ns.load(Ordering::Relaxed) as f64 / 1e6
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
