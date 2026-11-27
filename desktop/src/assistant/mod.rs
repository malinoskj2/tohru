//! The desktop assistant: chat history and memory in `SQLite`, provider CLIs, and Orca Home delivery.

pub mod ack;
pub mod chat;
pub mod claude_api;
pub mod cli;
pub mod home;
pub mod ledger;
pub mod memory;
pub mod models;
pub mod needs_you;
pub mod orca;
pub mod quick;
pub mod store;
pub mod stream;
pub mod transfer;

#[cfg(test)]
mod tests;

use std::ffi::OsString;
use std::io::{Read, Write as _};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value, json};

use crate::discord::TOKEN_ENV as DISCORD_TOKEN_ENV;
use crate::paths::{self, Xdg};
use crate::pyjson;

pub const DISCORD_SOCKET_ENV: &str = "TOHRU_DISCORD_SOCKET";
pub const PROBE_SECONDS: u64 = 6;

#[derive(Debug, thiserror::Error)]
pub enum AssistantError {
    /// A `SystemExit` with a message: printed to stderr, exit status 1.
    #[error("{0}")]
    Exit(String),
    #[error("database")]
    Database(#[from] rusqlite::Error),
    #[error("storage")]
    Storage(#[from] std::io::Error),
    #[error("json")]
    Json(#[from] serde_json::Error),
    #[error("discord")]
    Discord(#[from] crate::discord::DiscordError),
}

impl AssistantError {
    pub(crate) fn exit(message: impl Into<String>) -> Self {
        Self::Exit(message.into())
    }
}

pub type Result<T, E = AssistantError> = std::result::Result<T, E>;

#[must_use]
pub fn home() -> PathBuf {
    paths::home()
}

fn configured(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// `TOHRU_ASSISTANT_DATA`, else `$XDG_DATA_HOME/tohru`.
#[must_use]
pub fn data_dir() -> PathBuf {
    configured("TOHRU_ASSISTANT_DATA")
        .unwrap_or_else(|| paths::xdg_absolute(Xdg::Data).join("tohru"))
}

/// Private per-user scratch: `TOHRU_ASSISTANT_SCRATCH`, else under the XDG state directory.
#[must_use]
pub fn scratch_dir() -> PathBuf {
    configured("TOHRU_ASSISTANT_SCRATCH")
        .unwrap_or_else(|| paths::xdg_absolute(Xdg::State).join("tohru/assistant/scratch"))
}

/// Seconds since the epoch, as `int(time.time())`.
#[must_use]
pub fn now() -> i64 {
    truncate(now_f64())
}

/// Python `int()` of a float: truncation toward zero, saturating at the bounds.
#[must_use]
#[expect(clippy::cast_possible_truncation, reason = "Python int() truncates")]
pub fn truncate(value: f64) -> i64 {
    value as i64
}

/// An integer as a float, for counts and timestamps well below 2^52.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "counts and milliseconds are far below 2^52"
)]
pub fn float(value: i64) -> f64 {
    value as f64
}

/// `time.time()`.
#[must_use]
pub fn now_f64() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_secs_f64())
}

#[must_use]
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

/// Parses a JSON file, treating unreadable or invalid files as absent.
#[must_use]
pub fn read_json(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn private_dir(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    Ok(path.to_path_buf())
}

/// Writes `text` through a private temporary file renamed into place.
pub fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut file = tempfile::Builder::new()
        .prefix(".tohru-export-")
        .tempfile_in(parent)?;
    file.write_all(text.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

/// The environment provider and helper processes inherit: no Discord credentials or broker.
#[must_use]
pub fn scrubbed_env() -> Vec<(OsString, OsString)> {
    std::env::vars_os()
        .filter(|(key, _)| key != DISCORD_TOKEN_ENV && key != DISCORD_SOCKET_ENV)
        .collect()
}

/// A finished bounded command.
#[derive(Debug, Default)]
pub struct Finished {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Finished {
    #[must_use]
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

/// Runs `command` with null stdin in its own session, collecting both streams; `None` when it
/// cannot start or outlives `timeout`.
pub fn run_bounded(mut command: Command, timeout: Duration) -> Option<Finished> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.process_group(0);
    let mut child = command.spawn().ok()?;
    let readers = [
        child.stdout.take().map(drain),
        child.stderr.take().map(drain),
    ];
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            _ => break None,
        }
    };
    let Some(status) = status else {
        crate::probe::kill_group(&mut child);
        return None;
    };
    let [stdout, stderr] = readers.map(|reader| {
        reader
            .and_then(|reader| reader.join().ok())
            .unwrap_or_default()
    });
    Some(Finished {
        code: status.code(),
        stdout,
        stderr,
    })
}

fn drain(mut stream: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut data = Vec::new();
        let _ = stream.read_to_end(&mut data);
        String::from_utf8_lossy(&data).into_owned()
    })
}

