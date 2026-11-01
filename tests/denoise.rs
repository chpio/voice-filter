mod common;

use ::voice_filter::FRAME_LEN;
use ::voice_filter::denoise::{Denoise, MAX_INPUT_GAIN_DB};
use ::voice_filter::echo::{Echo, Reference};
use ::voice_filter::stage::{Frame, Stage};
use common::Noise;

/// How much `denoise` turns 1 s of the noise down, in dB, compared over the last half.
fn ratio_db(denoise: &mut Denoise, noise: &mut Noise) -> f32 {
    let (mut energy_in, mut energy_out) = (0., 0.);
    for i in 0..100 {
        let mut frame = noise.frame();
        let input = frame;
        denoise.process(&mut frame).unwrap();
        if i >= 50 {
            energy_in += input.iter().map(|x| x * x).sum::<f32>();
            energy_out += frame.iter().map(|x| x * x).sum::<f32>();
        }
    }
    10. * (energy_out / energy_in).log10()
}

#[test]
fn reports_the_model_delay_and_suppresses_noise() {
    let mut denoise = Denoise::new().unwrap();
    assert_eq!(denoise.latency(), 1440);
    let ratio = ratio_db(&mut denoise, &mut Noise(0x2545_f491));
    assert!(ratio < -20., "{ratio} dB");
}

#[test]
fn changes_the_attenuation_limit_at_runtime() {
    // the output level follows the limit: about -6 dB, then fully suppressed
    let mut denoise = Denoise::new().unwrap().with_atten_lim(6.);
    let atten_lim = denoise.atten_lim();
    let mut noise = Noise(0x2545_f491);
    let limited = ratio_db(&mut denoise, &mut noise);
    assert!((-7. ..-5.).contains(&limited), "{limited} dB");
    atten_lim.set(::voice_filter::denoise::UNLIMITED).unwrap();
    let unlimited = ratio_db(&mut denoise, &mut noise);
    assert!(unlimited < -20., "{unlimited} dB");
}

#[test]
fn has_no_estimate_at_a_limit_of_0_db() {
    // the model doesn't run then, the stages after it go by the level as if it were off
    let mut denoise = Denoise::new().unwrap().with_atten_lim(0.);
    let speech = denoise.speech();
    let ratio = ratio_db(&mut denoise, &mut Noise(0x2545_f491));
    assert!(ratio.abs() < 0.1, "{ratio} dB");
    assert_eq!(speech.frames(), 0);
    denoise.atten_lim().set(6.).unwrap();
    ratio_db(&mut denoise, &mut Noise(0x2545_f491));
    assert_eq!(speech.frames(), 100);
}

#[test]
fn plays_nothing_from_before_a_pause() {
    use ::std::sync::atomic::Ordering;
    use ::voice_filter::chain::Chain;

    // white noise it lets through, limited to 6 dB, then off and silence
    let mut chain = Chain::new(vec![Box::new(Denoise::new().unwrap().with_atten_lim(6.))]);
    let enabled = chain.switches()[0].1.clone();
    let mut noise = Noise(0x2545_f491);
    for _ in 0..100 {
        chain.process(&mut noise.frame()).unwrap();
    }
    enabled.store(false, Ordering::Relaxed);
    for _ in 0..10 {
        chain.process(&mut [0.; FRAME_LEN]).unwrap();
    }
    // on again with silence: the noise it still holds within its latency must not come out (the
    // frames the deep filter reaches back on top only show with speech, see resume_frames)
    enabled.store(true, Ordering::Relaxed);
    for i in 0..20 {
        let mut frame = [0.; FRAME_LEN];
        chain.process(&mut frame).unwrap();
        let peak = frame.iter().fold(0f32, |peak, x| peak.max(x.abs()));
        assert!(peak < 1e-4, "frame {i}: peak {peak}");
    }
}

/// Something like a voice, sample `n`: harmonics of 150 Hz shaped by two formants (700 and
/// 1200 Hz), in syllables of 200 ms with 50 ms between them; at a third of its level the model
/// didn't take it for speech.
fn vowel(n: usize) -> f32 {
    let t = n as f32 / ::voice_filter::SAMPLE_RATE as f32;
    let in_syllable = (t % 0.25) / 0.2;
    if in_syllable >= 1. {
        return 0.;
    }
    let envelope = (::std::f32::consts::PI * in_syllable).sin();
    let formant = |f: f32, center: f32| (-((f - center) / 200.).powi(2)).exp();
    let harmonics: f32 = (1..=20)
        .map(|k| {
            let f = 150. * k as f32;
            (formant(f, 700.) + 0.6 * formant(f, 1200.) + 0.05)
                * (::std::f32::consts::TAU * f * t).sin()
        })
        .sum();
    0.06 * envelope * harmonics
}

