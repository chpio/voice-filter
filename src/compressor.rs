use ::anyhow::Result;

use crate::SAMPLE_RATE;
use crate::agc::{TARGET_DB, db_to_gain};
use crate::param::Param;
use crate::stage::{Frame, Stage};

/// Evens out the loudness within words and sentences, which the AGC is too slow for: loud
/// syllables and exclamations get turned down, so the quiet ends of words don't get lost. It runs
/// after the AGC, so its threshold sits at a known level of the speech, and before the limiter,
/// which catches what's faster than its attack.
///
/// A feed-forward compressor: the RMS level over a few ms, a static curve with a soft knee
/// ([`reduction_db`]), the gain reduction smoothed in dB with an attack and a release, and a
/// make-up gain ([`makeup_db`]) that keeps speech from the AGC at its level.
pub struct Compressor {
    /// The smoothed power of the input.
    power: f32,
    /// The current gain reduction in dB, 0 or below.
    reduction_db: f32,
    threshold: Param,
    ratio: Param,
    /// The level the AGC brings the speech to, for the make-up gain; the AGC's parameter.
    target: Param,
}

/// Above this level (RMS, dBFS) the compressor turns down by default, 6 dB below the AGC's
/// target.
pub const THRESHOLD_DB: f32 = -24.;
/// By default, 3 dB more input above the threshold give 1 dB more output.
pub const RATIO: f32 = 3.;
/// The width of the soft knee around the threshold, in dB.
pub const KNEE_DB: f32 = 6.;
/// The time constants of the level detector, of the gain going down and of it coming back, in
/// seconds.
const DETECTOR_TAU: f32 = 0.005;
const ATTACK_TAU: f32 = 0.005;
const RELEASE_TAU: f32 = 0.12;

/// The gain reduction of the static curve at `level_db` (RMS, dBFS) for a threshold and a ratio,
/// 0 or below.
pub fn reduction_db(level_db: f32, threshold_db: f32, ratio: f32) -> f32 {
    let over = level_db - threshold_db;
    let slope = 1. / ratio - 1.;
    if 2. * over <= -KNEE_DB {
        0.
    } else if 2. * over >= KNEE_DB {
        slope * over
    } else {
        slope * (over + KNEE_DB / 2.).powi(2) / (2. * KNEE_DB)
    }
}

/// What the curve takes from speech at the AGC's target `target_db`, given back to all of it: the
/// detector
/// sees the speech over 5 ms, about 3 dB above its level over seconds, which the AGC brings to its
/// target. Measured with a recording of speech: its median level over 50 ms stays the same
/// (within 0.2 dB), the loud parts go down by about 2 dB, the quiet ones up by about 2 dB.
pub fn makeup_db(target_db: f32, threshold_db: f32, ratio: f32) -> f32 {
    -reduction_db(target_db + 3., threshold_db, ratio)
}

/// The smoothing factor per sample of a time constant in seconds.
fn coefficient(tau: f32) -> f32 {
    1. - (-1. / (tau * SAMPLE_RATE as f32)).exp()
}

impl Compressor {
    pub fn new() -> Self {
        Self {
            power: 0.,
            reduction_db: 0.,
            threshold: Param::new(
                "threshold",
                "Threshold",
                "dBFS",
                (-40., -6., 0.5),
                THRESHOLD_DB,
            ),
            ratio: Param::new("ratio", "Ratio", ":1", (1., 10., 0.1), RATIO),
            target: Param::new(
                "target",
                "Target Level",
                "dBFS",
                (-30., -12., 0.5),
                TARGET_DB,
            ),
        }
    }

    /// Follows the target level of an AGC before it ([`crate::agc::Agc::target`]) for its make-up
    /// gain, instead of the default target.
    pub fn following(self, target: Param) -> Self {
        Self { target, ..self }
    }

    /// The current gain reduction in dB, 0 or below.
    pub fn reduction(&self) -> f32 {
        self.reduction_db
    }
}

impl Default for Compressor {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for Compressor {
    fn name(&self) -> &'static str {
        "compressor"
    }

    fn latency(&self) -> usize {
        0
    }

    /// No latency, but its level and gain are from before a pause; the detector and the attack
    /// settle within 5 time constants, 25 ms. A gain reduction from before the pause can
    /// take its release to go away, like the gain of the AGC.
    fn resume_frames(&self) -> usize {
        3
    }

    fn params(&self) -> Vec<Param> {
        vec![self.threshold.clone(), self.ratio.clone()]
    }

    fn process(&mut self, frame: &mut Frame) -> Result<()> {
        let detector = coefficient(DETECTOR_TAU);
        let attack = coefficient(ATTACK_TAU);
        let release = coefficient(RELEASE_TAU);
        let (threshold, ratio) = (self.threshold.get(), self.ratio.get());
        let makeup = makeup_db(self.target.get(), threshold, ratio);
        for sample in frame.iter_mut() {
            self.power += (*sample * *sample - self.power) * detector;
            let level_db = 10. * (self.power + 1e-12).log10();
            let wanted = reduction_db(level_db, threshold, ratio);
            let speed = if wanted < self.reduction_db {
                attack
            } else {
                release
            };
            self.reduction_db += (wanted - self.reduction_db) * speed;
            *sample *= db_to_gain(self.reduction_db + makeup);
        }
        Ok(())
    }
}
