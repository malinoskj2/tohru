use std::process::ExitCode;

use tohru_desktop::session_action::{self, Action};

/// Session menu and idle actions; a successful systemd start means a compositor-confirmed lock.
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match Action::parse(&args).and_then(session_action::run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("tohru-session-action: {error}");
            ExitCode::FAILURE
        }
    }
}
