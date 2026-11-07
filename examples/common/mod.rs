//! What `batch`, `trace` and `sidechain` share: the arguments that set up the chain, and the
//! frames of a recording.

// each example compiles it on its own and uses only part of it
#![allow(dead_code)]

use ::anyhow::{Context, Result, ensure};
use ::voice_filter::FRAME_LEN;
use ::voice_filter::chain::{self, Chain};
use ::voice_filter::denoise::Denoise;
use ::voice_filter::echo::Reference;
use ::voice_filter::param::Param;
use ::voice_filter::stage::{Frame, Stage};

/// A `--set` argument, `stage.name=value`.
pub fn setting(arg: &str) -> Result<(String, String, f32), String> {
    let (key, value) = arg.split_once('=').ok_or("expected STAGE.NAME=VALUE")?;
    let (stage, name) = key.split_once('.').ok_or("expected STAGE.NAME=VALUE")?;
    let value = value
        .parse()
        .map_err(|_| format!("not a number: {value}"))?;
    Ok((stage.to_owned(), name.to_owned(), value))
}

/// A `--thresholds` argument, `zeros,unchanged,no-df` in dB.
pub fn thresholds(arg: &str) -> Result<(f32, f32, f32), String> {
    let values: Vec<f32> = arg
        .split(',')
        .map(|value| value.parse().map_err(|_| format!("not a number: {value}")))
        .collect::<Result<_, _>>()?;
    match values[..] {
        [zeros, unchanged, no_df] => Ok((zeros, unchanged, no_df)),
        _ => Err("expected three numbers, ZEROS,UNCHANGED,NO-DF".to_owned()),
    }
}

/// The chain of `chain::build_with`, set up by the arguments.
pub struct Setup {
    pub chain: Chain,
    pub reference: Reference,
    /// The names of its stages, in order.
    pub names: Vec<&'static str>,
    /// Which stages run: the others pass their input on, without their latency.
    pub on: Vec<bool>,
}

impl Setup {
    /// The chain with the noise suppression's `thresholds`, the parameters of `settings` and the
    /// stages of `off` left out, and with `only` not empty all but those.
    pub fn new(
        thresholds: Option<(f32, f32, f32)>,
        settings: &[(String, String, f32)],
        off: &[String],
        only: &[String],
    ) -> Result<Self> {
        let mut denoise = Denoise::new()?;
        if let Some((zeros, unchanged, no_df)) = thresholds {
            denoise = denoise.with_thresholds(zeros, unchanged, no_df);
        }
        let (chain, reference) = chain::build_with(denoise)?;
        let params = chain.params();
        for (stage, name, value) in settings {
            Param::find(&params, stage, name)
                .with_context(|| format!("no parameter {name} of {stage}"))?
                .set(*value)?;
        }
        let names: Vec<&'static str> = chain.stages().map(Stage::name).collect();
        for name in off.iter().chain(only) {
            ensure!(names.contains(&name.as_str()), "no stage {name}");
        }
        let on = names
            .iter()
            .map(|name| {
                !off.iter().any(|off| off == name)
                    && (only.is_empty() || only.iter().any(|only| only == name))
            })
            .collect();
        Ok(Self {
            chain,
            reference,
            names,
            on,
        })
    }

    /// The latency of the stages that run, in samples.
    pub fn latency(&self) -> usize {
        self.chain
            .stages()
            .zip(&self.on)
            .filter(|(_, on)| **on)
            .map(|(stage, _)| stage.latency())
            .sum()
    }
}

/// The `i`th frame of `samples`, padded with silence past their end.
pub fn frame_at(samples: &[f32], i: usize) -> Frame {
    let mut frame: Frame = [0.; FRAME_LEN];
    let start = (i * FRAME_LEN).min(samples.len());
    let end = ((i + 1) * FRAME_LEN).min(samples.len());
    frame[..end - start].copy_from_slice(&samples[start..end]);
    frame
}
