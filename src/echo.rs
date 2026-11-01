use ::std::sync::Arc;
use ::std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use ::anyhow::{Result, anyhow, ensure};
use ::deep_filter_rt::tract::DfTract;
use ::ndarray::Array2;
use ::rtrb::{Consumer, Producer, RingBuffer};
use ::sonora::config::{EchoCanceller, MaxProcessingRate, Pipeline};
use ::sonora::{AudioProcessing, Config, StreamConfig};

use crate::atomic::AtomicF32;
use crate::level;
use crate::param::Param;
use crate::stage::{Frame, Stage};
use crate::{FRAME_LEN, SAMPLE_RATE};

/// Removes the echo of the speakers from the microphone: AEC3, the echo canceller of WebRTC
/// ([sonora](https://github.com/dignifiedquire/sonora), in pure Rust). It needs what the speakers
/// play as a reference, which it gets through a [`Reference`] (in the live mode the monitor of the
/// output device); it learns from it how the room carries the sound to the microphone, estimates
/// the delay on its way and subtracts the echo. Without a reference (with headphones, or when
/// nothing plays) it leaves the microphone as it is.
///
/// Only the echo canceller of sonora runs, its noise suppression and gain control stay off: the
/// stages after it do that. It runs first, so the other stages don't take the echo for speech.
///
/// While you talk into the playback, AEC3's suppressor turns down the voice with what's left of
/// the echo. So it also hands the noise suppression the microphone as it was (a [`Sidechain`]),
/// and what the speakers play ([`Playback`]): music the noise suppression removes by itself, it
/// then applies its gains mostly to the raw microphone; speech from the speakers it can't tell from
/// yours, it does so only while you talk. Whether the speakers play speech it hears with
/// [`Echo::detecting`].
pub struct Echo {
    apm: AudioProcessing,
    reference: Consumer<f32>,
    /// A frame of the reference, of the input and the output of the render side (unused).
    render: [f32; FRAME_LEN],
    render_out: [f32; FRAME_LEN],
    capture: [f32; FRAME_LEN],
    status: Arc<EchoStatus>,
    /// The microphone of the last [`LATENCY`] samples, followed by room for a frame: delayed by
    /// the latency it lines up with the output.
    delay: [f32; LATENCY + FRAME_LEN],
    sidechain: Arc<Sidechain>,
    tracker: PlaybackTracker,
    /// Frames of the microphone in a row without a frame of the reference.
    idle: usize,
    detector: Option<Detector>,
    /// The mean estimate of the detector above which the speakers play speech, in dB.
    speech_db: Param,
}

/// What the speakers play, as the echo cancellation hears it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Playback {
    /// Nothing, or silence for a second.
    Silent = 0,
    /// Speech (a call, a video), or what it doesn't know yet: it starts as speech.
    Speech = 1,
    /// Something without speech, such as music, for [`SPEECH_HOLD_FRAMES`].
    Music = 2,
}

impl Playback {
    fn from_u32(value: u32) -> Self {
        match value {
            1 => Self::Speech,
            2 => Self::Music,
            _ => Self::Silent,
        }
    }
}

/// What the echo cancellation hands the noise suppression: the microphone before it, delayed by
/// its latency, with every frame it processes (a counter tells whether it's this frame's: it isn't
/// while the stage is switched off), what the speakers play and how much of the voice it protects.
#[derive(Debug)]
pub struct Sidechain {
    frames: AtomicU64,
    raw: [AtomicF32; FRAME_LEN],
    status: Arc<EchoStatus>,
    protect: Param,
}

impl Sidechain {
    /// The frames the echo cancellation handed over.
    pub fn frames(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }

    /// The microphone of the last frame, delayed by the echo cancellation's latency.
    pub fn raw(&self, out: &mut Frame) {
        for (x, slot) in out.iter_mut().zip(&self.raw) {
            *x = slot.load();
        }
    }

    pub fn playback(&self) -> Playback {
        self.status.playback()
    }

    /// The share of the raw microphone in what the noise suppression's gains go to, 0 to 1 (0:
    /// the echo cancellation's output as it is).
    pub fn protect(&self) -> f32 {
        self.protect.get() / 100.
    }
}

