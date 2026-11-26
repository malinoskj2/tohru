use std::io::Write as _;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use regex::Regex;
use serde_json::{Map, Value, json};

use super::store::casefold;
use super::{home, private_dir, read_json, scratch_dir, write_private};
use crate::paths::{self, Xdg};
use crate::pyjson;
use crate::pyvalue::{py_str, truthy};

pub const DESKTOP_TAG: &str = "[tohru-desktop]";
pub const DESKTOP_VOICE_TAG: &str = "[tohru-desktop:voice]";
pub const NEEDS_YOU_TAG: &str = "[tohru-desktop:needs-you]";
const PROBE_SECONDS: Duration = Duration::from_secs(2);
const STATUS_TTL: f64 = 10.0;
const ORCA_DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

static CONVERSATION_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^(?:[0-9a-f]{32}|[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$",
    )
    .expect("fixed pattern is valid")
});
const OWNER: &str = "\n[foreground-owner-turn]\n";

static SLUG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("[^a-z0-9]+").expect("fixed pattern is valid"));

/// The ids Tohru accepts for conversations and Orca chats.
#[must_use]
pub fn is_conversation_id(text: &str) -> bool {
    CONVERSATION_ID.is_match(text)
}

fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

fn absolute_env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

/// The Orca CLI and its user-data directory, or empty strings when none is installed.
#[must_use]
pub fn find_orca_cli() -> (String, String) {
    let user_data_env = absolute_env("ORCA_USER_DATA_PATH");
    let xdg_orca = paths::xdg_absolute(Xdg::Config).join("orca");
    let home_orca = home().join(".config/orca");
    let mut directories: Vec<PathBuf> = Vec::new();
    for candidate in [
        user_data_env.clone(),
        Some(xdg_orca.clone()),
        Some(home_orca.clone()),
    ]
    .into_iter()
    .flatten()
    {
        if !directories.contains(&candidate) {
            directories.push(candidate);
        }
    }
    for directory in &directories {
        let shim = directory.join("linux-orca-cli-shim/orca");
        if is_executable(&shim) {
            return (shim.display().to_string(), directory.display().to_string());
        }
    }
    // Never resolve bare `orca` on Linux: outside Orca's terminals it is usually the screen reader.
    let Some(cli) = crate::probe::which("orca-ide") else {
        return (String::new(), String::new());
    };
    let user_data = user_data_env.unwrap_or_else(|| {
        if xdg_orca.is_dir() {
            xdg_orca
        } else if home_orca.is_dir() {
            home_orca
        } else {
            xdg_orca.clone()
        }
    });
    (cli.display().to_string(), user_data.display().to_string())
}

/// The environment for Orca calls: no outer Orca session variables and no Discord credentials.
fn orca_env(user_data: &str) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    let mut env: Vec<_> = super::scrubbed_env()
        .into_iter()
        .filter(|(key, _)| !key.to_string_lossy().starts_with("ORCA_"))
        .collect();
    if !user_data.is_empty() {
        env.push(("ORCA_USER_DATA_PATH".into(), user_data.into()));
    }
    env
}

fn orca_command(cli: &str, user_data: &str, argv: &[&str]) -> Command {
    let mut command = Command::new(cli);
    command
        .args(argv)
        .current_dir(home())
        .env_clear()
        .envs(orca_env(user_data));
    command
}

