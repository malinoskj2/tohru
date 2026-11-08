use super::{Stage, dispatch_allowed, request_stop, run, take_owned};
use crate::idle::input::Cancellation;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

#[test]
fn cleanup_attempts_every_owned_stage_after_close_errors_and_blocks_actions_first() {
    let stop = AtomicBool::new(false);
    let cancellation = Cancellation::default();
    let queued = cancellation.stamp();
    let mut trace = Vec::new();
    let mut authority_calls = 0;
    let mut wakes = 0;
    let failures = run(|stage| -> Result<(), &str> {
        trace.push(stage);
        match stage {
            Stage::CancelRequests => request_stop(&stop, &cancellation, || wakes += 1),
            Stage::Logind => return Err("bus close failed"),
            Stage::ScreenSaver => return Err("reader join failed"),
            Stage::Observer | Stage::Guard => {}
            Stage::Actions => {
                // The production dispatcher uses this exact predicate. Neither
                // stale/current action nor an exempt state query survives stop.
                for (stamp, query) in [
                    (queued, false),
                    (cancellation.stamp(), false),
                    (queued, true),
                ] {
                    if dispatch_allowed(&stop, &cancellation, stamp, query) {
                        authority_calls += 1;
                    }
                }
            }
        }
        Ok(())
    });
    assert_eq!(
        trace,
        [
            Stage::CancelRequests,
            Stage::Logind,
            Stage::ScreenSaver,
            Stage::Observer,
            Stage::Actions,
            Stage::Guard
        ]
    );
    assert!(stop.load(Ordering::Acquire));
    assert_eq!(wakes, 1);
    assert_eq!(authority_calls, 0);
    assert_eq!(
        failures
            .iter()
            .map(|failure| failure.stage)
            .collect::<Vec<_>>(),
        [Stage::Logind, Stage::ScreenSaver]
    );
}

#[test]
fn taking_process_slots_is_idempotent_and_does_not_take_another_owner() {
    let own = Mutex::new(Some(7));
    let guard = Mutex::new(Some(8));
    let foreign = Mutex::new(Some(9));
    let mut failures = Vec::new();
    assert_eq!(
        take_owned(&own, Stage::CancelRequests, &mut failures),
        Some(7)
    );
    assert_eq!(take_owned(&guard, Stage::Guard, &mut failures), Some(8));
    assert_eq!(take_owned(&own, Stage::CancelRequests, &mut failures), None);
    assert_eq!(take_owned(&guard, Stage::Guard, &mut failures), None);
    assert_eq!(*foreign.lock().unwrap(), Some(9));
    assert!(failures.is_empty());
}

#[test]
fn poisoned_owner_is_recovered_and_does_not_skip_other_owner_cleanup() {
    let own = Mutex::new(Some(7));
    let _ = std::panic::catch_unwind(|| {
        let _guard = own.lock().unwrap();
        panic!("fixture poison");
    });
    let guard = Mutex::new(Some(8));
    let mut failures = Vec::new();
    assert_eq!(
        take_owned(&own, Stage::CancelRequests, &mut failures),
        Some(7)
    );
    assert_eq!(take_owned(&guard, Stage::Guard, &mut failures), Some(8));
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].stage, Stage::CancelRequests);
}

#[test]
fn actual_cpu_handles_all_join_and_guard_fd_closes_despite_an_earlier_error() {
    use std::io::Read;
    use std::os::unix::net::UnixStream;
    use std::sync::{Arc, mpsc};
    let cancellation = Cancellation::default();
    let stop = Arc::new(AtomicBool::new(false));
    let executed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut workers = Vec::new();
    for stage in [
        Stage::Logind,
        Stage::ScreenSaver,
        Stage::Observer,
        Stage::Actions,
    ] {
        let (send, receive) = mpsc::channel::<()>();
        let (stopped, generation, calls) = (stop.clone(), cancellation.clone(), executed.clone());
        let stamp = generation.stamp();
        let thread = std::thread::spawn(move || {
            receive
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap();
            if stage == Stage::Actions && dispatch_allowed(&stopped, &generation, stamp, false) {
                calls.fetch_add(1, Ordering::SeqCst);
            }
            stage
        });
        workers.push((stage, Some(send), Some(thread)));
    }
    let (guard, mut peer) = UnixStream::pair().unwrap();
    peer.set_read_timeout(Some(std::time::Duration::from_secs(1)))
        .unwrap();
    let mut guard = Some(guard);
    let mut joined = Vec::new();
    let errors = run(|stage| -> Result<(), &str> {
        if stage == Stage::CancelRequests {
            request_stop(&stop, &cancellation, || {});
        }
        if let Some((_, send, thread)) = workers.iter_mut().find(|worker| worker.0 == stage) {
            send.take().unwrap().send(()).unwrap();
            joined.push(thread.take().unwrap().join().unwrap());
        }
        if stage == Stage::Guard {
            guard.take();
        }
        if stage == Stage::Logind {
            return Err("injected close diagnostic after real join");
        }
        Ok(())
    });
    assert_eq!(
        joined,
        [
            Stage::Logind,
            Stage::ScreenSaver,
            Stage::Observer,
            Stage::Actions
        ]
    );
    assert!(
        workers
            .iter()
            .all(|(_, send, thread)| send.is_none() && thread.is_none())
    );
    assert_eq!(peer.read(&mut [0; 1]).unwrap(), 0);
    assert_eq!(executed.load(Ordering::SeqCst), 0);
    assert_eq!(errors.len(), 1);
}
