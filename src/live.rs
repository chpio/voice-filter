//! The live filter: a capture stream from the default microphone and the virtual microphone
//! "Voice Filter", with the chain between them (see [`crate::bridge`]), and a capture stream
//! from the monitor of an output device as the reference of the echo canceller.

use ::std::cell::RefCell;
use ::std::path::PathBuf;
use ::std::sync::atomic::Ordering;
use ::std::sync::{Arc, Mutex};
use ::std::time::Duration;

use ::anyhow::{Context as _, Result, anyhow, bail};
use ::pipewire as pw;
use ::pipewire::loop_::Signal;
use ::pipewire::properties::properties;
use ::pipewire::spa;
use ::pipewire::spa::pod::Pod;
use ::pipewire::stream::{StreamFlags, StreamRc, StreamState};
use ::pipewire::types::ObjectType;

use crate::bridge::{Capture, Playback, Stats, bridge};
use crate::chain::Chain;
use crate::dbus::{self, Control, Service};
use crate::echo::Reference;
use crate::{FRAME_LEN, SAMPLE_RATE};

const NODE_NAME: &str = "voice-filter";
const CAPTURE_NODE_NAME: &str = "voice-filter-capture";
const REFERENCE_NODE_NAME: &str = "voice-filter-reference";
/// The capture stream and the virtual microphone are in this group, so they run in the same cycle
/// of the microphone's driver (a virtual source has no clock of its own), and in this link group,
/// so the session manager never links the capture stream to the filter's own output. (PipeWire
/// schedules both, group and link group, on one driver.)
const GROUP: &str = "voice-filter";
/// The largest quantum the callbacks are prepared for without allocating.
const MAX_QUANTUM: usize = 8192;
const SAMPLE_SIZE: usize = size_of::<f32>();

/// The devices to read from and to take the reference from.
pub struct Devices<'a> {
    /// The `node.name` of the microphone, without it the session manager links the default input.
    pub input: Option<&'a str>,
    /// The `node.name` of the output device whose monitor is the reference of the echo canceller,
    /// without it the default output.
    pub output: Option<&'a str>,
}

