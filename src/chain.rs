use ::std::sync::Arc;
use ::std::sync::atomic::{AtomicBool, Ordering};

use ::anyhow::Result;

use crate::FRAME_LEN;
use crate::stage::{Frame, Stage};

/// The stages in the order the signal runs through them. Each one can be switched off at runtime
/// (see [`Chain::switches`]) without changing the latency: a switched off stage keeps processing,
/// so its state stays current, but its input is played instead, delayed by its latency, and
/// switching crossfades over a frame.
pub struct Chain {
    slots: Vec<Slot>,
}

struct Slot {
    stage: Box<dyn Stage>,
    enabled: Arc<AtomicBool>,
    /// How much of the stage's output is played, 0 to 1; moves to `enabled` over a frame.
    mix: f32,
    /// The stage's input of the last `latency` samples, followed by room for a frame.
    dry: Vec<f32>,
}

impl Chain {
    pub fn new(stages: Vec<Box<dyn Stage>>) -> Self {
        let slots = stages
            .into_iter()
            .map(|stage| Slot {
                dry: vec![0.; stage.latency() + FRAME_LEN],
                stage,
                enabled: Arc::new(AtomicBool::new(true)),
                mix: 1.,
            })
            .collect();
        Self { slots }
    }

    pub fn stages(&self) -> impl Iterator<Item = &dyn Stage> {
        self.slots.iter().map(|slot| &*slot.stage)
    }

    /// For callers that run the stages one by one, e.g. to time each of them; they run without
    /// the switches.
    pub fn stages_mut(&mut self) -> impl Iterator<Item = &mut Box<dyn Stage>> {
        self.slots.iter_mut().map(|slot| &mut slot.stage)
    }

    /// The name and the on/off switch of every stage, to change them while the chain runs in the
    /// audio path.
    pub fn switches(&self) -> Vec<(&'static str, Arc<AtomicBool>)> {
        self.slots
            .iter()
            .map(|slot| (slot.stage.name(), slot.enabled.clone()))
            .collect()
    }

    /// The delay of the whole chain, in samples, the same whether stages are on or off.
    pub fn latency(&self) -> usize {
        self.slots.iter().map(|slot| slot.stage.latency()).sum()
    }

    pub fn process(&mut self, frame: &mut Frame) -> Result<()> {
        for slot in &mut self.slots {
            slot.process(frame)?;
        }
        Ok(())
    }
}

impl Slot {
    fn process(&mut self, frame: &mut Frame) -> Result<()> {
        let target = if self.enabled.load(Ordering::Relaxed) {
            1.
        } else {
            0.
        };
        // the input delayed by the stage's latency is the oldest FRAME_LEN of the kept samples;
        // kept while the stage is on too, so switching it off plays the right samples
        let latency = self.dry.len() - FRAME_LEN;
        self.dry[latency..].copy_from_slice(frame);
        self.stage.process(frame)?;
        if target != 1. || self.mix != 1. {
            let from = self.mix;
            for (i, (wet, dry)) in frame.iter_mut().zip(&self.dry).enumerate() {
                let mix = from + (target - from) * (i + 1) as f32 / FRAME_LEN as f32;
                *wet = *wet * mix + dry * (1. - mix);
            }
        }
        self.dry.copy_within(FRAME_LEN.., 0);
        self.mix = target;
        Ok(())
    }
}
