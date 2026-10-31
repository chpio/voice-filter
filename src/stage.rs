use ::anyhow::Result;

use crate::FRAME_LEN;

/// One frame of mono audio at [`crate::SAMPLE_RATE`].
pub type Frame = [f32; FRAME_LEN];

/// A step of the signal chain. It runs in the audio path, so `process` must not allocate, lock or
/// make syscalls; everything it needs is set up in the constructor.
pub trait Stage: Send {
    /// A short name for reports and the control interface.
    fn name(&self) -> &'static str;
    /// The delay of the output against the input, in samples.
    fn latency(&self) -> usize;
    /// Processes one frame in place.
    fn process(&mut self, frame: &mut Frame) -> Result<()>;
}