/// Runs until SIGINT or SIGTERM. `reference` feeds the echo canceller of the chain with what the
/// output device plays. The control interface keeps the devices chosen there in `settings_dir`.
pub fn run(
    chain: Chain,
    mut reference: Reference,
    devices: Devices,
    settings_dir: Option<PathBuf>,
) -> Result<()> {
    pw::init();
    let switches = chain.switches();
    let params = chain.params();
    let (mut capture, mut playback, stats) = bridge(chain);
    let chain_latency = capture.latency();
    let inputs = Arc::new(Mutex::new(Vec::new()));
    let outputs = Arc::new(Mutex::new(Vec::new()));
    let Devices {
        input,
        output: output_device,
    } = devices;
    let (commands, command_receiver) = pw::channel::channel();
    // first, so a second instance stops before it creates nodes
    dbus::serve(Service {
        switches,
        params,
        stats: stats.clone(),
        chain_latency,
        input: input.unwrap_or_default().to_owned(),
        inputs: inputs.clone(),
        output: output_device.unwrap_or_default().to_owned(),
        outputs: outputs.clone(),
        send: Box::new(move |control| {
            commands
                .send(control)
                .map_err(|_| anyhow!("the PipeWire loop is gone"))
        }),
        listening: false,
        settings_dir,
    })?;

    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context
        .connect_rc(None)
        .context("Could not connect to PipeWire")?;
    let registry = core.get_registry_rc()?;
    let _inputs_listener = watch_devices(&registry, Direction::Input, inputs);
    let _outputs_listener = watch_devices(&registry, Direction::Output, outputs);
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

    // What an output device plays, from its monitor, for the echo canceller. Without a target the
    // session manager links the default output. In neither the group nor the link group of the
    // other streams, PipeWire puts the nodes of both on one driver: the output device would run
    // under the microphone's clock and at its quantum (256) while an app records, with xruns at
    // every join and leave and players that want a larger quantum (Firefox: 900–3600) falling
    // behind. It follows the output device's clock instead and hands its samples over through the ring of the reference; with
    // `stream.capture.sink` it only links to output devices, never to the filter's own output.
    let mut reference_properties = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::NODE_NAME => REFERENCE_NODE_NAME,
        *pw::keys::NODE_DESCRIPTION => "Voice Filter (speakers, for the echo cancellation)",
        *pw::keys::NODE_LATENCY => latency.as_str(),
        *pw::keys::AUDIO_CHANNELS => "1",
        "stream.capture.sink" => "true",
    };
    if let Some(output_device) = output_device {
        reference_properties.insert(*pw::keys::TARGET_OBJECT, output_device);
    }
    let reference_stream = StreamRc::new(core.clone(), REFERENCE_NODE_NAME, reference_properties)?;

    let mut samples = Vec::with_capacity(MAX_QUANTUM);
    let capture_stats = stats.clone();
    let _input_listener = input
        .add_local_listener_with_user_data(())
        .process(move |stream, _| {
            let delay = read_into(stream, &mut capture, &mut samples);
            capture_stats.capture_delay.store(delay, Ordering::Relaxed);
        })
        .register()?;

    let mut played = Vec::with_capacity(MAX_QUANTUM);
    let _reference_listener = reference_stream
        .add_local_listener_with_user_data(())
        .process(move |stream, _| {
            // what doesn't fit while the echo canceller is off or behind gets dropped
            if dequeue_samples(stream, &mut played) {
                reference.push(&played);
            }
        })
        .register()?;

    let mut pulled = vec![0.; MAX_QUANTUM];
    let playback_stats = stats.clone();
    let _output_listener = output
        .add_local_listener_with_user_data(())
        .process(move |stream, _| {
            let delay = write_from(stream, &mut playback, &mut pulled);
            playback_stats
                .playback_delay
                .store(delay, Ordering::Relaxed);
        })
        // The capture streams only exist while an app records from the filter. Just inactive, the
        // microphone's would still count as recording, and GNOME would show it as in use.
        .state_changed({
            let input = input.clone();
            let reference_stream = reference_stream.clone();
            move |_, _, _, new| {
                let result = match new {
                    StreamState::Streaming => {
                        connect_capture(&input).and_then(|()| connect_capture(&reference_stream))
                    }
                    StreamState::Paused => input
                        .disconnect()
                        .and_then(|()| reference_stream.disconnect())
                        .map_err(Into::into),
                    StreamState::Error(error) => {
                        eprintln!("Voice Filter stream error: {error}");
                        return;
                    }
                    _ => return,
                };
                if let Err(err) = result {
                    eprintln!("Could not switch the capture streams: {err}");
                }
            }
        })
        .register()?;

    let _commands = command_receiver.attach(mainloop.loop_(), {
        let input = input.clone();
        let reference_stream = reference_stream.clone();
        let core = core.clone();
        let listening = RefCell::new(None);
        move |control| match control {
            Control::SetInput(name) => {
                if let Err(err) = set_target(&input, &name) {
                    eprintln!("Could not switch to {name}: {err}");
                }
            }
            Control::SetOutput(name) => {
                if let Err(err) = set_target(&reference_stream, &name) {
                    eprintln!("Could not take the reference from {name}: {err}");
                }
            }
            Control::SetListen(false) => drop(listening.take()),
            Control::SetListen(true) if listening.borrow().is_none() => {
                match Listen::start(&core) {
                    Ok(listen) => drop(listening.replace(Some(listen))),
                    Err(err) => eprintln!("Could not start listening: {err}"),
                }
            }
            Control::SetListen(true) => {}
        }
    });

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

const LISTEN_NODE_NAME: &str = "voice-filter-listen";
/// Its own group, so its streams run in one cycle, and its own link group, so the session manager
/// may link it to "Voice Filter" (it never links within a link group).
const LISTEN_GROUP: &str = "voice-filter-listen";

/// Plays what "Voice Filter" outputs on the default output while it lives: a capture stream from
/// it and a playback stream, with a bridge without stages between them. As it records from "Voice
/// Filter" like an app, the filter runs while listening even if no app records.
struct Listen {
    _capture: StreamRc,
    _playback: StreamRc,
    _capture_listener: pw::stream::StreamListener<()>,
    _playback_listener: pw::stream::StreamListener<()>,
}

