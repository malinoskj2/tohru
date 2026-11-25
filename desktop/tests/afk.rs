mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};

use serde_json::Value;
use tempfile::TempDir;

const AFK: &str = env!("CARGO_BIN_EXE_tohru-afk-idle");

/// A private session: runtime, config and fake commands, never the live desktop.
struct Session {
    root: TempDir,
}

impl Session {
    fn new(config: &str) -> Self {
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
        let session = Self { root };
        session.fake(
            "wayland-info",
            "echo \"interface: 'ext_idle_notifier_v1', version: 1, name: 9\"\n",
        );
        session.fake(
            "pleamar",
            &format!(
                "printf '%s\\n' \"$3\" >> {}\n",
                session.path("events").display()
            ),
        );
        session.fake("busctl", "exec sleep 30\n");
        session.fake(
            "loginctl",
            "printf 'Active=yes\\nLockedHint=no\\nState=active\\n'\n",
        );
        session
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }

    fn fake(&self, name: &str, body: &str) {
        common::script(&self.path("bin"), name, body);
    }

    fn directory(&self) -> PathBuf {
        common::session_dir(&self.path("runtime"), "private-test", "private-test")
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(AFK);
        command
            .args(args)
            .env("PATH", common::path_with(&self.path("bin")))
            .env("XDG_RUNTIME_DIR", self.path("runtime"))
            .env("WAYLAND_DISPLAY", "private-test")
            .env("XDG_SESSION_ID", "private-test")
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("TOHRU_MANAGED_CONFIG", "")
            .env("TOHRU_RUNTIME", self.path("bin/pleamar"));
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }

