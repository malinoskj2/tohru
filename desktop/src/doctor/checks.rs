use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

use super::host::Host;
use super::{Check, Report, Values, summarize, text, truthy};
use crate::config::{Config, ConfigError};
use crate::paths::{self, Xdg};
use crate::probe::ProbeFailure;
use crate::pyvalue::{self, py_str};

const TOOLS: [(&str, &str); 26] = [
    ("bash", "required shell"),
    ("sh", "application launch shell"),
    ("readlink", "required coreutils"),
    ("dirname", "required coreutils"),
    ("sha256sum", "required coreutils"),
    ("cut", "required coreutils"),
    ("rm", "required coreutils"),
    ("setsid", "required util-linux"),
    ("flock", "required util-linux"),
    ("systemctl", "required systemd"),
    ("loginctl", "required logind"),
    ("busctl", "required systemd"),
    ("timeout", "required coreutils"),
    ("systemd-notify", "required locker readiness"),
    (
        "wayland-info",
        "protocol/output diagnostics and wallpaper discovery",
    ),
    ("xdg-terminal-exec", "terminal application launch"),
    ("wl-copy", "assistant copy"),
    ("pactl", "audio diagnostics/voice"),
    ("parec", "voice capture"),
    ("pacat", "voice playback"),
    ("espeak-ng", "voice fallback TTS"),
    ("ffmpeg", "wallpaper previews"),
    ("ffprobe", "wallpaper metadata"),
    (
        "notify-send",
        "libnotify desktop-first Discord acknowledgement (requires action support)",
    ),
    ("jq", "wallpaper preview metadata"),
    ("swayidle", "optional idle owner"),
];
const PROTOCOLS: [(&str, &str); 5] = [
    ("layer-shell", "zwlr_layer_shell_v1"),
    ("session-lock", "ext_session_lock_manager_v1"),
    ("idle-notify", "ext_idle_notifier_v1"),
    ("window", "zwlr_foreign_toplevel_manager_v1"),
    ("workspace", "ext_workspace_manager_v1"),
];
const COMPONENTS: [&str; 9] = [
    "bar",
    "session",
    "launcher",
    "notifications",
    "polkit",
    "idle",
    "afk",
    "wallpaper",
    "lock",
];
const POLKIT_AGENTS: [&str; 5] = [
    "hyprpolkitagent",
    "polkit-gnome-au",
    "lxqt-policykit-",
    "mate-polkit",
    "polkit-kde-auth",
];
const IDLE_MANAGERS: [&str; 4] = ["hypridle", "swayidle", "wayidle", "xidlehook"];
const NOTIFICATION_DAEMONS: [&str; 4] = ["dunst", "mako", "swaync", "xfce4-notifyd"];
const ECHO_SOURCE: &str = "tohru-echo-cancel-source";
const ECHO_MODULE: &str = "libpipewire-module-echo-cancel";

fn set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty())
}

fn failure(result: &Result<String, ProbeFailure>) -> String {
    match result {
        Ok(_) => "ok".to_owned(),
        Err(failure) => failure.state(),
    }
}

/// `out` from a probe when it succeeded with output; Python treated empty output as falsy.
fn nonempty(result: &Result<String, ProbeFailure>) -> Option<&str> {
    result.as_deref().ok().filter(|out| !out.is_empty())
}

struct Doctor<'a> {
    host: &'a dyn Host,
    values: Values,
    config: Option<Config>,
    checks: Vec<Check>,
    processes: Option<Vec<String>>,
}

