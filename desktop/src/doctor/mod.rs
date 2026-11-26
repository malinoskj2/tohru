mod checks;
mod host;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use serde_json::{Value, json};

use crate::pyjson;

pub use checks::run;
pub use host::{Host, System};

/// Flattened settings, as `config.rs` produces them.
pub type Values = BTreeMap<String, toml::Value>;

const HEALTHY: [&str; 5] = [
    "ok",
    "configured",
    "present",
    "present-unverified",
    "observed",
];
const UNKNOWN: [&str; 3] = ["unknown", "incompatible-or-unknown", "missing-or-unknown"];
const INFORMATIONAL: [&str; 6] = [
    "disabled",
    "optional-unset",
    "default",
    "selected",
    "loaded",
    "absent",
];

/// Python truthiness of a TOML value.
pub(crate) fn truthy(value: Option<&toml::Value>) -> bool {
    match value {
        None => false,
        Some(toml::Value::Boolean(flag)) => *flag,
        Some(toml::Value::Integer(number)) => *number != 0,
        Some(toml::Value::Float(number)) => *number != 0.0,
        Some(toml::Value::String(text)) => !text.is_empty(),
        Some(toml::Value::Array(items)) => !items.is_empty(),
        Some(toml::Value::Table(table)) => !table.is_empty(),
        Some(toml::Value::Datetime(_)) => true,
    }
}

pub(crate) fn text<'a>(values: &'a Values, key: &str) -> Option<&'a str> {
    values.get(key).and_then(toml::Value::as_str)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    fn name(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// One diagnostic; `required` and `severity` are set by [`summarize`].
#[derive(Clone, Debug)]
pub struct Check {
    pub name: String,
    pub state: String,
    pub detail: String,
    pub action: String,
    pub required: bool,
    pub severity: Severity,
}

impl Check {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        state: impl Into<String>,
        detail: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            state: state.into(),
            detail: detail.into(),
            action: action.into(),
            required: false,
            severity: Severity::Info,
        }
    }

    fn json(&self) -> Value {
        json!({"name": self.name, "state": self.state, "detail": self.detail, "action": self.action,
               "required": self.required, "severity": self.severity.name()})
    }
}

