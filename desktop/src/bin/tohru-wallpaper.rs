use std::process::ExitCode;

/// mpvpaper ownership: one foreground supervisor per Wayland session.
///
/// Commands use a private Unix socket; mpv IPC paths are never borrowed from another desktop's
/// wallpaper processes.
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match tohru_desktop::wallpaper::main(&args) {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(error) => {
            eprintln!("tohru-wallpaper: {error}");
            ExitCode::FAILURE
        }
    }
}
