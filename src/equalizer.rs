use ::anyhow::Result;

use crate::biquad::Biquad;
use crate::stage::{Frame, Stage};

/// Makes the voice clearer with peaking filters (see [`BANDS`]): less boom in the low mids, more
/// presence where consonants carry the intelligibility. It also makes up for part of the highs
/// the noise suppression dulls.
pub struct Equalizer {
    bands: [Biquad; BANDS.len()],
}

/// Frequency in Hz, Q and gain in dB of the bands.
pub const BANDS: [(f64, f64, f64); 2] = [(250., 1., -3.), (3500., 1., 3.)];

impl Equalizer {
    pub fn new() -> Self {
        Self {
            bands: BANDS.map(|(freq, q, gain_db)| Biquad::peaking(freq, q, gain_db)),
        }
    }
}

impl Default for Equalizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for Equalizer {
    fn name(&self) -> &'static str {
        "eq"
    }

    fn latency(&self) -> usize {
        0
    }

    /// No latency; its bands ring for about 2Q/ω after a pause, 1.3 ms at 250 Hz.
    fn resume_frames(&self) -> usize {
        1
    }

    fn process(&mut self, frame: &mut Frame) -> Result<()> {
        for sample in frame.iter_mut() {
            *sample = self
                .bands
                .iter_mut()
                .fold(*sample, |x, band| band.process(x));
        }
        Ok(())
    }
}
