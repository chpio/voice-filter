use ::std::sync::Arc;
use ::std::sync::atomic::{AtomicU64, Ordering};

use ::anyhow::{Result, ensure};
use ::deep_filter_rt::tract::{DfParams, DfTract, RuntimeParams};
use ::ndarray::Array2;

use crate::atomic::AtomicF32;
use crate::dsp::{db_to_gain, ramp};
use crate::echo::Sidechain;
use crate::level::{self, SpeechLevel};
use crate::param::Param;
use crate::stage::{Frame, Stage};
use crate::{FRAME_LEN, FRAME_S, SAMPLE_RATE};

/// Noise suppression with the embedded DeepFilterNet3 model. The model hears a quiet voice raised
/// to [`INPUT_LEVEL_DB`] (the parameter `level`), by at most [`MAX_INPUT_GAIN_DB`] (`max-gain`):
/// it takes a voice that's too quiet for noise. The gain comes off its output again, so the stages
/// after it get the level as before.
pub struct Denoise {
    df: DfTract,
    input_gain: InputGain,
    // [channels, samples] as DfTract wants them, allocated once
    noisy: Array2<f32>,
    enhanced: Array2<f32>,
    // the signal the gains go to in `process_with`
    apply: Array2<f32>,
    atten_lim: Param,
    /// The attenuation limit `df` has, to see changes of `atten_lim`.
    applied_atten_lim: f32,
    speech: Arc<Speech>,
    sidechain: Option<Arc<Sidechain>>,
    /// The frames of `sidechain` at the last frame, to see whether it has this one.
    sidechain_frames: u64,
    /// The share of the echo cancellation's output in what the gains went to in the last frame.
    mix_before: f32,
    /// Frames left to stay with your voice.
    voice_hold: usize,
}

/// How much speech the model hears, for the stages after it (AGC and expander): its estimate of
/// the local SNR of the frame it analyzes, in dB (−15 to 35), and how many frames it processed, so
/// a reader sees when it stopped (switched off, or the limit at 0 dB, where the model doesn't
/// run). The frame it analyzes is the newest, its output is
/// [`Stage::latency`] behind, so the estimate comes ahead of the audio it belongs to.
#[derive(Debug)]
pub struct Speech {
    snr_db: AtomicF32,
    frames: AtomicU64,
}

impl Speech {
    pub fn new() -> Self {
        Self {
            snr_db: AtomicF32::new(-15.),
            frames: AtomicU64::new(0),
        }
    }

    /// The local SNR of the last frame in dB.
    pub fn snr_db(&self) -> f32 {
        self.snr_db.load()
    }

    /// The frames the model processed so far.
    pub fn frames(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }

    pub fn update(&self, snr_db: f32) {
        self.snr_db.store(snr_db);
        self.frames.fetch_add(1, Ordering::Relaxed);
    }
}

impl Default for Speech {
    fn default() -> Self {
        Self::new()
    }
}

/// Reads a [`Speech`] once per frame, for a stage after the noise suppression: its estimate while
/// it runs, nothing while it's switched off.
#[derive(Debug)]
pub struct SpeechReader {
    speech: Arc<Speech>,
    /// The frames of `speech` at the last read.
    frames: u64,
}

impl SpeechReader {
    pub fn new(speech: Arc<Speech>) -> Self {
        let frames = speech.frames();
        Self { speech, frames }
    }

    /// The local SNR in dB of the frame the noise suppression processed since the last read,
    /// `None` if it didn't process one.
    pub fn read(&mut self) -> Option<f32> {
        let frames = self.speech.frames();
        let running = frames != self.frames;
        self.frames = frames;
        running.then(|| self.speech.snr_db())
    }
}

/// The default of the model: no limit.
pub const UNLIMITED: f32 = 100.;

