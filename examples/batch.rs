//! Runs every WAV file of a directory through the chain, or some of its stages, each with a fresh
//! state, and writes what comes out to another directory, lined up with the input; prints the
//! time per frame. To score settings on a test set (`scripts/score.py`) or to listen to them on
//! recordings, see `docs/measurements.md`:
//!
//! ```text
//! cargo run --release --example batch -- in/ out/ [--only denoise] [--off echo] \
//!   [--set denoise.level=-30] [--thresholds -10,30,20] [--reference-dir played/]
//! ```
//!
//! A stage left out with `--off` (or not named with `--only`) passes its input on, without its
//! latency, as in `examples/trace.rs`.

use ::std::path::PathBuf;
use ::std::time::{Duration, Instant};

use ::anyhow::{Context, Result};
use ::clap::Parser;
use ::voice_filter::dsp::without_denormals;
use ::voice_filter::{FRAME_LEN, offline};

mod common;

#[derive(Parser)]
struct Args {
    /// The directory of WAV files at 48 kHz.
    input: PathBuf,
    /// Where the output goes, a file of the same name for each.
    output: PathBuf,
    /// What the speakers played meanwhile, for the echo cancellation: a directory with files of
    /// the same names (one that's missing means none).
    #[arg(long)]
    reference_dir: Option<PathBuf>,
    /// A parameter, as `stage.name=value` (e.g. `denoise.level=-30`); can be repeated.
    #[arg(long = "set", value_name = "STAGE.NAME=VALUE", value_parser = common::setting)]
    settings: Vec<(String, String, f32)>,
    /// A stage to leave out; can be repeated.
    #[arg(long, value_name = "STAGE")]
    off: Vec<String>,
    /// Only these stages, the others left out; can be repeated.
    #[arg(long, value_name = "STAGE")]
    only: Vec<String>,
    /// The thresholds of the noise suppression's skipping, in dB, as `zeros,unchanged,no-df` (see
    /// `Denoise::with_thresholds`; upstream's defaults are `-10,30,20`).
    #[arg(long, value_name = "DB,DB,DB", value_parser = common::thresholds, allow_hyphen_values = true)]
    thresholds: Option<(f32, f32, f32)>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut files: Vec<PathBuf> = ::std::fs::read_dir(&args.input)
        .with_context(|| format!("Could not read {}", args.input.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<_, _>>()?;
    files.retain(|path| path.extension().is_some_and(|ext| ext == "wav"));
    files.sort();
    ::std::fs::create_dir_all(&args.output)?;

    let (mut frames, mut total, mut max) = (0u64, Duration::ZERO, Duration::ZERO);
    for path in &files {
        let mut setup = common::Setup::new(args.thresholds, &args.settings, &args.off, &args.only)?;
        let latency = setup.latency();

        let input = offline::read_wav(path)?;
        let played = match &args.reference_dir {
            Some(dir) => {
                let played = dir.join(path.file_name().unwrap());
                played
                    .exists()
                    .then(|| offline::read_wav(&played))
                    .transpose()?
            }
            None => None,
        };
        let count = (input.len() + latency).div_ceil(FRAME_LEN);
        let mut output = Vec::with_capacity(count * FRAME_LEN);
        for i in 0..count {
            if let Some(played) = &played {
                setup.reference.push(&common::frame_at(played, i));
            }
            let mut frame = common::frame_at(&input, i);
            let started = Instant::now();
            without_denormals(|| {
                for (stage, on) in setup.chain.stages_mut().zip(&setup.on) {
                    if *on {
                        stage.process(&mut frame)?;
                    }
                }
                Ok::<_, ::anyhow::Error>(())
            })?;
            let elapsed = started.elapsed();
            total += elapsed;
            max = max.max(elapsed);
            frames += 1;
            output.extend_from_slice(&frame);
        }
        output.drain(..latency);
        output.truncate(input.len());
        offline::write_wav(&args.output.join(path.file_name().unwrap()), &output)?;
    }
    let ms = |duration: Duration| duration.as_secs_f64() * 1e3;
    println!(
        "{} files, {frames} frames, mean {:.3} ms, max {:.2} ms per frame",
        files.len(),
        ms(total) / frames.max(1) as f64,
        ms(max),
    );
    Ok(())
}
