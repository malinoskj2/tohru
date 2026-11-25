mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const PORTABILITY: &str = env!("CARGO_BIN_EXE_tohru-portability");

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned()
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Runs with private state, as the checkout shim would: beside the repository's `bin`.
fn private(command: &mut Command, root: &Path) -> Output {
    if command.get_envs().all(|(key, _)| key != "TOHRU_BIN_DIR") {
        command.env("TOHRU_BIN_DIR", repository().join("bin"));
    }
    command
        .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
        .env_remove("SWAYSOCK")
        .env("HOME", root)
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_RUNTIME_DIR", root)
        .env("WAYLAND_DISPLAY", "wayland-test")
        .env("XDG_SESSION_ID", "test")
        .env("TOHRU_MANAGED_CONFIG", "")
        .output()
        .unwrap()
}

#[test]
fn relocated_release_layout_finds_helpers_and_shared_spec() {
    let root = tempfile::tempdir().unwrap();
    let bundle = root.path().join("bundle with spaces");
    std::fs::create_dir_all(bundle.join("bin")).unwrap();
    std::fs::copy(PORTABILITY, bundle.join("bin/tohru-portability")).unwrap();
    copy_tree(&repository().join("share"), &bundle.join("share"));
    let units = bundle.join("units");
    let mut generate = Command::new(bundle.join("bin/tohru-portability"));
    generate
        .args(["session-units", "--output"])
        .arg(&units)
        .args(["--components", "bar,shortcuts"])
        .env_remove("TOHRU_BIN_DIR");
    let output = private(&mut generate, root.path());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout,
        format!(
            "Generated 3 units in {}; no services enabled or started\n",
            units.display()
        )
    );
    let unit = std::fs::read_to_string(units.join("tohru-shortcuts.service")).unwrap();
    assert!(unit.contains(&bundle.join("bin/tohru-shortcuts").display().to_string()));
    assert!(units.join("tohru-lock.service").is_file());

    let mut again = Command::new(bundle.join("bin/tohru-portability"));
    again
        .args(["session-units", "--output"])
        .arg(&units)
        .args(["--components", "bar"])
        .env_remove("TOHRU_BIN_DIR");
    let output = private(&mut again, root.path());
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("tohru: cannot create"));

    let mut unknown = Command::new(bundle.join("bin/tohru-portability"));
    unknown
        .args(["session-units", "--output"])
        .arg(root.path().join("other"))
        .args(["--components", "bar,nonsense"])
        .env_remove("TOHRU_BIN_DIR");
    let output = private(&mut unknown, root.path());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "tohru: unknown component\n"
    );
}

#[test]
fn echo_config_never_changes_routes_or_overwrites() {
    let root = tempfile::tempdir().unwrap();
    let printed = private(Command::new(PORTABILITY).arg("echo-cancel"), root.path());
    let content = String::from_utf8_lossy(&printed.stdout);
    assert!(content.contains("libpipewire-module-echo-cancel"));
    assert!(content.contains("priority.session = 0"));
    let target = root.path().join("pipewire/aec.conf");
    let created = private(
        Command::new(PORTABILITY)
            .args(["echo-cancel", "--output"])
            .arg(&target),
        root.path(),
    );
    assert!(created.status.success());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), content);
    let again = private(
        Command::new(PORTABILITY)
            .args(["echo-cancel", "--output"])
            .arg(&target),
        root.path(),
    );
    assert_eq!(again.status.code(), Some(2));
}

#[test]
fn greeter_check_reports_read_only_checks() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("config.toml");
    let settings = root.path().join("tohru.toml");
    let sessions = root.path().join("sessions");
    std::fs::create_dir_all(sessions.join("wayland-sessions")).unwrap();
    std::fs::write(
        sessions.join("wayland-sessions/hyprland.desktop"),
        "[Desktop Entry]\n",
    )
    .unwrap();
    std::fs::write(
        &config,
        "[default_session]\ncommand=\"sh -c true\"\nuser=\"root\"\n",
    )
    .unwrap();
    std::fs::write(
        &settings,
        format!(
            "[greeter]\nwallpaper=\"{}\"\n",
            root.path().join("missing.png").display()
        ),
    )
    .unwrap();
    let mut check = Command::new(PORTABILITY);
    check
        .args(["greeter-check", "--json", "--config"])
        .arg(&config)
        .arg("--settings")
        .arg(&settings)
        .arg("--sessions-dir")
        .arg(&sessions);
    let output = private(&mut check, root.path());
    assert_eq!(output.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema"], 1);
    assert_eq!(report["read_only"], true);
    let state = |name: &str| {
        report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|check| check["name"] == name)
            .map(|check| check["state"].clone())
    };
    assert_eq!(state("host-command").unwrap(), "ok");
    assert_eq!(state("greeter-user").unwrap(), "missing-or-unreadable");
    assert_eq!(state("asset.0").unwrap(), "missing-or-unreadable");
    assert_eq!(state("sessions").unwrap(), "ok");

    std::fs::write(&config, "[default_session]\nuser=\"greeter\"\n").unwrap();
    let mut invalid = Command::new(PORTABILITY);
    invalid
        .args(["greeter-check", "--config"])
        .arg(&config)
        .arg("--settings")
        .arg(&settings);
    let output = private(&mut invalid, root.path());
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("default_session command and user"));
}

#[test]
fn compositor_shortcuts_are_read_only() {
    let root = tempfile::tempdir().unwrap();
    let status = private(Command::new(PORTABILITY).arg("shortcuts"), root.path());
    let report: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(report["state"], "unsupported");
    assert_eq!(report["editable"], false);
    let setup = private(
        Command::new(PORTABILITY).args(["shortcuts", "setup"]),
        root.path(),
    );
    assert_eq!(setup.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&setup.stderr).contains("read-only"));
    let typo = private(
        Command::new(PORTABILITY).args(["shortcuts", "rebind"]),
        root.path(),
    );
    assert_eq!(typo.status.code(), Some(2));
}

#[test]
fn the_tohru_cli_dispatches_to_the_native_commands() {
    let cli = repository().join("bin/tohru");
    if !cli.is_file() {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let target = Path::new(PORTABILITY).parent().unwrap();
    let mut command = Command::new(cli);
    command
        .args(["session-units", "--output"])
        .arg(root.path().join("units"))
        .env("TOHRU_DESKTOP_TARGET", target);
    let output = private(&mut command, root.path());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let unit = std::fs::read_to_string(root.path().join("units/tohru-bar.service")).unwrap();
    assert!(unit.contains(&repository().join("bin/tohru-run").display().to_string()));
}

#[test]
fn doctor_reports_read_only_json_and_exit_codes() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("config/tohru");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join("config.toml"),
        "[assistant.discord]\ntoken_file=\"/secret/do-not-report\"\n",
    )
    .unwrap();
    let output = private(
        Command::new(PORTABILITY).args(["doctor", "--json"]),
        root.path(),
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(!text.contains("do-not-report"));
    let report: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(report["read_only"], Value::Bool(true));
    assert_eq!(
        output.status.code(),
        report["summary"]["exit_code"]
            .as_i64()
            .and_then(|code| i32::try_from(code).ok())
    );

    std::fs::write(config.join("config.toml"), "[broken").unwrap();
    let output = private(Command::new(PORTABILITY).arg("doctor"), root.path());
    assert_eq!(output.status.code(), Some(2));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.starts_with("error   invalid                  config: "));
    assert!(text.ends_with("Doctor: configuration-error (exit 2)\n"));
}
