use ::std::path::PathBuf;

use ::anyhow::Result;
use ::clap::{Parser, Subcommand};
use ::voice_filter::chain::Chain;
use ::voice_filter::denoise::Denoise;
use ::voice_filter::offline;

/// Live voice filter for PipeWire.
#[derive(Parser)]
#[command(version, about)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Runs a WAV file (48 kHz, mixed down to mono) through the chain and writes the result, lined
    /// up with the input; prints the latency and processing time of every stage.
    Process { input: PathBuf, output: PathBuf },
}

fn main() -> Result<()> {
    match Args::parse().command {
        Command::Process { input, output } => {
            let mut chain = Chain::new(vec![Box::new(Denoise::new()?)]);
            let report = offline::process_file(&mut chain, &input, &output)?;
            println!("{report}");
        }
    }
    Ok(())
}
