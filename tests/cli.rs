//! The binary's offline mode, end to end with the whole chain.

mod common;

use ::std::path::Path;
use ::std::process::{Command, Output};

use ::voice_filter::{SAMPLE_RATE, offline};
use common::Noise;

/// Runs the binary with `args`, without the user's settings.
fn voice_filter(state: &Path, args: &[&Path]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_voice-filter"))
        .args(args)
        .env("XDG_STATE_HOME", state)
        .output()
        .unwrap()
}

/// 1 s of a tone in noise, at about -20 dBFS.
fn signal() -> Vec<f32> {
    let mut noise = Noise(0x2545_f491);
    (0..SAMPLE_RATE as usize)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            0.1 * (2. * ::std::f32::consts::PI * 220. * t).sin() + 0.01 * noise.sample()
        })
        .collect()
}

#[test]
fn processes_a_file_through_the_whole_chain() {
    let dir = ::tempfile::tempdir().unwrap();
    let (input, played, output) = (
        dir.path().join("in.wav"),
        dir.path().join("played.wav"),
        dir.path().join("out.wav"),
    );
    let samples = signal();
    offline::write_wav(&input, &samples).unwrap();
    offline::write_wav(&played, &samples).unwrap();

    for args in [
        &[Path::new("process"), &input, &output][..],
        &[
            Path::new("process"),
            &input,
            &output,
            Path::new("--reference"),
            &played,
        ],
    ] {
        let run = voice_filter(dir.path(), args);
        let stdout = String::from_utf8_lossy(&run.stdout);
        assert!(
            run.status.success(),
            "{stdout}{}",
            String::from_utf8_lossy(&run.stderr)
        );
        // lined up with the input
        let processed = offline::read_wav(&output).unwrap();
        assert_eq!(processed.len(), samples.len());
        assert!(processed.iter().all(|x| x.is_finite() && x.abs() <= 1.));
        // a row per stage, in the order of the chain
        let stages: Vec<&str> = stdout
            .lines()
            .skip(2)
            .filter_map(|line| line.split_whitespace().next())
            .collect();
        assert_eq!(
            stages[..10],
            [
                "echo",
                "denoise",
                "highpass",
                "eq",
                "deesser",
                "agc",
                "compressor",
                "expander",
                "limiter",
                "total"
            ],
            "{stdout}"
        );
    }
    // the offline mode keeps no settings
    assert!(!dir.path().join("voice-filter").exists());
}

#[test]
fn fails_on_a_missing_input() {
    let dir = ::tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.wav");
    let output = dir.path().join("out.wav");
    let run = voice_filter(dir.path(), &[Path::new("process"), &missing, &output]);
    assert!(!run.status.success());
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains("missing.wav"), "{stderr}");
    assert!(!output.exists());
}
