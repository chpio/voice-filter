//! Loading the model and its runtime parameters.

use ::std::path::Path;

use ::deep_filter_rt::tract::{DfParams, DfTract, ReduceMask, RuntimeParams};
use ::ndarray::Array2;

fn model_dir() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/models/DeepFilterNet3"
    ))
}

/// The embedded model with its `config.ini` changed.
fn with_config(change: impl FnOnce(&str) -> String) -> anyhow::Result<DfTract> {
    let dir = model_dir();
    let read = |name: &str| ::std::fs::read(dir.join(name)).unwrap();
    let config = change(&String::from_utf8(read("config.ini")).unwrap());
    let params = DfParams::from_bytes(
        config.as_bytes(),
        read("enc.onnx"),
        read("erb_dec.onnx"),
        read("df_dec.onnx"),
    )?;
    DfTract::new(params, &RuntimeParams::default())
}

/// White noise from a fixed seed, `frames` frames of 480 samples per channel.
fn noise(frames: usize, channels: usize, amplitude: f32) -> Vec<Array2<f32>> {
    let mut state = 0x2545_f491_4f6c_dd1du64;
    (0..frames)
        .map(|_| {
            Array2::from_shape_simple_fn((channels, 480), || {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                ((state >> 40) as f32 / (1u64 << 24) as f32 * 2. - 1.) * amplitude
            })
        })
        .collect()
}

/// `noise` with a tone in every channel, so the model applies its gains (it zeros noise alone).
fn tone_in_noise(frames: usize, channels: usize) -> Vec<Array2<f32>> {
    let mut frames = noise(frames, channels, 0.05);
    for (i, frame) in frames.iter_mut().enumerate() {
        for mut channel in frame.outer_iter_mut() {
            for (n, x) in channel.iter_mut().enumerate() {
                *x += 0.3 * ((i * 480 + n) as f32 * 0.06).sin();
            }
        }
    }
    frames
}

/// Runs `df` over `frames`, the output frame by frame.
fn run(df: &mut DfTract, frames: &[Array2<f32>]) -> Vec<Array2<f32>> {
    frames
        .iter()
        .map(|frame| {
            let mut enh = Array2::zeros(frame.raw_dim());
            df.process(frame.view(), enh.view_mut()).unwrap();
            assert!(enh.iter().all(|x| x.is_finite()));
            enh
        })
        .collect()
}

#[test]
fn loads_the_model_from_a_directory() {
    let mut df = DfTract::new(
        DfParams::new(model_dir()).unwrap(),
        &RuntimeParams::default(),
    )
    .unwrap();
    let mut embedded = DfTract::default();
    let frames = noise(20, 1, 0.1);
    assert_eq!(run(&mut df, &frames), run(&mut embedded, &frames));

    let missing = DfParams::new(Path::new("/nonexistent")).err().unwrap();
    assert!(missing.to_string().contains("config.ini"), "{missing}");
}

#[test]
fn reads_the_config() {
    // an alpha given directly instead of the time constant it's computed from
    let df = with_config(|c| c.replacen("norm_tau = 1", "norm_alpha = 0.5", 1)).unwrap();
    assert_eq!(df.alpha, 0.5);
    for (model, error) in [
        ("deepfilternet2", "deprecated"),
        ("other", "Unsupported model type other"),
    ] {
        let config = |c: &str| c.replacen("model = deepfilternet3", &format!("model = {model}"), 1);
        let e = with_config(config).err().unwrap();
        assert!(e.to_string().contains(error), "{e}");
    }
    let e = DfParams::from_bytes(b"[unclosed", vec![], vec![], vec![])
        .err()
        .unwrap();
    assert!(e.to_string().contains("config"), "{e}");
}

#[test]
fn takes_the_attenuation_limit_from_the_runtime_params() {
    let new = |db| {
        DfTract::new(
            DfParams::default(),
            &RuntimeParams::default().with_atten_lim(db),
        )
    };
    assert_eq!(new(100.).unwrap().atten_lim, None);
    let mut df = new(0.).unwrap();
    df.set_atten_lim(100.);
    assert_eq!(df.atten_lim, None);
    assert_eq!(new(0.).unwrap().atten_lim, Some(1.));
    assert!(!new(0.).unwrap().attenuates());
    let limited = new(-20.).unwrap().atten_lim.unwrap();
    assert!((limited - 0.1).abs() < 1e-6, "{limited}");
}

#[test]
fn sets_the_post_filter() {
    let params = RuntimeParams::default().with_post_filter(0.);
    assert!(!params.post_filter);
    let params = params.with_post_filter(0.02);
    assert!(params.post_filter);
    let frames = tone_in_noise(50, 1);
    let mut filtered = DfTract::new(DfParams::default(), &params).unwrap();
    let mut plain = DfTract::new(
        DfParams::default(),
        &RuntimeParams::default().with_atten_lim(6.),
    )
    .unwrap();
    filtered.set_atten_lim(6.);
    assert_ne!(run(&mut filtered, &frames), run(&mut plain, &frames));

    for (beta, on, kept) in [(0.05, true, 0.05), (0., false, 0.), (-1., false, 0.)] {
        filtered.set_pf_beta(beta);
        assert_eq!(
            (filtered.post_filter, filtered.post_filter_beta),
            (on, kept)
        );
    }
}

#[test]
fn processes_channels_alike_with_every_mask_reduction() {
    // two equal channels: each comes out as the mono model's output, the gains reduced to one
    // set for both channels (max, mean) or not (none)
    let mono_frames = tone_in_noise(30, 1);
    let stereo_frames: Vec<_> = mono_frames
        .iter()
        .map(|f| ::ndarray::concatenate![::ndarray::Axis(0), f.view(), f.view()])
        .collect();
    let mono = run(&mut DfTract::default(), &mono_frames);
    for reduce in [ReduceMask::NONE, ReduceMask::MAX, ReduceMask::MEAN] {
        let params = RuntimeParams::default_with_ch(2).with_mask_reduce(reduce);
        let mut df = DfTract::new(DfParams::default(), &params).unwrap();
        assert_eq!(df.ch, 2);
        for (stereo, mono) in run(&mut df, &stereo_frames).iter().zip(&mono) {
            for channel in stereo.outer_iter() {
                for (x, y) in channel.iter().zip(mono.iter()) {
                    assert!((x - y).abs() < 1e-4, "{x} against {y}");
                }
            }
        }
    }
}

#[test]
fn processes_a_clipping_input() {
    // it only warns
    let mut frames = noise(10, 1, 0.5);
    frames[5][[0, 100]] = 1.;
    run(&mut DfTract::default(), &frames);
}

#[test]
fn sets_the_thresholds() {
    let params = RuntimeParams::default().with_thresholds(-20., 40., 35.);
    let df = DfTract::new(DfParams::default(), &params).unwrap();
    assert_eq!(
        (df.min_db_thresh, df.max_db_erb_thresh, df.max_db_df_thresh),
        (-20., 40., 35.)
    );
    // below the lowest: zeros, above the highest: unchanged, between them: gains, with the deep
    // filter only below the second
    assert_eq!(df.apply_stages(-25.), (false, true, false));
    assert_eq!(df.apply_stages(45.), (false, false, false));
    assert_eq!(df.apply_stages(38.), (true, false, false));
    assert_eq!(df.apply_stages(0.), (true, false, true));
}