/// Whether the Orca app or runtime answers, cached briefly in the scratch parent.
#[must_use]
pub fn orca_running(cli: &str, user_data: &str) -> bool {
    let scratch = scratch_dir();
    let cache_path = scratch
        .parent()
        .unwrap_or(&scratch)
        .join("orca-status.json");
    let now = super::now_f64();
    if let Some(Value::Object(cached)) = read_json(&cache_path) {
        let fresh = cached
            .get("checked")
            .and_then(Value::as_f64)
            .is_some_and(|checked| (0.0..STATUS_TTL).contains(&(now - checked)))
            && cached
                .get("checked")
                .is_some_and(|checked| !checked.is_boolean());
        if cached.get("cli").and_then(Value::as_str) == Some(cli)
            && cached.get("user_data").and_then(Value::as_str) == Some(user_data)
            && fresh
            && let Some(Value::Bool(running)) = cached.get("running")
        {
            return *running;
        }
    }
    let result = super::run_bounded(
        orca_command(cli, user_data, &["status", "--json"]),
        PROBE_SECONDS,
    );
    let running = result
        .filter(super::Finished::success)
        .is_some_and(|result| {
            let Ok(Value::Object(data)) = serde_json::from_str::<Value>(&result.stdout) else {
                return false;
            };
            if data.get("ok") != Some(&Value::Bool(true)) {
                return false;
            }
            let body = data.get("result");
            let flag = |section: &str, key: &str| {
                body.and_then(|body| body.get(section))
                    .and_then(|section| section.get(key))
                    == Some(&Value::Bool(true))
            };
            flag("app", "running") || flag("runtime", "reachable")
        });
    if let Some(parent) = cache_path.parent()
        && private_dir(parent).is_ok()
    {
        let record = json!({"cli": cli, "user_data": user_data, "running": running, "checked": super::now_f64()});
        let _ = write_private(&cache_path, &pyjson::dumps(&record));
    }
    running
}

#[must_use]
pub fn orca_prompt(cli: &str, running: bool) -> String {
    if cli.is_empty() {
        return "Orca support is enabled, but no Orca CLI shim or `orca-ide` executable was found on this launch. \
You are NOT running inside an Orca terminal or worktree, and the user may or may not be talking about Orca. \
Never run bare `orca` on Linux (it is usually the GNOME screen reader); if the user asks for Orca work, \
look for `orca-ide` or `~/.config/orca/linux-orca-cli-shim/orca`, and run `<cli> skills get orca-cli` before guessing commands."
            .to_owned();
    }
    let state = if running {
        "currently running"
    } else {
        "installed, but not currently running"
    };
    format!(
        "Orca (the multi-agent IDE) is available on this machine ({state}). \
You are NOT running inside an Orca terminal or worktree, and the user may or may not be talking about Orca — \
only use it when their request involves Orca, its worktrees, agents, terminals, browser or integrations. \
When you do Orca work, invoke the CLI via `$ORCA_CLI_COMMAND` ({cli}); never run bare `orca` on Linux \
(it is usually the GNOME screen reader). \
Before running Orca commands, load the version-matched guide instead of guessing: \
`{cli} skills get orca-cli` (worktrees, terminals, handoffs, browser), \
`{cli} skills get orchestration` (supervised worker agents), \
`{cli} skills get orca-computer-use` (GUI control), or `{cli} skills list` for all bundled guides."
    )
}

/// The environment and prompt that tell a local provider about Orca, per `assistant.orca`.
#[must_use]
pub fn orca_context(mode: &str) -> (Vec<(String, String)>, String) {
    if mode == "false" {
        return (Vec::new(), String::new());
    }
    let (cli, user_data) = find_orca_cli();
    if !cli.is_empty() {
        let running = orca_running(&cli, &user_data);
        let mut env = vec![("ORCA_CLI_COMMAND".to_owned(), cli.clone())];
        if !user_data.is_empty() {
            env.push(("ORCA_USER_DATA_PATH".to_owned(), user_data));
        }
        return (env, orca_prompt(&cli, running));
    }
    if mode == "true" {
        return (Vec::new(), orca_prompt("", false));
    }
    (Vec::new(), String::new())
}

/// `TOHRU_ORCA_BIN` when set (and executable), else the discovered CLI.
fn orca_executable() -> (String, String) {
    match std::env::var("TOHRU_ORCA_BIN") {
        Ok(configured) if !configured.is_empty() => {
            if is_executable(Path::new(&configured)) {
                (configured, String::new())
            } else {
                (String::new(), String::new())
            }
        }
        _ => find_orca_cli(),
    }
}

pub type OrcaReply = Result<Map<String, Value>, String>;

fn last_json(raw: &str) -> Option<Value> {
    serde_json::from_str(raw).ok().or_else(|| {
        raw.lines()
            .rev()
            .find_map(|line| serde_json::from_str(line).ok())
    })
}

