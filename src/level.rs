//! Levels of frames: their RMS level and the noise floor under them.

use crate::dsp::power_db;
use crate::{FRAME_LEN, FRAME_S};

/// The RMS level of a frame in dBFS, −120 for silence.
pub fn frame_db(frame: &[f32; FRAME_LEN]) -> f32 {
    power_db(frame.iter().map(|x| x * x).sum::<f32>() / FRAME_LEN as f32)
}

/// The noise floor: the quietest frame level of the last 2 s. Longer than a syllable or a word,
/// so speech always dips to it, short enough that it follows when the noise gets louder. Steady
/// noise such as mains hum sits at it.
pub struct NoiseFloor {
    /// The quietest frame level of each block of [`BLOCK_FRAMES`], in dBFS; the oldest one is
    /// overwritten by the next block. Infinite until a block had a frame.
    blocks: [f32; BLOCKS],
    /// The block the next frame goes into, and the frames it has so far.
    block: usize,
    block_frames: usize,
}

const BLOCKS: usize = 20;
const BLOCK_FRAMES: usize = 10;

impl NoiseFloor {
    pub fn new() -> Self {
        Self {
            blocks: [f32::INFINITY; BLOCKS],
            block: 0,
            block_frames: 0,
        }
    }

    /// Adds the level of a frame, in dBFS, and returns the floor including it.
    pub fn push(&mut self, frame_db: f32) -> f32 {
        let block = &mut self.blocks[self.block];
        // a new block replaces the oldest one
        *block = if self.block_frames == 0 {
            frame_db
        } else {
            block.min(frame_db)
        };
        self.block_frames += 1;
        if self.block_frames == BLOCK_FRAMES {
            self.block_frames = 0;
            self.block = (self.block + 1) % BLOCKS;
        }
        self.blocks.iter().copied().fold(f32::INFINITY, f32::min)
    }
}

impl Default for NoiseFloor {
    fn default() -> Self {
        Self::new()
    }
}

/// A frame this far above the noise floor is speech, in dB. Steady noise varies by a few dB from
/// frame to frame, speech is 20 dB and more above the floor.
pub const SPEECH_ABOVE_FLOOR_DB: f32 = 10.;
/// The frames in a row (80 ms) that have to be loud enough before they count as speech: a syllable
/// lasts 100 ms and more, a keystroke 10 to 50 ms. Without it, the residue of typing raised the
/// AGC's gain to its maximum within 20 s, and with it the hum the noise suppression left.
pub const MIN_SPEECH_FRAMES: usize = 8;
/// The time constant of the measured level, in seconds.
const LEVEL_TAU: f32 = 0.3;

/// The level of the speech in a signal (RMS, dBFS), measured on the frames that are speech by their
/// level: above a gate and [`SPEECH_ABOVE_FLOOR_DB`] above the [`NoiseFloor`], for at least
/// [`MIN_SPEECH_FRAMES`] in a row, smoothed over 0.3 s. Pauses, steady noise such as mains hum (it
/// sits at the floor) and keystrokes (shorter than a syllable) leave it as it is.
pub struct SpeechLevel {
    gate_db: f32,
    level_db: f32,
    floor: NoiseFloor,
    /// The frames in a row that were speech, up to [`MIN_SPEECH_FRAMES`].
    loud_frames: usize,
}

impl SpeechLevel {
    /// Frames below `gate_db` (dBFS) are never speech; the level is `initial_db` until speech
    /// comes.
    pub fn new(gate_db: f32, initial_db: f32) -> Self {
        Self {
            gate_db,
            level_db: initial_db,
            floor: NoiseFloor::new(),
            loud_frames: 0,
        }
    }

    /// Adds the level of a frame (RMS, dBFS) and returns the level of the speech. With `heard`
    /// false the frame isn't speech whatever its level, e.g. where the noise suppression doesn't
    /// hear any.
    pub fn push(&mut self, frame_db: f32, heard: bool) -> f32 {
        let floor_db = self.floor.push(frame_db);
        let loud = frame_db > self.gate_db && frame_db > floor_db + SPEECH_ABOVE_FLOOR_DB;
        self.loud_frames = if loud && heard {
            (self.loud_frames + 1).min(MIN_SPEECH_FRAMES)
        } else {
            0
        };
        if self.loud_frames == MIN_SPEECH_FRAMES {
            let alpha = 1. - (-FRAME_S / LEVEL_TAU).exp();
            self.level_db += (frame_db - self.level_db) * alpha;
        }
        self.level_db
    }
}

/// A level meter as the app shows it: follows a louder frame at once and falls by
/// [`METER_FALL_DB_PER_S`] after, so short peaks stay visible between two reads.
pub struct Meter {
    db: f32,
}

pub const METER_FALL_DB_PER_S: f32 = 20.;

impl Meter {
    pub fn new() -> Self {
        Self { db: -120. }
    }

    /// Adds the level of a frame, in dBFS, and returns the meter's.
    pub fn push(&mut self, frame_db: f32) -> f32 {
        let fall = METER_FALL_DB_PER_S * FRAME_S;
        self.db = frame_db.max(self.db - fall).max(-120.);
        self.db
    }
}

impl Default for Meter {
    fn default() -> Self {
        Self::new()
    }
}
