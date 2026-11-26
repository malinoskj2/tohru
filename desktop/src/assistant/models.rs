use std::path::Path;
use std::sync::LazyLock;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use regex::Regex;
use serde_json::{Map, Value, json};

use super::{AssistantError, Result, home, private_dir, probe_timeout, quiet, read_json};
use crate::paths::{self, Xdg};
use crate::probe::which;
use crate::pyjson;

pub const EFFORTS: [&str; 6] = ["minimal", "low", "medium", "high", "xhigh", "max"];
/// Claude and Codex take instructions on every run, so the persona and memories are fresh each
/// turn; the others get them ahead of the first turn only.
pub const SYSTEM_FLAG: [&str; 2] = ["claude", "codex"];
/// Only providers that can fork a resumed session cleanly support speculative drafts.
pub const DRAFT_PROVIDERS: [&str; 2] = ["claude", "codex"];

static MODEL_TEXT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9._:/@#+-]{0,128}$").expect("fixed pattern is valid"));
static SUFFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[.*\]$").expect("fixed pattern is valid"));
static CLAUDE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^claude-(opus|sonnet|haiku|fable)-(\d+)-(\d+)").expect("fixed pattern is valid")
});
static GEMINI: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^gemini-(\d+(?:\.\d+)?)-([a-z]+)$").expect("fixed pattern is valid")
});

/// Used when `assistant.models.<provider>.model` or `.effort` is empty. `OpenCode` routes to
/// whichever providers the user configured, so it keeps its own default.
fn default_model(ident: &str) -> (&'static str, &'static str) {
    match ident {
        "claude" => ("claude-opus-5-5", "medium"),
        "codex" => ("gpt-6.2-sol", "medium"),
        "gemini" | "agy" => ("gemini-4-neon", "high"),
        _ => ("", ""),
    }
}

/// Tool-free spoken drafts use the measured fast profile when neither setting is pinned.
fn draft_default(ident: &str, semantic: bool) -> Option<(&'static str, &'static str)> {
    match (ident, semantic) {
        ("codex", false) => Some(("gpt-6-luna", "low")),
        ("codex", true) => Some(("gpt-6.2-sol", "low")),
        _ => None,
    }
}

/// Each CLI's own name for an effort level.
#[must_use]
pub fn effort_name(ident: &str, effort: &str) -> String {
    let name = match (ident, effort) {
        ("claude" | "agy", "minimal") => "low",
        ("codex", "max") => "xhigh",
        ("gemini", "xhigh" | "max") => "HIGH",
        ("gemini", level) => return level.to_uppercase(),
        (_, level) => level,
    };
    name.to_owned()
}

/// Which spoken draft profile a model is resolved for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Turn,
    VoiceDraft,
    Semantic,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Choice {
    pub model: String,
    pub effort: String,
}

/// The model and effort a provider runs with: the configured ones, else Tohru's default for it.
#[must_use]
pub fn resolve_model(ident: &str, chosen: &Choice, profile: Profile, scenario: &Choice) -> Choice {
    let (mut fallback_model, mut fallback_effort) = default_model(ident);
    let draft = profile != Profile::Turn;
    let unpinned = chosen.model.is_empty() && chosen.effort.is_empty();
    if draft
        && (unpinned || !scenario.model.is_empty() || !scenario.effort.is_empty())
        && let Some((model, effort)) = draft_default(ident, profile == Profile::Semantic)
    {
        (fallback_model, fallback_effort) = (model, effort);
    }
    let (mut model, mut effort) = (chosen.model.as_str(), chosen.effort.as_str());
    if draft {
        if !scenario.model.is_empty() {
            model = &scenario.model;
        }
        if !scenario.effort.is_empty() {
            effort = &scenario.effort;
        }
    }
    Choice {
        model: if !model.is_empty() && MODEL_TEXT.is_match(model) {
            model
        } else {
            fallback_model
        }
        .to_owned(),
        effort: if EFFORTS.contains(&effort) {
            effort
        } else {
            fallback_effort
        }
        .to_owned(),
    }
}

/// Whether a configured model name is safe to pass to a CLI.
#[must_use]
pub fn valid_model(model: &str) -> bool {
    MODEL_TEXT.is_match(model)
}

fn capitalize(word: &str) -> String {
    let mut letters = word.chars();
    letters.next().map_or_else(String::new, |first| {
        first
            .to_uppercase()
            .chain(letters.flat_map(char::to_lowercase))
            .collect()
    })
}

#[must_use]
pub fn pretty_model(model: &str) -> String {
    let model = SUFFIX.replace(model, "");
    if let Some(found) = CLAUDE.captures(&model) {
        return format!("{} {}.{}", capitalize(&found[1]), &found[2], &found[3]);
    }
    if let Some(found) = GEMINI.captures(&model) {
        return format!("Gemini {} {}", &found[1], capitalize(&found[2]));
    }
    model
        .split_once('/')
        .map_or_else(|| model.to_string(), |(_, rest)| rest.to_owned())
}