/// DeepFilterNet3's encoder on what the speakers play: its estimate of the local SNR tells speech
/// from music (which it takes for noise). Only the encoder, without the decoders, the deep filter
/// and the synthesis ([`DfTract::analyze`]).
struct Detector {
    df: DfTract,
    frame: Array2<f32>,
}

impl Detector {
    fn analyze(&mut self, frame: &Frame) -> Result<f32> {
        self.frame
            .row_mut(0)
            .as_slice_mut()
            .expect("contiguous")
            .copy_from_slice(frame);
        self.df.analyze(self.frame.view())
    }
}

/// Decides from the level of the reference and the detector's estimates what the speakers play,
/// frame by frame. The detector runs in runs of [`RUN_FRAMES`] of every [`PERIOD_FRAMES`]
/// (consecutive frames, so it hears the signal as it is: on every fourth frame it hardly recognized
/// speech anymore); a run whose mean estimate is above the threshold is speech. Asymmetric:
/// speech as soon as a run has it and when the playback starts, music only after
/// [`SPEECH_HOLD_FRAMES`] without; mistaken for music, the other side would hear itself, mistaken
/// for speech, the voice only loses a bit while you talk. On a video with speech it was music for
/// 2 of 84 s (in a pause of its speech), on two pieces of music speech for 10 s from their start
/// and once for 10 s in 45 s.
#[derive(Debug)]
pub struct PlaybackTracker {
    playback: Playback,
    /// Silent frames in a row.
    silent: usize,
    /// Frames with sound left until music.
    hold: usize,
    /// The frame within the period of the detector.
    phase: usize,
    /// The sum of the estimates of the current run and how many.
    run: (f32, usize),
}

/// Below this level (dBFS) a frame of the reference counts as silent.
pub const SILENT_DB: f32 = -70.;
/// Silent frames in a row after which the speakers are silent (1 s).
pub const SILENCE_FRAMES: usize = 100;
/// The frames the detector analyzes in a row, and the period they repeat in: a quarter of the
/// time, about 0.3 % of a core while the speakers play. Runs of 25 recognized speech better than
/// runs of 10 of 40 at the same cost.
pub const RUN_FRAMES: usize = 25;
pub const PERIOD_FRAMES: usize = 100;
/// Frames with sound but without speech until it's music (10 s): the pauses of a call shouldn't
/// count, music plays on. With 5 s the video was music for 15 of 84 s.
pub const SPEECH_HOLD_FRAMES: usize = 1000;

impl Default for PlaybackTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl PlaybackTracker {
    pub fn new() -> Self {
        Self {
            playback: Playback::Silent,
            silent: SILENCE_FRAMES,
            hold: 0,
            phase: 0,
            run: (0., 0),
        }
    }

    /// The speakers stopped playing (no reference for a while): silent at once.
    pub fn stopped(&mut self) -> Playback {
        self.silent = SILENCE_FRAMES;
        self.playback = Playback::Silent;
        self.playback
    }

    /// Whether the detector should analyze the next frame, at this level.
    pub fn analyzes(&self, level_db: f32) -> bool {
        level_db >= SILENT_DB && self.phase < RUN_FRAMES
    }

    /// Takes a frame of the reference: its level, the detector's estimate if it analyzed it, the
    /// threshold for speech; returns what the speakers play.
    pub fn frame(&mut self, level_db: f32, snr_db: Option<f32>, threshold_db: f32) -> Playback {
        self.phase = (self.phase + 1) % PERIOD_FRAMES;
        if level_db < SILENT_DB {
            self.silent += 1;
            if self.silent >= SILENCE_FRAMES {
                self.playback = Playback::Silent;
            }
        } else {
            if self.playback == Playback::Silent {
                // it starts as speech: it may be a call
                self.hold = SPEECH_HOLD_FRAMES;
                self.run = (0., 0);
            }
            self.silent = 0;
            self.hold = self.hold.saturating_sub(1);
        }
        if let Some(snr) = snr_db {
            self.run.0 += snr;
            self.run.1 += 1;
        }
        if self.phase == RUN_FRAMES && self.run.1 > 0 {
            if self.run.0 / self.run.1 as f32 > threshold_db {
                self.hold = SPEECH_HOLD_FRAMES;
            }
            self.run = (0., 0);
        }
        if self.playback != Playback::Silent || self.silent == 0 {
            self.playback = if self.hold > 0 {
                Playback::Speech
            } else {
                Playback::Music
            };
        }
        self.playback
    }
}

