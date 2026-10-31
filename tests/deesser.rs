use ::std::f32::consts::PI;

use ::voice_filter::deesser::{
    DELAY, DeEsser, MAX_REDUCTION_DB, RATIO, THRESHOLD_DB, reduction_db,
};
use ::voice_filter::stage::Stage;
use ::voice_filter::{FRAME_LEN, SAMPLE_RATE};

/// Sample `n` of a sum of sines, (frequency in Hz, amplitude); silence before 0.
fn sample(n: i64, tones: &[(f32, f32)]) -> f32 {
    if n < 0 {
        return 0.;
    }
    let t = n as f32 / SAMPLE_RATE as f32;
    tones
        .iter()
        .map(|(freq, amplitude)| amplitude * (2. * PI * freq * t).sin())
        .sum()
}

/// Frame `index` of the sines, delayed by `delay` samples.
fn frame(index: usize, tones: &[(f32, f32)], delay: usize) -> [f32; FRAME_LEN] {
    ::std::array::from_fn(|j| sample((index * FRAME_LEN + j) as i64 - delay as i64, tones))
}

fn energy(frame: &[f32]) -> f32 {
    frame.iter().map(|x| x * x).sum()
}

const VOWEL: [(f32, f32); 4] = [(200., 0.3), (600., 0.2), (2000., 0.05), (6000., 0.01)];

#[test]
fn turns_the_band_down_by_the_ratio_up_to_its_maximum() {
    assert_eq!(reduction_db(THRESHOLD_DB - 1.), 0.);
    assert_eq!(reduction_db(THRESHOLD_DB + 4.), -4. * (1. - 1. / RATIO));
    assert_eq!(reduction_db(100.), -MAX_REDUCTION_DB);
}

#[test]
fn leaves_a_voiced_sound_unchanged_but_delayed() {
    // a vowel, its energy far below the band; the abrupt start is broadband, what it does to it
    // fades with the release (50 ms) and ends at -0.01 dB, within 50 frames
    let mut deesser = DeEsser::new();
    assert_eq!(deesser.latency(), DELAY);
    for i in 0..50 {
        deesser.process(&mut frame(i, &VOWEL, 0)).unwrap();
    }
    for i in 50..100 {
        let mut output = frame(i, &VOWEL, 0);
        deesser.process(&mut output).unwrap();
        assert_eq!(output, frame(i, &VOWEL, DELAY), "frame {i}");
    }
}

#[test]
fn turns_down_an_s_and_lets_go_of_it_afterwards() {
    let mut deesser = DeEsser::new();
    let mut n = 0;
    let mut run = |deesser: &mut DeEsser, tones: &[(f32, f32)]| {
        let input = frame(n, tones, 0);
        let mut output = input;
        deesser.process(&mut output).unwrap();
        n += 1;
        (input, output)
    };
    for _ in 0..20 {
        run(&mut deesser, &VOWEL);
    }
    // an "s": nearly all of its energy in the band; the first frame holds the change of sound
    let s = [(7500., 0.2)];
    run(&mut deesser, &s);
    let (mut energy_in, mut energy_out) = (0., 0.);
    for _ in 0..10 {
        let (input, output) = run(&mut deesser, &s);
        energy_in += energy(&input);
        energy_out += energy(&output);
    }
    let reduced_db = 10. * (energy_out / energy_in).log10();
    let expected = reduction_db(0.);
    assert!(
        (reduced_db - expected).abs() < 0.5,
        "{reduced_db} dB, expected about {expected} dB"
    );
    // the vowel again: back to unchanged once the release (50 ms) reaches -0.01 dB, from about
    // -5 dB after 330 ms
    for _ in 0..40 {
        run(&mut deesser, &VOWEL);
    }
    assert_eq!(deesser.reduction(), 0.);
}
