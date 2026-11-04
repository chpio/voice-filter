//! The live filter: a capture stream from the default microphone and a playback stream into the
//! virtual microphone "Voice Filter", with the chain between them (see [`crate::bridge`]), and a
//! capture stream from the monitor of an output device as the reference of the echo canceller.
//! "Voice Filter" is a node of PipeWire's own that outlives the process (see `Graph`).

use ::std::cell::{Cell, RefCell};
use ::std::collections::HashMap;
use ::std::path::PathBuf;
use ::std::rc::Rc;
use ::std::sync::atomic::{AtomicBool, Ordering};
use ::std::sync::{Arc, Mutex, OnceLock};
use ::std::time::Duration;

use ::anyhow::{Context as _, Result, anyhow, bail};
use ::ksni::blocking::Handle;
use ::pipewire as pw;
use ::pipewire::loop_::Signal;
use ::pipewire::properties::properties;
use ::pipewire::spa;
use ::pipewire::spa::pod::Pod;
use ::pipewire::stream::{StreamFlags, StreamRc, StreamState};
use ::pipewire::types::ObjectType;

use crate::bridge::{Capture, FRAME_TIME_PARTS, Playback, Stats, bridge};
use crate::chain::Chain;
use crate::dbus::{self, Control, Service};
use crate::echo::Reference;
use crate::frame_time::{WINDOW_SECONDS, Window};
use crate::param::Param;
use crate::rt_alloc;
use crate::settings;
use crate::tray::{self, Tray};
use crate::volume::{self, Volume};
use crate::{FRAME_LEN, SAMPLE_RATE, samples_ms};

/// The virtual microphone apps record from.
const NODE_NAME: &str = "voice-filter";
/// The stream that plays the filtered microphone into it.
const OUTPUT_NODE_NAME: &str = "voice-filter-output";
const CAPTURE_NODE_NAME: &str = "voice-filter-capture";
const REFERENCE_NODE_NAME: &str = "voice-filter-reference";
/// The capture stream, the output stream and the virtual microphone are in this group, so they
/// run in the same cycle of the microphone's driver (rather than the virtual microphone's timer),
/// and in this link group, so the session manager never links the capture stream to the virtual
/// microphone. (PipeWire schedules both, group and link group, on one driver.)
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

