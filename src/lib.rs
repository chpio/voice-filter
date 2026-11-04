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
pub mod dsp;
pub mod echo;
pub mod equalizer;
pub mod expander;
pub mod frame_time;
#[cfg(feature = "gui")]
pub mod gui;
pub mod highpass;
pub mod i18n;
pub mod level;
pub mod limiter;
pub mod live;
pub mod offline;
pub mod param;
pub mod rt_alloc;
pub mod settings;
pub mod stage;
pub mod tray;
pub mod volume;

/// The sample rate of the whole chain.
pub const SAMPLE_RATE: u32 = 48000;
/// The samples per frame, 10 ms at [`SAMPLE_RATE`]; the hop size of DeepFilterNet3.
pub const FRAME_LEN: usize = 480;
/// The length of a frame in seconds, 10 ms.
pub const FRAME_S: f32 = FRAME_LEN as f32 / SAMPLE_RATE as f32;

/// A number of samples at [`SAMPLE_RATE`] in milliseconds.
pub fn samples_ms(samples: f64) -> f64 {
    samples * 1e3 / SAMPLE_RATE as f64
}