/// Runs an Orca CLI command as an external local caller.
pub fn orca_call(argv: &[&str], timeout: Duration) -> OrcaReply {
    let (exe, user_data) = orca_executable();
    if exe.is_empty() {
        return Err("orca CLI is not installed".to_owned());
    }
    let mut command = orca_command(&exe, &user_data, argv);
    command.stdin(std::process::Stdio::null());
    which_spawnable(&exe)?;
    let Some(finished) = super::run_bounded(command, timeout) else {
        return Err("orca command timed out".to_owned());
    };
    let raw = finished.stdout.trim();
    let code = finished
        .code
        .map_or_else(|| "None".to_owned(), |code| code.to_string());
    let data = if raw.is_empty() { None } else { last_json(raw) };
    if let Some(Value::Object(data)) = data {
        if truthy(data.get("ok")) {
            return Ok(match data.get("result") {
                Some(Value::Object(result)) => result.clone(),
                _ => Map::new(),
            });
        }
        return Err(match data.get("error") {
            Some(Value::Object(error)) => [error.get("message"), error.get("code")]
                .into_iter()
                .find(|value| truthy(*value))
                .map_or_else(|| "orca error".to_owned(), py_str),
            error if truthy(error) => py_str(error),
            _ => {
                let stderr = finished.stderr.trim();
                if stderr.is_empty() {
                    format!("orca exited with {code}")
                } else {
                    stderr.to_owned()
                }
            }
        });
    }
    if finished.success() {
        return Ok(Map::new());
    }
    let message = [finished.stderr.as_str(), raw]
        .into_iter()
        .find(|text| !text.is_empty())
        .map_or_else(|| format!("orca exited with {code}"), str::to_owned);
    Err(super::store::chars(message.trim(), 300))
}

/// Mirrors the `OSError` Python reports when the executable cannot start at all.
fn which_spawnable(exe: &str) -> Result<(), String> {
    match std::fs::metadata(exe) {
        Ok(_) => Ok(()),
        Err(error) => Err(format!(
            "[Errno {}] {}: '{exe}'",
            error.raw_os_error().unwrap_or_default(),
            describe(&error)
        )),
    }
}

fn describe(error: &std::io::Error) -> String {
    let text = error.to_string();
    text.split(" (os error").next().unwrap_or(&text).to_owned()
}

#[must_use]
pub fn orca_bots_root() -> PathBuf {
    std::env::var_os("TOHRU_ORCA_BOTS_DIR")
        .filter(|value| !value.is_empty())
        .map_or_else(|| home().join(".orca/bots"), PathBuf::from)
}

#[must_use]
pub fn bot_slug(name: &str) -> String {
    let slug = SLUG
        .replace_all(&casefold(name), "-")
        .trim_matches('-')
        .to_owned();
    if slug.is_empty() {
        "tohru".to_owned()
    } else {
        slug
    }
}

/// The bot's home directory: the one whose `bot.json` names it, else its slug.
#[must_use]
pub fn orca_bot_dir(bot_name: &str, bot_id: Option<&str>) -> PathBuf {
    let root = orca_bots_root();
    if let Ok(entries) = std::fs::read_dir(&root) {
        let mut entries: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .collect();
        entries.sort();
        for entry in entries.iter().filter(|entry| entry.is_dir()) {
            let Some(Value::Object(meta)) = read_json(&entry.join("bot.json")) else {
                continue;
            };
            if bot_id.is_some_and(|id| {
                !id.is_empty() && meta.get("id").and_then(Value::as_str) == Some(id)
            }) {
                return entry.clone();
            }
            let name = meta
                .get("name")
                .filter(|name| truthy(Some(name)))
                .map(|name| py_str(Some(name)))
                .unwrap_or_default();
            if casefold(&name) == casefold(bot_name) {
                return entry.clone();
            }
        }
    }
    root.join(bot_slug(bot_name))
}

