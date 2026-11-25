use std::process::ExitCode;

use clap::Parser;
use serde_json::json;
use tohru_desktop::{pyjson, voice_status};

/// Verify an owned worker and acknowledge current voice interest in one tool call.
#[derive(Parser)]
#[command(args_override_self = true)]
struct Cli {
    /// Exact owned worker dispatch from its accepted receipt
    #[arg(long)]
    dispatch: String,
    /// Exact run from the same receipt
    #[arg(long)]
    run: String,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match voice_status::run(&cli.dispatch, &cli.run) {
        Ok(result) => {
            println!("{}", pyjson::dumps(&json!({"ok": true, "result": result})));
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!(
                "{}",
                pyjson::dumps(&json!({"ok": false, "error": error.to_string()}))
            );
            ExitCode::FAILURE
        }
    }
}
