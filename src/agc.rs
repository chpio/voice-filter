use ::anyhow::Result;

use crate::level::{self, NoiseFloor};
use crate::param::Param;
use crate::stage::{Frame, Stage};
use crate::{FRAME_LEN, SAMPLE_RATE};

/// Automatic gain control: brings the speech to a constant level, whatever the microphone and the
/// distance to it. It follows the level of the speech only, so it keeps its gain in the pauses
/// instead of pulling up the remaining noise: a frame is speech if it's above the gate and
/// [`SPEECH_ABOVE_FLOOR_DB`] above the noise floor, the quietest frame of the last 2 s, for at
/// least [`MIN_SPEECH_FRAMES`] in a row. Steady noise such as mains hum sits at the floor, speech
/// dips to it between syllables; keystrokes are shorter than syllables.
pub struct Agc {
    /// The smoothed level of the speech, in dBFS (RMS).
    level_db: f32,
    gain_db: f32,
    floor: NoiseFloor,
    /// The frames in a row that were loud enough for speech, up to [`MIN_SPEECH_FRAMES`].
    loud_frames: usize,
    target: Param,
    max_gain: Param,
}

/// The level the speech is brought to by default, in dBFS (RMS): loud enough for calls, with headroom for
/// the peaks, which the limiter after the AGC catches.
pub const TARGET_DB: f32 = -18.;
/// Frames below this level (RMS, dBFS) are pauses, they don't change the measured level.
pub const GATE_DB: f32 = -55.;
/// The most it raises by default.
pub const MAX_GAIN_DB: f32 = 30.;
pub const MIN_GAIN_DB: f32 = -12.;
/// A frame this far above the noise floor is speech, in dB. Steady noise varies by a few dB from
/// frame to frame, speech is 20 dB and more above the floor.
pub const SPEECH_ABOVE_FLOOR_DB: f32 = 10.;
/// The frames in a row (80 ms) that have to be loud enough before they count as speech: a syllable
/// lasts 100 ms and more, a keystroke 10 to 50 ms. Without it, the residue of typing raised the
/// gain to its maximum within 20 s, and with it the hum the noise suppression left.
pub const MIN_SPEECH_FRAMES: usize = 8;
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
            floor: NoiseFloor::new(),
            loud_frames: 0,
            target: Param::new(
                "target",
                "Target Level",
                "dBFS",
                (-30., -12., 0.5),
                TARGET_DB,
            ),
            max_gain: Param::new("max-gain", "Maximum Gain", "dB", (0., 40., 1.), MAX_GAIN_DB),
        }
    }

    /// The level it brings the speech to, for the compressor, whose make-up gain depends on it.
    pub fn target(&self) -> Param {
        self.target.clone()
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

    fn params(&self) -> Vec<Param> {
        vec![self.target.clone(), self.max_gain.clone()]
    }

    fn process(&mut self, frame: &mut Frame) -> Result<()> {
        let frame_db = level::frame_db(frame);
        let floor_db = self.floor.push(frame_db);
        let loud = frame_db > GATE_DB && frame_db > floor_db + SPEECH_ABOVE_FLOOR_DB;
        self.loud_frames = if loud {
            (self.loud_frames + 1).min(MIN_SPEECH_FRAMES)
        } else {
            0
        };
        if self.loud_frames == MIN_SPEECH_FRAMES {
            let alpha = 1. - (-FRAME_S / LEVEL_TAU).exp();
            self.level_db += (frame_db - self.level_db) * alpha;
        }
        let wanted = (self.target.get() - self.level_db).clamp(MIN_GAIN_DB, self.max_gain.get());
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
