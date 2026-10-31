use ::std::f32::consts::PI;
use ::std::sync::Arc;

use ::voice_filter::denoise::Speech;
use ::voice_filter::expander::{Expander, HOLD_MS, RANGE_DB, RELEASE_S};
use ::voice_filter::stage::{Frame, Stage};
use ::voice_filter::{FRAME_LEN, SAMPLE_RATE};

/// A frame of a 300 Hz tone at `level_db` dBFS (RMS).
fn tone(index: usize, level_db: f32) -> Frame {
    let amplitude = 10f32.powf(level_db / 20.) * 2f32.sqrt();
    ::std::array::from_fn(|j| {
        let t = (index * FRAME_LEN + j) as f32 / SAMPLE_RATE as f32;
        amplitude * (2. * PI * 300. * t).sin()
    })
}

const HOLD_FRAMES: usize = (HOLD_MS / 10.) as usize;
const RELEASE_FRAMES: usize = (RELEASE_S * 100.) as usize;

/// An expander with what the noise suppression hears, as the chain wires it.
fn listening() -> (Expander, Arc<Speech>) {
    let speech = Arc::new(Speech::new());
    (Expander::new().listening(speech.clone()), speech)
}

#[test]
fn leaves_speech_alone() {
    let (mut expander, speech) = listening();
    for i in 0..50 {
        speech.update(15.);
        let input = tone(i, -30.);
        let mut output = input;
        expander.process(&mut output).unwrap();
        assert_eq!(output, input, "frame {i}");
    }
}

#[test]
fn turns_the_pauses_down_after_holding_open() {
    let (mut expander, speech) = listening();
    let run = |expander: &mut Expander, snr_db: f32| {
        speech.update(snr_db);
        expander.process(&mut tone(0, -60.)).unwrap();
        expander.gain_db()
    };
    run(&mut expander, 15.);
    // a pause: open for the hold time, then down over the release to the range
    for frame in 0..HOLD_FRAMES - 1 {
        assert_eq!(run(&mut expander, -12.), 0., "frame {frame} of the hold");
    }
    let mut gain = 0.;
    for _ in 0..RELEASE_FRAMES + 2 {
        gain = run(&mut expander, -12.);
    }
    assert_eq!(gain, -RANGE_DB);
    // speech again: open within its first frame
    assert_eq!(run(&mut expander, 15.), 0.);
}

#[test]
fn goes_by_the_level_without_the_noise_suppression() {
    // nobody updates what the noise suppression hears, as while it's switched off
    let (mut expander, _speech) = listening();
    expander.process(&mut tone(0, -30.)).unwrap();
    assert_eq!(expander.gain_db(), 0.);
    for i in 0..HOLD_FRAMES + RELEASE_FRAMES + 2 {
        expander.process(&mut tone(i, -65.)).unwrap();
    }
    assert_eq!(expander.gain_db(), -RANGE_DB);
    expander.process(&mut tone(0, -30.)).unwrap();
    assert_eq!(expander.gain_db(), 0.);
}

#[test]
fn follows_its_parameters() {
    let (mut expander, speech) = listening();
    let params = expander.params();
    let names: Vec<_> = params.iter().map(|param| param.name).collect();
    assert_eq!(names, ["range", "threshold", "hold", "level"]);
    // a range of 0 never changes anything
    params[0].set(0.).unwrap();
    for i in 0..100 {
        speech.update(-15.);
        let input = tone(i, -60.);
        let mut output = input;
        expander.process(&mut output).unwrap();
        assert_eq!(output, input, "frame {i}");
    }
    // a higher threshold takes 10 dB of SNR for a pause, a shorter hold closes sooner
    params[0].set(30.).unwrap();
    params[1].set(12.).unwrap();
    params[2].set(50.).unwrap();
    for _ in 0..5 + RELEASE_FRAMES * 3 / 2 + 2 {
        speech.update(10.);
        expander.process(&mut tone(0, -60.)).unwrap();
    }
    assert_eq!(expander.gain_db(), -30.);
}

#[test]
fn opens_for_a_level_above_its_threshold_whatever_the_noise_suppression_hears() {
    // quiet speech the noise suppression doesn't hear as such stays open while it's above -50
    let (mut expander, speech) = listening();
    for i in 0..HOLD_FRAMES + RELEASE_FRAMES + 2 {
        speech.update(-12.);
        let input = tone(i, -40.);
        let mut output = input;
        expander.process(&mut output).unwrap();
        assert_eq!(output, input, "frame {i}");
    }
}
