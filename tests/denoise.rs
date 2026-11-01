use ::voice_filter::FRAME_LEN;
use ::voice_filter::denoise::Denoise;
use ::voice_filter::stage::{Frame, Stage};

/// White noise at about −25 dBFS, from a fixed seed in `state` (xorshift).
fn noise(state: &mut u32) -> Frame {
    ::std::array::from_fn(|_| {
        *state ^= *state << 13;
        *state ^= *state >> 17;
        *state ^= *state << 5;
        (*state as f32 / u32::MAX as f32 * 2. - 1.) * 0.1
    })
}

/// How much `denoise` turns 1 s of the noise down, in dB, compared over the last half.
fn ratio_db(denoise: &mut Denoise, state: &mut u32) -> f32 {
    let (mut energy_in, mut energy_out) = (0., 0.);
    for i in 0..100 {
        let mut frame = noise(state);
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
    let ratio = ratio_db(&mut denoise, &mut 0x2545_f491);
    assert!(ratio < -20., "{ratio} dB");
}

#[test]
fn changes_the_attenuation_limit_at_runtime() {
    // the output level follows the limit: about -6 dB, then fully suppressed
    let mut denoise = Denoise::new().unwrap().with_atten_lim(6.);
    let atten_lim = denoise.atten_lim();
    let mut state = 0x2545_f491;
    let limited = ratio_db(&mut denoise, &mut state);
    assert!((-7. ..-5.).contains(&limited), "{limited} dB");
    atten_lim.set(::voice_filter::denoise::UNLIMITED).unwrap();
    let unlimited = ratio_db(&mut denoise, &mut state);
    assert!(unlimited < -20., "{unlimited} dB");
}

#[test]
fn plays_nothing_from_before_a_pause() {
    use ::std::sync::atomic::Ordering;
    use ::voice_filter::chain::Chain;

    // white noise it lets through, limited to 6 dB, then off and silence
    let mut chain = Chain::new(vec![Box::new(Denoise::new().unwrap().with_atten_lim(6.))]);
    let enabled = chain.switches()[0].1.clone();
    let mut state = 0x2545_f491;
    for _ in 0..100 {
        chain.process(&mut noise(&mut state)).unwrap();
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