/// The checks that must be healthy for the settings in force.
#[must_use]
pub fn required_checks(values: &Values) -> BTreeSet<String> {
    let mut core: BTreeSet<String> = [
        "config",
        "runtime",
        "systemd-user",
        "logind",
        "protocol.layer-shell",
        "environment.WAYLAND_DISPLAY",
        "environment.XDG_RUNTIME_DIR",
        "environment.DBUS_SESSION_BUS_ADDRESS",
        "environment.XDG_SESSION_ID",
        "environment.runtime-directory",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let mut add = |names: &[&str]| core.extend(names.iter().map(|name| (*name).to_owned()));
    let executables = |names: &[&str]| {
        names
            .iter()
            .map(|name| format!("executable.{name}"))
            .collect::<Vec<_>>()
    };
    let base = executables(&[
        "bash",
        "sh",
        "readlink",
        "dirname",
        "sha256sum",
        "cut",
        "rm",
        "setsid",
        "flock",
        "systemctl",
        "loginctl",
        "busctl",
        "timeout",
        "systemd-notify",
    ]);
    add(&base.iter().map(String::as_str).collect::<Vec<_>>());
    let idle_tohru = text(values, "idle.owner") == Some("tohru");
    if text(values, "shortcuts.mode") == Some("portal") {
        add(&[
            "shortcuts.dependencies",
            "shortcuts.portal",
            "shortcuts.registration",
        ]);
    }
    if truthy(values.get("wallpaper.enabled")) {
        add(&[
            "wallpaper",
            "wallpaper.owner",
            "outputs",
            "executable.wayland-info",
        ]);
    }
    if idle_tohru {
        add(&[
            "executable.swayidle",
            "protocol.idle-notify",
            "protocol.session-lock",
            "locker.pam",
            "service.lock",
        ]);
    }
    if truthy(values.get("assistant.afk_enabled")) {
        add(&["assistant.afk", "service.afk", "protocol.idle-notify"]);
        if idle_tohru || text(values, "assistant.afk_idle_source") == Some("passive") {
            add(&["executable.swayidle"]);
        }
    }
    if truthy(values.get("assistant.voice")) {
        add(&["voice.dependencies", "voice.vad_model", "voice.asr_model"]);
        add(&[
            "executable.parec",
            "executable.pacat",
            "executable.pactl",
            "executable.espeak-ng",
        ]);
        if text(values, "assistant.voice_wake") == Some("kws") {
            add(&["voice.kws_model"]);
        }
    }
    let discord = truthy(values.get("assistant.discord.enabled"));
    if discord
        && truthy(values.get("assistant.discord.post"))
        && truthy(values.get("assistant.discord.desktop_first"))
    {
        add(&["executable.notify-send"]);
    }
    if discord {
        add(&["discord.configuration"]);
        if truthy(values.get("assistant.discord.receive")) {
            add(&["discord.receiver"]);
        }
    }
    core
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Healthy,
    RequiredFailures,
    ConfigurationError,
    RequiredUnknown,
}

impl Status {
    #[must_use]
    pub fn exit_code(self) -> u8 {
        match self {
            Self::Healthy => 0,
            Self::RequiredFailures => 1,
            Self::ConfigurationError => 2,
            Self::RequiredUnknown => 3,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::RequiredFailures => "required-failures",
            Self::ConfigurationError => "configuration-error",
            Self::RequiredUnknown => "required-unknown",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Summary {
    pub status: Status,
    pub required_failures: Vec<String>,
    pub required_unknown: Vec<String>,
    pub warnings: Vec<String>,
}

impl Summary {
    fn json(&self) -> Value {
        json!({"exit_code": self.status.exit_code(), "status": self.status.name(),
               "required_failures": self.required_failures, "required_unknown": self.required_unknown,
               "warnings": self.warnings})
    }
}

/// Marks each check required or not and grades it, then derives the exit status.
pub fn summarize(checks: &mut [Check], values: &Values) -> Summary {
    let required = required_checks(values);
    let (mut failures, mut unobserved, mut warnings) = (Vec::new(), Vec::new(), Vec::new());
    for check in checks.iter_mut() {
        check.required = required.contains(&check.name);
        let healthy = HEALTHY.contains(&check.state.as_str());
        if check.required && !healthy {
            let unknown = UNKNOWN.contains(&check.state.as_str());
            if unknown {
                unobserved.push(check.name.clone());
                check.severity = Severity::Warning;
            } else {
                failures.push(check.name.clone());
                check.severity = Severity::Error;
            }
        } else if healthy || INFORMATIONAL.contains(&check.state.as_str()) {
            check.severity = Severity::Info;
        } else {
            check.severity = Severity::Warning;
            warnings.push(check.name.clone());
        }
    }
    let status = if failures.iter().any(|name| name == "config") {
        Status::ConfigurationError
    } else if !failures.is_empty() {
        Status::RequiredFailures
    } else if !unobserved.is_empty() {
        Status::RequiredUnknown
    } else {
        Status::Healthy
    };
    Summary {
        status,
        required_failures: failures,
        required_unknown: unobserved,
        warnings,
    }
}

/// The finished report.
#[derive(Clone, Debug)]
pub struct Report {
    pub summary: Summary,
    pub checks: Vec<Check>,
}

impl Report {
    #[must_use]
    pub fn json(&self) -> String {
        let checks: Vec<Value> = self.checks.iter().map(Check::json).collect();
        pyjson::dumps(
            &json!({"schema": 1, "read_only": true, "summary": self.summary.json(), "checks": checks}),
        )
    }

    #[must_use]
    pub fn text(&self) -> String {
        let mut out = String::new();
        for check in &self.checks {
            let _ = writeln!(
                out,
                "{:<7} {:<24} {}: {}",
                check.severity.name(),
                check.state,
                check.name,
                check.detail
            );
            if !check.action.is_empty() {
                let _ = writeln!(out, "  {}", check.action);
            }
        }
        let status = self.summary.status;
        let _ = writeln!(
            out,
            "Doctor: {} (exit {})",
            status.name(),
            status.exit_code()
        );
        out
    }
}

#[cfg(test)]
mod tests;
