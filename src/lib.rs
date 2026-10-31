//! Live voice filter for PipeWire.
//!
//! All modules live in the library so the tests under `tests/` can use them, the binary only
//! wires them up. The design is described in `docs/concept.md`.

pub mod agc;
pub mod atomic;
pub mod bridge;
pub mod chain;
pub mod denoise;
pub mod limiter;
pub mod live;
pub mod offline;
pub mod settings;
pub mod stage;

/// The sample rate of the whole chain.
pub const SAMPLE_RATE: u32 = 48000;
/// The samples per frame, 10 ms at [`SAMPLE_RATE`]; the hop size of DeepFilterNet3.
pub const FRAME_LEN: usize = 480;
