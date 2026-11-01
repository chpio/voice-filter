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
