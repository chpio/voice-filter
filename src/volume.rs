use ::anyhow::Result;

use crate::dsp::{db_to_gain, ramp};
use crate::param::Param;
use crate::stage::{Frame, Stage};

/// Turns the signal down by its volume, e.g. what Listen plays. A change ramps over a frame, so it
/// doesn't click.
pub struct Volume {
    volume: Param,
    /// The gain of the last frame.
    gain: f32,
}

/// The volume by default: as loud as the filter outputs it.
pub const VOLUME_DB: f32 = 0.;

/// The volume as a parameter, to share it with [`Volume::new`]: a `Volume` that's created again
/// (Listen, each time it starts) keeps it.
pub fn param() -> Param {
    Param::new("volume", "Volume", "dB", (-40., 0., 1.), VOLUME_DB)
}

impl Volume {
    pub fn new(volume: Param) -> Self {
        let gain = db_to_gain(volume.get());
        Self { volume, gain }
    }
}

impl Stage for Volume {
    fn name(&self) -> &'static str {
        "volume"
    }

    fn latency(&self) -> usize {
        0
    }

    fn params(&self) -> Vec<Param> {
        vec![self.volume.clone()]
    }

    fn process(&mut self, frame: &mut Frame) -> Result<()> {
        let (from, to) = (self.gain, db_to_gain(self.volume.get()));
        self.gain = to;
        for (sample, gain) in frame.iter_mut().zip(ramp(from, to)) {
            *sample *= gain;
        }
        Ok(())
    }
}
