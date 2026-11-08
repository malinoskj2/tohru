use super::*;
use std::io::{Read, Write};
use std::sync::Arc;

#[test]
fn startup_timeout_interrupts_only_its_owned_socket_and_joins_the_waiter() {
    let startup = Arc::new(Startup::default());
    let (mut owner, mut peer) = UnixStream::pair().unwrap();
    owner
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    startup.publish(owner.try_clone().unwrap()).unwrap();
    let worker = std::thread::spawn(move || {
        let mut byte = [0];
        owner.read(&mut byte).unwrap()
    });
    assert_eq!(
        startup.watch(Duration::from_millis(10)).unwrap(),
        Outcome::TimedOut
    );
    assert_eq!(worker.join().unwrap(), 0);
    let mut byte = [0];
    assert_eq!(peer.read(&mut byte).unwrap(), 0);
    assert!(matches!(startup.finish(), Err(Error::Timeout)));
}
#[test]
fn normal_completion_retires_watchdog_and_late_cancel_cannot_break_live_gl_connection() {
    let startup = Arc::new(Startup::default());
    let (mut owner, mut peer) = UnixStream::pair().unwrap();
    owner
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    startup.publish(owner.try_clone().unwrap()).unwrap();
    let watcher = Arc::clone(&startup);
    let watcher = std::thread::spawn(move || watcher.watch(Duration::from_secs(2)).unwrap());
    startup.finish().unwrap();
    assert_eq!(watcher.join().unwrap(), Outcome::Ready);
    startup.cancel();
    peer.write_all(b"x").unwrap();
    let mut byte = [0];
    owner.read_exact(&mut byte).unwrap();
    assert_eq!(byte, [b'x']);
}
#[test]
fn cancellation_before_connection_publication_rejects_and_closes_the_late_socket() {
    let startup = Startup::default();
    startup.cancel();
    let (owner, mut peer) = UnixStream::pair().unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    assert!(matches!(startup.publish(owner), Err(Error::Cancelled)));
    assert_eq!(
        startup.watch(Duration::from_secs(1)).unwrap(),
        Outcome::Cancelled
    );
    let mut byte = [0];
    assert_eq!(peer.read(&mut byte).unwrap(), 0);
}
