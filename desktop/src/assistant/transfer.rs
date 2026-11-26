//! Export and import of the assistant's history, memory and relationship.

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Map, Value, json};

use super::memory::{immediate, persona_text};
use super::orca::is_conversation_id;
use super::store::{Conversation, Turn, add_turn, load, save_conversation};
use super::{AssistantError, Result, assistant_settings, emit, write_private};
use crate::pyjson;
use crate::pyvalue::{py_str, truthy};

const FORMAT: &str = "tohru-assistant-export";
const SCHEMA: i64 = 1;
const VERSION: &str = "0.1.0";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
pub enum Section {
    History,
    Memory,
    Relationship,
}

impl Section {
    const ALL: [Self; 3] = [Self::History, Self::Memory, Self::Relationship];

    fn name(self) -> &'static str {
        match self {
            Self::History => "history",
            Self::Memory => "memory",
            Self::Relationship => "relationship",
        }
    }
}

pub fn export_state(db: &Connection) -> Result<Value> {
    let ids: Vec<String> = db
        .prepare("SELECT id FROM conversations ORDER BY id")?
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut conversations = Vec::new();
    for id in ids {
        let Some(conversation) = load(db, &id)? else {
            continue;
        };
        conversations.push(json!({
            "id": conversation.id, "title": conversation.title, "provider": conversation.provider,
            "created": conversation.created, "updated": conversation.updated, "voice": conversation.voice,
            "forgotten": conversation.forgotten, "remembered": conversation.remembered, "sessions": conversation.sessions,
            "turns": conversation.turns.iter().map(Turn::json).collect::<Vec<_>>(),
        }));
    }
    let settings = assistant_settings();
    let file = settings
        .get("assistant.persona_file")
        .cloned()
        .unwrap_or_else(|| json!(""));
    let persona = json!({"enabled": settings.get("assistant.persona").cloned().unwrap_or(Value::Bool(true)),
                         "file": file, "text": persona_text(true, file.as_str().unwrap_or_default())});
    let memories: Vec<Value> = db
        .prepare("SELECT id, kind, text, source_conversation, created, updated, confidence FROM memories ORDER BY id")?
        .query_map([], |row| {
            Ok(json!({"id": row.get::<_, String>(0)?, "kind": row.get::<_, String>(1)?, "text": row.get::<_, String>(2)?,
                      "source_conversation": row.get::<_, String>(3)?, "created": row.get::<_, i64>(4)?,
                      "updated": row.get::<_, i64>(5)?, "confidence": row.get::<_, f64>(6)?}))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let summaries: Vec<Value> = db
        .prepare("SELECT conversation_id, text, created, updated FROM summaries ORDER BY conversation_id")?
        .query_map([], |row| {
            Ok(json!({"conversation_id": row.get::<_, String>(0)?, "text": row.get::<_, String>(1)?,
                      "created": row.get::<_, i64>(2)?, "updated": row.get::<_, i64>(3)?}))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let rows: Vec<(String, String, i64)> = db
        .prepare("SELECT key, value, updated FROM relationship ORDER BY key")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut relationship = Map::new();
    for (key, value, updated) in rows {
        relationship.insert(
            key,
            json!({"value": serde_json::from_str::<Value>(&value)?, "updated": updated}),
        );
    }
    let exported = jiff::Timestamp::now()
        .strftime("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    Ok(json!({
        "format": FORMAT, "schema": SCHEMA, "tohru_version": VERSION, "exported": exported,
        "settings": settings, "persona": persona, "conversations": conversations, "memories": memories,
        "summaries": summaries, "relationship": relationship,
    }))
}

pub fn export_command(file: Option<&str>) -> Result<u8> {
    let db = super::store::connect(None)?;
    let text = format!("{}\n", pyjson::indented(&export_state(&db)?, false));
    if let Some(file) = file.filter(|file| !file.is_empty() && *file != "-") {
        write_private(std::path::Path::new(file), &text)?;
    } else {
        use std::io::Write as _;
        let mut out = std::io::stdout().lock();
        out.write_all(text.as_bytes())?;
        out.flush()?;
    }
    Ok(0)
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Invalid(String);

fn require(condition: bool, message: impl FnOnce() -> String) -> std::result::Result<(), Invalid> {
    if condition {
        Ok(())
    } else {
        Err(Invalid(message()))
    }
}

fn is_int(value: Option<&Value>) -> bool {
    value.is_some_and(|value| value.is_i64() || value.is_u64())
}

fn hex32(value: Option<&Value>) -> bool {
    value.and_then(Value::as_str).is_some_and(|text| {
        text.len() == 32
            && text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn is_str_or_missing(value: Option<&Value>) -> bool {
    value.is_none_or(Value::is_string)
}

fn list<'a>(data: &'a Map<String, Value>, name: &str) -> &'a [Value] {
    data.get(name)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// Brings an older export up to the current schema, then checks every record's shape.
#[expect(
    clippy::too_many_lines,
    reason = "one ordered sequence of checks, kept together to read against the protocol"
)]
pub fn validate_export(data: &Value) -> std::result::Result<Map<String, Value>, Invalid> {
    let Value::Object(data) = data else {
        return Err(Invalid("not a Tohru assistant export".to_owned()));
    };
    require(
        data.get("format").and_then(Value::as_str) == Some(FORMAT),
        || "not a Tohru assistant export".to_owned(),
    )?;
    let schema = data.get("schema");
    require(
        is_int(schema)
            && schema
                .and_then(Value::as_i64)
                .is_some_and(|schema| schema >= 1),
        || "missing schema version".to_owned(),
    )?;
    let schema = schema.and_then(Value::as_i64).unwrap_or_default();
    require(schema <= SCHEMA, || {
        format!("export schema {schema} is newer than this Tohru ({SCHEMA})")
    })?;
    for name in ["conversations", "memories", "summaries"] {
        require(data.get(name).is_none_or(Value::is_array), || {
            format!("{name} must be a list")
        })?;
    }
    require(
        data.get("relationship").is_none_or(Value::is_object),
        || "relationship must be an object".to_owned(),
    )?;
    for conversation in list(data, "conversations") {
        let id = conversation.get("id").and_then(Value::as_str);
        require(
            conversation.is_object() && id.is_some_and(is_conversation_id),
            || "bad conversation id".to_owned(),
        )?;
        let id = id.unwrap_or_default();
        let field = |key: &str| conversation.get(key);
        require(
            field("title").is_some_and(Value::is_string)
                && field("provider").is_some_and(Value::is_string),
            || format!("conversation {id}: title and provider"),
        )?;
        require(is_int(field("created")) && is_int(field("updated")), || {
            format!("conversation {id}: times")
        })?;
        require(field("sessions").is_none_or(Value::is_object), || {
            format!("conversation {id}: sessions")
        })?;
        let Some(Value::Array(turns)) = field("turns") else {
            return Err(Invalid(format!("conversation {id}: turns")));
        };
        for turn in turns {
            require(turn.is_object() && hex32(turn.get("id")), || {
                "bad turn id".to_owned()
            })?;
            let turn_id = py_str(turn.get("id"));
            let role = turn.get("role").and_then(Value::as_str);
            require(
                matches!(role, Some("user" | "assistant"))
                    && turn.get("text").is_some_and(Value::is_string)
                    && is_int(turn.get("time")),
                || format!("turn {turn_id}"),
            )?;
            require(
                is_str_or_missing(turn.get("error")) && is_str_or_missing(turn.get("provider")),
                || format!("turn {turn_id}"),
            )?;
            require(is_str_or_missing(turn.get("delivery")), || {
                format!("turn {turn_id}: delivery")
            })?;
        }
    }
    for memory in list(data, "memories") {
        require(memory.is_object() && hex32(memory.get("id")), || {
            "bad memory id".to_owned()
        })?;
        let id = py_str(memory.get("id"));
        let kind = memory.get("kind").and_then(Value::as_str);
        let text = memory.get("text").and_then(Value::as_str);
        require(
            matches!(kind, Some("fact" | "preference"))
                && text.is_some_and(|text| !text.trim().is_empty()),
            || format!("memory {id}"),
        )?;
        require(
            is_int(memory.get("created")) && is_int(memory.get("updated")),
            || format!("memory {id}: times"),
        )?;
        let confidence = memory.get("confidence").and_then(Value::as_f64);
        require(
            confidence.is_some_and(|confidence| (0.0..=1.0).contains(&confidence)),
            || format!("memory {id}: confidence"),
        )?;
        require(is_str_or_missing(memory.get("source_conversation")), || {
            format!("memory {id}: source")
        })?;
    }
    for summary in list(data, "summaries") {
        require(
            summary.get("conversation_id").is_some_and(Value::is_string)
                && summary.get("text").is_some_and(Value::is_string),
            || "bad summary".to_owned(),
        )?;
        require(
            is_int(summary.get("created")) && is_int(summary.get("updated")),
            || "summary times".to_owned(),
        )?;
    }
    if let Some(Value::Object(relationship)) = data.get("relationship") {
        for (key, entry) in relationship {
            require(
                entry.is_object() && entry.get("value").is_some() && is_int(entry.get("updated")),
                || format!("relationship {key}"),
            )?;
        }
    }
    Ok(data.clone())
}

fn text(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn int(value: Option<&Value>) -> i64 {
    value.and_then(Value::as_i64).unwrap_or_default()
}

fn turn_of(value: &Value) -> Turn {
    Turn {
        id: text(value.get("id")),
        role: text(value.get("role")),
        text: text(value.get("text")),
        time: int(value.get("time")),
        provider: text(value.get("provider")),
        voice: truthy(value.get("voice")),
        stopped: truthy(value.get("stopped")),
        error: text(value.get("error")),
        delivery: text(value.get("delivery")),
    }
}

fn newer(db: &Connection, sql: &str, key: &str, updated: i64) -> rusqlite::Result<bool> {
    let row: Option<i64> = db.query_row(sql, [key], |row| row.get(0)).optional()?;
    Ok(row.is_some_and(|current| current >= updated))
}

#[expect(
    clippy::too_many_lines,
    reason = "one ordered sequence of checks, kept together to read against the protocol"
)]
pub fn import_state(
    db: &Connection,
    data: &Map<String, Value>,
    replace: bool,
    sections: &[Section],
) -> Result<Value> {
    let mut counts = [0_usize; 4];
    let transaction = immediate(db)?;
    let has = |section: Section| sections.contains(&section);
    if replace {
        if has(Section::History) {
            transaction.execute("DELETE FROM conversations", [])?;
        }
        if has(Section::Memory) {
            transaction.execute_batch("DELETE FROM memories; DELETE FROM summaries;")?;
        }
        if has(Section::Relationship) {
            transaction.execute("DELETE FROM relationship", [])?;
        }
    }
    if has(Section::History) {
        for conversation in list(data, "conversations") {
            let id = text(conversation.get("id"));
            let updated = int(conversation.get("updated"));
            if newer(
                &transaction,
                "SELECT updated FROM conversations WHERE id = ?",
                &id,
                updated,
            )? {
                continue;
            }
            transaction.execute("DELETE FROM conversations WHERE id = ?", [&id])?;
            let remembered = conversation
                .get("remembered")
                .filter(|value| truthy(Some(value)));
            let record = Conversation {
                id: id.clone(),
                title: text(conversation.get("title")),
                provider: text(conversation.get("provider")),
                created: int(conversation.get("created")),
                updated,
                voice: truthy(conversation.get("voice")),
                sessions: conversation
                    .get("sessions")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default(),
                forgotten: truthy(conversation.get("forgotten")),
                remembered: remembered
                    .and_then(Value::as_f64)
                    .map_or(0, super::truncate),
                turns: Vec::new(),
            };
            save_conversation(&transaction, &record)?;
            for turn in conversation
                .get("turns")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                add_turn(&transaction, &id, &mut turn_of(turn))?;
            }
            counts[0] += 1;
        }
    }
    if has(Section::Memory) {
        for memory in list(data, "memories") {
            let id = text(memory.get("id"));
            if newer(
                &transaction,
                "SELECT updated FROM memories WHERE id = ?",
                &id,
                int(memory.get("updated")),
            )? {
                continue;
            }
            transaction.execute("DELETE FROM memories WHERE id = ?", [&id])?;
            transaction.execute(
                "INSERT INTO memories (id, kind, text, source_conversation, created, updated, confidence) VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![id, text(memory.get("kind")), text(memory.get("text")), text(memory.get("source_conversation")),
                        int(memory.get("created")), int(memory.get("updated")), memory.get("confidence").and_then(Value::as_f64).unwrap_or_default()],
            )?;
            counts[1] += 1;
        }
        for summary in list(data, "summaries") {
            let id = text(summary.get("conversation_id"));
            if newer(
                &transaction,
                "SELECT updated FROM summaries WHERE conversation_id = ?",
                &id,
                int(summary.get("updated")),
            )? {
                continue;
            }
            transaction.execute(
                "INSERT INTO summaries (conversation_id, text, created, updated) VALUES (?, ?, ?, ?)
                 ON CONFLICT(conversation_id) DO UPDATE SET text = excluded.text, created = excluded.created, updated = excluded.updated",
                params![id, text(summary.get("text")), int(summary.get("created")), int(summary.get("updated"))],
            )?;
            counts[2] += 1;
        }
    }
    if has(Section::Relationship)
        && let Some(Value::Object(relationship)) = data.get("relationship")
    {
        for (key, entry) in relationship {
            let updated = int(entry.get("updated"));
            if newer(
                &transaction,
                "SELECT updated FROM relationship WHERE key = ?",
                key,
                updated,
            )? {
                continue;
            }
            let value = pyjson::render(
                entry.get("value").unwrap_or(&Value::Null),
                pyjson::Style {
                    ascii: true,
                    sort_keys: true,
                    compact: false,
                },
            );
            transaction.execute(
                    "INSERT INTO relationship (key, value, updated) VALUES (?, ?, ?)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated = excluded.updated",
                    params![key, value, updated],
                )?;
            counts[3] += 1;
        }
    }
    transaction.commit()?;
    Ok(
        json!({"conversations": counts[0], "memories": counts[1], "summaries": counts[2], "relationship": counts[3]}),
    )
}

pub fn import_command(file: &str, replace: bool, yes: bool, only: &[Section]) -> Result<u8> {
    if replace && !yes {
        return Err(AssistantError::exit(
            "tohru-assistant: import --replace wipes the assistant's state first; add --yes",
        ));
    }
    let parsed = std::fs::read_to_string(file)
        .map_err(|error| error.to_string())
        .and_then(|raw| serde_json::from_str::<Value>(&raw).map_err(|error| error.to_string()))
        .and_then(|data| validate_export(&data).map_err(|error| error.to_string()));
    let data = match parsed {
        Ok(data) => data,
        Err(error) => {
            eprintln!("tohru-assistant: cannot import {file}: {error}");
            return Ok(1);
        }
    };
    let mut sections: Vec<Section> = if only.is_empty() {
        Section::ALL.to_vec()
    } else {
        only.to_vec()
    };
    sections.sort();
    sections.dedup();
    let counts = import_state(&super::store::connect(None)?, &data, replace, &sections)?;
    let mut names: Vec<&str> = sections.iter().map(|section| section.name()).collect();
    names.sort_unstable();
    emit(
        json!({"imported": counts, "mode": if replace { "replace" } else { "merge" }, "only": names}),
    );
    Ok(0)
}
