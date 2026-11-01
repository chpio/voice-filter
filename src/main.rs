use ::std::path::{Path, PathBuf};
use ::std::process::ExitCode;

use ::anyhow::Result;
use ::clap::{Parser, Subcommand};
use ::voice_filter::denoise;
use ::voice_filter::param::Param;
use ::voice_filter::{chain, live, offline, settings};

/// Live voice filter for PipeWire.
#[derive(Parser)]
#[command(version, about)]
struct Args {
    #[command(subcommand)]
    command: Command,
    /// Limits how much the noise suppression attenuates, in dB; 100 and above don't limit it.
    /// Lower values keep more of the original voice, e.g. its highs. `run` keeps it for the next
    /// runs, like a limit set in the app; without it, `run` takes the kept one and `process` 100.
    #[arg(long, global = true)]
    atten_lim: Option<f32>,
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
    /// Installs the desktop entry and the icon of the app into `~/.local/share`
    /// (`$XDG_DATA_HOME`), so it's among the apps and has its icon in the dock. The entry starts
    /// this binary by its full path.
    #[cfg(feature = "gui")]
    InstallDesktop,
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

/// Prints the microphones or output devices, the saved one marked with `*`.
fn list(
    direction: live::Direction,
    settings_dir: Option<&Path>,
    load: fn(&Path) -> Result<Option<String>>,
) -> Result<()> {
    let saved = settings_dir.map(load).transpose()?.flatten();
    for device in live::devices(direction)? {
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
    Ok(())
}

fn main() -> Result<ExitCode> {
    let args = Args::parse();
    let settings_dir = settings::dir();
    let settings_dir = settings_dir.as_deref();
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
            let (chain, reference) = chain::build(args.atten_lim)?;
            if let Some(dir) = settings_dir {
                let (switches, params) = (chain.switches(), chain.params());
                for warning in settings::restore_chain(dir, &switches, &params)? {
                    eprintln!("{warning}");
                }
                // a limit given on the command line wins over the kept one, and is kept instead
                if let Some(db) = args.atten_lim {
                    if let Some(limit) = Param::find(&params, "denoise", "limit") {
                        limit.set(db.clamp(0., denoise::UNLIMITED))?;
                    }
                    settings::save_chain(dir, &switches, &params)?;
                }
            }
            let devices = live::Devices {
                input: input.as_deref(),
                output: output.as_deref(),
            };
            live::run(chain, reference, devices, settings_dir.map(Path::to_owned))?;
        }
        Command::Inputs => list(live::Direction::Input, settings_dir, settings::load_input)?,
        Command::Outputs => list(live::Direction::Output, settings_dir, settings::load_output)?,
        Command::Process {
            input,
            output,
            reference: played,
        } => {
            let (mut chain, mut reference) = chain::build(args.atten_lim)?;
            let reference = played.as_deref().map(|path| (&mut reference, path));
            let report = offline::process_file(&mut chain, &input, &output, reference)?;
            println!("{report}");
        }
        #[cfg(feature = "gui")]
        Command::Ui => return Ok(::voice_filter::gui::run().into()),
        #[cfg(feature = "gui")]
        Command::InstallDesktop => {
            use ::voice_filter::gui::desktop;
            let data_home = desktop::data_home()?;
            let exe = ::std::env::current_exe()?;
            for path in desktop::install(&data_home, &exe)? {
                println!("Wrote {}", path.display());
            }
            desktop::update_icon_cache(&data_home);
        }
    }
    Ok(ExitCode::SUCCESS)
}
