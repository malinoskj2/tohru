use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const GUARD: &str = env!("CARGO_BIN_EXE_tohru-home-guard");

fn hook(tool: &str, input: &Value, remaining: i64) -> Value {
    let mut child = Command::new(GUARD)
        .args(["plan", "--remaining-ms", &remaining.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(json!({"tool": tool, "input": input}).to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn wrapped_foreground_bash_runs_through_the_installed_guard() {
    let decision = hook("Bash", &json!({"command": "printf guarded"}), 5000);
    assert_eq!(decision["decision"], "wrap");
    let command = decision["input"]["command"].as_str().unwrap();
    assert!(command.starts_with(&format!(
        "{} run",
        std::fs::canonicalize(GUARD).unwrap().display()
    )));
    let result = Command::new("bash").args(["-c", command]).output().unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout, b"guarded");
    assert!(String::from_utf8_lossy(&result.stderr).contains(r#""event": "home_tool""#));
}

#[test]
fn quick_tools_pass_before_any_foreground_work() {
    for tool in ["Read", "Glob", "Grep", "ToolSearch"] {
        assert_eq!(hook(tool, &json!({}), 5000), json!({"decision": "pass"}));
        assert_eq!(hook(tool, &json!({}), 0)["decision"], "deny");
    }
    assert_eq!(
        hook("Agent", &json!({"prompt": "work"}), 5000)["decision"],
        "deny"
    );
}

#[test]
fn unexpected_foreground_overrun_is_killed() {
    let started = Instant::now();
    let result = Command::new(GUARD)
        .args([
            "run",
            "--budget-ms",
            "100",
            "--command",
            "sleep 5; echo late",
        ])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(124));
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains(r#""timed_out": true"#));
}

#[test]
fn foreground_output_and_exit_status_are_preserved() {
    let result = Command::new(GUARD)
        .args([
            "run",
            "--budget-ms",
            "1000",
            "--command",
            "printf '%010000d' 0; exit 3",
        ])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(3));
    assert_eq!(result.stdout.len(), 10000);
}

#[test]
fn budgets_outside_five_seconds_are_rejected() {
    for budget in ["0", "5001"] {
        let result = Command::new(GUARD)
            .args(["run", "--budget-ms", budget, "--command", "true"])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2));
    }
}
