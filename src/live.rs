//! The live filter: a capture stream from the microphone and an output stream, with the chain
//! between them (see [`crate::bridge`]), and a capture stream from the monitor of an output device
//! as the reference of the echo canceller. In the automatic [`Mode`] the output stream is the
//! source "Voice Filter" itself, a smart filter WirePlumber links between the default input and
//! the apps that record from it; in the manual one it plays into "Voice Filter", a node of
//! PipeWire's own that outlives the process (see `Graph`).

use ::std::cell::{Cell, RefCell};
use ::std::collections::HashMap;
use ::std::path::PathBuf;
use ::std::rc::{Rc, Weak};
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
use ::zbus::object_server::InterfaceRef;

use crate::bridge::{Capture, FRAME_TIME_PARTS, Playback, Stats, bridge};
use crate::chain::Chain;
use crate::dbus::{self, Control, Service};
use crate::echo::Reference;
use crate::frame_time::{WINDOW_SECONDS, Window};
use crate::param::Param;
use crate::rt_alloc;
use crate::settings::{self, Mode};
use crate::tray::{self, Toggle, Tray};
use crate::volume::{self, Volume};
use crate::{FRAME_LEN, SAMPLE_RATE, samples_ms};

/// The virtual microphone apps record from: the null sink of the manual mode, or the output stream
/// itself in the automatic one.
const NODE_NAME: &str = "voice-filter";
/// The output stream in the manual mode, which plays the filtered microphone into the null sink.
const OUTPUT_NODE_NAME: &str = "voice-filter-output";
/// The metadata of WirePlumber's smart filters, there from version 0.5 on.
const FILTERS_METADATA: &str = "filters";
/// The metadata of the default devices, and the key of the default input in it.
const DEFAULT_METADATA: &str = "default";
const DEFAULT_SOURCE_KEY: &str = "default.audio.source";
/// The key of the smart filter's target in the metadata of the smart filters.
const SMART_TARGET_KEY: &str = "filter.smart.target";
const CAPTURE_NODE_NAME: &str = "voice-filter-capture";
const REFERENCE_NODE_NAME: &str = "voice-filter-reference";
/// The capture stream, the output stream and the virtual microphone are in this group, so they
/// run in the same cycle of the microphone's driver (rather than the virtual microphone's timer),
/// and in this link group, so the session manager never links the capture stream to the virtual
/// microphone. (PipeWire schedules both, group and link group, on one driver.) In the automatic
/// mode the link group makes the capture stream the input of the smart filter, and it's the
/// filter's name.
const GROUP: &str = "voice-filter";
/// The largest quantum the callbacks are prepared for without allocating.
const MAX_QUANTUM: usize = 8192;
const SAMPLE_SIZE: usize = size_of::<f32>();

