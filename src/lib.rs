//! Live voice filter for PipeWire.
//!
//! All modules live in the library so the tests under `tests/` can use them, the binary only
//! wires them up. The design is described in `docs/concept.md`.

pub mod agc;
pub mod atomic;
pub mod biquad;
pub mod bridge;
pub mod chain;
pub mod compressor;
pub mod dbus;
pub mod deesser;
pub mod denoise;
pub mod echo;
pub mod equalizer;
#[cfg(feature = "gui")]
pub mod gui;
pub mod highpass;
pub mod limiter;
pub mod live;
pub mod offline;
pub mod param;
pub mod settings;
pub mod stage;

/// The sample rate of the whole chain.
pub const SAMPLE_RATE: u32 = 48000;
/// The samples per frame, 10 ms at [`SAMPLE_RATE`]; the hop size of DeepFilterNet3.
pub const FRAME_LEN: usize = 480;
