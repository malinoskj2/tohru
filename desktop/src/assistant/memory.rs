use std::sync::LazyLock;

use regex::Regex;
use rusqlite::{Connection, Transaction, TransactionBehavior, params};
use serde_json::{Map, Value, json};

use super::store::{self, Conversation, casefold, chars, squash, strftime};
use super::{AssistantError, Result};
use crate::pyjson;
use crate::pyvalue::py_str;

pub const VOICE_RULE: &str = "This answer will be spoken aloud. Reply in one to three short sentences of plain speech: no markdown, lists, code, links or URLs. If the full answer would be long, give the gist and offer to show the rest in the chat popup. Preserve essential qualifications, uncertainty and tool outcomes; never claim an action succeeded before its tool result. Lead with the answer, use natural spoken wording and avoid reading out paths or technical markup.";
pub const TYPED_RULE: &str =
    "This chat is typed: keep the persona to a light touch and use markdown when it helps.";
pub const MEMORY_TOKENS: usize = 600;
const MEMORY_HEADER: &str = "What you remember about the user from earlier chats (use it naturally; never recite it or mention this list):";
pub const EXTRACT_PROMPT: &str = r#"You maintain the long-term memory of a desktop assistant called Tohru. Read the conversation below and reply with ONLY a JSON object, no prose and no code fence, of this exact shape:
{"summary": "one or two plain lines on what the conversation was about",
 "facts": [{"text": "a durable fact about the user, in the third person", "kind": "fact" or "preference", "confidence": 0.0 to 1.0}],
 "nicknames": ["names the user called Tohru, if any"],
 "topics": ["short recurring-topic labels"],
 "jokes": ["inside jokes worth remembering, if any"]}
Only record what the user said about themselves and would plausibly want remembered: no secrets, passwords, keys, health, financial or other sensitive details, nothing about third parties, and nothing the assistant guessed. Use empty lists when there is nothing. At most 8 facts.

Conversation:
"#;
const STOPWORDS: [&str; 19] = [
    "a", "an", "the", "my", "me", "i", "your", "our", "about", "of", "that", "this", "what", "and",
    "or", "to", "is", "was", "please",
];

pub static FORGET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*(?:(?:please|ok(?:ay)?|hey|tohru)[\s,]+)*forget\s+(?:that|this|what\s+i\s+(?:just\s+)?said|about\s+(?P<about>.+?)|(?P<thing>.+?))\s*[.!]*\s*$")
        .expect("fixed pattern is valid")
});
static FENCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)^```(?:json)?\s*(.*?)\s*```$").expect("fixed pattern is valid")
});

/// What a "forget …" request asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum Forget {
    /// "forget that": the conversation so far.
    Conversation,
    /// "forget about X" or "forget X".
    About(String),
}

#[must_use]
pub fn forget_request(text: &str) -> Option<Forget> {
    let captures = FORGET.captures(text)?;
    let subject = captures.name("about").or_else(|| captures.name("thing"));
    Some(subject.map_or(Forget::Conversation, |subject| {
        Forget::About(subject.as_str().to_owned())
    }))
}

/// `BEGIN IMMEDIATE` on a shared connection, committed by the caller.
pub fn immediate(db: &Connection) -> Result<Transaction<'_>> {
    Ok(Transaction::new_unchecked(
        db,
        TransactionBehavior::Immediate,
    )?)
}

#[must_use]
pub fn persona_text(enabled: bool, override_path: &str) -> String {
    if !enabled {
        return String::new();
    }
    let mut candidates = Vec::new();
    if !override_path.is_empty() {
        candidates.push(std::path::PathBuf::from(override_path));
    }
    candidates.extend(crate::paths::shell_candidates("assistant/persona.md"));
    candidates
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .map(|text| text.trim().to_owned())
        .find(|text| !text.is_empty())
        .map(|text| chars(&text, 8000))
        .unwrap_or_default()
}

#[must_use]
pub fn tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

