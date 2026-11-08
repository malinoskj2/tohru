// Actual Shared and query code; controller holder/renderer event types are explicit fixtures.
static SERIAL: Mutex<()> = Mutex::new(());
fn shared() -> (Arc<Shared>, std::sync::mpsc::Receiver<ToRender>) {
    let (changed, receive) = std::sync::mpsc::channel();
    let (wake, _receiver) = Wake::pair().unwrap();
    (
        Arc::new(Shared {
            snapshot: Mutex::new(Arc::new(Snapshot {
                revision: 1,
                ..Snapshot::default()
            })),
            report: Mutex::new(Report::default()),
            changed,
            notice_pending: AtomicBool::new(false),
            closing: AtomicBool::new(false),
            fatal: Arc::new(AtomicBool::new(false)),
            retained_cores: Arc::new(AtomicUsize::new(0)),
            startup: Mutex::new(None),
            wake,
        }),
        receive,
    )
}
fn field<'a>(value: &'a SysValue, name: &str) -> &'a SysValue {
    let SysValue::Map(fields) = value else {
        panic!("expected native status map")
    };
    &fields.iter().find(|(key, _)| key == name).unwrap().1
}
#[test]
fn report_updates_coalesce_one_notice_until_actual_status_query_acknowledges() {
    let _serial = SERIAL.lock().unwrap();
    let (shared, receive) = shared();
    *CONTROLLER.lock().unwrap() = OwnerState::Active(Controller {
        shared: Arc::clone(&shared),
    });
    for revision in 1..=10000 {
        shared.report(Report {
            revision,
            phase: Phase::Running,
            ..Report::default()
        });
    }
    let notice = receive.try_recv().unwrap();
    assert!(matches!(
        notice,
        ToRender::ExternalSignal("wallpaper_video_changed", None)
    ));
    assert!(receive.try_recv().is_err());
    let status = query("video.status", &[]).unwrap();
    assert!(matches!(field(&status,"revision"),SysValue::Text(value) if value=="10000"));
    shared.report(Report {
        revision: 10001,
        phase: Phase::Running,
        ..Report::default()
    });
    assert!(receive.try_recv().is_ok() && receive.try_recv().is_err());
    *CONTROLLER.lock().unwrap() = OwnerState::Idle;
}
#[test]
fn retiring_query_reports_stopping_and_retained_backend_failure_remains_visible() {
    let _serial = SERIAL.lock().unwrap();
    let (shared, receive) = shared();
    shared.report(Report {
        revision: 9,
        phase: Phase::Running,
        live_cores: 2,
        ..Report::default()
    });
    *CONTROLLER.lock().unwrap() = OwnerState::Retiring(Arc::clone(&shared));
    let status = query("video.status", &[]).unwrap();
    assert!(matches!(field(&status,"phase"),SysValue::Text(value) if value=="stopping"));
    assert!(
        matches!(field(&status,"live_cores"),SysValue::Num(value) if value.to_bits()==2.0_f64.to_bits())
    );
    shared.fatal.store(true, Ordering::Release);
    shared.retained_cores.store(2, Ordering::Release);
    shared.failed("context teardown unproven".into());
    assert!(
        receive
            .try_iter()
            .any(|event| matches!(event, ToRender::Quit))
    );
    let status = query("video.status", &[]).unwrap();
    assert!(matches!(field(&status,"phase"),SysValue::Text(value) if value=="failed"));
    assert!(matches!(
        field(&status, "native_cleanup_unproven"),
        SysValue::Bool(true)
    ));
    assert!(
        matches!(field(&status,"retained_cores"),SysValue::Num(value) if value.to_bits()==2.0_f64.to_bits())
    );
    *CONTROLLER.lock().unwrap() = OwnerState::Idle;
}
#[test]
fn failed_output_does_not_keep_advertising_playing_and_unknown_is_not_false() {
    let _serial = SERIAL.lock().unwrap();
    let (shared, _) = shared();
    shared.report(Report {
        revision: 5,
        phase: Phase::Running,
        outputs: BTreeMap::from([(
            "DP-1".into(),
            OutputReport {
                state: PlayerState::Playing,
                observed_pause: Some(false),
                ..OutputReport::default()
            },
        )]),
        ..Report::default()
    });
    shared.failed("backend stopped".into());
    *CONTROLLER.lock().unwrap() = OwnerState::Active(Controller { shared });
    let status = query("video.status", &[]).unwrap();
    let output = field(field(&status, "outputs"), "DP-1");
    assert!(matches!(field(output,"state"),SysValue::Text(value) if value=="failed"));
    let SysValue::Map(fields) = output else {
        panic!("map")
    };
    assert!(!fields.iter().any(|(key, _)| key == "observed_pause"));
    assert!(query("video.publish", &[]).is_err());
    assert!(query("video.status", &[SysValue::Bool(true)]).is_err());
    *CONTROLLER.lock().unwrap() = OwnerState::Idle;
}
#[test]
fn revision_check_is_fail_closed_on_newer_snapshot_closing_or_poisoned_observation() {
    let (shared, _) = shared();
    assert!(shared.current(1));
    assert!(!shared.current(2));
    let first = shared.snapshot().unwrap();
    let second = shared.snapshot().unwrap();
    assert!(
        Arc::ptr_eq(&first, &second),
        "frame reads borrow one immutable snapshot"
    );
    *shared.snapshot.lock().unwrap() = Arc::new(Snapshot {
        revision: 2,
        ..Snapshot::default()
    });
    assert_eq!(first.revision, 1);
    assert!(!shared.current(1) && shared.current(2));
    shared.closing.store(true, Ordering::Release);
    assert!(!shared.current(2));
    shared.closing.store(false, Ordering::Release);
    let poison = Arc::clone(&shared);
    assert!(
        std::thread::spawn(move || {
            let _guard = poison.snapshot.lock().unwrap();
            panic!("intentional observation poison fixture");
        })
        .join()
        .is_err()
    );
    assert!(!shared.current(2) && shared.snapshot().is_err());
}

