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
    let command = Args::parse().command;
    let chain = Chain::new(vec![Box::new(Denoise::new()?)]);
    match command {
        Command::Run => live::run(chain)?,
        Command::Process { input, output } => {
            let mut chain = chain;
            let report = offline::process_file(&mut chain, &input, &output)?;
            println!("{report}");
        }
    }
    Ok(())
}
