//! Tohru's lock authority. A `kind: lock` surface is released only after PAM
//! accepted a password for the lock cycle the compositor confirmed, or after the
//! compositor itself ended the lock. A scene's `open:` going false, an external
//! `fact` order, a reload or a dying logic cannot release it.
//!
//! The password never reaches the logic: a submitted secret field is taken out
//! of the render's buffer, checked on a thread of its own and zeroed. The logic
//! hears only its length and the outcome, through the `lock_auth` fact.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::SeqCst};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use crate::scene::ToRender;

/// `lock_auth`, as the scene reads it.
pub const IDLE: u8 = 0;
pub const CHECKING: u8 = 1;
pub const ACCEPTED: u8 = 2;
pub const REJECTED: u8 = 3;
pub const UNAVAILABLE: u8 = 4;

/// Exit status of a locker that ends while its lock is still engaged: whoever
/// supervises it must start another, which the compositor lets take over.
pub const EXIT_STILL_LOCKED: i32 = 75;
/// Exit status when the compositor refused the lock (another locker holds it).
pub const EXIT_REFUSED: i32 = 3;

/// How long a granted unlock waits for the scene's own release (its unlock
/// motion) before the lock is released anyway.
pub const RELEASE_GRACE: Duration = Duration::from_millis(2500);

static NEXT_CYCLE: AtomicU64 = AtomicU64::new(1);
static CYCLE: AtomicU64 = AtomicU64::new(0);
static HELD: AtomicU64 = AtomicU64::new(0);
static PERMIT: AtomicU64 = AtomicU64::new(0);
static PERMIT_AT: Mutex<Option<Instant>> = Mutex::new(None);
static CHECKING_NOW: AtomicBool = AtomicBool::new(false);
static REFUSED: AtomicBool = AtomicBool::new(false);
static RELEASED: AtomicBool = AtomicBool::new(false);

/// The dedicated locker process (`TOHRU_LOCKER=1`): takes no orders from
/// outside, does not reload its scene, and leaves once it has unlocked.
pub fn locker() -> bool {
    static LOCKER: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *LOCKER.get_or_init(|| std::env::var("TOHRU_LOCKER").as_deref() == Ok("1"))
}

/// A lock request went to the compositor: a new cycle, with no permit.
pub fn engaged() -> u64 {
    let cycle = NEXT_CYCLE.fetch_add(1, SeqCst);
    PERMIT.store(0, SeqCst);
    HELD.store(0, SeqCst);
    CYCLE.store(cycle, SeqCst);
    *PERMIT_AT.lock().unwrap() = None;
    start_status(cycle);
    cycle
}

/// The compositor confirmed the session is locked.
pub fn held() {
    HELD.store(CYCLE.load(SeqCst), SeqCst);
}

/// The compositor ended the cycle itself (`finished`). Before `locked` it is a
/// refusal: another locker holds the session.
pub fn finished() {
    let cycle = CYCLE.load(SeqCst);
    if cycle != 0 && HELD.load(SeqCst) != cycle {
        REFUSED.store(true, SeqCst);
    }
    clear();
}

/// The lock was released through a permit.
pub fn released() {
    RELEASED.store(true, SeqCst);
    clear();
}

fn clear() {
    CYCLE.store(0, SeqCst);
    HELD.store(0, SeqCst);
    PERMIT.store(0, SeqCst);
    *PERMIT_AT.lock().unwrap() = None;
    stop_status();
}

/// A fresh observation of the compositor-confirmed current cycle. A previous
/// systemd READY notification is not evidence for a later/recovering cycle.
fn confirmed_cycle() -> u64 {
    let cycle = CYCLE.load(SeqCst);
    if cycle != 0 && HELD.load(SeqCst) == cycle {
        cycle
    } else {
        0
    }
}

#[cfg(target_os = "linux")]
static STATUS_WORKER: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);

