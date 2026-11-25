pub mod portal;

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::config::{Config, ConfigError, ShortcutsMode};
use crate::paths;
use crate::probe::{DEFAULT_TIMEOUT, json_probe};
use crate::pyvalue::{py_str, truthy};

pub const SOCKET: &str = "shortcuts.sock";

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Operation {
    Status,
    Setup,
    Configure,
}

impl Operation {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Setup => "setup",
            Self::Configure => "configure",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ShortcutError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Session(#[from] paths::SessionError),
    #[error("shortcut owner unreachable")]
    Io(#[from] std::io::Error),
    #[error("shortcut owner operation unavailable")]
    Unavailable,
    #[error("compositor-managed shortcuts are read-only; portal mode is not enabled")]
    ReadOnly,
}

/// Asks the running portal owner over its private socket.
pub fn control(operation: Operation) -> Result<Map<String, Value>, ShortcutError> {
    let endpoint = paths::session_directory()?.join(SOCKET);
    let mut client = UnixStream::connect(endpoint)?;
    client.set_read_timeout(Some(Duration::from_secs(5)))?;
    client.set_write_timeout(Some(Duration::from_secs(5)))?;
    client.write_all(format!("{}\n", operation.name()).as_bytes())?;
    let mut data = Vec::new();
    let mut chunk = [0_u8; 16384];
    while !data.contains(&b'\n') && data.len() < 128 * 1024 {
        let count = client.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        data.extend_from_slice(&chunk[..count]);
    }
    match serde_json::from_slice::<Value>(&data) {
        Ok(Value::Object(result)) if !result.contains_key("error") => Ok(result),
        _ => Err(ShortcutError::Unavailable),
    }
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// The shortcut report the Settings window and doctor show.
pub fn report(config: &Config, operation: Operation) -> Result<Map<String, Value>, ShortcutError> {
    match config.shortcuts_mode {
        ShortcutsMode::Portal => {
            return match control(operation) {
                Ok(result) => Ok(result),
                Err(error) if operation != Operation::Status => Err(error),
                Err(_) => Ok(object(
                    json!({"owner": "Tohru actions via desktop portal", "state": "owner-unavailable",
                    "editable": false, "setup_available": false, "configure_available": false, "bindings": [],
                    "registration": "portal-session"}),
                )),
            };
        }
        _ if operation != Operation::Status => return Err(ShortcutError::ReadOnly),
        ShortcutsMode::Disabled => {
            return Ok(object(
                json!({"owner": "disabled", "state": "disabled", "editable": false, "bindings": []}),
            ));
        }
        ShortcutsMode::Compositor => {}
    }
    let set = |name: &str| std::env::var_os(name).is_some_and(|value| !value.is_empty());
    // Runtime IPC only: a text file parse would not establish active bindings.
    if set("HYPRLAND_INSTANCE_SIGNATURE") && !set("PLEAMAR_GENERIC") {
        return Ok(
            match json_probe("hyprctl", &["-j", "binds"], DEFAULT_TIMEOUT) {
                Ok(Value::Array(bindings)) => hyprland(&bindings),
                Ok(_) => object(
                    json!({"owner": "Hyprland-managed", "state": "ok", "editable": false, "bindings": []}),
                ),
                Err(failure) => object(
                    json!({"owner": "Hyprland-managed", "state": failure.state(), "editable": false, "bindings": []}),
                ),
            },
        );
    }
    if set("SWAYSOCK") {
        // Sway exposes loaded config, not a complete active binding table. Label that precisely.
        let response = json_probe("swaymsg", &["-t", "get_config", "-r"], DEFAULT_TIMEOUT);
        return Ok(object(
            json!({"owner": "Sway-managed", "state": "runtime binding enumeration unsupported",
            "editable": false, "bindings": [], "config_available": matches!(response, Ok(Value::Object(_)))}),
        ));
    }
    Ok(object(
        json!({"owner": "compositor-managed or unavailable", "state": "unsupported", "editable": false, "bindings": []}),
    ))
}

const MODIFIERS: [(i64, &str); 4] = [(1, "Shift"), (4, "Ctrl"), (8, "Alt"), (64, "Super")];
const FLAGS: [&str; 5] = ["release", "repeat", "locked", "longPress", "non_consuming"];

/// Hyprland's runtime bindings as rows, with duplicate shortcuts marked as conflicts.
#[must_use]
pub fn hyprland(bindings: &[Value]) -> Map<String, Value> {
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    for item in bindings.iter().take(512).filter(|item| item.is_object()) {
        let mask = item.get("modmask").map_or(Some(0), Value::as_i64);
        let mut parts: Vec<String> = MODIFIERS
            .iter()
            .filter(|(bit, _)| mask.is_some_and(|mask| mask & bit != 0))
            .map(|(_, name)| (*name).to_owned())
            .collect();
        let key = if truthy(item.get("key")) {
            py_str(item.get("key"))
        } else {
            item.get("keycode")
                .map_or_else(String::new, |code| py_str(Some(code)))
        };
        parts.push(key);
        let text = |name: &str| {
            item.get(name)
                .map_or_else(String::new, |value| py_str(Some(value)))
        };
        let shortcut = parts.join("+");
        let trigger = if truthy(item.get("release")) {
            "release"
        } else {
            "press"
        };
        let submap = text("submap");
        let conflict = !seen.insert((submap.clone(), shortcut.clone(), trigger));
        let flags: Vec<&str> = FLAGS
            .iter()
            .copied()
            .filter(|flag| truthy(item.get(*flag)))
            .collect();
        rows.push(json!({"shortcut": shortcut, "action": text("dispatcher"), "description": text("description"),
            "submap": submap, "arguments": text("arg"), "trigger": trigger, "flags": flags.join(", "), "conflict": conflict}));
    }
    let suffix = if bindings.len() > 512 {
        " (first 512 entries)"
    } else {
        ""
    };
    object(
        json!({"owner": "Hyprland-managed", "state": format!("runtime IPC snapshot{suffix}"), "editable": false, "bindings": rows}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyprland_rows_name_modifiers_and_mark_conflicts() {
        let binding = json!({"modmask": 64, "key": "SPACE", "dispatcher": "exec", "arg": "tohru launcher toggle"});
        let report = hyprland(&[
            binding.clone(),
            binding,
            json!({"modmask": 5, "key": "", "keycode": 38, "release": true}),
        ]);
        assert_eq!(report["owner"], "Hyprland-managed");
        assert_eq!(report["editable"], false);
        let rows = report["bindings"].as_array().unwrap();
        assert_eq!(rows[0]["shortcut"], "Super+SPACE");
        assert_eq!(rows[0]["conflict"], false);
        assert_eq!(rows[1]["conflict"], true);
        assert_eq!(rows[2]["shortcut"], "Shift+Ctrl+38");
        assert_eq!(
            (rows[2]["trigger"].as_str(), rows[2]["flags"].as_str()),
            (Some("release"), Some("release"))
        );
    }
}
