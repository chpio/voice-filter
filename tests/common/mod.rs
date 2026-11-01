//! Helpers shared by the tests; each test uses only some of them.
#![allow(dead_code)]

use ::std::f64::consts::PI;

use ::voice_filter::stage::{Frame, Stage};
use ::voice_filter::{FRAME_LEN, SAMPLE_RATE};

/// The gain of `stage` at `freq` (Hz) in dB: a sine of 1 s through it, compared over the last
/// half, after the filter settled.
pub fn gain_db(stage: &mut dyn Stage, freq: f64) -> f64 {
    let (mut energy_in, mut energy_out) = (0., 0.);
    let frames = SAMPLE_RATE as usize / FRAME_LEN;
    for i in 0..frames {
        let mut frame = [0.; FRAME_LEN];
        for (j, sample) in frame.iter_mut().enumerate() {
            let t = (i * FRAME_LEN + j) as f64 / SAMPLE_RATE as f64;
            *sample = (0.1 * (2. * PI * freq * t).sin()) as f32;
        }
        let input = frame;
        stage.process(&mut frame).unwrap();
        if i >= frames / 2 {
            energy_in += input.iter().map(|&x| (x as f64).powi(2)).sum::<f64>();
            energy_out += frame.iter().map(|&x| (x as f64).powi(2)).sum::<f64>();
        }
    }
    10. * (energy_out / energy_in).log10()
}

/// White noise from a fixed seed, the state of a xorshift generator.
pub struct Noise(pub u32);

impl Noise {
    /// The next sample, evenly spread over −1 to 1.
    pub fn sample(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32 * 2. - 1.
    }

    /// A frame of it within ±0.1, at about −25 dBFS RMS.
    pub fn frame(&mut self) -> Frame {
        ::std::array::from_fn(|_| 0.1 * self.sample())
    }

    /// `len` samples of it within ±0.1, like [`Self::frame`].
    pub fn samples(&mut self, len: usize) -> Vec<f32> {
        (0..len).map(|_| 0.1 * self.sample()).collect()
    }
}
