//! An experiment: how well DeepFilterNet3 tells speech in what the speakers play (a call, a video)
//! from music, and how rarely it has to run for that. For the echo cancellation, which is only
//! needed for the former: DeepFilterNet3 removes music by itself.
//!
//! ```text
//! cargo run --release --example reference_vad -- played.wav[@START..END]... \
//!   [--duty 1/1,1/2,1/4,10/40] [--per-second]
//! ```
//!
//! Only DeepFilterNet3's encoder runs ([`DfTract::analyze`], its SNR estimate without the
//! decoders, the deep filter and the synthesis), on `ON` of every `PERIOD` frames (`--duty
//! ON/PERIOD`): `1/4` every fourth frame (it sees the signal four times as fast), `10/40` runs of
//! 10 frames with 30 left out (it sees the signal as it is, with gaps). Per file (or section, in
//! seconds) and duty: the time per frame (over all frames) and how many seconds have a mean SNR
//! (of the frames it analyzed) above each threshold, and the highest; with `--per-second` the
//! means themselves.

use ::std::path::PathBuf;
use ::std::time::{Duration, Instant};

use ::anyhow::{Context, Result, ensure};
use ::clap::Parser;
use ::deep_filter_rt::tract::DfTract;
use ::ndarray::Array2;
use ::voice_filter::{FRAME_LEN, SAMPLE_RATE, offline};

#[derive(Parser)]
struct Args {
    /// What the speakers played, WAV files at 48 kHz, each optionally with a section in seconds
    /// (`file.wav@60..85`).
    files: Vec<String>,
    /// How often the encoder runs, `ON/PERIOD` frames.
    #[arg(
        long,
        value_delimiter = ',',
        default_value = "1/1,1/2,1/4,1/8,10/40,10/80,25/100"
    )]
    duty: Vec<String>,
    /// The thresholds of the mean SNR per second, in dB.
    #[arg(
        long,
        value_delimiter = ',',
        default_value = "0,3,5",
        allow_negative_numbers = true
    )]
    snr_db: Vec<f32>,
    /// Prints the mean of every duty over every second too.
    #[arg(long)]
    per_second: bool,
    /// The seconds a mean goes over.
    #[arg(long, default_value_t = 1.)]
    window: f32,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let duties = args
        .duty
        .iter()
        .map(|duty| -> Result<(usize, usize)> {
            let (on, period) = duty.split_once('/').context("expected ON/PERIOD")?;
            let (on, period) = (on.parse()?, period.parse()?);
            ensure!(on >= 1 && on <= period, "ON must be 1 to PERIOD: {duty}");
            Ok((on, period))
        })
        .collect::<Result<Vec<_>>>()?;
    for arg in &args.files {
        let (path, range) = match arg.split_once('@') {
            Some((path, range)) => (path, Some(range)),
            None => (arg.as_str(), None),
        };
        let samples = offline::read_wav(&PathBuf::from(path))?;
        let samples = match range {
            Some(range) => {
                let (start, end) = range.split_once("..").context("expected START..END")?;
                let at = |s: &str| -> Result<usize> {
                    Ok((s.parse::<f32>()? * SAMPLE_RATE as f32) as usize)
                };
                samples
                    .get(at(start)?..at(end)?)
                    .context("section outside the file")?
                    .to_vec()
            }
            None => samples,
        };
        run(arg, &samples, &duties, &args)?;
    }
    Ok(())
}

fn run(name: &str, samples: &[f32], duties: &[(usize, usize)], args: &Args) -> Result<()> {
    let frames = samples.as_chunks::<FRAME_LEN>().0;
    // the frames a mean goes over ("seconds" below are windows of `--window` seconds)
    let per_second = (args.window * SAMPLE_RATE as f32 / FRAME_LEN as f32).round() as usize;
    println!(
        "{name}: {:.1} s",
        samples.len() as f64 / f64::from(SAMPLE_RATE)
    );
    let mut all_means = Vec::new();
    for &(on, period) in duties {
        let mut encoder = DfTract::default();
        let mut noisy = Array2::zeros((1, FRAME_LEN));
        let mut time = Duration::ZERO;
        // per second: the sum of the SNR of the frames analyzed and how many
        let mut sums = vec![(0f32, 0usize); frames.len().div_ceil(per_second)];
        for (i, frame) in frames.iter().enumerate() {
            if i % period >= on {
                continue;
            }
            noisy
                .row_mut(0)
                .as_slice_mut()
                .unwrap()
                .copy_from_slice(frame);
            let start = Instant::now();
            let snr = encoder.analyze(noisy.view())?;
            time += start.elapsed();
            let sum = &mut sums[i / per_second];
            sum.0 += snr;
            sum.1 += 1;
        }
        let means: Vec<f32> = sums
            .iter()
            .filter(|(_, n)| *n > 0)
            .map(|(sum, n)| sum / *n as f32)
            .collect();
        let above: Vec<String> = args
            .snr_db
            .iter()
            .map(|&db| format!(">{db} {:3} s", means.iter().filter(|&&m| m > db).count()))
            .collect();
        let highest = means.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        println!(
            "  {:>7} {:5.0} µs/frame  {}  max {highest:5.1}",
            format!("{on}/{period}"),
            time.as_secs_f64() * 1e6 / frames.len().max(1) as f64,
            above.join(", "),
        );
        all_means.push(means);
    }
    if args.per_second {
        let header: Vec<String> = duties.iter().map(|(on, p)| format!("{on}/{p}")).collect();
        println!("  second\t{}", header.join("\t"));
        let seconds = all_means.iter().map(Vec::len).min().unwrap_or(0);
        for s in 0..seconds {
            let row: Vec<String> = all_means.iter().map(|m| format!("{:.1}", m[s])).collect();
            println!("  {s}\t{}", row.join("\t"));
        }
    }
    Ok(())
}
