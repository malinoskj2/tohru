use std::process::ExitCode;

/// Opt-in swayidle owner; logind and protocol inhibition remain swayidle's responsibility.
fn main() -> ExitCode {
    if std::env::args().len() > 1 {
        eprintln!("usage: tohru-idle");
        return ExitCode::from(2);
    }
    match tohru_desktop::idle::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("tohru-idle: {error}");
            ExitCode::FAILURE
        }
    }
}
