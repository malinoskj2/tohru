mod common;

use std::time::{Duration, Instant};

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use tohru_desktop::probe::{ProbeFailure, cancellable, probe};

#[test]
fn probes_bound_time_output_and_report_missing_programs() {
    let bash = common::bash();
    let started = Instant::now();
    let slow = probe(&bash, &["-c", "sleep 10"], Duration::from_millis(50));
    assert_eq!(slow, Err(ProbeFailure::Timeout));
    assert!(started.elapsed() < Duration::from_secs(2));
    let flood = probe(
        &bash,
        &["-c", "head -c 300000 /dev/zero"],
        Duration::from_secs(3),
    );
    assert_eq!(flood, Err(ProbeFailure::Oversized));
    let absent = probe(
        "/nonexistent/absent",
        &[] as &[&str],
        Duration::from_secs(1),
    );
    assert_eq!(
        absent.map_err(ProbeFailure::state),
        Err("missing".to_owned())
    );
    let failed = probe(&bash, &["-c", "echo out; exit 3"], Duration::from_secs(3));
    assert_eq!(failed, Err(ProbeFailure::Unavailable));
    assert_eq!(
        probe(&bash, &["-c", "echo ok"], Duration::from_secs(3)),
        Ok("ok\n".to_owned())
    );
}

#[test]
fn a_probe_that_leaves_a_descendant_holding_stdout_still_times_out() {
    let started = Instant::now();
    let result = probe(
        common::bash(),
        &["-c", "sleep 20 & echo started"],
        Duration::from_millis(200),
    );
    assert_eq!(result, Err(ProbeFailure::Timeout));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn a_cancelled_probe_returns_at_once_even_while_a_descendant_holds_stdout() {
    let mut command = Command::new(common::bash());
    command.args(["-c", "sleep 30 & exit 0"]);
    let cancel = AtomicBool::new(false);
    let started = Instant::now();
    let result = std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(100));
            cancel.store(true, Ordering::SeqCst);
        });
        cancellable(&mut command, Duration::from_secs(10), 1024, &|| {
            cancel.load(Ordering::SeqCst)
        })
    });
    assert_eq!(result, Err(ProbeFailure::Cancelled));
    assert!(started.elapsed() < Duration::from_millis(500));
}