/// What the speakers play, handed to the echo canceller from another thread.
pub struct Reference {
    producer: Producer<f32>,
    status: Arc<EchoStatus>,
}

/// How the echo canceller does, for the control interface, updated with every frame: whether it
/// runs and gets a reference (frame counters, a reader compares them with its last look), how loud
/// the reference is and what AEC3 measures.
#[derive(Debug)]
pub struct EchoStatus {
    capture_frames: AtomicU64,
    reference_frames: AtomicU64,
    reference_db: AtomicF32,
    erle_db: AtomicF32,
    erl_db: AtomicF32,
    delay_ms: AtomicF32,
    playback: AtomicU32,
}

/// How fast the level of the reference falls after a peak, in dB per frame: 60 dB per second, so
/// it shows what played within the last second or so.
const REFERENCE_DECAY_DB: f32 = 0.6;

impl EchoStatus {
    fn new() -> Self {
        Self {
            capture_frames: AtomicU64::new(0),
            reference_frames: AtomicU64::new(0),
            reference_db: AtomicF32::new(-120.),
            erle_db: AtomicF32::new(0.),
            erl_db: AtomicF32::new(0.),
            delay_ms: AtomicF32::new(0.),
            playback: AtomicU32::new(Playback::Silent as u32),
        }
    }

    /// What the speakers play, from the reference; [`Playback::Speech`] whenever they play
    /// without [`Echo::detecting`].
    pub fn playback(&self) -> Playback {
        Playback::from_u32(self.playback.load(Ordering::Relaxed))
    }

    /// The frames of the microphone it processed; they stop while it's off or no app records.
    pub fn capture_frames(&self) -> u64 {
        self.capture_frames.load(Ordering::Relaxed)
    }

    /// The frames of the reference it took; they stop while the speakers are idle.
    pub fn reference_frames(&self) -> u64 {
        self.reference_frames.load(Ordering::Relaxed)
    }

    /// The peak level of the reference (RMS per frame) in dBFS, falling by 60 dB per second.
    pub fn reference_db(&self) -> f32 {
        self.reference_db.load()
    }

    /// Echo return loss enhancement as AEC3 reports it, in dB: what its linear filter removes, not
    /// what it removes in all with its suppressor (a recording it reported 5–7 dB for lost 30 dB of
    /// echo).
    pub fn erle_db(&self) -> f32 {
        self.erle_db.load()
    }

    /// Echo return loss as AEC3 reports it, in dB: how much quieter the echo reaches the
    /// microphone than the speakers play, as its linear filter sees it (it stays at its initial
    /// 20 dB until the filter is sure).
    pub fn erl_db(&self) -> f32 {
        self.erl_db.load()
    }

    /// The delay from the speakers to the microphone AEC3 estimates, in ms, in steps of 4 ms (0 for
    /// less, e.g. with a USB microphone close to the speakers).
    pub fn delay_ms(&self) -> f32 {
        self.delay_ms.load()
    }
}

/// The reference the ring holds at most, in samples: 340 ms, more than the stages ever lag behind
/// (the echo canceller only takes whole frames).
const REFERENCE_CAPACITY: usize = 16384;

