//! Bounded persistence of last-selected username and session under `TOHRU_GREETER_STATE_DIR`.
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use thiserror::Error;

const MAX_STATE_BYTES: u64 = 16 * 1024;
const STATE_FILE: &str = "state.json";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LastState {
    pub last_user: Option<String>,
    pub last_session: Option<String>,
}

#[derive(Debug, Error)]
pub enum StateError {
    #[error("cannot write greeter state at {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("cannot serialize greeter state")]
    Serialize(#[from] serde_json::Error),

    #[error("cannot persist greeter state through symbolic link: {0}")]
    Symlink(PathBuf),
}

/// Load persisted `(last_user, last_session)` from `dir/state.json`.
/// Missing or unreadable/corrupt files return `LastState::default()` so login never breaks.
#[must_use]
pub fn load(dir: &Path) -> LastState {
    let path = dir.join(STATE_FILE);
    let Ok(file) = File::open(&path) else {
        return LastState::default();
    };
    let mut raw = String::new();
    if file
        .take(MAX_STATE_BYTES + 1)
        .read_to_string(&mut raw)
        .is_err()
        || raw.len() as u64 > MAX_STATE_BYTES
    {
        return LastState::default();
    }
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&raw) else {
        return LastState::default();
    };
    let read_str = |key: &str| {
        map.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
            .map(ToOwned::to_owned)
    };
    LastState {
        last_user: read_str("last_user"),
        last_session: read_str("last_session"),
    }
}

/// Atomically persist `(last_user, last_session)` to `dir/state.json`.
///
/// # Errors
/// Returns an error if the destination is a symlink or cannot be written.
pub fn save(dir: &Path, state: &LastState) -> Result<(), StateError> {
    let path = dir.join(STATE_FILE);
    if fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(StateError::Symlink(path));
    }

    let mut obj = Map::new();
    if let Some(user) = &state.last_user {
        obj.insert("last_user".into(), Value::String(user.clone()));
    }
    if let Some(session) = &state.last_session {
        obj.insert("last_session".into(), Value::String(session.clone()));
    }
    let bytes = serde_json::to_vec_pretty(&Value::Object(obj))?;

    let write = || -> std::io::Result<()> {
        fs::create_dir_all(dir)?;
        let temp = dir.join(format!(".tohru-greeter-state-{}.tmp", std::process::id()));
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o640);
        }
        let mut file = opts.open(&temp)?;
        let outcome = (|| {
            file.write_all(&bytes)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            fs::rename(&temp, &path)
        })();
        if outcome.is_err() {
            let _ = fs::remove_file(&temp);
        }
        outcome
    };

    write().map_err(|source| StateError::Write {
        path: path.clone(),
        source,
    })
}
