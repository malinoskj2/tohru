use std::io::{Read, Write};
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::pyvalue::{field, same, truthy};

const RESPONSE_LIMIT: u64 = 262_144;
const NAMESPACE: &str = "tohru-dragon-flight";

/// A compositor reply that lacks what the geometry needs; Hyprland is then ignored.
#[derive(Debug, thiserror::Error)]
#[error("malformed compositor data")]
pub struct Malformed;

fn query(path: &Path, command: &str) -> Result<Value, Malformed> {
    let timeout = Some(Duration::from_millis(180));
    let mut client = UnixStream::connect(path).map_err(|_| Malformed)?;
    client.set_read_timeout(timeout).map_err(|_| Malformed)?;
    client.set_write_timeout(timeout).map_err(|_| Malformed)?;
    client
        .write_all(format!("j/{command}").as_bytes())
        .map_err(|_| Malformed)?;
    let mut raw = Vec::new();
    client
        .take(RESPONSE_LIMIT + 1)
        .read_to_end(&mut raw)
        .map_err(|_| Malformed)?;
    if raw.len() as u64 > RESPONSE_LIMIT {
        return Err(Malformed);
    }
    serde_json::from_slice(&raw).map_err(|_| Malformed)
}

fn number(value: Option<&Value>) -> Result<f64, Malformed> {
    value.and_then(Value::as_f64).ok_or(Malformed)
}

fn pair(value: Option<&Value>) -> Result<(f64, f64), Malformed> {
    match value.and_then(Value::as_array).map(Vec::as_slice) {
        Some([first, second]) => Ok((number(Some(first))?, number(Some(second))?)),
        _ => Err(Malformed),
    }
}

fn workspace_id(value: &Value) -> Option<&Value> {
    field(value.get("workspace"), "id")
}