fn opencode_configured_model() -> String {
    read_json(&paths::xdg_absolute(Xdg::Config).join("opencode/opencode.json"))
        .and_then(|config| {
            config
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_default()
}

#[must_use]
pub fn model_label(ident: &str, model: &str, effort: &str) -> String {
    let model = if ident == "opencode" && model.is_empty() {
        opencode_configured_model()
    } else {
        model.to_owned()
    };
    let name = if model.is_empty() {
        String::new()
    } else {
        pretty_model(&model)
    };
    if !name.is_empty() && !effort.is_empty() {
        format!("{name} · {effort}")
    } else {
        name
    }
}

fn has_content(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.len() > 2)
}

fn env_set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty())
}

/// A provider CLI Tohru can drive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Provider {
    pub slot: u8,
    pub id: &'static str,
    pub name: &'static str,
    pub login: &'static [&'static str],
}

pub const PROVIDERS: [Provider; 5] = [
    Provider {
        slot: 0,
        id: "claude",
        name: "Claude",
        login: &["claude", "auth", "login"],
    },
    Provider {
        slot: 1,
        id: "codex",
        name: "Codex",
        login: &["codex", "login"],
    },
    Provider {
        slot: 2,
        id: "gemini",
        name: "Gemini",
        login: &["gemini"],
    },
    Provider {
        slot: 3,
        id: "agy",
        name: "Antigravity",
        login: &["agy"],
    },
    Provider {
        slot: 4,
        id: "opencode",
        name: "OpenCode",
        login: &["opencode", "auth", "login"],
    },
];

#[must_use]
pub fn provider(ident: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|provider| provider.id == ident)
}

/// The history slot of a provider, or `null` for one Tohru no longer knows.
#[must_use]
pub fn slot(ident: &str) -> Value {
    provider(ident).map_or(Value::Null, |provider| json!(provider.slot))
}

impl Provider {
    #[must_use]
    pub fn installed(&self) -> bool {
        which(self.id).is_some()
    }

    #[must_use]
    pub fn signed_in(&self) -> bool {
        let home = home();
        match self.id {
            "claude" => {
                if let Some(result) = quiet(&["claude", "auth", "status"], probe_timeout())
                    && result.stdout.trim().starts_with('{')
                    && let Ok(status) = serde_json::from_str::<Value>(&result.stdout)
                {
                    return crate::pyvalue::truthy(status.get("loggedIn"));
                }
                has_content(&home.join(".claude/.credentials.json")) || env_set("ANTHROPIC_API_KEY")
            }
            "codex" => quiet(&["codex", "login", "status"], probe_timeout()).map_or_else(
                || has_content(&home.join(".codex/auth.json")),
                |result| result.success(),
            ),
            "gemini" => {
                has_content(&home.join(".gemini/oauth_creds.json")) || env_set("GEMINI_API_KEY")
            }
            "agy" => has_content(&home.join(".gemini/antigravity-cli/antigravity-oauth-token")),
            _ => read_json(&paths::xdg_absolute(Xdg::Data).join("opencode/auth.json"))
                .and_then(|auth| auth.as_object().map(|auth| !auth.is_empty()))
                .unwrap_or(false),
        }
    }
}

fn text_field(object: Option<&Value>, key: &str) -> String {
    match object.and_then(|object| object.get(key)) {
        Some(value) if crate::pyvalue::truthy(Some(value)) => crate::pyvalue::py_str(Some(value)),
        _ => String::new(),
    }
}

fn scenario(chosen: Option<&Value>, key: &str) -> Choice {
    let section = chosen
        .and_then(|chosen| chosen.get(key))
        .filter(|section| section.is_object());
    let field = |name: &str| match section.and_then(|section| section.get(name)) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    };
    Choice {
        model: field("model"),
        effort: field("effort"),
    }
}

