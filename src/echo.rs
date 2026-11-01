use ::std::sync::Arc;
use ::std::sync::atomic::{AtomicU64, Ordering};

use ::anyhow::{Result, anyhow};
use ::rtrb::{Consumer, Producer, RingBuffer};
use ::sonora::config::{EchoCanceller, MaxProcessingRate, Pipeline};
use ::sonora::{AudioProcessing, Config, StreamConfig};

use crate::atomic::AtomicF32;
use crate::level;
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
pub struct Echo {
    apm: AudioProcessing,
    reference: Consumer<f32>,
    /// A frame of the reference, of the input and the output of the render side (unused).
    render: [f32; FRAME_LEN],
    render_out: [f32; FRAME_LEN],
    capture: [f32; FRAME_LEN],
    status: Arc<EchoStatus>,
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
        }
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
        let echo = Self {
            apm,
            reference: consumer,
            render: [0.; FRAME_LEN],
            render_out: [0.; FRAME_LEN],
            capture: [0.; FRAME_LEN],
            status: status.clone(),
        };
        (echo, Reference { producer, status })
    }

    pub fn status(&self) -> Arc<EchoStatus> {
        self.status.clone()
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

    fn process(&mut self, frame: &mut Frame) -> Result<()> {
        let _flush = FlushDenormals::new();
        // the reference first, all of it there is: AEC3 buffers it and finds its delay to the
        // microphone
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
        }
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
