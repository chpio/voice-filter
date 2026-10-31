use ::anyhow::Result;

use crate::stage::{Frame, Stage};
use crate::{FRAME_LEN, SAMPLE_RATE};

/// Automatic gain control: brings the speech to a constant level, whatever the microphone and the
/// distance to it. It follows the level of the speech only, so it keeps its gain in the pauses
/// instead of pulling up the remaining noise.
pub struct Agc {
    /// The smoothed level of the speech, in dBFS (RMS).
    level_db: f32,
    gain_db: f32,
}

/// The level the speech is brought to, in dBFS (RMS): loud enough for calls, with headroom for
/// the peaks, which the limiter after the AGC catches.
pub const TARGET_DB: f32 = -18.;
/// Frames below this level (RMS, dBFS) are pauses, they don't change the measured level.
pub const GATE_DB: f32 = -55.;
pub const MAX_GAIN_DB: f32 = 30.;
pub const MIN_GAIN_DB: f32 = -12.;
/// The time constant of the measured level, in seconds.
const LEVEL_TAU: f32 = 0.3;
/// How fast the gain may rise and fall, in dB per second: slow up, so it doesn't pump, faster
/// down, so loud speech isn't too loud for long.
const RISE_DB_PER_S: f32 = 6.;
const FALL_DB_PER_S: f32 = 30.;

const FRAME_S: f32 = FRAME_LEN as f32 / SAMPLE_RATE as f32;

impl Agc {
    pub fn new() -> Self {
        // starts as if the speech were at the target, i.e. at 0 dB gain
        Self {
            level_db: TARGET_DB,
            gain_db: 0.,
        }
    }

    /// The current gain, in dB.
    pub fn gain_db(&self) -> f32 {
        self.gain_db
    }
}

impl Default for Agc {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for Agc {
    fn name(&self) -> &'static str {
        "agc"
    }

    fn latency(&self) -> usize {
        0
    }

    fn process(&mut self, frame: &mut Frame) -> Result<()> {
        let power = frame.iter().map(|x| x * x).sum::<f32>() / FRAME_LEN as f32;
        let frame_db = 10. * (power + 1e-12).log10();
        if frame_db > GATE_DB {
            let alpha = 1. - (-FRAME_S / LEVEL_TAU).exp();
            self.level_db += (frame_db - self.level_db) * alpha;
        }
        let wanted = (TARGET_DB - self.level_db).clamp(MIN_GAIN_DB, MAX_GAIN_DB);
        let previous = self.gain_db;
        self.gain_db = if wanted > previous {
            wanted.min(previous + RISE_DB_PER_S * FRAME_S)
        } else {
            wanted.max(previous - FALL_DB_PER_S * FRAME_S)
        };

        // a ramp from the previous to the new gain over the frame, no steps
        let from = db_to_gain(previous);
        let to = db_to_gain(self.gain_db);
        for (i, sample) in frame.iter_mut().enumerate() {
            let t = (i + 1) as f32 / FRAME_LEN as f32;
            *sample *= from + (to - from) * t;
        }
        Ok(())
    }
}

pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.)
}