/// The installed CLIs; `models` maps a provider to its configured `{"model", "effort"}`.
#[must_use]
pub fn discover(models: Option<&Map<String, Value>>) -> Vec<Value> {
    let found: Vec<&Provider> = PROVIDERS
        .iter()
        .filter(|provider| provider.installed())
        .collect();
    let (sender, receiver) = mpsc::channel();
    for provider in &found {
        let (sender, provider) = (sender.clone(), **provider);
        std::thread::spawn(move || {
            let _ = sender.send((provider.id, provider.signed_in()));
        });
    }
    drop(sender);
    let deadline = Instant::now() + probe_timeout() + Duration::from_secs(1);
    let mut signed = Map::new();
    while let Ok((id, signed_in)) =
        receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
    {
        signed.insert(id.to_owned(), json!(signed_in));
    }
    found
        .iter()
        .map(|provider| {
            let chosen = models.and_then(|models| models.get(provider.id)).filter(|chosen| chosen.is_object());
            let configured = Choice { model: text_field(chosen, "model"), effort: text_field(chosen, "effort") };
            let turn = resolve_model(provider.id, &configured, Profile::Turn, &Choice::default());
            let voice = resolve_model(provider.id, &configured, Profile::VoiceDraft, &scenario(chosen, "voice_models"));
            let semantic =
                resolve_model(provider.id, &configured, Profile::Semantic, &scenario(chosen, "semantic_models"));
            json!({"id": provider.id, "name": provider.name, "slot": provider.slot,
                   "signed_in": signed.get(provider.id).cloned().unwrap_or(json!(false)),
                   "model": model_label(provider.id, &turn.model, &turn.effort), "model_id": turn.model,
                   "effort": turn.effort, "voice_draft_model_id": voice.model, "voice_draft_effort": voice.effort,
                   "semantic_model_id": semantic.model, "semantic_effort": semantic.effort})
        })
        .collect()
}

#[must_use]
pub fn terminal_command(argv: &[&str]) -> Option<Vec<String>> {
    let words = |text: &str| {
        text.split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let argv: Vec<String> = argv.iter().map(|word| (*word).to_owned()).collect();
    if let Some(configured) = std::env::var("TOHRU_TERMINAL")
        .ok()
        .filter(|value| !value.is_empty())
    {
        return Some([words(&configured), argv].concat());
    }
    if which("xdg-terminal-exec").is_some() {
        return Some([vec!["xdg-terminal-exec".to_owned()], argv].concat());
    }
    if let Some(terminal) = std::env::var("TERMINAL")
        .ok()
        .filter(|value| !value.is_empty())
    {
        let parts = words(&terminal);
        if parts
            .first()
            .is_some_and(|program| which(program).is_some())
        {
            return Some([parts, vec!["-e".to_owned()], argv].concat());
        }
    }
    let names = [
        "foot",
        "kitty",
        "alacritty",
        "wezterm",
        "ghostty",
        "konsole",
        "gnome-terminal",
        "xterm",
    ];
    let name = names.into_iter().find(|name| which(name).is_some())?;
    let lead: &[&str] = match name {
        "wezterm" => &["start", "--"],
        "gnome-terminal" => &["--wait", "--"],
        _ => &["-e"],
    };
    Some(
        [
            vec![name.to_owned()],
            lead.iter().map(|word| (*word).to_owned()).collect(),
            argv,
        ]
        .concat(),
    )
}

/// Opens the provider's login in a terminal, then reports its state.
pub fn signin(ident: &str) -> Result<()> {
    let provider = provider(ident)
        .filter(|provider| provider.installed())
        .ok_or_else(|| {
            AssistantError::exit(format!("tohru-assistant: {ident} is not installed"))
        })?;
    let command = terminal_command(provider.login).ok_or_else(|| {
        AssistantError::exit("tohru-assistant: no terminal found; set TOHRU_TERMINAL")
    })?;
    let (program, args) = command
        .split_first()
        .ok_or_else(|| AssistantError::exit("tohru-assistant: no terminal found"))?;
    let _ = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .current_dir(home())
        .env_clear()
        .envs(super::scrubbed_env())
        .status();
    let report = discover(None)
        .into_iter()
        .find(|found| found.get("id").and_then(Value::as_str) == Some(ident))
        .unwrap_or_else(|| json!({"id": ident, "signed_in": false}));
    super::emit(report);
    Ok(())
}

/// How a provider run relates to the conversation's saved CLI session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub resume: bool,
    pub fork: bool,
    pub ephemeral: bool,
}

/// Gemini sets thinking only in settings: a project alias in the scratch folder carries the model
/// and level.
pub fn gemini_settings(scratch: &Path, model: &str, effort: &str) -> Result<&'static str> {
    let folder = private_dir(&scratch.join(".gemini"))?;
    let mut config = json!({"model": model});
    if !effort.is_empty() {
        config["generateContentConfig"] =
            json!({"thinkingConfig": {"thinkingLevel": effort_name("gemini", effort)}});
    }
    let alias = json!({"extends": "chat-base-3", "modelConfig": config});
    let settings = json!({"modelConfigs": {"customAliases": {"tohru-chat": alias}}});
    std::fs::write(folder.join("settings.json"), pyjson::dumps(&settings))?;
    Ok("tohru-chat")
}

/// How much a provider run may do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tools {
    Enabled,
    Disabled,
}

pub struct Invocation<'a> {
    pub ident: &'a str,
    pub session: &'a Session,
    pub model: &'a str,
    pub effort: &'a str,
    pub system: &'a str,
    pub scratch: &'a Path,
    pub tools: Tools,
}

