use ::anyhow::Result;

use crate::biquad::Biquad;
use crate::param::Param;
use crate::stage::{Frame, Stage};
use crate::{FRAME_LEN, SAMPLE_RATE};

/// Removes what's below its frequency ([`FREQ`] by default), where speech has nothing useful:
/// rumble, footfall, knocks on the desk and the low thump of plosives. A Butterworth filter of 4th
/// order (24 dB per octave), two biquads.
pub struct HighPass {
    sections: [Biquad; 2],
    freq: Param,
    /// The frequency `sections` have, to see changes of `freq`.
    applied_freq: f32,
}

/// The default cutoff in Hz, −3 dB.
pub const FREQ: f32 = 80.;
/// The Q of the two sections of a 4th order Butterworth filter.
const Q: [f64; 2] = [0.541_196_1, 1.306_563];

impl HighPass {
    pub fn new() -> Self {
        Self {
            sections: sections(FREQ),
            freq: Param::new("freq", "Frequency", "Hz", (40., 200., 5.), FREQ),
            applied_freq: FREQ,
        }
    }
}

fn sections(freq: f32) -> [Biquad; 2] {
    Q.map(|q| Biquad::high_pass(freq as f64, q))
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
    /// with the higher Q decays with a time constant of 2Q/ω (5 ms at 80 Hz), below −60 dB after
    /// 7 of them (40 ms at 80 Hz, 80 ms at 40 Hz).
    fn resume_frames(&self) -> usize {
        let tau = 2. * Q[1] / (2. * ::std::f64::consts::PI * self.freq.get() as f64);
        (7. * tau * SAMPLE_RATE as f64 / FRAME_LEN as f64).ceil() as usize
    }

    fn params(&self) -> Vec<Param> {
        vec![self.freq.clone()]
    }

    fn process(&mut self, frame: &mut Frame) -> Result<()> {
        let freq = self.freq.get();
        if freq != self.applied_freq {
            for (section, new) in self.sections.iter_mut().zip(sections(freq)) {
                section.retune(new);
            }
            self.applied_freq = freq;
        }
        for sample in frame.iter_mut() {
            *sample = self
                .sections
                .iter_mut()
                .fold(*sample, |x, section| section.process(x));
        }
        Ok(())
    }
}
