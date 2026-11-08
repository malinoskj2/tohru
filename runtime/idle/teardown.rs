//! Ordered teardown of this process's idle resources, with no action authority.
use super::input::Cancellation;
use std::fmt::Display;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stage {
    CancelRequests,
    Logind,
    ScreenSaver,
    Observer,
    Actions,
    Guard,
}

#[derive(Debug, thiserror::Error)]
#[error("{stage:?}: {detail}")]
pub struct Failure {
    pub stage: Stage,
    pub detail: String,
}

/// Stop producers/actions before closing services or joining worker threads.
pub fn request_stop(stop: &AtomicBool, cancellation: &Cancellation, wake: impl FnOnce()) {
    stop.store(true, Ordering::Release);
    cancellation.invalidate();
    wake();
}

#[must_use]
pub fn dispatch_allowed(
    stop: &AtomicBool,
    cancellation: &Cancellation,
    stamp: u64,
    query: bool,
) -> bool {
    !stop.load(Ordering::Acquire) && (query || cancellation.current(stamp))
}

/// A poisoned owner still owns resources; recover them for best-effort cleanup.
pub fn take_owned<T>(
    slot: &Mutex<Option<T>>,
    stage: Stage,
    failures: &mut Vec<Failure>,
) -> Option<T> {
    match slot.lock() {
        Ok(mut owner) => owner.take(),
        Err(poisoned) => {
            failures.push(Failure {
                stage,
                detail: "owner mutex poisoned; recovered resources".into(),
            });
            poisoned.into_inner().take()
        }
    }
}

/// Attempt every stage even when an earlier close/join reports an error.
pub fn run<E: Display>(mut stop: impl FnMut(Stage) -> Result<(), E>) -> Vec<Failure> {
    let mut failures = Vec::new();
    for stage in [
        Stage::CancelRequests,
        Stage::Logind,
        Stage::ScreenSaver,
        Stage::Observer,
        Stage::Actions,
        Stage::Guard,
    ] {
        if let Err(error) = stop(stage) {
            failures.push(Failure {
                stage,
                detail: error.to_string(),
            });
        }
    }
    failures
}

#[cfg(test)]
#[path = "../../tests/session/idle_teardown.rs"]
mod tests;
