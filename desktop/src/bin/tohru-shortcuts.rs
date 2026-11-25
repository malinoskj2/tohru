use std::collections::BTreeMap;
use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use serde_json::json;

use tohru_desktop::config::{Config, ShortcutsMode};
use tohru_desktop::pyjson;
use tohru_desktop::shortcuts::portal::{self, Action};

/// The opt-in `GlobalShortcuts` portal owner; other modes contact nothing.
#[derive(Parser)]
#[command(name = "tohru-shortcuts")]
struct Cli {
    /// Report the portal client this build carries.
    #[arg(long)]
    check_dependencies: bool,
}

fn run(cli: &Cli) -> Result<()> {
    let config = Config::load()?;
    if cli.check_dependencies {
        println!("{}", pyjson::dumps(&json!({"portal_client": "built-in"})));
        return Ok(());
    }
    if config.shortcuts_mode != ShortcutsMode::Portal {
        println!(
            "tohru-shortcuts: portal ownership disabled (mode {})",
            config.shortcuts_mode.name()
        );
        return Ok(());
    }
    let preferred: BTreeMap<Action, String> = Action::ALL
        .into_iter()
        .map(|action| {
            (
                action,
                config
                    .text(&format!("shortcuts.{}_trigger", action.id()))
                    .to_owned(),
            )
        })
        .collect();
    portal::serve(preferred)?;
    Ok(())
}

fn main() -> ExitCode {
    match run(&Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("tohru-shortcuts: {error:#}");
            ExitCode::FAILURE
        }
    }
}
