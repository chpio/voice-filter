use ::std::sync::Arc;

use ::anyhow::Result;

use crate::denoise::{Speech, SpeechReader};
use crate::dsp::{db_to_gain, ramp};
use crate::level;
use crate::param::Param;
use crate::stage::{Frame, Stage};
use crate::{FRAME_LEN, SAMPLE_RATE};

/// Keeps the noise down in speech pauses: what the noise suppression leaves of it, raised by the
/// AGC and the make-up gain of the compressor. It turns everything down by its range while nobody
/// speaks and opens for speech, so it doesn't touch the voice.
///
/// It opens for a level above its `level` (what's that loud is speech, or noise loud enough to be
/// heard anyway) and for what DeepFilterNet3 hears as speech ([`Speech`], its local SNR above the
/// threshold), which catches quieter speech: it estimates the frame it analyzes, whose output
/// comes [`crate::denoise::Denoise`]'s latency (30 ms) later, so the expander knows ahead and opens
/// before a word starts. While the noise suppression doesn't run (switched off), only the level
/// counts. It stays open for the hold time after, so it doesn't close between words, then closes
/// over [`RELEASE_S`].
pub struct Expander {
    speech: SpeechReader,
    range: Param,
    threshold: Param,
    /// In ms.
    hold: Param,
    level: Param,
    /// The current gain in dB, from minus the range up to 0.
    gain_db: f32,
    /// The samples it still stays open.
    holding: usize,
}

/// The defaults: how much it turns the pauses down (dB), the local SNR above which it hears
/// speech (dB), how long it stays open after (ms) and the level above which it opens anyway
/// (dBFS, 32 dB below the AGC's target). Measured on three recordings (windows of 50 ms, speech
/// within 30 dB of the loud ones): with the SNR alone it turned down up to 20 % of the speech,
/// quiet speech and the ends of words; with the level too none on two, 1.7 % on one that's very
/// quiet (speech down to −55 dBFS), and the pauses after it went 10–20 dB down. The pauses lie
/// at −55 to −65 dBFS after the compressor, so −50 dBFS keeps them closed.
pub const RANGE_DB: f32 = 20.;
pub const THRESHOLD_DB: f32 = 0.;
pub const HOLD_MS: f32 = 200.;
pub const LEVEL_DB: f32 = -50.;
/// How long it takes to close over its whole range, in seconds; it opens within a frame.
pub const RELEASE_S: f32 = 0.15;

impl Expander {
    /// Goes by the level only, until [`Self::listening`] gives it what the noise suppression
    /// hears.
    pub fn new() -> Self {
        Self {
            speech: SpeechReader::new(Arc::new(Speech::new())),
            range: Param::new("range", "Range", "dB", (0., 40., 1.), RANGE_DB),
            threshold: Param::new(
                "threshold",
                "Speech Threshold",
                "dB",
                (-10., 20., 0.5),
                THRESHOLD_DB,
            ),
            hold: Param::new("hold", "Hold", "ms", (50., 1000., 10.), HOLD_MS),
            level: Param::new("level", "Opens Above", "dBFS", (-70., -20., 1.), LEVEL_DB),
            gain_db: 0.,
            holding: 0,
        }
    }

    /// Hears speech where the noise suppression does ([`crate::denoise::Denoise::speech`]).
    pub fn listening(self, speech: Arc<Speech>) -> Self {
        Self {
            speech: SpeechReader::new(speech),
            ..self
        }
    }

    /// The current gain in dB, from minus the range up to 0.
    pub fn gain_db(&self) -> f32 {
        self.gain_db
    }

    /// Whether the frame holds speech: by its level, or by the noise suppression while it runs.
    fn speaks(&mut self, frame: &Frame) -> bool {
        // read every frame, so it sees whether the noise suppression still runs
        let snr_db = self.speech.read();
        level::frame_db(frame) > self.level.get()
            || snr_db.is_some_and(|snr_db| snr_db > self.threshold.get())
    }
}

impl Default for Expander {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for Expander {
    fn name(&self) -> &'static str {
        "expander"
    }

    fn latency(&self) -> usize {
        0
    }

    fn params(&self) -> Vec<Param> {
        vec![
            self.range.clone(),
            self.threshold.clone(),
            self.hold.clone(),
            self.level.clone(),
        ]
    }

    fn process(&mut self, frame: &mut Frame) -> Result<()> {
        let range = self.range.get();
        if self.speaks(frame) {
            self.holding = (self.hold.get() / 1000. * SAMPLE_RATE as f32) as usize;
        }
        let previous = self.gain_db.max(-range);
        self.gain_db = if self.holding > 0 {
            self.holding = self.holding.saturating_sub(FRAME_LEN);
            0.
        } else {
            let step = range / RELEASE_S * FRAME_LEN as f32 / SAMPLE_RATE as f32;
            (previous - step).max(-range)
        };
        if previous == self.gain_db {
            // open, or closed with a gain that stays
            if previous < 0. {
                let gain = db_to_gain(previous);
                frame.iter_mut().for_each(|sample| *sample *= gain);
            }
            return Ok(());
        }
        // a ramp in dB over the frame, no steps
        for (sample, db) in frame.iter_mut().zip(ramp(previous, self.gain_db)) {
            *sample *= db_to_gain(db);
        }
        Ok(())
    }
}
