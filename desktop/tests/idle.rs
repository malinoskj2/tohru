mod common;

use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Output};

use tempfile::TempDir;

const IDLE: &str = env!("CARGO_BIN_EXE_tohru-idle");

fn session(config: &str) -> TempDir {
    let root = tempfile::tempdir().unwrap();
    for name in ["runtime", "config/tohru", "bin"] {
        std::fs::create_dir_all(root.path().join(name)).unwrap();
    }
    std::fs::set_permissions(
        root.path().join("runtime"),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    std::fs::write(root.path().join("config/tohru/config.toml"), config).unwrap();
    let record = root.path().join("swayidle.args");
    common::script(
        &root.path().join("bin"),
        "swayidle",
        &format!("printf '%s\\n' \"$@\" > {}\n", record.display()),
    );
    root
}

fn run(root: &TempDir) -> Output {
    Command::new(IDLE)
        .env("PATH", common::path_with(&root.path().join("bin")))
        .env("XDG_RUNTIME_DIR", root.path().join("runtime"))
        .env("WAYLAND_DISPLAY", "private-test")
        .env("XDG_SESSION_ID", "private-test")
        .env("XDG_CONFIG_HOME", root.path().join("config"))
        .env("TOHRU_MANAGED_CONFIG", "")
        .output()
        .unwrap()
}

fn recorded(root: &TempDir) -> Vec<String> {
    std::fs::read_to_string(root.path().join("swayidle.args"))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn external_ownership_runs_nothing() {
    let root = session("");
    let output = run(&root);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "tohru-idle: idle ownership is external\n"
    );
    assert!(!root.path().join("swayidle.args").exists());
}

#[test]
fn owned_swayidle_waits_for_confirmed_locks_and_suspends_after_them() {
    let root = session("[idle]\nowner=\"tohru\"\nlock_seconds=300\nsuspend_seconds=900\n");
    let output = run(&root);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let args = recorded(&root);
    assert_eq!(args[..2], ["-w", "lock"]);
    assert!(args[2].ends_with("tohru-session-action lock"));
    assert_eq!(args[3], "before-sleep");
    assert_eq!(args[5..7], ["timeout", "300"]);
    assert_eq!(args[8..10], ["timeout", "900"]);
    assert!(args[10].ends_with("tohru-session-action sleep"));
}

#[test]
fn owned_idle_captures_the_registered_afk_generation_once() {
    let root = session("[idle]\nowner=\"tohru\"\nlock_seconds=0\n[assistant]\nafk_enabled=true\n");
    let directory =
        common::session_dir(&root.path().join("runtime"), "private-test", "private-test");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(run(&root).status.success());
    assert_eq!(recorded(&root).len(), 5);

    std::fs::write(
        directory.join("afk.json"),
        r#"{"schema": 1, "generation": 123, "seconds": 300}"#,
    )
    .unwrap();
    assert!(run(&root).status.success());
    let args = recorded(&root);
    assert_eq!(args[5..7], ["timeout", "300"]);
    assert!(args[7].contains("tohru-afk-idle notify idle --generation 123 --manager-pid"));
    assert_eq!(args[8], "resume");
    assert!(args[9].contains("notify resume --generation 123"));

    std::fs::write(
        directory.join("afk.json"),
        r#"{"schema": 1, "generation": 124, "seconds": 301}"#,
    )
    .unwrap();
    assert!(run(&root).status.success());
    assert_eq!(recorded(&root).len(), 5);
}

#[test]
fn effective_afk_source_overrides_the_stored_settings() {
    let root = session("[idle]\nowner=\"tohru\"\nlock_seconds=0\n");
    let directory =
        common::session_dir(&root.path().join("runtime"), "private-test", "private-test");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(
        directory.join("afk.json"),
        r#"{"schema": 1, "generation": 99, "seconds": 420}"#,
    )
    .unwrap();
    let effective = directory.join("afk-effective.json");
    std::fs::write(
        &effective,
        r#"{"enabled": true, "seconds": 420, "source": "callbacks", "owner": "tohru"}"#,
    )
    .unwrap();
    assert!(run(&root).status.success());
    assert_eq!(recorded(&root)[5..7], ["timeout", "420"]);

    std::fs::write(
        &effective,
        r#"{"enabled": true, "seconds": 0, "source": "callbacks", "owner": "tohru"}"#,
    )
    .unwrap();
    let output = run(&root);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("invalid effective AFK configuration")
    );
}
