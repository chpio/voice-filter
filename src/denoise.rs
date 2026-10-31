use ::std::sync::Arc;
use ::std::sync::atomic::{AtomicU64, Ordering};

use ::anyhow::{Result, ensure};
use ::deep_filter_rt::tract::DfTract;
use ::ndarray::Array2;

use crate::atomic::AtomicF32;
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
    speech: Arc<Speech>,
}

/// How much speech the model hears, for the stages after it (the expander): its estimate of the
/// local SNR of the frame it analyzes, in dB (−15 to 35), and how many frames it processed, so a
/// reader sees when it stopped (switched off). The frame it analyzes is the newest, its output is
/// [`Stage::latency`] behind, so the estimate comes ahead of the audio it belongs to.
#[derive(Debug)]
pub struct Speech {
    snr_db: AtomicF32,
    frames: AtomicU64,
}

impl Speech {
    pub fn new() -> Self {
        Self {
            snr_db: AtomicF32::new(-15.),
            frames: AtomicU64::new(0),
        }
    }

    /// The local SNR of the last frame in dB.
    pub fn snr_db(&self) -> f32 {
        self.snr_db.load()
    }

    /// The frames the model processed so far.
    pub fn frames(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }

    pub fn update(&self, snr_db: f32) {
        self.snr_db.store(snr_db);
        self.frames.fetch_add(1, Ordering::Relaxed);
    }
}

impl Default for Speech {
    fn default() -> Self {
        Self::new()
    }
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
            speech: Arc::new(Speech::new()),
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

    /// How much speech the model hears, updated with every frame it processes.
    pub fn speech(&self) -> Arc<Speech> {
        self.speech.clone()
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
        let snr_db = self
            .df
            .process(self.noisy.view(), self.enhanced.view_mut())?;
        self.speech.update(snr_db);
        frame.copy_from_slice(self.enhanced.row(0).as_slice().unwrap());
        Ok(())
    }
}
