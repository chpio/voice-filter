use ::anyhow::Result;

use crate::FRAME_LEN;
use crate::SAMPLE_RATE;
use crate::agc::db_to_gain;
use crate::stage::{Frame, Stage};

/// A peak limiter with lookahead: it sees a peak [`LOOKAHEAD`] samples before it plays it and
/// lowers the gain in time, so the output stays below [`CEILING_DB`] without clipping.
pub struct Limiter {
    /// The last samples of the previous frame, not played yet.
    delay: [f32; LOOKAHEAD],
    gain: f32,
}

/// 1 ms, the latency of the limiter.
pub const LOOKAHEAD: usize = SAMPLE_RATE as usize / 1000;
/// The highest output peak, in dBFS; a bit below full scale for the inter-sample peaks of codecs.
pub const CEILING_DB: f32 = -1.;
/// The time constant of the gain going down to a peak, a fraction of the lookahead, and back up
/// after it, in seconds.
const ATTACK_TAU: f32 = LOOKAHEAD as f32 / SAMPLE_RATE as f32 / 4.;
const RELEASE_TAU: f32 = 0.05;

impl Limiter {
    pub fn new() -> Self {
        Self {
            delay: [0.; LOOKAHEAD],
            gain: 1.,
        }
    }
}

impl Default for Limiter {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for Limiter {
    fn name(&self) -> &'static str {
        "limiter"
    }

    fn latency(&self) -> usize {
        LOOKAHEAD
    }

    fn process(&mut self, frame: &mut Frame) -> Result<()> {
        let ceiling = db_to_gain(CEILING_DB);
        let attack = 1. - (-1. / (ATTACK_TAU * SAMPLE_RATE as f32)).exp();
        let release = 1. - (-1. / (RELEASE_TAU * SAMPLE_RATE as f32)).exp();

        // the delayed samples followed by the new ones, so every output sample sees the next
        // LOOKAHEAD samples
        let mut work = [0.; LOOKAHEAD + FRAME_LEN];
        work[..LOOKAHEAD].copy_from_slice(&self.delay);
        work[LOOKAHEAD..].copy_from_slice(frame);
        for (i, out) in frame.iter_mut().enumerate() {
            let peak = work[i..=i + LOOKAHEAD]
                .iter()
                .fold(0f32, |peak, x| peak.max(x.abs()));
            let wanted = if peak > ceiling { ceiling / peak } else { 1. };
            let rate = if wanted < self.gain { attack } else { release };
            self.gain += (wanted - self.gain) * rate;
            // the smoothing may lag behind a steep peak, the clamp catches what's left
            *out = (work[i] * self.gain).clamp(-ceiling, ceiling);
        }
        self.delay.copy_from_slice(&work[FRAME_LEN..]);
        Ok(())
    }
}
