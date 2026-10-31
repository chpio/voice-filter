use ::std::path::PathBuf;

use ::anyhow::Result;
use ::clap::{Parser, Subcommand};
use ::voice_filter::chain::Chain;
use ::voice_filter::denoise::Denoise;
use ::voice_filter::{live, offline};

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
    /// Provides the virtual microphone "Voice Filter": the default microphone through the chain.
    /// Runs until Ctrl+C.
    Run,
    /// Runs a WAV file (48 kHz, mixed down to mono) through the chain and writes the result, lined
    /// up with the input; prints the latency and processing time of every stage.
    Process { input: PathBuf, output: PathBuf },
}

fn main() -> Result<()> {
    let args = Args::parse();
    let denoise = Denoise::new()?.with_atten_lim(args.atten_lim);
    let chain = Chain::new(vec![Box::new(denoise)]);
    match args.command {
        Command::Run => live::run(chain)?,
        Command::Process { input, output } => {
            let mut chain = chain;
            let report = offline::process_file(&mut chain, &input, &output)?;
            println!("{report}");
        }
    }
    Ok(())
}
