use ::anyhow::Result;

use crate::stage::{Frame, Stage};

/// The stages in the order the signal runs through them.
pub struct Chain {
    stages: Vec<Box<dyn Stage>>,
}

impl Chain {
    pub fn new(stages: Vec<Box<dyn Stage>>) -> Self {
        Self { stages }
    }

    pub fn stages(&self) -> &[Box<dyn Stage>] {
        &self.stages
    }

    /// For callers that run the stages one by one, e.g. to time each of them.
    pub fn stages_mut(&mut self) -> &mut [Box<dyn Stage>] {
        &mut self.stages
    }

    /// The delay of the whole chain, in samples.
    pub fn latency(&self) -> usize {
        self.stages.iter().map(|stage| stage.latency()).sum()
    }

    pub fn process(&mut self, frame: &mut Frame) -> Result<()> {
        for stage in &mut self.stages {
            stage.process(frame)?;
        }
        Ok(())
    }
}
