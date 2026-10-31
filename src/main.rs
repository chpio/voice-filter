use ::std::path::PathBuf;
use ::std::process::ExitCode;
use ::std::sync::Arc;

use ::anyhow::Result;
use ::clap::{Parser, Subcommand};
use ::voice_filter::agc::Agc;
use ::voice_filter::atomic::AtomicF32;
use ::voice_filter::chain::Chain;
use ::voice_filter::denoise::Denoise;
use ::voice_filter::limiter::Limiter;
use ::voice_filter::{live, offline, settings};

/// Live voice filter for PipeWire.
#[derive(Parser)]
#[command(version, about)]
struct Args {
    #[command(subcommand)]
    command: Command,
    /// Limits how much the noise suppression attenuates, in dB; 100 and above don't limit it.
    /// Lower values keep more of the original voice, e.g. its highs.
    #[arg(long, global = true, default_value_t = 100.)]
    atten_lim: f32,
}

#[derive(Subcommand)]
enum Command {
    /// Provides the virtual microphone "Voice Filter": the microphone through the chain. Runs
    /// until Ctrl+C.
    Run {
        /// The microphone, a name from `voice-filter inputs`; kept for the next runs. Without
        /// one so far, the default input.
        #[arg(long)]
        input: Option<String>,
    },
    /// Lists the microphones (and other audio sources) for `run --input`.
    Inputs,
    /// Runs a WAV file (48 kHz, mixed down to mono) through the chain and writes the result, lined
    /// up with the input; prints the latency and processing time of every stage.
    Process { input: PathBuf, output: PathBuf },
    /// Opens the GNOME app, which switches the stages and settings of `voice-filter run` and shows
    /// its latency.
    #[cfg(feature = "gui")]
    Ui,
}

fn main() -> Result<ExitCode> {
    let args = Args::parse();
    let settings_dir = settings::dir();
    // the chain and the attenuation limit of its denoise stage, for the control interface
    let chain = || -> Result<(Chain, Arc<AtomicF32>)> {
        let denoise = Denoise::new()?.with_atten_lim(args.atten_lim);
        let atten_lim = denoise.atten_lim();
        let chain = Chain::new(vec![
            Box::new(denoise),
            Box::new(Agc::new()),
            Box::new(Limiter::new()),
        ]);
        Ok((chain, atten_lim))
    };
    match args.command {
        Command::Run { input } => {
            let input = match (input, &settings_dir) {
                (Some(input), Some(dir)) => {
                    settings::save_input(dir, &input)?;
                    Some(input)
                }
                (Some(input), None) => Some(input),
                (None, Some(dir)) => settings::load_input(dir)?,
                (None, None) => None,
            };
            match &input {
                Some(input) => eprintln!("Reading from {input}"),
                None => eprintln!("Reading from the default input"),
            }
            let (chain, atten_lim) = chain()?;
            live::run(chain, input.as_deref(), atten_lim, settings_dir)?;
        }
        Command::Inputs => {
            let saved = match &settings_dir {
                Some(dir) => settings::load_input(dir)?,
                None => None,
            };
            for input in live::inputs()? {
                let mark = if saved.as_deref() == Some(&input.name) {
                    "*"
                } else {
                    " "
                };
                println!(
                    "{mark} {}  ({}, priority {})",
                    input.name, input.description, input.priority
                );
            }
        }
        Command::Process { input, output } => {
            let report = offline::process_file(&mut chain()?.0, &input, &output)?;
            println!("{report}");
        }
        #[cfg(feature = "gui")]
        Command::Ui => return Ok(::voice_filter::gui::run().into()),
    }
    Ok(ExitCode::SUCCESS)
}
