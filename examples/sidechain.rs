//! An experiment: the echo cancellation only decides, it doesn't shape the voice. DeepFilterNet3
//! analyzes the microphone after the echo cancellation, as in the chain, but its gains go to a mix
//! of that and the raw microphone (delayed by the echo cancellation's latency): music and other
//! noise from the speakers the model removes by itself, the echo cancellation's suppressor, which
//! also turns down the voice while you talk into the playback, only steers it.
//!
//! ```text
//! cargo run --release --example sidechain -- mic.wav played.wav \
//!   --voice 0..21 --echo 60..85 [--lead 5] [--out out/]
//! ```
//!
//! It builds a double talk whose voice is known: the echo section (the microphone and what the
//! speakers played) with the voice section (a recording of the speaker alone) added after `--lead`
//! seconds of echo alone. Every variant runs on that and on the voice alone, the latter is what it
//! should give for the voice; per variant:
//!
//! - `echo`: the level in the lead (from its second second on, AEC3 needs a moment), what's left
//!   of the echo alone,
//! - `voice`: the level of the double talk's output against the voice alone's, how much of the
//!   voice it keeps (0 dB: all of it; residual echo raises it, a suppressed voice lowers it),
//! - per band (24, 100 Hz to 20 kHz) and 20 ms where the voice alone's output has the voice (at
//!   most 40 dB below its loudest), how far the double talk's output lies from it: `loss` the mean
//!   of how much quieter (the voice turned down), `leak` of how much louder (residual echo), `lost`
//!   the share more than 6 dB quieter (choppy).
//!
//! With `--out` it writes every variant's output over the whole recording (with its real double
//! talk) as `<variant>.wav`, to listen to.

use ::std::ops::Range;
use ::std::path::PathBuf;

use ::anyhow::{Context, Result, ensure};
use ::clap::Parser;
use ::voice_filter::denoise::Denoise;
use ::voice_filter::dsp::{power_db, ramp};
use ::voice_filter::echo::{Echo, Reference};
use ::voice_filter::stage::{Frame, Stage};
use ::voice_filter::{FRAME_LEN, SAMPLE_RATE, offline};

#[derive(Parser)]
struct Args {
    /// The microphone, a WAV file at 48 kHz.
    mic: PathBuf,
    /// What the speakers played meanwhile.
    reference: PathBuf,
    /// The section with the speaker alone, in seconds (`start..end`).
    #[arg(long, value_parser = seconds)]
    voice: Range<f32>,
    /// The section with the playback alone, in seconds (`start..end`), longer than `--lead`.
    #[arg(long, value_parser = seconds)]
    echo: Range<f32>,
    /// The seconds of echo alone before the voice comes in.
    #[arg(long, default_value_t = 5.)]
    lead: f32,
    /// The mixes to try: 1 is the echo cancellation's output as now, 0 the raw microphone.
    #[arg(long, value_delimiter = ',', default_value = "1,0.5,0.25,0.1,0")]
    mix: Vec<f32>,
    /// Raises the echo section in the microphone by this many dB (not what the speakers played),
    /// as with louder speakers; its noise rises with it.
    #[arg(long, default_value_t = 0., allow_negative_numbers = true)]
    echo_gain: f32,
    /// The SNR estimates above which the adaptive variants take it for a voice, in dB.
    #[arg(
        long,
        value_delimiter = ',',
        default_value = "0",
        allow_negative_numbers = true
    )]
    speech_db: Vec<f32>,
    /// Writes every variant's output over the whole recording into this directory.
    #[arg(long)]
    out: Option<PathBuf>,
}

fn seconds(arg: &str) -> Result<Range<f32>, String> {
    let (start, end) = arg.split_once("..").ok_or("expected START..END")?;
    let parse = |s: &str| s.parse::<f32>().map_err(|_| format!("not a number: {s}"));
    Ok(parse(start)?..parse(end)?)
}

