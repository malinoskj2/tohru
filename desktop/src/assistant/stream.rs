use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write as _};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use rustix::process::{Pid, Signal};
use serde_json::{Map, Value};

use super::{Result, private_dir, scratch_dir};
use crate::pyvalue::{py_str, truthy};

/// Turns one CLI's stdout lines into answer text and its resumable session id.
#[derive(Debug, Default)]
pub struct StreamParser {
    ident: String,
    pub text: String,
    pub session: Option<String>,
    pub error: Option<String>,
    pub final_text: Option<String>,
}

fn get<'a>(event: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    event.get(key)
}

fn text_of(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        None | Some(Value::Null) => String::new(),
        Some(other) => py_str(Some(other)),
    }
}

fn or_text(values: &[Option<&Value>], fallback: &str) -> String {
    values
        .iter()
        .find(|value| truthy(**value))
        .map_or_else(|| fallback.to_owned(), |value| py_str(*value))
}

fn object(value: Option<&Value>) -> Map<String, Value> {
    match value {
        Some(Value::Object(object)) => object.clone(),
        _ => Map::new(),
    }
}

impl StreamParser {
    #[must_use]
    pub fn new(ident: &str) -> Self {
        Self {
            ident: ident.to_owned(),
            ..Self::default()
        }
    }

    pub fn feed(&mut self, line: &str) -> String {
        let Ok(Value::Object(event)) =
            serde_json::from_str::<Value>(line.trim_end_matches(['\n', '\r']))
        else {
            return String::new();
        };
        let delta = match self.ident.as_str() {
            "claude" => self.claude(&event),
            "codex" => self.codex(&event),
            "gemini" => self.gemini(&event),
            "agy" => self.agy(&event),
            _ => self.opencode(&event),
        };
        self.text.push_str(&delta);
        delta
    }

    pub fn finish(&mut self) -> String {
        let Some(final_text) = self.final_text.clone().filter(|text| !text.is_empty()) else {
            return String::new();
        };
        if self.text.trim().is_empty() {
            self.text.clone_from(&final_text);
            return final_text;
        }
        if final_text.starts_with(&self.text) && final_text.len() > self.text.len() {
            let rest = final_text[self.text.len()..].to_owned();
            self.text = final_text;
            return rest;
        }
        String::new()
    }

    fn claude(&mut self, event: &Map<String, Value>) -> String {
        if truthy(get(event, "session_id")) {
            self.session = Some(py_str(get(event, "session_id")));
        }
        match get(event, "type").and_then(Value::as_str) {
            Some("stream_event") => {
                let inner = object(get(event, "event"));
                let kind = inner.get("type").and_then(Value::as_str);
                if kind == Some("message_start")
                    && !self.text.is_empty()
                    && !self.text.ends_with('\n')
                {
                    return "\n\n".to_owned();
                }
                if kind == Some("content_block_delta") {
                    let delta = object(inner.get("delta"));
                    if delta.get("type").and_then(Value::as_str) == Some("text_delta") {
                        return text_of(delta.get("text"));
                    }
                }
            }
            Some("result") => {
                let subtype = get(event, "subtype");
                let fine = matches!(subtype, None | Some(Value::Null))
                    || subtype.and_then(Value::as_str) == Some("success");
                if truthy(get(event, "is_error")) || !fine {
                    self.error = Some(or_text(&[get(event, "result"), subtype], "error"));
                } else if let Some(Value::String(result)) = get(event, "result") {
                    self.final_text = Some(result.clone());
                }
            }
            _ => {}
        }
        String::new()
    }