pub fn relationship(db: &Connection) -> Result<Map<String, Value>> {
    let rows: Vec<(String, String)> = db
        .prepare("SELECT key, value FROM relationship")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    rows.into_iter()
        .map(|(key, value)| Ok((key, serde_json::from_str(&value)?)))
        .collect()
}

fn familiarity_word(value: f64) -> &'static str {
    if value < 0.1 {
        "you have only just met"
    } else if value < 0.35 {
        "you are getting to know each other"
    } else if value < 0.7 {
        "you know each other fairly well"
    } else {
        "you are old friends"
    }
}

fn number(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Number(number)) => number.as_f64().unwrap_or_default(),
        Some(Value::Bool(flag)) => f64::from(u8::from(*flag)),
        Some(Value::String(text)) => text.trim().parse().unwrap_or_default(),
        _ => 0.0,
    }
}

/// Python `int()` of a stored relationship number.
fn integer(value: Option<&Value>, default: i64) -> i64 {
    match value {
        None => default,
        Some(Value::Number(number)) => number
            .as_i64()
            .unwrap_or_else(|| super::truncate(number.as_f64().unwrap_or_default())),
        Some(other) => super::truncate(number(Some(other))),
    }
}

fn strings(value: Option<&Value>, limit: usize) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .take(limit)
                .map(|item| py_str(Some(item)))
                .collect()
        })
        .unwrap_or_default()
}

fn plus_one(value: &Value) -> String {
    match value.as_i64() {
        Some(count) => (count + 1).to_string(),
        None => py_str(Some(&json!(value.as_f64().unwrap_or_default() + 1.0))),
    }
}

fn relationship_lines(state: &Map<String, Value>) -> Vec<String> {
    let mut lines = Vec::new();
    let Some(conversations) = state
        .get("conversations")
        .filter(|value| crate::pyvalue::truthy(Some(value)))
    else {
        return lines;
    };
    let first_met = state
        .get("first_met")
        .map_or_else(super::now, |value| integer(Some(value), 0));
    let mut parts = vec![
        format!("You first met on {}", strftime("%-d %B %Y", first_met)),
        format!("this is conversation {}", plus_one(conversations)),
        familiarity_word(number(state.get("familiarity"))).to_owned(),
    ];
    if number(state.get("streak")) > 1.0 {
        parts.push(format!(
            "they have talked to you {} days running",
            py_str(state.get("streak"))
        ));
    }
    let nicknames = strings(state.get("nicknames"), 3);
    if !nicknames.is_empty() {
        parts.push(format!("they call you {}", nicknames.join(", ")));
    }
    lines.push(format!("- {}.", parts.join("; ")));
    let topics = strings(state.get("topics"), 6);
    if !topics.is_empty() {
        lines.push(format!("- Recurring topics: {}.", topics.join(", ")));
    }
    let jokes = strings(state.get("jokes"), 3);
    if !jokes.is_empty() {
        lines.push(format!("- Inside jokes: {}.", jokes.join("; ")));
    }
    lines
}

