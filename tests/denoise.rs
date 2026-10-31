use ::voice_filter::FRAME_LEN;
use ::voice_filter::denoise::Denoise;
use ::voice_filter::stage::Stage;

#[test]
fn reports_the_model_delay_and_suppresses_noise() {
    let mut denoise = Denoise::new().unwrap();
    assert_eq!(denoise.latency(), 1440);

    // 1 s of white noise from a fixed seed, the last half is compared
    let mut state = 0x2545_f491u32;
    let mut energy_in = 0.;
    let mut energy_out = 0.;
    for i in 0..100 {
        let mut frame = [0.; FRAME_LEN];
        for sample in &mut frame {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *sample = (state as f32 / u32::MAX as f32 * 2. - 1.) * 0.1;
        }
        let input = frame;
        denoise.process(&mut frame).unwrap();
        if i >= 50 {
            energy_in += input.iter().map(|x| x * x).sum::<f32>();
            energy_out += frame.iter().map(|x| x * x).sum::<f32>();
        }
    }
    assert!(energy_out < energy_in / 100., "{energy_in} -> {energy_out}");
}

#[test]
fn changes_the_attenuation_limit_at_runtime() {
    // white noise, the output level follows the limit: about -6 dB, then fully suppressed
    let mut denoise = Denoise::new().unwrap().with_atten_lim(6.);
    let atten_lim = denoise.atten_lim();
    let mut state = 0x2545_f491u32;
    let mut ratio_db = |denoise: &mut Denoise| {
        let (mut energy_in, mut energy_out) = (0., 0.);
        for i in 0..100 {
            let mut frame = [0.; FRAME_LEN];
            for sample in &mut frame {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                *sample = (state as f32 / u32::MAX as f32 * 2. - 1.) * 0.1;
            }
            let input = frame;
            denoise.process(&mut frame).unwrap();
            if i >= 50 {
                energy_in += input.iter().map(|x| x * x).sum::<f32>();
                energy_out += frame.iter().map(|x| x * x).sum::<f32>();
            }
        }
        10. * (energy_out / energy_in).log10()
    };
    let limited = ratio_db(&mut denoise);
    assert!((-7. ..-5.).contains(&limited), "{limited} dB");
    atten_lim.store(::voice_filter::denoise::UNLIMITED);
    let unlimited = ratio_db(&mut denoise);
    assert!(unlimited < -20., "{unlimited} dB");
}
