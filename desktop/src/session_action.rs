use std::os::unix::net::UnixDatagram;
use std::time::Duration;

use serde_json::Value;

use crate::paths;
use crate::probe::probe;
use crate::shell;

#[derive(Debug, thiserror::Error)]
pub enum ActionError {
    #[error("usage: tohru-session-action lock|sleep")]
    Usage,
    #[error(
        "lock not confirmed ({0}); install generated tohru-lock.service and inspect journalctl --user -u tohru-lock; not suspending"
    )]
    LockNotConfirmed(String),
    #[error("locker is no longer active; not suspending")]
    LockerInactive,
    #[error("native locker no longer confirms the current lock cycle; not suspending")]
    CycleUnconfirmed,
    #[error("suspend unavailable ({0}); locker remains owned by systemd")]
    SuspendUnavailable(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Confirmed,
    Lock,
    Sleep,
}

impl Action {
    pub fn parse(args: &[String]) -> Result<Self, ActionError> {
        match args {
            [only] if only == "confirmed" => Ok(Self::Confirmed),
            [only] if only == "lock" => Ok(Self::Lock),
            [only] if only == "sleep" => Ok(Self::Sleep),
            _ => Err(ActionError::Usage),
        }
    }
}

const LOGIN1: &str = "org.freedesktop.login1";

/// Sets logind's `LockedHint` for this session once the compositor confirmed the lock.
pub fn confirmed() {
    let Ok(session) = std::env::var("XDG_SESSION_ID") else {
        return;
    };
    if session.is_empty() {
        return;
    }
    let reply = probe(
        "busctl",
        &[
            "--system",
            "call",
            LOGIN1,
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
            "GetSession",
            "s",
            &session,
        ],
        Duration::from_secs(1),
    );
    let Ok(reply) = reply else { return };
    let Ok(parts) = shell::split(&reply) else {
        return;
    };
    if let [kind, path] = parts.as_slice()
        && kind == "o"
        && path.starts_with("/org/freedesktop/login1/session/")
    {
        let _ = probe(
            "busctl",
            &[
                "--system",
                "call",
                LOGIN1,
                path,
                "org.freedesktop.login1.Session",
                "SetLockedHint",
                "b",
                "true",
            ],
            Duration::from_secs(1),
        );
    }
}

/// Asks the native locker, with a fresh challenge, whether it still holds the current lock cycle.
///
/// Stale sockets or files, and a systemd unit still marked active after a crash, cannot pass.
#[must_use]
pub fn lock_held() -> bool {
    let Ok(directory) = paths::session_directory() else {
        return false;
    };
    let Some(runtime) = directory.parent() else {
        return false;
    };
    let endpoint = paths::lock_status_endpoint(runtime);
    let nonce = paths::hex(&uuid::Uuid::new_v4().into_bytes());
    let Ok(scratch) = tempfile::Builder::new().prefix("tlq-").tempdir_in(runtime) else {
        return false;
    };
    let exchange = || -> std::io::Result<Vec<u8>> {
        let client = UnixDatagram::bind(scratch.path().join("r"))?;
        client.set_read_timeout(Some(Duration::from_secs(1)))?;
        client.set_write_timeout(Some(Duration::from_secs(1)))?;
        client.connect(&endpoint)?;
        client.send(nonce.as_bytes())?;
        let mut buffer = [0_u8; 1024];
        let count = client.recv(&mut buffer)?;
        Ok(buffer[..count].to_vec())
    };
    let Ok(reply) = exchange() else {
        return false;
    };
    let Ok(result) = serde_json::from_slice::<Value>(&reply) else {
        return false;
    };
    result.get("schema") == Some(&Value::from(1))
        && result.get("nonce").and_then(Value::as_str) == Some(nonce.as_str())
        && result.get("held") == Some(&Value::Bool(true))
        && result
            .get("cycle")
            .and_then(Value::as_i64)
            .is_some_and(|cycle| cycle > 0)
}

/// Locks through the generated systemd unit (whose readiness is the compositor's lock), then
/// optionally suspends once the native locker still confirms the cycle.
pub fn run(action: Action) -> Result<(), ActionError> {
    if action == Action::Confirmed {
        confirmed();
        return Ok(());
    }
    probe(
        "systemctl",
        &["--user", "start", "tohru-lock.service"],
        Duration::from_secs(35),
    )
    .map_err(|failure| ActionError::LockNotConfirmed(failure.state()))?;
    let active = probe(
        "systemctl",
        &["--user", "is-active", "tohru-lock.service"],
        Duration::from_secs(2),
    )
    .unwrap_or_default();
    if active.trim() != "active" {
        return Err(ActionError::LockerInactive);
    }
    if !lock_held() {
        return Err(ActionError::CycleUnconfirmed);
    }
    if action == Action::Sleep {
        probe("systemctl", &["suspend"], Duration::from_secs(10))
            .map_err(|failure| ActionError::SuspendUnavailable(failure.state()))?;
    }
    Ok(())
}