/// What Tohru remembers, cut to the token budget: the relationship, facts that match the prompt,
/// then recent ones, then the last summaries.
pub fn memory_block(db: &Connection, prompt: &str, budget: usize) -> Result<String> {
    let mut lines = relationship_lines(&relationship(db)?);
    let fact =
        |row: &rusqlite::Row<'_>| Ok((row.get::<_, String>("id")?, row.get::<_, String>("text")?));
    let mut facts: Vec<(String, String)> = Vec::new();
    let query = store::fts_query(prompt);
    if !query.is_empty() {
        let matched = db
            .prepare(
                "SELECT m.* FROM memories_fts f JOIN memories m ON m.rowid = f.rowid
                 WHERE memories_fts MATCH ? ORDER BY bm25(memories_fts) LIMIT 12",
            )
            .and_then(|mut statement| {
                statement
                    .query_map([&query], fact)?
                    .collect::<Result<Vec<_>, _>>()
            });
        facts.extend(matched.unwrap_or_default());
    }
    facts.extend(
        db.prepare("SELECT * FROM memories ORDER BY updated DESC, confidence DESC LIMIT 24")?
            .query_map([], fact)?
            .collect::<Result<Vec<_>, _>>()?,
    );
    let summaries: Vec<String> = db
        .prepare("SELECT text FROM summaries ORDER BY updated DESC LIMIT 3")?
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    let mut used = tokens(MEMORY_HEADER) + lines.iter().map(|line| tokens(line) + 1).sum::<usize>();
    if used > budget {
        lines.clear();
        used = tokens(MEMORY_HEADER);
    }
    let mut seen = std::collections::HashSet::new();
    for (id, text) in facts {
        if !seen.insert(id) {
            continue;
        }
        let line = format!("- {text}");
        if used + tokens(&line) + 1 > budget {
            break;
        }
        used += tokens(&line) + 1;
        lines.push(line);
    }
    for text in summaries {
        let line = format!("- Earlier: {text}");
        if used + tokens(&line) + 1 > budget {
            break;
        }
        used += tokens(&line) + 1;
        lines.push(line);
    }
    Ok(if lines.is_empty() {
        String::new()
    } else {
        format!("{MEMORY_HEADER}\n{}", lines.join("\n"))
    })
}

#[must_use]
pub fn instructions(persona: &str, block: &str, voice: bool, orca: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if voice && persona.is_empty() {
        parts.push(VOICE_RULE);
    }
    let rule = match (voice, persona.is_empty()) {
        (_, true) => "",
        (true, false) => VOICE_RULE,
        (false, false) => TYPED_RULE,
    };
    parts.extend(
        [persona, rule, orca, block]
            .into_iter()
            .filter(|part| !part.is_empty()),
    );
    parts.join("\n\n")
}