pub fn orca_find_bot(bot_name: &str) -> Result<Map<String, Value>, String> {
    let result =
        orca_call(&["bot", "list", "--json"], super::probe_timeout()).map_err(|error| {
            if error.is_empty() {
                "Orca is not running".to_owned()
            } else {
                error
            }
        })?;
    let bots = result
        .get("bots")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for bot in bots {
        let Value::Object(bot) = bot else {
            continue;
        };
        let name = bot
            .get("name")
            .filter(|name| truthy(Some(name)))
            .map(|name| py_str(Some(name)))
            .unwrap_or_default();
        if casefold(&name) == casefold(bot_name)
            || bot.get("id").and_then(Value::as_str) == Some(bot_name)
        {
            return Ok(bot);
        }
    }
    Err(format!("Bot '{bot_name}' was not found in Orca"))
}

pub fn orca_bot_chats(
    bot_name: &str,
    include_all: bool,
) -> Result<Vec<Map<String, Value>>, String> {
    let mut argv = vec!["bot", "chats", "--bot", bot_name];
    if include_all {
        argv.push("--all");
    }
    argv.push("--json");
    let result = orca_call(&argv, Duration::from_secs(10))?;
    Ok(result
        .get("chats")
        .and_then(Value::as_array)
        .map(|chats| {
            chats
                .iter()
                .filter_map(|chat| chat.as_object().cloned())
                .collect()
        })
        .unwrap_or_default())
}

/// An Orca timestamp in seconds; millisecond values are scaled down.
#[must_use]
pub fn orca_ts(value: Option<&Value>) -> i64 {
    let stamp = match value {
        Some(Value::Number(number)) => number
            .as_i64()
            .or_else(|| number.as_f64().map(super::truncate)),
        Some(Value::String(text)) => text.trim().parse::<i64>().ok(),
        Some(Value::Bool(flag)) => Some(i64::from(*flag)),
        _ => Some(0),
    };
    let Some(stamp) = stamp else {
        return 0;
    };
    if stamp > 10_000_000_000 {
        stamp.div_euclid(1000)
    } else {
        stamp
    }
}

/// Strips Orca mail framing and Tohru desktop tags from user turns; Bots-panel turns stay verbatim.
#[must_use]
pub fn unwrap_desktop_text(text: &str) -> (String, bool) {
    let raw = text.replace("\r\n", "\n").trim().to_owned();
    if raw.is_empty() {
        return (String::new(), false);
    }
    let mut body = raw.clone();
    if body.contains("──── From:") {
        let mut cleaned: Vec<&str> = Vec::new();
        for line in raw.lines() {
            let stripped = line.trim();
            if stripped.starts_with("────") && stripped.ends_with("────") {
                continue;
            }
            if stripped.starts_with("Subject:") && cleaned.is_empty() {
                continue;
            }
            if stripped.starts_with("[Reply: orca orchestration reply") {
                continue;
            }
            cleaned.push(line);
        }
        cleaned.join("\n").trim().clone_into(&mut body);
    }
    for (tag, voice) in [
        (DESKTOP_VOICE_TAG, true),
        (NEEDS_YOU_TAG, false),
        (DESKTOP_TAG, false),
    ] {
        if let Some(rest) = body.strip_prefix(tag) {
            let mut rest = rest.trim_start_matches(['\n', ' ']);
            if rest.starts_with("[tohru-foreground home=")
                && let Some((_, owner)) = rest.split_once(OWNER)
            {
                rest = owner;
            }
            return (rest.to_owned(), voice);
        }
        if let Some(index) = body.find(&format!("\n{tag}")) {
            let rest = body[index + 1 + tag.len()..].trim_start_matches(['\n', ' ']);
            return (rest.to_owned(), voice);
        }
    }
    (raw, false)
}

