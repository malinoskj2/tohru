use std::env;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("XDG_RUNTIME_DIR must be an existing directory owned by this user")]
    Runtime,
    #[error("WAYLAND_DISPLAY is missing")]
    Display,
}

/// The directory holding Tohru's executables: `TOHRU_BIN_DIR`, else the running executable's.
#[must_use]
pub fn bin_dir() -> PathBuf {
    let configured = env::var_os("TOHRU_BIN_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let directory = configured.unwrap_or_else(|| {
        env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."))
    });
    directory.canonicalize().unwrap_or(directory)
}

/// The checkout or package root above `bin/`.
#[must_use]
pub fn root_dir() -> PathBuf {
    let bin = bin_dir();
    bin.parent().map_or(bin.clone(), Path::to_path_buf)
}

/// Installed data: `share/tohru` under the root.
#[must_use]
pub fn data_dir() -> PathBuf {
    root_dir().join("share/tohru")
}

/// The first of `shell/<relative>` (checkout) or `share/tohru/shell/<relative>` (package).
#[must_use]
pub fn shell_candidates(relative: &str) -> [PathBuf; 2] {
    let root = root_dir();
    [
        root.join("shell").join(relative),
        root.join("share/tohru/shell").join(relative),
    ]
}

#[must_use]
pub fn home() -> PathBuf {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

#[derive(Clone, Copy, Debug)]
pub enum Xdg {
    Config,
    State,
    Cache,
    Data,
}

impl Xdg {
    fn variable(self) -> &'static str {
        match self {
            Self::Config => "XDG_CONFIG_HOME",
            Self::State => "XDG_STATE_HOME",
            Self::Cache => "XDG_CACHE_HOME",
            Self::Data => "XDG_DATA_HOME",
        }
    }

    fn fallback(self) -> &'static str {
        match self {
            Self::Config => ".config",
            Self::State => ".local/state",
            Self::Cache => ".cache",
            Self::Data => ".local/share",
        }
    }
}

/// An XDG base directory; any non-empty value is honoured, as the shell scripts do.
#[must_use]
pub fn xdg(kind: Xdg) -> PathBuf {
    env::var_os(kind.variable())
        .filter(|value| !value.is_empty())
        .map_or_else(|| home().join(kind.fallback()), PathBuf::from)
}

/// An XDG base directory that ignores relative values, as the specification requires.
#[must_use]
pub fn xdg_absolute(kind: Xdg) -> PathBuf {
    env::var_os(kind.variable())
        .map(PathBuf::from)
        .filter(|value| value.is_absolute())
        .unwrap_or_else(|| home().join(kind.fallback()))
}

fn session_identity() -> String {
    let display = env::var("WAYLAND_DISPLAY").unwrap_or_default();
    let session = env::var("XDG_SESSION_ID").unwrap_or_default();
    let digest = Sha256::digest(format!("{display}\0{session}").as_bytes());
    hex(&digest)[..16].to_owned()
}

#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

#[must_use]
pub fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

/// `$XDG_RUNTIME_DIR` when it is an existing directory owned by this user.
pub fn runtime_dir() -> Result<PathBuf, SessionError> {
    let runtime = env::var_os("XDG_RUNTIME_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or(SessionError::Runtime)?;
    let metadata = std::fs::metadata(&runtime).map_err(|_| SessionError::Runtime)?;
    if !metadata.is_dir() || metadata.uid() != rustix::process::getuid().as_raw() {
        return Err(SessionError::Runtime);
    }
    Ok(runtime)
}

/// The per-session directory every Tohru helper shares, named from the display and session.
pub fn session_directory() -> Result<PathBuf, SessionError> {
    let runtime = runtime_dir()?;
    let identity = session_identity();
    if env::var_os("WAYLAND_DISPLAY").is_none_or(|value| value.is_empty()) {
        return Err(SessionError::Display);
    }
    Ok(runtime.join(format!("tohru-{identity}")))
}

/// The native locker's per-cycle status endpoint, beside the session directory.
#[must_use]
pub fn lock_status_endpoint(runtime: &Path) -> PathBuf {
    runtime.join(format!("tohru-lock-{}.status", session_identity()))
}

#[must_use]
pub fn uid() -> u32 {
    rustix::process::getuid().as_raw()
}
