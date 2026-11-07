//! The signal processing around the model: the ERB bands, the STFT and the post filter.

use ::deep_filter_rt::{Complex32, DFState, erb_fb, post_filter};

#[test]
fn the_erb_bands_cover_every_bin() {
    // DeepFilterNet3's layout, and one with so many bands of at least 5 bins that the last one
    // gets shortened
    for (bands, min) in [(32, 2), (97, 5)] {
        let erb = erb_fb(48000, 960, bands, min);
        assert_eq!(erb.len(), bands);
        assert_eq!(erb.iter().sum::<usize>(), 481);
    }
}

/// Runs `input` through the analysis and the synthesis of a state with `hop` samples per frame.
fn round_trip(hop: usize, input: &[f32]) -> Vec<f32> {
    let mut state = DFState::new(48000, 960, hop, 32, 2);
    let mut spec = vec![Complex32::new(0., 0.); 481];
    let mut out = vec![0.; hop];
    let mut output = Vec::new();
    for frame in input.chunks_exact(hop) {
        state.analysis(frame, &mut spec);
        let before = spec.clone();
        state.synthesis_from(&spec, &mut out);
        assert_eq!(spec, before);
        output.extend(&out);
    }
    output
}

#[test]
fn the_stft_reconstructs_its_input() {
    // 50 % overlap as DeepFilterNet3 runs it, and 75 %, delayed by the window minus a hop
    let input: Vec<f32> = (0..9600).map(|n| (n as f32 * 0.037).sin() * 0.5).collect();
    for hop in [480, 240] {
        let output = round_trip(hop, &input);
        let delay = 960 - hop;
        for (n, (x, y)) in input.iter().zip(&output[delay..]).enumerate().skip(960) {
            assert!(
                (x - y).abs() < 1e-4,
                "hop {hop}, sample {n}: {x} against {y}"
            );
        }
    }
}

#[test]
fn the_post_filter_turns_down_what_the_model_turned_down() {
    let noisy = vec![Complex32::new(0.3, -0.4); 8];
    // unchanged where the model left the bin as it was, a bit further down where it halved it
    let mut kept = noisy.clone();
    post_filter(&noisy, &mut kept, 0.02);
    for (k, n) in kept.iter().zip(&noisy) {
        assert!((k - n).norm() < 1e-6, "{k} against {n}");
    }
    let mut halved: Vec<_> = noisy.iter().map(|x| x * 0.5).collect();
    post_filter(&noisy, &mut halved, 0.02);
    for (h, n) in halved.iter().zip(&noisy) {
        let ratio = h.norm() / n.norm();
        assert!((0.45..0.5).contains(&ratio), "{ratio}");
    }
}