/// The provider list shown for the Orca backend: the bot, a local fallback, or an offline row.
#[must_use]
pub fn orca_providers(
    bot_name: &str,
    models: Option<&Map<String, Value>>,
    offline: &str,
) -> Vec<Value> {
    let error = match orca_find_bot(bot_name) {
        Ok(bot) => {
            let paused = truthy(bot.get("paused"));
            let agent = bot
                .get("agent")
                .filter(|agent| truthy(Some(agent)))
                .map(|agent| py_str(Some(agent)))
                .unwrap_or_default();
            let name = py_str(bot.get("name"));
            let label = if paused {
                "Paused in Orca".to_owned()
            } else if agent.is_empty() {
                "Orca bot".to_owned()
            } else {
                format!("Orca · {agent}")
            };
            return vec![json!({
                "id": format!("orca:{name}"), "name": name, "slot": 0, "signed_in": !paused, "model": label,
                "model_id": agent, "effort": "", "backend": "orca", "bot": name, "can_signin": false,
                "offline_title": if paused { format!("{name} is paused in Orca") } else { String::new() },
                "offline_body": if paused { "Unpause this bot in Orca’s Bots panel to resume chatting." } else { "" },
            })];
        }
        Err(error) => error,
    };
    if offline == "local" {
        let mut found = super::models::discover(models);
        for provider in &mut found {
            let label = provider
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            provider["offline_fallback"] = json!(true);
            provider["model"] = json!(if label.is_empty() {
                "Local fallback".to_owned()
            } else {
                format!("{label} · offline")
            });
        }
        return found;
    }
    let not_found = error.contains("was not found");
    vec![json!({
        "id": format!("orca:{bot_name}"), "name": bot_name, "slot": 0, "signed_in": false,
        "model": if not_found { "Not in Orca" } else { "Orca offline" }, "model_id": "", "effort": "",
        "backend": "orca", "bot": bot_name, "can_signin": false,
        "offline_title": if not_found { format!("{bot_name} isn’t in Orca yet") } else { "Orca isn’t running".to_owned() },
        "offline_body": if not_found {
            format!("Create a bot named {bot_name} in Orca’s Bots panel, then run: tohru assistant orca init --bot {bot_name}")
        } else {
            "Open Orca (or run orca serve) so desktop chats stay in sync with Orca’s Bots panel.".to_owned()
        },
    })]
}

/// One user or assistant message of an Orca chat.
#[derive(Clone, Debug, Default)]
pub struct Entry {
    pub role: String,
    pub text: String,
    pub id: String,
    pub shortened: bool,
    pub input_kind: Option<String>,
    pub request_id: String,
    pub completion_id: String,
}

pub struct Read {
    pub entries: Vec<Entry>,
    pub state: String,
}