impl Doctor<'_> {
    fn add(
        &mut self,
        name: impl Into<String>,
        state: impl Into<String>,
        detail: impl Into<String>,
        action: &str,
    ) {
        self.checks.push(Check::new(name, state, detail, action));
    }

    fn flag(&self, key: &str) -> bool {
        truthy(self.values.get(key))
    }

    fn text(&self, key: &str) -> Option<&str> {
        text(&self.values, key)
    }

    fn configuration(&mut self, config: Result<Config, ConfigError>) {
        match config {
            Ok(config) => {
                let detail = if config.managed.is_empty() {
                    "XDG native config"
                } else {
                    "XDG native config; managed overlay"
                };
                self.add(
                    "config",
                    "ok",
                    detail,
                    "Managed values are read-only for standalone lifecycle helpers",
                );
                self.values.clone_from(&config.values);
                self.config = Some(config);
            }
            Err(_) => self.add(
                "config",
                "invalid",
                "Configuration cannot be read or validated",
                "Check TOML types, absolute paths and managed-file readability; contents withheld",
            ),
        }
    }

    fn afk(&mut self) {
        let enabled = self.flag("assistant.afk_enabled");
        let runtime = std::env::var("TOHRU_RUNTIME").unwrap_or_else(|_| "pleamar".to_owned());
        let status = enabled
            .then(|| {
                self.host
                    .json_probe(&runtime, &["--say", "tohru", "get ai_afk_status"])
                    .ok()
            })
            .flatten()
            .filter(Value::is_object);
        let observed = status
            .as_ref()
            .is_some_and(|status| status.get("source") == Some(&Value::Bool(true)));
        let state = if observed {
            "observed"
        } else if enabled {
            "unknown"
        } else {
            "disabled"
        };
        let detail = match &status {
            Some(status) => status
                .get("state")
                .map_or_else(|| "Unknown".to_owned(), |value| py_str(Some(value))),
            None if !enabled => "Default-off; source/geometry not observed".to_owned(),
            None => "Native AFK source/geometry unavailable; no synthetic idle fallback".to_owned(),
        };
        self.add("assistant.afk", state, detail, "Install the optional AFK service and inspect tohru afk status --json; unknown session/protocol state remains inactive");
        if let Some(Value::Object(availability)) = status
            .as_ref()
            .and_then(|status| status.get("availability"))
        {
            let mut scenes: Vec<(&String, &Value)> = availability.iter().collect();
            scenes.sort_by(|left, right| left.0.cmp(right.0));
            for (scene, reason) in scenes {
                self.add(format!("assistant.afk.scene.{scene}"), "unsupported", py_str(Some(reason)), "Normal-size eligible scenes remain available; no tiny substitute for an unavailable closeup");
            }
        }
    }

    fn discord(&mut self) {
        let status = self.host.discord();
        let valid = pyvalue::truthy(status.get("enabled"));
        let state = if !self.flag("assistant.discord.enabled") {
            "disabled"
        } else if valid {
            "configured"
        } else {
            "invalid"
        };
        let detail = if valid {
            "Local setup validated; credentials/access not verified"
        } else {
            "Disabled or incomplete; run tohru-discord status"
        };
        self.add("discord.configuration", state, detail, "");
        let ready = valid
            && self
                .values
                .get("assistant.backend")
                .is_none_or(|backend| backend.as_str() == Some("local"))
            && status.get("destination").and_then(Value::as_str) == Some("dm")
            && pyvalue::truthy(status.get("read"));
        let state = if !self.flag("assistant.discord.receive") {
            "disabled"
        } else if ready {
            "configured"
        } else {
            "unsupported"
        };
        let detail = if ready {
            "Owned by the running Tohru bar assistant controller; activity not probed"
        } else {
            "Requires local backend, DM destination, enabled/read and a valid configured user"
        };
        self.add(
            "discord.receiver",
            state,
            detail,
            "Keep the Tohru bar running; remote Orca reception is unsupported",
        );
    }

    #[expect(
        clippy::verbose_bit_mask,
        reason = "permission bits read clearer as a mask"
    )]
    fn environment(&mut self) {
        for name in [
            "WAYLAND_DISPLAY",
            "XDG_RUNTIME_DIR",
            "DBUS_SESSION_BUS_ADDRESS",
            "XDG_SESSION_ID",
        ] {
            let (state, detail) = if set(name) {
                ("ok", "set")
            } else {
                ("missing", "unset")
            };
            self.add(format!("environment.{name}"), state, detail, "Import session environment into systemd --user before starting graphical-session.target");
        }
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").filter(|value| !value.is_empty());
        let valid = runtime
            .and_then(|runtime| std::fs::metadata(runtime).ok())
            .is_some_and(|info| {
                info.is_dir() && info.uid() == paths::uid() && info.mode() & 0o077 == 0
            });
        if valid {
            self.add(
                "environment.runtime-directory",
                "ok",
                "Existing private directory owned by the current user",
                "",
            );
        } else {
            self.add(
                "environment.runtime-directory",
                "invalid",
                "XDG_RUNTIME_DIR is missing, inaccessible or not private/owned",
                "",
            );
        }
        for (tool, purpose) in TOOLS {
            let state = if self.host.which(tool) {
                "ok"
            } else {
                "missing"
            };
            self.add(
                format!("executable.{tool}"),
                state,
                purpose,
                "Install through your distribution package manager",
            );
        }
    }

    fn compositor(&mut self) {
        let runtime = std::env::var("TOHRU_RUNTIME").unwrap_or_else(|_| "pleamar".to_owned());
        let version = self.host.json_probe(&runtime, &["--tohru-capabilities"]);
        let compatible = version.as_ref().is_ok_and(|version| {
            version.is_object()
                && pyvalue::same(version.get("tohru_portability"), Some(&Value::from(1)))
                && version.get("lock_state").and_then(Value::as_str) == Some("native-cycle")
        });
        let state = match &version {
            _ if compatible => "ok",
            Err(ProbeFailure::Missing) => "missing",
            Ok(Value::Object(_)) => "incompatible",
            _ => "incompatible-or-unknown",
        };
        let detail = if compatible {
            "Patched runtime capability schema 1"
        } else {
            "Patch compatibility was not established"
        };
        self.add(
            "runtime",
            state,
            detail,
            "Use the runtime shipped with this checkout/release; stock Pleamar is insufficient",
        );

        let protocols = self.host.protocols();
        for (name, interface) in PROTOCOLS {
            let (state, detail) = match &protocols {
                Ok(caps) if caps.iter().any(|cap| cap == interface) => {
                    ("ok", format!("{interface} advertised"))
                }
                Ok(_) => ("unsupported", format!("{interface} not advertised")),
                Err(failure) => ("unknown", format!("{interface}: {}", failure.state())),
            };
            let action = if state == "ok" {
                ""
            } else {
                "Choose a layer-shell compositor; workspace/window support degrades independently"
            };
            self.add(format!("protocol.{name}"), state, detail, action);
        }
        let (count, discovery) = self.host.outputs();
        let state = if count > 0 {
            "ok"
        } else if discovery != "ok" {
            "unknown"
        } else {
            "missing"
        };
        self.add("outputs", state, format!("{count} named wl_output outputs; discovery {discovery}"), "wl_output/xdg-output names are required by wallpaper discovery; no workspace metadata is used");
        let hypr = set("HYPRLAND_INSTANCE_SIGNATURE") && !set("PLEAMAR_GENERIC");
        let state = if hypr || protocols.is_ok() {
            "ok"
        } else {
            "unknown"
        };
        let detail = if hypr {
            "Hyprland IPC (window/workspace), Wayland outputs"
        } else {
            "Standard protocol backend; only advertised protocols are supported"
        };
        self.add("compositor.backend", state, detail, "");
    }

    fn session(&mut self) {
        let session = std::env::var("XDG_SESSION_ID").unwrap_or_else(|_| "self".to_owned());
        let probes: [(&str, &str, Vec<&str>); 2] = [
            (
                "systemd-user",
                "systemctl",
                vec!["--user", "show-environment"],
            ),
            (
                "logind",
                "loginctl",
                vec!["show-session", &session, "-p", "Type", "--value"],
            ),
        ];
        for (label, program, args) in probes {
            // Never include show-environment in the report: it can contain secrets.
            let result = self.host.probe(program, &args, Duration::from_secs(3));
            let (state, detail) = if result.is_ok() {
                ("ok".to_owned(), "reachable")
            } else {
                (failure(&result), "unavailable")
            };
            self.add(
                label,
                state,
                detail,
                "Check systemd user manager, logind session and D-Bus permissions",
            );
        }
        let names = self.host.probe(
            "busctl",
            &["--user", "list", "--no-pager", "--no-legend"],
            Duration::from_secs(3),
        );
        let notification = names.as_ref().ok().and_then(|names| {
            names
                .lines()
                .find(|line| line.starts_with("org.freedesktop.Notifications "))
                .map(str::to_owned)
        });
        let conflict = notification.as_ref().is_some_and(|line| {
            line.split_whitespace()
                .any(|part| NOTIFICATION_DAEMONS.contains(&part))
        });
        let state = if conflict {
            "possible-conflict".to_owned()
        } else if names.is_err() {
            "unknown".to_owned()
        } else if notification.is_some() {
            "present-unverified".to_owned()
        } else {
            "missing".to_owned()
        };
        let detail = if notification.is_some() {
            "A notification owner exists; verify it is Tohru before enabling another".to_owned()
        } else if names.is_ok() {
            "No owner observed".to_owned()
        } else {
            failure(&names)
        };
        self.add("notifications.owner", state, detail, "");
        let uid = paths::uid().to_string();
        let processes = self
            .host
            .probe("ps", &["-u", &uid, "-o", "comm="], Duration::from_secs(3));
        self.processes =
            nonempty(&processes).map(|out| out.split_whitespace().map(str::to_owned).collect());
        let agents = self.running(&POLKIT_AGENTS);
        let (state, detail) = if agents.is_empty() {
            (
                "unknown",
                "No known competing process observed; registration ownership is not universally enumerable",
            )
        } else {
            (
                "possible-conflict",
                "Another known polkit agent process was observed",
            )
        };
        self.add(
            "polkit.conflicts",
            state,
            detail,
            "Run only one polkit agent per session",
        );
    }

    /// The sorted names from `candidates` among this user's processes.
    fn running(&self, candidates: &[&str]) -> Vec<String> {
        let mut found: Vec<String> = candidates
            .iter()
            .filter(|name| {
                self.processes
                    .as_ref()
                    .is_some_and(|processes| processes.iter().any(|process| process == *name))
            })
            .map(|name| (*name).to_owned())
            .collect();
        found.sort();
        found
    }

    fn services(&mut self) {
        for component in COMPONENTS {
            let unit = format!("tohru-{component}.service");
            let result = self.host.probe(
                "systemctl",
                &[
                    "--user",
                    "show",
                    &unit,
                    "-p",
                    "LoadState",
                    "-p",
                    "ActiveState",
                    "--value",
                ],
                Duration::from_secs(1),
            );
            let out = nonempty(&result);
            let states: Vec<&str> = out
                .map(|out| out.split_whitespace().collect())
                .unwrap_or_default();
            let state = if states.contains(&"loaded") {
                "present"
            } else if states.contains(&"not-found") {
                "missing"
            } else if states.contains(&"masked") {
                "unavailable"
            } else {
                "unknown"
            };
            let detail = out.map_or_else(|| failure(&result), |out| out.trim().replace('\n', ", "));
            self.add(
                format!("service.{component}"),
                state,
                detail,
                &format!(
                    "Generate/install units and inspect journalctl --user -u tohru-{component}"
                ),
            );
        }
        let pam = ["tohru-lock", "pleamar", "hyprlock", "login"]
            .into_iter()
            .find(|name| Path::new("/etc/pam.d").join(name).is_file());
        self.add("locker.pam", if pam.is_some() { "present-unverified" } else { "missing" }, pam.unwrap_or("No fallback PAM policy found"), "Have your distro administrator review tohru-lock PAM policy; no authentication attempted");
        self.add("locker.recovery", "requires-validation", "Recovery after a locker crash is compositor-dependent", "Hyprland needs allow_session_lock_restore; verify nested-session recovery and keep TTY access");
    }

    fn wallpaper(&mut self) {
        let enabled = self.flag("wallpaper.enabled");
        let executable = self
            .text("wallpaper.executable")
            .unwrap_or("mpvpaper")
            .to_owned();
        let state = if !enabled {
            "disabled"
        } else if self.host.which(&executable) {
            "configured"
        } else {
            "missing"
        };
        let detail = if enabled {
            "Explicit ownership enabled"
        } else {
            "Explicit ownership disabled"
        };
        self.add("wallpaper", state, detail, "Configure wallpaper.directory, install mpvpaper, and start only Tohru's owner if desired");
        let supervisor = paths::bin_dir().join("tohru-wallpaper");
        let status = if enabled {
            self.host
                .json_probe(&supervisor.to_string_lossy(), &["status"])
                .ok()
                .filter(Value::is_object)
        } else {
            None
        };
        let errors = status
            .as_ref()
            .is_some_and(|status| pyvalue::truthy(status.get("errors")));
        let owned = status
            .as_ref()
            .is_some_and(|status| status.get("owner").and_then(Value::as_str) == Some("tohru"));
        let state = if !enabled {
            "disabled"
        } else if errors {
            "backend-errors"
        } else if owned {
            "ok"
        } else if status.is_some() {
            "missing"
        } else {
            "unknown"
        };
        let detail = if errors {
            "Owned supervisor reports output backend failures"
        } else if owned {
            "Owned supervisor reachable"
        } else {
            "No owned supervisor observed"
        };
        self.add(
            "wallpaper.owner",
            state,
            detail,
            "Explicitly start the configured wallpaper owner; doctor never starts one",
        );
        for key in ["wallpaper.directory", "wallpaper.assignments"] {
            let value = self.text(key).unwrap_or_default().to_owned();
            let (state, detail) = if value.is_empty() {
                ("default", "XDG/Pictures default")
            } else if Path::new(&value).exists() {
                ("present", "Configured path exists")
            } else {
                ("missing", "Configured path missing")
            };
            self.add(key, state, detail, "");
        }
    }

    fn idle(&mut self) {
        let owner = self.values.get("idle.owner").map_or_else(
            || "unknown".to_owned(),
            |value| {
                value
                    .as_str()
                    .map_or_else(|| value.to_string(), str::to_owned)
            },
        );
        self.add(
            "idle.owner",
            "configured",
            owner,
            "Use tohru or an external idle manager, never both; suspend requires confirmed lock",
        );
        let managers = self.running(&IDLE_MANAGERS);
        let (state, detail) = if managers.is_empty() {
            (
                "unknown",
                "No known idle process observed; unknown managers cannot be excluded",
            )
        } else {
            (
                "present-unverified",
                "Known idle-manager process observed; check ownership before enabling Tohru",
            )
        };
        self.add("idle.processes", state, detail, "");
        let delay = self.host.probe(
            "busctl",
            &[
                "--system",
                "--timeout=1",
                "get-property",
                "org.freedesktop.login1",
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
                "InhibitDelayMaxUSec",
            ],
            Duration::from_secs(2),
        );
        let (state, detail) = if nonempty(&delay).is_some() {
            (
                "observed",
                "logind finite delay limit is readable; validate cold lock startup against it"
                    .to_owned(),
            )
        } else {
            ("unknown", failure(&delay))
        };
        self.add("idle.inhibit-delay", state, detail, "External/forced suspend may proceed after logind's delay deadline; Tohru's own suspend requires confirmed lock");
        for key in [
            "appearance.font_text",
            "appearance.font_display",
            "appearance.font_mono",
        ] {
            let family = self.values.get(key).map_or_else(
                || "Noto Sans".to_owned(),
                |value| {
                    value
                        .as_str()
                        .map_or_else(|| value.to_string(), str::to_owned)
                },
            );
            let result = self.host.probe(
                "fc-match",
                &["--format=%{family}", &family],
                Duration::from_secs(3),
            );
            let (state, detail) = match nonempty(&result) {
                Some(out) => (
                    "ok",
                    format!(
                        "Resolved font: {}",
                        out.chars().take(160).collect::<String>()
                    ),
                ),
                None => ("unknown", failure(&result)),
            };
            self.add(key, state, detail, "Install Noto Sans or another freely distributable system font; restart components after changes");
        }
    }

    fn audio(&mut self) {
        let audio = self.host.json_probe("pactl", &["-f", "json", "info"]);
        let state = if audio.as_ref().is_ok_and(Value::is_object) {
            "ok"
        } else {
            "unknown"
        };
        let detail = match &audio {
            Ok(value) if pyvalue::truthy(Some(value)) => {
                "PulseAudio-compatible server reachable".to_owned()
            }
            Ok(_) => "ok".to_owned(),
            Err(failure) => failure.state(),
        };
        self.add("audio", state, detail, "Use PipeWire-Pulse or PulseAudio");
        for (name, bus) in [
            ("networkmanager", "org.freedesktop.NetworkManager"),
            ("iwd", "net.connman.iwd"),
            ("bluetooth", "org.bluez"),
        ] {
            let result = self.host.probe(
                "busctl",
                &["--system", "--timeout=1", "status", bus],
                Duration::from_secs(2),
            );
            let (state, detail) = if result.is_ok() {
                ("ok", "Service reachable".to_owned())
            } else {
                ("unknown", failure(&result))
            };
            self.add(name, state, detail, "Install/enable the service and review system-bus policy; kernel network fallback is read-only");
        }
    }

    fn echo_cancel(&mut self) {
        let sources = self
            .host
            .json_probe("pactl", &["-f", "json", "list", "sources"]);
        let available = matches!(&sources, Ok(Value::Array(items)) if items.iter().any(|item| item.get("name").and_then(Value::as_str) == Some(ECHO_SOURCE)));
        let modules = self
            .host
            .probe("pw-cli", &["ls", "Module"], Duration::from_secs(3));
        let (state, detail) = match &modules {
            Ok(modules) if modules.contains(ECHO_MODULE) => {
                ("loaded", "PipeWire echo-cancel module observed".to_owned())
            }
            Ok(_) => ("not-observed", "Module not in PipeWire registry".to_owned()),
            Err(failure) => ("unknown", failure.state()),
        };
        self.add("echo-cancel.module", state, detail, "");
        let mut directories: Vec<PathBuf> = [
            "/usr/lib/pipewire-0.3",
            "/usr/lib64/pipewire-0.3",
            "/usr/lib/x86_64-linux-gnu/pipewire-0.3",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect();
        if let Some(directory) =
            std::env::var_os("PIPEWIRE_MODULE_DIR").filter(|value| !value.is_empty())
        {
            directories.insert(0, PathBuf::from(directory));
        }
        let installed = directories
            .iter()
            .any(|directory| directory.join(format!("{ECHO_MODULE}.so")).is_file());
        if installed {
            self.add(
                "echo-cancel.installed",
                "present",
                "Module file observed",
                "",
            );
        } else {
            self.add(
                "echo-cancel.installed",
                "unknown",
                "Module file not found in common module directories; custom/Nix layout may differ",
                "",
            );
        }
        let drop_in =
            paths::xdg(Xdg::Config).join("pipewire/pipewire.conf.d/60-tohru-echo-cancel.conf");
        if drop_in.is_file() {
            self.add("echo-cancel.config", "configured", "Drop-in present", "");
        } else {
            self.add(
                "echo-cancel.config",
                "absent",
                "No Tohru user drop-in found",
                "",
            );
        }
        let state = if available {
            "present"
        } else if sources.is_err() {
            "unknown"
        } else {
            "missing"
        };
        self.add(
            "echo-cancel.nodes",
            state,
            if available {
                "Virtual source observed"
            } else {
                "Virtual source not observed"
            },
            "",
        );
        if self.text("assistant.voice_source") == Some(ECHO_SOURCE) {
            self.add(
                "echo-cancel.selection",
                "selected",
                "Explicit voice source selection",
                "",
            );
        } else {
            self.add(
                "echo-cancel.selection",
                "automatic-or-other",
                "Voice source not explicitly selected",
                "",
            );
        }
        self.add("echo-cancel.effectiveness", "unknown", "No physical acoustic validation performed", "Select both the virtual source and playback sink; module presence alone proves no cancellation");
    }

    fn voice(&mut self) {
        let enabled = self.flag("assistant.voice");
        let bundled = paths::bin_dir().join("tohru-voice");
        let service = self.host.which("tohru-voice") || bundled.is_file();
        let missing: Vec<&str> = ["parec", "pacat", "pactl"]
            .into_iter()
            .filter(|name| !self.host.which(name))
            .collect();
        let state = if !enabled {
            "disabled"
        } else if service && missing.is_empty() {
            "ok"
        } else {
            "missing"
        };
        let detail = if service && missing.is_empty() {
            "tohru-voice and the PulseAudio clients found (not started)".to_owned()
        } else if !enabled {
            "disabled".to_owned()
        } else {
            let mut names = if service {
                Vec::new()
            } else {
                vec!["tohru-voice"]
            };
            names.extend(&missing);
            format!("Missing: {}", names.join(", "))
        };
        self.add("voice.dependencies", state, detail, "Install the tohru-voice binary from voice/ (docs/standalone.md) and the PulseAudio client tools");
        let kokoro =
            self.text("assistant.tts_engine") == Some("kokoro") && self.flag("assistant.tts_model");
        let espeak = self.host.which("espeak-ng");
        let state = if !enabled {
            "disabled"
        } else if espeak {
            "present-unverified"
        } else {
            "missing"
        };
        let detail = if espeak {
            "espeak-ng found; it phonemizes Kokoro and is the speech fallback"
        } else {
            "espeak-ng is required for Kokoro phonemes and the fallback voice"
        };
        let action = if kokoro {
            "Install espeak-ng"
        } else {
            "Install espeak-ng; Kokoro also needs assistant.tts_model"
        };
        self.add("voice.tts.dependencies", state, detail, action);
        let kws = self.text("assistant.voice_wake") == Some("kws");
        for (key, required) in [
            ("vad_model", true),
            ("asr_model", true),
            ("turn_model", false),
            ("kws_model", kws),
            ("tts_model", false),
        ] {
            let path = self
                .text(&format!("assistant.{key}"))
                .unwrap_or_default()
                .to_owned();
            let present = !path.is_empty() && Path::new(&path).exists();
            let state = if !enabled {
                "disabled"
            } else if present {
                "present-unverified"
            } else if required {
                "missing"
            } else {
                "optional-unset"
            };
            let detail = if present {
                "Configured artifact exists; compatibility not tested"
            } else if required {
                "Required artifact missing"
            } else {
                "Optional; fallback applies"
            };
            self.add(
                format!("voice.{key}"),
                state,
                detail,
                "Use pinned artifacts and hashes in docs/configuration.md; nothing is downloaded",
            );
        }
    }

    fn shortcuts(&mut self) {
        let report = self.host.shortcuts(self.config.as_ref());
        let state = py_str(report.get("state"));
        let bindings = report
            .get("bindings")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        self.add(
            "shortcuts",
            state.clone(),
            format!(
                "{}; {bindings} read-only runtime entries",
                py_str(report.get("owner"))
            ),
            "Configure shortcuts in your compositor; Tohru has no global registration backend",
        );
        let portal = self.host.probe(
            "busctl",
            &[
                "--user",
                "--auto-start=no",
                "--timeout=1",
                "get-property",
                "org.freedesktop.portal.Desktop",
                "/org/freedesktop/portal/desktop",
                "org.freedesktop.portal.GlobalShortcuts",
                "version",
            ],
            Duration::from_secs(2),
        );
        let (state_portal, detail) = if nonempty(&portal).is_some() {
            (
                "present-unverified",
                "GlobalShortcuts interface version readable; no binding session requested"
                    .to_owned(),
            )
        } else {
            ("unknown", failure(&portal))
        };
        self.add("shortcuts.portal", state_portal, detail, "");
        let portal_mode = self.text("shortcuts.mode") == Some("portal");
        let owner = paths::bin_dir().join("tohru-shortcuts");
        let dependency = portal_mode
            .then(|| {
                self.host
                    .json_probe(&owner.to_string_lossy(), &["--check-dependencies"])
                    .ok()
            })
            .flatten();
        let dependency_state = match &dependency {
            _ if !portal_mode => "disabled",
            Some(Value::Object(found))
                if found.get("portal_client").and_then(Value::as_str) == Some("built-in") =>
            {
                "ok"
            }
            Some(Value::Object(_)) => "missing",
            _ => "unknown",
        };
        let detail = if portal_mode {
            "Built-in portal client"
        } else {
            "Portal ownership disabled"
        };
        self.add(
            "shortcuts.dependencies",
            dependency_state,
            detail,
            "Rebuild tohru-shortcuts; the portal client is compiled in",
        );
        let granted = pyvalue::truthy(report.get("active_registration")) && bindings > 0;
        let registration = if !portal_mode {
            "disabled"
        } else if granted {
            "ok"
        } else if state == "request-pending" {
            "unknown"
        } else {
            "configured-ungranted"
        };
        let detail = if granted {
            "Current portal session has backend-confirmed action grants".to_owned()
        } else {
            report
                .get("state")
                .map_or_else(|| "unavailable".to_owned(), |value| py_str(Some(value)))
        };
        self.add("shortcuts.registration", registration, detail, "Use Settings → Shortcuts setup/configure after explicitly enabling the owner service; doctor never requests grants");
    }

    fn greeter(&mut self) {
        for (name, path) in [
            ("greeter.greetd", "/etc/greetd/config.toml"),
            ("greeter.pam", "/etc/pam.d/greetd"),
        ] {
            let (state, detail) = if Path::new(path).is_file() {
                ("present-unverified", "File present")
            } else {
                ("optional-unset", "Standalone shell does not require greetd")
            };
            self.add(
                name,
                state,
                detail,
                "See docs/standalone.md; no system changes or authentication attempted",
            );
        }
    }
}

/// Runs every read-only check against `host`; never starts services or reports config values.
pub fn run(host: &dyn Host, config: Result<Config, ConfigError>) -> Report {
    let mut doctor = Doctor {
        host,
        values: Values::new(),
        config: None,
        checks: Vec::new(),
        processes: None,
    };
    doctor.configuration(config);
    doctor.afk();
    doctor.discord();
    doctor.environment();
    doctor.compositor();
    doctor.session();
    doctor.services();
    doctor.wallpaper();
    doctor.idle();
    doctor.audio();
    doctor.echo_cancel();
    doctor.voice();
    doctor.shortcuts();
    doctor.greeter();
    let mut checks = doctor.checks;
    let summary = summarize(&mut checks, &doctor.values);
    Report { summary, checks }
}
