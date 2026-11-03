use ::std::sync::Arc;
use ::std::sync::atomic::{AtomicBool, Ordering};
use ::std::time::Instant;

use ::anyhow::Result;

use crate::FRAME_LEN;
use crate::agc::Agc;
use crate::compressor::Compressor;
use crate::deesser::DeEsser;
use crate::denoise::Denoise;
use crate::dsp::{ramp, without_denormals};
use crate::echo::{Echo, Reference};
use crate::equalizer::Equalizer;
use crate::expander::Expander;
use crate::highpass::HighPass;
use crate::limiter::Limiter;
use crate::param::Param;
use crate::stage::{Frame, Stage};

/// The chain of the filter, as `run` and `process` use it: highpass, echo, denoise, eq, deesser,
/// agc, compressor, expander and limiter, the noise suppression with the attenuation limit
/// `atten_lim` (dB), if given. With the reference to feed the echo canceller.
///
/// The high-pass comes first: the low rumble of a microphone made the noise suppression take
/// pauses for speech (AEC3 filters it out too, but not while the echo cancellation is off), and
/// the microphone the echo cancellation hands the noise suppression is filtered with it.
pub fn build(atten_lim: Option<f32>) -> Result<(Chain, Reference)> {
    let denoise = Denoise::new()?;
    build_with(match atten_lim {
        Some(db) => denoise.with_atten_lim(db),
        None => denoise,
    })
}

/// [`build`] with a noise suppression the caller set up, e.g. with other thresholds for
/// measurements ([`Denoise::with_thresholds`]).
pub fn build_with(denoise: Denoise) -> Result<(Chain, Reference)> {
    let (echo, reference) = Echo::new();
    let denoise = denoise.with_sidechain(echo.sidechain());
    // the estimate of the noise suppression comes its latency ahead of the audio
    let agc = Agc::new().listening(denoise.speech(), denoise.latency() / FRAME_LEN)?;
    let compressor = Compressor::new().following(agc.target());
    let expander = Expander::new().listening(denoise.speech());
    let chain = Chain::new(vec![
        Box::new(HighPass::new()),
        Box::new(echo),
        Box::new(denoise),
        Box::new(Equalizer::new()),
        Box::new(DeEsser::new()),
        Box::new(agc),
        Box::new(compressor),
        Box::new(expander),
        Box::new(Limiter::new()),
    ]);
    Ok((chain, reference))
}

/// The stages in the order the signal runs through them. Each one can be switched off at runtime
/// (see [`Chain::switches`]) without changing the latency: its input is played instead, delayed by
/// its latency, and switching crossfades over a frame. A switched off stage doesn't process, so it
/// costs nothing; switched on again, it resumes from its state before, first for
/// [`Stage::resume_frames`] without being played, so nothing from before the pause is heard.
pub struct Chain {
    slots: Vec<Slot>,
    /// The processing time of the last frame of each stage given to [`Chain::time`], in ns.
    times: Vec<u64>,
}

struct Slot {
    stage: Box<dyn Stage>,
    enabled: Arc<AtomicBool>,
    /// How much of the stage's output is played, 0 to 1; moves to `enabled` over a frame.
    mix: f32,
    /// The stage's input of the last `latency` samples, followed by room for a frame.
    dry: Vec<f32>,
    /// The stage didn't process the last frame.
    paused: bool,
    /// The frames the stage still runs without being played, after a pause.
    resuming: usize,
    /// Where its processing time goes in `Chain::times`, if it's timed.
    timer: Option<usize>,
}

impl Chain {
    pub fn new(stages: Vec<Box<dyn Stage>>) -> Self {
        let slots = stages
            .into_iter()
            .map(|stage| Slot {
                dry: vec![0.; stage.latency() + FRAME_LEN],
                stage,
                enabled: Arc::new(AtomicBool::new(true)),
                mix: 1.,
                paused: false,
                resuming: 0,
                timer: None,
            })
            .collect();
        Self {
            slots,
            times: Vec::new(),
        }
    }

