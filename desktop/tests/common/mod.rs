#![allow(
    dead_code,
    unreachable_pub,
    reason = "each integration test uses a different subset"
)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

/// The `bash` on `PATH`, for fake-command shebangs that also work under Nix.
pub fn bash() -> String {
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join("bash"))
        .find(|candidate| candidate.is_file())
        .unwrap()
        .display()
        .to_string()
}

/// The first `name` on `PATH`.
pub fn which(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

/// Writes an executable bash script named `name` into `directory`.
pub fn script(directory: &Path, name: &str, body: &str) -> PathBuf {
    let path = directory.join(name);
    std::fs::write(&path, format!("#!{}\n{body}", bash())).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// `PATH` with `directory` searched first.
pub fn path_with(directory: &Path) -> String {
    format!("{}:{}", directory.display(), std::env::var("PATH").unwrap())
}

/// The per-session runtime directory Tohru derives from these display and session names.
pub fn session_dir(runtime: &Path, display: &str, session: &str) -> PathBuf {
    let digest = Sha256::digest(format!("{display}\0{session}").as_bytes());
    let hex: String = tohru_desktop::paths::hex(&digest);
    runtime.join(format!("tohru-{}", &hex[..16]))
}

/// Polls `condition` for up to five seconds.
pub fn wait_for<T>(mut condition: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Some(value) = condition() {
            return value;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("bounded local IPC setup timed out");
}