/// Shows the icon in the tray, with the icons written where it finds them and the binary that
/// opens the app (this one, it has the app built in). `None` if it can't, after saying why.
fn show_tray(
    enabled: Arc<AtomicBool>,
    switches: Vec<(&'static str, Arc<AtomicBool>)>,
    params: Vec<(&'static str, Param)>,
    settings_dir: Option<PathBuf>,
) -> Option<Handle<Tray>> {
    let icons = tray::icons_dir(settings_dir.as_deref()).filter(|dir| {
        tray::write_icons(dir)
            .inspect_err(|err| eprintln!("{err:#}"))
            .is_ok()
    });
    let app = if cfg!(feature = "gui") {
        ::std::env::current_exe()
            .inspect_err(|err| eprintln!("The tray can't open the app: {err}"))
            .ok()
    } else {
        None
    };
    let tray = Tray {
        enabled,
        switches,
        params,
        settings_dir,
        icon_theme_path: icons
            .and_then(|dir| dir.to_str().map(str::to_owned))
            .unwrap_or_default(),
        app,
    };
    tray::spawn(tray)
        .inspect_err(|err| eprintln!("{err:#}"))
        .ok()
}

/// Runs until SIGINT or SIGTERM. `reference` feeds the echo canceller of the chain with what the
/// output device plays. The control interface keeps the devices chosen there in `settings_dir`,
/// and Listen: its volume, and whether it was on if the user chose to keep that, which starts it
/// again here. `verbose` prints the counters and the latency every 10 s, not only when something
/// went wrong.
pub fn run(
    chain: Chain,
    mut reference: Reference,
    devices: Devices,
    settings_dir: Option<PathBuf>,
    verbose: bool,
) -> Result<()> {
    pw::init();
    let enabled = chain.enabled();
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
    let listen_volume = volume::param();
    let mut keep_listen = None;
    if let Some(dir) = &settings_dir {
        match settings::load_listen_volume(dir) {
            Ok(Some(db)) => {
                if let Err(err) = listen_volume.set(db) {
                    eprintln!("Skipped the saved volume of Listen: {err}");
                }
            }
            Ok(None) => {}
            Err(err) => eprintln!("Skipped the saved volume of Listen: {err:#}"),
        }
        keep_listen = settings::load_listen(dir)?;
        match settings::load_enabled(dir) {
            Ok(on) => enabled.store(on, Ordering::Relaxed),
            Err(err) => eprintln!("Skipped the saved switch of the filter: {err:#}"),
        }
    }
    let listen_on = keep_listen == Some(true);
    // set once the control interface runs
    let tray = Arc::new(OnceLock::<Handle<Tray>>::new());
    let tray_settings = (
        enabled.clone(),
        switches.clone(),
        params.clone(),
        settings_dir.clone(),
    );
    // first, so a second instance stops before it creates nodes or shows its icon
    dbus::serve(Service {
        enabled,
        switched: Box::new({
            let tray = tray.clone();
            move || {
                if let Some(tray) = tray.get() {
                    // reads the switches again, updates icon, tooltip and menu
                    tray.update(|_| ());
                }
            }
        }),
        switches,
        params,
        stats: stats.clone(),
        echo: Some(reference.status()),
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
        listening: listen_on,
        keep_listen: keep_listen.is_some(),
        listen_volume: listen_volume.clone(),
        settings_dir,
    })?;
    let (enabled, switches, params, settings_dir) = tray_settings;
    if let Some(handle) = show_tray(enabled, switches, params, settings_dir) {
        let _ = tray.set(handle);
    }

    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context
        .connect_rc(None)
        .context("Could not connect to PipeWire")?;
    let registry = core.get_registry_rc()?;
    let _inputs_listener = watch_devices(&registry, Direction::Input, inputs);
    let _outputs_listener = watch_devices(&registry, Direction::Output, outputs);

    // What the chain outputs, into the virtual microphone. The session manager links playback
    // streams only to sinks, so `Graph` links it. Passive, so its link doesn't keep "Voice Filter"
    // running while nothing records from it (`link.passive` on the link has no effect).
    let output = stream(
        &core,
        OUTPUT_NODE_NAME,
        "Voice Filter (output)",
        None,
        &[
            (*pw::keys::MEDIA_CATEGORY, "Playback"),
            (*pw::keys::NODE_GROUP, GROUP),
            (*pw::keys::NODE_LINK_GROUP, GROUP),
            (*pw::keys::NODE_AUTOCONNECT, "false"),
            (*pw::keys::NODE_PASSIVE, "true"),
        ],
    )?;
    // Without a target, the session manager links the default input; if that's "Voice Filter"
    // itself, the link group rules it out and it falls back to the source with the highest
    // priority, whichever that is.
    let input = stream(
        &core,
        CAPTURE_NODE_NAME,
        "Voice Filter (microphone)",
        input,
        &[
            (*pw::keys::MEDIA_CATEGORY, "Capture"),
            (*pw::keys::MEDIA_ROLE, "Communication"),
            (*pw::keys::NODE_GROUP, GROUP),
            (*pw::keys::NODE_LINK_GROUP, GROUP),
        ],
    )?;

    // What an output device plays, from its monitor, for the echo canceller. Without a target the
    // session manager links the default output. In neither the group nor the link group of the
    // other streams, PipeWire puts the nodes of both on one driver: the output device would run
    // under the microphone's clock and at its quantum (256) while an app records, with xruns at
    // every join and leave and players that want a larger quantum (Firefox: 900–3600) falling
    // behind. It follows the output device's clock instead and hands its samples over through the
    // ring of the reference; with `stream.capture.sink` it only links to output devices, never to
    // the filter's own output.
    let reference_stream = stream(
        &core,
        REFERENCE_NODE_NAME,
        "Voice Filter (speakers, for the echo cancellation)",
        output_device,
        &[
            (*pw::keys::MEDIA_CATEGORY, "Capture"),
            ("stream.capture.sink", "true"),
        ],
    )?;

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
        .state_changed(|_, _, _, new| {
            if let StreamState::Error(error) = new {
                eprintln!("Voice Filter stream error: {error}");
            }
        })
        .register()?;
    let _graph = Graph::watch(&core, &registry, [input.clone(), reference_stream.clone()])?;

    let listening = Rc::new(RefCell::new(None));
    let start_listening = {
        let (core, listening) = (core.clone(), listening.clone());
        move || {
            if listening.borrow().is_some() {
                return;
            }
            match Listen::start(&core, listen_volume.clone()) {
                Ok(listen) => drop(listening.replace(Some(listen))),
                Err(err) => eprintln!("Could not start listening: {err}"),
            }
        }
    };
    let _commands = command_receiver.attach(mainloop.loop_(), {
        let input = input.clone();
        let reference_stream = reference_stream.clone();
        let listening = listening.clone();
        let start_listening = start_listening.clone();
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
            Control::SetListen(true) => start_listening(),
        }
    });

    // not `connect`, the session manager mustn't link it
    let format = format_param();
    output.connect(
        spa::utils::Direction::Output,
        None,
        StreamFlags::MAP_BUFFERS | StreamFlags::RT_PROCESS,
        &mut [Pod::from_bytes(&format).unwrap()],
    )?;
    // "Voice Filter" may not exist yet, Listen waits for it (it never falls back to another source)
    if listen_on {
        start_listening();
    }

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
    // Without the connection (PipeWire restarted, or the client was destroyed) the streams are
    // gone for good, and the apps would record the silence of "Voice Filter" while this keeps
    // running. Failing lets a service manager start it again.
    let disconnected = Rc::new(Cell::new(false));
    let _core_listener = core
        .add_listener_local()
        .error({
            let disconnected = disconnected.clone();
            let quit = quit(&mainloop);
            move |id, _, res, message| {
                eprintln!("PipeWire error (object {id}, {res}): {message}");
                if id == pw::core::PW_ID_CORE && res == -EPIPE {
                    disconnected.set(true);
                    quit();
                }
            }
        })
        .register();

    eprintln!(
        "\"Voice Filter\" is ready, the chain adds {:.1} ms",
        samples_ms(chain_latency as f64)
    );
    report_periodically(stats, chain_latency, verbose);
    mainloop.run();
    if disconnected.get() {
        bail!("Lost the connection to PipeWire");
    }
    Ok(())
}

/// The error PipeWire reports on the core when the connection is gone.
const EPIPE: i32 = 32;

const LISTEN_NODE_NAME: &str = "voice-filter-listen";

/// Plays what "Voice Filter" outputs on the default output while it lives: a capture stream from
/// it and a playback stream, with a bridge between them whose only stage is the [`Volume`]. As it
/// records from "Voice Filter" like an app, the filter runs while listening even if no app
/// records.
///
/// The two streams are in no group: PipeWire puts the nodes of a group (or link group) on one
/// driver, so the output device would run under the microphone's clock. A Bluetooth headset woken
/// from suspend that way stayed silent until Listen was switched off and on again. The output
/// device drives the playback stream itself, and the bridge hands over between the two clocks,
/// as for the reference of the echo canceller.
struct Listen {
    _capture: StreamRc,
    _playback: StreamRc,
    _capture_listener: pw::stream::StreamListener<()>,
    _playback_listener: pw::stream::StreamListener<()>,
}

impl Listen {
    fn start(core: &pw::core::CoreRc, volume: Param) -> Result<Self> {
        let (mut capture, mut playback, _) =
            bridge(Chain::new(vec![Box::new(Volume::new(volume))]));
        let description = "Voice Filter (listening)";
        let capture_stream = stream(
            core,
            LISTEN_NODE_NAME,
            description,
            Some(NODE_NAME),
            &[
                (*pw::keys::MEDIA_CATEGORY, "Capture"),
                // without "Voice Filter" WirePlumber would link the default source, i.e. play
                // the unfiltered microphone
                ("node.dont-fallback", "true"),
                // linked to the default source ("Voice Filter" often is), WirePlumber sets the
                // target of a stream to follow the default (`target.node` -1) unless it has a
                // link group or doesn't reconnect, then finds no target and destroys it
                // (dont-fallback)
                (*pw::keys::NODE_DONT_RECONNECT, "true"),
            ],
        )?;
        let playback_stream = stream(
            core,
            LISTEN_NODE_NAME,
            description,
            None,
            &[(*pw::keys::MEDIA_CATEGORY, "Playback")],
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
        connect(&capture_stream, spa::utils::Direction::Input)?;
        connect(&playback_stream, spa::utils::Direction::Output)?;
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
    connect(input, spa::utils::Direction::Input)
}

/// A stream of mono audio named `name`, asking for a quantum of a frame, with `properties` on top.
/// It links to the node `target`, without one to the default the session manager picks.
fn stream(
    core: &pw::core::CoreRc,
    name: &str,
    description: &str,
    target: Option<&str>,
    properties: &[(&str, &str)],
) -> Result<StreamRc> {
    // the graph may still run a smaller quantum
    let latency = format!("{FRAME_LEN}/{SAMPLE_RATE}");
    let mut all = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::NODE_NAME => name,
        *pw::keys::NODE_DESCRIPTION => description,
        *pw::keys::NODE_LATENCY => latency.as_str(),
        *pw::keys::AUDIO_CHANNELS => "1",
    };
    for (key, value) in properties {
        all.insert(*key, *value);
    }
    if let Some(target) = target {
        all.insert(*pw::keys::TARGET_OBJECT, target);
    }
    Ok(StreamRc::new(core.clone(), name, all)?)
}

/// Connects a stream in [`format_param`].
fn connect(stream: &StreamRc, direction: spa::utils::Direction) -> Result<()> {
    let format = format_param();
    stream.connect(
        direction,
        None,
        StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS | StreamFlags::RT_PROCESS,
        &mut [Pod::from_bytes(&format).unwrap()],
    )?;
    Ok(())
}

/// The virtual microphone, the output stream's link into it and the capture streams, kept up to
/// date from the registry.
///
/// "Voice Filter" is a node of PipeWire's own (a null sink in an adapter, `object.linger`), not a
/// stream of this process: it outlives a restart or crash of the filter, so the apps recording
/// from it stay linked (and record silence meanwhile). It's created once the registry shows it
/// doesn't exist, and again if it's destroyed while the filter runs. The output stream's link
/// into it is the filter's own and goes with the process.
///
/// The capture streams are connected while something records from "Voice Filter", i.e. a link
/// from it exists, and disconnected when the last one is gone: just inactive, the microphone's
/// would still count as recording, and GNOME would show it as in use.
struct Graph {
    core: pw::core::CoreRc,
    captures: [StreamRc; 2],
    /// The proxy of "Voice Filter" while it's being created.
    creating: Option<pw::node::Node>,
    /// The id of "Voice Filter".
    source: Option<u32>,
    /// The id of the output stream's node.
    output: Option<u32>,
    /// The node of every port and whether it's an input.
    ports: HashMap<u32, (u32, bool)>,
    /// The output and input node of every link.
    links: HashMap<u32, (u32, u32)>,
    /// The output stream's link into "Voice Filter", with the two nodes.
    link: Option<(pw::link::Link, (u32, u32))>,
    recording: bool,
}

/// The listeners that keep a [`Graph`] up to date while they live.
struct GraphListeners {
    _registry: pw::registry::Listener,
    _core: pw::core::Listener,
}

impl Graph {
    fn watch(
        core: &pw::core::CoreRc,
        registry: &pw::registry::RegistryRc,
        captures: [StreamRc; 2],
    ) -> Result<GraphListeners> {
        let graph = Rc::new(RefCell::new(Self {
            core: core.clone(),
            captures,
            creating: None,
            source: None,
            output: None,
            ports: HashMap::new(),
            links: HashMap::new(),
            link: None,
            recording: false,
        }));
        let registry_listener = registry
            .add_listener_local()
            .global({
                let graph = graph.clone();
                move |global| {
                    let Some(props) = global.props else { return };
                    let id = |key| props.get(key).and_then(|id| id.parse::<u32>().ok());
                    let mut graph = graph.borrow_mut();
                    match global.type_ {
                        ObjectType::Node => match props.get(*pw::keys::NODE_NAME) {
                            Some(NODE_NAME) => {
                                graph.source = Some(global.id);
                                graph.creating = None;
                            }
                            Some(OUTPUT_NODE_NAME) => graph.output = Some(global.id),
                            _ => return,
                        },
                        ObjectType::Port => {
                            let Some(node) = id(*pw::keys::NODE_ID) else {
                                return;
                            };
                            let input = props.get(*pw::keys::PORT_DIRECTION) == Some("in");
                            graph.ports.insert(global.id, (node, input));
                        }
                        ObjectType::Link => {
                            let (Some(from), Some(to)) = (
                                id(*pw::keys::LINK_OUTPUT_NODE),
                                id(*pw::keys::LINK_INPUT_NODE),
                            ) else {
                                return;
                            };
                            graph.links.insert(global.id, (from, to));
                        }
                        _ => return,
                    }
                    graph.update();
                }
            })
            .global_remove({
                let graph = graph.clone();
                move |id| graph.borrow_mut().remove(id)
            })
            .register();
        // all globals are announced once the server answers the sync
        let pending = core.sync(0)?;
        let core_listener = core
            .add_listener_local()
            .done(move |id, seq| {
                if id == pw::core::PW_ID_CORE && seq == pending {
                    graph.borrow_mut().create_source();
                }
            })
            .register();
        Ok(GraphListeners {
            _registry: registry_listener,
            _core: core_listener,
        })
    }

    fn remove(&mut self, id: u32) {
        self.ports.remove(&id);
        // The proxy goes only with the link, after the server is done with it (dropping it while
        // the server removes it is an error). A node's global goes before its links.
        if let Some(nodes) = self.links.remove(&id)
            && self
                .link
                .as_ref()
                .is_some_and(|&(_, linked)| linked == nodes)
        {
            self.link = None;
        }
        if Some(id) == self.output {
            self.output = None;
        }
        if Some(id) == self.source {
            self.source = None;
            self.create_source();
        }
        self.update();
    }

    /// Creates "Voice Filter" unless it exists or is being created.
    fn create_source(&mut self) {
        if self.source.is_some() || self.creating.is_some() {
            return;
        }
        let rate = SAMPLE_RATE.to_string();
        let created = self.core.create_object::<pw::node::Node>(
            "adapter",
            &properties! {
                *pw::keys::FACTORY_NAME => "support.null-audio-sink",
                *pw::keys::NODE_NAME => NODE_NAME,
                *pw::keys::NODE_DESCRIPTION => "Voice Filter",
                // the session manager gives a virtual source ports for both directions, an
                // `Audio/Source` none
                *pw::keys::MEDIA_CLASS => "Audio/Source/Virtual",
                *pw::keys::AUDIO_RATE => rate.as_str(),
                "audio.position" => "[ MONO ]",
                *pw::keys::NODE_GROUP => GROUP,
                *pw::keys::NODE_LINK_GROUP => GROUP,
                *pw::keys::OBJECT_LINGER => "true",
            },
        );
        match created {
            Ok(node) => self.creating = Some(node),
            Err(err) => eprintln!("Could not create \"Voice Filter\": {err}"),
        }
    }

    /// Links the output stream into "Voice Filter" once both have their ports, and switches the
    /// capture streams when the recording starts or stops.
    fn update(&mut self) {
        let recording = self
            .source
            .is_some_and(|source| self.links.values().any(|&(from, _)| from == source));
        if recording != self.recording {
            self.recording = recording;
            for capture in &self.captures {
                let result = if recording {
                    connect_capture(capture)
                } else {
                    capture.disconnect().map_err(Into::into)
                };
                if let Err(err) = result {
                    eprintln!("Could not switch the capture streams: {err}");
                }
            }
        }
        let (Some(output), Some(source)) = (self.output, self.source) else {
            return;
        };
        if self
            .link
            .as_ref()
            .is_some_and(|&(_, nodes)| nodes == (output, source))
        {
            return;
        }
        let port = |node, input| {
            self.ports
                .iter()
                .find(|&(_, &port)| port == (node, input))
                .map(|(&id, _)| id.to_string())
        };
        let (Some(from), Some(to)) = (port(output, false), port(source, true)) else {
            return;
        };
        let link = self.core.create_object::<pw::link::Link>(
            "link-factory",
            &properties! {
                *pw::keys::LINK_OUTPUT_NODE => output.to_string().as_str(),
                *pw::keys::LINK_OUTPUT_PORT => from.as_str(),
                *pw::keys::LINK_INPUT_NODE => source.to_string().as_str(),
                *pw::keys::LINK_INPUT_PORT => to.as_str(),
            },
        );
        match link {
            // replaces one to a node that's gone
            Ok(link) => self.link = Some((link, (output, source))),
            Err(err) => eprintln!("Could not link the filter to \"Voice Filter\": {err}"),
        }
    }
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

/// How often [`report_periodically`] looks at the counters; it hands the processing time to the
/// control interface every second, over this window.
const REPORT_INTERVAL: Duration = Duration::from_secs(WINDOW_SECONDS as u64);

/// Hands the processing time of the frames over the last [`REPORT_INTERVAL`] to the control
/// interface every second ([`Window::push`]) and, every [`REPORT_INTERVAL`], prints the counters,
/// the latency and the processing time while frames are processed:
/// `verbose` every time, otherwise only when the chain failed on frames or allocated outside the
/// pool of [`rt_alloc`], or when the playback side filled in silence or dropped samples in the
/// middle of a recording. At its start and end that's
/// how the bridge settles, so an interval counts only if frames ran through all of it and the one
/// before.
fn report_periodically(stats: Arc<Stats>, chain_latency: usize, verbose: bool) {
    // a few frames less than the interval holds, the cycles don't line up with it exactly
    let whole = REPORT_INTERVAL.as_secs() * (SAMPLE_RATE as u64 / FRAME_LEN as u64) * 99 / 100;
    ::std::thread::spawn(move || {
        let mut last = [0; 5];
        let mut was_running = false;
        let mut windows: [Window; FRAME_TIME_PARTS.len()] = Default::default();
        for second in (1..=WINDOW_SECONDS).cycle() {
            ::std::thread::sleep(Duration::from_secs(1));
            let times: [_; FRAME_TIME_PARTS.len()] =
                ::std::array::from_fn(|part| windows[part].push(&stats.frame_times[part]));
            if second != WINDOW_SECONDS {
                continue;
            }
            let now = [
                stats.frames.load(Ordering::Relaxed),
                stats.underruns.load(Ordering::Relaxed),
                stats.dropped.load(Ordering::Relaxed),
                stats.errors.load(Ordering::Relaxed),
                rt_alloc::stats().map_or(0, |pool| pool.outside),
            ];
            let max_ms = times[0].max_ns as f64 / 1e6;
            let frames = now[0] - last[0];
            let steady = was_running && frames >= whole;
            let failed = now[3] != last[3] || now[4] != last[4];
            let glitched = now[1] != last[1] || now[2] != last[2];
            if frames > 0 && (verbose || failed || steady && glitched) {
                let parts = stats.latency(chain_latency);
                let total: i64 = parts.iter().map(|(_, samples)| samples).sum();
                let parts = parts
                    .map(|(name, samples)| format!("{name} {:.1}", samples_ms(samples as f64)))
                    .join(" + ");
                eprintln!(
                    "{frames} frames, max {max_ms:.2} ms per frame, {} samples silence, {} dropped, \
                     {} errors",
                    now[1] - last[1],
                    now[2] - last[2],
                    now[3] - last[3],
                );
                eprintln!(
                    "latency {:.1} ms: {parts}, quantum {}",
                    samples_ms(total as f64),
                    stats.quantum.load(Ordering::Relaxed),
                );
                let times = FRAME_TIME_PARTS
                    .iter()
                    .zip(&times)
                    .map(|(part, time)| {
                        let ms = |ns: u64| ns as f64 / 1e6;
                        format!(
                            "{part} {:.2}/{:.2}/{:.2}/{:.2}",
                            ms(time.mean_ns),
                            ms(time.p95_ns),
                            ms(time.p99_ns),
                            ms(time.max_ns),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                eprintln!("time per frame in ms, mean/95 %/99 %/max: {times}");
                if let Some(pool) = rt_alloc::stats() {
                    eprintln!(
                        "pool: {} of {} KiB used, at most {}, {} allocations outside it",
                        pool.used / 1024,
                        pool.size / 1024,
                        pool.peak / 1024,
                        now[4] - last[4],
                    );
                }
            }
            was_running = frames > 0;
            last = now;
        }
    });
}
