//! The Needs-you watcher: notices Orca items that moved to Needs you and wakes the bot once.

use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Map, Value, json};

use super::orca::{NEEDS_YOU_TAG, orca_bot_chats, orca_call, orca_find_bot, orca_send_to_bot};
use super::quick::{clip, text};
use super::store::casefold;
use super::{data_dir, private_dir, read_json, write_private};
use crate::pyjson;
use crate::pyvalue::{py_str, truthy};

const PAGES: usize = 10;
const TEXT: usize = 600;

fn timeout() -> Duration {
    super::probe_timeout() * 2
}

fn path() -> PathBuf {
    data_dir().join("orca-needs-you.json")
}

/// Serializes watchers over the saved state.
struct Lock(#[expect(dead_code, reason = "held for its flock")] std::fs::File);

impl Lock {
    fn take() -> std::io::Result<Self> {
        let directory = private_dir(&data_dir())?;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(directory.join(".orca-needs-you.lock"))?;
        rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive)?;
        Ok(Self(file))
    }
}

type Items = Map<String, Value>;

fn load(bot_name: &str) -> Option<Items> {
    let Some(Value::Object(data)) = read_json(&path()) else {
        return None;
    };
    let version_one = data.get("version").and_then(Value::as_f64) == Some(1.0)
        && !data.get("version").is_some_and(Value::is_boolean);
    if !version_one || casefold(&text(data.get("bot"))) != casefold(bot_name) {
        return None;
    }
    Some(match data.get("items") {
        Some(Value::Object(items)) => items
            .iter()
            .filter(|(_, entry)| entry.is_object())
            .map(|(uid, entry)| (uid.clone(), entry.clone()))
            .collect(),
        _ => Map::new(),
    })
}

fn save(bot_name: &str, items: &Items) -> std::io::Result<()> {
    let state = json!({"version": 1, "bot": bot_name, "items": items});
    write_private(
        &path(),
        &pyjson::render(
            &state,
            pyjson::Style {
                ascii: true,
                sort_keys: true,
                compact: false,
            },
        ),
    )
}

fn error_text(error: &str, fallback: &str) -> Value {
    let error = if error.is_empty() { fallback } else { error };
    json!(clip(Some(&json!(error)), 200))
}

/// Every item now in Needs you, or an error when the list could not be read completely.
fn items(bot_name: &str) -> Result<Vec<Map<String, Value>>, String> {
    let mut items = Vec::new();
    let mut cursor = String::new();
    for _ in 0..PAGES {
        let mut argv = vec![
            "bot",
            "item",
            "list",
            "--bot",
            bot_name,
            "--status",
            "needs-you",
            "--limit",
            "100",
        ];
        if !cursor.is_empty() {
            argv.extend(["--cursor", cursor.as_str()]);
        }
        argv.push("--json");
        let result = orca_call(&argv, timeout());
        let Ok(page) = &result else {
            let error = result.err().unwrap_or_default();
            return Err(if error.is_empty() {
                "Orca returned no item list".to_owned()
            } else {
                error
            });
        };
        let Some(Value::Array(found)) = page.get("items") else {
            return Err("Orca returned no item list".to_owned());
        };
        for item in found {
            let Value::Object(item) = item else {
                continue;
            };
            let status = item.get("status").and_then(Value::as_str);
            if truthy(item.get("uid")) && matches!(status, Some("needs_you" | "needs-you")) {
                items.push(item.clone());
            }
        }
        let next = page.get("nextCursor");
        if !truthy(next) {
            return Ok(items);
        }
        cursor = py_str(next);
    }
    Err("too many Needs you items to page through".to_owned())
}

struct Episode {
    event: String,
    context: String,
    questions: Vec<String>,
    comment: String,
}

