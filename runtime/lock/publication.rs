//! The lock-surface configure ACK/state update and native commit use one order.
//! This gate never covers socket reads, GPU fence waits or target destruction.
use std::sync::Mutex;

static REQUEST_ORDER: Mutex<()> = Mutex::new(());

pub(crate) fn ordered<T>(operation: impl FnOnce() -> T) -> Option<T> {
    let _guard = REQUEST_ORDER.lock().ok()?;
    Some(operation())
}

#[cfg(test)]
#[path = "../../tests/session/lock_publication.rs"]
mod tests;
