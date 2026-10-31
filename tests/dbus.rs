//! The logic of the control interface, called directly, without a bus.

use ::std::sync::atomic::{AtomicBool, Ordering};
use ::std::sync::{Arc, Mutex};

use ::voice_filter::atomic::AtomicF32;
use ::voice_filter::bridge::Stats;
use ::voice_filter::dbus::{Control, Service};
use ::voice_filter::live::Input;
use ::voice_filter::settings;

fn service() -> Service {
    service_with(Arc::default(), None)
}

/// A service that records what it sends in `sent`.
fn service_with(
    sent: Arc<Mutex<Vec<Control>>>,
    settings_dir: Option<::std::path::PathBuf>,
) -> Service {
    let input = |id, name: &str, priority| Input {
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
        atten_lim: Arc::new(AtomicF32::new(100.)),
        stats: Arc::new(Stats::default()),
        chain_latency: 1488,
        input: String::new(),
        inputs: Arc::new(Mutex::new(vec![
            input(40, "usb-mic", 2100),
            input(41, "webcam", 2009),
        ])),
        send: Box::new(move |control| {
            sent.lock().unwrap().push(control);
            Ok(())
        }),
        listening: false,
        settings_dir,
    }
}

#[test]
fn sets_the_attenuation_limit_within_its_range() {
    let mut service = service();
    service.set_attenuation_limit(12.).unwrap();
    assert_eq!(service.atten_lim.load(), 12.);
    assert_eq!(service.attenuation_limit(), 12.);
    assert!(service.set_attenuation_limit(101.).is_err());
    assert!(service.set_attenuation_limit(-1.).is_err());
    assert!(service.set_attenuation_limit(f64::NAN).is_err());
    assert_eq!(service.atten_lim.load(), 12.);
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
