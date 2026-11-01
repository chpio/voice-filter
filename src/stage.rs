use ::anyhow::Result;

use crate::FRAME_LEN;
use crate::param::Param;

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
    /// The frames the stage still plays from before a pause: a switched off stage doesn't
    /// process, so after switching it on again the chain runs it that many frames before playing
    /// it. By default its latency, rounded up to frames.
    fn resume_frames(&self) -> usize {
        self.latency().div_ceil(FRAME_LEN)
    }
    /// The settings the control interface can change while it runs, sharing their values with
    /// the stage; called once, before it runs.
    fn params(&self) -> Vec<Param> {
        Vec::new()
    }
    /// What the stage applied or measured in its last frame, as a name and a value (the gain of
    /// the AGC, the estimate of the noise suppression), for traces of a recording
    /// (`examples/trace.rs`); `None` if there's nothing to it beyond its output.
    fn trace(&self) -> Option<(&'static str, f32)> {
        None
    }
}