fn question(item: &Map<String, Value>) -> Option<Episode> {
    let question = item.get("question").and_then(Value::as_object)?;
    if !truthy(question.get("eventId")) {
        return None;
    }
    let asks: Vec<String> = question
        .get("decisions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|decision| decision.is_object() && truthy(decision.get("question")))
        .map(|decision| clip(decision.get("question"), 200))
        .collect();
    let has_asks = !asks.is_empty();
    Some(Episode {
        event: py_str(question.get("eventId")),
        context: if has_asks {
            clip(question.get("gist"), TEXT)
        } else {
            String::new()
        },
        questions: asks.into_iter().take(4).collect(),
        comment: if has_asks {
            String::new()
        } else {
            clip(question.get("text"), TEXT)
        },
    })
}

/// What put this item in Needs you: its open question, else the latest move there and the
/// comment after it.
fn episode(item: &Map<String, Value>) -> Option<Episode> {
    if let Some(found) = question(item) {
        return Some(found);
    }
    let uid = py_str(item.get("uid"));
    let result = orca_call(&["bot", "item", "show", &uid, "--json"], timeout()).ok()?;
    let events = result.get("events").and_then(Value::as_array)?;
    let (mut key, mut comment) = (String::new(), String::new());
    for event in events.iter().filter_map(Value::as_object) {
        let data = event
            .get("data")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let kind = event.get("kind").and_then(Value::as_str);
        if kind == Some("status") && data.get("to").and_then(Value::as_str) == Some("needs_you") {
            key = text(event.get("id"));
        } else if kind == Some("commented") && truthy(data.get("text")) {
            comment = clip(data.get("text"), TEXT);
        }
    }
    (!key.is_empty()).then_some(Episode {
        event: key,
        context: String::new(),
        questions: Vec::new(),
        comment,
    })
}

fn public(uid: &str, entry: &Value) -> Value {
    json!({"uid": uid, "event": entry.get("event").cloned().unwrap_or_else(|| json!("")),
           "number": entry.get("number").cloned().unwrap_or(Value::Null),
           "title": entry.get("title").cloned().unwrap_or_else(|| json!(""))})
}

fn number(value: Option<&Value>) -> f64 {
    value
        .filter(|value| truthy(Some(value)))
        .and_then(Value::as_f64)
        .unwrap_or_default()
}

/// Compares Orca's Needs-you items with what was already seen; nothing here runs a model.
#[must_use]
pub fn poll(bot_name: &str) -> Value {
    let items = match items(bot_name) {
        Ok(items) => items,
        Err(error) => return json!({"ok": false, "error": error_text(&error, "")}),
    };
    let Ok(lock) = Lock::take() else {
        return json!({"ok": false, "error": "Needs you state is unavailable"});
    };
    let known = load(bot_name);
    let baseline = known.is_none();
    let known = known.unwrap_or_default();
    let mut fresh = Map::new();
    for item in &items {
        let uid = py_str(item.get("uid"));
        let previous = known.get(&uid);
        let last = text(item.get("lastEventId"));
        if let Some(previous) = previous
            && !last.is_empty()
            && previous.get("last").and_then(Value::as_str) == Some(last.as_str())
        {
            fresh.insert(uid, previous.clone());
            continue;
        }
        let Some(episode) = episode(item) else {
            if let Some(previous) = previous {
                fresh.insert(uid, previous.clone());
            }
            continue;
        };
        if let Some(previous) = previous.filter(|previous| {
            previous.get("event").and_then(Value::as_str) == Some(episode.event.as_str())
        }) {
            let mut updated = previous.clone();
            updated["last"] = json!(last);
            fresh.insert(uid, updated);
            continue;
        }
        let chat = item
            .get("chat")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let chat = if chat.get("kind").and_then(Value::as_str) == Some("home") {
            String::new()
        } else {
            clip(chat.get("title"), 80)
        };
        fresh.insert(uid, json!({
            "event": episode.event, "last": last, "announced": baseline,
            "number": item.get("number").cloned().unwrap_or(Value::Null), "title": clip(item.get("title"), 200),
            "description": clip(item.get("description"), TEXT), "context": episode.context,
            "questions": episode.questions, "comment": episode.comment, "chat": chat, "seen": super::now(),
        }));
    }
    let saved = save(bot_name, &fresh);
    drop(lock);
    if saved.is_err() {
        return json!({"ok": false, "error": "Needs you state is unavailable"});
    }
    let mut pending: Vec<(f64, f64, Value)> = fresh
        .iter()
        .filter(|(_, entry)| !truthy(entry.get("announced")))
        .map(|(uid, entry)| {
            (
                number(entry.get("seen")),
                number(entry.get("number")),
                public(uid, entry),
            )
        })
        .collect();
    pending.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    json!({"ok": true, "baseline": baseline, "count": fresh.len(),
           "pending": pending.into_iter().map(|(_, _, entry)| entry).collect::<Vec<_>>()})
}

fn lines(entry: &Value) -> Vec<String> {
    let number = match entry.get("number") {
        None | Some(Value::Null) => String::new(),
        Some(number) => format!("#{} ", py_str(Some(number))),
    };
    let title = entry
        .get("title")
        .filter(|title| truthy(Some(title)))
        .map_or_else(|| "(untitled)".to_owned(), |title| py_str(Some(title)));
    let mut lines = vec![format!("Item {number}{title}")];
    let field = |key: &str| {
        entry
            .get(key)
            .filter(|value| truthy(Some(value)))
            .map(|value| py_str(Some(value)))
    };
    if let Some(chat) = field("chat") {
        lines.push(format!("From your chat: {chat}"));
    }
    if let Some(description) = field("description") {
        lines.push(format!("About: {description}"));
    }
    if let Some(context) = field("context") {
        lines.push(format!("Context: {context}"));
    }
    let questions: Vec<String> = entry
        .get("questions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|question| py_str(Some(question)))
        .collect();
    lines.extend(questions.iter().map(|question| format!("Asks: {question}")));
    if questions.is_empty()
        && let Some(comment) = field("comment")
    {
        lines.push(format!("Says: {comment}"));
    }
    lines
}

fn message(entries: &[&Value], home_chat: &str) -> String {
    let one = entries.len() == 1;
    let moved = if one {
        "This item just moved".to_owned()
    } else {
        format!("These {} items just moved", entries.len())
    };
    let each = if one { "it is" } else { "each is" };
    let mut out = vec![
        NEEDS_YOU_TAG.to_owned(),
        format!(
            "[Sent automatically by the Tohru desktop's Needs-you watcher, not typed by your owner. {moved} to Needs you. \
Tell your owner what {each} and what is being asked, in plain spoken sentences (one or two per item), by running exactly one \
`tohru assistant alert --chat {home_chat} --speak \"<summary>\"`, then reply here with the same summary. \
Don't answer the questions, change the items or start other work.]"
        ),
    ];
    for entry in entries {
        out.push(String::new());
        out.extend(lines(entry));
    }
    out.join("\n")
}

/// Wakes the bot once in its Home Chat for the pending items.
#[must_use]
pub fn wake(bot_name: &str, pairs: &[(String, String)]) -> Value {
    let Ok(lock) = Lock::take() else {
        return json!({"ok": false, "error": "Needs you state is unavailable"});
    };
    let mut known = load(bot_name).unwrap_or_default();
    let mut due: Vec<String> = Vec::new();
    for (uid, event) in pairs {
        let Some(entry) = known.get(uid) else {
            continue;
        };
        if entry.get("event").and_then(Value::as_str) == Some(event.as_str())
            && !truthy(entry.get("announced"))
        {
            due.push(uid.clone());
        }
    }
    if due.is_empty() {
        return json!({"ok": true, "woke": false, "items": []});
    }
    let bot = match orca_find_bot(bot_name) {
        Ok(bot) => bot,
        Err(error) => return json!({"ok": false, "error": error_text(&error, "")}),
    };
    let name = py_str(bot.get("name"));
    if truthy(bot.get("paused")) {
        return json!({"ok": false, "error": format!("Bot '{name}' is paused in Orca")});
    }
    let (chats, error) = match orca_bot_chats(&name, false) {
        Ok(chats) => (chats, String::new()),
        Err(error) => (Vec::new(), error),
    };
    let Some(home) = chats.iter().find(|chat| {
        truthy(chat.get("home")) && !truthy(chat.get("closed")) && truthy(chat.get("id"))
    }) else {
        return json!({"ok": false, "error": error_text(&error, "The bot has no Home Chat")});
    };
    let home_id = py_str(home.get("id"));
    let entries: Vec<&Value> = due.iter().filter_map(|uid| known.get(uid)).collect();
    if let Err(error) = orca_send_to_bot(&name, &message(&entries, &home_id), Some(&home_id)) {
        return json!({"ok": false, "error": error_text(&error, "Could not reach the bot")});
    }
    let now = super::now();
    for uid in &due {
        if let Some(entry) = known.get_mut(uid) {
            entry["announced"] = json!(true);
            entry["woke"] = json!(now);
        }
    }
    let _ = save(bot_name, &known);
    drop(lock);
    let items: Vec<Value> = due
        .iter()
        .filter_map(|uid| known.get(uid).map(|entry| public(uid, entry)))
        .collect();
    json!({"ok": true, "woke": true, "chat": home_id, "items": items})
}
