#![deny(unsafe_code)]
//! Small bounded Sway IPC adapter; no shell or helper process in animation.
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("Sway IPC transport")]
    Io(#[from] std::io::Error),
    #[error("Sway IPC JSON")]
    Json(#[from] serde_json::Error),
    #[error("invalid or oversized Sway IPC response")]
    Protocol,
}

fn request(kind: u32, body: &str) -> Result<serde_json::Value, Error> {
    let path = std::env::var_os("SWAYSOCK").ok_or(Error::Protocol)?;
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(Duration::from_millis(200)))?;
    stream.set_write_timeout(Some(Duration::from_millis(200)))?;
    stream.write_all(b"i3-ipc")?;
    stream.write_all(&(body.len() as u32).to_le_bytes())?;
    stream.write_all(&kind.to_le_bytes())?;
    stream.write_all(body.as_bytes())?;
    let mut header = [0; 14];
    stream.read_exact(&mut header)?;
    let size = u32::from_le_bytes([header[6], header[7], header[8], header[9]]) as usize;
    if &header[..6] != b"i3-ipc" || size > 1_048_576 {
        return Err(Error::Protocol);
    }
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub(crate) fn active() -> Result<usize, Error> {
    let data = request(1, "")?;
    let rows = data.as_array().ok_or(Error::Protocol)?;
    let id = rows
        .iter()
        .find(|row| row["focused"].as_bool() == Some(true))
        .and_then(|row| row["num"].as_u64())
        .ok_or(Error::Protocol)?;
    Ok((id as usize).clamp(1, 5))
}

pub(crate) fn select(id: usize) -> Result<(), Error> {
    if !(1..=5).contains(&id) {
        return Err(Error::Protocol);
    }
    let data = request(0, &format!("workspace number {id}"))?;
    if data[0]["success"].as_bool() != Some(true) {
        return Err(Error::Protocol);
    }
    Ok(())
}