/// What `denoise` makes of `frames` frames of a microphone picking up a voice ([`vowel`], with
/// `voice`; quiet noise otherwise) and, with `played`, quiet noise the speakers play (about
/// −65 dBFS, above the silence of the reference), after `echo` (whose sidechain `denoise` may
/// take). Louder, AEC3 turned the synthetic voice down by 9 dB, even without its echo, and the
/// model didn't take it for speech anymore.
fn after_echo(
    echo: &mut Echo,
    reference: &mut Reference,
    denoise: &mut Denoise,
    frames: usize,
    (voice, played): (bool, bool),
) -> Vec<f32> {
    let (mut own, mut speakers) = (Noise(0x2545_f491), Noise(0x1234_5678));
    let mut out = Vec::new();
    for i in 0..frames {
        let mut frame = own.frame();
        for (n, x) in frame.iter_mut().enumerate() {
            *x *= 0.03;
            if voice {
                *x += vowel(i * FRAME_LEN + n);
            }
        }
        if played {
            let playing = speakers.frame().map(|x| 0.01 * x);
            reference.push(&playing);
            for (x, p) in frame.iter_mut().zip(&playing) {
                *x += p;
            }
        }
        echo.process(&mut frame).unwrap();
        denoise.process(&mut frame).unwrap();
        out.extend_from_slice(&frame);
    }
    out
}

/// The output of `after_echo` with Voice Protection at `protect` (%), or without the sidechain.
fn protected(protect: Option<f32>, signals: (bool, bool)) -> Vec<f32> {
    let (echo, mut reference) = Echo::new();
    let mut echo = echo;
    let mut denoise = Denoise::new().unwrap();
    if let Some(protect) = protect {
        let params = echo.params();
        let param = params.iter().find(|p| p.name == "protect").unwrap();
        param.set(protect).unwrap();
        denoise = denoise.with_sidechain(echo.sidechain());
    }
    // AEC3 suppresses strongly in its first 2.5 s, before it knows the echo path
    after_echo(&mut echo, &mut reference, &mut denoise, 500, signals)
}

#[test]
fn takes_the_sidechain_only_while_the_speakers_play_and_you_talk() {
    let plain = protected(None, (true, true));
    // with the speakers and the voice it takes it, at 0 % it's as without
    assert_ne!(protected(Some(100.), (true, true)), plain);
    assert_eq!(protected(Some(0.), (true, true)), plain);
    // silent speakers, or no voice: exactly as without the sidechain
    assert_eq!(
        protected(Some(100.), (true, false)),
        protected(None, (true, false))
    );
    assert_eq!(
        protected(Some(100.), (false, true)),
        protected(None, (false, true))
    );
}

/// `vowel` 20 dB down (about −46 dBFS) over noise at about −95 dBFS, in frames, and a noise
/// suppression that hears the voice at −26 dBFS, the level of `vowel`, so it raises it by its
/// maximum of 20 dB.
fn quiet_vowel(frames: usize) -> (Vec<Frame>, Denoise) {
    let mut noise = Noise(0x2545_f491);
    let input = (0..frames)
        .map(|i| {
            let mut frame = noise.frame();
            for (n, x) in frame.iter_mut().enumerate() {
                *x = 0.0003 * *x + 0.1 * vowel(i * FRAME_LEN + n);
            }
            frame
        })
        .collect();
    let denoise = Denoise::new().unwrap();
    param(&denoise, "level").set(-26.).unwrap();
    (input, denoise)
}

fn param(denoise: &Denoise, name: &str) -> ::voice_filter::param::Param {
    denoise
        .params()
        .into_iter()
        .find(|param| param.name == name)
        .unwrap()
}

#[test]
fn takes_its_input_gain_off_the_output() {
    // at a limit of 0 dB the model doesn't run, its output is the raised input, delayed
    let (input, denoise) = quiet_vowel(300);
    let mut denoise = denoise.with_atten_lim(0.);
    let mut output = Vec::new();
    for frame in &input {
        let mut frame = *frame;
        denoise.process(&mut frame).unwrap();
        output.extend_from_slice(&frame);
    }
    let gain = denoise.input_gain_db();
    assert!(gain > MAX_INPUT_GAIN_DB - 1., "{gain} dB");
    let input: Vec<f32> = input.concat();
    let delay = denoise.latency();
    for (n, (x, y)) in input.iter().zip(&output[delay..]).enumerate() {
        assert!((x - y).abs() < 1e-6, "sample {n}: {x} in, {y} out");
    }
}

#[test]
fn raises_only_a_quiet_voice() {
    let (quiet, mut denoise) = quiet_vowel(300);
    for mut frame in quiet {
        denoise.process(&mut frame).unwrap();
    }
    let gain = denoise.input_gain_db();
    assert!(gain > MAX_INPUT_GAIN_DB - 1., "{gain} dB");
    // a loud voice, at its level, and steady noise: not at all
    let mut loud = Denoise::new().unwrap();
    for i in 0..300 {
        let mut frame: Frame = ::std::array::from_fn(|n| vowel(i * FRAME_LEN + n));
        loud.process(&mut frame).unwrap();
    }
    assert_eq!(loud.input_gain_db(), 0.);
    let mut noisy = Denoise::new().unwrap();
    let mut noise = Noise(0x2545_f491);
    for _ in 0..300 {
        noisy.process(&mut noise.frame().map(|x| 0.1 * x)).unwrap();
    }
    assert_eq!(noisy.input_gain_db(), 0.);
}
