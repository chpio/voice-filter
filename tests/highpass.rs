mod common;

use ::voice_filter::highpass::HighPass;
use ::voice_filter::stage::Stage;

use common::gain_db;

#[test]
fn removes_what_is_below_80_hz() {
    let mut highpass = HighPass::new();
    assert_eq!(highpass.latency(), 0);
    // 4th order Butterworth: |H|² = 1 / (1 + (80 Hz / f)⁸)
    for (freq, expected) in [
        (20., -48.2),
        (40., -24.1),
        (80., -3.0),
        (160., -0.1),
        (1000., 0.),
        (10000., 0.),
    ] {
        let gain = gain_db(&mut highpass, freq);
        assert!((gain - expected).abs() < 0.2, "{freq} Hz: {gain:.2} dB");
    }
}

#[test]
fn moves_its_cutoff_with_the_frequency() {
    let mut highpass = HighPass::new();
    let params = highpass.params();
    assert_eq!(params.len(), 1);
    params[0].set(160.).unwrap();
    // an octave lower than at first: what was the cutoff is now an octave below it
    let gain = gain_db(&mut highpass, 80.);
    assert!((gain - -24.1).abs() < 0.3, "{gain:.2} dB");
    let gain = gain_db(&mut highpass, 160.);
    assert!((gain - -3.0).abs() < 0.2, "{gain:.2} dB");
}
