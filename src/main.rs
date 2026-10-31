use ::std::path::{Path, PathBuf};
use ::std::process::ExitCode;

use ::anyhow::Result;
use ::clap::{Parser, Subcommand};
use ::voice_filter::agc::Agc;
use ::voice_filter::chain::Chain;
use ::voice_filter::compressor::Compressor;
use ::voice_filter::deesser::DeEsser;
use ::voice_filter::denoise::Denoise;
use ::voice_filter::echo::{Echo, Reference};
use ::voice_filter::equalizer::Equalizer;
use ::voice_filter::expander::Expander;
use ::voice_filter::highpass::HighPass;
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
        /// The output device (speakers) whose echo gets removed, a name from `voice-filter
        /// outputs`; kept for the next runs. Without one so far, the default output.
        #[arg(long)]
        output: Option<String>,
    },
    /// Lists the microphones (and other audio sources) for `run --input`.
    Inputs,
    /// Lists the output devices for `run --output`.
    Outputs,
    /// Runs a WAV file (48 kHz, mixed down to mono) through the chain and writes the result, lined
    /// up with the input; prints the latency and processing time of every stage.
    Process {
        input: PathBuf,
        output: PathBuf,
        /// What the speakers played while `input` was recorded (48 kHz, from the start of it),
        /// for the echo cancellation; without it, it leaves the input as it is.
        #[arg(long)]
        reference: Option<PathBuf>,
    },
    /// Opens the GNOME app, which switches the stages and settings of `voice-filter run` and shows
    /// its latency.
    #[cfg(feature = "gui")]
    Ui,
}

/// The device given on the command line, which is saved, or else the saved one.
fn device(
    given: Option<String>,
    settings_dir: Option<&Path>,
    load: fn(&Path) -> Result<Option<String>>,
    save: fn(&Path, &str) -> Result<()>,
) -> Result<Option<String>> {
    Ok(match (given, settings_dir) {
        (Some(name), Some(dir)) => {
            save(dir, &name)?;
            Some(name)
        }
        (Some(name), None) => Some(name),
        (None, Some(dir)) => load(dir)?,
        (None, None) => None,
    })
}

/// Prints the devices, the saved one marked with `*`.
fn list(devices: Vec<live::Device>, saved: Option<String>) {
    for device in devices {
        let mark = if saved.as_deref() == Some(&device.name) {
            "*"
        } else {
            " "
        };
        println!(
            "{mark} {}  ({}, priority {})",
            device.name, device.description, device.priority
        );
    }
}

fn main() -> Result<ExitCode> {
    let args = Args::parse();
    let settings_dir = settings::dir();
    let settings_dir = settings_dir.as_deref();
    let chain = || -> Result<(Chain, Reference)> {
        let (echo, reference) = Echo::new();
        let denoise = Denoise::new()?.with_atten_lim(args.atten_lim);
        let agc = Agc::new();
        let compressor = Compressor::new().following(agc.target());
        let expander = Expander::new().listening(denoise.speech());
        let chain = Chain::new(vec![
            Box::new(echo),
            Box::new(denoise),
            Box::new(HighPass::new()),
            Box::new(Equalizer::new()),
            Box::new(DeEsser::new()),
            Box::new(agc),
            Box::new(compressor),
            Box::new(expander),
            Box::new(Limiter::new()),
        ]);
        Ok((chain, reference))
    };
    match args.command {
        Command::Run { input, output } => {
            let input = device(
                input,
                settings_dir,
                settings::load_input,
                settings::save_input,
            )?;
            let output = device(
                output,
                settings_dir,
                settings::load_output,
                settings::save_output,
            )?;
            match &input {
                Some(input) => eprintln!("Reading from {input}"),
                None => eprintln!("Reading from the default input"),
            }
            match &output {
                Some(output) => eprintln!("Removing the echo of {output}"),
                None => eprintln!("Removing the echo of the default output"),
            }
            let (chain, reference) = chain()?;
            let devices = live::Devices {
                input: input.as_deref(),
                output: output.as_deref(),
            };
            live::run(chain, reference, devices, settings_dir.map(Path::to_owned))?;
        }
        Command::Inputs => {
            let saved = settings_dir
                .map(settings::load_input)
                .transpose()?
                .flatten();
            list(live::devices(live::Direction::Input)?, saved);
        }
        Command::Outputs => {
            let saved = settings_dir
                .map(settings::load_output)
                .transpose()?
                .flatten();
            list(live::devices(live::Direction::Output)?, saved);
        }
        Command::Process {
            input,
            output,
            reference: played,
        } => {
            let (mut chain, mut reference) = chain()?;
            let reference = played.as_deref().map(|path| (&mut reference, path));
            let report = offline::process_file(&mut chain, &input, &output, reference)?;
            println!("{report}");
        }
        #[cfg(feature = "gui")]
        Command::Ui => return Ok(::voice_filter::gui::run().into()),
    }
    Ok(ExitCode::SUCCESS)
}