#[test]
fn private_video_endpoint_uses_only_explicit_named_display_without_default_or_fd_takeover() {
    assert!(display_path(None, Some("/private/runtime".into())).is_err());
    assert!(display_path(Some("".into()), Some("/private/runtime".into())).is_err());
    assert!(display_path(Some("wayland-7".into()), Some("relative".into())).is_err());
    assert_eq!(
        display_path(Some("wayland-7".into()), Some("/private/runtime".into())).unwrap(),
        PathBuf::from("/private/runtime/wayland-7")
    );
    assert_eq!(
        display_path(Some("/private/named-display".into()), None).unwrap(),
        PathBuf::from("/private/named-display")
    );
}

// Explicit backend boundary: actual supervisor runs, native protocol/GL never does.
struct RecoveryFixture {
    calls: Vec<Instant>,
    stop_after: usize,
    deactivate_after: Option<usize>,
}
static RECOVERY: Mutex<Option<RecoveryFixture>> = Mutex::new(None);
fn run(shared: &Shared, _: &Receiver) -> Result<()> {
    let mut control = RECOVERY.lock().unwrap();
    let control = control.as_mut().unwrap();
    control.calls.push(Instant::now());
    if control.deactivate_after == Some(control.calls.len()) {
        let mut snapshot = shared.snapshot.lock().unwrap();
        let mut disabled = (**snapshot).clone();
        disabled.enabled = false;
        disabled.revision += 1;
        *snapshot = Arc::new(disabled);
        Ok(())
    } else if control.calls.len() >= control.stop_after {
        shared.closing.store(true, Ordering::Release);
        Ok(())
    } else {
        bail!("injected responsive private-connection failure")
    }
}
fn recovery_enabled(shared: &Shared) {
    *shared.snapshot.lock().unwrap() = Arc::new(Snapshot {
        enabled: true,
        revision: 1,
        assignments: BTreeMap::from([("DP-1".into(), "/chosen/video.mp4".into())]),
        ..Snapshot::default()
    });
}
fn wait_for_attempt() {
    let deadline = Instant::now() + Duration::from_secs(2);
    while RECOVERY.lock().unwrap().as_ref().unwrap().calls.is_empty() {
        assert!(
            Instant::now() < deadline,
            "fake backend attempt did not start"
        );
        thread::yield_now();
    }
}
#[test]
fn responsive_owner_failure_retries_and_root_wakes_cannot_bypass_backoff() {
    let _serial = SERIAL.lock().unwrap();
    let (shared, _) = shared();
    recovery_enabled(&shared);
    *RECOVERY.lock().unwrap() = Some(RecoveryFixture {
        calls: vec![],
        stop_after: 2,
        deactivate_after: None,
    });
    let (wake, receiver) = Wake::pair().unwrap();
    let actor = Arc::clone(&shared);
    let worker = thread::spawn(move || supervise(&actor, &receiver).unwrap());
    wait_for_attempt();
    for _ in 0..1000 {
        worker.thread().unpark();
    }
    worker.join().unwrap();
    drop(wake);
    let control = RECOVERY.lock().unwrap();
    let calls = &control.as_ref().unwrap().calls;
    assert_eq!(calls.len(), 2);
    assert!(calls[1] - calls[0] >= Duration::from_millis(100));
}
#[test]
fn root_shutdown_wakes_retry_wait_and_joins_owner_without_another_backend_attempt() {
    let _serial = SERIAL.lock().unwrap();
    let (shared, _) = shared();
    recovery_enabled(&shared);
    *RECOVERY.lock().unwrap() = Some(RecoveryFixture {
        calls: vec![],
        stop_after: usize::MAX,
        deactivate_after: None,
    });
    let (wake, receiver) = Wake::pair().unwrap();
    let actor = Arc::clone(&shared);
    let (done, ended) = std::sync::mpsc::channel();
    let worker = thread::spawn(move || {
        supervise(&actor, &receiver).unwrap();
        done.send(()).unwrap();
    });
    wait_for_attempt();
    shared.closing.store(true, Ordering::Release);
    worker.thread().unpark();
    ended.recv_timeout(Duration::from_secs(2)).unwrap();
    worker.join().unwrap();
    drop(wake);
    assert_eq!(RECOVERY.lock().unwrap().as_ref().unwrap().calls.len(), 1);
}

