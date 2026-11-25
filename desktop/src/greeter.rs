use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

use crate::config::{ConfigError, read_toml};
use crate::probe::{DEFAULT_TIMEOUT, probe, which};
use crate::{paths, shell};

#[derive(Debug, thiserror::Error)]
pub enum GreeterError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("greetd requires default_session command and user")]
    Session,
    #[error("greeter font families must be strings")]
    Font,
}

pub struct Request {
    pub config: PathBuf,
    pub settings: PathBuf,
    pub sessions: Vec<PathBuf>,
}

pub const LIMITS: &str = "Run under the intended greeter identity/environment. No compositor, authentication, login session or privileged changes attempted.";

struct Checks(Vec<Value>);

impl Checks {
    fn add(&mut self, name: &str, valid: bool, detail: impl Into<String>) {
        let state = if valid { "ok" } else { "missing-or-unreadable" };
        self.0
            .push(json!({"name": name, "state": state, "detail": detail.into()}));
    }
}

fn readable(path: &Path) -> bool {
    rustix::fs::access(path, rustix::fs::Access::READ_OK).is_ok()
}

/// The account's UID through NSS, as `getent passwd` reports it.
fn account_uid(user: &str) -> Option<u32> {
    let entry = probe("getent", &["passwd", user], Duration::from_secs(2)).ok()?;
    entry.lines().next()?.split(':').nth(2)?.parse().ok()
}

/// Readable session `.desktop` files, at most 64 per directory.
fn session_files(sessions: &[PathBuf]) -> usize {
    let defaults = [
        PathBuf::from("/usr/local/share"),
        PathBuf::from("/usr/share"),
    ];
    let directories = if sessions.is_empty() {
        &defaults[..]
    } else {
        sessions
    };
    let mut found = 0;
    for directory in directories {
        for kind in ["wayland-sessions", "xsessions"] {
            let Ok(entries) = std::fs::read_dir(directory.join(kind)) else {
                continue;
            };
            let mut desktop: Vec<PathBuf> = entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension()
                        .is_some_and(|extension| extension == "desktop")
                })
                .collect();
            desktop.truncate(64);
            found += desktop.iter().filter(|path| readable(path)).count();
        }
    }
    found
}

/// Read-only greeter readiness checks, run as the current user.
pub fn check(request: &Request) -> Result<Value, GreeterError> {
    let mut checks = Checks(Vec::new());
    let data = read_toml(&request.config)?;
    let session = data.get("default_session").and_then(toml::Value::as_table);
    let text = |key: &str| {
        session
            .and_then(|table| table.get(key))
            .and_then(toml::Value::as_str)
            .unwrap_or_default()
    };
    let (command, user) = (text("command"), text("user"));
    if command.is_empty() || user.is_empty() {
        return Err(GreeterError::Session);
    }
    let argv = shell::split(command).unwrap_or_default();
    checks.add(
        "host-command",
        argv.first().is_some_and(|program| which(program).is_some()),
        "Configured hosting command executable; arguments were not executed",
    );
    match account_uid(user) {
        Some(uid) => checks.add(
            "greeter-user",
            uid != 0,
            "Configured greeter account exists and is not root",
        ),
        None => checks.add(
            "greeter-user",
            false,
            "Create/use the distro greeter account; do not use a desktop account",
        ),
    }
    let settings = read_toml(&request.settings)?;
    let greeter = settings.get("greeter").and_then(toml::Value::as_table);
    let mut assets = vec![greeter.and_then(|table| table.get("wallpaper")).cloned()];
    if let Some(avatars) = greeter
        .and_then(|table| table.get("avatars"))
        .and_then(toml::Value::as_table)
    {
        assets.extend(avatars.values().cloned().map(Some));
    }
    for (index, asset) in assets.iter().enumerate() {
        let Some(asset) = asset
            .as_ref()
            .filter(|value| crate::pyvalue::truthy(Some(&toml_json(value))))
        else {
            continue;
        };
        let valid = asset
            .as_str()
            .is_some_and(|path| Path::new(path).is_file() && readable(Path::new(path)));
        checks.add(
            &format!("asset.{index}"),
            valid,
            "Configured asset readability checked as current UID",
        );
    }
    let found = session_files(&request.sessions);
    checks.add(
        "sessions",
        found > 0,
        format!("{found} readable desktop session files; Exec/login not tested"),
    );
    checks.add(
        "greetd-pam",
        Path::new("/etc/pam.d/greetd").is_file(),
        "Have a distro administrator review the PAM stack",
    );
    let appearance = settings.get("appearance").and_then(toml::Value::as_table);
    for role in ["font_text", "font_display"] {
        let family = match appearance.and_then(|table| table.get(role)) {
            None => "Noto Sans",
            Some(toml::Value::String(family)) => family.as_str(),
            Some(_) => return Err(GreeterError::Font),
        };
        match probe("fc-match", &["--format=%{family}", family], DEFAULT_TIMEOUT) {
            Ok(matched) if !matched.is_empty() => {
                let shown: String = matched.chars().take(160).collect();
                checks.add(role, true, format!("Resolved: {shown}"));
            }
            Ok(_) => checks.add(role, false, "ok"),
            Err(failure) => checks.add(role, false, failure.state()),
        }
    }
    Ok(
        json!({"schema": 1, "read_only": true, "checked_as_uid": paths::uid(), "checks": checks.0, "limits": LIMITS}),
    )
}

fn toml_json(value: &toml::Value) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

/// The human-readable form of a greeter report.
#[must_use]
pub fn text_report(report: &Value) -> String {
    let mut lines: Vec<String> = report["checks"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|check| {
            format!(
                "{}: {}: {}",
                check["state"].as_str().unwrap_or_default(),
                check["name"].as_str().unwrap_or_default(),
                check["detail"].as_str().unwrap_or_default()
            )
        })
        .collect();
    lines.push(LIMITS.to_owned());
    lines.join("\n")
}

#[must_use]
pub fn healthy(report: &Value) -> bool {
    report["checks"]
        .as_array()
        .into_iter()
        .flatten()
        .all(|check| check["state"] == "ok")
}
