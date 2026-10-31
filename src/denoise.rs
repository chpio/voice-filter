use ::anyhow::{Result, ensure};
use ::deep_filter_rt::tract::DfTract;
use ::ndarray::Array2;

use crate::param::Param;
use crate::stage::{Frame, Stage};
use crate::{FRAME_LEN, SAMPLE_RATE};

/// Noise suppression with the embedded DeepFilterNet3 model.
pub struct Denoise {
    df: DfTract,
    // [channels, samples] as DfTract wants them, allocated once
    noisy: Array2<f32>,
    enhanced: Array2<f32>,
    atten_lim: Param,
    /// The attenuation limit `df` has, to see changes of `atten_lim`.
    applied_atten_lim: f32,
}

/// The default of the model: no limit.
pub const UNLIMITED: f32 = 100.;

impl Denoise {
    pub fn new() -> Result<Self> {
        let df = DfTract::default();
        ensure!(
            df.sr == SAMPLE_RATE as usize && df.hop_size == FRAME_LEN && df.ch == 1,
            "the model expects {} Hz, {} samples per frame and {} channels",
            df.sr,
            df.hop_size,
            df.ch,
        );
        Ok(Self {
            df,
            noisy: Array2::zeros((1, FRAME_LEN)),
            enhanced: Array2::zeros((1, FRAME_LEN)),
            atten_lim: Param::new("limit", "Limit", "dB", (0., UNLIMITED, 1.), UNLIMITED),
            applied_atten_lim: UNLIMITED,
        })
    }

    /// Limits how much the model attenuates, in dB. It keeps part of the original everywhere,
    /// which sounds more natural, [`UNLIMITED`] (100 dB and above) doesn't limit it.
    /// Values above [`UNLIMITED`] mean the same, they're taken as it.
    pub fn with_atten_lim(self, db: f32) -> Self {
        self.atten_lim
            .set(db.clamp(0., UNLIMITED))
            .expect("clamped to the range");
        self
    }

    /// The attenuation limit in dB, to change it while the stage runs in the audio path; it takes
    /// effect with the next frame.
    pub fn atten_lim(&self) -> Param {
        self.atten_lim.clone()
    }
}

impl Stage for Denoise {
    fn name(&self) -> &'static str {
        "denoise"
    }

    fn latency(&self) -> usize {
        self.df.delay()
    }

    /// Its latency plus the frames the deep filter reaches back (`df_order - df_lookahead`), 6 for
    /// DeepFilterNet3. Measured the same with speech before a pause and silence after it: 3 frames
    /// still from before it, then 3 fading out (up to -31 dBFS).
    fn resume_frames(&self) -> usize {
        self.df.delay().div_ceil(FRAME_LEN) + self.df.df_order - self.df.df_lookahead
    }

    fn params(&self) -> Vec<Param> {
        vec![self.atten_lim.clone()]
    }

    fn process(&mut self, frame: &mut Frame) -> Result<()> {
        let atten_lim = self.atten_lim.get();
        if atten_lim != self.applied_atten_lim {
            self.df.set_atten_lim(atten_lim);
            self.applied_atten_lim = atten_lim;
        }
        self.noisy
            .row_mut(0)
            .as_slice_mut()
            .unwrap()
            .copy_from_slice(frame);
        self.df
            .process(self.noisy.view(), self.enhanced.view_mut())?;
        frame.copy_from_slice(self.enhanced.row(0).as_slice().unwrap());
        Ok(())
    }
}
