//! Building blocks the stages share: gains and levels in dB, one-pole smoothing and ramps over a
//! frame.

use crate::{FRAME_LEN, SAMPLE_RATE};

/// The factor of a gain in dB.
pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.)
}

/// The level of a power (a mean square) in dB, −120 for silence.
pub fn power_db(power: f32) -> f32 {
    10. * (power + 1e-12).log10()
}

/// The factor per sample of a one-pole smoothing (`y += (x - y) * factor`) with the time constant
/// `tau` in seconds.
pub fn smoothing(tau: f32) -> f32 {
    1. - (-1. / (tau * SAMPLE_RATE as f32)).exp()
}

/// A linear ramp from `from` to `to` over a frame, a value per sample: the last one is `to`, so the
/// next frame goes on from there without a step.
pub fn ramp(from: f32, to: f32) -> impl Iterator<Item = f32> {
    (0..FRAME_LEN).map(move |i| {
        let t = (i + 1) as f32 / FRAME_LEN as f32;
        from + (to - from) * t
    })
}
