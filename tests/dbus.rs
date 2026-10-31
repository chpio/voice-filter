//! The logic of the control interface, called directly, without a bus.

use ::std::sync::atomic::{AtomicBool, Ordering};
use ::std::sync::{Arc, Mutex};

use ::voice_filter::bridge::Stats;
use ::voice_filter::dbus::{Control, Service};
use ::voice_filter::live::Device;
use ::voice_filter::param::Param;
use ::voice_filter::settings;

fn service() -> Service {
    service_with(Arc::default(), None)
}

/// A service that records what it sends in `sent`.
fn service_with(
    sent: Arc<Mutex<Vec<Control>>>,
    settings_dir: Option<::std::path::PathBuf>,
) -> Service {
    let device = |id, name: &str, priority| Device {
        id,
        name: name.to_owned(),
        description: format!("{name} description"),
        priority,
    };
    Service {
        switches: vec![
            ("denoise", Arc::new(AtomicBool::new(true))),
            ("agc", Arc::new(AtomicBool::new(true))),
        ],
        params: vec![
            (
                "denoise",
                Param::new("limit", "Limit", "dB", (0., 100., 1.), 100.),
            ),
            (
                "agc",
                Param::new("target", "Target", "dBFS", (-30., -12., 0.5), -18.),
            ),
        ],
        stats: Arc::new(Stats::default()),
        echo: None,
        chain_latency: 1488,
        input: String::new(),
        inputs: Arc::new(Mutex::new(vec![
            device(40, "usb-mic", 2100),
            device(41, "webcam", 2009),
        ])),
        output: String::new(),
        outputs: Arc::new(Mutex::new(vec![
            device(50, "speakers", 1009),
            device(51, "hdmi", 784),
        ])),
        send: Box::new(move |control| {
            sent.lock().unwrap().push(control);
            Ok(())
        }),
        listening: false,
        keep_listen: false,
        listen_volume: ::voice_filter::volume::param(),
        settings_dir,
    }
}

#[test]
fn lists_the_parameters_of_the_stages() {
    let service = service();
    let parameters = service.parameters();
    assert_eq!(parameters.len(), 2);
    assert_eq!(
        parameters[1],
        (
            "agc".to_owned(),
            "target".to_owned(),
            "Target".to_owned(),
            "dBFS".to_owned(),
            -30.,
            -12.,
            0.5,
            -18.,
            -18.
        )
    );
}

#[test]
fn sets_parameters_within_their_range() {
    let mut service = service();
    service.set_parameter("denoise", "limit", 12.).unwrap();
    assert_eq!(service.params[0].1.get(), 12.);
    assert_eq!(service.parameters()[0].8, 12.);
    assert!(service.set_parameter("denoise", "limit", 101.).is_err());
    assert!(service.set_parameter("denoise", "limit", -1.).is_err());
    assert!(service.set_parameter("denoise", "limit", f64::NAN).is_err());
    assert_eq!(service.params[0].1.get(), 12.);
    // the name belongs to its stage
    assert!(service.set_parameter("agc", "limit", 12.).is_err());
    assert!(service.set_parameter("denoise", "target", -20.).is_err());
}

#[test]
fn switches_stages_by_name() {
    let mut service = service();
    service.set_stage_enabled("agc", false).unwrap();
    assert!(!service.switches[1].1.load(Ordering::Relaxed));
    assert_eq!(
        service.stages(),
        [("denoise".to_owned(), true), ("agc".to_owned(), false)]
    );
    assert!(service.set_stage_enabled("reverb", true).is_err());
}

#[test]
fn sums_the_latency_in_ms() {
    let service = service();
    service.stats.capture_delay.store(384, Ordering::Relaxed);
    service.stats.bridge_delay.store(448, Ordering::Relaxed);
    let latency = service.latency();
    assert_eq!(latency["microphone"], 8.);
    assert_eq!(latency["chain"], 31.);
    assert_eq!(latency["app"], 0.);
    assert!((latency["total"] - 48.333).abs() < 0.001);
}

#[test]
fn switches_to_known_inputs_and_keeps_them() {
    let dir = ::tempfile::tempdir().unwrap();
    let sent = Arc::<Mutex<Vec<Control>>>::default();
    let mut service = service_with(sent.clone(), Some(dir.path().to_owned()));
    assert_eq!(
        service.inputs()[0],
        ("usb-mic".to_owned(), "usb-mic description".to_owned(), 2100)
    );

    service.set_input("webcam".to_owned()).unwrap();
    assert_eq!(service.input(), "webcam");
    assert_eq!(
        settings::load_input(dir.path()).unwrap().as_deref(),
        Some("webcam")
    );

    // unknown names are refused and change nothing
    assert!(service.set_input("speakers".to_owned()).is_err());
    assert_eq!(service.input(), "webcam");

    // the empty name is the default input, and forgets the chosen one
    service.set_input(String::new()).unwrap();
    assert_eq!(settings::load_input(dir.path()).unwrap(), None);
    assert_eq!(
        *sent.lock().unwrap(),
        [
            Control::SetInput("webcam".to_owned()),
            Control::SetInput(String::new())
        ]
    );
}

#[test]
fn switches_listening_on_and_off() {
    let sent = Arc::<Mutex<Vec<Control>>>::default();
    let mut service = service_with(sent.clone(), None);
    assert!(!service.listen());
    service.set_listen(true).unwrap();
    assert!(service.listen());
    service.set_listen(false).unwrap();
    assert_eq!(
        *sent.lock().unwrap(),
        [Control::SetListen(true), Control::SetListen(false)]
    );
}

