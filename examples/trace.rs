//! The level after every stage of the chain and what the stages applied (the gain of the AGC and
//! the expander, the estimate of the noise suppression, …) over time, as TSV on stdout, to measure
//! the stages on a recording and compare settings:
//!
//! ```text
//! cargo run --release --example trace -- in.wav [--reference played.wav] \
//!   [--set expander.threshold=6] [--off echo] [--interval 0.01] > trace.tsv
//! ```
//!
//! A row per interval (1 s by default, at least a frame): its start in seconds, the level of the
//! input and after every stage (dBFS, the mean power over the interval, −120 for silence), then
//! the mean of every stage's [`Stage::trace`]. The levels are where the frames are, without
//! shifting by the latency of the stages before. A stage switched off with `--off` passes its
//! input on, without its latency, its trace stays empty.

use ::std::io::{BufWriter, Write};
use ::std::path::PathBuf;

use ::anyhow::{Context, Result, ensure};
use ::clap::Parser;
use ::voice_filter::dsp::power_db;
use ::voice_filter::param::Param;
use ::voice_filter::stage::{Frame, Stage};
use ::voice_filter::{FRAME_LEN, SAMPLE_RATE, chain, offline};

#[derive(Parser)]
struct Args {
    /// The recording, a WAV file at 48 kHz.
    input: PathBuf,
    /// What the speakers played meanwhile, for the echo cancellation.
    #[arg(long)]
    reference: Option<PathBuf>,
    /// A parameter, as `stage.name=value` (e.g. `expander.threshold=6`); can be repeated.
    #[arg(long = "set", value_name = "STAGE.NAME=VALUE", value_parser = setting)]
    settings: Vec<(String, String, f32)>,
    /// A stage to leave out; can be repeated.
    #[arg(long, value_name = "STAGE")]
    off: Vec<String>,
    /// The seconds per row; 0.01 gives a row per frame.
    #[arg(long, default_value_t = 1.)]
    interval: f32,
}

fn setting(arg: &str) -> Result<(String, String, f32), String> {
    let (key, value) = arg.split_once('=').ok_or("expected STAGE.NAME=VALUE")?;
    let (stage, name) = key.split_once('.').ok_or("expected STAGE.NAME=VALUE")?;
    let value = value
        .parse()
        .map_err(|_| format!("not a number: {value}"))?;
    Ok((stage.to_owned(), name.to_owned(), value))
}

fn main() -> Result<()> {
    let args = Args::parse();
    let input = offline::read_wav(&args.input)?;
    let played = args
        .reference
        .as_deref()
        .map(offline::read_wav)
        .transpose()?;
    let (mut chain, mut reference) = chain::build(None)?;
    let params = chain.params();
    for (stage, name, value) in &args.settings {
        Param::find(&params, stage, name)
            .with_context(|| format!("no parameter {name} of {stage}"))?
            .set(*value)?;
    }
    let names: Vec<&'static str> = chain.stages().map(Stage::name).collect();
    for off in &args.off {
        ensure!(names.contains(&off.as_str()), "no stage {off}");
    }
    let on: Vec<bool> = names
        .iter()
        .map(|name| !args.off.iter().any(|off| off == name))
        .collect();
    let traces: Vec<Option<&'static str>> = chain
        .stages()
        .map(|stage| stage.trace().map(|(name, _)| name))
        .collect();
    let frames_per_row = (args.interval * SAMPLE_RATE as f32 / FRAME_LEN as f32).round() as usize;
    let frames_per_row = frames_per_row.max(1);

    let mut out = BufWriter::new(::std::io::stdout().lock());
    write!(out, "time\tinput")?;
    for name in &names {
        write!(out, "\t{name}")?;
    }
    for (stage, trace) in names.iter().zip(&traces) {
        if let Some(trace) = trace {
            write!(out, "\t{stage}.{trace}")?;
        }
    }
    writeln!(out)?;

    // the input and after every stage
    let mut powers = vec![0f64; names.len() + 1];
    let mut sums = vec![0f64; names.len()];
    let mut frames = 0;
    let chunks = input.as_chunks::<FRAME_LEN>().0;
    for (i, samples) in chunks.iter().enumerate() {
        if let Some(played) = &played {
            let start = (i * FRAME_LEN).min(played.len());
            let end = ((i + 1) * FRAME_LEN).min(played.len());
            let mut frame = [0.; FRAME_LEN];
            frame[..end - start].copy_from_slice(&played[start..end]);
            reference.push(&frame);
        }
        let mut frame: Frame = *samples;
        powers[0] += power(&frame);
        for (j, stage) in chain.stages_mut().enumerate() {
            if on[j] {
                stage.process(&mut frame)?;
                if let Some((_, value)) = stage.trace() {
                    sums[j] += f64::from(value);
                }
            }
            powers[j + 1] += power(&frame);
        }
        frames += 1;
        if frames == frames_per_row || i + 1 == chunks.len() {
            let start = (i + 1 - frames) * FRAME_LEN;
            write!(out, "{:.2}", start as f64 / f64::from(SAMPLE_RATE))?;
            for power in &powers {
                write!(out, "\t{:.1}", power_db((power / frames as f64) as f32))?;
            }
            for ((sum, trace), on) in sums.iter().zip(&traces).zip(&on) {
                match (trace, on) {
                    (Some(_), true) => write!(out, "\t{:.1}", sum / frames as f64)?,
                    (Some(_), false) => write!(out, "\t")?,
                    (None, _) => {}
                }
            }
            writeln!(out)?;
            powers.fill(0.);
            sums.fill(0.);
            frames = 0;
        }
    }
    out.flush()?;
    Ok(())
}

/// The mean power of a frame.
fn power(frame: &Frame) -> f64 {
    frame.iter().map(|x| f64::from(x * x)).sum::<f64>() / FRAME_LEN as f64
}
