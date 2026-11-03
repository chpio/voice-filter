//! Between the two PipeWire streams of the live filter. The capture callback gets as many samples
//! as the quantum of the graph, the chain needs frames of [`FRAME_LEN`], and the playback callback
//! wants its own number of samples. Lock-free and without allocations after [`bridge`], both
//! sides run in the audio path; the chain runs in [`rt_alloc::audio_path`], so what its stages
//! allocate comes from the pool of the binary's allocator.
//!
//! There's no prefill: when the playback side runs short at the start, it plays silence and the
//! samples that arrive later stay buffered, so the buffer settles at the latency the order and
//! quantum of the callbacks need.

use ::std::sync::Arc;
use ::std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use ::std::time::Instant;

use ::rtrb::{Consumer, Producer, RingBuffer};

use crate::atomic::AtomicF32;
use crate::chain::Chain;
use crate::frame_time::{FrameTime, Summary};
use crate::level::{self, Meter, NoiseFloor};
use crate::rt_alloc;
use crate::stage::Frame;
use crate::{FRAME_LEN, SAMPLE_RATE};

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
    /// The processing time of a frame, in the parts of [`FRAME_TIME_PARTS`].
    pub frame_times: [FrameTime; FRAME_TIME_PARTS.len()],
    /// The delay through the bridge at the last pull, in samples: from the capture of a sample to
    /// its playback, without the latency of the chain.
    pub bridge_delay: AtomicI64,
    /// The samples of the last pull, i.e. the quantum of the graph.
    pub quantum: AtomicU64,
    /// From the microphone to the capture stream as PipeWire reports it, in samples.
    pub capture_delay: AtomicI64,
    /// From the output stream to the app as PipeWire reports it, in samples.
    pub playback_delay: AtomicI64,
    /// The level of the microphone and of the output of the chain, as a [`Meter`], in dBFS.
    pub input_db: AtomicF32,
    pub output_db: AtomicF32,
    /// The noise floor of the microphone, see [`NoiseFloor`], in dBFS.
    pub input_floor_db: AtomicF32,
    // what the playback side needs from the capture side for `bridge_delay`: the cycle time of the
    // last push, its number of samples and the samples waiting for a complete frame
    captured_at_ns: AtomicI64,
    captured_block: AtomicU64,
    pending: AtomicU64,
}

/// The parts of the processing time of a frame in [`Stats::frame_times`]: `total`, the stages
/// timed on their own (the expensive ones, neighbors in the chain, so they share a clock reading;
/// 0 if the chain doesn't have them) and `other`, the rest: the other stages (the high-pass
/// before them takes under 1 % of the time, `cargo bench --bench stages`) and the levels
/// measured around the chain.
pub const FRAME_TIME_PARTS: [&str; 4] = ["total", "echo", "denoise", "other"];

/// The stages timed on their own, the parts of [`FRAME_TIME_PARTS`] between `total` and `other`.
const TIMED: [&str; 2] = ["echo", "denoise"];

impl Stats {
    /// The processing time of a frame in its parts ([`FRAME_TIME_PARTS`]), over the window of
    /// the last [`Window::push`](crate::frame_time::Window::push).
    pub fn frame_times(&self) -> impl Iterator<Item = (&'static str, Summary)> {
        FRAME_TIME_PARTS
            .into_iter()
            .zip(&self.frame_times)
            .map(|(part, time)| (part, time.last()))
    }

