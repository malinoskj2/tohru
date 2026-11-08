//! Bound/cancel SDK registry bootstrap without closing a connection after GL begins.
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Ready,
    Cancelled,
    TimedOut,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum Error {
    #[error("video startup state unavailable")]
    Unavailable,
    #[error("video startup cancelled")]
    Cancelled,
    #[error("video Wayland bootstrap exceeded its deadline")]
    Timeout,
}

#[derive(Default)]
struct State {
    socket: Option<UnixStream>,
    outcome: Option<Outcome>,
}
#[derive(Default)]
pub(super) struct Startup {
    state: Mutex<State>,
    changed: Condvar,
}

impl Startup {
    /// Socket is a duplicate of this video's sole connection, never a shared
    /// root/desktop connection. Publish before entering SDK registry roundtrip.
    pub(super) fn publish(&self, socket: UnixStream) -> Result<(), Error> {
        let mut state = self.state.lock().map_err(|_| Error::Unavailable)?;
        match state.outcome {
            None => {
                state.socket = Some(socket);
                Ok(())
            }
            Some(outcome) => {
                let _ = socket.shutdown(Shutdown::Both);
                Err(error(outcome))
            }
        }
    }

    /// Call before creating ANY EGL/decoder object. After this point root
    /// shutdown wakes the normal owner; it must never abort the Wayland socket.
    pub(super) fn finish(&self) -> Result<(), Error> {
        let mut state = self.state.lock().map_err(|_| Error::Unavailable)?;
        if let Some(outcome) = state.outcome {
            return Err(error(outcome));
        }
        state.outcome = Some(Outcome::Ready);
        state.socket = None;
        self.changed.notify_all();
        Ok(())
    }

    pub(super) fn cancel(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.outcome.is_some() {
            return;
        }
        state.outcome = Some(Outcome::Cancelled);
        if let Some(socket) = state.socket.take() {
            let _ = socket.shutdown(Shutdown::Both);
        }
        self.changed.notify_all();
    }

    pub(super) fn watch(&self, within: Duration) -> Result<Outcome, Error> {
        let until = Instant::now() + within;
        let mut state = self.state.lock().map_err(|_| Error::Unavailable)?;
        loop {
            if let Some(outcome) = state.outcome {
                return Ok(outcome);
            }
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                state.outcome = Some(Outcome::TimedOut);
                if let Some(socket) = state.socket.take() {
                    let _ = socket.shutdown(Shutdown::Both);
                }
                self.changed.notify_all();
                return Ok(Outcome::TimedOut);
            }
            let (next, _) = self
                .changed
                .wait_timeout(state, left)
                .map_err(|_| Error::Unavailable)?;
            state = next;
        }
    }
}

fn error(outcome: Outcome) -> Error {
    match outcome {
        Outcome::TimedOut => Error::Timeout,
        Outcome::Ready | Outcome::Cancelled => Error::Cancelled,
    }
}

// The duplicated socket only exists during bootstrap. Closing/cancelling this
// latch never claims an EGL/driver or a kernel-uninterruptible connect deadline.
#[cfg(test)]
#[path = "../../tests/wallpaper/video_startup.rs"]
mod tests;
