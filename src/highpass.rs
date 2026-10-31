use ::anyhow::Result;

use crate::biquad::Biquad;
use crate::stage::{Frame, Stage};

/// Removes what's below [`FREQ`], where speech has nothing useful: rumble, footfall, knocks on the
/// desk and the low thump of plosives. A Butterworth filter of 4th order (24 dB per octave), two
/// biquads.
pub struct HighPass {
    sections: [Biquad; 2],
}

/// The cutoff in Hz, −3 dB.
pub const FREQ: f64 = 80.;
/// The Q of the two sections of a 4th order Butterworth filter.
const Q: [f64; 2] = [0.541_196_1, 1.306_563];

impl HighPass {
    pub fn new() -> Self {
        Self {
            sections: Q.map(|q| Biquad::high_pass(FREQ, q)),
        }
    }
}

impl Default for HighPass {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for HighPass {
    fn name(&self) -> &'static str {
        "highpass"
    }

    fn latency(&self) -> usize {
        0
    }

    /// No latency, but its state still rings with the input from before a pause: the section
    /// with the higher Q decays with a time constant of 2Q/ω, 5 ms at 80 Hz, 40 ms are below
    /// −60 dB.
    fn resume_frames(&self) -> usize {
        4
    }

    fn process(&mut self, frame: &mut Frame) -> Result<()> {
        for sample in frame.iter_mut() {
            *sample = self
                .sections
                .iter_mut()
                .fold(*sample, |x, section| section.process(x));
        }
        Ok(())
    }
}