pub fn orca_read_messages(chat_id: &str, limit: usize) -> Result<Read, String> {
    let handle = format!("orca_session_id:{chat_id}");
    let limit = limit.to_string();
    let result = orca_call(
        &[
            "agent", "read", &handle, "--limit", &limit, "--ids", "--json",
        ],
        Duration::from_secs(10),
    )?;
    let mut entries = Vec::new();
    for item in result
        .get("entries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Value::Object(item) = item else {
            continue;
        };
        let kind = item.get("kind");
        let role = item.get("role").and_then(Value::as_str).unwrap_or_default();
        if !(matches!(kind, None | Some(Value::Null))
            || kind.and_then(Value::as_str) == Some("message"))
        {
            continue;
        }
        if role != "user" && role != "assistant" {
            continue;
        }
        let authority = item.get("interaction").and_then(Value::as_object);
        let text_or_empty = |value: Option<&Value>| {
            if truthy(value) {
                py_str(value)
            } else {
                String::new()
            }
        };
        entries.push(Entry {
            role: role.to_owned(),
            text: text_or_empty(item.get("text")),
            id: text_or_empty(item.get("itemId")),
            shortened: truthy(item.get("shortened")),
            input_kind: authority
                .and_then(|authority| authority.get("kind"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            request_id: authority
                .and_then(|authority| authority.get("requestId"))
                .map(|value| py_str(Some(value)))
                .unwrap_or_default(),
            completion_id: authority
                .and_then(|authority| authority.get("completionId"))
                .map(|value| py_str(Some(value)))
                .unwrap_or_default(),
        });
    }
    let state = result
        .get("state")
        .filter(|state| truthy(Some(state)))
        .map(|state| py_str(Some(state)))
        .unwrap_or_default();
    Ok(Read { entries, state })
}

#[must_use]
pub fn orca_extract_chat_id(receipt: Option<&Value>) -> String {
    let Some(Value::Object(receipt)) = receipt else {
        return String::new();
    };
    let value = |key: &str| {
        receipt
            .get(key)
            .filter(|value| truthy(Some(value)))
            .map(|value| py_str(Some(value)))
            .unwrap_or_default()
    };
    for key in ["chatId", "orcaSessionId", "sessionId", "targetChatId"] {
        let raw = value(key);
        let id = raw.strip_prefix("orca_session_id:").unwrap_or(&raw);
        if is_conversation_id(id) {
            return id.to_lowercase();
        }
    }
    for key in ["toHandle", "to_handle", "to"] {
        if let Some(id) = value(key).strip_prefix("orca_session_id:")
            && is_conversation_id(id)
        {
            return id.to_lowercase();
        }
    }
    String::new()
}

/// A private temporary file in the scratch folder, removed on drop.
pub fn private_text_file(prefix: &str, text: &str) -> std::io::Result<tempfile::NamedTempFile> {
    let scratch = private_dir(&scratch_dir())?;
    let mut file = tempfile::Builder::new()
        .prefix(prefix)
        .tempfile_in(scratch)?;
    file.as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.write_all(text.as_bytes())?;
    file.flush()?;
    Ok(file)
}

/// Submits one owner turn; the shared chat history is the only answer path.
pub fn orca_send_to_bot(bot_name: &str, body: &str, chat_arg: Option<&str>) -> OrcaReply {
    let file = private_text_file(".orca-body-", body).map_err(|error| error.to_string())?;
    let path = file.path().display().to_string();
    let operation = format!("{:013}-{}", super::now_ms(), uuid::Uuid::new_v4().simple());
    let mut argv = vec![
        "bot",
        "send",
        "--bot",
        bot_name,
        "--text-file",
        &path,
        "--request-id",
        &operation,
        "--delivery",
        "now",
        "--json",
    ];
    if let Some(chat) = chat_arg.filter(|chat| !chat.is_empty()) {
        argv.extend(["--chat", chat]);
    }
    orca_call(&argv, Duration::from_secs(30))
}

#[must_use]
pub fn orca_poll_interval() -> Duration {
    let ms = std::env::var("TOHRU_ORCA_POLL_MS")
        .ok()
        .map_or(Some(250), |value| value.trim().parse::<i64>().ok())
        .unwrap_or(250);
    Duration::from_secs_f64((super::float(ms) / 1000.0).clamp(0.02, 5.0))
}

#[must_use]
pub fn orca_timeout() -> Duration {
    let seconds = std::env::var("TOHRU_ORCA_TIMEOUT_S")
        .ok()
        .map_or(Some(600.0), |value| value.trim().parse::<f64>().ok());
    Duration::from_secs_f64(
        seconds
            .filter(|seconds| seconds.is_finite())
            .map_or(600.0, |seconds| seconds.max(1.0)),
    )
}

/// How the reply poll finished.
pub struct Waited {
    pub text: String,
    pub stopped: bool,
    pub error: String,
}

pub struct Wait<'a> {
    pub chat_id: &'a str,
    pub before_count: usize,
    pub prompt_marker: &'a str,
    pub message_id: &'a str,
}

/// The callbacks a reply poll reports through.
pub struct Hooks<'a> {
    pub on_delta: &'a mut dyn FnMut(&str),
    pub stop: &'a dyn Fn() -> bool,
    pub on_origin: &'a mut dyn FnMut(&str, &str),
}

/// Polls `orca agent read` until the bot finishes this turn.
pub fn orca_wait_for_reply(wait: &Wait<'_>, hooks: &mut Hooks<'_>) -> Waited {
    let chat = wait.chat_id.to_owned();
    let interval = orca_poll_interval();
    wait_for_reply(
        wait,
        hooks,
        &mut || orca_read_messages(&chat, 100),
        &mut || std::thread::sleep(interval),
    )
}

