use ::std::f32::consts::PI;

use ::voice_filter::compressor::{
    Compressor, KNEE_DB, RATIO, THRESHOLD_DB, makeup_db, reduction_db,
};
use ::voice_filter::stage::Stage;
use ::voice_filter::{FRAME_LEN, SAMPLE_RATE};

/// Runs `frames` of a 1 kHz sine with an RMS of `level_db` (dBFS) through `compressor`, returns
/// the RMS of the output over the last frame in dBFS.
fn run(compressor: &mut Compressor, level_db: f32, frames: usize) -> f32 {
    let amplitude = 10f32.powf(level_db / 20.) * 2f32.sqrt();
    let mut out_db = 0.;
    for i in 0..frames {
        let mut frame = [0.; FRAME_LEN];
        for (j, sample) in frame.iter_mut().enumerate() {
            let t = (i * FRAME_LEN + j) as f32 / SAMPLE_RATE as f32;
            *sample = amplitude * (2. * PI * 1000. * t).sin();
        }
        compressor.process(&mut frame).unwrap();
        let power = frame.iter().map(|x| x * x).sum::<f32>() / FRAME_LEN as f32;
        out_db = 10. * power.log10();
    }
    out_db
}

#[test]
fn has_a_soft_knee_and_the_ratio_above_it() {
    assert_eq!(reduction_db(THRESHOLD_DB - 10.), 0.);
    // at the threshold, in the middle of the knee: the slope times an eighth of its width
    let slope = 1. / RATIO - 1.;
    assert!((reduction_db(THRESHOLD_DB) - slope * KNEE_DB / 8.).abs() < 1e-5);
    assert!((reduction_db(THRESHOLD_DB + 12.) - slope * 12.).abs() < 1e-5);
    assert!(makeup_db() > 0.);
}

#[test]
fn compresses_steady_levels_by_the_ratio() {
    // settled: below the knee only the make-up gain, above it 3 dB in give 1 dB out
    let settled = |level_db| run(&mut Compressor::new(), level_db, 100);
    let quiet = settled(-45.);
    assert!((quiet - (-45. + makeup_db())).abs() < 0.2, "{quiet}");
    let (loud, louder) = (settled(-12.), settled(-6.));
    assert!(
        ((louder - loud) - 6. / RATIO).abs() < 0.2,
        "{loud} -> {louder}"
    );
}

#[test]
fn turns_down_fast_and_back_up_slowly() {
    let mut compressor = Compressor::new();
    run(&mut compressor, -40., 50);
    assert!(compressor.reduction() > -0.01);
    // loud: within 3 frames (30 ms) almost all of the reduction
    run(&mut compressor, -6., 3);
    let wanted = reduction_db(-6.);
    assert!(
        compressor.reduction() < wanted * 0.95,
        "{}",
        compressor.reduction()
    );
    run(&mut compressor, -6., 50);
    // quiet again: after 30 ms still most of it, the release is 120 ms
    run(&mut compressor, -40., 3);
    assert!(
        compressor.reduction() < wanted * 0.6,
        "{}",
        compressor.reduction()
    );
    run(&mut compressor, -40., 100);
    assert!(compressor.reduction() > -0.1, "{}", compressor.reduction());
}
