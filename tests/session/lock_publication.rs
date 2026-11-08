use super::*;
use std::sync::{Arc, mpsc};
use std::time::Duration;

#[test]
fn commit_cannot_split_configure_ack_from_matching_viewport_update() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let viewport = Arc::new(Mutex::new((1280, 800)));
    let (acked, ack_seen) = mpsc::sync_channel(1);
    let (resume, resume_seen) = mpsc::sync_channel(1);
    let configure_events = events.clone();
    let configure_viewport = viewport.clone();
    let configure = std::thread::spawn(move || {
        ordered(|| {
            configure_events.lock().unwrap().push("ack new size");
            acked.send(()).unwrap();
            resume_seen.recv_timeout(Duration::from_secs(2)).unwrap();
            *configure_viewport.lock().unwrap() = (1920, 1080);
            configure_events.lock().unwrap().push("matching viewport");
        })
        .unwrap();
    });
    ack_seen.recv_timeout(Duration::from_secs(2)).unwrap();
    let (attempted, attempt_seen) = mpsc::sync_channel(1);
    let (committed, commit_seen) = mpsc::sync_channel(1);
    let commit_events = events.clone();
    let worker = std::thread::spawn(move || {
        attempted.send(()).unwrap();
        ordered(|| {
            assert_eq!(*viewport.lock().unwrap(), (1920, 1080));
            commit_events.lock().unwrap().push("commit new size");
            committed.send(()).unwrap();
        })
        .unwrap();
    });
    attempt_seen.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(commit_seen.try_recv().is_err());
    resume.send(()).unwrap();
    configure.join().unwrap();
    worker.join().unwrap();
    assert_eq!(
        *events.lock().unwrap(),
        ["ack new size", "matching viewport", "commit new size"]
    );
}
