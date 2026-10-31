//! Between the two PipeWire streams of the live filter. The capture callback gets as many samples
//! as the quantum of the graph, the chain needs frames of [`FRAME_LEN`], and the playback callback
//! wants its own number of samples. Lock-free and without allocations after [`bridge`], both
//! sides run in the audio path.
//!
//! There's no prefill: when the playback side runs short at the start, it plays silence and the
//! samples that arrive later stay buffered, so the buffer settles at the latency the order and
//! quantum of the callbacks need.

use ::std::sync::Arc;
use ::std::sync::atomic::{AtomicU64, Ordering};
use ::std::time::Instant;

use ::rtrb::{Consumer, Producer, RingBuffer};

use crate::FRAME_LEN;
use crate::chain::Chain;
use crate::stage::Frame;

/// Counters both sides update and the main thread reads.
#[derive(Default)]
pub struct Stats {
    /// Frames run through the chain.
    pub frames: AtomicU64,
    /// Frames the chain failed on, they're replaced by silence.
    pub errors: AtomicU64,
    /// Samples the playback side filled with silence because the chain wasn't ahead.
    pub underruns: AtomicU64,
    /// Samples dropped to bring the latency back down, e.g. after the playback side paused.
    pub dropped: AtomicU64,
    /// The longest processing time of a frame in nanoseconds, reset by the reader.
    pub max_frame_ns: AtomicU64,
}

/// The capture side: collects samples into frames and runs the chain on them.
pub struct Capture {
    chain: Chain,
    frame: Frame,
    filled: usize,
    output: Producer<f32>,
    stats: Arc<Stats>,
}

/// The playback side: hands out the processed samples.
pub struct Playback {
    input: Consumer<f32>,
    stats: Arc<Stats>,
}

/// Room for a large quantum plus a few frames; the playback side keeps the level far below.
const CAPACITY: usize = 16 * FRAME_LEN;

pub fn bridge(chain: Chain) -> (Capture, Playback, Arc<Stats>) {
    let (output, input) = RingBuffer::new(CAPACITY);
    let stats = Arc::new(Stats::default());
    let capture = Capture {
        chain,
        frame: [0.; FRAME_LEN],
        filled: 0,
        output,
        stats: stats.clone(),
    };
    let playback = Playback {
        input,
        stats: stats.clone(),
    };
    (capture, playback, stats)
}

impl Capture {
    /// The latency of the chain, in samples.
    pub fn latency(&self) -> usize {
        self.chain.latency()
    }

    pub fn push(&mut self, mut samples: &[f32]) {
        while !samples.is_empty() {
            let n = (FRAME_LEN - self.filled).min(samples.len());
            self.frame[self.filled..self.filled + n].copy_from_slice(&samples[..n]);
            self.filled += n;
            samples = &samples[n..];
            if self.filled == FRAME_LEN {
                self.filled = 0;
                self.process_frame();
            }
        }
    }

    fn process_frame(&mut self) {
        // Instant::now goes through the vDSO, no syscall
        let started = Instant::now();
        if self.chain.process(&mut self.frame).is_err() {
            self.stats.errors.fetch_add(1, Ordering::Relaxed);
            self.frame.fill(0.);
        }
        let elapsed = started.elapsed().as_nanos() as u64;
        self.stats
            .max_frame_ns
            .fetch_max(elapsed, Ordering::Relaxed);
        self.stats.frames.fetch_add(1, Ordering::Relaxed);
        match self.output.write_chunk_uninit(FRAME_LEN) {
            Ok(chunk) => {
                chunk.fill_from_iter(self.frame.iter().copied());
            }
            // nobody pulls, the playback side trims the level once it runs again
            Err(_) => {
                self.stats
                    .dropped
                    .fetch_add(FRAME_LEN as u64, Ordering::Relaxed);
            }
        }
    }
}

impl Playback {
    pub fn pull(&mut self, out: &mut [f32]) {
        let n = self.input.slots().min(out.len());
        if let Ok(chunk) = self.input.read_chunk(n) {
            let (first, second) = chunk.as_slices();
            out[..first.len()].copy_from_slice(first);
            out[first.len()..n].copy_from_slice(second);
            chunk.commit_all();
        }
        if n < out.len() {
            out[n..].fill(0.);
            self.stats
                .underruns
                .fetch_add((out.len() - n) as u64, Ordering::Relaxed);
        }

        // In a steady state less than the quantum and two frames are left over (the order of the
        // callbacks may change between cycles, then the next pull comes before the next frames).
        // More means the samples piled up, e.g. while the playback side was paused, and would only
        // add latency.
        let left = self.input.slots();
        if left > out.len() + 2 * FRAME_LEN {
            let drop = left - (out.len() + FRAME_LEN);
            if let Ok(chunk) = self.input.read_chunk(drop) {
                chunk.commit_all();
            }
            self.stats.dropped.fetch_add(drop as u64, Ordering::Relaxed);
        }
    }

    /// Samples waiting to be played, i.e. the latency the buffer adds.
    pub fn buffered(&self) -> usize {
        self.input.slots()
    }
}