impl Echo {
    /// The stage and the reference to feed it with.
    pub fn new() -> (Self, Reference) {
        let config = Config {
            pipeline: Pipeline {
                // 32 kHz by default, which would cut what's above 16 kHz
                maximum_internal_processing_rate: MaxProcessingRate::Rate48kHz,
                ..Pipeline::default()
            },
            echo_canceller: Some(EchoCanceller::default()),
            ..Config::default()
        };
        let stream = StreamConfig::new(SAMPLE_RATE, 1);
        let apm = AudioProcessing::builder()
            .config(config)
            .capture_config(stream)
            .render_config(stream)
            .build();
        let (producer, consumer) = RingBuffer::new(REFERENCE_CAPACITY);
        let status = Arc::new(EchoStatus::new());
        let sidechain = Arc::new(Sidechain {
            frames: AtomicU64::new(0),
            raw: ::std::array::from_fn(|_| AtomicF32::new(0.)),
            status: status.clone(),
            protect: Param::new("protect", "Voice Protection", "%", (0., 100., 5.), 25.),
        });
        let echo = Self {
            apm,
            reference: consumer,
            render: [0.; FRAME_LEN],
            render_out: [0.; FRAME_LEN],
            capture: [0.; FRAME_LEN],
            status: status.clone(),
            delay: [0.; LATENCY + FRAME_LEN],
            sidechain,
            tracker: PlaybackTracker::new(),
            idle: 0,
            detector: None,
            speech_db: Param::new("speech", "Speech Threshold", "dB", (-5., 20., 1.), 5.),
        };
        (echo, Reference { producer, status })
    }

    /// Also hears whether the speakers play speech or something else, such as music, with
    /// DeepFilterNet3's encoder (see [`PlaybackTracker`]); without, it takes all they play for
    /// speech. It allocates in the frames it analyzes, as the noise suppression does.
    pub fn detecting(mut self) -> Result<Self> {
        let df = DfTract::default();
        ensure!(
            df.sr == SAMPLE_RATE as usize && df.hop_size == FRAME_LEN && df.ch == 1,
            "the model expects {} Hz, {} samples per frame and {} channels",
            df.sr,
            df.hop_size,
            df.ch,
        );
        self.detector = Some(Detector {
            df,
            frame: Array2::zeros((1, FRAME_LEN)),
        });
        Ok(self)
    }

    pub fn status(&self) -> Arc<EchoStatus> {
        self.status.clone()
    }

    /// The microphone before it and what the speakers play, for the noise suppression.
    pub fn sidechain(&self) -> Arc<Sidechain> {
        self.sidechain.clone()
    }
}

impl Reference {
    /// Hands over what the speakers played, as much of it as fits; returns how many samples
    /// didn't (while the stage is off or behind).
    pub fn push(&mut self, samples: &[f32]) -> usize {
        let fits = samples.len().min(self.producer.slots());
        if let Ok(chunk) = self.producer.write_chunk_uninit(fits) {
            chunk.fill_from_iter(samples[..fits].iter().copied());
        }
        samples.len() - fits
    }

    /// The status of its echo canceller, for the control interface.
    pub fn status(&self) -> Arc<EchoStatus> {
        self.status.clone()
    }
}