/// The devices to read from and to take the reference from, and which apps get the filter.
pub struct Devices<'a> {
    /// The mode chosen; without smart filters in the session manager it's the manual one.
    pub mode: Mode,
    /// The `node.name` of the microphone in the manual mode, without it the session manager links
    /// the default input.
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
    service: InterfaceRef<Service>,
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
    // Listen as the app switches it, so it starts its streams and keeps it the same way
    let get = service.clone();
    let listen = Toggle {
        get: Box::new(move || ::zbus::block_on(async { get.get().await.listen() })),
        set: Box::new(move |on| {
            let result = ::zbus::block_on(async { service.get_mut().await.set_listen(on) });
            if let Err(err) = result {
                eprintln!("Could not switch Listen: {err}");
            }
        }),
    };
    let tray = Tray {
        enabled,
        switches,
        params,
        settings_dir,
        listen: Some(listen),
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
        mode,
        input: input_name,
        output: output_device,
    } = devices;
    let auto_available = Arc::new(AtomicBool::new(false));
    let active_input = Arc::new(Mutex::new(String::new()));
    let default_input = Arc::new(Mutex::new(String::new()));
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
        match settings::load_listen(dir) {
            Ok(listen) => keep_listen = listen,
            Err(err) => eprintln!("Skipped the saved switch of Listen: {err:#}"),
        }
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
    let service = dbus::serve(Service {
        enabled,
        switched: Box::new({
            let tray = tray.clone();
            move || {
                if let Some(tray) = tray.get().cloned() {
                    // reads the switches again, updates icon, tooltip and menu; not here, as a
                    // click in the tray's own menu switches Listen through the control interface
                    // and holds the tray until it's done, `update` would wait for it forever
                    let _ = ::std::thread::Builder::new()
                        .name("tray-update".into())
                        .spawn(move || tray.update(|_| ()));
                }
            }
        }),
        switches,
        params,
        stats: stats.clone(),
        echo: Some(reference.status()),
        chain_latency,
        mode,
        auto_available: auto_available.clone(),
        active_input: active_input.clone(),
        default_input: default_input.clone(),
        input: input_name.unwrap_or_default().to_owned(),
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
    if let Some(handle) = show_tray(enabled, switches, params, settings_dir, service) {
        let _ = tray.set(handle);
    }

    let (mainloop, _context, core, registry) = connect_server()?;
    let _inputs_listener = watch_devices(&registry, Direction::Input, inputs);
    let _outputs_listener = watch_devices(&registry, Direction::Output, outputs);

    // What the chain outputs, the source "Voice Filter" or into it, see `output_properties`; set
    // up by `Graph` once it knows which mode it can run in
    let output = stream(
        &core,
        OUTPUT_NODE_NAME,
        "Voice Filter (output)",
        None,
        &[
            (*pw::keys::MEDIA_CATEGORY, "Playback"),
            (*pw::keys::NODE_GROUP, GROUP),
            (*pw::keys::NODE_LINK_GROUP, GROUP),
        ],
    )?;
    // Without a target, the session manager links the default input; if that's "Voice Filter"
    // itself, the link group rules it out and it falls back to the source with the highest
    // priority, whichever that is (smart filters aside). `Graph` sets the target.
    let input = stream(
        &core,
        CAPTURE_NODE_NAME,
        "Voice Filter (microphone)",
        None,
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
    let input = ListenedStream::new(input, |input| {
        input
            .add_local_listener_with_user_data(())
            .process(move |stream, _| {
                let delay = read_into(stream, &mut capture, &mut samples);
                capture_stats.capture_delay.store(delay, Ordering::Relaxed);
            })
            .register()
    })?;

    let mut played = Vec::with_capacity(MAX_QUANTUM);
    let reference_stream = ListenedStream::new(reference_stream, |reference_stream| {
        reference_stream
            .add_local_listener_with_user_data(())
            .process(move |stream, _| {
                // what doesn't fit while the echo canceller is off or behind gets dropped
                if dequeue_samples(stream, &mut played) {
                    reference.push(&played);
                }
            })
            .register()
    })?;

    let mut pulled = vec![0.; MAX_QUANTUM];
    let playback_stats = stats.clone();
    let output = ListenedStream::new(output, |output| {
        output
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
            .register()
    })?;
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
    // "Voice Filter" is another node after switching the mode, Listen's capture stream doesn't
    // follow (it doesn't reconnect)
    let restart_listening = {
        let (listening, start_listening) = (listening.clone(), start_listening.clone());
        move || {
            if listening.take().is_some() {
                start_listening();
            }
        }
    };
    let graph = Graph::watch(
        &core,
        &registry,
        Streams {
            output: output.stream.clone(),
            captures: [input.stream.clone(), reference_stream.stream.clone()],
        },
        Setup {
            mode,
            input: input_name.unwrap_or_default().to_owned(),
            auto_available,
            switched: Box::new(restart_listening),
            active_input,
            default_input,
        },
    )?;

    let _commands = command_receiver.attach(mainloop.loop_(), {
        let graph = graph.graph.clone();
        let reference_stream = reference_stream.stream.clone();
        let listening = listening.clone();
        let start_listening = start_listening.clone();
        move |control| match control {
            Control::SetInput(name) => graph.borrow_mut().set_input(name),
            Control::SetMode(mode) => graph.borrow_mut().set_mode(mode),
            Control::SetOutput(name) => {
                if let Err(err) = set_target(&reference_stream, &name) {
                    eprintln!("Could not take the reference from {name}: {err}");
                }
            }
            Control::SetListen(false) => drop(listening.take()),
            Control::SetListen(true) => start_listening(),
        }
    });

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
    _capture: ListenedStream,
    _playback: ListenedStream,
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
        let capture_stream = ListenedStream::new(capture_stream, |capture_stream| {
            capture_stream
                .add_local_listener_with_user_data(())
                .process(move |stream, _| {
                    read_into(stream, &mut capture, &mut samples);
                })
                .register()
        })?;
        let mut pulled = vec![0.; MAX_QUANTUM];
        let playback_stream = ListenedStream::new(playback_stream, |playback_stream| {
            playback_stream
                .add_local_listener_with_user_data(())
                .process(move |stream, _| {
                    write_from(stream, &mut playback, &mut pulled);
                })
                .register()
        })?;
        connect(&capture_stream.stream, spa::utils::Direction::Input)?;
        connect(&playback_stream.stream, spa::utils::Direction::Output)?;
        Ok(Self {
            _capture: capture_stream,
            _playback: playback_stream,
        })
    }
}

/// A stream with its listener, which goes first: pipewire-rs's listener doesn't keep its stream
/// alive, removed from a destroyed stream it writes into the freed memory. Dropping it disconnects
/// the stream, even one with other owners.
struct ListenedStream {
    // before the stream, fields drop in order
    _listener: pw::stream::StreamListener<()>,
    stream: StreamRc,
}

impl ListenedStream {
    /// `stream` with the listener `listen` registers on it.
    fn new(
        stream: StreamRc,
        listen: impl FnOnce(&StreamRc) -> Result<pw::stream::StreamListener<()>, pw::Error>,
    ) -> Result<Self> {
        let listener = listen(&stream)?;
        Ok(Self {
            _listener: listener,
            stream,
        })
    }
}

impl Drop for ListenedStream {
    fn drop(&mut self) {
        // before the listener goes, so the data loop no longer calls it (`RT_PROCESS`)
        let _ = self.stream.disconnect();
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

/// Sets `properties` of a stream. A connected stream only follows them when it connects again.
fn update_properties(stream: &StreamRc, properties: &[(&str, &str)]) -> Result<()> {
    let mut all = pw::properties::PropertiesBox::new();
    for (key, value) in properties {
        all.insert(*key, *value);
    }
    // SAFETY: both pointers are valid during the call, which copies the properties
    let result = unsafe {
        pw::sys::pw_stream_update_properties(stream.as_raw_ptr(), &(*all.as_raw_ptr()).dict)
    };
    if result < 0 {
        bail!("pw_stream_update_properties failed with {result}");
    }
    Ok(())
}

/// Points a capture stream to the device `name`, the empty name for the default one, and connects
/// it again if it's connected, so it follows.
fn set_target(input: &StreamRc, name: &str) -> Result<()> {
    update_properties(input, &[(*pw::keys::TARGET_OBJECT, name)])?;
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

/// Connects a stream in [`format_param`], for the session manager to link.
fn connect(stream: &StreamRc, direction: spa::utils::Direction) -> Result<()> {
    connect_with(stream, direction, StreamFlags::AUTOCONNECT)
}

/// Connects a stream in [`format_param`] with `flags` on top of what every stream has.
fn connect_with(
    stream: &StreamRc,
    direction: spa::utils::Direction,
    flags: StreamFlags,
) -> Result<()> {
    let format = format_param();
    stream.connect(
        direction,
        None,
        flags | StreamFlags::MAP_BUFFERS | StreamFlags::RT_PROCESS,
        &mut [Pod::from_bytes(&format).unwrap()],
    )?;
    Ok(())
}

/// The properties of the output stream in `mode`.
///
/// Automatic: the stream is the source "Voice Filter", a smart filter. WirePlumber links every
/// app that records from the default input to it instead, and the capture stream, in its link
/// group, to the default input; without `filter.smart.target` it follows the default input, and
/// apps that record from another microphone by name pass it by. It never picks a smart filter as
/// the default input itself.
///
/// Manual: a playback stream into the null sink "Voice Filter". The session manager links playback
/// streams only to sinks, so `Graph` links it. Passive, so its link doesn't keep "Voice Filter"
/// running while nothing records from it (`link.passive` on the link has no effect).
fn output_properties(mode: Mode) -> [(&'static str, &'static str); 7] {
    match mode {
        Mode::Auto => [
            (*pw::keys::NODE_NAME, NODE_NAME),
            (*pw::keys::NODE_DESCRIPTION, "Voice Filter"),
            (*pw::keys::MEDIA_CLASS, "Audio/Source"),
            (*pw::keys::NODE_AUTOCONNECT, "true"),
            (*pw::keys::NODE_PASSIVE, "false"),
            ("filter.smart", "true"),
            ("filter.smart.name", GROUP),
        ],
        Mode::Manual => [
            (*pw::keys::NODE_NAME, OUTPUT_NODE_NAME),
            (*pw::keys::NODE_DESCRIPTION, "Voice Filter (output)"),
            (*pw::keys::MEDIA_CLASS, "Stream/Output/Audio"),
            (*pw::keys::NODE_AUTOCONNECT, "false"),
            (*pw::keys::NODE_PASSIVE, "true"),
            ("filter.smart", "false"),
            ("filter.smart.name", GROUP),
        ],
    }
}

/// The streams `Graph` keeps in place.
struct Streams {
    /// What the chain outputs, see [`output_properties`].
    output: StreamRc,
    /// The microphone's and the reference's, connected while something records.
    captures: [StreamRc; 2],
}

/// What `Graph` starts with.
struct Setup {
    /// The mode chosen.
    mode: Mode,
    /// The microphone of the manual mode, empty for the default input.
    input: String,
    /// Set once the registry shows whether the session manager has smart filters.
    auto_available: Arc<AtomicBool>,
    /// Called after the streams were set up for another mode.
    switched: Box<dyn Fn()>,
    /// The `node.name` of the source the capture stream reads from, empty while it reads from
    /// none; kept up to date for the control interface.
    active_input: Arc<Mutex<String>>,
    /// The `node.name` of the default input, from the metadata `default`, empty without one.
    default_input: Arc<Mutex<String>>,
}

/// The output stream in the mode it can run in, the virtual microphone of the manual mode, the
/// output stream's link into it and the capture streams, kept up to date from the registry.
///
/// The mode is set up once the registry has shown everything: the automatic one if it's chosen
/// and the metadata of the smart filters is there, otherwise the manual one; again when another
/// one is chosen, or the metadata appears later (WirePlumber started after the filter). It stays
/// automatic when the metadata goes, WirePlumber restarting. Switching connects the output stream
/// again with other properties, so "Voice Filter" is a new node, and drops the sound for a moment.
///
/// In the manual mode "Voice Filter" is a node of PipeWire's own (a null sink in an adapter,
/// `object.linger`), not a stream of this process: it outlives a restart or crash of the filter,
/// so the apps recording from it stay linked (and record silence meanwhile). It's created once
/// the registry shows it doesn't exist, and again if it's destroyed while the filter runs; in the
/// automatic mode it's destroyed, and its apps go to the default input, i.e. to the filter. The
/// output stream's link into it is the filter's own and goes with the process. In the automatic
/// mode the filter goes with the process too, and WirePlumber links the apps to the default input
/// directly.
///
/// The capture streams are connected while something records from "Voice Filter", i.e. a link
/// from it exists, and disconnected when the last one is gone: just inactive, the microphone's
/// would still count as recording, and GNOME would show it as in use.
struct Graph {
    core: pw::core::CoreRc,
    registry: pw::registry::RegistryRc,
    streams: Streams,
    setup: Setup,
    /// Whether the registry has shown everything once.
    synced: bool,
    /// The mode the streams are set up for.
    running: Option<Mode>,
    /// The id of the metadata of the smart filters, and a proxy to write the target into it.
    filters: Option<u32>,
    filters_metadata: Option<pw::metadata::Metadata>,
    /// The output stream's node and the microphone its `filter.smart.target` was set to.
    target: Option<(u32, String)>,
    /// The proxy of the null sink "Voice Filter" while it's being created.
    creating: Option<pw::node::Node>,
    /// The id of the null sink "Voice Filter".
    source: Option<u32>,
    /// The id of the output stream's node.
    output: Option<u32>,
    /// The id of the capture stream's node, while it's connected.
    capture: Option<u32>,
    /// The `node.name` of every audio source.
    sources: HashMap<u32, String>,
    /// The metadata `default` with the listener that reads the default input from it.
    defaults: Option<ListenedMetadata>,
    /// The node of every port and whether it's an input.
    ports: HashMap<u32, (u32, bool)>,
    /// The output and input node of every link.
    links: HashMap<u32, (u32, u32)>,
    /// The output stream's link into "Voice Filter", with the two nodes.
    link: Option<(pw::link::Link, (u32, u32))>,
    recording: bool,
}

/// A metadata with a listener on it, which goes first: the listener doesn't keep the proxy alive,
/// and a proxy the server has removed (the session manager restarting) is freed when it's dropped,
/// so removed after it the listener writes into the freed memory.
struct ListenedMetadata {
    /// The id of its global.
    id: u32,
    // before the proxy, fields drop in order
    _listener: pw::metadata::MetadataListener,
    _metadata: pw::metadata::Metadata,
}

/// The listeners that keep a [`Graph`] up to date while they live, and the graph.
struct GraphListeners {
    graph: Rc<RefCell<Graph>>,
    _registry: pw::registry::Listener,
    _core: pw::core::Listener,
}

impl Graph {
    fn watch(
        core: &pw::core::CoreRc,
        registry: &pw::registry::RegistryRc,
        streams: Streams,
        setup: Setup,
    ) -> Result<GraphListeners> {
        let graph = Rc::new(RefCell::new(Self {
            core: core.clone(),
            registry: registry.clone(),
            streams,
            setup,
            synced: false,
            running: None,
            filters: None,
            filters_metadata: None,
            target: None,
            creating: None,
            source: None,
            output: None,
            capture: None,
            sources: HashMap::new(),
            defaults: None,
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
                    let weak = Rc::downgrade(&graph);
                    let mut graph = graph.borrow_mut();
                    match global.type_ {
                        ObjectType::Node => {
                            let class = props.get(*pw::keys::MEDIA_CLASS);
                            let name = props.get(*pw::keys::NODE_NAME);
                            if let (Some(class), Some(name)) = (class, name)
                                && class.starts_with("Audio/Source")
                            {
                                graph.sources.insert(global.id, name.to_owned());
                            }
                            match name {
                                // the null sink, not the output stream of the automatic mode
                                Some(NODE_NAME) if class == Some("Audio/Source/Virtual") => {
                                    graph.source = Some(global.id);
                                    graph.creating = None;
                                }
                                Some(NODE_NAME | OUTPUT_NODE_NAME) => {
                                    graph.output = Some(global.id);
                                }
                                Some(CAPTURE_NODE_NAME) => graph.capture = Some(global.id),
                                _ => return,
                            }
                        }
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
                        ObjectType::Metadata => match props.get("metadata.name") {
                            Some(FILTERS_METADATA) => {
                                graph.filters = Some(global.id);
                                let registry = graph.registry.clone();
                                match registry.bind::<pw::metadata::Metadata, _>(global) {
                                    Ok(metadata) => graph.filters_metadata = Some(metadata),
                                    Err(err) => {
                                        eprintln!("Could not bind the smart filters: {err}")
                                    }
                                }
                                graph.apply_mode();
                            }
                            Some(DEFAULT_METADATA) => {
                                let registry = graph.registry.clone();
                                match registry.bind::<pw::metadata::Metadata, _>(global) {
                                    Ok(metadata) => graph.watch_defaults(global.id, metadata, weak),
                                    Err(err) => {
                                        eprintln!("Could not read the default input: {err}")
                                    }
                                }
                                return;
                            }
                            _ => return,
                        },
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
            .done({
                let graph = graph.clone();
                move |id, seq| {
                    if id == pw::core::PW_ID_CORE && seq == pending {
                        let mut graph = graph.borrow_mut();
                        graph.synced = true;
                        graph.apply_mode();
                        graph.update();
                    }
                }
            })
            .register();
        Ok(GraphListeners {
            graph,
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
        if Some(id) == self.capture {
            self.capture = None;
        }
        self.sources.remove(&id);
        // The proxies of the metadata go with their globals: the server removes what's bound to a
        // global before it announces the removal (`pw_global_destroy`), so they've been removed
        // by now and dropping them asks the server for nothing. Not so the link above, which
        // outlives the global of its node.
        if self
            .defaults
            .as_ref()
            .is_some_and(|defaults| defaults.id == id)
        {
            self.defaults = None;
        }
        if Some(id) == self.filters {
            self.filters = None;
            self.filters_metadata = None;
            self.target = None;
            self.apply_mode();
        }
        if Some(id) == self.source {
            self.source = None;
            if self.running == Some(Mode::Manual) {
                self.create_source();
            }
        }
        self.update();
    }

    /// Keeps [`Setup::default_input`] up to date from the metadata `default`.
    fn watch_defaults(
        &mut self,
        id: u32,
        metadata: pw::metadata::Metadata,
        graph: Weak<RefCell<Self>>,
    ) {
        let listener = metadata
            .add_listener_local()
            .property(move |subject, key, _, value| {
                // `None` as the key clears everything
                if subject != pw::core::PW_ID_CORE
                    || key.is_some_and(|key| key != DEFAULT_SOURCE_KEY)
                {
                    return 0;
                }
                if let Some(graph) = graph.upgrade() {
                    let name = value.and_then(metadata_node_name).unwrap_or_default();
                    graph.borrow_mut().set_default_input(name);
                }
                0
            })
            .register();
        self.defaults = Some(ListenedMetadata {
            id,
            _listener: listener,
            _metadata: metadata,
        });
    }

    fn set_default_input(&mut self, name: String) {
        let mut default = self.setup.default_input.lock().unwrap();
        if *default == name {
            return;
        }
        if name == NODE_NAME && self.running == Some(Mode::Auto) {
            eprintln!(
                "\"Voice Filter\" is the default input, so the filter reads from another \
                 source; choose the microphone in the system's sound settings"
            );
        }
        *default = name;
    }

    /// Switches to `mode`, if it can run in it.
    fn set_mode(&mut self, mode: Mode) {
        self.setup.mode = mode;
        self.apply_mode();
        self.update();
    }

    /// Reads from the microphone `name` in the manual mode, the empty name for the default input.
    fn set_input(&mut self, name: String) {
        if self.running == Some(Mode::Manual)
            && let Err(err) = set_target(&self.streams.captures[0], &name)
        {
            eprintln!("Could not switch to {name}: {err}");
        }
        self.setup.input = name;
        self.apply_target();
    }

    /// Points the smart filter to the microphone chosen, through the metadata of the smart
    /// filters (WirePlumber rescans when it changes, not when the node's properties do), or
    /// removes its target for the default input. With a target, WirePlumber links the apps that
    /// record from that microphone to the filter, and the capture stream to the microphone; while
    /// it's unplugged, no app.
    fn apply_target(&mut self) {
        if self.running != Some(Mode::Auto) {
            return;
        }
        let (Some(output), Some(metadata)) = (self.output, &self.filters_metadata) else {
            return;
        };
        let wanted = (output, self.setup.input.clone());
        if self.target.as_ref() == Some(&wanted) {
            return;
        }
        // also clears what a crashed run left on a node of the same id
        let value = (!wanted.1.is_empty()).then(|| smart_target(&wanted.1));
        metadata.set_property(
            output,
            SMART_TARGET_KEY,
            value.as_ref().map(|_| "Spa:String:JSON"),
            value.as_deref(),
        );
        self.target = Some(wanted);
    }

    /// Removes the target of the smart filter, before its node goes.
    fn clear_target(&mut self) {
        if let (Some((node, _)), Some(metadata)) = (self.target.take(), &self.filters_metadata) {
            metadata.set_property(node, SMART_TARGET_KEY, None, None);
        }
    }

    /// Sets the streams up for the mode chosen, or the manual one if the automatic one isn't
    /// possible, unless they are.
    fn apply_mode(&mut self) {
        if !self.synced {
            return;
        }
        let available = self.filters.is_some();
        self.setup
            .auto_available
            .store(available, Ordering::Relaxed);
        let mode = match (self.setup.mode, available, self.running) {
            (Mode::Manual, ..) => Mode::Manual,
            // WirePlumber restarts, nothing links meanwhile anyway
            (Mode::Auto, true, _) | (Mode::Auto, false, Some(Mode::Auto)) => Mode::Auto,
            (Mode::Auto, false, _) => {
                if self.running.is_none() {
                    eprintln!(
                        "The session manager has no smart filters (WirePlumber 0.5 or later), \
                         filtering only for the apps that record from \"Voice Filter\""
                    );
                }
                Mode::Manual
            }
        };
        if self.running == Some(mode) {
            return;
        }
        let switched = self.running.is_some();
        self.running = Some(mode);
        if let Err(err) = self.set_up(mode) {
            eprintln!("Could not set up the streams: {err:#}");
        }
        match mode {
            Mode::Auto if self.setup.input.is_empty() => {
                eprintln!("Filtering the default input for all apps that record from it")
            }
            Mode::Auto => eprintln!(
                "Filtering {} for all apps that record from it",
                self.setup.input
            ),
            Mode::Manual if self.setup.input.is_empty() => eprintln!(
                "Filtering the default input for the apps that record from \"Voice Filter\""
            ),
            Mode::Manual => eprintln!(
                "Filtering {} for the apps that record from \"Voice Filter\"",
                self.setup.input
            ),
        }
        if switched {
            (self.setup.switched)();
        }
    }

    /// Connects the output stream for `mode`, points the capture stream to its microphone and
    /// creates or destroys the null sink.
    fn set_up(&mut self, mode: Mode) -> Result<()> {
        self.clear_target();
        let Streams { output, captures } = &self.streams;
        let output = output.clone();
        // connected again when something records from the new "Voice Filter"
        for stream in captures.iter().chain([&output]) {
            if !matches!(stream.state(), StreamState::Unconnected) {
                stream.disconnect()?;
            }
        }
        // Their nodes go, but stay in the registry until the server says so: forgotten here, the
        // links of the old "Voice Filter" don't count as recording anymore, and no target is
        // written for a node that's gone. The links stay, `remove` needs them for the proxy of
        // the filter's own link; they belong to nodes this doesn't look at anymore.
        self.output = None;
        self.capture = None;
        self.recording = false;
        let input = match mode {
            Mode::Auto => "",
            Mode::Manual => &self.setup.input,
        };
        update_properties(&captures[0], &[(*pw::keys::TARGET_OBJECT, input)])?;
        update_properties(&output, &output_properties(mode))?;
        match mode {
            Mode::Auto => {
                if let Some(source) = self.source {
                    // its apps go to the default input, i.e. to the filter
                    let _ = self.registry.destroy_global(source);
                }
                connect(&output, spa::utils::Direction::Output)
            }
            Mode::Manual => {
                self.create_source();
                // not `connect`, the session manager mustn't link it
                connect_with(&output, spa::utils::Direction::Output, StreamFlags::empty())
            }
        }
    }

    /// Creates the null sink "Voice Filter" unless it exists or is being created.
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

    /// The node apps record from: the output stream in the automatic mode, the null sink in the
    /// manual one.
    fn recorded(&self) -> Option<u32> {
        match self.running? {
            Mode::Auto => self.output,
            Mode::Manual => self.source,
        }
    }

    /// Switches the capture streams when the recording starts or stops, and in the manual mode
    /// links the output stream into "Voice Filter" once both have their ports.
    fn update(&mut self) {
        let active = self
            .capture
            .and_then(|capture| self.links.values().find(|&&(_, to)| to == capture))
            .and_then(|(from, _)| self.sources.get(from))
            .cloned()
            .unwrap_or_default();
        {
            let mut input = self.setup.active_input.lock().unwrap();
            if *input != active {
                *input = active;
            }
        }
        self.apply_target();
        let recording = self
            .recorded()
            .is_some_and(|source| self.links.values().any(|&(from, _)| from == source));
        if recording != self.recording {
            self.recording = recording;
            for capture in &self.streams.captures {
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
        if self.running != Some(Mode::Manual) {
            return;
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

/// The `node.name` in a value of the metadata `default`, a JSON object such as
/// `{ "name": "alsa_input.usb-mic" }`; `None` if it has none.
pub fn metadata_node_name(value: &str) -> Option<String> {
    let start = value.find("\"name\"")? + "\"name\"".len();
    let rest = value[start..].trim_start().strip_prefix(':')?.trim_start();
    let mut chars = rest.strip_prefix('"')?.chars();
    let mut name = String::new();
    loop {
        match chars.next()? {
            '"' => return Some(name),
            '\\' => name.push(chars.next()?),
            c => name.push(c),
        }
    }
}

/// The value of `filter.smart.target` for the microphone `name`: match rules for its `node.name`,
/// as JSON.
pub fn smart_target(name: &str) -> String {
    let mut escaped = String::new();
    for c in name.chars() {
        match c {
            '"' | '\\' => {
                escaped.push('\\');
                escaped.push(c);
            }
            c => escaped.push(c),
        }
    }
    format!("{{\"node.name\":\"{escaped}\"}}")
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
    let (mainloop, _context, core, registry) = connect_server()?;
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

/// A main loop connected to the PipeWire server, with its context (kept for as long as the
/// connection), the core and the registry.
fn connect_server() -> Result<(
    pw::main_loop::MainLoopRc,
    pw::context::ContextRc,
    pw::core::CoreRc,
    pw::registry::RegistryRc,
)> {
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context
        .connect_rc(None)
        .context("Could not connect to PipeWire")?;
    let registry = core.get_registry_rc()?;
    Ok((mainloop, context, core, registry))
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