#[cfg(target_os = "linux")]
fn stop_status() {
    if let Ok(mut worker) = STATUS_WORKER.lock() {
        if let Some(worker) = worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn stop_status() {}

#[cfg(target_os = "linux")]
fn start_status(cycle: u64) {
    use std::os::unix::net::UnixDatagram;
    if !locker() {
        return;
    }
    stop_status();
    let Some(path) = std::env::var_os("TOHRU_LOCK_STATUS_SOCKET").map(std::path::PathBuf::from)
    else {
        return;
    };
    let Ok(socket) = UnixDatagram::bind(&path) else {
        return;
    };
    if socket
        .set_read_timeout(Some(Duration::from_millis(100)))
        .is_err()
        || socket
            .set_write_timeout(Some(Duration::from_millis(100)))
            .is_err()
    {
        let _ = std::fs::remove_file(&path);
        return;
    }
    let Ok(mut owner) = STATUS_WORKER.lock() else {
        return;
    };
    *owner = std::thread::Builder::new().name("tohru-lock-status".into()).spawn(move || {
        // The cycle owns this read-only listener. clear/re-engage ends it, and
        // joins the bounded receive loop. It accepts no control/authentication.
        let mut bytes = [0_u8; 64];
        while CYCLE.load(SeqCst) == cycle {
            let Ok((size, peer)) = socket.recv_from(&mut bytes) else { continue; };
            let Some(reply) = peer.as_pathname() else { continue; };
            let Ok(nonce) = std::str::from_utf8(&bytes[..size]) else { continue; };
            if nonce.len() != 32 || !nonce.bytes().all(|b| b.is_ascii_hexdigit()) { continue; }
            let confirmed = confirmed_cycle();
            let response = serde_json::json!({"schema": 1, "nonce": nonce, "cycle": confirmed, "held": confirmed != 0});
            let _ = socket.send_to(response.to_string().as_bytes(), reply);
        }
        drop(socket);
        let _ = std::fs::remove_file(path);
    }).ok();
}

#[cfg(not(target_os = "linux"))]
fn start_status(_: u64) {}

/// Whether a lock is engaged in this process.
pub fn active() -> bool {
    CYCLE.load(SeqCst) != 0
}

/// Whether the current cycle may be released: PAM accepted a password for it.
pub fn may_release() -> bool {
    let cycle = CYCLE.load(SeqCst);
    cycle != 0 && PERMIT.load(SeqCst) == cycle
}

/// A permit has waited past `RELEASE_GRACE` for the scene to release the lock.
pub fn release_overdue() -> bool {
    may_release()
        && PERMIT_AT
            .lock()
            .unwrap()
            .is_some_and(|at| at.elapsed() >= RELEASE_GRACE)
}

/// Whether the last lock ended through a permit, so the locker may leave.
pub fn unlocked() -> bool {
    RELEASED.load(SeqCst)
}

/// The status the process ends with.
pub fn exit_code(normal: i32) -> i32 {
    if active() {
        EXIT_STILL_LOCKED
    } else if REFUSED.load(SeqCst) && !RELEASED.load(SeqCst) {
        EXIT_REFUSED
    } else {
        normal
    }
}

/// Overwrites a buffer that held a password before it is freed.
pub fn zero(text: &mut String) {
    // SAFETY: zero bytes are valid UTF-8.
    unsafe { text.as_bytes_mut() }
        .iter_mut()
        .for_each(|b| unsafe { std::ptr::write_volatile(b, 0) });
    text.clear();
}

/// What the logic is told while a lock is engaged instead of a secret field's
/// text: as many bullets as it has characters.
pub fn masked(text: &str) -> String {
    "•".repeat(text.chars().count())
}

/// A secret field submitted while a lock is engaged. The password is checked on
/// a thread of its own, against the cycle the compositor confirmed when it was
/// submitted, and zeroed afterwards whatever the outcome.
pub fn submit(mut password: String, to_render: Sender<ToRender>) {
    let report = |tx: &Sender<ToRender>, state: u8| {
        let _ = tx.send(ToRender::ExternalFact(
            crate::scene::intern("lock_auth"),
            state.to_string(),
        ));
    };
    let cycle = HELD.load(SeqCst);
    if cycle == 0
        || cycle != CYCLE.load(SeqCst)
        || password.is_empty()
        || CHECKING_NOW.swap(true, SeqCst)
    {
        zero(&mut password);
        report(&to_render, IDLE);
        return;
    }
    report(&to_render, CHECKING);
    std::thread::Builder::new()
        .name("lock-auth".into())
        .spawn(move || {
            let outcome = check(&password, cycle, |p| crate::platform::auth::verify(p));
            zero(&mut password);
            CHECKING_NOW.store(false, SeqCst);
            report(&to_render, outcome);
            if outcome == ACCEPTED {
                std::thread::sleep(RELEASE_GRACE);
                let _ = to_render.send(ToRender::ExternalFact(
                    crate::scene::intern("lock_overdue"),
                    "1".into(),
                ));
            }
        })
        .expect("start the lock authentication thread");
}

/// PAM, then the permit, only if the cycle it was asked for is still the one held.
fn check(password: &str, cycle: u64, verify: impl FnOnce(&str) -> Result<bool, String>) -> u8 {
    match verify(password) {
        Ok(true) if HELD.load(SeqCst) == cycle && CYCLE.load(SeqCst) == cycle => {
            *PERMIT_AT.lock().unwrap() = Some(Instant::now());
            PERMIT.store(cycle, SeqCst);
            ACCEPTED
        }
        Ok(true) => IDLE,
        Ok(false) => REJECTED,
        Err(e) => {
            eprintln!("lock   · authentication unavailable: {e}");
            UNAVAILABLE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static SERIAL: Mutex<()> = Mutex::new(());

    fn fresh() -> std::sync::MutexGuard<'static, ()> {
        let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        REFUSED.store(false, SeqCst);
        RELEASED.store(false, SeqCst);
        CHECKING_NOW.store(false, SeqCst);
        guard
    }

    #[test]
    fn tohru_lock_readiness_cannot_survive_release_or_reengage() {
        let _guard = fresh();
        assert_eq!(confirmed_cycle(), 0);
        let first = engaged();
        assert_eq!(confirmed_cycle(), 0);
        held();
        assert_eq!(confirmed_cycle(), first);
        released();
        assert_eq!(confirmed_cycle(), 0);
        let second = engaged();
        assert_ne!(first, second);
        assert_eq!(confirmed_cycle(), 0);
        held();
        assert_eq!(confirmed_cycle(), second);
        finished();
        assert_eq!(confirmed_cycle(), 0);
    }

    #[test]
    fn tohru_lock_needs_a_permit_for_the_held_cycle() {
        let _g = fresh();
        assert!(!may_release());
        let cycle = engaged();
        assert!(
            !may_release(),
            "an engaged lock is not releasable without PAM"
        );
        assert_eq!(
            check("x", cycle, |_| Ok(true)),
            IDLE,
            "a password before `locked` grants nothing"
        );
        assert!(!may_release());
        held();
        assert_eq!(check("wrong", cycle, |_| Ok(false)), REJECTED);
        assert!(!may_release());
        assert_eq!(check("x", cycle, |_| Err("no pam".into())), UNAVAILABLE);
        assert!(!may_release());
        assert_eq!(check("right", cycle, |_| Ok(true)), ACCEPTED);
        assert!(may_release());
        assert!(!release_overdue());
        assert_eq!(exit_code(0), EXIT_STILL_LOCKED);
        released();
        assert!(!active() && unlocked());
        assert_eq!(exit_code(0), 0);
    }

    #[test]
    fn tohru_lock_permit_does_not_survive_a_new_cycle() {
        let _g = fresh();
        let first = engaged();
        held();
        let second_started_during_pam = |_: &str| {
            engaged();
            held();
            Ok(true)
        };
        assert_eq!(check("right", first, second_started_during_pam), IDLE);
        assert!(!may_release());
        let cycle = CYCLE.load(SeqCst);
        assert_eq!(check("right", cycle, |_| Ok(true)), ACCEPTED);
        engaged();
        assert!(!may_release(), "a permit belongs to its own cycle");
    }

    #[test]
    fn tohru_lock_exit_codes() {
        let _g = fresh();
        assert_eq!(exit_code(0), 0);
        engaged();
        assert_eq!(
            exit_code(0),
            EXIT_STILL_LOCKED,
            "quitting while locked must be restarted"
        );
        finished();
        assert_eq!(exit_code(0), EXIT_REFUSED, "refused before `locked`");
        RELEASED.store(false, SeqCst);
        REFUSED.store(false, SeqCst);
        engaged();
        held();
        finished();
        assert_eq!(exit_code(0), 0, "the compositor ended a held lock itself");
    }

    #[test]
    fn tohru_lock_submit_outside_a_held_lock_is_dropped() {
        let _g = fresh();
        let (tx, rx) = std::sync::mpsc::channel();
        submit("secret".into(), tx.clone());
        assert!(
            matches!(rx.try_recv(), Ok(ToRender::ExternalFact(n, v)) if n == "lock_auth" && v == "0")
        );
        engaged();
        submit("secret".into(), tx);
        assert!(matches!(rx.try_recv(), Ok(ToRender::ExternalFact(_, v)) if v == "0"));
        assert!(!may_release());
    }

    #[test]
    fn tohru_lock_zero_and_mask() {
        let mut s = String::from("hunter2é");
        assert_eq!(masked(&s), "••••••••");
        zero(&mut s);
        assert!(s.is_empty());
    }

    /// The real PAM conversation against a private service: pam_exec checks the
    /// password the conversation hands it.
    #[test]
    fn tohru_lock_pam_success_and_failure() {
        let Some(modules) = crate::platform::auth::module_dir() else {
            eprintln!("skipped: libpam's module directory was not found");
            return;
        };
        let dir = std::env::temp_dir().join(format!("tohru-pam-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("check");
        std::fs::write(
            &script,
            "#!/bin/sh\nIFS= read -r pw\n[ \"$pw\" = 'correct horse' ]\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(
            dir.join("tohru-test"),
            format!(
                "auth required {m}/pam_exec.so expose_authtok quiet {s}\naccount required {m}/pam_permit.so\n",
                m = modules.display(),
                s = script.display()
            ),
        )
        .unwrap();
        let verify = |p: &str| crate::platform::auth::verify_with("tohru-test", Some(&dir), p);
        assert_eq!(verify("correct horse"), Ok(true));
        assert_eq!(verify("wrong"), Ok(false));
        assert_eq!(verify(""), Ok(false));
        let _g = fresh();
        let cycle = engaged();
        held();
        assert_eq!(check("wrong", cycle, verify), REJECTED);
        assert!(!may_release());
        assert_eq!(check("correct horse", cycle, verify), ACCEPTED);
        assert!(may_release());
        clear();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
