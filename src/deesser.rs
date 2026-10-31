use ::std::f64::consts::PI;

use ::anyhow::Result;

use crate::agc::db_to_gain;
use crate::stage::{Frame, Stage};
use crate::{FRAME_LEN, SAMPLE_RATE};

/// Tames harsh S sounds (sibilants), which the presence boost of the EQ brings out further: it
/// turns down the band above [`FREQ`] while it carries most of the energy, as it does in an "s",
/// and leaves everything else alone.
///
/// A split-band de-esser. A linear-phase FIR low-pass splits the input into two bands that are in
/// phase and add up to the input exactly (delayed by [`DELAY`]): the band is the delayed input
/// minus the low-pass. Its share of the energy (both smoothed over a few ms) decides the reduction
/// ([`reduction_db`]); relative, so it doesn't depend on the level and can run before the AGC.
/// The output is `x + (g - 1) · band`, without a reduction the delayed input, unchanged. (A
/// biquad high-pass for the band shifts its phase, by 55° at 7 kHz, so subtracting it took only a
/// fraction of the band away.)
pub struct DeEsser {
    taps: [f32; TAPS],
    /// The last `TAPS - 1` samples of the input, before the current frame.
    history: [f32; TAPS - 1],
    /// The smoothed power of the input and of the band.
    power: f32,
    band_power: f32,
    /// The current reduction of the band in dB, 0 or below.
    reduction_db: f32,
}

/// Where the band of the sibilants starts, in Hz, the middle of the low-pass's transition (about
/// 3.7 to 6.3 kHz).
pub const FREQ: f64 = 5000.;
/// The length of the low-pass, odd; its delay, half of it, is the latency, 1 ms.
const TAPS: usize = 97;
pub const DELAY: usize = TAPS / 2;
/// Above this share of the band in the energy (in dB, 0 is all of it) it turns the band down.
/// Measured on two recordings of speech (after denoise, high-pass and EQ): at −7 dB it turns down
/// by more than 1 dB in 14–16 % of the frames of speech, by more than 3 dB in 5 %, at most by
/// 5 dB and for at most 150 ms in a row, about an "s"; at −12 dB it was 40 % and 18 %, with runs
/// of 260 ms.
pub const THRESHOLD_DB: f32 = -7.;
/// 4 dB more share above the threshold give 1 dB more.
pub const RATIO: f32 = 4.;
/// The most it turns the band down, in dB.
pub const MAX_REDUCTION_DB: f32 = 12.;
/// The time constants of the detector, of the reduction going down and of it coming back, in
/// seconds: an "s" starts suddenly and lasts about 100 ms.
const DETECTOR_TAU: f32 = 0.002;
const ATTACK_TAU: f32 = 0.001;
const RELEASE_TAU: f32 = 0.05;

/// The reduction of the band at its share `share_db` of the energy, 0 or below.
pub fn reduction_db(share_db: f32) -> f32 {
    let over = (share_db - THRESHOLD_DB).max(0.);
    -(over * (1. - 1. / RATIO)).min(MAX_REDUCTION_DB)
}

/// The smoothing factor per sample of a time constant in seconds.
fn coefficient(tau: f32) -> f32 {
    1. - (-1. / (tau * SAMPLE_RATE as f32)).exp()
}

/// The windowed-sinc low-pass at [`FREQ`] (Blackman window), its gain at 0 Hz exactly 1.
fn low_pass() -> [f32; TAPS] {
    let cutoff = FREQ / SAMPLE_RATE as f64;
    let taps: [f64; TAPS] = ::std::array::from_fn(|i| {
        let t = i as f64 - DELAY as f64;
        let sinc = if t == 0. {
            2. * cutoff
        } else {
            (2. * PI * cutoff * t).sin() / (PI * t)
        };
        let phase = 2. * PI * i as f64 / (TAPS - 1) as f64;
        sinc * (0.42 - 0.5 * phase.cos() + 0.08 * (2. * phase).cos())
    });
    let sum: f64 = taps.iter().sum();
    taps.map(|tap| (tap / sum) as f32)
}

impl DeEsser {
    pub fn new() -> Self {
        Self {
            taps: low_pass(),
            history: [0.; TAPS - 1],
            power: 0.,
            band_power: 0.,
            reduction_db: 0.,
        }
    }

    /// The current reduction of the band in dB, 0 or below.
    pub fn reduction(&self) -> f32 {
        self.reduction_db
    }
}

impl Default for DeEsser {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for DeEsser {
    fn name(&self) -> &'static str {
        "deesser"
    }

    fn latency(&self) -> usize {
        DELAY
    }

    fn process(&mut self, frame: &mut Frame) -> Result<()> {
        let detector = coefficient(DETECTOR_TAU);
        let attack = coefficient(ATTACK_TAU);
        let release = coefficient(RELEASE_TAU);

        let mut work = [0.; TAPS - 1 + FRAME_LEN];
        work[..TAPS - 1].copy_from_slice(&self.history);
        work[TAPS - 1..].copy_from_slice(frame);
        for (i, out) in frame.iter_mut().enumerate() {
            let window = &work[i..i + TAPS];
            let delayed = window[DELAY];
            let low: f32 = window.iter().zip(&self.taps).map(|(x, tap)| x * tap).sum();
            let band = delayed - low;

            self.power += (delayed * delayed - self.power) * detector;
            self.band_power += (band * band - self.band_power) * detector;
            let share_db = 10. * ((self.band_power + 1e-12) / (self.power + 1e-12)).log10();
            let wanted = reduction_db(share_db);
            let speed = if wanted < self.reduction_db {
                attack
            } else {
                release
            };
            self.reduction_db += (wanted - self.reduction_db) * speed;
            // the release only nears 0, back to the unchanged input once it's inaudible
            if wanted == 0. && self.reduction_db > -0.01 {
                self.reduction_db = 0.;
            }
            *out = if self.reduction_db < 0. {
                delayed + (db_to_gain(self.reduction_db) - 1.) * band
            } else {
                delayed
            };
        }
        self.history.copy_from_slice(&work[FRAME_LEN..]);
        Ok(())
    }
}
