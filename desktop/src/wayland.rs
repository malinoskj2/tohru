use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

use crate::probe::{DEFAULT_TIMEOUT, ProbeFailure, probe};

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("fixed pattern is valid")
}

static INTERFACE: LazyLock<Regex> = LazyLock::new(|| regex(r"interface: '([^']+)'"));
static OUTPUT_HEADER: LazyLock<Regex> =
    LazyLock::new(|| regex(r"^interface: 'wl_output',[^\n]*name:\s*(\d+)"));
static NAME: LazyLock<Regex> = LazyLock::new(|| regex(r"(?m)^\s*name:\s*(.*?)\s*$"));
static SCALE: LazyLock<Regex> = LazyLock::new(|| regex(r"\bscale:\s*(\d+)"));
static TRANSFORM: LazyLock<Regex> = LazyLock::new(|| regex(r"output_transform:\s*([^,\n]+)"));
static MODE: LazyLock<Regex> = LazyLock::new(|| {
    regex(r"width:\s*(\d+) px, height:\s*(\d+) px,[^\n]*\n\s*flags:[^\n]*current")
});
static XDG_SPLIT: LazyLock<Regex> = LazyLock::new(|| regex(r"\bxdg_output_v1\s*\n"));
static XDG_OUTPUT: LazyLock<Regex> = LazyLock::new(|| regex(r"output:\s*(\d+)"));
static XDG_NAME: LazyLock<Regex> = LazyLock::new(|| regex(r"name: '([^']+)'"));
static LOGICAL: LazyLock<Regex> =
    LazyLock::new(|| regex(r"logical_width:\s*(\d+), logical_height:\s*(\d+)"));

/// The interfaces `wayland-info` reports as advertised.
pub fn protocols() -> Result<Vec<String>, ProbeFailure> {
    let report = probe("wayland-info", &[] as &[&str], DEFAULT_TIMEOUT)?;
    let mut names: Vec<String> = INTERFACE
        .captures_iter(&report)
        .map(|capture| capture[1].to_owned())
        .collect();
    names.sort();
    names.dedup();
    Ok(names)
}

/// Splits a report before every `interface: '` line, as the reports' blocks begin.
fn blocks(report: &str) -> Vec<&str> {
    let mut starts: Vec<usize> = report
        .match_indices("interface: '")
        .map(|(at, _)| at)
        .collect();
    if starts.first() != Some(&0) {
        starts.insert(0, 0);
    }
    starts
        .iter()
        .enumerate()
        .map(|(index, start)| {
            &report[*start..starts.get(index + 1).copied().unwrap_or(report.len())]
        })
        .collect()
}

fn number(value: f64) -> Value {
    if value.fract() == 0.0 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "output sizes are small integers"
        )]
        return Value::from(value as i64);
    }
    serde_json::json!(value)
}

/// Named outputs from a wayland-utils 1.1 `wayland-info` report (not workspace IPC), sorted by name.
#[must_use]
pub fn parse_outputs(report: &str) -> Vec<Map<String, Value>> {
    let blocks = blocks(report);
    let mut records: BTreeMap<u64, Map<String, Value>> = BTreeMap::new();
    for block in &blocks {
        let Some(header) = OUTPUT_HEADER.captures(block) else {
            continue;
        };
        let Ok(id) = header[1].parse::<u64>() else {
            continue;
        };
        let mut item = Map::new();
        if let Some(name) = NAME.captures(block) {
            item.insert("name".into(), Value::String(name[1].to_owned()));
        }
        let scale = SCALE
            .captures(block)
            .and_then(|capture| capture[1].parse::<u64>().ok())
            .map_or(1, |scale| scale.max(1));
        item.insert("scale".into(), Value::from(scale));
        let transform = TRANSFORM
            .captures(block)
            .map(|capture| capture[1].to_owned());
        if let Some(transform) = &transform {
            item.insert("transform".into(), Value::String(transform.clone()));
        }
        if let Some(mode) = MODE.captures(block) {
            let (mut width, mut height) = (
                mode[1].parse::<f64>().unwrap_or(0.0),
                mode[2].parse::<f64>().unwrap_or(0.0),
            );
            if matches!(
                transform.as_deref(),
                Some("90°" | "270°" | "flipped 90°" | "flipped 270°")
            ) {
                std::mem::swap(&mut width, &mut height);
            }
            #[expect(
                clippy::cast_precision_loss,
                reason = "output scales are small integers"
            )]
            let scale = scale as f64;
            item.insert("width".into(), serde_json::json!(width / scale));
            item.insert("height".into(), serde_json::json!(height / scale));
        }
        records.insert(id, item);
    }
    for block in &blocks {
        if !block.starts_with("interface: 'zxdg_output_manager_v1'") {
            continue;
        }
        for output in XDG_SPLIT.split(block).skip(1) {
            let Some(id) = XDG_OUTPUT
                .captures(output)
                .and_then(|capture| capture[1].parse::<u64>().ok())
            else {
                continue;
            };
            let Some(item) = records.get_mut(&id) else {
                continue;
            };
            if let Some(name) = XDG_NAME.captures(output) {
                item.entry("name")
                    .or_insert_with(|| Value::String(name[1].to_owned()));
            }
            if let Some(size) = LOGICAL.captures(output) {
                let width = size[1].parse::<f64>().unwrap_or(0.0);
                let height = size[2].parse::<f64>().unwrap_or(0.0);
                item.insert("width".into(), number(width));
                item.insert("height".into(), number(height));
            }
        }
    }
    let mut named: Vec<_> = records
        .into_values()
        .filter(|item| item.contains_key("name"))
        .collect();
    named.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    named
}

