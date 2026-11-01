use ::anyhow::Result;

use crate::dsp::{db_to_gain, smoothing};
use crate::param::Param;
use crate::stage::{Frame, Stage};
use crate::{FRAME_LEN, SAMPLE_RATE};

/// A peak limiter with lookahead: it sees a peak [`LOOKAHEAD`] samples before it plays it and
/// lowers the gain in time, so the output stays below its ceiling ([`CEILING_DB`] by default)
/// without clipping.
pub struct Limiter {
    /// The last samples of the previous frame, not played yet.
    delay: [f32; LOOKAHEAD],
    gain: f32,
    ceiling: Param,
    /// In ms.
    release: Param,
}

/// 1 ms, the latency of the limiter; not adjustable, the latency of the chain stays the same.
pub const LOOKAHEAD: usize = SAMPLE_RATE as usize / 1000;
/// The highest output peak by default, in dBFS; a bit below full scale for the inter-sample peaks
/// of codecs.
pub const CEILING_DB: f32 = -1.;
/// The time constant of the gain going down to a peak, in seconds, a fraction of the lookahead.
const ATTACK_TAU: f32 = LOOKAHEAD as f32 / SAMPLE_RATE as f32 / 4.;
/// The time constant of the gain going back up after a peak by default, in ms: shorter is louder,
/// but distorts sooner.
pub const RELEASE_MS: f32 = 50.;

impl Limiter {
    pub fn new() -> Self {
        Self {
            delay: [0.; LOOKAHEAD],
            gain: 1.,
            ceiling: Param::new("ceiling", "Ceiling", "dBFS", (-6., 0., 0.5), CEILING_DB),
            release: Param::new("release", "Release", "ms", (10., 200., 5.), RELEASE_MS),
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

    fn params(&self) -> Vec<Param> {
        vec![self.ceiling.clone(), self.release.clone()]
    }

    fn process(&mut self, frame: &mut Frame) -> Result<()> {
        let ceiling = db_to_gain(self.ceiling.get());
        let attack = smoothing(ATTACK_TAU);
        let release = smoothing(self.release.get() / 1000.);

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