/// The level the model hears the speech at by default, in dBFS (RMS), and how far it raises the
/// input for it at most, in dB.
pub const INPUT_LEVEL_DB: f32 = -35.;
pub const MAX_INPUT_GAIN_DB: f32 = 20.;
/// Quieter frames (RMS, dBFS) are never speech for [`InputGain`].
const INPUT_GATE_DB: f32 = -70.;
/// How fast the input gain may rise and fall, in dB per second.
const INPUT_RISE_DB_PER_S: f32 = 20.;
const INPUT_FALL_DB_PER_S: f32 = 30.;
/// The longest latency of the model [`InputGain`] supports, in frames.
const MAX_DELAY_FRAMES: usize = 8;

/// Raises the input of the model so it hears the speech at its `level`: the model judges by the
/// level of what it hears (its complex features grow with it) and takes too quiet speech for
/// noise, which it mutes, while the low rumble of a microphone (below the high-pass) made it take
/// pauses for speech. It measures the speech by its level ([`SpeechLevel`]), raises the input
/// (never lowers it, the model works on loud speech too) and takes the gain off the output again,
/// with the gain of the frame the output belongs to: the stages after it get the level as before.
struct InputGain {
    speech: SpeechLevel,
    /// The current gain, in dB.
    gain_db: f32,
    level: Param,
    max_gain: Param,
    /// The gains of the last frames as factors, at the start and the end of the frame (ramped in
    /// between); the next one goes to `next`.
    gains: [(f32, f32); MAX_DELAY_FRAMES + 1],
    next: usize,
    /// The latency of the model, in frames.
    delay: usize,
}

impl InputGain {
    fn new(delay: usize) -> Result<Self> {
        ensure!(
            delay <= MAX_DELAY_FRAMES,
            "the model's latency is {delay} frames, at most {MAX_DELAY_FRAMES} are supported"
        );
        Ok(Self {
            speech: SpeechLevel::new(INPUT_GATE_DB, INPUT_LEVEL_DB),
            gain_db: 0.,
            level: Param::new(
                "level",
                "Input Level",
                "dBFS",
                (-45., -20., 1.),
                INPUT_LEVEL_DB,
            ),
            max_gain: Param::new(
                "max-gain",
                "Maximum Input Gain",
                "dB",
                (0., 30., 1.),
                MAX_INPUT_GAIN_DB,
            ),
            gains: [(1., 1.); MAX_DELAY_FRAMES + 1],
            next: 0,
            delay,
        })
    }

    /// Measures the frame and returns its gain, and the gain of the frame whose output comes now,
    /// as factors at the start and the end of the frame.
    fn next(&mut self, frame: &Frame) -> ((f32, f32), (f32, f32)) {
        let level_db = self.speech.push(level::frame_db(frame), true);
        let wanted = (self.level.get() - level_db).clamp(0., self.max_gain.get());
        let previous = self.gain_db;
        self.gain_db = if wanted > previous {
            wanted.min(previous + INPUT_RISE_DB_PER_S * FRAME_S)
        } else {
            wanted.max(previous - INPUT_FALL_DB_PER_S * FRAME_S)
        };
        let gain = (db_to_gain(previous), db_to_gain(self.gain_db));
        let len = self.gains.len();
        self.gains[self.next] = gain;
        let output = self.gains[(self.next + len - self.delay) % len];
        self.next = (self.next + 1) % len;
        (gain, output)
    }
}

