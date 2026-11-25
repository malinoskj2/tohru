use std::process::ExitCode;

use clap::{Parser, Subcommand};
use serde_json::json;

use tohru_desktop::afk::{self, AfkError, ControlAction, Event, Manager};
use tohru_desktop::config::{IdleOwner, IdleSource};
use tohru_desktop::pyjson;

/// Maintained desktop idle notifications, never a lock or suspend owner.
#[derive(Parser)]
#[command(name = "tohru-afk-idle")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, Subcommand)]
enum Command {
    /// Run the systemd-owned AFK helper.
    Serve,
    /// Print this generation's callbacks for the given idle manager.
    Callbacks {
        #[arg(long)]
        manager_pid: i64,
    },
    /// Print the AFK artwork directory.
    Assets,
    /// Start or stop the AFK service with the bar's requested source.
    Control {
        #[arg(value_enum)]
        action: ControlAction,
        #[arg(long)]
        idle_seconds: Option<i64>,
        #[arg(long, value_enum)]
        source: Option<IdleSource>,
        #[arg(long, value_enum)]
        owner: Option<IdleOwner>,
    },
    /// Forward one swayidle callback to the serving generation.
    Notify {
        #[arg(value_enum)]
        event: Event,
        #[arg(long)]
        generation: i64,
        #[arg(long)]
        manager_pid: Option<i64>,
        #[arg(long)]
        manager_start: Option<u64>,
    },
}

fn run(command: Command) -> Result<(), AfkError> {
    match command {
        Command::Serve => afk::serve(),
        Command::Callbacks { manager_pid } => {
            println!("{}", pyjson::dumps(&afk::callbacks(manager_pid)?));
            Ok(())
        }
        Command::Assets => {
            let art = afk::assets()?;
            println!("{}", pyjson::dumps(&json!({"art": art.to_string_lossy()})));
            Ok(())
        }
        Command::Control {
            action,
            idle_seconds,
            source,
            owner,
        } => afk::control(action, idle_seconds.map(|seconds| (seconds, source, owner))),
        Command::Notify {
            event,
            generation,
            manager_pid,
            manager_start,
        } => {
            let manager = manager_pid
                .zip(manager_start)
                .map(|(pid, start)| Manager { pid, start });
            afk::notify(event, generation, manager)
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("tohru-afk-idle: {error}");
            ExitCode::FAILURE
        }
    }
}
