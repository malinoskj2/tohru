use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::time::{SystemTime, UNIX_EPOCH};

static SESSION: OnceLock<String> = OnceLock::new();
static CLOSED: AtomicBool = AtomicBool::new(false);

pub fn elapsed_ns() -> u64 {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    (time.tv_sec as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(time.tv_nsec as u64)
}

pub fn emit(mut event: Value) {
    if CLOSED.load(Ordering::Relaxed) {
        return;
    }
    let mut stdout = io::stdout().lock();
    if !event.is_object() {
        event = json!({"event":"error","message":"invalid internal event"});
    }
    if serde_json::to_writer(&mut stdout, &event).is_err()
        || stdout.write_all(b"\n").is_err()
        || stdout.flush().is_err()
    {
        CLOSED.store(true, Ordering::Relaxed);
    }
}

pub fn closed() -> bool {
    CLOSED.load(Ordering::Relaxed)
}

/// Accept only bounded metadata from the desktop controller, never arbitrary
/// task text or nested provider payloads.
pub fn diagnostic_message(message: &Value) -> Option<(&str, Value)> {
    let stage = message["stage"].as_str()?;
    if ![
        "ui_visibility_request",
        "ui_state_observed",
        "assistant_request",
        "assistant_origin",
        "continuation_speech",
        "continuation_dropped",
        "task_update",
        "task_update_dropped",
        "waiting_config",
    ]
    .contains(&stage)
    {
        return None;
    }
    let mut fields = serde_json::Map::new();
    for key in ["visible", "ui_sequence", "flight_alpha", "generation"] {
        let value = &message[key];
        if value.is_boolean() || value.is_number() {
            fields.insert(key.into(), value.clone());
        }
    }
    for key in ["turn_id", "request", "canonical_request", "completion"] {
        let value = match &message[key] {
            Value::String(s) if !s.is_empty() => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            Value::Bool(true) => Some("True".into()),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(
                key.into(),
                value.chars().take(512).collect::<String>().into(),
            );
        }
    }
    if ["task_update", "task_update_dropped"].contains(&stage) {
        if let Some(task) = message["task"].as_str().filter(|task| {
            !task.is_empty()
                && task.len() <= 128
                && task
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_:%.-".contains(&c))
        }) {
            fields.insert("task".into(), task.into());
        }
        if let Some(sequence) = message["sequence"]
            .as_i64()
            .filter(|n| (1..=2147483646).contains(n))
        {
            fields.insert("sequence".into(), sequence.into());
        }
        if let Some(kind) = message["kind"]
            .as_str()
            .filter(|kind| ["ack", "progress", "done", "failed", "cancelled"].contains(kind))
        {
            fields.insert("kind".into(), kind.into());
        }
    }
    if stage == "waiting_config" && message["enabled"].is_boolean() {
        fields.insert("enabled".into(), message["enabled"].clone());
    }
    if let Some(reason) = message["reason"].as_str().filter(|reason| {
        [
            "duplicate",
            "origin_mismatch",
            "hidden",
            "busy",
            "matched",
            "hearing",
            "invalid",
            "unknown_or_full",
            "replay",
        ]
        .contains(reason)
    }) {
        fields.insert("reason".into(), reason.into());
    }
    Some((stage, Value::Object(fields)))
}

pub fn diagnostic(stage: &str, fields: Value) {
    if std::env::var("TOHRU_VOICE_LOG").as_deref() == Ok("off") {
        return;
    }
    let mut record = fields.as_object().cloned().unwrap_or_default();
    record.insert("stage".into(), stage.into());
    record.insert(
        "session".into(),
        SESSION
            .get_or_init(|| format!("native-{}-{}", std::process::id(), elapsed_ns()))
            .clone()
            .into(),
    );
    record.insert("mono_ns".into(), elapsed_ns().into());
    record.insert(
        "wall_ms".into(),
        json!(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ),
    );
    eprintln!("tohru-voice: {}", Value::Object(record));
}

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("voice protocol line exceeds 64 KiB")]
    TooLong,
    #[error("read voice command")]
    Io(#[from] io::Error),
    #[error("invalid voice JSON")]
    Json(#[from] serde_json::Error),
}

/// Bound input before allocating an arbitrary line; reject EOF fragments.
pub fn read_command(input: &mut impl BufRead) -> Result<Option<Value>, ReadError> {
    let mut line = Vec::new();
    let count = std::io::Read::take(&mut *input, 65538).read_until(b'\n', &mut line)?;
    if count == 0 {
        return Ok(None);
    }
    if count > 65536 {
        return Err(ReadError::TooLong);
    }
    if line.last() != Some(&b'\n') {
        return Err(ReadError::Io(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "partial voice command",
        )));
    }
    Ok(Some(serde_json::from_slice(&line)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_diagnostics_keep_correlation_and_exclude_private_payloads() {
        let message = json!({"stage":"task_update","request":"r".repeat(700),"canonical_request":"orca:queue-id","task":"task:1","sequence":7,"kind":"progress","reason":"matched","generation":9,"text":"PRIVATE","payload":{"text":"PRIVATE"}});
        let (stage, fields) = diagnostic_message(&message).unwrap();
        assert_eq!(stage, "task_update");
        assert_eq!(fields["request"].as_str().unwrap().len(), 512);
        assert_eq!(fields["canonical_request"], "orca:queue-id");
        assert_eq!(fields["sequence"], 7);
        assert!(!fields.to_string().contains("PRIVATE"));
        let (_,fields)=diagnostic_message(&json!({"stage":"task_update_dropped","task":"PRIVATE secret","sequence":2147483647_i64,"kind":"PRIVATE","reason":"PRIVATE"})).unwrap();
        assert_eq!(fields, json!({}));
        assert_eq!(
            diagnostic_message(&json!({"stage":"waiting_config","enabled":false}))
                .unwrap()
                .1,
            json!({"enabled":false})
        );
        assert!(diagnostic_message(&json!({"stage":"PRIVATE"})).is_none());
    }

    #[test]
    fn rejects_oversize_or_partial_commands_without_reading_unbounded_input() {
        let mut huge = io::Cursor::new(vec![b'x'; 1_000_000]);
        assert!(matches!(read_command(&mut huge), Err(ReadError::TooLong)));
        assert_eq!(huge.position(), 65538);
        assert!(read_command(&mut io::Cursor::new(b"{}" as &[u8])).is_err());
    }

    #[test]
    fn accepts_additive_fields_and_unicode() {
        let mut input = io::Cursor::new("{\"cmd\":\"say\",\"text\":\"こんにちは\",\"future\":1}\n");
        let value = read_command(&mut input).unwrap().unwrap();
        assert_eq!(value["cmd"], "say");
        assert!(read_command(&mut input).unwrap().is_none());
    }
}
