//! The live filter: a capture stream from the default microphone and the virtual microphone
//! "Voice Filter", with the chain between them (see [`crate::bridge`]).

use ::std::cell::RefCell;
use ::std::rc::Rc;
use ::std::sync::Arc;
use ::std::sync::atomic::Ordering;
use ::std::time::Duration;

use ::anyhow::{Context as _, Result};
use ::pipewire as pw;
use ::pipewire::loop_::Signal;
use ::pipewire::properties::properties;
use ::pipewire::spa;
use ::pipewire::spa::pod::Pod;
use ::pipewire::stream::{StreamFlags, StreamRc, StreamState};
use ::pipewire::types::ObjectType;

use crate::bridge::{Stats, bridge};
use crate::chain::Chain;
use crate::{FRAME_LEN, SAMPLE_RATE};

const NODE_NAME: &str = "voice-filter";
const CAPTURE_NODE_NAME: &str = "voice-filter-capture";
/// Both streams are in this group, so they run in the same cycle of the microphone's driver (a
/// virtual source has no clock of its own), and in this link group, so the session manager never
/// links the capture stream to the filter's own output.
const GROUP: &str = "voice-filter";
/// The largest quantum the callbacks are prepared for without allocating.
const MAX_QUANTUM: usize = 8192;
const SAMPLE_SIZE: usize = size_of::<f32>();

/// Runs until SIGINT or SIGTERM. `input` is the `node.name` of the microphone, without it the
/// session manager links the default input.
pub fn run(chain: Chain, input: Option<&str>) -> Result<()> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context
        .connect_rc(None)
        .context("Could not connect to PipeWire")?;
    let (mut capture, mut playback, stats) = bridge(chain);
    let chain_latency = capture.latency();
    // the quantum the streams ask for, the graph may still run a smaller one
    let latency = format!("{FRAME_LEN}/{SAMPLE_RATE}");

    // The virtual microphone: `Audio/Source`, not `Audio/Source/Virtual`, WirePlumber doesn't set
    // a format for the latter, so it would get no ports.
    let output = StreamRc::new(
        core.clone(),
        NODE_NAME,
        properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_CLASS => "Audio/Source",
            *pw::keys::NODE_NAME => NODE_NAME,
            *pw::keys::NODE_DESCRIPTION => "Voice Filter",
            *pw::keys::NODE_LATENCY => latency.as_str(),
            *pw::keys::NODE_GROUP => GROUP,
            *pw::keys::NODE_LINK_GROUP => GROUP,
            *pw::keys::AUDIO_CHANNELS => "1",
        },
    )?;
    let mut capture_properties = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Communication",
        *pw::keys::NODE_NAME => CAPTURE_NODE_NAME,
        *pw::keys::NODE_DESCRIPTION => "Voice Filter (microphone)",
        *pw::keys::NODE_LATENCY => latency.as_str(),
        *pw::keys::NODE_GROUP => GROUP,
        *pw::keys::NODE_LINK_GROUP => GROUP,
        *pw::keys::AUDIO_CHANNELS => "1",
    };
    // Without a target, the session manager links the default input; if that's "Voice Filter"
    // itself, the link group rules it out and it falls back to the source with the highest
    // priority, whichever that is.
    if let Some(input) = input {
        capture_properties.insert(*pw::keys::TARGET_OBJECT, input);
    }
    let input = StreamRc::new(core.clone(), CAPTURE_NODE_NAME, capture_properties)?;

    let mut samples = Vec::with_capacity(MAX_QUANTUM);
    let capture_stats = stats.clone();
    let _input_listener = input
        .add_local_listener_with_user_data(())
        .process(move |stream, _| {
            let (now, delay) = timing(stream);
            capture_stats.capture_delay.store(delay, Ordering::Relaxed);
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let data = &mut buffer.datas_mut()[0];
            let offset = data.chunk().offset() as usize;
            let size = data.chunk().size() as usize;
            let Some(bytes) = data.data() else { return };
            let end = (offset + size).min(bytes.len());
            samples.clear();
            samples.extend(
                bytes[offset.min(end)..end]
                    .as_chunks::<SAMPLE_SIZE>()
                    .0
                    .iter()
                    .map(|&b| f32::from_le_bytes(b)),
            );
            capture.push(&samples, now);
        })
        .register()?;

    let mut pulled = vec![0.; MAX_QUANTUM];
    let playback_stats = stats.clone();
    let _output_listener = output
        .add_local_listener_with_user_data(())
        .process(move |stream, _| {
            let (now, delay) = timing(stream);
            playback_stats
                .playback_delay
                .store(delay, Ordering::Relaxed);
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            // the samples the graph wants this cycle, not the size of the buffer, which would
            // add its whole length as latency
            let requested = match buffer.requested() as usize {
                0 => FRAME_LEN,
                n => n,
            };
            let data = &mut buffer.datas_mut()[0];
            let n = {
                let Some(bytes) = data.data() else { return };
                let n = requested.min(bytes.len() / SAMPLE_SIZE).min(MAX_QUANTUM);
                playback.pull(&mut pulled[..n], now);
                for (b, sample) in bytes
                    .as_chunks_mut::<SAMPLE_SIZE>()
                    .0
                    .iter_mut()
                    .zip(&pulled[..n])
                {
                    *b = sample.to_le_bytes();
                }
                n
            };
            let chunk = data.chunk_mut();
            *chunk.offset_mut() = 0;
            *chunk.stride_mut() = SAMPLE_SIZE as _;
            *chunk.size_mut() = (n * SAMPLE_SIZE) as _;
        })
        // The capture stream only exists while an app records from the filter. Just inactive, it
        // would still count as recording, and GNOME would show the microphone as in use.
        .state_changed({
            let input = input.clone();
            move |_, _, _, new| {
                let result = match new {
                    StreamState::Streaming => connect_capture(&input),
                    StreamState::Paused => input.disconnect().map_err(Into::into),
                    StreamState::Error(error) => {
                        eprintln!("Voice Filter stream error: {error}");
                        return;
                    }
                    _ => return,
                };
                if let Err(err) = result {
                    eprintln!("Could not switch the microphone stream: {err}");
                }
            }
        })
        .register()?;

    let format = format_param();
    output.connect(
        spa::utils::Direction::Output,
        None,
        StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS | StreamFlags::RT_PROCESS,
        &mut [Pod::from_bytes(&format).unwrap()],
    )?;

    let quit = |mainloop: &pw::main_loop::MainLoopRc| {
        let mainloop = mainloop.clone();
        move || mainloop.quit()
    };
    let _sigint = mainloop
        .loop_()
        .add_signal_local(Signal::INT, quit(&mainloop));
    let _sigterm = mainloop
        .loop_()
        .add_signal_local(Signal::TERM, quit(&mainloop));

    eprintln!(
        "\"Voice Filter\" is ready, the chain adds {:.1} ms",
        chain_latency as f64 * 1e3 / SAMPLE_RATE as f64
    );
    report_periodically(stats, chain_latency);
    mainloop.run();
    Ok(())
}

