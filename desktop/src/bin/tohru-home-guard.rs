use std::io::Read;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use serde_json::Value;
use tohru_desktop::{home_guard, paths, pyjson};

/// Bound foreground Home commands while leaving replies and background jobs alone.
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    mode: Mode,
}

#[derive(Subcommand)]
enum Mode {
    Plan {
        #[arg(long, allow_hyphen_values = true)]
        remaining_ms: i64,
    },
    Run {
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..=5000))]
        budget_ms: u64,
        #[arg(long, allow_hyphen_values = true)]
        command: String,
    },
}

fn main() -> anyhow::Result<ExitCode> {
    match Cli::parse().mode {
        Mode::Plan { remaining_ms } => {
            let mut raw = Vec::new();
            std::io::stdin().take(65537).read_to_end(&mut raw)?;
            let value: Value = serde_json::from_slice(&raw)?;
            let request = value
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("expected a tool request object"))?;
            let guard = paths::bin_dir().join("tohru-home-guard");
            let plan = home_guard::plan(
                request.get("tool").and_then(Value::as_str),
                request.get("input"),
                remaining_ms,
                &guard,
            );
            println!("{}", pyjson::dumps(&plan.to_json()));
            Ok(ExitCode::SUCCESS)
        }
        Mode::Run { budget_ms, command } => {
            let code = home_guard::foreground(&command, budget_ms)?;
            Ok(ExitCode::from(u8::try_from(code & 0xff).unwrap_or(1)))
        }
    }
}