impl Denoise {
    pub fn new() -> Result<Self> {
        // Upstream's real-time defaults skip the decoders where the model estimates a high local
        // SNR, to save CPU: no gains above 30 dB, no deep filter above 20 dB. That cost most of
        // its quality: on the VoiceBank-DEMAND test set PESQ 2.07 instead of 2.70, STOI 0.861
        // (below the noisy input's 0.921) instead of 0.925, for 0.05 ms less per frame. The zeros
        // below −10 dB stay, upstream's default.
        let params = RuntimeParams::default();
        let zeros_below_db = params.min_db_thresh;
        let params = params.with_thresholds(zeros_below_db, f32::INFINITY, f32::INFINITY);
        let df = DfTract::new(DfParams::default(), &params)?;
        ensure!(
            df.sr == SAMPLE_RATE as usize && df.hop_size == FRAME_LEN && df.ch == 1,
            "the model expects {} Hz, {} samples per frame and {} channels",
            df.sr,
            df.hop_size,
            df.ch,
        );
        ensure!(
            df.delay() % FRAME_LEN == 0,
            "the model's latency of {} samples isn't whole frames",
            df.delay()
        );
        let input_gain = InputGain::new(df.delay() / FRAME_LEN)?;
        Ok(Self {
            df,
            input_gain,
            noisy: Array2::zeros((1, FRAME_LEN)),
            enhanced: Array2::zeros((1, FRAME_LEN)),
            apply: Array2::zeros((1, FRAME_LEN)),
            atten_lim: Param::new("limit", "Limit", "dB", (0., UNLIMITED, 1.), UNLIMITED),
            applied_atten_lim: UNLIMITED,
            speech: Arc::new(Speech::new()),
            sidechain: None,
            sidechain_frames: 0,
            mix_before: 1.,
            voice_hold: 0,
        })
    }

    /// Limits how much the model attenuates, in dB. It keeps part of the original everywhere,
    /// which sounds more natural, [`UNLIMITED`] (100 dB and above) doesn't limit it.
    /// Values above [`UNLIMITED`] mean the same, they're taken as it.
    pub fn with_atten_lim(self, db: f32) -> Self {
        self.atten_lim
            .set(db.clamp(0., UNLIMITED))
            .expect("clamped to the range");
        self
    }

    /// Takes the microphone before the echo cancellation and what the speakers play from it, so
    /// the echo cancellation doesn't turn down the voice where the model tells it from the echo by
    /// itself (see [`Stage::process`]).
    pub fn with_sidechain(mut self, sidechain: Arc<Sidechain>) -> Self {
        self.sidechain = Some(sidechain);
        self
    }

    /// How much speech the model hears, updated with every frame it processes.
    pub fn speech(&self) -> Arc<Speech> {
        self.speech.clone()
    }

    /// The attenuation limit in dB, to change it while the stage runs in the audio path; it takes
    /// effect with the next frame.
    pub fn atten_lim(&self) -> Param {
        self.atten_lim.clone()
    }

    /// How far it raises the input of the model now, in dB (see [`Denoise`]).
    pub fn input_gain_db(&self) -> f32 {
        self.input_gain.gain_db
    }
}

