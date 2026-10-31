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
