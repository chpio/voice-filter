use ::voice_filter::FRAME_LEN;
use ::voice_filter::agc::db_to_gain;
use ::voice_filter::limiter::{CEILING_DB, LOOKAHEAD, Limiter};
use ::voice_filter::stage::{Frame, Stage};

fn run(limiter: &mut Limiter, input: &[f32]) -> Vec<f32> {
    let mut output = Vec::with_capacity(input.len());
    for chunk in input.chunks(FRAME_LEN) {
        let mut frame: Frame = [0.; FRAME_LEN];
        frame[..chunk.len()].copy_from_slice(chunk);
        limiter.process(&mut frame).unwrap();
        output.extend_from_slice(&frame);
    }
    output
}

/// A signal with sharp peaks up to twice full scale, also at frame boundaries.
fn peaks() -> Vec<f32> {
    let mut signal: Vec<f32> = (0..20 * FRAME_LEN)
        .map(|i| 0.3 * (i as f32 * 0.05).sin())
        .collect();
    for at in [100, FRAME_LEN - 1, FRAME_LEN, 3 * FRAME_LEN - 2, 7777] {
        signal[at] = 2.;
        signal[at + 1] = -1.5;
    }
    signal
}

#[test]
fn keeps_peaks_below_the_ceiling() {
    let output = run(&mut Limiter::new(), &peaks());
    let ceiling = db_to_gain(CEILING_DB);
    assert!(output.iter().all(|x| x.abs() <= ceiling));
}

#[test]
fn reduces_peaks_before_they_come_instead_of_clipping() {
    // with the lookahead the gain is already down when the peak is played, so the peak ends up at
    // the ceiling by the gain, not by the clamp, and its neighbors are lowered too
    let mut input = vec![0.5; 10 * FRAME_LEN];
    input[2000] = 2.;
    let output = run(&mut Limiter::new(), &input);
    let peak = output[2000 + LOOKAHEAD];
    let ceiling = db_to_gain(CEILING_DB);
    assert!(peak > 0.95 * ceiling && peak <= ceiling, "peak {peak}");
    assert!(
        output[2000 + LOOKAHEAD - 1] < 0.5,
        "the gain is down before the peak"
    );
}

#[test]
fn passes_quiet_signals_unchanged_but_delayed() {
    let input: Vec<f32> = (0..5 * FRAME_LEN)
        .map(|i| 0.5 * (i as f32 * 0.01).sin())
        .collect();
    let mut limiter = Limiter::new();
    assert_eq!(limiter.latency(), LOOKAHEAD);
    let output = run(&mut limiter, &input);
    assert!(output[..LOOKAHEAD].iter().all(|&x| x == 0.));
    for (out, x) in output[LOOKAHEAD..].iter().zip(&input) {
        assert!((out - x).abs() < 1e-6);
    }
}