    fn codex(&mut self, event: &Map<String, Value>) -> String {
        match get(event, "type").and_then(Value::as_str) {
            Some("thread.started") => {
                self.session = get(event, "thread_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
            Some("item.completed") => {
                let item = object(get(event, "item"));
                if item.get("type").and_then(Value::as_str) == Some("agent_message") {
                    let text = text_of(item.get("text"));
                    // A completed message is a safe speech boundary even while the turn continues.
                    if !text.is_empty() {
                        return format!("{}{text}\n", if self.text.is_empty() { "" } else { "\n" });
                    }
                }
            }
            Some("turn.failed" | "error") => {
                let error = [get(event, "error"), get(event, "message")]
                    .into_iter()
                    .find(|value| truthy(*value))
                    .flatten()
                    .cloned()
                    .unwrap_or_else(|| Value::String("error".to_owned()));
                self.error = Some(match &error {
                    Value::Object(error) => error.get("message").map_or_else(
                        || py_str(Some(&Value::Object(error.clone()))),
                        |message| py_str(Some(message)),
                    ),
                    other => py_str(Some(other)),
                });
            }
            _ => {}
        }
        String::new()
    }

    fn gemini(&mut self, event: &Map<String, Value>) -> String {
        match get(event, "type").and_then(Value::as_str) {
            Some("init") if truthy(get(event, "session_id")) => {
                self.session = Some(py_str(get(event, "session_id")));
            }
            Some("message") if get(event, "role").and_then(Value::as_str) == Some("assistant") => {
                return text_of(get(event, "content"));
            }
            Some("error") => self.error = Some(or_text(&[get(event, "message")], "error")),
            _ => {}
        }
        String::new()
    }

    fn agy(&mut self, event: &Map<String, Value>) -> String {
        match get(event, "event").and_then(Value::as_str) {
            Some("init") if truthy(get(event, "conversation_id")) => {
                self.session = Some(py_str(get(event, "conversation_id")));
            }
            Some("step_update") => {
                let step = object(get(event, "step_update"));
                if step.get("step_type").and_then(Value::as_str) == Some("agent_response") {
                    return text_of(step.get("text_delta"));
                }
            }
            Some("result") => {
                let result = object(get(event, "result"));
                let status = result.get("status");
                let fine = matches!(status, None | Some(Value::Null))
                    || status.and_then(Value::as_str) == Some("SUCCESS");
                if !fine {
                    self.error = Some(or_text(&[result.get("error"), status], "None"));
                } else if let Some(Value::String(response)) = result.get("response") {
                    self.final_text = Some(response.clone());
                }
            }
            _ => {}
        }
        String::new()
    }

    fn opencode(&mut self, event: &Map<String, Value>) -> String {
        if truthy(get(event, "sessionID")) {
            self.session = Some(py_str(get(event, "sessionID")));
        }
        let part = object(get(event, "part"));
        let kind = get(event, "type").and_then(Value::as_str);
        if kind == Some("text")
            && let Some(Value::String(text)) = part.get("text")
        {
            return format!("{}{text}", if self.text.is_empty() { "" } else { "\n\n" });
        }
        if kind == Some("error") {
            self.error = Some(match get(event, "error") {
                Some(Value::Object(error)) => {
                    or_text(&[error.get("message"), error.get("name")], "error")
                }
                None | Some(Value::Null) => "{}".to_owned(),
                Some(other) => py_str(Some(other)),
            });
        }
        String::new()
    }
}

type Cancel = Box<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct RunState {
    child: Option<u32>,
    timers: bool,
    cancel_tools: Option<Arc<Cancel>>,
}

/// A stoppable provider run shared with signal handlers and timers.
#[derive(Clone, Default)]
pub struct Run {
    state: Arc<Mutex<RunState>>,
    stopped: Arc<AtomicBool>,
    stop_grace: Option<f64>,
}

impl Run {
    #[must_use]
    pub fn new(stop_grace: Option<f64>) -> Self {
        Self {
            stop_grace,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    pub fn set_cancel_tools(&self, cancel: Option<Cancel>) {
        self.lock().cancel_tools = cancel.map(Arc::new);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, RunState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn signal(&self, pid: u32, how: Signal) {
        let state = self.lock();
        if state.child == Some(pid)
            && let Some(group) = i32::try_from(pid).ok().and_then(Pid::from_raw)
        {
            let _ = rustix::process::kill_process_group(group, how);
        }
    }

    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        let cancel = self.lock().cancel_tools.clone();
        if let Some(cancel) = cancel {
            cancel();
        }
        let Some(pid) = self.lock().child else {
            return;
        };
        self.signal(pid, Signal::INT);
        let Some(grace) = self.stop_grace else {
            return;
        };
        let start = {
            let mut state = self.lock();
            !std::mem::replace(&mut state.timers, true)
        };
        if start {
            for (seconds, how) in [(grace, Signal::TERM), (grace + 1.0, Signal::KILL)] {
                let run = self.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs_f64(seconds.max(0.0)));
                    run.signal(pid, how);
                });
            }
        }
    }

    /// Stops this run on SIGINT or SIGTERM until the returned guard drops.
    pub fn on_signals(&self) -> Result<SignalGuard> {
        let run = self.clone();
        on_signals(Arc::new(move |_| run.stop()))
    }

    /// As [`Run::on_signals`], noting the interruption first.
    pub fn on_signals_with(&self, note: impl Fn() + Send + Sync + 'static) -> Result<SignalGuard> {
        let run = self.clone();
        on_signals(Arc::new(move |_| {
            note();
            run.stop();
        }))
    }
}

/// Called with the signal number.
pub type Handler = Arc<dyn Fn(i32) + Send + Sync>;

static HANDLER: Mutex<Option<Handler>> = Mutex::new(None);
static DISPATCH: std::sync::Once = std::sync::Once::new();

/// Routes SIGINT and SIGTERM to `handler` until the guard drops, then to the previous handler;
/// with none installed they act as they would by default.
pub fn on_signals(handler: Handler) -> Result<SignalGuard> {
    let mut failure = None;
    DISPATCH.call_once(|| {
        let signals = signal_hook::iterator::Signals::new([
            signal_hook::consts::SIGINT,
            signal_hook::consts::SIGTERM,
        ]);
        match signals {
            Ok(mut signals) => {
                std::thread::spawn(move || {
                    for signal in signals.forever() {
                        let current = HANDLER
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .clone();
                        match current {
                            Some(handler) => handler(signal),
                            None => {
                                let _ = signal_hook::low_level::emulate_default_handler(signal);
                            }
                        }
                    }
                });
            }
            Err(error) => failure = Some(error),
        }
    });
    if let Some(error) = failure {
        return Err(error.into());
    }
    let previous = HANDLER
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .replace(handler);
    Ok(SignalGuard(previous))
}

pub struct SignalGuard(Option<Handler>);

impl Drop for SignalGuard {
    fn drop(&mut self) {
        *HANDLER.lock().unwrap_or_else(PoisonError::into_inner) = self.0.take();
    }
}

/// One finished provider turn.
pub struct Outcome {
    pub parser: StreamParser,
    pub code: Option<i32>,
    pub errors: Vec<String>,
    pub stopped: bool,
}

/// Names Orca exports into its own terminals; never inherited by Tohru's provider runs.
fn orca_free_env() -> HashMap<std::ffi::OsString, std::ffi::OsString> {
    super::scrubbed_env()
        .into_iter()
        .filter(|(key, _)| !key.to_string_lossy().starts_with("ORCA_"))
        .collect()
}

pub struct Launch<'a> {
    pub ident: &'a str,
    pub argv: &'a [String],
    pub stdin: &'a str,
    pub timeout: Option<f64>,
    pub extra_env: &'a [(String, String)],
}