    /// Measures the processing time of the stages named `names` on every frame (only of these:
    /// reading the clock costs about 20 ns), see [`Self::times`]. Neighbors share the time
    /// between them, so timing the first stages takes one reading per stage and one more.
    pub fn time(&mut self, names: &[&str]) {
        self.times = vec![0; names.len()];
        for slot in &mut self.slots {
            slot.timer = names.iter().position(|name| *name == slot.stage.name());
        }
    }

    /// The processing time of the last frame in ns of each stage given to [`Self::time`], in that
    /// order, with the switching and crossfading around it; 0 for a name the chain doesn't have.
    pub fn times(&self) -> &[u64] {
        &self.times
    }

    pub fn stages(&self) -> impl Iterator<Item = &dyn Stage> {
        self.slots.iter().map(|slot| &*slot.stage)
    }

    /// For callers that run the stages one by one, e.g. to time each of them; they run without
    /// the switches.
    pub fn stages_mut(&mut self) -> impl Iterator<Item = &mut Box<dyn Stage>> {
        self.slots.iter_mut().map(|slot| &mut slot.stage)
    }

    /// The name and the on/off switch of every stage, to change them while the chain runs in the
    /// audio path.
    pub fn switches(&self) -> Vec<(&'static str, Arc<AtomicBool>)> {
        self.slots
            .iter()
            .map(|slot| (slot.stage.name(), slot.enabled.clone()))
            .collect()
    }

    /// The settings of every stage, with the name of the stage, to change them while the chain
    /// runs in the audio path.
    pub fn params(&self) -> Vec<(&'static str, Param)> {
        self.slots
            .iter()
            .flat_map(|slot| {
                let name = slot.stage.name();
                slot.stage
                    .params()
                    .into_iter()
                    .map(move |param| (name, param))
            })
            .collect()
    }

    /// The delay of the whole chain, in samples, the same whether stages are on or off.
    pub fn latency(&self) -> usize {
        self.slots.iter().map(|slot| slot.stage.latency()).sum()
    }

    /// Runs the frame through the stages, with denormals flushed ([`without_denormals`]).
    pub fn process(&mut self, frame: &mut Frame) -> Result<()> {
        without_denormals(|| {
            // the end of the timed stage before, the start of this one
            let mut clock = None;
            for slot in &mut self.slots {
                match slot.timer {
                    Some(timer) => {
                        // Instant::now goes through the vDSO, no syscall
                        let started = clock.unwrap_or_else(Instant::now);
                        slot.process(frame)?;
                        let now = Instant::now();
                        self.times[timer] = (now - started).as_nanos() as u64;
                        clock = Some(now);
                    }
                    None => {
                        clock = None;
                        slot.process(frame)?;
                    }
                }
            }
            Ok(())
        })
    }
}

impl Slot {
    fn process(&mut self, frame: &mut Frame) -> Result<()> {
        let enabled = self.enabled.load(Ordering::Relaxed);
        if enabled && self.paused {
            self.paused = false;
            self.resuming = self.stage.resume_frames();
        }
        // the input delayed by the stage's latency is the oldest FRAME_LEN of the kept samples;
        // kept while the stage is on too, so switching it off plays the right samples
        let latency = self.dry.len() - FRAME_LEN;
        self.dry[latency..].copy_from_slice(frame);
        // off, it still runs for the frame that fades it out
        if enabled || self.mix > 0. {
            self.stage.process(frame)?;
            let target = if enabled && self.resuming == 0 {
                1.
            } else {
                0.
            };
            self.resuming = self.resuming.saturating_sub(1);
            if target != 1. || self.mix != 1. {
                let mixes = ramp(self.mix, target);
                for ((wet, dry), mix) in frame.iter_mut().zip(&self.dry).zip(mixes) {
                    *wet = *wet * mix + dry * (1. - mix);
                }
            }
            self.mix = target;
        } else {
            self.paused = true;
            frame.copy_from_slice(&self.dry[..FRAME_LEN]);
        }
        self.dry.copy_within(FRAME_LEN.., 0);
        Ok(())
    }
}