    /// The latency from the microphone to the app in samples, in its parts: `microphone` (from the
    /// device to the capture stream), `bridge`, `chain` (`chain_latency`) and `app` (from the
    /// output stream to the app).
    pub fn latency(&self, chain_latency: usize) -> [(&'static str, i64); 4] {
        [
            ("microphone", self.capture_delay.load(Ordering::Relaxed)),
            ("bridge", self.bridge_delay.load(Ordering::Relaxed)),
            ("chain", chain_latency as i64),
            ("app", self.playback_delay.load(Ordering::Relaxed)),
        ]
    }
}

/// The capture side: collects samples into frames and runs the chain on them.
pub struct Capture {
    chain: Chain,
    frame: Frame,
    filled: usize,
    output: Producer<f32>,
    stats: Arc<Stats>,
    input_meter: Meter,
    output_meter: Meter,
    input_floor: NoiseFloor,
}

/// The playback side: hands out the processed samples.
pub struct Playback {
    input: Consumer<f32>,
    stats: Arc<Stats>,
}

/// Room for a large quantum plus a few frames; the playback side keeps the level far below.
const CAPACITY: usize = 16 * FRAME_LEN;

pub fn bridge(mut chain: Chain) -> (Capture, Playback, Arc<Stats>) {
    chain.time(&TIMED);
    let (output, input) = RingBuffer::new(CAPACITY);
    let stats = Arc::new(Stats::default());
    for level in [&stats.input_db, &stats.output_db, &stats.input_floor_db] {
        level.store(-120.);
    }
    let capture = Capture {
        chain,
        frame: [0.; FRAME_LEN],
        filled: 0,
        output,
        stats: stats.clone(),
        input_meter: Meter::new(),
        output_meter: Meter::new(),
        input_floor: NoiseFloor::new(),
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

    /// `now_ns` is the time of the graph cycle, the same clock as for [`Playback::pull`].
    pub fn push(&mut self, mut samples: &[f32], now_ns: i64) {
        self.stats.captured_at_ns.store(now_ns, Ordering::Relaxed);
        self.stats
            .captured_block
            .store(samples.len() as u64, Ordering::Relaxed);
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
        self.stats
            .pending
            .store(self.filled as u64, Ordering::Relaxed);
    }

    fn process_frame(&mut self) {
        // Instant::now goes through the vDSO, no syscall
        let started = Instant::now();
        let input_db = level::frame_db(&self.frame);
        // what the chain allocates (tract, for the noise suppression) comes from the pool
        if rt_alloc::audio_path(|| self.chain.process(&mut self.frame)).is_err() {
            self.stats.errors.fetch_add(1, Ordering::Relaxed);
            self.frame.fill(0.);
        }
        let elapsed = started.elapsed().as_nanos() as u64;
        let stats = &self.stats;
        let [total, timed @ .., other] = &stats.frame_times;
        total.record(elapsed);
        let mut timed_ns = 0;
        for (time, &ns) in timed.iter().zip(self.chain.times()) {
            time.record(ns);
            timed_ns += ns;
        }
        other.record(elapsed.saturating_sub(timed_ns));
        stats.input_db.store(self.input_meter.push(input_db));
        stats.input_floor_db.store(self.input_floor.push(input_db));
        let output_db = level::frame_db(&self.frame);
        stats.output_db.store(self.output_meter.push(output_db));
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
    /// `now_ns` is the time of the graph cycle, the same clock as for [`Capture::push`].
    pub fn pull(&mut self, out: &mut [f32], now_ns: i64) {
        self.update_delay(out.len(), now_ns);
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

    /// The first sample this pull hands out is older than the newest captured one by the samples
    /// in the ring and those waiting for a frame. The newest captured one is the end of the last
    /// push, whose cycle may be this one (the capture side ran first) or an earlier one.
    fn update_delay(&self, quantum: usize, now_ns: i64) {
        let stats = &self.stats;
        stats.quantum.store(quantum as u64, Ordering::Relaxed);
        let block = stats.captured_block.load(Ordering::Relaxed) as i64;
        if block == 0 {
            return;
        }
        let since_push_ns = now_ns - stats.captured_at_ns.load(Ordering::Relaxed);
        let since_push = (since_push_ns as f64 * SAMPLE_RATE as f64 / 1e9).round() as i64;
        let waiting = self.input.slots() as i64 + stats.pending.load(Ordering::Relaxed) as i64;
        stats
            .bridge_delay
            .store(since_push - block + waiting, Ordering::Relaxed);
    }

    /// Samples waiting to be played.
    pub fn buffered(&self) -> usize {
        self.input.slots()
    }
}