/// How a variant runs.
#[derive(Clone, Copy)]
struct Variant {
    /// Without the echo cancellation: DeepFilterNet3 on the raw microphone.
    echo: bool,
    /// The share of the echo cancellation's output in what the gains go to, the rest is the raw
    /// microphone.
    mix: f32,
    /// Whether the deep filter is applied too.
    df: bool,
    /// `mix` and `df` only while DeepFilterNet3 hears a voice (its SNR above `speech_db`, held
    /// for [`HOLD_FRAMES`]), otherwise the chain as now: the echo cancellation's output, with the
    /// deep filter.
    adaptive: bool,
    speech_db: f32,
}

/// How long [`Variant::adaptive`] stays with the voice after it, in frames (200 ms).
const HOLD_FRAMES: usize = 20;

impl Variant {
    fn name(&self) -> String {
        if !self.echo {
            return "no-echo".to_owned();
        }
        format!(
            "{}-{:.2}{}",
            if self.adaptive {
                format!("adapt{:+.0}", self.speech_db)
            } else {
                "mix".to_owned()
            },
            self.mix,
            if self.df { "" } else { "-nodf" }
        )
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mic = offline::read_wav(&args.mic)?;
    let played = offline::read_wav(&args.reference)?;
    ensure!(
        args.echo.end - args.echo.start > args.lead,
        "--echo is shorter than --lead"
    );
    let section = |samples: &[f32], range: &Range<f32>| -> Result<Vec<f32>> {
        let start = (range.start * SAMPLE_RATE as f32) as usize;
        let end = (range.end * SAMPLE_RATE as f32) as usize;
        samples
            .get(start..end)
            .map(<[f32]>::to_vec)
            .context("section outside the recording")
    };
    let voice = section(&mic, &args.voice)?;
    let echo = section(&mic, &args.echo)?;
    let echo_played = section(&played, &args.echo)?;
    // the voice after the lead, as long as the echo lasts
    let lead = (args.lead * SAMPLE_RATE as f32) as usize;
    let len = echo.len().min(lead + voice.len());
    let mut voice_only = vec![0.; len];
    let talk = len - lead;
    voice_only[lead..].copy_from_slice(&voice[..talk]);
    let echo_gain = 10f32.powf(args.echo_gain / 20.);
    let echo: Vec<f32> = echo.iter().map(|e| e * echo_gain).collect();
    let double_talk: Vec<f32> = echo[..len]
        .iter()
        .zip(&voice_only)
        .map(|(e, v)| e + v)
        .collect();
    let silence = vec![0.; len];

    let mut variants = vec![Variant {
        echo: false,
        mix: 1.,
        df: true,
        adaptive: false,
        speech_db: 0.,
    }];
    for &mix in &args.mix {
        for df in [true, false] {
            variants.push(Variant {
                echo: true,
                mix,
                df,
                adaptive: false,
                speech_db: 0.,
            });
        }
    }
    for &speech_db in &args.speech_db {
        for &mix in &args.mix {
            for df in [true, false] {
                // adaptive at 1 with the deep filter is the chain as now
                if mix < 1. || !df {
                    variants.push(Variant {
                        echo: true,
                        mix,
                        df,
                        adaptive: true,
                        speech_db,
                    });
                }
            }
        }
    }

    println!(
        "input: echo {:.1} dBFS (played {:.1}), voice {:.1} dBFS",
        level_db(&echo[..len]),
        level_db(&echo_played[..len]),
        level_db(&voice_only[lead..]),
    );
    println!("variant\techo dBFS\tvoice dB\tloss dB\tleak dB\tlost %\tgaps dB");
    {
        let talk_range = (lead + SAMPLE_RATE as usize).min(len)..len;
        println!(
            "input\t{:.1}\t{:+.1}\t{}",
            level_db(&double_talk[SAMPLE_RATE as usize..lead]),
            level_db(&double_talk[talk_range.clone()]) - level_db(&voice_only[talk_range.clone()]),
            Deviation::new(&double_talk[talk_range.clone()], &voice_only[talk_range]),
        );
    }
    // the lead from its second second on, the double talk from its first second on
    let second = SAMPLE_RATE as usize;
    for variant in &variants {
        let out = run(*variant, &double_talk, &echo_played[..len])?;
        let target = run(*variant, &voice_only, &silence)?;
        let echo_db = level_db(&out[second.min(lead)..lead]);
        let talk_range = (lead + second).min(len)..len;
        let voice_db = level_db(&out[talk_range.clone()]) - level_db(&target[talk_range.clone()]);
        let deviation = Deviation::new(&out[talk_range.clone()], &target[talk_range]);
        if ::std::env::var_os("SIDECHAIN_DEBUG").is_some() {
            for (s, (o, t)) in out.chunks(second).zip(target.chunks(second)).enumerate() {
                eprintln!(
                    "{s}\t{:.1}\t{:.1}\t{}",
                    level_db(o),
                    level_db(t),
                    Deviation::new(o, t)
                );
            }
        }
        println!(
            "{}\t{echo_db:.1}\t{voice_db:+.1}\t{deviation}",
            variant.name()
        );
        if let Some(dir) = &args.out {
            ::std::fs::create_dir_all(dir)?;
            let out = run(*variant, &mic, &played)?;
            offline::write_wav(&dir.join(format!("{}.wav", variant.name())), &out)?;
        }
    }
    Ok(())
}

/// The output of a variant, shifted by its latency so it lines up with the input.
fn run(variant: Variant, mic: &[f32], played: &[f32]) -> Result<Vec<f32>> {
    let (mut echo, mut reference): (Echo, Reference) = Echo::new();
    let mut denoise = Denoise::new()?;
    let echo_latency = if variant.echo { echo.latency() } else { 0 };
    let latency = echo_latency + denoise.latency();
    // the raw microphone, delayed by the echo cancellation's latency so it lines up with its output
    let mut delayed = vec![0f32; echo_latency];
    let mut out = Vec::with_capacity(mic.len() + latency);
    let mut padded = mic.to_vec();
    padded.resize((mic.len() + latency).next_multiple_of(FRAME_LEN), 0.);
    let speech = denoise.speech();
    // frames left to stay with the voice, and the mix of the frame before
    let mut hold = 0;
    let mut mix_before = if variant.adaptive { 1. } else { variant.mix };
    for (i, samples) in padded.as_chunks::<FRAME_LEN>().0.iter().enumerate() {
        let mut frame: Frame = *samples;
        if !variant.echo {
            denoise.process(&mut frame)?;
            out.extend_from_slice(&frame);
            continue;
        }
        let start = (i * FRAME_LEN).min(played.len());
        let end = ((i + 1) * FRAME_LEN).min(played.len());
        let mut reference_frame = [0.; FRAME_LEN];
        reference_frame[..end - start].copy_from_slice(&played[start..end]);
        reference.push(&reference_frame);

        // the raw frame as far back as the echo cancellation's output
        delayed.extend_from_slice(&frame);
        let mut raw: Frame = [0.; FRAME_LEN];
        raw.copy_from_slice(&delayed[..FRAME_LEN]);
        delayed.drain(..FRAME_LEN);

        echo.process(&mut frame)?;
        // the mix and the deep filter for this frame: adaptive ones by whether the last frame
        // had a voice, crossfading over the frame
        let voice = !variant.adaptive || hold > 0;
        let (mix, df) = if voice {
            (variant.mix, variant.df)
        } else {
            (1., true)
        };
        if mix >= 1. && mix_before >= 1. && df {
            denoise.process(&mut frame)?;
        } else {
            let mut apply: Frame = [0.; FRAME_LEN];
            for (((a, e), r), m) in apply
                .iter_mut()
                .zip(&frame)
                .zip(&raw)
                .zip(ramp(mix_before, mix))
            {
                *a = m * e + (1. - m) * r;
            }
            denoise.process_with(&mut frame, Some(&apply), df)?;
        }
        mix_before = mix;
        hold = if speech.snr_db() > variant.speech_db {
            HOLD_FRAMES
        } else {
            hold.saturating_sub(1)
        };
        out.extend_from_slice(&frame);
    }
    Ok(out[latency..latency + mic.len()].to_vec())
}

/// The level of samples in dBFS, the mean power.
fn level_db(samples: &[f32]) -> f32 {
    let power = samples.iter().map(|x| f64::from(x * x)).sum::<f64>() / samples.len().max(1) as f64;
    power_db(power as f32)
}

/// How far a signal lies from a target per band and STFT frame, where the target has something.
struct Deviation {
    /// The mean of how much quieter, in dB.
    loss: f64,
    /// The mean of how much louder, in dB.
    leak: f64,
    /// The share more than 6 dB quieter.
    lost: f64,
    /// The mean power where the target has nothing (between syllables, in bands without the
    /// voice) against its mean power where it has, in dB: what comes through besides the voice.
    gaps: f64,
}

/// The STFT of [`Deviation`]: 20 ms frames, 10 ms apart.
const FFT_LEN: usize = 960;
const BANDS: usize = 24;

impl Deviation {
    fn new(signal: &[f32], target: &[f32]) -> Self {
        let signal = band_powers(signal);
        let target = band_powers(target);
        let loudest = target.iter().flatten().copied().fold(0f64, f64::max);
        let floor = (loudest * 1e-4).max(1e-12);
        let (mut loss, mut leak, mut lost, mut cells) = (0., 0., 0, 0);
        let (mut gap_power, mut gap_cells, mut voice_power) = (0., 0, 0.);
        for (s, t) in signal.iter().flatten().zip(target.iter().flatten()) {
            if *t < floor {
                gap_power += s;
                gap_cells += 1;
                continue;
            }
            voice_power += t;
            let dev = (10. * (s.max(1e-20) / t).log10()).clamp(-30., 30.);
            loss += (-dev).max(0.);
            leak += dev.max(0.);
            lost += usize::from(dev < -6.);
            cells += 1;
        }
        let cells = cells.max(1) as f64;
        let gap_mean = gap_power / gap_cells.max(1) as f64;
        let voice_mean = voice_power / cells;
        Self {
            loss: loss / cells,
            leak: leak / cells,
            lost: lost as f64 / cells,
            gaps: 10. * (gap_mean.max(1e-30) / voice_mean.max(1e-30)).log10(),
        }
    }
}

impl ::std::fmt::Display for Deviation {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(
            f,
            "{:.1}\t{:.1}\t{:.0}\t{:.1}",
            self.loss,
            self.leak,
            self.lost * 100.,
            self.gaps
        )
    }
}

