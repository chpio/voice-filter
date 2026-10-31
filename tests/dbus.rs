//! The logic of the control interface, called directly, without a bus.

use ::std::sync::Arc;
use ::std::sync::atomic::{AtomicBool, Ordering};

use ::voice_filter::atomic::AtomicF32;
use ::voice_filter::bridge::Stats;
use ::voice_filter::dbus::Service;

fn service() -> Service {
    Service {
        switches: vec![
            ("denoise", Arc::new(AtomicBool::new(true))),
            ("agc", Arc::new(AtomicBool::new(true))),
        ],
        atten_lim: Arc::new(AtomicF32::new(100.)),
        stats: Arc::new(Stats::default()),
        chain_latency: 1488,
        input: String::new(),
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