/// Connects the capture stream to the default microphone, unless it already is.
fn connect_capture(input: &StreamRc) -> Result<()> {
    if !matches!(input.state(), StreamState::Unconnected) {
        return Ok(());
    }
    let format = format_param();
    input.connect(
        spa::utils::Direction::Input,
        None,
        StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS | StreamFlags::RT_PROCESS,
        &mut [Pod::from_bytes(&format).unwrap()],
    )?;
    Ok(())
}

/// An audio source the filter can read from.
pub struct Input {
    /// The `node.name`, for `--input`.
    pub name: String,
    pub description: String,
    /// The session manager falls back to the highest one.
    pub priority: i32,
}

/// The audio sources of the graph except the filter's own, the highest priority first.
pub fn inputs() -> Result<Vec<Input>> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context
        .connect_rc(None)
        .context("Could not connect to PipeWire")?;
    let registry = core.get_registry_rc()?;
    let inputs = Rc::new(RefCell::new(Vec::new()));
    let _registry_listener = registry
        .add_listener_local()
        .global({
            let inputs = inputs.clone();
            move |global| {
                let Some(props) = global.props else { return };
                let class = props.get(*pw::keys::MEDIA_CLASS).unwrap_or_default();
                let name = props.get(*pw::keys::NODE_NAME).unwrap_or_default();
                if global.type_ != ObjectType::Node
                    || !class.starts_with("Audio/Source")
                    || name == NODE_NAME
                {
                    return;
                }
                inputs.borrow_mut().push(Input {
                    name: name.to_owned(),
                    description: props
                        .get(*pw::keys::NODE_DESCRIPTION)
                        .unwrap_or(name)
                        .to_owned(),
                    priority: props
                        .get(*pw::keys::PRIORITY_SESSION)
                        .and_then(|priority| priority.parse().ok())
                        .unwrap_or(0),
                });
            }
        })
        .register();
    // all globals are announced once the server answers the sync
    let pending = core.sync(0)?;
    let _core_listener = core
        .add_listener_local()
        .done({
            let mainloop = mainloop.clone();
            move |id, seq| {
                if id == pw::core::PW_ID_CORE && seq == pending {
                    mainloop.quit();
                }
            }
        })
        .register();
    mainloop.run();
    let mut inputs = inputs.take();
    inputs.sort_by_key(|input| ::std::cmp::Reverse(input.priority));
    Ok(inputs)
}

