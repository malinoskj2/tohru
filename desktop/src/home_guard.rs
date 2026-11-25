use std::io::Write;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use rustix::process::{Pid, Signal, kill_process_group};
use serde_json::{Map, Value, json};

use crate::shell::{self, Token};

pub const REFUSAL: &str = "Home has a five-second foreground work budget. Use Orca worker-start --background or Bash run_in_background=true, then end the turn; never use internal subagents.";
pub const BUDGET_LIMIT_MS: u64 = 5000;
const QUICK_TOOLS: [&str; 4] = ["Read", "Glob", "Grep", "ToolSearch"];
const SEPARATORS: [&str; 8] = [";", "&&", "||", "|", "&", "(", ")", "\n"];
const SLOW_PROGRAMS: [&str; 9] = [
    "make", "cmake", "ninja", "pytest", "curl", "wget", "sleep", "gpu-run", "gpu-game",
];
const PREFIXES: [&str; 9] = [
    "env", "command", "time", "then", "do", "if", "while", "until", "!",
];

fn slow_verbs(program: &str) -> &'static [&'static str] {
    match program {
        "cargo" => &["build", "test", "bench", "run", "install", "clippy"],
        "nix" => &["build", "develop", "shell", "run", "flake"],
        "git" => &["clone", "fetch", "pull"],
        "pip" | "pip3" => &["install", "download"],
        "uv" => &["sync", "pip", "run"],
        "npm" => &["install", "ci", "test", "run"],
        "pnpm" => &["install", "test", "run", "exec"],
        _ => &[],
    }
}

/// What the hook does with one tool call.
#[derive(Debug, PartialEq)]
pub enum Plan {
    Pass,
    PassBackground,
    Deny(String),
    Wrap(Map<String, Value>),
}

impl Plan {
    #[must_use]
    pub fn to_json(&self) -> Value {
        match self {
            Self::Pass => json!({"decision": "pass"}),
            Self::PassBackground => json!({"decision": "pass", "background": true}),
            Self::Deny(reason) => json!({"decision": "deny", "reason": reason}),
            Self::Wrap(input) => json!({"decision": "wrap", "input": input}),
        }
    }
}

fn refuse() -> Plan {
    Plan::Deny(REFUSAL.to_owned())
}

fn within_budget(remaining_ms: i64) -> bool {
    remaining_ms > 0 && remaining_ms.unsigned_abs() <= BUDGET_LIMIT_MS
}

/// Decides one Home tool call against the remaining foreground budget.
#[must_use]
pub fn plan(tool: Option<&str>, value: Option<&Value>, remaining_ms: i64, guard: &Path) -> Plan {
    match tool {
        Some("AskUserQuestion") => return Plan::Pass,
        Some(name) if QUICK_TOOLS.contains(&name) => {
            return if within_budget(remaining_ms) {
                Plan::Pass
            } else {
                refuse()
            };
        }
        Some("Bash") => {}
        _ => return refuse(),
    }
    let Some(input) = value.and_then(Value::as_object) else {
        return refuse();
    };
    let Some(command) = input.get("command").and_then(Value::as_str) else {
        return refuse();
    };
    if input.get("run_in_background") == Some(&Value::Bool(true)) {
        return Plan::PassBackground;
    }
    if !within_budget(remaining_ms) {
        return refuse();
    }
    let Ok(tokens) = shell::tokens(command) else {
        return Plan::Deny(format!("Invalid shell command. {REFUSAL}"));
    };
    if denied(command, &tokens) {
        return refuse();
    }
    let wrapped = shell::join(&[
        guard.to_string_lossy().as_ref(),
        "run",
        "--budget-ms",
        &remaining_ms.to_string(),
        "--command",
        command,
    ]);
    let mut updated = input.clone();
    updated.insert("command".to_owned(), Value::String(wrapped));
    updated.insert("timeout".to_owned(), Value::from(remaining_ms + 1500));
    Plan::Wrap(updated)
}