    fn events(&self) -> Vec<String> {
        std::fs::read_to_string(self.path("events"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// The emitted idle and resume decisions, without heartbeat facts.
    fn decisions(&self) -> Vec<String> {
        let ignored = ["source", "unsafe", "safe", "threshold", "seed", "episode"];
        self.events()
            .iter()
            .filter_map(|line| {
                line.strip_prefix("emit afk_")?
                    .split_once(' ')
                    .map(|(name, _)| name.to_owned())
            })
            .filter(|name| !ignored.contains(&name.as_str()))
            .collect()
    }

    fn json(&self, name: &str) -> Option<Value> {
        serde_json::from_slice(&std::fs::read(self.directory().join(name)).ok()?).ok()
    }
}

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn terminate(child: &Child) {
    let pid = rustix::process::Pid::from_raw(i32::try_from(child.id()).unwrap()).unwrap();
    rustix::process::kill_process(pid, rustix::process::Signal::TERM).unwrap();
}

fn alive(pid: i64) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

#[test]
fn real_socket_generation_lifecycle_and_callback_delivery() {
    let session = Session::new("[assistant]\nafk_enabled=true\n");
    let client = session.path("client");
    session.fake(
        "swayidle",
        &format!("printf '%s\\n' \"$$\" \"$@\" > {}\ntrap 'bash -c \"${{@: -1}}\"; exit 0' TERM\nsleep 30 & wait $!\n", client.display()),
    );
    let mut server = Server(
        session
            .command(&["serve"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );

    let registration = common::wait_for(|| {
        session
            .json("afk.json")
            .filter(|value| value.get("observer_pid").is_some())
    });
    let generation = registration["generation"].as_i64().unwrap();
    let arguments: Vec<String> = common::wait_for(|| {
        let text = std::fs::read_to_string(&client).ok()?;
        Some(text.lines().map(str::to_owned).collect::<Vec<_>>()).filter(|lines| lines.len() >= 8)
    });
    assert_eq!(arguments[1..5], ["-C", "/dev/null", "timeout", "300"]);
    assert_eq!(arguments[6], "resume");
    assert_eq!(
        registration["observer_pid"].as_i64().unwrap().to_string(),
        arguments[0]
    );
    common::wait_for(|| Some(()).filter(|()| !session.events().is_empty()));

    let wrong = (generation + 1).to_string();
    let current = generation.to_string();
    assert!(
        session
            .run(&["notify", "idle", "--generation", &wrong])
            .status
            .success()
    );
    assert!(
        session
            .run(&["notify", "idle", "--generation", &current])
            .status
            .success()
    );
    common::wait_for(|| {
        Some(()).filter(|()| {
            session
                .events()
                .contains(&format!("emit afk_idle {generation}"))
        })
    });
    assert!(
        session
            .run(&["notify", "resume", "--generation", &current])
            .status
            .success()
    );
    let observed = common::wait_for(|| Some(session.decisions()).filter(|names| names.len() >= 3));
    assert_eq!(observed, ["idle", "resume", "resume"]);
    assert!(
        session
            .events()
            .contains(&format!("emit afk_idle {generation}"))
    );

    common::wait_for(|| {
        session
            .json("afk-episode.json")
            .filter(|state| state["idle"] == false)
    });
    assert!(
        session
            .run(&["notify", "idle", "--generation", &current])
            .status
            .success()
    );
    common::wait_for(|| {
        session
            .json("afk-episode.json")
            .filter(|state| state["episode"] == 2)
    });

    terminate(&server.0);
    assert!(server.0.wait().unwrap().success());
    assert!(!session.directory().join("afk.socket").exists());
    assert!(!session.directory().join("afk.json").exists());
    assert_eq!(
        session.events().last().unwrap(),
        &format!("emit afk_lost {generation}")
    );
    // swayidle runs its resume command on SIGTERM; that must not rearm a roll.
    assert_eq!(session.json("afk-episode.json").unwrap()["idle"], true);
    assert!(!alive(arguments[0].parse().unwrap()));
}

#[test]
fn unknown_protocol_does_not_start_an_idle_client() {
    let session = Session::new("[assistant]\nafk_enabled=true\n");
    session.fake("wayland-info", "exit 1\n");
    session.fake(
        "swayidle",
        &format!("touch {}\n", session.path("spawned").display()),
    );
    let output = session.run(&["serve"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("inactive"));
    assert!(!session.path("spawned").exists());
}

#[test]
fn disabled_afk_serves_nothing() {
    let session = Session::new("");
    let output = session.run(&["serve"]);
    assert!(output.status.success());
    assert!(!session.directory().join("afk.json").exists());
}

#[test]
fn control_effective_override_and_stop_survives_invalid_config() {
    let session = Session::new("");
    session.fake(
        "systemctl",
        &format!(
            "printf '%s\\n' \"$*\" >> {}\n",
            session.path("calls").display()
        ),
    );
    let start = [
        "control",
        "start",
        "--idle-seconds",
        "420",
        "--source",
        "passive",
        "--owner",
        "external",
    ];
    assert!(session.run(&start).status.success());
    let calls = std::fs::read_to_string(session.path("calls")).unwrap();
    assert_eq!(calls, "--user restart tohru-afk.service\n");
    let effective =
        std::fs::read_to_string(session.directory().join("afk-effective.json")).unwrap();
    assert_eq!(
        effective,
        r#"{"enabled": true, "seconds": 420, "source": "passive", "owner": "external"}"#
    );

    std::fs::write(session.path("config/tohru/config.toml"), "invalid TOML !!!").unwrap();
    assert!(session.run(&["control", "stop"]).status.success());
    assert!(!session.directory().join("afk-effective.json").exists());

    let invalid = session.run(&[
        "control",
        "start",
        "--idle-seconds",
        "0",
        "--source",
        "passive",
        "--owner",
        "external",
    ]);
    assert_eq!(invalid.status.code(), Some(1));
}

#[test]
fn failed_service_control_names_the_unit() {
    let session = Session::new("");
    session.fake("systemctl", "exit 5\n");
    let output = session.run(&["control", "stop"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("install the optional generated unit")
    );
}

#[test]
fn callbacks_capture_the_registered_generation_and_manager() {
    let session = Session::new("");
    std::fs::create_dir_all(session.directory()).unwrap();
    std::fs::set_permissions(session.directory(), std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(
        session.directory().join("afk.json"),
        r#"{"schema": 1, "generation": 123, "seconds": 300}"#,
    )
    .unwrap();
    let pid = std::process::id().to_string();
    let output = session.run(&["callbacks", "--manager-pid", &pid]);
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["seconds"], 300);
    let idle = shlex(value["idle"].as_str().unwrap());
    assert_eq!(idle[1..5], ["notify", "idle", "--generation", "123"]);
    assert_eq!(idle[5..7], ["--manager-pid".to_owned(), pid]);

    std::fs::write(
        session.directory().join("afk.json"),
        r#"{"schema": 1, "generation": 0, "seconds": 300}"#,
    )
    .unwrap();
    assert_eq!(
        session
            .run(&["callbacks", "--manager-pid", "1"])
            .status
            .code(),
        Some(1)
    );
}

fn shlex(text: &str) -> Vec<String> {
    tohru_desktop::shell::split(text).unwrap()
}