/// The reply poll over any chat reader, so its admission rules can be exercised without Orca.
#[expect(
    clippy::too_many_lines,
    reason = "one ordered sequence of checks, kept together to read against the protocol"
)]
pub fn wait_for_reply(
    wait: &Wait<'_>,
    hooks: &mut Hooks<'_>,
    read: &mut dyn FnMut() -> Result<Read, String>,
    pause: &mut dyn FnMut(),
) -> Waited {
    let deadline = Instant::now() + orca_timeout();
    let mut origin_reported = false;
    let done = |text: String, stopped: bool, error: &str| Waited {
        text,
        stopped,
        error: error.to_owned(),
    };
    loop {
        if (hooks.stop)() {
            return done(String::new(), true, "");
        }
        let read = match read() {
            Ok(read) => read,
            Err(error) => {
                if Instant::now() >= deadline {
                    let error = if error.is_empty() {
                        "Could not read Orca bot chat".to_owned()
                    } else {
                        error
                    };
                    return done(String::new(), false, &error);
                }
                pause();
                continue;
            }
        };
        let entries = &read.entries;
        // Stable journal ids survive a full or shifted history window and repeated reads.
        let mut tail: &[Entry] = if entries.len() >= wait.before_count {
            &entries[wait.before_count..]
        } else {
            entries
        };
        if !wait.message_id.is_empty() {
            tail = &[];
            for (index, item) in entries.iter().enumerate() {
                let key = if item.request_id.is_empty() {
                    &item.id
                } else {
                    &item.request_id
                };
                if key == wait.message_id && item.role == "user" {
                    tail = &entries[index..];
                    if !origin_reported {
                        (hooks.on_origin)(
                            wait.message_id,
                            if item.id.is_empty() {
                                wait.message_id
                            } else {
                                &item.id
                            },
                        );
                        origin_reported = true;
                    }
                    break;
                }
            }
        } else if entries.len() < wait.before_count
            && !wait.prompt_marker.is_empty()
            && let Some(index) = entries
                .iter()
                .rposition(|item| item.role == "user" && item.text.contains(wait.prompt_marker))
        {
            tail = &entries[index..];
        }
        // Speech and the popup receive only the last assistant message of a completed turn.
        let mut current = String::new();
        let mut completed = false;
        let mut seen_new_user = false;
        for item in tail {
            if item.role == "user" {
                if item.input_kind.as_deref() == Some("internal") {
                    continue;
                }
                if seen_new_user {
                    return done(
                        String::new(),
                        false,
                        "Another user turn arrived before the desktop answer was read",
                    );
                }
                seen_new_user = !wait.message_id.is_empty()
                    || (!wait.prompt_marker.is_empty() && item.text.contains(wait.prompt_marker));
                current.clear();
                completed = false;
            } else if item.role == "assistant" && seen_new_user {
                let kind = item.input_kind.as_deref();
                if kind == Some("internal")
                    || (kind == Some("answer") && item.request_id != wait.message_id)
                {
                    continue;
                }
                current = if item.shortened {
                    String::new()
                } else {
                    item.text.trim().to_owned()
                };
                completed = !item.completion_id.is_empty();
            }
        }
        if (completed || read.state == "done") && !current.is_empty() {
            (hooks.on_delta)(&current);
            return done(current, false, "");
        }
        if Instant::now() >= deadline {
            return done(
                String::new(),
                false,
                "Timed out waiting for Orca bot response",
            );
        }
        pause();
    }
}

/// Records a side voice exchange in the bot's Home chat.
pub fn orca_side_record(
    bot: &str,
    phase: &str,
    request: &str,
    text: &str,
    answered: bool,
) -> OrcaReply {
    let file = private_text_file(".voice-side-", text).map_err(|error| error.to_string())?;
    let path = file.path().display().to_string();
    let argv = [
        "bot",
        "voice-side",
        "--bot",
        bot,
        "--phase",
        phase,
        "--request",
        request,
        "--text-file",
        &path,
        "--json",
    ];
    if answered {
        return orca_call(&argv, ORCA_DEFAULT_TIMEOUT);
    }
    let mut unanswered = argv.to_vec();
    unanswered.push("--unanswered");
    let result = orca_call(&unanswered, ORCA_DEFAULT_TIMEOUT);
    // A CLI from before the flag refuses it before reaching Orca, so the retry records once.
    match result {
        Err(error) if error.contains("--unanswered") => orca_call(&argv, ORCA_DEFAULT_TIMEOUT),
        other => other,
    }
}
