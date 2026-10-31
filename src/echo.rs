use ::anyhow::{Result, anyhow};
use ::rtrb::{Consumer, Producer, RingBuffer};
use ::sonora::config::{EchoCanceller, MaxProcessingRate, Pipeline};
use ::sonora::{AudioProcessing, Config, StreamConfig};

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
}

/// What the speakers play, handed to the echo canceller from another thread.
pub struct Reference {
    producer: Producer<f32>,
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
        let echo = Self {
            apm,
            reference: consumer,
            render: [0.; FRAME_LEN],
            render_out: [0.; FRAME_LEN],
            capture: [0.; FRAME_LEN],
        };
        (echo, Reference { producer })
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
        }
        self.capture.copy_from_slice(frame);
        self.apm
            .process_capture_f32(&[&self.capture], &mut [frame])
            .map_err(|err| anyhow!("{err}"))?;
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
