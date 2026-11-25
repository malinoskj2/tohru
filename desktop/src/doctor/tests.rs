use std::time::Duration;

use serde_json::{Map, Value, json};

use super::*;
use crate::config::{Config, ConfigError, parse_toml};
use crate::probe::ProbeFailure;

struct Fake {
    found: bool,
    json: fn(&str) -> Result<Value, ProbeFailure>,
}

fn missing(_: &str) -> Result<Value, ProbeFailure> {
    Err(ProbeFailure::Missing)
}

impl Host for Fake {
    fn probe(&self, _: &str, _: &[&str], _: Duration) -> Result<String, ProbeFailure> {
        Err(ProbeFailure::Missing)
    }

    fn json_probe(&self, program: &str, _: &[&str]) -> Result<Value, ProbeFailure> {
        (self.json)(program)
    }

    fn protocols(&self) -> Result<Vec<String>, ProbeFailure> {
        Err(ProbeFailure::Missing)
    }

    fn outputs(&self) -> (usize, String) {
        (0, "missing".to_owned())
    }

    fn shortcuts(&self, _: Option<&Config>) -> Map<String, Value> {
        match json!({"state": "unsupported", "owner": "unknown", "bindings": []}) {
            Value::Object(map) => map,
            _ => unreachable!(),
        }
    }

    fn which(&self, _: &str) -> bool {
        self.found
    }

    fn discord(&self) -> Value {
        json!({"enabled": false, "status": "disabled"})
    }
}

fn config(text: &str) -> Result<Config, ConfigError> {
    Config::from_layers(&[(parse_toml(text)?, false)])
}

fn values(pairs: &[(&str, toml::Value)]) -> Values {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

#[test]
fn missing_tools_are_structured_and_redacted() {
    let host = Fake {
        found: false,
        json: missing,
    };
    let report = run(
        &host,
        config("[assistant.discord]\ntoken_file=\"/secret/do-not-report\"\n"),
    );
    assert_eq!(report.summary.status.exit_code(), 1);
    let text = report.json();
    let data: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(data["read_only"], json!(true));
    assert_eq!(data["schema"], json!(1));
    assert!(!text.contains("do-not-report"));
    assert!(report.checks.iter().any(|check| check.state == "missing"));
    let keys: Vec<&String> = data["checks"][0].as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        ["name", "state", "detail", "action", "required", "severity"]
    );
    let summary: Vec<&String> = data["summary"].as_object().unwrap().keys().collect();
    assert_eq!(
        summary,
        [
            "exit_code",
            "status",
            "required_failures",
            "required_unknown",
            "warnings"
        ]
    );
}

fn failing_wallpaper(program: &str) -> Result<Value, ProbeFailure> {
    if program.ends_with("tohru-wallpaper") {
        Ok(json!({"owner": "tohru", "errors": {"eDP-1": "backend failed"}}))
    } else {
        Err(ProbeFailure::Missing)
    }
}

#[test]
fn a_failed_owned_wallpaper_is_not_healthy() {
    let host = Fake {
        found: true,
        json: failing_wallpaper,
    };
    let report = run(
        &host,
        config("[wallpaper]\nenabled=true\n[assistant]\nvoice=false\n"),
    );
    assert_ne!(report.summary.status, Status::Healthy);
    let check = report
        .checks
        .iter()
        .find(|check| check.name == "wallpaper.owner")
        .unwrap();
    assert_eq!(
        (check.state.as_str(), check.required, check.severity),
        ("backend-errors", true, Severity::Error)
    );
}

#[test]
fn exit_contract_covers_required_optional_unknown_and_invalid() {
    let base = values(&[
        ("assistant.voice", toml::Value::Boolean(false)),
        ("wallpaper.enabled", toml::Value::Boolean(false)),
        ("idle.owner", toml::Value::String("external".into())),
    ]);
    let names = required_checks(&base);
    let report = |overrides: &[(&str, &str)]| {
        let mut checks: Vec<Check> = names
            .iter()
            .map(|name| {
                let state = overrides
                    .iter()
                    .find(|(key, _)| key == name)
                    .map_or("ok", |(_, state)| state);
                Check::new(name.clone(), state, "", "")
            })
            .collect();
        checks.push(Check::new("executable.ffmpeg", "missing", "", ""));
        summarize(&mut checks, &base).status.exit_code()
    };
    assert_eq!(report(&[]), 0);
    assert_eq!(report(&[("executable.flock", "missing")]), 1);
    assert_eq!(report(&[("config", "invalid")]), 2);
    assert_eq!(report(&[("protocol.layer-shell", "unknown")]), 3);
    assert_eq!(report(&[("systemd-user", "missing")]), 1);
    assert!(!names.contains("executable.parec"));
    let enabled = |key: &str| values(&[(key, toml::Value::Boolean(true))]);
    assert!(required_checks(&enabled("assistant.voice")).contains("voice.vad_model"));
    assert!(
        required_checks(&values(&[(
            "idle.owner",
            toml::Value::String("tohru".into())
        )]))
        .contains("service.lock")
    );
    assert!(required_checks(&enabled("wallpaper.enabled")).contains("wallpaper.owner"));
    assert!(!required_checks(&Values::new()).contains("executable.notify-send"));
    let discord = values(&[
        ("assistant.discord.enabled", toml::Value::Boolean(true)),
        ("assistant.discord.post", toml::Value::Boolean(true)),
        (
            "assistant.discord.desktop_first",
            toml::Value::Boolean(true),
        ),
    ]);
    assert!(required_checks(&discord).contains("executable.notify-send"));
}

#[test]
fn malformed_or_unreadable_config_is_exit_two_without_details() {
    let host = Fake {
        found: true,
        json: missing,
    };
    let report = run(&host, config("[broken"));
    assert_eq!(report.summary.status, Status::ConfigurationError);
    assert!(
        report
            .json()
            .contains("\"status\": \"configuration-error\"")
    );
    let report = run(&host, Err(ConfigError::ManagedMissing));
    assert_eq!(report.summary.status.exit_code(), 2);
    assert_eq!(
        report.checks[0].detail,
        "Configuration cannot be read or validated"
    );
}

#[test]
fn text_report_pads_columns_like_before() {
    let mut checks = vec![Check::new(
        "config",
        "ok",
        "XDG native config",
        "Managed values",
    )];
    let summary = summarize(&mut checks, &Values::new());
    let text = Report { summary, checks }.text();
    assert_eq!(
        text,
        "info    ok                       config: XDG native config\n  Managed values\nDoctor: healthy (exit 0)\n"
    );
}