fn owned(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

#[expect(
    clippy::too_many_lines,
    reason = "one ordered sequence of checks, kept together to read against the protocol"
)]
pub fn provider_argv(run: &Invocation<'_>) -> Result<Vec<String>> {
    let tools = run.tools == Tools::Enabled;
    let session = run.session;
    let mut argv;
    match run.ident {
        "claude" => {
            argv = owned(&[
                "claude",
                "-p",
                "--output-format",
                "stream-json",
                "--verbose",
                "--include-partial-messages",
                "--setting-sources",
                "project",
            ]);
            argv.extend(owned(if tools {
                &["--permission-mode", "bypassPermissions"]
            } else {
                &["--tools", "", "--strict-mcp-config"]
            }));
            push_pair(&mut argv, "--model", run.model);
            if !run.effort.is_empty() {
                argv.extend(["--effort".to_owned(), effort_name("claude", run.effort)]);
            }
            push_pair(&mut argv, "--append-system-prompt", run.system);
            if session.ephemeral {
                argv.push("--no-session-persistence".to_owned());
            } else if session.resume {
                argv.extend(["--resume".to_owned(), session.id.clone()]);
                if session.fork {
                    argv.push("--fork-session".to_owned());
                }
            } else {
                argv.extend(["--session-id".to_owned(), session.id.clone()]);
            }
        }
        "codex" => {
            let mut common = owned(&["--json", "--skip-git-repo-check"]);
            common.extend(owned(if tools {
                &["--dangerously-bypass-approvals-and-sandbox"]
            } else {
                &["-c", "mcp_servers={}", "-c", "sandbox_mode=\"read-only\""]
            }));
            push_pair(&mut common, "-m", run.model);
            if !run.effort.is_empty() {
                common.extend([
                    "-c".to_owned(),
                    format!(
                        "model_reasoning_effort=\"{}\"",
                        effort_name("codex", run.effort)
                    ),
                ]);
            }
            if !run.system.is_empty() {
                common.extend([
                    "-c".to_owned(),
                    format!(
                        "developer_instructions={}",
                        pyjson::dumps(&json!(run.system))
                    ),
                ]);
            }
            if session.ephemeral {
                common.push("--ephemeral".to_owned());
            }
            if session.resume {
                let verb = if session.fork { "fork" } else { "resume" };
                argv = owned(&["codex", "exec", verb]);
                argv.extend(common);
                argv.extend([session.id.clone(), "-".to_owned()]);
            } else {
                argv = owned(&["codex", "exec"]);
                argv.extend(common);
                if !tools {
                    argv.extend(owned(&["--sandbox", "read-only"]));
                }
                argv.extend(owned(&["--color", "never", "-"]));
            }
        }
        "gemini" => {
            argv = owned(&[
                "gemini",
                "--output-format",
                "stream-json",
                "--approval-mode",
            ]);
            argv.push(if tools { "yolo" } else { "default" }.to_owned());
            if !run.model.is_empty() {
                argv.extend([
                    "-m".to_owned(),
                    gemini_settings(run.scratch, run.model, run.effort)?.to_owned(),
                ]);
            }
            if session.resume {
                argv.extend(["--resume".to_owned(), session.id.clone()]);
            }
            argv.extend(owned(&["-p", ""]));
        }
        "agy" => {
            argv = owned(&[
                "agy",
                "-p",
                "--output-format",
                "stream-json",
                "--disable-slash-commands",
            ]);
            argv.extend(owned(if tools {
                &["--dangerously-skip-permissions"]
            } else {
                &["--mode", "plan"]
            }));
            push_pair(&mut argv, "--model", run.model);
            if !run.effort.is_empty() {
                argv.extend(["--effort".to_owned(), effort_name("agy", run.effort)]);
            }
            if session.resume {
                argv.extend(["--conversation".to_owned(), session.id.clone()]);
            }
        }
        "opencode" => {
            argv = owned(&["opencode", "run", "--format", "json"]);
            if !run.model.is_empty() {
                let model = if !run.effort.is_empty() && !run.model.contains('#') {
                    format!("{}#{}", run.model, effort_name("opencode", run.effort))
                } else {
                    run.model.to_owned()
                };
                argv.extend(["--model".to_owned(), model]);
            }
            if session.resume {
                argv.extend(["--session".to_owned(), session.id.clone()]);
            }
        }
        other => {
            return Err(AssistantError::exit(format!(
                "tohru-assistant: unknown provider {other}"
            )));
        }
    }
    Ok(argv)
}

fn push_pair(argv: &mut Vec<String>, flag: &str, value: &str) {
    if !value.is_empty() {
        argv.extend([flag.to_owned(), value.to_owned()]);
    }
}
