//! Building blocks the stages share: gains and levels in dB, one-pole smoothing and ramps over a
//! frame.

use crate::{FRAME_LEN, SAMPLE_RATE};

/// The factor of a gain in dB. With `exp`, which is faster than `powf`.
pub fn db_to_gain(db: f32) -> f32 {
    (db * (::std::f32::consts::LN_10 / 20.)).exp()
}

/// The level of a gain factor in dB, the inverse of [`db_to_gain`].
pub fn gain_to_db(gain: f32) -> f32 {
    20. * gain.log10()
}

/// The level of a power (a mean square) in dB, −120 for silence.
pub fn power_db(power: f32) -> f32 {
    10. * (power + 1e-12).log10()
}

/// Runs `f` with denormals flushed to zero, the flags restored after: flush to zero and
/// denormals are zero in the MXCSR on x86, flush to zero in the FPCR on aarch64 (both per
/// thread); elsewhere `f` runs as it is. Around every frame, as WebRTC does around its audio
/// processing: denormals (below 1.2e-38 in f32) are many times slower to compute, and in a pause
/// `denoise` puts out exact zeros, through which a filter's memory decays and toward which a
/// one-pole smoothing sticks at a denormal (`y * factor` rounds to nothing). On a recording of a
/// call it saved about 7 µs per frame in eq, deesser and compressor (an AMD Zen 4). AEC3, 10–50×
/// slower while the reference is silent (sonora issue #34), flushes them itself since sonora 0.3.0.
pub fn without_denormals<T>(f: impl FnOnce() -> T) -> T {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]
    // SAFETY: Rust assumes the default floating-point environment, so what the compiler computes
    // ahead of time can differ from what runs here in the denormals only, below 1.2e-38: far below
    // anything audible, and no code in the audio path depends on them. A guard restores the flags
    // when `f` returns or panics.
    let result = unsafe { ::no_denormals::no_denormals(f) };
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]
    let result = f();
    result
}

/// The factor per sample of a one-pole smoothing (`y += (x - y) * factor`) with the time constant
/// `tau` in seconds.
pub fn smoothing(tau: f32) -> f32 {
    1. - (-1. / (tau * SAMPLE_RATE as f32)).exp()
}

/// One sample of a one-pole smoothing of `value` toward `target` that falls by the factor `attack`
/// and rises by `release` (see [`smoothing`]): the detector of a gain or a reduction in dB, which
/// turn down by falling.
#[inline]
pub fn follow(value: f32, target: f32, attack: f32, release: f32) -> f32 {
    let factor = if target < value { attack } else { release };
    value + (target - value) * factor
}

/// `value` moved toward `target`, by at most `rise` up and `fall` down: a gain in dB that may
/// change only so fast per frame.
pub fn slew(value: f32, target: f32, rise: f32, fall: f32) -> f32 {
    // not `clamp`, which panics on NaN
    if target > value {
        target.min(value + rise)
    } else {
        target.max(value - fall)
    }
}

/// A linear ramp from `from` to `to` over a frame, a value per sample: the last one is `to`, so the
/// next frame goes on from there without a step.
pub fn ramp(from: f32, to: f32) -> impl Iterator<Item = f32> {
    (0..FRAME_LEN).map(move |i| {
        let t = (i + 1) as f32 / FRAME_LEN as f32;
        from + (to - from) * t
    })
}
