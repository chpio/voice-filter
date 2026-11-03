//! The processing time and the allocations per frame of every stage of the chain, on a WAV file
//! (48 kHz) or else on a speech-like signal: `cargo bench --bench stages [-- in.wav]`. Finer than
//! `voice-filter process`, which rounds to 10 µs; the mean is the best of a few runs, the maximum
//! only means something on an idle machine. Without a reference, so the echo canceller has no
//! echo to remove.

use ::std::alloc::{GlobalAlloc, Layout, System};
use ::std::cell::Cell;
use ::std::f32::consts::{PI, TAU};
use ::std::path::Path;
use ::std::time::{Duration, Instant};

use ::anyhow::Result;
use ::voice_filter::dsp::without_denormals;
use ::voice_filter::stage::Frame;
use ::voice_filter::{FRAME_LEN, SAMPLE_RATE, chain, offline, samples_ms};

/// Counts the allocations of the current thread.
struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

// SAFETY: forwards to the system allocator, only counting
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.with(|count| count.set(count.get() + 1));
        // SAFETY: the caller's guarantees hold for the system allocator too
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: as above
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

const RUNS: usize = 5;
/// The length of the speech-like signal.
const SECONDS: usize = 20;

fn main() -> Result<()> {
    // cargo passes `--bench`
    let path = ::std::env::args().skip(1).find(|arg| arg != "--bench");
    let input = match &path {
        Some(path) => offline::read_wav(Path::new(path))?,
        None => speech(SECONDS * SAMPLE_RATE as usize),
    };
    let frames = input.len() / FRAME_LEN;
    let names: Vec<&str> = chain::build(None)?
        .0
        .stages()
        .map(|stage| stage.name())
        .collect();
    let mut means = vec![Duration::MAX; names.len()];
    let mut maxima = vec![Duration::ZERO; names.len()];
    let mut allocations = vec![0; names.len()];
    // of a frame through the whole chain
    let mut max = Duration::ZERO;
    for _ in 0..RUNS {
        let (mut chain, _reference) = chain::build(None)?;
        let mut sums = vec![Duration::ZERO; names.len()];
        for samples in input.as_chunks::<FRAME_LEN>().0 {
            let mut frame: Frame = *samples;
            let mut total = Duration::ZERO;
            // with denormals flushed, as in the chain
            without_denormals(|| {
                for (i, stage) in chain.stages_mut().enumerate() {
                    let before = ALLOCATIONS.get();
                    let started = Instant::now();
                    stage.process(&mut frame)?;
                    let elapsed = started.elapsed();
                    allocations[i] += ALLOCATIONS.get() - before;
                    sums[i] += elapsed;
                    maxima[i] = maxima[i].max(elapsed);
                    total += elapsed;
                }
                Ok::<_, ::anyhow::Error>(())
            })?;
            max = max.max(total);
        }
        for (mean, sum) in means.iter_mut().zip(sums) {
            *mean = (*mean).min(sum / frames as u32);
        }
    }

    let source = path.unwrap_or_else(|| "a speech-like signal".to_owned());
    println!("{source}: {frames} frames, the mean the best of {RUNS} runs");
    println!(
        "{:<12} {:>13} {:>13} {:>18}",
        "stage", "mean/frame", "max/frame", "allocations/frame"
    );
    let us = |duration: Duration| duration.as_secs_f64() * 1e6;
    let row = |name: &str, mean: Duration, max: Duration, allocations: usize| {
        println!(
            "{name:<12} {:>10.2} µs {:>10.2} µs {:>18.1}",
            us(mean),
            us(max),
            allocations as f64 / (frames * RUNS) as f64
        );
    };
    for (i, name) in names.iter().enumerate() {
        row(name, means[i], maxima[i], allocations[i]);
    }
    let total = means.iter().sum::<Duration>();
    row("total", total, max, allocations.iter().sum());
    println!(
        "{:.1} % of the {:.0} ms a frame may take live",
        us(total) / 1e3 / samples_ms(FRAME_LEN as f64) * 100.,
        samples_ms(FRAME_LEN as f64)
    );
    Ok(())
}

/// Something like speech, at its level after a microphone: a vowel (a pulse train at 100 to
/// 160 Hz through three formants) in syllables of 200 ms, words of four of them at different
/// levels, pauses of 800 ms with an S after every second word, and white noise and hum at −50 and
/// −55 dBFS.
fn speech(len: usize) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let formants =
        [(700., 80.), (1220., 90.), (2600., 120.)].map(|(freq, bandwidth): (f32, f32)| {
            let r = (-PI * bandwidth / sr).exp();
            (2. * r * (TAU * freq / sr).cos(), -r * r)
        });
    let mut states = [[0f32; 2]; 3];
    let mut phase = 0.;
    let vowel: Vec<f32> = (0..len)
        .map(|i| {
            phase += (130. + 30. * (TAU * 0.3 * i as f32 / sr).sin()) / sr;
            let pulse = if phase >= 1. { 1. } else { 0. };
            phase %= 1.;
            let resonate = |(&(a1, a2), state): (&(f32, f32), &mut [f32; 2])| {
                let y = pulse + a1 * state[0] + a2 * state[1];
                *state = [y, state[0]];
                y
            };
            formants.iter().zip(&mut states).map(resonate).sum()
        })
        .collect();
    let peak = vowel.iter().fold(0f32, |peak, x| peak.max(x.abs()));

    let syllable = SAMPLE_RATE as usize / 5;
    // four syllables, then a pause as long
    let word = 8 * syllable;
    let (mut noise, mut previous) = (0x2545_f491u32, 0.);
    vowel
        .iter()
        .enumerate()
        .map(|(i, vowel)| {
            noise ^= noise << 13;
            noise ^= noise >> 17;
            noise ^= noise << 5;
            let white = noise as f32 / u32::MAX as f32 * 2. - 1.;
            // a high-pass of the noise
            let hiss = white - previous;
            previous = white;
            let slot = i % word / syllable;
            let shape = (PI * (i % syllable) as f32 / syllable as f32).sin().powi(2);
            let level = [1., 0.5, 0.8, 0.3][i / word % 4];
            let voice = if slot < 4 {
                0.3 * level * vowel / peak * shape
            } else if slot == 4 && (i / word).is_multiple_of(2) {
                0.15 * level * hiss * shape
            } else {
                0.
            };
            let t = i as f32 / sr;
            voice + 0.0055 * white + 0.0025 * (TAU * 50. * t).sin()
        })
        .collect()
}
