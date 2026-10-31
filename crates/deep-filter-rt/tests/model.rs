//! Runs the embedded DeepFilterNet3 model on a synthetic signal.

use ::deep_filter_rt::tract::DfTract;
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
