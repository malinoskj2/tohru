use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixDatagram;
use std::path::Path;
use std::process::Command;

use sha2::{Digest, Sha256};

const ACTION: &str = env!("CARGO_BIN_EXE_tohru-session-action");

fn bash() -> String {
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join("bash"))
        .find(|candidate| candidate.is_file())
        .unwrap()
        .display()
        .to_string()
}

/// A `systemctl` that records its arguments and reports the lock unit as `state`.
fn fake_systemctl(directory: &Path, state: &str) {
    let script = format!(
        "#!{}\nprintf '%s\\n' \"$*\" >> {}/calls\n[[ $2 == is-active ]] && {{ echo {state}; [[ {state} == active ]]; exit; }}\nexit 0\n",
        bash(),
        directory.display()
    );
    let path = directory.join("systemctl");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn endpoint(runtime: &Path) -> std::path::PathBuf {
    let digest = Sha256::digest(b"wayland-test\0");
    let hex = tohru_desktop::paths::hex(&digest);
    runtime.join(format!("tohru-lock-{}.status", &hex[..16]))
}

/// Answers one challenge the way the native locker does, or with a stale nonce.
fn locker(runtime: &Path, honest: bool) -> std::thread::JoinHandle<()> {
    let socket = UnixDatagram::bind(endpoint(runtime)).unwrap();
    std::thread::spawn(move || {
        let mut buffer = [0_u8; 64];
        let (count, peer) = socket.recv_from(&mut buffer).unwrap();
        let nonce = if honest {
            String::from_utf8_lossy(&buffer[..count]).into_owned()
        } else {
            "stale".to_owned()
        };
        let reply = format!(r#"{{"schema": 1, "nonce": "{nonce}", "held": true, "cycle": 4}}"#);
        socket.send_to_addr(reply.as_bytes(), &peer).unwrap();
    })
}

fn run(runtime: &Path, action: &str) -> (Option<i32>, String, String) {
    let path = format!("{}:{}", runtime.display(), std::env::var("PATH").unwrap());
    let output = Command::new(ACTION)
        .arg(action)
        .env("PATH", path)
        .env("XDG_RUNTIME_DIR", runtime)
        .env("WAYLAND_DISPLAY", "wayland-test")
        .env("XDG_SESSION_ID", "")
        .output()
        .unwrap();
    let calls = std::fs::read_to_string(runtime.join("calls")).unwrap_or_default();
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        calls,
    )
}

#[test]
fn sleep_suspends_only_after_the_locker_confirms_a_fresh_challenge() {
    let runtime = tempfile::tempdir().unwrap();
    fake_systemctl(runtime.path(), "active");
    let locker = locker(runtime.path(), true);
    let (code, stderr, calls) = run(runtime.path(), "sleep");
    locker.join().unwrap();
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(
        calls,
        "--user start tohru-lock.service\n--user is-active tohru-lock.service\nsuspend\n"
    );
}

#[test]
fn a_stale_reply_or_inactive_locker_never_suspends() {
    let runtime = tempfile::tempdir().unwrap();
    fake_systemctl(runtime.path(), "active");
    let locker = locker(runtime.path(), false);
    let (code, stderr, calls) = run(runtime.path(), "sleep");
    locker.join().unwrap();
    assert_eq!(code, Some(1));
    assert!(
        stderr.contains("no longer confirms the current lock cycle"),
        "{stderr}"
    );
    assert!(!calls.contains("suspend"));

    let runtime = tempfile::tempdir().unwrap();
    fake_systemctl(runtime.path(), "failed");
    let (code, stderr, calls) = run(runtime.path(), "sleep");
    assert_eq!(code, Some(1));
    assert!(stderr.contains("locker is no longer active"), "{stderr}");
    assert!(!calls.contains("suspend"));
}

#[test]
fn unknown_actions_print_usage() {
    let runtime = tempfile::tempdir().unwrap();
    let (code, stderr, _) = run(runtime.path(), "reboot");
    assert_eq!(code, Some(1));
    assert_eq!(
        stderr,
        "tohru-session-action: usage: tohru-session-action lock|sleep\n"
    );
}