/// Validated memories extracted from a provider's reply.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Extraction {
    pub summary: String,
    pub facts: Vec<Fact>,
    pub nicknames: Vec<String>,
    pub topics: Vec<String>,
    pub jokes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Fact {
    pub text: String,
    pub kind: String,
    pub confidence: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum ExtractionError {
    #[error("{0}")]
    Shape(&'static str),
    #[error("invalid JSON")]
    Json(#[from] serde_json::Error),
}

fn contains_folded(list: &[String], item: &str) -> bool {
    let folded = casefold(item);
    list.iter().any(|existing| casefold(existing) == folded)
}

fn clean_list(
    value: Option<&Value>,
    limit: usize,
    length: usize,
) -> Result<Vec<String>, ExtractionError> {
    let items = match value {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Array(items)) => items,
        Some(_) => return Err(ExtractionError::Shape("expected a list of strings")),
    };
    let mut out: Vec<String> = Vec::new();
    for item in items {
        let Value::String(item) = item else {
            return Err(ExtractionError::Shape("expected a list of strings"));
        };
        let item = chars(&squash(item), length);
        if !item.is_empty() && !contains_folded(&out, &item) {
            out.push(item);
        }
    }
    out.truncate(limit);
    Ok(out)
}

fn only_keys(object: &Map<String, Value>, allowed: &[&str]) -> bool {
    object.keys().all(|key| allowed.contains(&key.as_str()))
}

pub fn parse_extraction(text: &str) -> Result<Extraction, ExtractionError> {
    let mut text = text.trim();
    if let Some(captures) = FENCE.captures(text) {
        text = captures.get(1).map_or("", |inner| inner.as_str());
    }
    let data: Value = serde_json::from_str(text)?;
    let Value::Object(data) = data else {
        return Err(ExtractionError::Shape("unexpected keys"));
    };
    if !only_keys(&data, &["summary", "facts", "nicknames", "topics", "jokes"]) {
        return Err(ExtractionError::Shape("unexpected keys"));
    }
    let summary = match data.get("summary") {
        Some(Value::String(summary))
            if !summary.trim().is_empty() && summary.chars().count() <= 400 =>
        {
            summary
        }
        _ => {
            return Err(ExtractionError::Shape(
                "summary must be one or two short lines",
            ));
        }
    };
    let empty = Vec::new();
    let facts = match data.get("facts") {
        None => &empty,
        Some(Value::Array(facts)) if facts.len() <= 8 => facts,
        Some(_) => return Err(ExtractionError::Shape("facts must be a short list")),
    };
    let mut clean = Vec::new();
    for fact in facts {
        let Value::Object(fact) = fact else {
            return Err(ExtractionError::Shape("bad fact"));
        };
        if !only_keys(fact, &["text", "kind", "confidence"]) {
            return Err(ExtractionError::Shape("bad fact"));
        }
        let body = match fact.get("text") {
            Some(Value::String(body)) if !body.trim().is_empty() && body.chars().count() <= 240 => {
                body
            }
            _ => return Err(ExtractionError::Shape("bad fact text")),
        };
        let kind = match fact.get("kind") {
            None => "fact",
            Some(Value::String(kind)) if kind == "fact" || kind == "preference" => kind,
            Some(_) => return Err(ExtractionError::Shape("bad fact kind")),
        };
        let confidence = match fact.get("confidence") {
            None => 0.6,
            Some(Value::Number(confidence)) => confidence.as_f64().unwrap_or(-1.0),
            Some(_) => -1.0,
        };
        if !(0.0..=1.0).contains(&confidence) {
            return Err(ExtractionError::Shape("bad confidence"));
        }
        clean.push(Fact {
            text: squash(body),
            kind: kind.to_owned(),
            confidence,
        });
    }
    Ok(Extraction {
        summary: squash(summary),
        facts: clean,
        nicknames: clean_list(data.get("nicknames"), 5, 40)?,
        topics: clean_list(data.get("topics"), 8, 40)?,
        jokes: clean_list(data.get("jokes"), 5, 160)?,
    })
}

fn merge_list(old: Option<&Value>, new: &[String], limit: usize) -> Value {
    let mut out: Vec<Value> = old.and_then(Value::as_array).cloned().unwrap_or_default();
    for item in new {
        let folded = casefold(item);
        if !out
            .iter()
            .any(|existing| casefold(&py_str(Some(existing))) == folded)
        {
            out.push(json!(item));
        }
    }
    let skip = out.len().saturating_sub(limit);
    Value::Array(out.split_off(skip))
}

fn relationship_json(value: &Value) -> String {
    pyjson::render(
        value,
        pyjson::Style {
            ascii: true,
            sort_keys: true,
            compact: false,
        },
    )
}

pub fn store_memories(
    db: &Connection,
    conversation: &Conversation,
    found: &Extraction,
    now: i64,
) -> Result<()> {
    let cid = &conversation.id;
    let transaction = immediate(db)?;
    for fact in &found.facts {
        let row: Option<(String, f64)> = transaction
            .query_row(
                "SELECT id, confidence FROM memories WHERE lower(text) = lower(?)",
                [&fact.text],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map(Some)
            .or_else(|error| {
                if error == rusqlite::Error::QueryReturnedNoRows {
                    Ok(None)
                } else {
                    Err(error)
                }
            })?;
        if let Some((id, confidence)) = row {
            transaction.execute(
                "UPDATE memories SET updated = ?, confidence = ?, kind = ?, source_conversation = ? WHERE id = ?",
                params![now, confidence.max(fact.confidence), fact.kind, cid, id],
            )?;
        } else {
            transaction.execute(
                "INSERT INTO memories (id, kind, text, source_conversation, created, updated, confidence) VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![uuid::Uuid::new_v4().simple().to_string(), fact.kind, fact.text, cid, now, now, fact.confidence],
            )?;
        }
    }
    transaction.execute(
        "INSERT INTO summaries (conversation_id, text, created, updated) VALUES (?, ?, ?, ?)
         ON CONFLICT(conversation_id) DO UPDATE SET text = excluded.text, updated = excluded.updated",
        params![cid, found.summary, now, now],
    )?;
    let mut state = relationship(&transaction)?;
    let counted = state
        .get("counted")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !counted.iter().any(|item| item.as_str() == Some(cid)) {
        let count = integer(state.get("conversations"), 0) + 1;
        let created = conversation.created;
        let today = strftime("%Y-%m-%d", created);
        let yesterday = strftime("%Y-%m-%d", created - 86400);
        let last_day = state.get("last_day").and_then(Value::as_str);
        let streak = integer(state.get("streak"), 0);
        let streak = if last_day == Some(today.as_str()) {
            streak
        } else if last_day == Some(yesterday.as_str()) {
            streak + 1
        } else {
            1
        };
        let mut counted = counted;
        counted.push(json!(cid));
        let skip = counted.len().saturating_sub(500);
        let familiarity = (1.0 - (-super::float(count) / 40.0).exp()) * 10000.0;
        state.insert(
            "first_met".into(),
            json!(integer(state.get("first_met"), created).min(created)),
        );
        state.insert("conversations".into(), json!(count));
        state.insert("familiarity".into(), json!(familiarity.round() / 10000.0));
        state.insert("streak".into(), json!(streak));
        state.insert("last_day".into(), json!(today));
        state.insert("counted".into(), Value::Array(counted.split_off(skip)));
    }
    state.insert(
        "last_seen".into(),
        json!(integer(state.get("last_seen"), 0).max(conversation.updated)),
    );
    state.insert(
        "nicknames".into(),
        merge_list(state.get("nicknames"), &found.nicknames, 5),
    );
    state.insert(
        "topics".into(),
        merge_list(state.get("topics"), &found.topics, 12),
    );
    state.insert(
        "jokes".into(),
        merge_list(state.get("jokes"), &found.jokes, 8),
    );
    for (key, value) in &state {
        transaction.execute(
            "INSERT INTO relationship (key, value, updated) VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated = excluded.updated",
            params![key, relationship_json(value), now],
        )?;
    }
    transaction.execute(
        "UPDATE conversations SET remembered = ? WHERE id = ?",
        params![conversation.updated, cid],
    )?;
    transaction.commit()?;
    Ok(())
}

pub fn forget_conversation(db: &Connection, cid: &str) -> Result<()> {
    let transaction = immediate(db)?;
    transaction.execute("DELETE FROM memories WHERE source_conversation = ?", [cid])?;
    transaction.execute("DELETE FROM summaries WHERE conversation_id = ?", [cid])?;
    transaction.execute("UPDATE conversations SET forgotten = 1 WHERE id = ?", [cid])?;
    transaction.commit()?;
    Ok(())
}

pub fn forget_about(db: &Connection, subject: &str) -> Result<usize> {
    let words: Vec<String> = store::words(&casefold(subject))
        .into_iter()
        .filter(|word| word.chars().count() >= 2 && !STOPWORDS.contains(&word.as_str()))
        .collect();
    if words.is_empty() {
        return Ok(0);
    }
    let query = words
        .iter()
        .map(|word| format!("\"{word}\""))
        .collect::<Vec<_>>()
        .join(" AND ");
    let transaction = immediate(db)?;
    let ids: Vec<String> = transaction
        .prepare("SELECT m.id, m.text FROM memories_fts f JOIN memories m ON m.rowid = f.rowid WHERE memories_fts MATCH ?")?
        .query_map([&query], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    for id in &ids {
        transaction.execute("DELETE FROM memories WHERE id = ?", [id])?;
    }
    transaction.commit()?;
    Ok(ids.len())
}

#[must_use]
pub fn transcript_prompt(turns: &[store::Turn], prompt: &str) -> String {
    let mut lines = vec!["Earlier in this conversation (with another assistant):".to_owned()];
    for turn in turns {
        let who = if turn.role == "user" {
            "User"
        } else {
            "Assistant"
        };
        lines.push(format!("{who}: {}", turn.text));
        if !turn.delivery.is_empty() {
            lines.push(turn.delivery.clone());
        }
    }
    lines.push(String::new());
    lines.push(prompt.to_owned());
    lines.join("\n")
}

impl From<ExtractionError> for AssistantError {
    fn from(error: ExtractionError) -> Self {
        Self::Exit(error.to_string())
    }
}
