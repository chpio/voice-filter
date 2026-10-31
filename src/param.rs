use ::std::sync::Arc;

use ::anyhow::{Result, ensure};

use crate::atomic::AtomicF32;

/// A setting of a stage that the control interface changes while the stage runs in the audio
/// path: the stage reads [`Param::get`] at the start of every frame and applies a change with it.
/// Clones share the value.
#[derive(Clone, Debug)]
pub struct Param {
    /// Unique within its stage, for the control interface.
    pub name: &'static str,
    /// For the app.
    pub title: &'static str,
    /// Shown after the value, e.g. `dB`, `Hz` or `:1`.
    pub unit: &'static str,
    pub min: f32,
    pub max: f32,
    /// The step the app offers.
    pub step: f32,
    pub default: f32,
    value: Arc<AtomicF32>,
}

impl Param {
    pub fn new(
        name: &'static str,
        title: &'static str,
        unit: &'static str,
        (min, max, step): (f32, f32, f32),
        default: f32,
    ) -> Self {
        Self {
            name,
            title,
            unit,
            min,
            max,
            step,
            default,
            value: Arc::new(AtomicF32::new(default)),
        }
    }

    pub fn get(&self) -> f32 {
        self.value.load()
    }

    /// Fails outside `min..=max` (and for NaN), the value stays as it is then.
    pub fn set(&self, value: f32) -> Result<()> {
        ensure!(
            (self.min..=self.max).contains(&value),
            "{} must be {} to {} {}, not {value}",
            self.name,
            self.min,
            self.max,
            self.unit,
        );
        self.value.store(value);
        Ok(())
    }
}
