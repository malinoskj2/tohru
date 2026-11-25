use std::io::{ErrorKind, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use regex::Regex;
use serde_json::{Map, Value, json};

use crate::probe::{wait_readable, which};

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct StatusError(pub String);

fn fail<T>(message: &str) -> Result<T, StatusError> {
    Err(StatusError(message.to_owned()))
}

/// Validates one identity against its full-string pattern.
pub fn identity<'a>(
    value: Option<&'a str>,
    pattern: &str,
    name: &str,
) -> Result<&'a str, StatusError> {
    let regex = Regex::new(&format!("^(?:{pattern})$"))
        .map_err(|_| StatusError(format!("Invalid {name}")))?;
    match value {
        Some(text) if regex.is_match(text) => Ok(text),
        _ => Err(StatusError(format!("Invalid {name}"))),
    }
}

pub const DISPATCH_PATTERN: &str = "ctx_[a-z0-9]{1,120}";
pub const RUN_PATTERN: &str = "run_[a-z0-9]{1,120}";
pub const SESSION_PATTERN: &str = "[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}";
const REQUEST_PATTERN: &str = "orca:[A-Za-z0-9_:%.-]{1,507}";
const VOICE_PREFIX: &str = "[tohru-desktop:voice]\n";

/// The latest actual owner turn's request, which must be desktop voice.
pub fn voice_request(history: &Map<String, Value>) -> Result<String, StatusError> {
    let Some(entries) = history.get("entries").and_then(Value::as_array) else {
        return fail("Orca did not return conversation history");
    };
    let owner = entries.iter().rev().find(|entry| {
        entry.get("role").and_then(Value::as_str) == Some("user")
            && entry
                .get("interaction")
                .and_then(|interaction| interaction.get("kind"))
                .and_then(Value::as_str)
                == Some("owner")
    });
    let Some(owner) = owner else {
        return fail("No actual owner turn in the conversation window");
    };
    if !owner
        .get("text")
        .and_then(Value::as_str)
        .is_some_and(|text| text.starts_with(VOICE_PREFIX))
    {
        return fail("The latest owner turn is not desktop voice");
    }
    let request = owner
        .get("interaction")
        .and_then(|interaction| interaction.get("requestId"));
    identity(
        request.and_then(Value::as_str),
        REQUEST_PATTERN,
        "owner request",
    )
    .map(str::to_owned)
}

/// One confirmed Orca CLI operation.
pub trait Orca {
    fn call(&mut self, arguments: &[&str]) -> Result<Map<String, Value>, StatusError>;
}

/// The configured Orca CLI, sharing one deadline across its calls.
pub struct OrcaCli {
    executable: PathBuf,
    pub deadline: Instant,
}

const OUTPUT_LIMIT: usize = 2 * 1024 * 1024;

impl OrcaCli {
    pub fn from_environment() -> Result<Self, StatusError> {
        let configured = std::env::var("ORCA_CLI_COMMAND")
            .ok()
            .filter(|value| !value.is_empty());
        let executable = which(configured.as_deref().unwrap_or("orca-ide"))
            .ok_or_else(|| StatusError("The configured Orca CLI is unavailable".to_owned()))?;
        Ok(Self {
            executable,
            deadline: Instant::now() + Duration::from_secs(15),
        })
    }

