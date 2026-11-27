use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};
use tohru_desktop::screens;

/// Rolling monitor captures on tmpfs while the dragon is on screen.
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Capture every monitor now and every interval until stdin closes, then delete every capture
    Run {
        #[arg(long, default_value_t = 3000, value_parser = clap::value_parser!(u64).range(500..=60_000))]
        interval_ms: u64,
        #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u64).range(1..=60))]
        keep: u64,
        #[arg(long, default_value_t = 85, value_parser = clap::value_parser!(u8).range(1..=100))]
        quality: u8,
    },
    /// Delete leftover captures unless a runner owns them
    Clear,
    /// Wait for stdin to close, then clear unless a runner owns the folder
    #[command(hide = true)]
    Reap,
    /// Print the folder
    Path,
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Commands::Run {
            interval_ms,
            keep,
            quality,
        } => screens::run(
            screens::Policy {
                interval: Duration::from_millis(interval_ms),
                keep: usize::try_from(keep).unwrap_or(10),
            },
            quality,
        ),
        Commands::Clear => screens::directory().and_then(|dir| screens::clear(&dir).map(drop)),
        Commands::Reap => screens::reap(),
        Commands::Path => screens::directory().map(|dir| println!("{}", dir.display())),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("tohru-screens: {error}");
            ExitCode::FAILURE
        }
    }
}