/// `quiet()`: a short probe of another CLI from the home directory without Discord credentials.
pub fn quiet<S: AsRef<std::ffi::OsStr>>(argv: &[S], timeout: Duration) -> Option<Finished> {
    let (program, rest) = argv.split_first()?;
    let mut command = Command::new(program);
    command
        .args(rest)
        .current_dir(home())
        .env_clear()
        .envs(scrubbed_env());
    run_bounded(command, timeout)
}

#[must_use]
pub fn probe_timeout() -> Duration {
    Duration::from_secs(PROBE_SECONDS)
}

/// Writes one event line to stdout, adding a spoken line to voiced failures.
pub fn emit(value: Value) {
    let value = quick::voice_failure(value);
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", pyjson::dumps_unicode(&value));
    let _ = out.flush();
}

/// Bounded local diagnostics. No prompt, answer, command or authority is stored.
pub fn home_metric(event: &str, fields: &[(&str, Value)]) {
    let _ = write_metric(event, fields);
}

fn write_metric(event: &str, fields: &[(&str, Value)]) -> std::io::Result<()> {
    let path = private_dir(&scratch_dir())?.join("home-metrics.jsonl");
    let file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(&path)?;
    rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive)?;
    if file.metadata()?.len() > 1024 * 1024 {
        file.set_len(0)?;
    }
    let mut record = Map::new();
    record.insert("event".into(), json!(event));
    record.insert("wall_ms".into(), json!(now_ms()));
    for (key, value) in fields {
        record.insert((*key).to_owned(), value.clone());
    }
    let mut file = file;
    writeln!(file, "{}", pyjson::dumps(&Value::Object(record)))
}

const DISCORD_SETTINGS: [&str; 11] = [
    "enabled",
    "read",
    "post",
    "guild_id",
    "channels",
    "token_file",
    "allowed_user_id",
    "desktop_first",
    "desktop_timeout_seconds",
    "destination",
    "receive",
];

/// The `assistant.*` strings, booleans and integers from the user and managed config files,
/// managed winning, sorted by key.
#[must_use]
pub fn assistant_settings() -> std::collections::BTreeMap<String, Value> {
    let mut values = std::collections::BTreeMap::new();
    let user = paths::xdg_absolute(Xdg::Config).join("tohru/config.toml");
    let managed = configured("TOHRU_MANAGED_CONFIG");
    for path in std::iter::once(user).chain(managed) {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(table) = text.parse::<toml::Table>() else {
            continue;
        };
        if let Some(toml::Value::Table(assistant)) = table.get("assistant") {
            walk_settings("", assistant, &mut values);
        }
    }
    values
}

fn walk_settings(
    prefix: &str,
    node: &toml::Table,
    values: &mut std::collections::BTreeMap<String, Value>,
) {
    for (key, value) in node {
        if prefix.starts_with("discord.")
            && (prefix != "discord." || !DISCORD_SETTINGS.contains(&key.as_str()))
        {
            continue;
        }
        let name = format!("assistant.{prefix}{key}");
        match value {
            toml::Value::Table(table) => walk_settings(&format!("{prefix}{key}."), table, values),
            toml::Value::String(text) => {
                values.insert(name, json!(text));
            }
            toml::Value::Boolean(flag) => {
                values.insert(name, json!(flag));
            }
            toml::Value::Integer(number) => {
                values.insert(name, json!(number));
            }
            _ => {}
        }
    }
}
