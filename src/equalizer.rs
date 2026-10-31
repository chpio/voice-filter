use ::anyhow::Result;

use crate::biquad::Biquad;
use crate::param::Param;
use crate::stage::{Frame, Stage};

/// Makes the voice clearer with two peaking filters: less boom in the low mids around
/// [`LOW_MID_FREQ`], more presence where consonants carry the intelligibility, around
/// [`PRESENCE_FREQ`] by default. It also makes up for part of the highs the noise suppression
/// dulls.
pub struct Equalizer {
    low_mid: Biquad,
    presence: Biquad,
    params: Params,
    /// The values the filters have, to see changes of `params`.
    applied: [f32; 3],
}

/// The parameters: gain of the low mids, gain and frequency of the presence.
struct Params {
    low_mid: Param,
    presence: Param,
    presence_freq: Param,
}

impl Params {
    fn values(&self) -> [f32; 3] {
        [
            self.low_mid.get(),
            self.presence.get(),
            self.presence_freq.get(),
        ]
    }
}

pub const LOW_MID_FREQ: f64 = 250.;
/// The defaults of the gains in dB and of the frequency of the presence in Hz.
pub const LOW_MID_DB: f32 = -3.;
pub const PRESENCE_DB: f32 = 3.;
pub const PRESENCE_FREQ: f32 = 3500.;
/// The Q of both bands, about 1.4 octaves wide.
const Q: f64 = 1.;

/// The filters for `[low mid gain, presence gain, presence frequency]`.
fn filters([low_mid, presence, presence_freq]: [f32; 3]) -> (Biquad, Biquad) {
    (
        Biquad::peaking(LOW_MID_FREQ, Q, low_mid as f64),
        Biquad::peaking(presence_freq as f64, Q, presence as f64),
    )
}

impl Equalizer {
    pub fn new() -> Self {
        let params = Params {
            low_mid: Param::new(
                "low-mid",
                "Low Mids (250 Hz)",
                "dB",
                (-9., 3., 0.5),
                LOW_MID_DB,
            ),
            presence: Param::new("presence", "Presence", "dB", (-3., 9., 0.5), PRESENCE_DB),
            presence_freq: Param::new(
                "presence-freq",
                "Presence Frequency",
                "Hz",
                (2000., 6000., 100.),
                PRESENCE_FREQ,
            ),
        };
        let applied = params.values();
        let (low_mid, presence) = filters(applied);
        Self {
            low_mid,
            presence,
            params,
            applied,
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

    fn params(&self) -> Vec<Param> {
        vec![
            self.params.low_mid.clone(),
            self.params.presence.clone(),
            self.params.presence_freq.clone(),
        ]
    }

    fn process(&mut self, frame: &mut Frame) -> Result<()> {
        let values = self.params.values();
        if values != self.applied {
            let (low_mid, presence) = filters(values);
            self.low_mid.retune(low_mid);
            self.presence.retune(presence);
            self.applied = values;
        }
        for sample in frame.iter_mut() {
            *sample = self.presence.process(self.low_mid.process(*sample));
        }
        Ok(())
    }
}