/// The output's corner where the landed actor would sit is free of other windows.
///
/// Hyprland supplies no general stacking order, so any overlap keeps the overlay level.
fn clear_landing(
    clients: &[Value],
    target: &Value,
    right: f64,
    bottom: f64,
    workspace: &Value,
) -> Result<bool, Malformed> {
    if clients.len() > 256 {
        return Ok(false);
    }
    for client in clients {
        if !truthy(client.get("mapped")) || truthy(client.get("hidden")) {
            continue;
        }
        if !same(workspace_id(client), Some(workspace)) && !truthy(client.get("pinned")) {
            continue;
        }
        if same(client.get("address"), Some(target)) {
            continue;
        }
        let (x, y) = pair(client.get("at"))?;
        let (w, h) = pair(client.get("size"))?;
        if x < right && x + w > right - 228.0 && y < bottom && y + h > bottom - 248.0 {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The one output showing this process's flight layer surface.
fn flight_output(layers: &Value, owner_pid: u32) -> Result<Option<String>, Malformed> {
    let outputs = layers.as_object().ok_or(Malformed)?;
    let mut matches = Vec::new();
    for (name, output) in outputs {
        let levels = output
            .get("levels")
            .and_then(Value::as_object)
            .ok_or(Malformed)?;
        for surfaces in levels.values() {
            for surface in surfaces.as_array().ok_or(Malformed)? {
                if surface.get("namespace").and_then(Value::as_str) == Some(NAMESPACE)
                    && same(surface.get("pid"), Some(&Value::from(owner_pid)))
                {
                    matches.push(name.clone());
                }
            }
        }
    }
    Ok(if matches.len() == 1 {
        matches.pop()
    } else {
        None
    })
}

fn difference(left: Option<&Value>, right: Option<&Value>) -> Result<Value, Malformed> {
    if let (Some(a), Some(b)) = (left.and_then(Value::as_i64), right.and_then(Value::as_i64)) {
        return Ok(Value::from(a - b));
    }
    Ok(json!(number(left)? - number(right)?))
}

/// The geometry the actor needs for one output.
fn snapshot(
    monitors: &Value,
    window: &Value,
    clients: &Value,
    output: Option<&str>,
) -> Result<Map<String, Value>, Malformed> {
    let monitors = monitors.as_array().ok_or(Malformed)?;
    let mut selected = Vec::new();
    for monitor in monitors {
        if monitor.get("name").ok_or(Malformed)?.as_str() == output && output.is_some() {
            selected.push(monitor);
        }
    }
    let [monitor] = selected.as_slice() else {
        return Ok(Map::new());
    };
    let scale = number(monitor.get("scale"))?;
    let rotation = monitor.get("transform").ok_or(Malformed)?;
    let rotation_index = rotation
        .as_f64()
        .filter(|value| value.fract() == 0.0 && (0.0..8.0).contains(value));
    let Some(rotation_index) = rotation_index.filter(|_| (0.25..=8.0).contains(&scale)) else {
        return Ok(Map::new());
    };
    let (mut width, mut height) = (
        number(monitor.get("width"))?,
        number(monitor.get("height"))?,
    );
    if rotation_index % 2.0 > 0.5 {
        std::mem::swap(&mut width, &mut height);
    }
    let workspace = field(monitor.get("activeWorkspace"), "id").ok_or(Malformed)?;
    let id = monitor.get("id").ok_or(Malformed)?;
    let clients = clients.as_array().ok_or(Malformed)?;
    let fullscreen = clients.iter().any(|client| {
        truthy(client.get("mapped"))
            && !truthy(client.get("hidden"))
            && same(client.get("monitor"), Some(id))
            && (same(workspace_id(client), Some(workspace)) || truthy(client.get("pinned")))
            && truthy(client.get("fullscreen"))
    });
    let special = monitor.get("specialWorkspace");
    let solitary = monitor.get("solitary");
    let solitary_set = !matches!(solitary, None | Some(Value::Null))
        && !same(solitary, Some(&Value::from(0)))
        && !matches!(solitary.and_then(Value::as_str), Some("0" | "0x0"));
    let blocked = truthy(field(special, "name"))
        || truthy(field(special, "id"))
        || solitary_set
        || (same(window.get("monitor"), Some(id)) && truthy(window.get("fullscreen")))
        || fullscreen;
    let (ox, oy) = (
        monitor.get("x").ok_or(Malformed)?,
        monitor.get("y").ok_or(Malformed)?,
    );
    #[expect(
        clippy::cast_possible_truncation,
        reason = "output sizes are far inside i64"
    )]
    let logical = |size: f64| Value::from((size / scale).round_ties_even() as i64);
    let mut out = Map::new();
    out.insert("output".into(), monitor["name"].clone());
    out.insert("output_id".into(), id.clone());
    out.insert("ox".into(), ox.clone());
    out.insert("oy".into(), oy.clone());
    out.insert("w".into(), logical(width));
    out.insert("h".into(), logical(height));
    out.insert("scale".into(), monitor["scale"].clone());
    out.insert("rotation".into(), rotation.clone());
    out.insert("workspace".into(), workspace.clone());
    out.insert("fullscreen".into(), Value::Bool(fullscreen));
    out.insert("blocked".into(), Value::Bool(blocked));
    let landing = truthy(window.get("mapped"))
        && !window
            .get("hidden")
            .is_none_or(|hidden| truthy(Some(hidden)))
        && truthy(window.get("address"))
        && same(window.get("monitor"), Some(id))
        && same(workspace_id(window), Some(workspace));
    if landing {
        let right = number(Some(ox))? + (width / scale).round_ties_even();
        let bottom = number(Some(oy))? + (height / scale).round_ties_even();
        if clear_landing(clients, &window["address"], right, bottom, workspace)? {
            let (x, y) = pair(window.get("at"))?;
            let (w, h) = pair(window.get("size"))?;
            if [x, y, w, h]
                .iter()
                .all(|value| value.is_finite() && value.abs() <= 65536.0)
            {
                let at = window["at"].as_array().ok_or(Malformed)?;
                let size = window["size"].as_array().ok_or(Malformed)?;
                out.insert(
                    "target".into(),
                    json!({"id": window["address"], "x": difference(at.first(), Some(ox))?,
                           "y": difference(at.get(1), Some(oy))?, "w": size[0], "h": size[1]}),
                );
            }
        }
    }
    Ok(out)
}

fn session_state(session: Option<&str>) -> Map<String, Value> {
    let mut result = Map::new();
    let Some(session) =
        session.filter(|id| !id.is_empty() && id.chars().all(char::is_alphanumeric))
    else {
        return result;
    };
    let Ok(reply) = crate::probe::probe(
        "loginctl",
        &["show-session", session, "-p", "Active", "-p", "LockedHint"],
        Duration::from_millis(300),
    ) else {
        return result;
    };
    for line in reply.lines() {
        match line.split_once('=') {
            Some(("Active", value @ ("yes" | "no"))) => {
                result.insert("session_active".into(), Value::Bool(value == "yes"));
            }
            Some(("LockedHint", value @ ("yes" | "no"))) => {
                result.insert("unlocked".into(), Value::Bool(value == "no"));
            }
            _ => {}
        }
    }
    result
}

/// Whether the Hyprland instance named in the environment serves this Wayland display.
fn hyprland_socket(runtime: &Path) -> Option<PathBuf> {
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").unwrap_or_default();
    if signature.is_empty()
        || signature.contains('/')
        || std::env::var("PLEAMAR_GENERIC").as_deref() == Ok("1")
    {
        return None;
    }
    let directory = runtime.join("hypr").join(&signature);
    let mut lock = Vec::new();
    std::fs::File::open(directory.join("hyprland.lock"))
        .ok()?
        .take(1024)
        .read_to_end(&mut lock)
        .ok()?;
    let lock = String::from_utf8(lock).ok()?;
    let display = lock.lines().nth(1)?;
    let ours = std::env::var("WAYLAND_DISPLAY").ok()?;
    let a = std::fs::metadata(runtime.join(display)).ok()?;
    let b = std::fs::metadata(runtime.join(ours)).ok()?;
    (a.dev() == b.dev() && a.ino() == b.ino()).then(|| directory.join(".socket.sock"))
}

fn geometry(path: &Path, owner_pid: u32, first: bool) -> Result<Map<String, Value>, Malformed> {
    let mut output = flight_output(&query(path, "layers")?, owner_pid)?;
    if output.is_none() && first {
        // The first visible configure can race the geometry helper.
        for _ in 0..3 {
            std::thread::sleep(Duration::from_millis(35));
            output = flight_output(&query(path, "layers")?, owner_pid)?;
            if output.is_some() {
                break;
            }
        }
    }
    snapshot(
        &query(path, "monitors")?,
        &query(path, "activewindow")?,
        &query(path, "clients")?,
        output.as_deref(),
    )
}

/// Streams the focused geometry as changed JSON lines through one entrance. No titles or captures.
pub fn run(actor: bool) -> anyhow::Result<()> {
    let runtime = PathBuf::from(
        std::env::var_os("XDG_RUNTIME_DIR")
            .ok_or_else(|| anyhow::anyhow!("XDG_RUNTIME_DIR is unset"))?,
    );
    // The native lock authority creates this endpoint only during its lock cycle.
    // For decorative motion, even a stale endpoint is reason to stay still.
    let lock = crate::paths::lock_status_endpoint(&runtime);
    let mut hypr = hyprland_socket(&runtime);
    // Pleamar spawns this executable directly; its PID tells this character from another scene.
    let owner_pid = std::os::unix::process::parent_id();
    let art = crate::paths::shell_candidates("assistant/dragon")
        .into_iter()
        .find(|path| path.join("body.webp").is_file())
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let session = std::env::var("XDG_SESSION_ID").ok();
    let mut previous: Option<Map<String, Value>> = None;
    let mut status = Map::new();
    let mut stdout = std::io::stdout().lock();
    for index in 0..(if actor { 336 } else { 64 }) {
        let mut scene = Map::new();
        if let Some(path) = &hypr {
            match geometry(path, owner_pid, previous.is_none()) {
                Ok(found) => scene = found,
                Err(Malformed) => hypr = None,
            }
        }
        if actor {
            if index % 6 == 0 {
                status = session_state(session.as_deref());
            }
            scene.extend(status.clone());
            scene.insert("art".into(), Value::String(art.clone()));
            if scene.get("unlocked") == Some(&Value::Bool(false))
                || scene.get("session_active") == Some(&Value::Bool(false))
            {
                scene.insert("blocked".into(), Value::Bool(true));
            }
        }
        if lock.exists() {
            if actor {
                scene.insert("unlocked".into(), Value::Bool(false));
            }
            scene.insert("blocked".into(), Value::Bool(true));
        }
        if previous.as_ref() != Some(&scene) {
            let line = crate::pyjson::render(
                &Value::Object(scene.clone()),
                crate::pyjson::Style {
                    ascii: true,
                    sort_keys: false,
                    compact: true,
                },
            );
            writeln!(stdout, "{line}")?;
            stdout.flush()?;
            previous = Some(scene);
        }
        std::thread::sleep(Duration::from_millis(180));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor() -> Value {
        json!({"name": "DP-1", "id": 0, "x": 0, "y": 0, "width": 2560, "height": 1440, "scale": 1.25,
               "transform": 0, "activeWorkspace": {"id": 1}, "specialWorkspace": {"id": 0, "name": ""}, "solitary": "0"})
    }

    fn window() -> Value {
        json!({"mapped": true, "hidden": false, "address": "0xabc", "monitor": 0, "workspace": {"id": 1},
               "at": [100, 120], "size": [800, 600], "fullscreen": 0})
    }

    #[test]
    fn a_focused_window_with_a_clear_corner_becomes_the_landing_target() {
        let scene = snapshot(
            &json!([monitor()]),
            &window(),
            &json!([window()]),
            Some("DP-1"),
        )
        .unwrap();
        assert_eq!(scene["w"], 2048);
        assert_eq!(scene["h"], 1152);
        assert_eq!(scene["blocked"], false);
        assert_eq!(
            scene["target"],
            json!({"id": "0xabc", "x": 100, "y": 120, "w": 800, "h": 600})
        );
    }

    #[test]
    fn fullscreen_special_workspaces_and_crowded_corners_block_or_drop_the_target() {
        let mut full = window();
        full["fullscreen"] = json!(2);
        let scene = snapshot(&json!([monitor()]), &full, &json!([full]), Some("DP-1")).unwrap();
        assert_eq!(scene["blocked"], true);
        assert_eq!(scene["fullscreen"], true);

        let mut special = monitor();
        special["specialWorkspace"] = json!({"id": -98, "name": "special:term"});
        let scene = snapshot(&json!([special]), &window(), &json!([]), Some("DP-1")).unwrap();
        assert_eq!(scene["blocked"], true);

        let corner = json!({"mapped": true, "hidden": false, "address": "0xdef", "workspace": {"id": 1},
                            "at": [1900, 1000], "size": [200, 200]});
        let scene = snapshot(
            &json!([monitor()]),
            &window(),
            &json!([window(), corner]),
            Some("DP-1"),
        )
        .unwrap();
        assert!(scene.get("target").is_none());
    }

    #[test]
    fn only_one_owned_flight_surface_selects_an_output() {
        let layers = json!({"DP-1": {"levels": {"3": [{"namespace": NAMESPACE, "pid": 42}]}},
                            "HDMI-A-1": {"levels": {"3": [{"namespace": NAMESPACE, "pid": 7}]}}});
        assert_eq!(flight_output(&layers, 42).unwrap().as_deref(), Some("DP-1"));
        assert_eq!(flight_output(&layers, 9).unwrap(), None);
        assert!(flight_output(&json!({"DP-1": {}}), 42).is_err());
        assert!(
            snapshot(&json!([monitor()]), &window(), &json!([]), None)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn rotated_outputs_swap_logical_size_and_bad_scales_are_ignored() {
        let mut rotated = monitor();
        rotated["transform"] = json!(1);
        let scene = snapshot(&json!([rotated]), &json!({}), &json!([]), Some("DP-1")).unwrap();
        assert_eq!(
            (scene["w"].clone(), scene["h"].clone()),
            (json!(1152), json!(2048))
        );
        let mut tiny = monitor();
        tiny["scale"] = json!(0.1);
        assert!(
            snapshot(&json!([tiny]), &json!({}), &json!([]), Some("DP-1"))
                .unwrap()
                .is_empty()
        );
    }
}

#[cfg(test)]
mod rotated_output_tests {
    use super::*;

    fn monitor() -> Value {
        json!({"id": 7, "name": "DP-4", "x": -900, "y": 220, "width": 1920, "height": 1080, "scale": 1.5,
               "transform": 1, "activeWorkspace": {"id": 3}, "solitary": "0"})
    }

    fn landscape() -> Value {
        let mut value = monitor();
        value["id"] = json!(9);
        value["name"] = json!("DP-1");
        value["transform"] = json!(0);
        value["x"] = json!(0);
        value
    }

    fn window() -> Value {
        json!({"address": "0xab", "workspace": {"id": 3}, "mapped": true, "hidden": false, "monitor": 7,
               "at": [-780, 380], "size": [420, 610]})
    }

    fn with(base: Value, change: &Value) -> Value {
        let mut value = base;
        for (key, item) in change.as_object().unwrap() {
            value[key] = item.clone();
        }
        value
    }

    #[test]
    fn rotated_fractional_output_global_origin_identity_and_visibility() {
        let actor = json!({"pid": 211, "namespace": NAMESPACE});
        let layers = |surfaces: Value| json!({"levels": {"1": surfaces, "3": []}});
        let mut foreign = actor.clone();
        foreign["pid"] = json!(12);
        let both = json!({"DP-4": layers(json!([actor])), "DP-1": layers(json!([foreign]))});
        assert_eq!(flight_output(&both, 211).unwrap().as_deref(), Some("DP-4"));
        let twice = json!({"DP-4": layers(json!([actor])), "DP-1": layers(json!([actor]))});
        assert_eq!(flight_output(&twice, 211).unwrap(), None);

        let twin = with(monitor(), &json!({"id": 9, "name": "DP-1", "x": 0}));
        let scene = snapshot(
            &json!([monitor(), twin]),
            &window(),
            &json!([window()]),
            Some("DP-1"),
        )
        .unwrap();
        assert!(scene.get("target").is_none());
        assert!(
            snapshot(&json!([twin]), &window(), &json!([window()]), Some("DP-4"))
                .unwrap()
                .is_empty()
        );
        let full = with(window(), &json!({"fullscreen": 2}));
        assert_eq!(
            snapshot(&json!([monitor(), twin]), &full, &json!([]), Some("DP-1")).unwrap()["blocked"],
            false
        );

        let scene = snapshot(
            &json!([landscape(), monitor()]),
            &window(),
            &json!([window()]),
            Some("DP-4"),
        )
        .unwrap();
        assert_eq!(
            (
                scene["w"].clone(),
                scene["h"].clone(),
                scene["blocked"].clone()
            ),
            (json!(720), json!(1280), json!(false))
        );
        assert_eq!(
            scene["target"],
            json!({"id": "0xab", "x": 120, "y": 160, "w": 420, "h": 610})
        );
        assert!(
            snapshot(
                &json!([monitor(), monitor()]),
                &window(),
                &json!([window()]),
                Some("DP-4")
            )
            .unwrap()
            .is_empty()
        );
        for change in [
            json!({"hidden": true}),
            json!({"mapped": false}),
            json!({"workspace": {"id": 2}}),
            json!({"monitor": 3}),
        ] {
            let scene = snapshot(
                &json!([landscape(), monitor()]),
                &with(window(), &change),
                &json!([]),
                Some("DP-4"),
            )
            .unwrap();
            assert!(scene.get("target").is_none());
        }
        assert_eq!(
            snapshot(
                &json!([landscape(), monitor()]),
                &full,
                &json!([]),
                Some("DP-4")
            )
            .unwrap()["blocked"],
            true
        );
        assert!(
            snapshot(
                &json!([landscape(), monitor()]),
                &json!({}),
                &json!([]),
                Some("DP-4")
            )
            .unwrap()
            .get("target")
            .is_none()
        );
    }

    #[test]
    fn a_window_covering_the_landing_corner_drops_the_target() {
        let cover = with(
            window(),
            &json!({"address": "0xcd", "at": [-300, 1300], "size": [200, 240]}),
        );
        let scene = snapshot(
            &json!([landscape(), monitor()]),
            &window(),
            &json!([window(), cover]),
            Some("DP-4"),
        )
        .unwrap();
        assert!(scene.get("target").is_none());
    }
}