/// The time of the graph cycle in nanoseconds and the delay to the device in samples (including
/// what the resampler buffers), as PipeWire reports them; zero if it can't.
fn timing(stream: &pw::stream::Stream) -> (i64, i64) {
    let Ok(time) = stream.time() else {
        return (0, 0);
    };
    let rate = time.rate();
    let delay = if rate.denom == 0 {
        0
    } else {
        time.delay() * rate.num as i64 * SAMPLE_RATE as i64 / rate.denom as i64
    };
    (time.now(), delay + time.buffered() as i64)
}

/// Mono F32 at [`SAMPLE_RATE`] for both streams, PipeWire converts from and to the devices.
fn format_param() -> Vec<u8> {
    let mut info = spa::param::audio::AudioInfoRaw::new();
    info.set_format(spa::param::audio::AudioFormat::F32LE);
    info.set_rate(SAMPLE_RATE);
    info.set_channels(1);
    let mut position = [0; spa::param::audio::MAX_CHANNELS];
    position[0] = spa::sys::SPA_AUDIO_CHANNEL_MONO;
    info.set_position(position);
    spa::pod::serialize::PodSerializer::serialize(
        ::std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(spa::pod::Object {
            type_: spa::sys::SPA_TYPE_OBJECT_Format,
            id: spa::sys::SPA_PARAM_EnumFormat,
            properties: info.into(),
        }),
    )
    .expect("the format serializes")
    .0
    .into_inner()
}

/// Prints the counters and the latency every 10 s while frames are processed.
fn report_periodically(stats: Arc<Stats>, chain_latency: usize) {
    let ms = |samples: i64| samples as f64 * 1e3 / SAMPLE_RATE as f64;
    ::std::thread::spawn(move || {
        let mut last = [0; 4];
        loop {
            ::std::thread::sleep(Duration::from_secs(10));
            let now = [
                stats.frames.load(Ordering::Relaxed),
                stats.underruns.load(Ordering::Relaxed),
                stats.dropped.load(Ordering::Relaxed),
                stats.errors.load(Ordering::Relaxed),
            ];
            let max_ms = stats.max_frame_ns.swap(0, Ordering::Relaxed) as f64 / 1e6;
            if now[0] != last[0] {
                let capture = stats.capture_delay.load(Ordering::Relaxed);
                let bridge = stats.bridge_delay.load(Ordering::Relaxed);
                let playback = stats.playback_delay.load(Ordering::Relaxed);
                let total = capture + bridge + chain_latency as i64 + playback;
                eprintln!(
                    "{} frames, max {max_ms:.2} ms per frame, {} samples silence, {} dropped, \
                     {} errors",
                    now[0] - last[0],
                    now[1] - last[1],
                    now[2] - last[2],
                    now[3] - last[3],
                );
                eprintln!(
                    "latency {:.1} ms: microphone {:.1} + bridge {:.1} + chain {:.1} + app {:.1}, \
                     quantum {}",
                    ms(total),
                    ms(capture),
                    ms(bridge),
                    ms(chain_latency as i64),
                    ms(playback),
                    stats.quantum.load(Ordering::Relaxed),
                );
            }
            last = now;
        }
    });
}
