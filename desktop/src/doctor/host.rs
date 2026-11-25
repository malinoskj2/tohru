use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::config::Config;
use crate::probe::{self, DEFAULT_TIMEOUT, ProbeFailure};
use crate::{discord, shortcuts, wayland};

/// Everything the doctor observes about the host, so tests can supply their own.
pub trait Host {
    fn probe(
        &self,
        program: &str,
        args: &[&str],
        timeout: Duration,
    ) -> Result<String, ProbeFailure>;

    fn json_probe(&self, program: &str, args: &[&str]) -> Result<Value, ProbeFailure> {
        let text = self.probe(program, args, DEFAULT_TIMEOUT)?;
        serde_json::from_str(&text).map_err(|_| ProbeFailure::Invalid)
    }

    fn protocols(&self) -> Result<Vec<String>, ProbeFailure>;

    /// The named outputs and the discovery state.
    fn outputs(&self) -> (usize, String);

    /// The shortcut report, given the settings when they loaded.
    fn shortcuts(&self, config: Option<&Config>) -> Map<String, Value>;

    fn which(&self, name: &str) -> bool;

    /// `tohru-discord status`.
    fn discord(&self) -> Value;
}

/// The real host.
pub struct System;

impl Host for System {
    fn probe(
        &self,
        program: &str,
        args: &[&str],
        timeout: Duration,
    ) -> Result<String, ProbeFailure> {
        probe::probe(program, args, timeout)
    }

    fn protocols(&self) -> Result<Vec<String>, ProbeFailure> {
        wayland::protocols()
    }

    fn outputs(&self) -> (usize, String) {
        let (outputs, discovery) = wayland::outputs();
        (outputs.len(), discovery.state())
    }

    fn shortcuts(&self, config: Option<&Config>) -> Map<String, Value> {
        let invalid = || match json!({"state": "config-invalid", "owner": "unknown", "bindings": []})
        {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        config
            .and_then(|config| shortcuts::report(config, shortcuts::Operation::Status).ok())
            .unwrap_or_else(invalid)
    }

    fn which(&self, name: &str) -> bool {
        probe::which(name).is_some()
    }

    fn discord(&self) -> Value {
        discord::status()
    }
}
