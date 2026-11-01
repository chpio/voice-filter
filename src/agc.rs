use ::std::sync::Arc;

use ::anyhow::{Result, ensure};

use crate::denoise::{Speech, SpeechReader};
use crate::dsp::{db_to_gain, ramp};
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
///
/// With [`Self::listening`] a frame also has to be what DeepFilterNet3 hears as speech (its local
/// SNR above the threshold, [`SPEECH_SNR_DB`] by default), so louder noise the floor and the run
/// let through, such as a cough, a click or a door, doesn't count either. While the noise
/// suppression doesn't run (switched off), only the level counts. Measured on two recordings, the
/// estimate drops below 0 dB within words too (143 of the 160 frames it took away on a live
/// recording, some at −13 dBFS), which lowered the gain by 1 dB on average and up to 9 dB, while
/// it took only 17 frames in 89 s that weren't next to speech. [`SNR_OFF_DB`] turns it off.
pub struct Agc {
    /// The smoothed level of the speech, in dBFS (RMS).
    level_db: f32,
    gain_db: f32,
    floor: NoiseFloor,
    /// The frames in a row that were speech (loud enough and, if it listens, heard as speech), up
    /// to [`MIN_SPEECH_FRAMES`].
    loud_frames: usize,
    speech: SpeechReader,
    /// The estimates of `speech` of the last frames (`None` while it didn't run), the next one
    /// goes to `estimate`: the noise suppression estimates the newest frame, the AGC gets it
    /// `ahead` frames later.
    estimates: [Option<f32>; MAX_AHEAD + 1],
    estimate: usize,
    ahead: usize,
    target: Param,
    max_gain: Param,
    threshold: Param,
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
/// The local SNR (DeepFilterNet3's estimate, dB) above which a frame is speech by default, as the
/// expander's default.
pub const SPEECH_SNR_DB: f32 = 0.;
/// The lowest threshold, below the lowest estimate (−15 dB): every frame passes, only the level
/// counts.
pub const SNR_OFF_DB: f32 = -20.;
/// The most frames the estimate of the noise suppression may come ahead of the audio.
pub const MAX_AHEAD: usize = 8;
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
            speech: SpeechReader::new(Arc::new(Speech::new())),
            estimates: [None; MAX_AHEAD + 1],
            estimate: 0,
            ahead: 0,
            target: target_param(),
            max_gain: Param::new("max-gain", "Maximum Gain", "dB", (0., 40., 1.), MAX_GAIN_DB),
            threshold: Param::new(
                "threshold",
                "Speech Threshold",
                "dB",
                (SNR_OFF_DB, 20., 0.5),
                SPEECH_SNR_DB,
            ),
        }
    }

    /// Counts only what the noise suppression hears as speech
    /// ([`crate::denoise::Denoise::speech`]). Its estimate comes `ahead` frames before the audio
    /// it belongs to (the latency of the noise suppression in frames, at most [`MAX_AHEAD`]),
    /// the AGC holds it back that long.
    pub fn listening(self, speech: Arc<Speech>, ahead: usize) -> Result<Self> {
        ensure!(
            ahead <= MAX_AHEAD,
            "the estimate comes {ahead} frames ahead, at most {MAX_AHEAD} are supported"
        );
        Ok(Self {
            speech: SpeechReader::new(speech),
            ahead,
            ..self
        })
    }

    /// Whether the noise suppression hears speech in the frame, `None` while it doesn't run.
    fn hears_speech(&mut self) -> Option<bool> {
        self.estimates[self.estimate] = self.speech.read();
        let len = self.estimates.len();
        let belonging = self.estimates[(self.estimate + len - self.ahead) % len];
        self.estimate = (self.estimate + 1) % len;
        belonging.map(|snr_db| snr_db > self.threshold.get())
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
        vec![
            self.target.clone(),
            self.max_gain.clone(),
            self.threshold.clone(),
        ]
    }

    fn trace(&self) -> Option<(&'static str, f32)> {
        Some(("gain", self.gain_db))
    }

    fn process(&mut self, frame: &mut Frame) -> Result<()> {
        let frame_db = level::frame_db(frame);
        let floor_db = self.floor.push(frame_db);
        let loud = frame_db > GATE_DB && frame_db > floor_db + SPEECH_ABOVE_FLOOR_DB;
        // every frame, so its estimates keep in step
        let heard = self.hears_speech().unwrap_or(true);
        let speech = loud && heard;
        self.loud_frames = if speech {
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
        let gains = ramp(db_to_gain(previous), db_to_gain(self.gain_db));
        for (sample, gain) in frame.iter_mut().zip(gains) {
            *sample *= gain;
        }
        Ok(())
    }
}

/// The level the AGC brings the speech to, as a parameter: [`Agc::target`] shares it.
pub fn target_param() -> Param {
    Param::new(
        "target",
        "Target Level",
        "dBFS",
        (-30., -12., 0.5),
        TARGET_DB,
    )
}