impl Listen {
    fn start(core: &pw::core::CoreRc) -> Result<Self> {
        let (mut capture, mut playback, _) = bridge(Chain::new(vec![]));
        let latency = format!("{FRAME_LEN}/{SAMPLE_RATE}");
        let capture_stream = StreamRc::new(
            core.clone(),
            LISTEN_NODE_NAME,
            properties! {
                *pw::keys::MEDIA_TYPE => "Audio",
                *pw::keys::MEDIA_CATEGORY => "Capture",
                *pw::keys::NODE_NAME => LISTEN_NODE_NAME,
                *pw::keys::NODE_DESCRIPTION => "Voice Filter (listening)",
                *pw::keys::TARGET_OBJECT => NODE_NAME,
                *pw::keys::NODE_LATENCY => latency.as_str(),
                *pw::keys::NODE_GROUP => LISTEN_GROUP,
                *pw::keys::NODE_LINK_GROUP => LISTEN_GROUP,
                *pw::keys::AUDIO_CHANNELS => "1",
            },
        )?;
        let playback_stream = StreamRc::new(
            core.clone(),
            LISTEN_NODE_NAME,
            properties! {
                *pw::keys::MEDIA_TYPE => "Audio",
                *pw::keys::MEDIA_CATEGORY => "Playback",
                *pw::keys::NODE_NAME => LISTEN_NODE_NAME,
                *pw::keys::NODE_DESCRIPTION => "Voice Filter (listening)",
                *pw::keys::NODE_LATENCY => latency.as_str(),
                *pw::keys::NODE_GROUP => LISTEN_GROUP,
                *pw::keys::NODE_LINK_GROUP => LISTEN_GROUP,
                *pw::keys::AUDIO_CHANNELS => "1",
            },
        )?;
        let mut samples = Vec::with_capacity(MAX_QUANTUM);
        let capture_listener = capture_stream
            .add_local_listener_with_user_data(())
            .process(move |stream, _| {
                read_into(stream, &mut capture, &mut samples);
            })
            .register()?;
        let mut pulled = vec![0.; MAX_QUANTUM];
        let playback_listener = playback_stream
            .add_local_listener_with_user_data(())
            .process(move |stream, _| {
                write_from(stream, &mut playback, &mut pulled);
            })
            .register()?;
        let flags = StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS | StreamFlags::RT_PROCESS;
        let format = format_param();
        capture_stream.connect(
            spa::utils::Direction::Input,
            None,
            flags,
            &mut [Pod::from_bytes(&format).unwrap()],
        )?;
        playback_stream.connect(
            spa::utils::Direction::Output,
            None,
            flags,
            &mut [Pod::from_bytes(&format).unwrap()],
        )?;
        Ok(Self {
            _capture: capture_stream,
            _playback: playback_stream,
            _capture_listener: capture_listener,
            _playback_listener: playback_listener,
        })
    }
}

/// Reads the samples of the cycle from a capture stream into `capture`, `samples` is a buffer for
/// them. Returns the delay from the device in samples.
fn read_into(stream: &pw::stream::Stream, capture: &mut Capture, samples: &mut Vec<f32>) -> i64 {
    let (now, delay) = timing(stream);
    if dequeue_samples(stream, samples) {
        capture.push(samples, now);
    }
    delay
}

/// Takes the samples of the cycle from a capture stream into `samples`, a buffer with room for
/// them; false if there's no buffer.
fn dequeue_samples(stream: &pw::stream::Stream, samples: &mut Vec<f32>) -> bool {
    let Some(mut buffer) = stream.dequeue_buffer() else {
        return false;
    };
    let data = &mut buffer.datas_mut()[0];
    let offset = data.chunk().offset() as usize;
    let size = data.chunk().size() as usize;
    let Some(bytes) = data.data() else {
        return false;
    };
    let end = (offset + size).min(bytes.len());
    samples.clear();
    samples.extend(
        bytes[offset.min(end)..end]
            .as_chunks::<SAMPLE_SIZE>()
            .0
            .iter()
            .map(|&b| f32::from_le_bytes(b)),
    );
    true
}

