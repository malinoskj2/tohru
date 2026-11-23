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