/// The power per band and frame, Hann window.
fn band_powers(samples: &[f32]) -> Vec<[f64; BANDS]> {
    let fft = ::realfft::RealFftPlanner::<f32>::new().plan_fft_forward(FFT_LEN);
    let window: Vec<f32> = (0..FFT_LEN)
        .map(|i| 0.5 - 0.5 * (2. * ::std::f32::consts::PI * i as f32 / FFT_LEN as f32).cos())
        .collect();
    // log-spaced band edges from 100 Hz to 20 kHz, in bins
    let bin = |freq: f64| (freq * FFT_LEN as f64 / f64::from(SAMPLE_RATE)).round() as usize;
    let edges: Vec<usize> = (0..=BANDS)
        .map(|b| bin(100. * 200f64.powf(b as f64 / BANDS as f64)))
        .collect();
    let mut input = fft.make_input_vec();
    let mut spectrum = fft.make_output_vec();
    let mut powers = Vec::new();
    let mut start = 0;
    while start + FFT_LEN <= samples.len() {
        for ((x, s), w) in input.iter_mut().zip(&samples[start..]).zip(&window) {
            *x = s * w;
        }
        fft.process(&mut input, &mut spectrum)
            .expect("buffers of the plan");
        let mut bands = [0f64; BANDS];
        for (b, band) in bands.iter_mut().enumerate() {
            let bins = &spectrum[edges[b]..edges[b + 1].max(edges[b] + 1)];
            *band = bins.iter().map(|c| f64::from(c.norm_sqr())).sum::<f64>() / bins.len() as f64;
        }
        powers.push(bands);
        start += FRAME_LEN;
    }
    powers
}