/// Writes the samples of the cycle from `playback` into a playback stream, `pulled` is a buffer
/// for them. Returns the delay to the device in samples.
fn write_from(stream: &pw::stream::Stream, playback: &mut Playback, pulled: &mut [f32]) -> i64 {
    let (now, delay) = timing(stream);
    let Some(mut buffer) = stream.dequeue_buffer() else {
        return delay;
    };
    // the samples the graph wants this cycle, not the size of the buffer, which would add its
    // whole length as latency
    let requested = match buffer.requested() as usize {
        0 => FRAME_LEN,
        n => n,
    };
    let data = &mut buffer.datas_mut()[0];
    let n = {
        let Some(bytes) = data.data() else {
            return delay;
        };
        let n = requested.min(bytes.len() / SAMPLE_SIZE).min(pulled.len());
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
    delay
}

/// Points a capture stream to the device `name`, the empty name for the default one. A connected
/// stream only follows a new target when it connects again.
fn set_target(input: &StreamRc, name: &str) -> Result<()> {
    let properties = properties! { *pw::keys::TARGET_OBJECT => name };
    // SAFETY: both pointers are valid during the call, which copies the properties
    let result = unsafe {
        pw::sys::pw_stream_update_properties(input.as_raw_ptr(), &(*properties.as_raw_ptr()).dict)
    };
    if result < 0 {
        bail!("pw_stream_update_properties failed with {result}");
    }
    if !matches!(input.state(), StreamState::Unconnected) {
        input.disconnect()?;
        connect_capture(input)?;
    }
    Ok(())
}

/// Connects a capture stream to its target, unless it already is.
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

/// An audio device: a source the filter can read from, or an output device whose monitor is the
/// reference of the echo canceller.
#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    /// The id of its global in the registry.
    pub id: u32,
    /// The `node.name`, for `--input` and `--output`.
    pub name: String,
    pub description: String,
    /// The session manager falls back to the highest one.
    pub priority: i32,
}

/// Microphones (audio sources) or output devices (audio sinks).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Direction {
    Input,
    Output,
}

/// Keeps `devices` up to date with the audio sources (except the filter's own) or sinks of the
/// graph, the highest priority first, as long as the returned listener lives.
fn watch_devices(
    registry: &pw::registry::RegistryRc,
    direction: Direction,
    devices: Arc<Mutex<Vec<Device>>>,
) -> pw::registry::Listener {
    let class_prefix = match direction {
        Direction::Input => "Audio/Source",
        Direction::Output => "Audio/Sink",
    };
    registry
        .add_listener_local()
        .global({
            let devices = devices.clone();
            move |global| {
                let Some(props) = global.props else { return };
                let class = props.get(*pw::keys::MEDIA_CLASS).unwrap_or_default();
                let name = props.get(*pw::keys::NODE_NAME).unwrap_or_default();
                if global.type_ != ObjectType::Node
                    || !class.starts_with(class_prefix)
                    || name == NODE_NAME
                {
                    return;
                }
                let mut devices = devices.lock().unwrap();
                devices.push(Device {
                    id: global.id,
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
                devices.sort_by_key(|device| ::std::cmp::Reverse(device.priority));
            }
        })
        .global_remove(move |id| devices.lock().unwrap().retain(|device| device.id != id))
        .register()
}

/// The audio sources of the graph except the filter's own, or its sinks, the highest priority
/// first.
pub fn devices(direction: Direction) -> Result<Vec<Device>> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context
        .connect_rc(None)
        .context("Could not connect to PipeWire")?;
    let registry = core.get_registry_rc()?;
    let devices = Arc::new(Mutex::new(Vec::new()));
    let _registry_listener = watch_devices(&registry, direction, devices.clone());
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
    let devices = devices.lock().unwrap().clone();
    Ok(devices)
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

/// Mono F32 at [`SAMPLE_RATE`] for all streams, PipeWire converts from and to the devices (and
/// mixes the monitor of a stereo output down).
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
            let max_ns = stats.max_frame_ns.swap(0, Ordering::Relaxed);
            stats.last_max_frame_ns.store(max_ns, Ordering::Relaxed);
            let max_ms = max_ns as f64 / 1e6;
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
