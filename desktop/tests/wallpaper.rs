mod common;

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tempfile::TempDir;

const WALLPAPER: &str = env!("CARGO_BIN_EXE_tohru-wallpaper");
const OUTPUTS: &str = "interface: 'wl_output', version: 4, name: 8\n\tname: DP-1\n";

struct Desktop {
    root: TempDir,
    loaded: Arc<Mutex<Vec<Value>>>,
}

impl Desktop {
    fn new(enabled: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        for name in ["runtime", "config/tohru", "bin", "walls"] {
            std::fs::create_dir_all(root.path().join(name)).unwrap();
        }
        std::fs::set_permissions(
            root.path().join("runtime"),
            std::fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        std::fs::write(root.path().join("walls/dune.mp4"), b"video").unwrap();
        std::fs::write(root.path().join("walls/notes.txt"), b"text").unwrap();
        std::fs::write(root.path().join("outputs"), OUTPUTS).unwrap();
        common::script(
            &root.path().join("bin"),
            "wayland-info",
            &format!("cat {}/outputs\n", root.path().display()),
        );
        let mpv = common::script(
            &root.path().join("bin"),
            "mpvpaper",
            &format!(
                "echo $$ > {}/pid-$3\nprintf '%s\\n' \"$@\" > {0}/spawn.tmp && mv {0}/spawn.tmp {0}/spawn-$3\ntrap 'kill $!; exit 0' TERM\nsleep 30 & wait $!\n",
                root.path().display()
            ),
        );
        let config = format!(
            "[wallpaper]\nenabled={enabled}\ndirectory=\"{}\"\nassignments=\"{}\"\nexecutable=\"{}\"\n",
            root.path().join("walls").display(),
            root.path().join("state/assignments.json").display(),
            mpv.display()
        );
        std::fs::write(root.path().join("config/tohru/config.toml"), config).unwrap();
        let desktop = Self {
            root,
            loaded: Arc::new(Mutex::new(Vec::new())),
        };
        desktop.fake_mpv();
        desktop
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }

    /// Plays mpv's part of each spawned mpvpaper: binds its IPC socket and answers requests.
    fn fake_mpv(&self) {
        let spawn = self.path("spawn-DP-1");
        let loaded = Arc::clone(&self.loaded);
        std::thread::spawn(move || {
            loop {
                let Ok(args) = std::fs::read_to_string(&spawn) else {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    continue;
                };
                let _ = std::fs::remove_file(&spawn);
                let options = args.lines().nth(1).unwrap_or_default().to_owned();
                let socket = options
                    .split(' ')
                    .find_map(|word| word.strip_prefix("--input-ipc-server="))
                    .unwrap()
                    .to_owned();
                let listener = UnixListener::bind(&socket).unwrap();
                let file = args.lines().nth(3).unwrap_or_default().to_owned();
                let loaded = Arc::clone(&loaded);
                std::thread::spawn(move || serve_mpv(&listener, file, &loaded));
            }
        });
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(WALLPAPER);
        command
            .args(args)
            .env("PATH", common::path_with(&self.path("bin")))
            .env("XDG_RUNTIME_DIR", self.path("runtime"))
            .env("WAYLAND_DISPLAY", "private-test")
            .env("XDG_SESSION_ID", "private-test")
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("TOHRU_MANAGED_CONFIG", "");
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }

    fn json(&self, args: &[&str]) -> Value {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }

    fn error(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        String::from_utf8_lossy(&output.stderr).into_owned()
    }

    fn control(&self) -> PathBuf {
        common::session_dir(&self.path("runtime"), "private-test", "private-test")
            .join("wallpaper/control.sock")
    }

    fn serve(&self) -> Owner {
        let owner = Owner(
            self.command(&["serve"])
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        common::wait_for(|| Some(()).filter(|()| self.control().exists()));
        owner
    }

    fn file(&self) -> String {
        self.path("walls/dune.mp4")
            .canonicalize()
            .unwrap()
            .display()
            .to_string()
    }
}

fn serve_mpv(listener: &UnixListener, mut file: String, loaded: &Mutex<Vec<Value>>) {
    for connection in listener.incoming() {
        let Ok(mut connection) = connection else {
            return;
        };
        let mut line = String::new();
        if BufReader::new(&connection).read_line(&mut line).is_err() {
            continue;
        }
        let request: Value = serde_json::from_str(&line).unwrap();
        let command = request["command"].clone();
        let data = match command[0].as_str().unwrap() {
            "get_property" => match command[1].as_str().unwrap() {
                "path" => json!(file),
                "time-pos" => json!(1.5),
                "duration" => json!(10),
                _ => json!(false),
            },
            "loadfile" => {
                command[1].as_str().unwrap().clone_into(&mut file);
                loaded.lock().unwrap().push(command.clone());
                Value::Null
            }
            _ => {
                loaded.lock().unwrap().push(command.clone());
                Value::Null
            }
        };
        let event = json!({"event": "property-change"});
        let reply = json!({"request_id": 1, "error": "success", "data": data});
        let _ = connection.write_all(format!("{event}\n{reply}\n").as_bytes());
    }
}

struct Owner(Child);

impl Owner {
    fn stop(mut self) -> bool {
        let pid = rustix::process::Pid::from_raw(i32::try_from(self.0.id()).unwrap()).unwrap();
        rustix::process::kill_process(pid, rustix::process::Signal::TERM).unwrap();
        self.0.wait().unwrap().success()
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn mpvpaper_running(root: &Path) -> bool {
    let needle = root.join("bin/mpvpaper").display().to_string();
    std::fs::read_dir("/proc").unwrap().flatten().any(|entry| {
        std::fs::read(entry.path().join("cmdline"))
            .is_ok_and(|raw| String::from_utf8_lossy(&raw).contains(&needle))
    })
}

#[test]
fn owner_applies_persists_restores_and_stops_its_own_wallpapers() {
    let desktop = Desktop::new(true);
    let owner = desktop.serve();
    let status = desktop.json(&["status"]);
    assert_eq!(status["owner"], "tohru");
    assert_eq!(status["discovery"], "ok");
    assert_eq!(status["playing"], json!({"DP-1": null}));

    let file = desktop.file();
    assert_eq!(
        desktop.json(&["apply", "DP-1", &file, "4"]),
        json!({"ok": true})
    );
    let loaded = desktop.loaded.lock().unwrap().clone();
    assert_eq!(
        loaded,
        [
            json!(["loadfile", file, "replace"]),
            json!(["seek", 4.0, "absolute"])
        ]
    );
    let playing = desktop.json(&["playing", "DP-1"]);
    assert_eq!(
        playing,
        json!({"path": file, "pos": 1.5, "duration": 10, "paused": false})
    );
    let assignments = std::fs::read_to_string(desktop.path("state/assignments.json")).unwrap();
    assert_eq!(
        assignments,
        format!(r#"{{"assignments": {{"DP-1": "{file}"}}}}"#)
    );

    let scan = desktop.json(&["scan"]);
    let files = scan["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["kind"], "video");
    assert_eq!(scan["assignments"]["DP-1"], json!(file));
    assert_eq!(scan["monitors"][0]["name"], "DP-1");

    assert!(
        desktop
            .error(&["seek", "DP-1", "nan"])
            .contains("seconds must be finite")
    );
    assert!(
        desktop
            .error(&["apply", "HDMI-A-9", &file])
            .contains("output is not currently advertised")
    );
    let outside = desktop.path("walls/notes.txt").display().to_string();
    assert!(
        desktop
            .error(&["load", "DP-1", &outside])
            .contains("supported regular file")
    );
    assert!(
        desktop
            .error(&["pause", "DP-1", "maybe"])
            .contains("unsupported wallpaper command")
    );
    assert_eq!(desktop.json(&["pause", "DP-1", "yes"]), json!({"ok": true}));

    assert!(owner.stop());
    assert!(!desktop.control().exists());
    common::wait_for(|| Some(()).filter(|()| !mpvpaper_running(desktop.root.path())));

    let restored = desktop.serve();
    common::wait_for(|| Some(()).filter(|()| mpvpaper_running(desktop.root.path())));
    let playing = common::wait_for(|| {
        Some(desktop.json(&["status"])).filter(|status| !status["playing"]["DP-1"].is_null())
    });
    assert_eq!(playing["playing"]["DP-1"]["path"], json!(file));
    assert_eq!(desktop.json(&["stop", "DP-1"]), json!({"ok": true}));
    assert!(!mpvpaper_running(desktop.root.path()));
    let assignments = std::fs::read_to_string(desktop.path("state/assignments.json")).unwrap();
    assert_eq!(assignments, r#"{"assignments": {}}"#);
    assert!(restored.stop());
}

#[test]
fn without_an_owner_status_degrades_and_commands_explain() {
    let desktop = Desktop::new(true);
    let status = desktop.json(&["status"]);
    assert_eq!(
        status,
        json!({"playing": {}, "enabled": true, "owner": "unavailable", "discovery": "ok"})
    );
    let scan = desktop.json(&["scan"]);
    assert_eq!(scan["files"].as_array().unwrap().len(), 1);
    assert!(
        desktop
            .error(&["apply", "DP-1", &desktop.file()])
            .contains("start tohru-wallpaper.service")
    );
    assert!(desktop.error(&[]).contains("usage: tohru-wallpaper"));
}

#[test]
fn disabled_wallpaper_starts_no_owner() {
    let desktop = Desktop::new(false);
    let output = desktop.run(&["serve"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("wallpaper.enabled is false"));
    assert_eq!(desktop.json(&["status"])["owner"], "disabled");
}

#[test]
fn options_cannot_replace_the_ipc_endpoint() {
    let desktop = Desktop::new(true);
    let config = desktop.path("config/tohru/config.toml");
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str("options=\"--input-ipc-server=/tmp/elsewhere\"\n");
    std::fs::write(&config, text).unwrap();
    let owner = desktop.serve();
    assert!(
        desktop
            .error(&["apply", "DP-1", &desktop.file()])
            .contains("cannot override IPC")
    );
    assert!(owner.stop());
}

fn pid(desktop: &Desktop, output: &str) -> Option<i32> {
    std::fs::read_to_string(desktop.path(&format!("pid-{output}")))
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn alive(pid: i32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

#[test]
fn crashed_and_hotplugged_outputs_restart_without_touching_other_wallpapers() {
    let desktop = Desktop::new(true);
    let owner = desktop.serve();
    let external = Command::new(desktop.path("bin/mpvpaper"))
        .args(["-o", "unused", "external", "unused"])
        .spawn()
        .unwrap();
    let external = Owner(external);
    assert_eq!(
        desktop.json(&["apply", "DP-1", &desktop.file()]),
        json!({"ok": true})
    );
    let first = pid(&desktop, "DP-1").unwrap();

    let target = rustix::process::Pid::from_raw(first).unwrap();
    rustix::process::kill_process(target, rustix::process::Signal::KILL).unwrap();
    let second =
        common::wait_for(|| pid(&desktop, "DP-1").filter(|pid| *pid != first && alive(*pid)));

    std::fs::write(desktop.path("outputs"), "").unwrap();
    common::wait_for(|| Some(()).filter(|()| !alive(second)));
    assert_eq!(desktop.json(&["status"])["playing"], json!({}));
    assert!(alive(i32::try_from(external.0.id()).unwrap()));

    std::fs::write(desktop.path("outputs"), OUTPUTS).unwrap();
    common::wait_for(|| pid(&desktop, "DP-1").filter(|pid| *pid != second && alive(*pid)));
    assert!(owner.stop());
    let external_pid = i32::try_from(external.0.id()).unwrap();
    assert!(alive(external_pid));
}