#[test]
fn switches_the_output_device_and_keeps_it() {
    let dir = ::tempfile::tempdir().unwrap();
    let sent = Arc::<Mutex<Vec<Control>>>::default();
    let mut service = service_with(sent.clone(), Some(dir.path().to_owned()));
    assert_eq!(
        service.outputs()[1],
        ("hdmi".to_owned(), "hdmi description".to_owned(), 784)
    );
    assert_eq!(service.output(), "");

    service.set_output("hdmi".to_owned()).unwrap();
    assert_eq!(service.output(), "hdmi");
    assert_eq!(
        settings::load_output(dir.path()).unwrap().as_deref(),
        Some("hdmi")
    );
    // a microphone isn't an output device
    assert!(service.set_output("usb-mic".to_owned()).is_err());
    assert_eq!(service.output(), "hdmi");

    service.set_output(String::new()).unwrap();
    assert_eq!(settings::load_output(dir.path()).unwrap(), None);
    // the input is kept apart
    assert_eq!(settings::load_input(dir.path()).unwrap(), None);
    assert_eq!(
        *sent.lock().unwrap(),
        [
            Control::SetOutput("hdmi".to_owned()),
            Control::SetOutput(String::new())
        ]
    );
}

#[test]
fn reports_how_the_echo_cancellation_does() {
    use ::voice_filter::FRAME_LEN;
    use ::voice_filter::echo::Echo;
    use ::voice_filter::stage::Stage;

    let mut service = service();
    assert!(service.echo().is_empty());
    let (mut echo, mut reference) = Echo::new();
    service.echo = Some(echo.status());
    // two frames of the microphone, one of a reference at -20 dBFS (RMS)
    reference.push(&[0.1; FRAME_LEN]);
    echo.process(&mut [0.; FRAME_LEN]).unwrap();
    echo.process(&mut [0.; FRAME_LEN]).unwrap();
    let status = service.echo();
    assert_eq!(status["capture-frames"], 2.);
    assert_eq!(status["reference-frames"], 1.);
    assert!(
        (status["reference"] - -20.).abs() < 0.01,
        "{}",
        status["reference"]
    );
    for key in ["erle", "erl", "delay"] {
        assert!(status.contains_key(key), "{key}");
    }
}

#[test]
fn keeps_the_stages_and_parameters() {
    let dir = ::tempfile::tempdir().unwrap();
    let mut service = service_with(Arc::default(), Some(dir.path().to_owned()));
    service.set_parameter("denoise", "limit", 24.).unwrap();
    service.set_stage_enabled("agc", false).unwrap();
    // refused values aren't kept either
    assert!(service.set_parameter("agc", "target", -40.).is_err());

    let restored = service_with(Arc::default(), None);
    let warnings =
        settings::restore_chain(dir.path(), &restored.switches, &restored.params).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(restored.params[0].1.get(), 24.);
    assert_eq!(restored.params[1].1.get(), -18.);
    assert_eq!(
        restored.stages(),
        [("denoise".to_owned(), true), ("agc".to_owned(), false)]
    );

    // back on and at the default, there's nothing left to keep
    service.set_parameter("denoise", "limit", 100.).unwrap();
    service.set_stage_enabled("agc", true).unwrap();
    assert_eq!(
        ::std::fs::read_to_string(dir.path().join("chain")).unwrap(),
        "\n"
    );
}

#[test]
fn reports_the_levels() {
    let service = service();
    assert_eq!(service.levels()["input"], 0.);
    service.stats.input_db.store(-40.);
    service.stats.output_db.store(-18.);
    service.stats.input_floor_db.store(-62.5);
    let levels = service.levels();
    assert_eq!(levels["input"], -40.);
    assert_eq!(levels["output"], -18.);
    assert_eq!(levels["input-floor"], -62.5);
}

#[test]
fn keeps_listen_only_when_asked_to() {
    let dir = ::tempfile::tempdir().unwrap();
    let sent = Arc::<Mutex<Vec<Control>>>::default();
    let mut service = service_with(sent.clone(), Some(dir.path().to_owned()));
    service.set_listen(true).unwrap();
    assert_eq!(settings::load_listen(dir.path()).unwrap(), None);

    // keeping it saves the current state, and every change after
    service.set_keep_listen(true).unwrap();
    assert!(service.keep_listen());
    assert_eq!(settings::load_listen(dir.path()).unwrap(), Some(true));
    service.set_listen(false).unwrap();
    assert_eq!(settings::load_listen(dir.path()).unwrap(), Some(false));

    // not keeping it forgets it
    service.set_listen(true).unwrap();
    service.set_keep_listen(false).unwrap();
    assert_eq!(settings::load_listen(dir.path()).unwrap(), None);
    assert!(service.listen());
}

#[test]
fn sets_and_keeps_the_volume_of_listen() {
    let dir = ::tempfile::tempdir().unwrap();
    let mut service = service_with(Arc::default(), Some(dir.path().to_owned()));
    assert_eq!(service.listen_volume(), 0.);
    service.set_listen_volume(-20.).unwrap();
    assert_eq!(service.listen_volume(), -20.);
    assert_eq!(
        settings::load_listen_volume(dir.path()).unwrap(),
        Some(-20.)
    );
    // out of range: refused, nothing changes
    assert!(service.set_listen_volume(-50.).is_err());
    assert!(service.set_listen_volume(3.).is_err());
    assert_eq!(service.listen_volume(), -20.);
    assert_eq!(
        settings::load_listen_volume(dir.path()).unwrap(),
        Some(-20.)
    );
}
