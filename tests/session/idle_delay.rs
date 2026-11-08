use super::Delay;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
struct Fd(Arc<AtomicUsize>);
impl Drop for Fd {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn sleep_request_and_repeated_requests_keep_fd_until_native_ack() {
    let closed = Arc::new(AtomicUsize::new(0));
    let mut delay = Delay::new(Fd(closed.clone()));
    delay.acknowledge(); // Held before a sleep request is harmless.
    assert!(delay.protected());
    delay.prepare();
    delay.prepare();
    assert_eq!(closed.load(Ordering::SeqCst), 0);
    delay.acknowledge();
    delay.acknowledge();
    assert_eq!(closed.load(Ordering::SeqCst), 1);
    assert!(!delay.protected());
}

#[test]
fn resume_replaces_guard_and_stale_ack_cannot_release_new_delay() {
    let closed = Arc::new(AtomicUsize::new(0));
    let mut delay = Delay::new(Fd(closed.clone()));
    delay.prepare();
    delay.resumed(Fd(closed.clone()));
    assert_eq!(closed.load(Ordering::SeqCst), 1);
    delay.acknowledge(); // A queued old-cycle callback after resume.
    assert!(delay.protected());
    assert_eq!(closed.load(Ordering::SeqCst), 1);
    delay.prepare();
    delay.acknowledge();
    assert_eq!(closed.load(Ordering::SeqCst), 2);
}

#[test]
fn failed_reacquire_cancels_old_sleep_without_inventing_a_descriptor() {
    let closed = Arc::new(AtomicUsize::new(0));
    let mut delay = Delay::new(Fd(closed.clone()));
    delay.prepare();
    delay.acknowledge();
    delay.resume_failed();
    delay.acknowledge();
    assert!(!delay.protected());
    assert_eq!(closed.load(Ordering::SeqCst), 1);
}

#[test]
fn owner_close_releases_its_descriptor_once_without_waiting_for_ack() {
    let closed = Arc::new(AtomicUsize::new(0));
    let mut delay = Delay::new(Fd(closed.clone()));
    delay.prepare();
    delay.close();
    delay.close();
    delay.acknowledge();
    assert_eq!(closed.load(Ordering::SeqCst), 1);
    assert!(!delay.protected());
}
