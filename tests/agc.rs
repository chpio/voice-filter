use ::std::f32::consts::TAU;

use ::voice_filter::agc::{self, Agc};
use ::voice_filter::stage::{Frame, Stage};
use ::voice_filter::{FRAME_LEN, SAMPLE_RATE};

/// Frames of a 440 Hz sine with the given RMS level, `on` frames of it and then `off` frames of
/// silence, repeated. [`syllables`] for speech: a steady tone is noise to the AGC.
fn bursts(level_db: f32, on: usize, off: usize, frames: usize) -> Vec<Frame> {
    let amplitude = agc::db_to_gain(level_db) * 2f32.sqrt();
    (0..frames)
        .map(|f| {
            let mut frame = [0.; FRAME_LEN];
            if f % (on + off) < on {
                for (i, sample) in frame.iter_mut().enumerate() {
                    let t = (f * FRAME_LEN + i) as f32 / SAMPLE_RATE as f32;
                    *sample = amplitude * (TAU * 440. * t).sin();
                }
            }
            frame
        })
        .collect()
}

/// Speech as the AGC sees it: 300 ms at the level, 100 ms of silence between, for `frames`.
fn syllables(level_db: f32, frames: usize) -> Vec<Frame> {
    bursts(level_db, 30, 10, frames)
}

fn rms_db(frame: &Frame) -> f32 {
    10. * (frame.iter().map(|x| x * x).sum::<f32>() / FRAME_LEN as f32).log10()
}

/// Runs the frames through a new AGC and returns the level of the last frame.
fn settled_level(frames: Vec<Frame>) -> (f32, Agc) {
    let mut agc = Agc::new();
    let mut last = 0.;
    for mut frame in frames {
        agc.process(&mut frame).unwrap();
        if frame.iter().any(|&x| x != 0.) {
            last = rms_db(&frame);
        }
    }
    (last, agc)
}

#[test]
fn brings_quiet_and_loud_speech_to_the_target() {
    // 10 s each, enough for the slow rise
    for level in [-40., -30., -10.] {
        let (out, _) = settled_level(syllables(level, 1000));
        assert!(
            (out - agc::TARGET_DB).abs() < 1.,
            "{level} dBFS ends at {out} dBFS"
        );
    }
}

#[test]
fn limits_the_gain() {
    let (_, agc) = settled_level(syllables(-54., 1000));
    assert_eq!(agc.gain_db(), agc::MAX_GAIN_DB);
}

#[test]
fn keeps_the_gain_in_pauses_and_below_the_gate() {
    // speech at -30 dBFS for 10 s, then 5 s of silence: the gain stays where it was
    let mut agc = Agc::new();
    for mut frame in syllables(-30., 1000) {
        agc.process(&mut frame).unwrap();
    }
    let gain = agc.gain_db();
    for mut frame in bursts(-30., 0, 500, 500) {
        agc.process(&mut frame).unwrap();
    }
    assert_eq!(agc.gain_db(), gain);

    // noise below the gate doesn't count as speech either
    for mut frame in bursts(agc::GATE_DB - 5., 500, 0, 500) {
        agc.process(&mut frame).unwrap();
    }
    assert_eq!(agc.gain_db(), gain);
}

#[test]
fn keeps_the_gain_in_steady_noise_above_the_gate() {
    // hum at -50 dBFS throughout, speech at -30 dBFS over it for 10 s, then 20 s of the hum
    // alone: with only the gate the AGC raised the hum to the speech level within seconds
    let hum = || bursts(-50., 3000, 0, 3000).into_iter();
    let speech = syllables(-30., 1000)
        .into_iter()
        .chain(bursts(0., 0, 2000, 2000));
    let mut agc = Agc::new();
    let mut gain = 0.;
    for (f, (mut frame, speech)) in hum().zip(speech).enumerate() {
        for (sample, speech) in frame.iter_mut().zip(speech) {
            *sample += speech;
        }
        agc.process(&mut frame).unwrap();
        if f == 999 {
            // speech over the hum still counts
            gain = agc.gain_db();
            assert!((gain - 12.).abs() < 1., "{gain} dB");
        }
    }
    assert_eq!(agc.gain_db(), gain);
}

#[test]
fn keeps_the_gain_while_typing() {
    // speech at -30 dBFS, then 30 s of keystrokes: 30 ms at -40 dBFS every 200 ms, as the
    // noise suppression leaves them; counted as speech, they raised the gain to its maximum
    let mut agc = Agc::new();
    for mut frame in syllables(-30., 1000) {
        agc.process(&mut frame).unwrap();
    }
    let gain = agc.gain_db();
    for mut frame in bursts(-40., 3, 17, 3000) {
        agc.process(&mut frame).unwrap();
    }
    assert_eq!(agc.gain_db(), gain);
}

#[test]
fn has_no_latency() {
    assert_eq!(Agc::new().latency(), 0);
}

#[test]
fn follows_its_target_and_maximum_gain() {
    let mut agc = Agc::new();
    let params = agc.params();
    let names: Vec<_> = params.iter().map(|param| param.name).collect();
    assert_eq!(names, ["target", "max-gain"]);
    params[0].set(-24.).unwrap();
    let mut last = 0.;
    for mut frame in syllables(-40., 1000) {
        agc.process(&mut frame).unwrap();
        if frame.iter().any(|&x| x != 0.) {
            last = rms_db(&frame);
        }
    }
    assert!((last - -24.).abs() < 1., "{last} dBFS");
    // a lower maximum: the gain falls to it
    params[1].set(10.).unwrap();
    for mut frame in syllables(-40., 100) {
        agc.process(&mut frame).unwrap();
    }
    assert_eq!(agc.gain_db(), 10.);
}
