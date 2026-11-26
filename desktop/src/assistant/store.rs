use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};
use serde_json::{Map, Value, json};

use super::{AssistantError, data_dir, private_dir};
use crate::pyjson;

/// Applied in order; `PRAGMA user_version` counts those applied. Never edit a released one.
const MIGRATIONS: [&str; 1] = [r"
    CREATE TABLE conversations (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        provider TEXT NOT NULL,
        created INTEGER NOT NULL,
        updated INTEGER NOT NULL,
        voice INTEGER NOT NULL DEFAULT 0,
        sessions TEXT NOT NULL DEFAULT '{}',
        forgotten INTEGER NOT NULL DEFAULT 0,
        remembered INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE turns (
        rowid INTEGER PRIMARY KEY,
        id TEXT NOT NULL UNIQUE,
        conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
        seq INTEGER NOT NULL,
        role TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
        text TEXT NOT NULL,
        time INTEGER NOT NULL,
        provider TEXT NOT NULL DEFAULT '',
        voice INTEGER NOT NULL DEFAULT 0,
        stopped INTEGER NOT NULL DEFAULT 0,
        error TEXT NOT NULL DEFAULT '',
        UNIQUE (conversation_id, seq)
    );
    CREATE VIRTUAL TABLE turns_fts USING fts5(text, content='turns', content_rowid='rowid');
    CREATE TRIGGER turns_ai AFTER INSERT ON turns BEGIN
        INSERT INTO turns_fts(rowid, text) VALUES (new.rowid, new.text);
    END;
    CREATE TRIGGER turns_ad AFTER DELETE ON turns BEGIN
        INSERT INTO turns_fts(turns_fts, rowid, text) VALUES ('delete', old.rowid, old.text);
    END;
    CREATE TRIGGER turns_au AFTER UPDATE ON turns BEGIN
        INSERT INTO turns_fts(turns_fts, rowid, text) VALUES ('delete', old.rowid, old.text);
        INSERT INTO turns_fts(rowid, text) VALUES (new.rowid, new.text);
    END;
    CREATE TABLE memories (
        rowid INTEGER PRIMARY KEY,
        id TEXT NOT NULL UNIQUE,
        kind TEXT NOT NULL CHECK (kind IN ('fact', 'preference')),
        text TEXT NOT NULL,
        source_conversation TEXT NOT NULL DEFAULT '',
        created INTEGER NOT NULL,
        updated INTEGER NOT NULL,
        confidence REAL NOT NULL CHECK (confidence >= 0 AND confidence <= 1)
    );
    CREATE VIRTUAL TABLE memories_fts USING fts5(text, content='memories', content_rowid='rowid');
    CREATE TRIGGER memories_ai AFTER INSERT ON memories BEGIN
        INSERT INTO memories_fts(rowid, text) VALUES (new.rowid, new.text);
    END;
    CREATE TRIGGER memories_ad AFTER DELETE ON memories BEGIN
        INSERT INTO memories_fts(memories_fts, rowid, text) VALUES ('delete', old.rowid, old.text);
    END;
    CREATE TRIGGER memories_au AFTER UPDATE ON memories BEGIN
        INSERT INTO memories_fts(memories_fts, rowid, text) VALUES ('delete', old.rowid, old.text);
        INSERT INTO memories_fts(rowid, text) VALUES (new.rowid, new.text);
    END;
    CREATE TABLE summaries (
        conversation_id TEXT PRIMARY KEY,
        text TEXT NOT NULL,
        created INTEGER NOT NULL,
        updated INTEGER NOT NULL
    );
    CREATE TABLE relationship (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL,
        updated INTEGER NOT NULL
    );
    "];

const TITLE_CHARS: usize = 48;

static CONVERSATION_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^(?:[0-9a-f]{32}|[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$",
    )
    .expect("fixed pattern is valid")
});
static WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\w+").expect("fixed pattern is valid"));

/// The schema this build writes; a newer database is refused rather than downgraded.
#[must_use]
pub fn schema() -> i64 {
    i64::try_from(MIGRATIONS.len()).unwrap_or(i64::MAX)
}

#[must_use]
pub fn database_path() -> PathBuf {
    data_dir().join("assistant.sqlite")
}

/// Python's `str.casefold` for the comparisons history and memory make.
#[must_use]
pub fn casefold(text: &str) -> String {
    text.to_lowercase().replace('ß', "ss")
}

/// The `\w+` words of `text`.
pub(crate) fn words(text: &str) -> Vec<String> {
    WORD.find_iter(text)
        .map(|word| word.as_str().to_owned())
        .collect()
}

/// Collapses runs of whitespace to single spaces, as `" ".join(text.split())`.
#[must_use]
pub fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The first `count` characters of `text`.
#[must_use]
pub fn chars(text: &str, count: usize) -> String {
    text.chars().take(count).collect()
}

/// Opens (creating privately) and migrates the assistant database.
pub fn connect(path: Option<&Path>) -> Result<Connection, AssistantError> {
    let path = path.map_or_else(database_path, Path::to_path_buf);
    if let Some(parent) = path.parent() {
        private_dir(parent)?;
    }
    if !path.exists() {
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&path)?;
    }
    let mut db = Connection::open(&path)?;
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    db.pragma_update(None, "journal_mode", "WAL")?;
    db.pragma_update(None, "foreign_keys", "ON")?;
    db.pragma_update(None, "busy_timeout", 5000)?;
    migrate(&mut db, &path)?;
    // Additive optional data: schema-1 readers use explicit INSERT columns and ignore unknown
    // SELECT columns, so they can still open this database.
    let transaction = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let columns: Vec<String> = transaction
        .prepare("PRAGMA table_info(turns)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<_, _>>()?;
    if !columns.iter().any(|column| column == "delivery") {
        transaction.execute(
            "ALTER TABLE turns ADD COLUMN delivery TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }
    transaction.commit()?;
    for suffix in ["", "-wal", "-shm"] {
        let mut name = path.clone().into_os_string();
        name.push(suffix);
        let _ = std::fs::set_permissions(&name, std::fs::Permissions::from_mode(0o600));
    }
    Ok(db)
}

fn migrate(db: &mut Connection, path: &Path) -> Result<(), AssistantError> {
    let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > schema() {
        return Err(AssistantError::Exit(format!(
            "tohru-assistant: {} is from a newer Tohru (schema {version})",
            path.display()
        )));
    }
    for number in version..schema() {
        let transaction = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let index = usize::try_from(number).unwrap_or_default();
        transaction.execute_batch(MIGRATIONS[index])?;
        transaction.pragma_update(None, "user_version", number + 1)?;
        transaction.commit()?;
    }
    Ok(())
}

pub fn check_id(cid: &str) -> Result<String, AssistantError> {
    if CONVERSATION_ID.is_match(cid) {
        Ok(cid.to_lowercase())
    } else {
        Err(AssistantError::Exit(
            "tohru-assistant: invalid conversation id".to_owned(),
        ))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Turn {
    pub id: String,
    pub role: String,
    pub text: String,
    pub time: i64,
    pub provider: String,
    pub voice: bool,
    pub stopped: bool,
    pub error: String,
    pub delivery: String,
}

impl Turn {
    #[must_use]
    pub fn new(role: &str, text: &str, time: i64) -> Self {
        Self {
            role: role.to_owned(),
            text: text.to_owned(),
            time,
            ..Self::default()
        }
    }

    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            role: row.get("role")?,
            text: row.get("text")?,
            time: row.get("time")?,
            provider: row.get("provider")?,
            voice: row.get::<_, i64>("voice")? != 0,
            stopped: row.get::<_, i64>("stopped")? != 0,
            error: row.get("error")?,
            delivery: row.get("delivery")?,
        })
    }

    /// The turn as history shows it: optional fields only when set.
    #[must_use]
    pub fn json(&self) -> Value {
        let mut turn = Map::new();
        turn.insert("id".into(), json!(self.id));
        turn.insert("role".into(), json!(self.role));
        turn.insert("text".into(), json!(self.text));
        turn.insert("time".into(), json!(self.time));
        if !self.provider.is_empty() {
            turn.insert("provider".into(), json!(self.provider));
        }
        if self.voice {
            turn.insert("voice".into(), json!(true));
        }
        if self.stopped {
            turn.insert("stopped".into(), json!(true));
        }
        if !self.error.is_empty() {
            turn.insert("error".into(), json!(self.error));
        }
        if !self.delivery.is_empty() {
            turn.insert("delivery".into(), json!(self.delivery));
        }
        Value::Object(turn)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub provider: String,
    pub created: i64,
    pub updated: i64,
    pub voice: bool,
    /// Provider id → that CLI's session id.
    pub sessions: Map<String, Value>,
    pub forgotten: bool,
    pub remembered: i64,
    pub turns: Vec<Turn>,
}

impl Conversation {
    #[must_use]
    pub fn json(&self) -> Value {
        json!({"id": self.id, "title": self.title, "provider": self.provider, "created": self.created,
               "updated": self.updated, "voice": self.voice, "sessions": self.sessions,
               "forgotten": self.forgotten, "remembered": self.remembered,
               "turns": self.turns.iter().map(Turn::json).collect::<Vec<_>>()})
    }

    #[must_use]
    pub fn session(&self, provider: &str) -> Option<String> {
        self.sessions
            .get(provider)
            .and_then(Value::as_str)
            .map(str::to_owned)
    }
}

pub fn load(db: &Connection, cid: &str) -> Result<Option<Conversation>, AssistantError> {
    let cid = check_id(cid)?;
    let found = db
        .query_row("SELECT * FROM conversations WHERE id = ?", [&cid], |row| {
            Ok(Conversation {
                id: row.get("id")?,
                title: row.get("title")?,
                provider: row.get("provider")?,
                created: row.get("created")?,
                updated: row.get("updated")?,
                voice: row.get::<_, i64>("voice")? != 0,
                sessions: match serde_json::from_str::<Value>(&row.get::<_, String>("sessions")?) {
                    Ok(Value::Object(sessions)) => sessions,
                    _ => Map::new(),
                },
                forgotten: row.get::<_, i64>("forgotten")? != 0,
                remembered: row.get("remembered")?,
                turns: Vec::new(),
            })
        })
        .optional()?;
    let Some(mut conversation) = found else {
        return Ok(None);
    };
    conversation.turns = db
        .prepare("SELECT * FROM turns WHERE conversation_id = ? ORDER BY seq")?
        .query_map([&cid], Turn::from_row)?
        .collect::<Result<_, _>>()?;
    Ok(Some(conversation))
}

pub fn save_conversation(
    db: &Connection,
    conversation: &Conversation,
) -> Result<(), AssistantError> {
    let sessions = pyjson::render(
        &Value::Object(conversation.sessions.clone()),
        pyjson::Style {
            ascii: true,
            sort_keys: true,
            compact: false,
        },
    );
    db.execute(
        "INSERT INTO conversations (id, title, provider, created, updated, voice, sessions, forgotten, remembered)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET title = excluded.title, provider = excluded.provider,
           updated = excluded.updated, voice = excluded.voice, sessions = excluded.sessions,
           forgotten = excluded.forgotten",
        params![
            conversation.id,
            conversation.title,
            conversation.provider,
            conversation.created,
            conversation.updated,
            i64::from(conversation.voice),
            sessions,
            i64::from(conversation.forgotten),
            conversation.remembered
        ],
    )?;
    Ok(())
}

/// Appends `turn`, giving it a fresh id when it has none.
pub fn add_turn(db: &Connection, cid: &str, turn: &mut Turn) -> Result<(), AssistantError> {
    let seq: i64 = db.query_row(
        "SELECT COALESCE(MAX(seq), -1) + 1 FROM turns WHERE conversation_id = ?",
        [cid],
        |row| row.get(0),
    )?;
    if turn.id.is_empty() {
        turn.id = uuid::Uuid::new_v4().simple().to_string();
    }
    db.execute(
        "INSERT INTO turns (id, conversation_id, seq, role, text, time, provider, voice, stopped, error, delivery)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            turn.id,
            cid,
            seq,
            turn.role,
            turn.text,
            turn.time,
            turn.provider,
            i64::from(turn.voice),
            i64::from(turn.stopped),
            turn.error,
            turn.delivery
        ],
    )?;
    Ok(())
}

/// Records playback evidence on the conversation's latest assistant turn.
pub fn mark_interrupted(db: &Connection, cid: &str, evidence: &str) -> Result<(), AssistantError> {
    if !evidence.is_empty() {
        let note = super::chat::interrupted_note(evidence);
        db.execute(
            "UPDATE turns SET delivery = ? WHERE id = (SELECT id FROM turns WHERE conversation_id = ? AND role = 'assistant' ORDER BY seq DESC LIMIT 1)",
            params![note, cid],
        )?;
    }
    Ok(())
}

#[must_use]
pub fn title_of(text: &str) -> String {
    let line = squash(text);
    if line.chars().count() <= TITLE_CHARS {
        return line;
    }
    let head = chars(&line, TITLE_CHARS);
    let cut = match head.rsplit_once(' ') {
        Some((before, _)) if !before.is_empty() => before.to_owned(),
        _ => head,
    };
    format!("{cut}…")
}

fn local(stamp: i64) -> jiff::ZonedDateTime {
    let timestamp = jiff::Timestamp::from_second(stamp).unwrap_or(jiff::Timestamp::UNIX_EPOCH);
    timestamp.to_zoned_datetime(jiff::tz::TimeZone::system())
}

/// `time.strftime(format, time.localtime(stamp))`.
#[must_use]
pub fn strftime(format: &str, stamp: i64) -> String {
    local(stamp).strftime(format).to_string()
}

/// The history list's day heading and time label.
#[must_use]
pub fn day_group(stamp: i64, now: i64) -> (&'static str, String) {
    let (then, today) = (local(stamp), local(now));
    let days = then
        .date()
        .until(today.date())
        .map_or(0, |span| span.get_days());
    let clock = || then.strftime("%-I:%M %p").to_string();
    if days <= 0 {
        ("Today", clock())
    } else if days == 1 {
        ("Yesterday", clock())
    } else if days < 7 {
        ("This week", then.strftime("%a").to_string())
    } else {
        ("Earlier", then.strftime("%-d %b").to_string())
    }
}

/// A history row: the conversation columns plus its turn count.
#[derive(Clone, Debug)]
pub struct Listed {
    pub id: String,
    pub title: String,
    pub provider: String,
    pub created: i64,
    pub updated: i64,
    pub voice: bool,
    pub turns: i64,
}

#[must_use]
pub fn summary(row: &Listed, matched: &str, now: i64) -> Value {
    let (group, when) = day_group(row.updated, now);
    json!({"id": row.id, "title": row.title, "provider": row.provider,
           "slot": super::models::slot(&row.provider), "created": row.created, "updated": row.updated,
           "voice": row.voice, "turns": row.turns, "match": matched, "group": group, "when": when})
}

fn snippet(text: &[char], at: usize, length: usize) -> String {
    let start = at.saturating_sub(24);
    let end = text.len().min(at + length + 40);
    let piece = squash(&text[start..end].iter().collect::<String>());
    format!(
        "{}{piece}{}",
        if start > 0 { "…" } else { "" },
        if end < text.len() { "…" } else { "" }
    )
}

/// The character index of `needle` in the casefolded `text`, as Python's `find` reports it.
fn find(text: &str, needle: &str) -> Option<(Vec<char>, usize)> {
    let folded = casefold(text);
    let byte = folded.find(needle)?;
    let at = folded[..byte].chars().count();
    Some((text.chars().collect(), at))
}

/// A snippet around the first casefolded match of `needle` in `text`.
#[must_use]
pub fn find_snippet(text: &str, needle: &str) -> Option<String> {
    find(text, needle).map(|(chars, at)| snippet(&chars, at, needle.chars().count()))
}

/// An FTS5 query matching any of the words, each quoted so user text is never query syntax.
#[must_use]
pub fn fts_query(text: &str) -> String {
    words(&casefold(text))
        .into_iter()
        .filter(|word| word.chars().count() >= 2)
        .take(24)
        .map(|word| format!("\"{}\"", word.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// History rows, newest first, optionally filtered by text, provider and update time.
pub fn search(
    db: &Connection,
    query: &str,
    limit: i64,
    provider: Option<&str>,
    since: Option<i64>,
) -> Result<Vec<Value>, AssistantError> {
    let needle = casefold(query).trim().to_owned();
    let mut clauses = Vec::new();
    let mut parameters: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(provider) = provider.filter(|provider| !provider.is_empty()) {
        clauses.push("c.provider = ?".to_owned());
        parameters.push(provider.to_owned().into());
    }
    if let Some(since) = since {
        clauses.push("c.updated >= ?".to_owned());
        parameters.push(since.into());
    }
    let terms = words(&needle);
    if !needle.is_empty() {
        let phrase = terms
            .iter()
            .map(|word| format!("\"{word}\""))
            .collect::<Vec<_>>()
            .join(" ");
        let like = format!(
            "%{}%",
            needle
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        clauses.push(
            "(c.title LIKE ? ESCAPE '\\' OR c.id IN (
            SELECT t.conversation_id FROM turns t WHERE t.text LIKE ? ESCAPE '\\'
            UNION SELECT t.conversation_id FROM turns_fts f JOIN turns t ON t.rowid = f.rowid WHERE turns_fts MATCH ?))"
                .to_owned(),
        );
        parameters.push(like.clone().into());
        parameters.push(like.into());
        parameters.push(
            if phrase.is_empty() {
                "\"\"".to_owned()
            } else {
                phrase
            }
            .into(),
        );
    }
    let filter = if clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", clauses.join(" AND "))
    };
    parameters.push(limit.into());
    let sql = format!(
        "SELECT c.*, (SELECT COUNT(*) FROM turns t WHERE t.conversation_id = c.id) AS turns
         FROM conversations c {filter} ORDER BY c.updated DESC, c.id LIMIT ?"
    );
    let rows: Vec<Listed> = db
        .prepare(&sql)?
        .query_map(rusqlite::params_from_iter(parameters), |row| {
            Ok(Listed {
                id: row.get("id")?,
                title: row.get("title")?,
                provider: row.get("provider")?,
                created: row.get("created")?,
                updated: row.get("updated")?,
                voice: row.get::<_, i64>("voice")? != 0,
                turns: row.get("turns")?,
            })
        })?
        .collect::<Result<_, _>>()?;
    let now = super::now();
    let mut results = Vec::new();
    for row in &rows {
        let mut matched = String::new();
        if !needle.is_empty() {
            if casefold(&row.title).contains(&needle) {
                matched.clone_from(&row.title);
            } else {
                let texts: Vec<String> = db
                    .prepare("SELECT text FROM turns WHERE conversation_id = ? ORDER BY seq")?
                    .query_map([&row.id], |row| row.get(0))?
                    .collect::<Result<_, _>>()?;
                let first = terms.first().cloned().unwrap_or_else(|| needle.clone());
                let found = texts
                    .iter()
                    .find_map(|text| {
                        find(text, &needle)
                            .map(|(chars, at)| snippet(&chars, at, needle.chars().count()))
                    })
                    .or_else(|| {
                        texts.iter().find_map(|text| {
                            find(text, &first)
                                .map(|(chars, at)| snippet(&chars, at, first.chars().count()))
                        })
                    });
                matched = found.unwrap_or_default();
            }
        }
        results.push(summary(row, &matched, now));
    }
    Ok(results)
}