    fn read(&self, child: &mut Child) -> Result<Vec<u8>, StatusError> {
        let Some(mut stdout) = child.stdout.take() else {
            return fail("Could not start the configured Orca CLI");
        };
        let mut output = Vec::new();
        let mut block = vec![0_u8; 65536];
        loop {
            if !wait_readable(&stdout, self.deadline) {
                return fail("Orca call timed out; do not retry speech automatically");
            }
            let room = (OUTPUT_LIMIT + 1 - output.len()).min(block.len());
            let count = match stdout.read(&mut block[..room]) {
                Ok(count) => count,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(_) => return fail("Orca call timed out; do not retry speech automatically"),
            };
            if count == 0 {
                break;
            }
            output.extend_from_slice(&block[..count]);
            if output.len() > OUTPUT_LIMIT {
                return fail("Orca output exceeded its byte limit; no automatic retry");
            }
        }
        loop {
            if let Ok(Some(status)) = child.try_wait() {
                if !status.success() {
                    return fail("Orca rejected the operation; no automatic retry");
                }
                return Ok(output);
            }
            if Instant::now() >= self.deadline {
                return fail("Orca call timed out; do not retry speech automatically");
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

impl Orca for OrcaCli {
    fn call(&mut self, arguments: &[&str]) -> Result<Map<String, Value>, StatusError> {
        if Instant::now() >= self.deadline {
            return fail("Status operation timed out; do not retry speech automatically");
        }
        let mut child = Command::new(&self.executable)
            .args(arguments)
            .arg("--json")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| StatusError("Could not start the configured Orca CLI".to_owned()))?;
        let output = self.read(&mut child);
        if matches!(child.try_wait(), Ok(None)) {
            let _ = child.kill();
        }
        let _ = child.wait();
        let envelope: Value = serde_json::from_slice(&output?)
            .map_err(|_| StatusError("Invalid Orca response".to_owned()))?;
        match (envelope.get("ok"), envelope.get("result")) {
            (Some(Value::Bool(true)), Some(Value::Object(result))) => Ok(result.clone()),
            _ => fail("Orca did not confirm the operation; no automatic retry"),
        }
    }
}

const ACK_TEXT: &str = "The task's worker is active. No completion is recorded yet.";

/// Snapshots the owner request, verifies the worker, then publishes one acknowledgement.
///
/// The request is taken before the status lookup so an intervening owner turn cannot acquire
/// this acknowledgement; native admission rechecks it.
pub fn acknowledge(
    client: &mut dyn Orca,
    session: &str,
    dispatch: &str,
    run: &str,
) -> Result<Value, StatusError> {
    let caller = format!("orca_session_id:{session}");
    let history = client.call(&["agent", "read", &caller, "--limit", "100", "--ids"])?;
    let request = voice_request(&history)?;
    let status = client.call(&["orchestration", "worker-show", "--dispatch", dispatch])?;
    let text = |key: &str| status.get(key).and_then(Value::as_str);
    if text("dispatchId") != Some(dispatch) || text("runId") != Some(run) {
        return fail("Worker identity does not match the requested dispatch and run");
    }
    if text("state") != Some("ready")
        || text("stage") != Some("input_accepted")
        || text("liveness") != Some("live")
        || text("activity") != Some("working")
    {
        return fail("Worker is not confirmed ready and active; no running-status speech");
    }
    let task = format!("dispatch:{dispatch}");
    let receipt = client.call(&[
        "bot",
        "voice-task",
        "--task",
        &task,
        "--request",
        &request,
        "--sequence",
        "1",
        "--kind",
        "ack",
        "--text",
        ACK_TEXT,
    ])?;
    let delivery = receipt.get("delivery").and_then(Value::as_str);
    if receipt.get("request").and_then(Value::as_str) != Some(request.as_str())
        || !matches!(delivery, Some("pending" | "dispatched" | "replay"))
    {
        return fail("Unrecognized speech receipt; do not retry automatically");
    }
    let mut result = json!({
        "dispatchId": dispatch,
        "taskId": status.get("taskId").cloned().unwrap_or(Value::Null),
        "runId": run,
        "state": status["state"],
        "stage": status["stage"],
    });
    if let Some(object) = result.as_object_mut() {
        object.extend(receipt);
    }
    Ok(result)
}

/// The whole command: identity checks before any CLI call.
pub fn run(dispatch: &str, run_id: &str) -> Result<Value, StatusError> {
    let dispatch = identity(Some(dispatch), DISPATCH_PATTERN, "dispatch")?;
    let run_id = identity(Some(run_id), RUN_PATTERN, "run")?;
    let session = std::env::var("ORCA_AGENT_SESSION_ID").ok();
    let session = identity(session.as_deref(), SESSION_PATTERN, "injected caller")?;
    let remote = std::env::var("ORCA_PAIRING_CODE").is_ok_and(|code| !code.is_empty())
        || !matches!(
            std::env::var("ORCA_ENVIRONMENT").as_deref(),
            Err(_) | Ok("" | "local")
        );
    if remote {
        return fail("Voice status requires the existing local coordinating bot");
    }
    acknowledge(&mut OrcaCli::from_environment()?, session, dispatch, run_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REQUEST: &str = "orca:1795437663530-current";

    struct Fake {
        calls: Vec<Vec<String>>,
        entries: Vec<Value>,
        status: Map<String, Value>,
        receipt: Map<String, Value>,
        rejected: bool,
    }

    impl Fake {
        fn new() -> Self {
            let object = |value: Value| value.as_object().cloned().unwrap_or_default();
            Self {
                calls: Vec::new(),
                entries: vec![
                    json!({"role": "user", "text": "[tohru-desktop:voice]\nHow is it going?",
                    "interaction": {"kind": "owner", "requestId": REQUEST}}),
                ],
                status: object(
                    json!({"dispatchId": "ctx_owned", "runId": "run_owned", "taskId": "task_owned",
                    "state": "ready", "stage": "input_accepted", "liveness": "live", "activity": "working"}),
                ),
                receipt: object(json!({"request": REQUEST, "delivery": "dispatched"})),
                rejected: false,
            }
        }
    }

    impl Orca for Fake {
        fn call(&mut self, arguments: &[&str]) -> Result<Map<String, Value>, StatusError> {
            self.calls.push(
                arguments
                    .iter()
                    .map(|argument| (*argument).to_owned())
                    .collect(),
            );
            match arguments[0] {
                "agent" => Ok(json!({"entries": self.entries})
                    .as_object()
                    .cloned()
                    .unwrap_or_default()),
                "orchestration" => Ok(self.status.clone()),
                _ if self.rejected => {
                    fail("native admission rejected current request or ownership")
                }
                _ => Ok(self.receipt.clone()),
            }
        }
    }

    #[test]
    fn success_binds_snapshot_and_preserves_native_receipt() {
        let mut client = Fake::new();
        let result = acknowledge(&mut client, "home", "ctx_owned", "run_owned").unwrap();
        assert_eq!(result["delivery"], "dispatched");
        let last = client.calls.last().unwrap();
        assert_eq!(last[6..8], ["--sequence", "1"]);
        assert_eq!(last[4..6], ["--request", REQUEST]);
        assert_eq!(client.calls.len(), 3);
    }

    #[test]
    fn internal_mail_does_not_replace_actual_owner() {
        let mut client = Fake::new();
        client.entries.push(
            json!({"role": "user", "text": "worker mail", "interaction": {"kind": "internal"}}),
        );
        acknowledge(&mut client, "home", "ctx_owned", "run_owned").unwrap();
        assert_eq!(client.calls.last().unwrap()[5], REQUEST);
    }

    #[test]
    fn terminal_unknown_foreign_and_unready_fail_before_publish() {
        for (key, value) in [
            ("state", "succeeded"),
            ("state", "failed"),
            ("state", "stopped"),
            ("state", "starting"),
            ("state", "accepted"),
            ("stage", "unknown"),
            ("activity", "idle"),
            ("liveness", "unknown"),
            ("dispatchId", "ctx_foreign"),
            ("runId", "run_foreign"),
        ] {
            let mut client = Fake::new();
            client.status.insert(key.to_owned(), Value::from(value));
            assert!(
                acknowledge(&mut client, "home", "ctx_owned", "run_owned").is_err(),
                "{key}={value}"
            );
            assert_eq!(client.calls.len(), 2);
        }
    }

    #[test]
    fn malformed_or_typed_owner_does_not_query_worker() {
        for entries in [
            vec![],
            vec![json!({"role": "user", "interaction": {"kind": "owner"}, "text": "typed"})],
            vec![
                json!({"role": "user", "interaction": {"kind": "owner", "requestId": "invented"},
                "text": "[tohru-desktop:voice]\nhello"}),
            ],
        ] {
            let mut client = Fake::new();
            client.entries = entries;
            assert!(acknowledge(&mut client, "home", "ctx_owned", "run_owned").is_err());
            assert_eq!(client.calls.len(), 1);
        }
    }

    #[test]
    fn native_rejection_never_retries_or_falls_back_to_latest() {
        let mut client = Fake::new();
        client.rejected = true;
        assert!(acknowledge(&mut client, "home", "ctx_owned", "run_owned").is_err());
        assert_eq!(client.calls.len(), 3);
        assert!(
            client
                .calls
                .last()
                .unwrap()
                .iter()
                .any(|argument| argument == REQUEST)
        );
    }

    #[test]
    fn replay_and_hidden_pending_remain_native_receipts() {
        for delivery in ["replay", "pending"] {
            let mut client = Fake::new();
            client
                .receipt
                .insert("delivery".to_owned(), Value::from(delivery));
            let result = acknowledge(&mut client, "home", "ctx_owned", "run_owned").unwrap();
            assert_eq!(result["delivery"], delivery);
            assert_eq!(client.calls.len(), 3);
        }
    }
}