#[test]
fn inactive_report_preserves_actual_protection_and_disables_retained_assignments() {
    let snapshot = Snapshot {
        revision: 42,
        enabled: false,
        assignments: BTreeMap::from([("DP-1".into(), "/chosen/video.mp4".into())]),
        logind: Protection::Protected,
        locker: Protection::Unknown,
        ..Snapshot::default()
    };
    let report = inactive_report(&snapshot);
    assert_eq!(report.phase, Phase::Stopped);
    assert_eq!(report.revision, 42);
    assert_eq!(report.logind, Protection::Protected);
    assert_eq!(report.locker, Protection::Unknown);
    assert_eq!(report.live_cores, 0);
    assert_eq!(report.outputs["DP-1"].state, PlayerState::Disabled);
    assert_eq!(report.outputs["DP-1"].observed_pause, None);
}
#[test]
fn inactive_backend_returns_to_park_and_can_restart_on_new_assignment_snapshot() {
    let _serial = SERIAL.lock().unwrap();
    let (shared, _) = shared();
    recovery_enabled(&shared);
    *RECOVERY.lock().unwrap() = Some(RecoveryFixture {
        calls: vec![],
        stop_after: 2,
        deactivate_after: Some(1),
    });
    let (wake, receiver) = Wake::pair().unwrap();
    let actor = Arc::clone(&shared);
    let worker = thread::spawn(move || supervise(&actor, &receiver).unwrap());
    wait_for_attempt();
    let until = Instant::now() + Duration::from_secs(2);
    while shared.report.lock().unwrap().revision != 2 {
        assert!(
            Instant::now() < until,
            "owner did not report parked inactive map"
        );
        thread::yield_now();
    }
    assert!(!worker.is_finished());
    assert_eq!(RECOVERY.lock().unwrap().as_ref().unwrap().calls.len(), 1);
    {
        let mut current = shared.snapshot.lock().unwrap();
        let mut enabled = (**current).clone();
        enabled.revision += 1;
        enabled.enabled = true;
        *current = Arc::new(enabled);
    }
    worker.thread().unpark();
    worker.join().unwrap();
    drop(wake);
    assert_eq!(RECOVERY.lock().unwrap().as_ref().unwrap().calls.len(), 2);
}