/// Runs one headless CLI turn in the scratch folder.
pub fn run_provider(
    launch: &Launch<'_>,
    run: &Run,
    on_delta: &mut dyn FnMut(&str),
) -> Result<Outcome> {
    let scratch = private_dir(&scratch_dir())?;
    let mut parser = StreamParser::new(launch.ident);
    let mut env = orca_free_env();
    env.insert("NO_COLOR".into(), "1".into());
    for (key, value) in launch.extra_env {
        env.insert(key.into(), value.into());
    }
    // Tohru keeps her own memory; Claude Code's would be a second one other providers never see.
    env.insert("CLAUDE_CODE_DISABLE_AUTO_MEMORY".into(), "1".into());
    let (program, args) = launch
        .argv
        .split_first()
        .ok_or_else(|| super::AssistantError::exit("tohru-assistant: empty command"))?;
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(&scratch)
        .env_clear()
        .envs(env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command.spawn()?;
    run.lock().child = Some(child.id());
    let deadline = launch.timeout.map(|seconds| {
        let run = run.clone();
        let done = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&done);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs_f64(seconds.max(0.0)));
            if !flag.load(Ordering::SeqCst) {
                run.stop();
            }
        });
        done
    });
    let (sender, errors) = std::sync::mpsc::channel();
    if let Some(stderr) = child.stderr.take() {
        std::thread::spawn(move || {
            let lines: Vec<String> = BufReader::new(stderr)
                .split(b'\n')
                .map_while(std::result::Result::ok)
                .map(|line| format!("{}\n", String::from_utf8_lossy(&line)))
                .collect();
            let skip = lines.len().saturating_sub(20);
            let _ = sender.send(lines[skip..].to_vec());
        });
    }
    if let Some(mut stdin) = child.stdin.take() {
        let text = launch.stdin.to_owned();
        std::thread::spawn(move || {
            let _ = stdin.write_all(text.as_bytes());
        });
    }
    if run.stopped() {
        run.stop();
    }
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout)
            .split(b'\n')
            .map_while(std::result::Result::ok)
        {
            let delta = parser.feed(&String::from_utf8_lossy(&line));
            if !delta.is_empty() {
                on_delta(&delta);
            }
        }
    }
    let status = child.wait()?;
    run.lock().child = None;
    if let Some(done) = deadline {
        done.store(true, Ordering::SeqCst);
    }
    // A helper that inherited stderr may hold it open; wait briefly, as Python joins for a second.
    let errors = errors
        .recv_timeout(Duration::from_secs(1))
        .unwrap_or_default();
    let stopped = run.stopped();
    let rest = if stopped {
        String::new()
    } else {
        parser.finish()
    };
    if !rest.is_empty() {
        on_delta(&rest);
    }
    Ok(Outcome {
        parser,
        code: status.code().or_else(|| signal_code(status)),
        errors,
        stopped,
    })
}

