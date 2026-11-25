use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

const STATUS: &str = env!("CARGO_BIN_EXE_tohru-voice-status");
const REQUEST: &str = "orca:1795437663530-current";

/// A fake Orca CLI: logs each call and answers from fixture files in its directory.
const FAKE: &str = r#"
dir=$(dirname "$0")
printf '%s\n' "$*" >> "$dir/calls"
behavior=$(cat "$dir/behavior" 2>/dev/null)
case $behavior in
    oversize) head -c 3145728 /dev/zero; sleep 30 ;;
    timeout) sleep 30 ;;
    malformed) echo 'not json'; exit 0 ;;
esac
case "$1 $2" in
    'agent read') cat "$dir/agent.json" ;;
    'orchestration worker-show') cat "$dir/status.json" ;;
    *)
        request=
        while [[ $# -gt 0 ]]; do [[ $1 == --request ]] && request=$2; shift; done
        if [[ -e $dir/reject || $request != "$(cat "$dir/current")" ]]; then
            echo '{"ok": false, "error": "native rejection"}'; exit 1
        fi
        cat "$dir/receipt.json" ;;
esac
"#;

fn bash() -> String {
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join("bash"))
        .find(|candidate| candidate.is_file())
        .unwrap()
        .display()
        .to_string()
}

struct Fixture {
    directory: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let cli = root.join("orca-test");
        std::fs::write(&cli, format!("#!{}{FAKE}", bash())).unwrap();
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(root.join("agent.json"), format!(r#"{{"ok": true, "result": {{"entries": [{{"role": "user", "text": "[tohru-desktop:voice]\nHow is it going?", "interaction": {{"kind": "owner", "requestId": "{REQUEST}"}}}}]}}}}"#)).unwrap();
        std::fs::write(root.join("status.json"), r#"{"ok": true, "result": {"dispatchId": "ctx_owned", "runId": "run_owned", "taskId": "task_owned", "state": "ready", "stage": "input_accepted", "liveness": "live", "activity": "working"}}"#).unwrap();
        std::fs::write(
            root.join("receipt.json"),
            format!(
                r#"{{"ok": true, "result": {{"request": "{REQUEST}", "delivery": "dispatched"}}}}"#
            ),
        )
        .unwrap();
        std::fs::write(root.join("current"), REQUEST).unwrap();
        Self { directory }
    }

    fn path(&self, name: &str) -> std::path::PathBuf {
        self.directory.path().join(name)
    }

    fn invoke(&self, extra: &[(&str, &str)], args: &[&str]) -> (Option<i32>, Value) {
        let mut command = Command::new(STATUS);
        command
            .args(["--dispatch", "ctx_owned", "--run", "run_owned"])
            .args(args)
            .env(
                "ORCA_AGENT_SESSION_ID",
                "12345678-1234-4234-8234-123456789abc",
            )
            .env("ORCA_CLI_COMMAND", self.path("orca-test"))
            .env("ORCA_ENVIRONMENT", "local")
            .env_remove("ORCA_PAIRING_CODE");
        for (key, value) in extra {
            command.env(key, value);
        }
        let output = command.output().unwrap();
        (
            output.status.code(),
            serde_json::from_slice(&output.stdout).unwrap_or(Value::Null),
        )
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.path("calls"))
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_default()
    }
}

#[test]
fn actual_cli_process_passes_snapshot_request_once() {
    let fixture = Fixture::new();
    let (code, output) = fixture.invoke(&[], &[]);
    assert_eq!(code, Some(0), "{output}");
    assert_eq!(output["result"]["request"], REQUEST);
    assert_eq!(output["result"]["taskId"], "task_owned");
    let calls = fixture.calls();
    assert_eq!(calls.len(), 3);
    assert!(calls[2].contains(REQUEST) && calls[2].ends_with("--json"));
    assert!(!calls[2].contains("--from"));
}

#[test]
fn new_owner_or_foreign_admission_never_retries() {
    for rejection in ["current", "reject"] {
        let fixture = Fixture::new();
        std::fs::write(fixture.path(rejection), "orca:new-owner").unwrap();
        let (code, output) = fixture.invoke(&[], &[]);
        assert_eq!(code, Some(1));
        assert_eq!(output["ok"], false);
        let voice = fixture
            .calls()
            .iter()
            .filter(|call| call.starts_with("bot voice-task"))
            .count();
        assert_eq!(voice, 1);
    }
}

#[test]
fn bad_identity_missing_caller_and_remote_fail_before_cli() {
    for (key, value) in [
        ("ORCA_AGENT_SESSION_ID", ""),
        ("ORCA_PAIRING_CODE", "remote"),
        ("ORCA_ENVIRONMENT", "ssh:remote"),
    ] {
        let fixture = Fixture::new();
        assert_eq!(fixture.invoke(&[(key, value)], &[]).0, Some(1));
        assert!(fixture.calls().is_empty());
    }
    let fixture = Fixture::new();
    assert_eq!(
        fixture.invoke(&[], &["--dispatch", "../../foreign"]).0,
        Some(1)
    );
    assert!(fixture.calls().is_empty());
}

#[test]
fn oversized_slow_and_malformed_output_fail_without_retry() {
    for (behavior, message) in [
        ("oversize", "byte limit"),
        ("malformed", "Invalid Orca response"),
    ] {
        let fixture = Fixture::new();
        std::fs::write(fixture.path("behavior"), behavior).unwrap();
        let started = std::time::Instant::now();
        let (code, output) = fixture.invoke(&[], &[]);
        assert!(started.elapsed().as_secs() < 3);
        assert_eq!(code, Some(1));
        assert!(
            output["error"].as_str().unwrap().contains(message),
            "{output}"
        );
        assert_eq!(fixture.calls().len(), 1);
    }
    assert!(Path::new(STATUS).exists());
}