fn denied(command: &str, tokens: &[Token]) -> bool {
    let mut words = Vec::new();
    let mut segments: Vec<Vec<&str>> = vec![Vec::new()];
    for token in tokens {
        match token {
            Token::Control(control) if SEPARATORS.contains(&control.as_str()) || control == "`" => {
                segments.push(Vec::new());
            }
            Token::Word(word) | Token::Control(word) => {
                words.push(word.as_str());
                if let Some(segment) = segments.last_mut() {
                    segment.push(word.as_str());
                }
            }
        }
    }
    for segment in &segments {
        let mut arguments = segment.as_slice();
        while let Some((first, rest)) = arguments.split_first() {
            if first.contains('=') || PREFIXES.contains(first) {
                arguments = rest;
            } else {
                break;
            }
        }
        let Some((first, rest)) = arguments.split_first() else {
            continue;
        };
        let program = Path::new(first).file_name().map_or_else(
            || (*first).to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        let verbs = slow_verbs(&program);
        if SLOW_PROGRAMS.contains(&program.as_str()) || rest.iter().any(|word| verbs.contains(word))
        {
            return true;
        }
    }
    let has = |word: &str| words.contains(&word);
    if has("worker-start") && !has("--background") {
        return true;
    }
    if has("wait") || (has("check") && has("--wait")) {
        return true;
    }
    has("worker-show")
        && ["while ", "until ", "for "]
            .iter()
            .any(|loop_word| command.contains(loop_word))
}

const DEADLINE_NOTICE: &str = "The foreground deadline expired. The command may have taken effect; inspect its existing receipt or shared records before retrying.";

/// Runs `command` under bash in its own process group, killing the group at the budget.
pub fn foreground(command: &str, budget_ms: u64) -> anyhow::Result<i32> {
    let started = Instant::now();
    let mut child = Command::new("bash")
        .arg("-c")
        .arg(command)
        .process_group(0)
        .spawn()?;
    let group = Pid::from_raw(child.id().cast_signed());
    let mut signals = signal_hook::iterator::Signals::new([
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
    ])?;
    let handle = signals.handle();
    let forward = std::thread::spawn(move || {
        for _ in signals.forever() {
            if let Some(group) = group {
                let _ = kill_process_group(group, Signal::TERM);
            }
        }
    });
    let deadline = started + Duration::from_millis(budget_ms);
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            if let Some(group) = group {
                let _ = kill_process_group(group, Signal::TERM);
            }
            let grace = Instant::now() + Duration::from_secs(1);
            while child.try_wait()?.is_none() && Instant::now() < grace {
                std::thread::sleep(Duration::from_millis(10));
            }
            if child.try_wait()?.is_none()
                && let Some(group) = group
            {
                let _ = kill_process_group(group, Signal::KILL);
            }
            break child.wait()?;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    // Ordinary foreground commands may not leave untracked descendants behind.
    if let Some(group) = group {
        let _ = kill_process_group(group, Signal::KILL);
    }
    handle.close();
    let _ = forward.join();
    let code = if timed_out {
        124
    } else {
        status
            .code()
            .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
    };
    let mut stderr = std::io::stderr().lock();
    if timed_out {
        let _ = writeln!(stderr, "{DEADLINE_NOTICE}");
        let _ = writeln!(stderr, "{REFUSAL}");
    }
    let duration = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let _ = writeln!(
        stderr,
        "{}",
        crate::pyjson::dumps(
            &json!({"event": "home_tool", "duration_ms": duration, "timed_out": timed_out, "exit_code": code})
        )
    );
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decide(tool: &str, value: &Value, remaining: i64) -> Plan {
        plan(
            Some(tool),
            Some(value),
            remaining,
            Path::new("/bin/tohru-home-guard"),
        )
    }

    fn decision(plan: &Plan) -> &'static str {
        match plan {
            Plan::Pass | Plan::PassBackground => "pass",
            Plan::Deny(_) => "deny",
            Plan::Wrap(_) => "wrap",
        }
    }

    #[test]
    fn known_slow_work_and_internal_subagents_are_denied_before_execution() {
        for command in [
            "cargo test",
            "cargo --color never test",
            "nix build .",
            "npm test",
            "curl https://example.com",
            "sleep 30",
            "orca orchestration worker-start --spec work",
            "orca orchestration check --wait",
            "true\nsleep 9",
            "echo `sleep 9`",
            "echo $(sleep 9)",
            "FOO=1 env /usr/bin/make",
        ] {
            assert_eq!(
                decision(&decide("Bash", &json!({"command": command}), 5000)),
                "deny",
                "{command}"
            );
        }
        assert_eq!(
            decision(&decide("Agent", &json!({"prompt": "do work"}), 5000)),
            "deny"
        );
    }

    #[test]
    fn background_handles_and_owner_questions_do_not_block_the_reply() {
        assert_eq!(
            decide(
                "Bash",
                &json!({"command": "cargo test", "run_in_background": true}),
                0
            ),
            Plan::PassBackground
        );
        assert_eq!(decide("AskUserQuestion", &json!({}), 0), Plan::Pass);
        let background = decide(
            "Bash",
            &json!({"command": "orca orchestration worker-start --background --spec work"}),
            5000,
        );
        assert_eq!(decision(&background), "wrap");
        assert_eq!(
            decision(&decide(
                "Bash",
                &json!({"command": "printf 'cargo test'"}),
                5000
            )),
            "wrap"
        );
        assert_eq!(
            decision(&decide("Bash", &json!({"command": "printf hello"}), 0)),
            "deny"
        );
        assert_eq!(
            decision(&decide("Bash", &json!({"command": "printf 'open"}), 5000)),
            "deny"
        );
    }

    #[test]
    fn quick_tools_pass_only_inside_the_budget() {
        for tool in QUICK_TOOLS {
            assert_eq!(decide(tool, &json!({}), 5000), Plan::Pass);
            assert_eq!(decision(&decide(tool, &json!({}), 0)), "deny");
        }
    }

    #[test]
    fn wrapping_keeps_other_fields_and_bounds_the_tool_timeout() {
        let Plan::Wrap(input) = decide(
            "Bash",
            &json!({"command": "printf hi", "description": "say"}),
            3000,
        ) else {
            panic!("expected a wrap");
        };
        assert_eq!(input["description"], "say");
        assert_eq!(input["timeout"], 4500);
        assert_eq!(
            input["command"],
            "/bin/tohru-home-guard run --budget-ms 3000 --command 'printf hi'"
        );
    }
}