impl Stage for Echo {
    fn name(&self) -> &'static str {
        "echo"
    }

    fn latency(&self) -> usize {
        LATENCY
    }

    /// After a pause the ring holds the reference from before it, played through first; and the
    /// filter has to find the echo path again if it changed, which takes AEC3 about a second.
    fn resume_frames(&self) -> usize {
        REFERENCE_CAPACITY.div_ceil(FRAME_LEN)
    }

    fn params(&self) -> Vec<Param> {
        vec![self.sidechain.protect.clone(), self.speech_db.clone()]
    }

    /// What the speakers play, as [`Playback`] (0 silent, 1 speech, 2 music).
    fn trace(&self) -> Option<(&'static str, f32)> {
        Some(("playback", self.status.playback() as u32 as f32))
    }

    fn process(&mut self, frame: &mut Frame) -> Result<()> {
        let _flush = FlushDenormals::new();
        // the reference first, all of it there is: AEC3 buffers it and finds its delay to the
        // microphone
        if self.reference.slots() < FRAME_LEN {
            // the speakers' stream stops while they're idle
            self.idle += 1;
            if self.idle == SILENCE_FRAMES {
                let playback = self.tracker.stopped();
                self.status
                    .playback
                    .store(playback as u32, Ordering::Relaxed);
            }
        } else {
            self.idle = 0;
        }
        while self.reference.slots() >= FRAME_LEN {
            let chunk = self
                .reference
                .read_chunk(FRAME_LEN)
                .map_err(|err| anyhow!("{err}"))?;
            let (first, second) = chunk.as_slices();
            self.render[..first.len()].copy_from_slice(first);
            self.render[first.len()..].copy_from_slice(second);
            chunk.commit_all();
            self.apm
                .process_render_f32(&[&self.render], &mut [&mut self.render_out])
                .map_err(|err| anyhow!("{err}"))?;
            let status = &self.status;
            let level_db = level::frame_db(&self.render);
            let peak_db = (status.reference_db() - REFERENCE_DECAY_DB).max(level_db);
            status.reference_db.store(peak_db);
            status.reference_frames.fetch_add(1, Ordering::Relaxed);
            let snr_db = match &mut self.detector {
                Some(detector) if self.tracker.analyzes(level_db) => {
                    Some(detector.analyze(&self.render)?)
                }
                _ => None,
            };
            let playback = match self.tracker.frame(level_db, snr_db, self.speech_db.get()) {
                // without the detector it can't tell
                Playback::Music if self.detector.is_none() => Playback::Speech,
                playback => playback,
            };
            status.playback.store(playback as u32, Ordering::Relaxed);
        }
        // the microphone as it was, as far back as the output
        self.delay[LATENCY..].copy_from_slice(frame);
        for (slot, x) in self.sidechain.raw.iter().zip(&self.delay) {
            slot.store(*x);
        }
        self.delay.copy_within(FRAME_LEN.., 0);
        self.sidechain.frames.fetch_add(1, Ordering::Relaxed);
        self.capture.copy_from_slice(frame);
        self.apm
            .process_capture_f32(&[&self.capture], &mut [frame])
            .map_err(|err| anyhow!("{err}"))?;
        let stats = self.apm.statistics();
        let status = &self.status;
        if let Some(erle) = stats.echo_return_loss_enhancement {
            status.erle_db.store(erle as f32);
        }
        if let Some(erl) = stats.echo_return_loss {
            status.erl_db.store(erl as f32);
        }
        if let Some(delay) = stats.delay_ms {
            status.delay_ms.store(delay as f32);
        }
        status.capture_frames.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

/// The delay of the stage in samples, 5 ms: AEC3 works in blocks of 4 ms and splits the 48 kHz
/// signal into three bands. Measured with noise through it, see the tests.
pub const LATENCY: usize = 238;

/// Sets flush to zero and denormals are zero in the MXCSR for as long as it lives, and restores
/// it after. As WebRTC does around its audio processing: the filters of AEC3 decay towards zero
/// while the reference is silent, and denormal floats are up to 100 times slower on x86
/// (sonora issue #34). Nothing on other architectures.
struct FlushDenormals {
    #[cfg(target_arch = "x86_64")]
    saved: u32,
}

impl FlushDenormals {
    #[cfg(target_arch = "x86_64")]
    fn new() -> Self {
        /// Flush to zero (bit 15) and denormals are zero (bit 6).
        const FTZ_DAZ: u32 = 0x8040;
        let mut saved = 0u32;
        // SAFETY: stmxcsr stores the 32-bit MXCSR to the given address, a local u32
        unsafe { ::std::arch::asm!("stmxcsr [{}]", in(reg) &mut saved, options(nostack)) };
        let flushing = saved | FTZ_DAZ;
        // SAFETY: ldmxcsr loads a valid MXCSR value, the saved one with two mode bits set
        unsafe { ::std::arch::asm!("ldmxcsr [{}]", in(reg) &flushing, options(nostack)) };
        Self { saved }
    }

    #[cfg(not(target_arch = "x86_64"))]
    fn new() -> Self {
        Self {}
    }
}

impl Drop for FlushDenormals {
    fn drop(&mut self) {
        #[cfg(target_arch = "x86_64")]
        // SAFETY: restores the MXCSR value stored in `new`
        unsafe {
            ::std::arch::asm!("ldmxcsr [{}]", in(reg) &self.saved, options(nostack))
        };
    }
}
