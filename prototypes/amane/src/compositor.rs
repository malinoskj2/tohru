use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc::Sender;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("Sway IPC I/O failed")]
    Io(#[from] std::io::Error),
    #[error("Sway IPC JSON was invalid")]
    Json(#[from] serde_json::Error),
    #[error("Sway IPC message exceeds one MiB or has invalid magic")]
    Protocol,
    #[error("SWAYSOCK is missing; use --mode replay, idle, or active for a demo")]
    MissingSocket,
}

fn connect() -> Result<UnixStream, Error> {
    let path = std::env::var_os("SWAYSOCK").ok_or(Error::MissingSocket)?;
    let socket = UnixStream::connect(path)?;
    socket.set_write_timeout(Some(Duration::from_secs(2)))?;
    Ok(socket)
}

fn send(socket: &mut UnixStream, kind: u32, body: &str) -> Result<(), Error> {
    socket.write_all(b"i3-ipc")?;
    socket.write_all(&(body.len() as u32).to_le_bytes())?;
    socket.write_all(&kind.to_le_bytes())?;
    socket.write_all(body.as_bytes())?;
    Ok(())
}

fn receive(socket: &mut UnixStream) -> Result<serde_json::Value, Error> {
    let mut header = [0; 14];
    socket.read_exact(&mut header)?;
    let len = u32::from_le_bytes([header[6], header[7], header[8], header[9]]) as usize;
    if &header[..6] != b"i3-ipc" || len > 1024 * 1024 {
        return Err(Error::Protocol);
    }
    let mut body = vec![0; len];
    socket.read_exact(&mut body)?;
    Ok(serde_json::from_slice(&body)?)
}

pub(crate) fn focus(target: usize) -> Result<(), Error> {
    let mut socket = connect()?;
    socket.set_read_timeout(Some(Duration::from_secs(2)))?;
    send(&mut socket, 0, &format!("workspace number {target}"))?;
    let _reply = receive(&mut socket)?;
    Ok(())
}

pub(crate) fn watch(tx: Sender<usize>) -> Result<(), Error> {
    let mut socket = connect()?;
    send(&mut socket, 1, "")?;
    if let Some(workspaces) = receive(&mut socket)?.as_array() {
        for workspace in workspaces {
            if workspace["focused"].as_bool() == Some(true)
                && let Some(n) = workspace["num"].as_u64()
            {
                let _ = tx.send(n as usize);
            }
        }
    }
    send(&mut socket, 2, "[\"workspace\"]")?;
    loop {
        let event = receive(&mut socket)?;
        if event["change"].as_str() == Some("focus")
            && let Some(n) = event["current"]["num"].as_u64()
            && tx.send(n as usize).is_err()
        {
            return Ok(());
        }
    }
}
