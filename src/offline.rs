//! The test mode: runs WAV files through the same chain as the live filter, to compare models
//! and settings and to check that every stage keeps up with real time.

use ::std::fmt;
use ::std::path::Path;
use ::std::time::{Duration, Instant};

use ::anyhow::{Context as _, Result, ensure};
use ::hound::{SampleFormat, WavReader, WavSpec, WavWriter};

use crate::chain::Chain;
use crate::stage::Frame;
use crate::{FRAME_LEN, SAMPLE_RATE};

/// Latency and processing time of one stage.
pub struct StageReport {
    pub name: &'static str,
    /// In samples.
    pub latency: usize,
    /// Mean processing time per frame.
    pub mean: Duration,
    /// The longest processing time of a single frame.
    pub max: Duration,
}

pub struct Report {
    /// The length of the input.
    pub duration: Duration,
    pub frames: usize,
    pub stages: Vec<StageReport>,
    /// The longest processing time of a single frame through the whole chain.
    pub max: Duration,
}

impl Report {
    /// The latency of the whole chain, in samples.
    pub fn latency(&self) -> usize {
        self.stages.iter().map(|stage| stage.latency).sum()
    }

    /// The mean processing time of a frame through the whole chain.
    pub fn mean(&self) -> Duration {
        self.stages.iter().map(|stage| stage.mean).sum()
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ms = |d: Duration| d.as_secs_f64() * 1e3;
        let latency_ms = |samples: usize| samples as f64 * 1e3 / SAMPLE_RATE as f64;
        writeln!(
            f,
            "{:.1} s, {} frames of {:.0} ms",
            self.duration.as_secs_f64(),
            self.frames,
            latency_ms(FRAME_LEN)
        )?;
        writeln!(
            f,
            "{:<12} {:>9} {:>11} {:>11}",
            "stage", "latency", "mean/frame", "max/frame"
        )?;
        let mut row = |name: &str, latency: usize, mean: Duration, max: Duration| {
            writeln!(
                f,
                "{name:<12} {:>6.1} ms {:>8.2} ms {:>8.2} ms",
                latency_ms(latency),
                ms(mean),
                ms(max)
            )
        };
        for stage in &self.stages {
            row(stage.name, stage.latency, stage.mean, stage.max)?;
        }
        row("total", self.latency(), self.mean(), self.max)?;
        write!(
            f,
            "mean {:.1} %, max {:.1} % of the {:.0} ms a frame may take live",
            ms(self.mean()) / latency_ms(FRAME_LEN) * 100.,
            ms(self.max) / latency_ms(FRAME_LEN) * 100.,
            latency_ms(FRAME_LEN)
        )
    }
}

/// Reads a WAV file with [`SAMPLE_RATE`] as mono samples, mixing down all channels.
pub fn read_wav(path: &Path) -> Result<Vec<f32>> {
    let mut reader =
        WavReader::open(path).with_context(|| format!("Could not open {}", path.display()))?;
    let spec = reader.spec();
    ensure!(
        spec.sample_rate == SAMPLE_RATE,
        "{} has {} Hz, only {SAMPLE_RATE} Hz are supported",
        path.display(),
        spec.sample_rate
    );
    let samples: Vec<f32> = match spec.sample_format {
        SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        SampleFormat::Int => {
            let scale = 1. / (1u64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|sample| sample as f32 * scale))
                .collect::<Result<_, _>>()?
        }
    };
    let channels = spec.channels as usize;
    Ok(samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect())
}

/// Writes mono samples as a 32-bit float WAV file with [`SAMPLE_RATE`].
pub fn write_wav(path: &Path, samples: &[f32]) -> Result<()> {
    let spec = WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut writer = WavWriter::create(path, spec)
        .with_context(|| format!("Could not create {}", path.display()))?;
    for &sample in samples {
        writer.write_sample(sample)?;
    }
    writer.finalize()?;
    Ok(())
}

/// Runs `input` through `chain` frame by frame and times each stage. The output is shifted by the
/// latency of the chain, so it lines up with the input and has the same length; the input is
/// padded with silence for that.
pub fn process(chain: &mut Chain, input: &[f32]) -> Result<(Vec<f32>, Report)> {
    let latency = chain.latency();
    let frames = (input.len() + latency).div_ceil(FRAME_LEN);
    let mut output = Vec::with_capacity(frames * FRAME_LEN);
    let mut times = vec![(Duration::ZERO, Duration::ZERO); chain.stages().len()];
    let mut max = Duration::ZERO;
    let mut frame: Frame = [0.; FRAME_LEN];
    for i in 0..frames {
        let start = (i * FRAME_LEN).min(input.len());
        let end = ((i + 1) * FRAME_LEN).min(input.len());
        frame.fill(0.);
        frame[..end - start].copy_from_slice(&input[start..end]);
        let mut total = Duration::ZERO;
        for (stage, (sum, stage_max)) in chain.stages_mut().iter_mut().zip(&mut times) {
            let started = Instant::now();
            stage.process(&mut frame)?;
            let elapsed = started.elapsed();
            *sum += elapsed;
            *stage_max = (*stage_max).max(elapsed);
            total += elapsed;
        }
        max = max.max(total);
        output.extend_from_slice(&frame);
    }
    output.drain(..latency);
    output.truncate(input.len());

    let stages = chain
        .stages()
        .iter()
        .zip(times)
        .map(|(stage, (sum, max))| StageReport {
            name: stage.name(),
            latency: stage.latency(),
            mean: sum / frames.max(1) as u32,
            max,
        })
        .collect();
    let report = Report {
        duration: Duration::from_secs_f64(input.len() as f64 / SAMPLE_RATE as f64),
        frames,
        stages,
        max,
    };
    Ok((output, report))
}

/// [`process`] from one WAV file into another, see [`read_wav`] and [`write_wav`].
pub fn process_file(chain: &mut Chain, input: &Path, output: &Path) -> Result<Report> {
    let samples = read_wav(input)?;
    let (processed, report) = process(chain, &samples)?;
    write_wav(output, &processed)?;
    Ok(report)
}
