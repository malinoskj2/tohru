use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_json::Value;

use tohru_desktop::config::Config;
use tohru_desktop::shortcuts::{self, Operation};
use tohru_desktop::{greeter, paths, pyjson, units};

/// Portable session setup and read-only diagnostics.
#[derive(Parser)]
#[command(name = "tohru")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write systemd user units without enabling or starting any.
    SessionUnits {
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        prefix: Option<PathBuf>,
        #[arg(long, default_value = "graphical-session.target")]
        target: String,
        #[arg(long, default_value = "bar,session,launcher")]
        components: String,
        #[arg(long)]
        managed: Option<String>,
    },
    /// Print the `PipeWire` echo-cancel drop-in, or create it without overwriting.
    EchoCancel {
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Report shortcut ownership, or ask the portal owner to set up or configure grants.
    Shortcuts {
        #[arg(value_enum, default_value_t = Operation::Status)]
        operation: Operation,
    },
    /// Check greetd readiness as the current user.
    GreeterCheck {
        #[arg(long, default_value = "/etc/greetd/config.toml")]
        config: PathBuf,
        #[arg(long, default_value = "/etc/greetd/tohru.toml")]
        settings: PathBuf,
        #[arg(long = "sessions-dir")]
        sessions_dir: Vec<PathBuf>,
        #[arg(long)]
        json: bool,
    },
}

fn run(command: Command) -> Result<ExitCode> {
    match command {
        Command::SessionUnits {
            output,
            prefix,
            target,
            components,
            managed,
        } => {
            let specs = units::load_specs()?;
            let prefix = prefix.unwrap_or_else(paths::bin_dir);
            let request = units::UnitRequest {
                output: &output,
                prefix: &prefix,
                target: &target,
                components: &components,
                managed: managed.as_deref(),
            };
            let count = units::generate(&specs, &request)?;
            println!(
                "Generated {count} units in {}; no services enabled or started",
                output.display()
            );
        }
        Command::EchoCancel { output } => {
            if let Some(path) = units::echo_cancel(output.as_deref())? {
                println!(
                    "Created {}; restart PipeWire yourself when convenient; routes unchanged",
                    path.display()
                );
            }
        }
        Command::Shortcuts { operation } => {
            let report = shortcuts::report(&Config::load()?, operation)?;
            println!("{}", pyjson::dumps(&Value::Object(report)));
        }
        Command::GreeterCheck {
            config,
            settings,
            sessions_dir,
            json,
        } => {
            let report = greeter::check(&greeter::Request {
                config,
                settings,
                sessions: sessions_dir,
            })?;
            if json {
                println!("{}", pyjson::dumps(&report));
            } else {
                println!("{}", greeter::text_report(&report));
            }
            return Ok(if greeter::healthy(&report) {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            });
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    run(cli.command).unwrap_or_else(|error| {
        eprintln!("tohru: {error:#}");
        ExitCode::from(2)
    })
}