/// The discovery state reported beside the outputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Discovery {
    Ok,
    UnsupportedNames,
    Failed(ProbeFailure),
}

impl Discovery {
    #[must_use]
    pub fn state(&self) -> String {
        match self {
            Self::Ok => "ok".to_owned(),
            Self::UnsupportedNames => "unsupported-output-names".to_owned(),
            Self::Failed(failure) => failure.state(),
        }
    }
}

/// Probes `wayland-info` for the named outputs.
#[must_use]
pub fn outputs() -> (Vec<Map<String, Value>>, Discovery) {
    match probe("wayland-info", &[] as &[&str], DEFAULT_TIMEOUT) {
        Ok(report) => {
            let result = parse_outputs(&report);
            let state = if !result.is_empty() || !report.contains("interface: 'wl_output'") {
                Discovery::Ok
            } else {
                Discovery::UnsupportedNames
            };
            (result, state)
        }
        Err(failure) => (Vec::new(), Discovery::Failed(failure)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn output_discovery_without_workspaces() {
        let report = "interface: 'wl_output', version: 4, name: 8\n\tname: HDMI-A-1\n\tscale: 2\ninterface: 'wl_output', version: 4, name: 9\n\tname: eDP-1\n";
        let outputs: Vec<Value> = parse_outputs(report)
            .into_iter()
            .map(Value::Object)
            .collect();
        assert_eq!(
            outputs,
            [
                json!({"name": "HDMI-A-1", "scale": 2}),
                json!({"name": "eDP-1", "scale": 1})
            ]
        );
    }

    #[test]
    fn output_names_from_xdg_output_and_rotated_logical_size() {
        let report = "interface: 'wl_output', version: 3, name: 8
 x: 0, y: 0, scale: 2,
 output_transform: 90°,
 mode:
  width: 3840 px, height: 2160 px, refresh: 60.000 Hz,
  flags: current preferred
interface: 'zxdg_output_manager_v1', version: 3, name: 9
 xdg_output_v1
  output: 8
  name: 'HDMI-A-1'
  logical_width: 1440, logical_height: 2560
";
        let outputs: Vec<Value> = parse_outputs(report)
            .into_iter()
            .map(Value::Object)
            .collect();
        assert_eq!(
            outputs,
            [
                json!({"name": "HDMI-A-1", "scale": 2, "transform": "90°", "width": 1440, "height": 2560})
            ]
        );
    }

    #[test]
    fn mode_size_without_xdg_output_is_logical_and_rotated() {
        let report = "interface: 'wl_output', version: 3, name: 4\n name: DP-2\n scale: 2,\n output_transform: 270°,\n mode:\n  width: 3840 px, height: 2160 px, refresh: 60 Hz,\n  flags: current\n";
        let outputs = parse_outputs(report);
        assert_eq!(outputs[0]["width"], json!(1080.0));
        assert_eq!(outputs[0]["height"], json!(1920.0));
    }
}
