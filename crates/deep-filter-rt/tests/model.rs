//! Runs the embedded DeepFilterNet3 model on a synthetic signal.

use ::deep_filter_rt::tract::DfTract;
use ::deep_filter_rt::{Complex32, band_unit_norm, band_unit_norm_t};
use ::ndarray::Array2;

/// White noise in `[-amplitude, amplitude)` from a fixed seed, so every run gets the same input.
fn white_noise(len: usize, amplitude: f32) -> Vec<f32> {
    let mut state = 0x2545_f491_4f6c_dd1du64;
    (0..len)
        .map(|_| {
            // xorshift64
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            ((state >> 40) as f32 / (1u64 << 24) as f32 * 2. - 1.) * amplitude
        })
        .collect()
}

fn rms(xs: &[f32]) -> f32 {
    (xs.iter().map(|x| x * x).sum::<f32>() / xs.len() as f32).sqrt()
}

#[test]
fn suppresses_white_noise() {
    let mut df = DfTract::default();
    assert_eq!((df.sr, df.hop_size, df.ch), (48000, 480, 1));

    // 2 s, the first 0.5 s are left out of the comparison to give the model time to settle
    let frames = 200;
    let settle = 50;
    let input = white_noise(frames * df.hop_size, 0.1);
    let mut output = Vec::<f32>::with_capacity(input.len());
    let mut enh = Array2::zeros((1, df.hop_size));
    for frame in input.chunks_exact(df.hop_size) {
        let noisy = Array2::from_shape_vec((1, df.hop_size), frame.to_vec()).unwrap();
        df.process(noisy.view(), enh.view_mut()).unwrap();
        output.extend(enh.iter());
    }

    assert!(output.iter().all(|x| x.is_finite()));
    let start = settle * df.hop_size;
    let (rms_in, rms_out) = (rms(&input[start..]), rms(&output[start..]));
    assert!(
        rms_out < rms_in / 10.,
        "noise only reduced from rms {rms_in} to {rms_out}"
    );
}

/// 1 s of white noise, frame by frame.
fn frames() -> Vec<Array2<f32>> {
    white_noise(100 * 480, 0.1)
        .as_chunks::<480>()
        .0
        .iter()
        .map(|frame| Array2::from_shape_vec((1, 480), frame.to_vec()).unwrap())
        .collect()
}

#[test]
fn analyzes_with_the_estimate_of_process() {
    let (mut full, mut encoder) = (DfTract::default(), DfTract::default());
    let mut enh = Array2::zeros((1, 480));
    for noisy in frames() {
        let lsnr = full.process(noisy.view(), enh.view_mut()).unwrap();
        assert_eq!(encoder.analyze(noisy.view()).unwrap(), lsnr);
    }
}

#[test]
fn processes_the_same_with_the_analyzed_signal_as_the_one_to_apply_to() {
    let (mut plain, mut with) = (DfTract::default(), DfTract::default());
    let (mut a, mut b) = (Array2::zeros((1, 480)), Array2::zeros((1, 480)));
    for noisy in frames() {
        plain.process(noisy.view(), a.view_mut()).unwrap();
        with.process_with(noisy.view(), Some(noisy.view()), true, b.view_mut())
            .unwrap();
        assert_eq!(a, b);
    }
}

#[test]
fn applies_the_gains_to_the_other_signal() {
    // the gains and the deep filter are linear in what they're applied to: twice the signal gives
    // twice the output; limited, so it lets part of the noise through
    for df in [true, false] {
        let (mut once, mut twice) = (DfTract::default(), DfTract::default());
        once.set_atten_lim(6.);
        twice.set_atten_lim(6.);
        let (mut a, mut b) = (Array2::zeros((1, 480)), Array2::zeros((1, 480)));
        let mut energy = 0.;
        for noisy in frames() {
            let double = &noisy * 2.;
            once.process_with(noisy.view(), Some(noisy.view()), df, a.view_mut())
                .unwrap();
            twice
                .process_with(noisy.view(), Some(double.view()), df, b.view_mut())
                .unwrap();
            for (x, y) in a.iter().zip(&b) {
                assert!((2. * x - y).abs() < 1e-5, "{x} {y}");
            }
            energy += a.iter().map(|x| x * x).sum::<f32>();
        }
        assert!(energy > 1., "{energy}");
    }
}

#[test]
fn skips_the_model_in_digital_silence() {
    // after 5 frames of silence it gives the skip estimate of -15 dB and zeros, also while the
    // model would apply its zero gains (upstream issue #713: they reset the counter, so it never
    // skipped)
    let mut df = DfTract::default();
    let mut enh = Array2::ones((1, 480));
    let silence = Array2::zeros((1, 480));
    let lsnr: Vec<f32> = (0..10)
        .map(|_| df.process(silence.view(), enh.view_mut()).unwrap())
        .collect();
    assert!(lsnr[..5].iter().all(|&x| x != -15.), "{lsnr:?}");
    assert!(lsnr[6..].iter().all(|&x| x == -15.), "{lsnr:?}");
    assert!(enh.iter().all(|&x| x == 0.));
}

#[test]
fn unit_norm_gives_zeros_for_a_state_of_zero() {
    // the state of a bin decays to 0 in long silence where denormals are flushed to zero, which
    // gave 0 / 0 (upstream issue #713)
    let mut xs = [Complex32::new(0., 0.); 4];
    let mut state = [0.; 4];
    band_unit_norm(&mut xs, &mut state, 0.99);
    assert_eq!(xs, [Complex32::new(0., 0.); 4]);
    let mut out = [1.; 8];
    band_unit_norm_t(&xs, &mut state, 0.99, &mut out);
    assert_eq!(out, [0.; 8]);
}

#[test]
fn passes_quiet_input_at_a_limit_of_0_db() {
    // below the level of the silence skip (-70 dBFS), but the model doesn't run at all: the output
    // is the input, delayed
    let mut df = DfTract::default();
    df.set_atten_lim(0.);
    let delay = df.delay();
    let input: Vec<f32> = (0..30 * 480)
        .map(|n| 1e-4 * (n as f32 * 0.05).sin())
        .collect();
    let mut output = Vec::<f32>::new();
    let mut enh = Array2::<f32>::zeros((1, 480));
    for frame in input.as_chunks::<480>().0 {
        let noisy = Array2::from_shape_vec((1, 480), frame.to_vec()).unwrap();
        df.process(noisy.view(), enh.view_mut()).unwrap();
        output.extend(enh.iter());
    }
    for (x, y) in input.iter().zip(&output[delay..]).skip(960) {
        assert!((x - y).abs() < 1e-7, "{x} against {y}");
    }
}