impl Stage for Denoise {
    fn name(&self) -> &'static str {
        "denoise"
    }

    fn latency(&self) -> usize {
        self.df.delay()
    }

    /// Its latency plus the frames the deep filter reaches back (`df_order - df_lookahead`), 6 for
    /// DeepFilterNet3. Measured the same with speech before a pause and silence after it: 3 frames
    /// still from before it, then 3 fading out (up to -31 dBFS).
    fn resume_frames(&self) -> usize {
        self.df.delay().div_ceil(FRAME_LEN) + self.df.df_order - self.df.df_lookahead
    }

    fn params(&self) -> Vec<Param> {
        vec![
            self.atten_lim.clone(),
            self.input_gain.level.clone(),
            self.input_gain.max_gain.clone(),
        ]
    }

    /// The local SNR the model estimated for the newest frame it analyzed, whose output comes
    /// its latency later; it stays as it was while the model doesn't run (at a limit of 0 dB).
    fn trace(&self) -> Option<(&'static str, f32)> {
        Some(("snr", self.speech.snr_db()))
    }

    /// With a [`Sidechain`] (see [`Denoise::with_sidechain`]) the model analyzes the echo
    /// cancellation's output as always, but where the echo cancellation's suppressor would take
    /// the voice with the echo, its gains go to a mix with the raw microphone instead: while the
    /// speakers play and you talk (above [`VOICE_DB`], held for [`VOICE_HOLD_FRAMES`]). Without
    /// the deep filter then: its coefficients are a filter for the analyzed spectrum. Switching
    /// crossfades the mix over a frame. While you're silent the echo cancellation acts fully: with
    /// the mix all the time, music came through in the pauses, 11 dB louder.
    fn process(&mut self, frame: &mut Frame) -> Result<()> {
        let Some(sidechain) = self.sidechain.clone() else {
            return self.process_with(frame, None, true);
        };
        // not this frame's while the echo cancellation is switched off
        let frames = sidechain.frames();
        let fresh = frames != self.sidechain_frames;
        self.sidechain_frames = frames;
        let protect = sidechain.protect();
        let active = fresh && protect > 0. && sidechain.playing() && self.voice_hold > 0;
        let mix = if active { 1. - protect } else { 1. };
        if mix >= 1. && self.mix_before >= 1. {
            self.process_with(frame, None, true)?;
        } else {
            let mut raw = *frame;
            if fresh {
                sidechain.raw(&mut raw);
            }
            let mut apply = [0.; FRAME_LEN];
            for (((a, e), r), m) in apply
                .iter_mut()
                .zip(frame.iter())
                .zip(&raw)
                .zip(ramp(self.mix_before, mix))
            {
                *a = m * e + (1. - m) * r;
            }
            self.process_with(frame, Some(&apply), !active)?;
        }
        self.mix_before = mix;
        self.voice_hold = if self.speech.snr_db() > VOICE_DB {
            VOICE_HOLD_FRAMES
        } else {
            self.voice_hold.saturating_sub(1)
        };
        Ok(())
    }
}

/// The estimate above which the sidechain takes it for your voice while the speakers play, in dB:
/// at 5 dB remains of speech from the speakers after the echo cancellation opened it too, and left
/// more of a quiet echo (−83 instead of −89 dBFS).
pub const VOICE_DB: f32 = 10.;
/// How long it stays with the voice after it, in frames (200 ms).
pub const VOICE_HOLD_FRAMES: usize = 20;

impl Denoise {
    /// [`Stage::process`], but with `apply` the model analyzes `frame` and its gains go to
    /// `apply` (see [`DfTract::process_with`]), the result in `frame`; `df` applies the deep
    /// filter too.
    pub fn process_with(
        &mut self,
        frame: &mut Frame,
        apply: Option<&Frame>,
        df: bool,
    ) -> Result<()> {
        let atten_lim = self.atten_lim.get();
        if atten_lim != self.applied_atten_lim {
            self.df.set_atten_lim(atten_lim);
            self.applied_atten_lim = atten_lim;
        }
        let (gain, output_gain) = self.input_gain.next(frame);
        let noisy = self.noisy.row_mut(0).into_slice().unwrap();
        for ((x, &y), g) in noisy.iter_mut().zip(frame.iter()).zip(ramp(gain.0, gain.1)) {
            *x = y * g;
        }
        if let Some(apply) = apply {
            let to = self.apply.row_mut(0).into_slice().unwrap();
            for ((x, &y), g) in to.iter_mut().zip(apply.iter()).zip(ramp(gain.0, gain.1)) {
                *x = y * g;
            }
        }
        let snr_db = self.df.process_with(
            self.noisy.view(),
            apply.map(|_| self.apply.view()),
            df,
            self.enhanced.view_mut(),
        )?;
        // at a limit of 0 dB the model doesn't run and has no estimate, the stages after it go by
        // the level as if it were switched off
        if self.df.attenuates() {
            self.speech.update(snr_db);
        }
        // the gain off again, that of the frame the output belongs to
        let enhanced = self.enhanced.row(0).to_slice().unwrap();
        for ((y, &x), g) in frame
            .iter_mut()
            .zip(enhanced)
            .zip(ramp(output_gain.0, output_gain.1))
        {
            *y = x / g;
        }
        Ok(())
    }
}
