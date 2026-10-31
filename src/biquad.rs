//! Second-order IIR filters (biquads), the building block of the high-pass and the EQ.

use ::std::f64::consts::PI;

use crate::SAMPLE_RATE;

/// A biquad in transposed direct form II, with the coefficients of the Audio EQ Cookbook (Robert
/// Bristow-Johnson). Computes in f64: at 48 kHz the poles of low frequencies lie close to the unit
/// circle, where f32 gets noisy.
#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    /// Normalizes the coefficients by `a0`.
    fn new(b: [f64; 3], a: [f64; 3]) -> Self {
        Self {
            b0: b[0] / a[0],
            b1: b[1] / a[0],
            b2: b[2] / a[0],
            a1: a[1] / a[0],
            a2: a[2] / a[0],
            z1: 0.,
            z2: 0.,
        }
    }

    /// The angle per sample of `freq` (Hz) and the cookbook's alpha for `q`.
    fn omega_alpha(freq: f64, q: f64) -> (f64, f64) {
        let omega = 2. * PI * freq / SAMPLE_RATE as f64;
        (omega, omega.sin() / (2. * q))
    }

    /// Lets through what's above `freq` (Hz), 12 dB per octave below it; `q` 1/√2 is Butterworth.
    pub fn high_pass(freq: f64, q: f64) -> Self {
        let (omega, alpha) = Self::omega_alpha(freq, q);
        let cos = omega.cos();
        Self::new(
            [(1. + cos) / 2., -(1. + cos), (1. + cos) / 2.],
            [1. + alpha, -2. * cos, 1. - alpha],
        )
    }

    /// Raises or lowers a band around `freq` (Hz) by `gain_db`; the higher `q`, the narrower.
    pub fn peaking(freq: f64, q: f64, gain_db: f64) -> Self {
        let (omega, alpha) = Self::omega_alpha(freq, q);
        let cos = omega.cos();
        let a = 10f64.powf(gain_db / 40.);
        Self::new(
            [1. + alpha * a, -2. * cos, 1. - alpha * a],
            [1. + alpha / a, -2. * cos, 1. - alpha / a],
        )
    }

    /// Takes the coefficients of `other` and keeps its own state, so a change of the frequency or
    /// gain while it runs doesn't click.
    pub fn retune(&mut self, other: Biquad) {
        *self = Self {
            z1: self.z1,
            z2: self.z2,
            ..other
        };
    }

    pub fn process(&mut self, x: f32) -> f32 {
        let x = x as f64;
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y as f32
    }
}