/// Python reports a signalled child's return code as the negated signal number.
fn signal_code(status: std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal().map(|signal| -signal)
}

/// Where a provider takes the prompt: the final `-p` value, an extra argument, or stdin.
pub fn place_prompt(ident: &str, argv: &mut Vec<String>, text: &str) -> String {
    match ident {
        "gemini" => {
            if let Some(last) = argv.last_mut() {
                text.clone_into(last);
            }
            String::new()
        }
        "agy" | "opencode" => {
            argv.push(text.to_owned());
            String::new()
        }
        _ => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::io::BufRead as _;
    use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
    use std::process::{Command, Stdio};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use super::Run;

    #[test]
    fn stopping_escalates_to_kill_for_a_provider_ignoring_interrupts() {
        let mut child = Command::new("bash")
            .args([
                "-c",
                "trap '' INT TERM; echo ready; while :; do sleep 0.05; done",
            ])
            .stdout(Stdio::piped())
            .process_group(0)
            .spawn()
            .expect("spawn bash");
        let mut ready = String::new();
        std::io::BufReader::new(child.stdout.take().expect("stdout"))
            .read_line(&mut ready)
            .expect("read");
        assert_eq!(ready.trim(), "ready");
        let run = Run::new(Some(0.05));
        run.lock().child = Some(child.id());
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancelled);
        run.set_cancel_tools(Some(Box::new(move || flag.store(true, Ordering::SeqCst))));
        let started = Instant::now();
        run.stop();
        let status = loop {
            if let Some(status) = child.try_wait().expect("wait") {
                break status;
            }
            if started.elapsed() > Duration::from_secs(3) {
                let _ = child.kill();
                panic!("the interrupt-ignoring provider outlived its grace");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(status.signal(), Some(9));
        assert!(cancelled.load(Ordering::SeqCst));
        assert!(run.stopped());
    }
}
