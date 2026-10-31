use ::anyhow::{Result, ensure};
use ::deep_filter_rt::tract::DfTract;
use ::ndarray::Array2;

use crate::stage::{Frame, Stage};
use crate::{FRAME_LEN, SAMPLE_RATE};

/// Noise suppression with the embedded DeepFilterNet3 model.
pub struct Denoise {
    df: DfTract,
    // [channels, samples] as DfTract wants them, allocated once
    noisy: Array2<f32>,
    enhanced: Array2<f32>,
}

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
        })
    }
}

impl Denoise {
    /// Limits how much the model attenuates, in dB. It keeps part of the original everywhere,
    /// which sounds more natural, the default of the model (100 dB and above) doesn't limit it.
    pub fn with_atten_lim(mut self, db: f32) -> Self {
        self.df.set_atten_lim(db);
        self
    }
}

impl Stage for Denoise {
    fn name(&self) -> &'static str {
        "denoise"
    }

    fn latency(&self) -> usize {
        self.df.delay()
    }

    fn process(&mut self, frame: &mut Frame) -> Result<()> {
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
