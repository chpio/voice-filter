mod common;

use ::voice_filter::equalizer::Equalizer;
use ::voice_filter::stage::Stage;

use common::gain_db;

#[test]
fn lowers_the_low_mids_and_raises_the_presence() {
    let mut eq = Equalizer::new();
    assert_eq!(eq.latency(), 0);
    for (freq, expected, tolerance) in [
        (250., -3., 0.2),
        (3500., 3., 0.2),
        // far from both bands
        (40., 0., 0.3),
        (1000., 0., 0.6),
        (16000., 0., 0.3),
    ] {
        let gain = gain_db(&mut eq, freq);
        assert!(
            (gain - expected).abs() < tolerance,
            "{freq} Hz: {gain:.2} dB"
        );
    }
}

#[test]
fn follows_its_gains_and_the_presence_frequency() {
    let mut eq = Equalizer::new();
    let params = eq.params();
    let names: Vec<_> = params.iter().map(|param| param.name).collect();
    assert_eq!(names, ["low-mid", "presence", "presence-freq"]);
    params[0].set(0.).unwrap();
    params[1].set(6.).unwrap();
    params[2].set(5000.).unwrap();
    for (freq, expected, tolerance) in [(250., 0., 0.3), (5000., 6., 0.2), (3500., 4., 1.)] {
        let gain = gain_db(&mut eq, freq);
        assert!(
            (gain - expected).abs() < tolerance,
            "{freq} Hz: {gain:.2} dB"
        );
    }
}
