//! Installed only in the existing native runtime. Root supplies accepted snapshots;
//! one in-process actor owns all video Wayland/EGL/libmpv objects and shutdown.
#![expect(
    unsafe_code,
    reason = "establish original EGL context and retained dependencies for private libmpv constructor"
)]
use std::collections::BTreeMap;
use std::ffi::CString;
use std::ffi::OsString;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc::Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use tohru_shell::video::{Action, Backoff, PauseReason, Playback, Protection, Snapshot};
use wayland_client::Connection;

use crate::platform::SysValue;
use crate::scene::ToRender;

#[path = "egl.rs"]
mod egl;
#[path = "mpv.rs"]
mod mpv;
#[path = "startup.rs"]
mod startup;
#[path = "wake.rs"]
mod wake;
#[path = "wayland.rs"]
mod wayland;
use wake::{Receiver, Wake};

enum OwnerState {
    Idle,
    Active(Controller),
    Retiring(Arc<Shared>),
}
static CONTROLLER: Mutex<OwnerState> = Mutex::new(OwnerState::Idle);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    Starting,
    Running,
    Stopping,
    #[default]
    Stopped,
    Failed,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PlayerState {
    #[default]
    Unknown,
    Disabled,
    Disconnected,
    WaitingForConfigure,
    Superseded,
    StartingPaused,
    Pausing,
    Paused,
    Playing,
    Failed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Report {
    revision: u64,
    phase: Phase,
    error: String,
    logind: Protection,
    locker: Protection,
    outputs: BTreeMap<String, OutputReport>,
    live_cores: usize,
    retained_cores: usize,
    native_cleanup_unproven: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct OutputReport {
    path: String,
    state: PlayerState,
    desired_pause: bool,
    observed_pause: Option<bool>,
    error: String,
    pause_reason: Option<PauseReason>,
}
struct Shared {
    snapshot: Mutex<Arc<Snapshot>>,
    report: Mutex<Report>,
    changed: Sender<ToRender>,
    notice_pending: AtomicBool,
    closing: AtomicBool,
    fatal: Arc<AtomicBool>,
    retained_cores: Arc<AtomicUsize>,
    startup: Mutex<Option<Arc<startup::Startup>>>,
    wake: Wake,
}
impl Shared {
    fn snapshot(&self) -> Result<Arc<Snapshot>> {
        self.snapshot
            .lock()
            .map(|snapshot| Arc::clone(&snapshot))
            .map_err(|_| anyhow::anyhow!("video snapshot unavailable"))
    }
    fn current(&self, revision: u64) -> bool {
        !self.closing.load(Ordering::Acquire)
            && self
                .snapshot
                .lock()
                .is_ok_and(|snapshot| snapshot.revision == revision)
    }
    fn report(&self, report: Report) {
        let Ok(mut previous) = self.report.lock() else {
            return;
        };
        if *previous == report {
            return;
        }
        *previous = report;
        if !self.notice_pending.swap(true, Ordering::AcqRel) {
            let _ = self
                .changed
                .send(ToRender::ExternalSignal("wallpaper_video_changed", None));
        }
    }

    fn failed(&self, error: String) {
        let mut report = self
            .report
            .lock()
            .map(|report| report.clone())
            .unwrap_or_default();
        report.phase = Phase::Failed;
        report.error = error;
        report.live_cores = 0;
        report.retained_cores = self.retained_cores.load(Ordering::Acquire);
        report.native_cleanup_unproven = self.fatal.load(Ordering::Acquire);
        for output in report.outputs.values_mut() {
            output.state = PlayerState::Failed;
            output.observed_pause = None;
            report.error.clone_into(&mut output.error);
        }
        self.report(report);
        if self.fatal.load(Ordering::Acquire) {
            // Existing native quit terminates only this owning role/process;
            // OS teardown reclaims dependencies retained to avoid unsafe frees.
            let _ = self.changed.send(ToRender::Quit);
        }
    }
}
struct Controller {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}
impl Controller {
    fn new(snapshot: Snapshot, changed: Sender<ToRender>) -> Result<Self> {
        let (wake, receiver) = Wake::pair().context("create coalesced video wake")?;
        let shared = Arc::new(Shared {
            snapshot: Mutex::new(Arc::new(snapshot)),
            report: Mutex::new(Report {
                phase: Phase::Starting,
                ..Report::default()
            }),
            changed,
            notice_pending: AtomicBool::new(false),
            closing: AtomicBool::new(false),
            fatal: Arc::new(AtomicBool::new(false)),
            retained_cores: Arc::new(AtomicUsize::new(0)),
            startup: Mutex::new(None),
            wake,
        });
        let actor = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("tohru-video-owner".into())
            .spawn(move || {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    supervise(&actor, &receiver)
                })) {
                    Ok(Ok(())) => {}
                    Ok(Err(_))
                        if actor.closing.load(Ordering::Acquire)
                            && !actor.fatal.load(Ordering::Acquire) =>
                    {
                        actor.report(Report {
                            phase: Phase::Stopped,
                            ..Report::default()
                        });
                    }
                    Ok(Err(error)) => actor.failed(message(&error)),
                    Err(_) => {
                        actor.fatal.store(true, Ordering::Release);
                        actor.failed(
                            "video owner panicked; native cleanup requires inspection".into(),
                        );
                    }
                }
            })
            .context("start video owner")?;
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }
    fn publish(&self, snapshot: Snapshot) -> Result<()> {
        if self.worker.as_ref().is_none_or(JoinHandle::is_finished) {
            bail!("video owner has stopped; inspect video.status before restarting owned role")
        }
        let mut previous = self
            .shared
            .snapshot
            .lock()
            .map_err(|_| anyhow::anyhow!("video snapshot unavailable"))?;
        if snapshot.revision <= previous.revision {
            return Err(tohru_shell::video::Error::Revision.into());
        }
        *previous = Arc::new(snapshot);
        drop(previous);
        self.shared.wake.notify();
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
        Ok(())
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.shared.closing.store(true, Ordering::Release);
        // Only bootstrap may abort the private socket; after Ready normal owner
        // cleanup must retain Wayland while dropping GL/core resources.
        if let Ok(startup) = self.shared.startup.lock() {
            if let Some(startup) = startup.as_ref() {
                startup.cancel();
            }
        }
        self.shared.wake.notify();
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                self.shared.report(Report {
                    phase: Phase::Failed,
                    error: "video owner panicked; native cleanup requires inspection".into(),
                    native_cleanup_unproven: true,
                    ..Report::default()
                });
            }
        }
    }
}

/// Only the existing root settings binding calls this after whole-map acceptance.
/// It supplies a strictly increasing VIDEO observation revision, not merely config revision.
pub(crate) fn publish(snapshot: Snapshot, changed: Sender<ToRender>) -> Result<(), String> {
    snapshot.validate().map_err(|error| error.to_string())?;
    let mut owner = CONTROLLER.lock().map_err(|_| "video owner unavailable")?;
    match &*owner {
        OwnerState::Active(controller) => controller
            .publish(snapshot)
            .map_err(|error| message(&error)),
        OwnerState::Retiring(_) => {
            Err("video owner is retiring or its native cleanup is unproven".into())
        }
        OwnerState::Idle => {
            if snapshot.enabled && !snapshot.assignments.is_empty() {
                *owner = OwnerState::Active(
                    Controller::new(snapshot, changed).map_err(|error| message(&error))?,
                );
            }
            Ok(())
        }
    }
}

/// S forwards this at orderly root teardown before closing native platform resources.
pub(crate) fn shutdown() -> Result<(), String> {
    let controller = {
        let mut owner = CONTROLLER.lock().map_err(|_| "video owner unavailable")?;
        match std::mem::replace(&mut *owner, OwnerState::Idle) {
            OwnerState::Active(controller) => {
                *owner = OwnerState::Retiring(Arc::clone(&controller.shared));
                Some(controller)
            }
            OwnerState::Retiring(shared) => {
                *owner = OwnerState::Retiring(shared);
                return Err("video shutdown already pending or native cleanup unproven".into());
            }
            OwnerState::Idle => None,
        }
    };
    if let Some(controller) = controller {
        let shared = Arc::clone(&controller.shared);
        drop(controller);
        let report = shared
            .report
            .lock()
            .map_err(|_| "video shutdown report unavailable")?;
        if report.native_cleanup_unproven || report.retained_cores != 0 {
            return Err(report.error.clone());
        }
        drop(report);
        *CONTROLLER.lock().map_err(|_| "video owner unavailable")? = OwnerState::Idle;
    }
    Ok(())
}

pub(crate) fn query(name: &str, args: &[SysValue]) -> Result<SysValue, String> {
    if name != "video.status" || !args.is_empty() {
        return Err("video.status takes no arguments".into());
    }
    let controller = CONTROLLER.lock().map_err(|_| "video owner unavailable")?;
    let shared = match &*controller {
        OwnerState::Active(controller) => Some(&controller.shared),
        OwnerState::Retiring(shared) => Some(shared),
        OwnerState::Idle => None,
    };
    let requested_revision = match shared {
        Some(shared) => shared.snapshot().map_err(|error| message(&error))?.revision,
        None => 0,
    };
    let report = match shared {
        Some(shared) => {
            let report = shared
                .report
                .lock()
                .map_err(|_| "video report unavailable")?;
            // Reset while holding the report lock: a subsequent change must
            // queue a new notice, while at most one old notice remains queued.
            shared.notice_pending.store(false, Ordering::Release);
            let mut value = report.clone();
            if matches!(&*controller, OwnerState::Retiring(_))
                && !matches!(value.phase, Phase::Failed | Phase::Stopped)
            {
                value.phase = Phase::Stopping;
            }
            value
        }
        None => Report {
            phase: Phase::Stopped,
            ..Report::default()
        },
    };
    Ok(map(vec![
        ("revision", SysValue::Text(report.revision.to_string())),
        (
            "requested_revision",
            SysValue::Text(requested_revision.to_string()),
        ),
        ("phase", SysValue::Text(phase(report.phase).into())),
        ("error", SysValue::Text(report.error)),
        ("logind", SysValue::Text(protection(report.logind).into())),
        ("locker", SysValue::Text(protection(report.locker).into())),
        ("live_cores", number(report.live_cores)),
        ("retained_cores", number(report.retained_cores)),
        (
            "native_cleanup_unproven",
            SysValue::Bool(report.native_cleanup_unproven),
        ),
        (
            "outputs",
            SysValue::Map(
                report
                    .outputs
                    .into_iter()
                    .map(|(name, output)| {
                        let mut fields = vec![
                            ("path", SysValue::Text(output.path)),
                            ("state", SysValue::Text(player_state(output.state).into())),
                            ("desired_pause", SysValue::Bool(output.desired_pause)),
                            ("error", SysValue::Text(output.error)),
                            (
                                "pause_reason",
                                SysValue::Text(pause_reason(output.pause_reason).into()),
                            ),
                        ];
                        if let Some(paused) = output.observed_pause {
                            fields.push(("observed_pause", SysValue::Bool(paused)));
                        }
                        (name, map(fields))
                    })
                    .collect(),
            ),
        ),
    ]))
}
fn map(fields: Vec<(&str, SysValue)>) -> SysValue {
    SysValue::Map(
        fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}
fn number(value: usize) -> SysValue {
    SysValue::Num(f64::from(u32::try_from(value).unwrap_or(u32::MAX)))
}
fn phase(value: Phase) -> &'static str {
    match value {
        Phase::Starting => "starting",
        Phase::Running => "running",
        Phase::Stopping => "stopping",
        Phase::Stopped => "stopped",
        Phase::Failed => "failed",
    }
}
fn player_state(value: PlayerState) -> &'static str {
    match value {
        PlayerState::Unknown => "unknown",
        PlayerState::Disabled => "disabled",
        PlayerState::Disconnected => "disconnected",
        PlayerState::WaitingForConfigure => "waiting_for_configure",
        PlayerState::Superseded => "superseded",
        PlayerState::StartingPaused => "starting_paused",
        PlayerState::Pausing => "pausing",
        PlayerState::Paused => "paused",
        PlayerState::Playing => "playing",
        PlayerState::Failed => "failed",
    }
}
fn pause_reason(value: Option<PauseReason>) -> &'static str {
    match value {
        None => "",
        Some(PauseReason::Protected) => "protected",
        Some(PauseReason::UnknownProtection) => "protection_unknown",
        Some(PauseReason::Occupied) => "occupied",
        Some(PauseReason::UnknownOccupancy) => "occupancy_unknown",
    }
}
fn protection(value: Protection) -> &'static str {
    match value {
        Protection::Unknown => "unknown",
        Protection::Clear => "clear",
        Protection::Protected => "protected",
    }
}
fn display_path(display: Option<OsString>, runtime: Option<OsString>) -> Result<PathBuf> {
    let display = PathBuf::from(
        display.context("video needs explicit WAYLAND_DISPLAY for its separate connection")?,
    );
    if display.as_os_str().is_empty() {
        bail!("video WAYLAND_DISPLAY must not be empty")
    }
    if display.is_absolute() {
        return Ok(display);
    }
    let runtime = PathBuf::from(runtime.context("video needs an absolute owned XDG_RUNTIME_DIR")?);
    if !runtime.is_absolute() {
        bail!("video XDG_RUNTIME_DIR must be absolute")
    }
    Ok(runtime.join(display))
}
fn message(error: &anyhow::Error) -> String {
    let mut value = format!("{error:#}");
    let mut limit = value.len().min(4096);
    while !value.is_char_boundary(limit) {
        limit -= 1;
    }
    value.truncate(limit);
    value
}

struct Live {
    mpv: Option<mpv::Player>,
    target: Rc<egl::Target>,
    path: String,
    geometry: wayland::Geometry,
    dirty: bool,
    presented: bool,
    resized: bool,
    available_frame: bool,
    due: Instant,
    started: Instant,
    fatal: Arc<AtomicBool>,
    retained: Arc<AtomicUsize>,
}
impl Live {
    fn new(
        display: Rc<egl::Display>,
        layer: &wayland::LayerRecord,
        geometry: wayland::Geometry,
        path: &str,
        shared: &Shared,
    ) -> Result<Self> {
        let target = egl::Target::new(
            display,
            layer.layer.clone(),
            layer.viewport.clone(),
            geometry.logical,
            geometry.physical,
            geometry.scale,
        )?;
        target.current()?;
        let init = target.gl_init()?;
        let wayland = target.wayland_display()?;
        let file = CString::new(path).context("validated video path has NUL")?;
        // SAFETY: Target retains original current GL context, stable resolver and Wayland display.
        // Every render call/Drop below rebinds it; on bind failure Drop retains all dependencies.
        let mpv = unsafe { mpv::Player::new(&file, init, wayland, shared.wake.clone()) }?;
        Ok(Self {
            mpv: Some(mpv),
            target,
            path: path.into(),
            geometry,
            dirty: false,
            presented: false,
            resized: false,
            available_frame: false,
            due: Instant::now(),
            started: Instant::now(),
            fatal: Arc::clone(&shared.fatal),
            retained: Arc::clone(&shared.retained_cores),
        })
    }
    fn update(
        &mut self,
        geometry: wayland::Geometry,
        playback: Playback,
        shared: &Shared,
        revision: u64,
        now: Instant,
    ) -> Result<OutputReport> {
        self.target.current()?;
        if self.geometry != geometry {
            self.target
                .resize(geometry.logical, geometry.physical, geometry.scale)?;
            self.geometry = geometry;
            self.resized = self.presented;
            self.dirty |= self.presented;
        }
        let mpv = self.mpv.as_mut().context("video core retired")?;
        mpv.events(now)?;
        let paused = matches!(playback, Playback::Paused(_));
        // Recheck the latest root revision immediately before ANY unpause. Newer
        // snapshots are resolved on the next pass before a stale frame can swap.
        if paused || shared.current(revision) {
            mpv.pause(paused, now)?;
        }
        if mpv.update() {
            self.available_frame = true;
            self.dirty = true;
            self.due = Instant::now()
                .checked_add(mpv.delay()?)
                .context("invalid video target time")?;
        }
        if paused && mpv.paused() == Some(true) {
            if self.resized || (!self.presented && self.available_frame) {
                self.dirty = true;
            }
            self.due = now;
        }
        if paused && self.dirty && mpv.paused() != Some(true) {
            mpv.render(geometry.physical, true)?;
            self.dirty = false;
        }
        let observed = mpv.paused();
        let state = match (paused, observed) {
            (true, Some(true)) => PlayerState::Paused,
            (true, _) => PlayerState::Pausing,
            (false, Some(false)) => PlayerState::Playing,
            (false, _) => PlayerState::StartingPaused,
        };
        Ok(OutputReport {
            path: self.path.clone(),
            state,
            desired_pause: paused,
            observed_pause: observed,
            error: String::new(),
            pause_reason: match playback {
                Playback::Playing => None,
                Playback::Paused(reason) => Some(reason),
            },
        })
    }
    fn draw(&mut self) -> Result<()> {
        self.target.current()?;
        let mpv = self.mpv.as_ref().context("video core retired")?;
        mpv.render(self.geometry.physical, false)?;
        self.target.swap(self.geometry.physical)?;
        mpv.swapped();
        self.dirty = false;
        self.presented = true;
        self.resized = false;
        Ok(())
    }
    fn deadline(&self) -> Option<Instant> {
        self.mpv.as_ref().and_then(mpv::Player::deadline)
    }
}
impl Drop for Live {
    fn drop(&mut self) {
        if let Some(mut mpv) = self.mpv.take() {
            if self.target.current().is_ok() {
                drop(mpv);
            } else {
                let _ = mpv.terminal_pause(); // best effort only; no false pause/cleanup acknowledgment.
                self.fatal.store(true, Ordering::Release);
                self.retained.fetch_add(1, Ordering::AcqRel);
                std::mem::forget(Rc::clone(&self.target));
                std::mem::forget(mpv);
            }
        }
    }
}
struct Retry {
    path: String,
    backoff: Backoff,
    error: String,
}
struct Engine {
    players: BTreeMap<String, Live>,
    wayland: wayland::Wayland,
    display: Option<Rc<egl::Display>>,
    retries: BTreeMap<String, Retry>,
}
impl Drop for Engine {
    fn drop(&mut self) {
        self.players.clear();
        self.wayland.state.layers.clear();
        self.display = None;
    }
}

struct Bootstrap<'a> {
    shared: &'a Shared,
    startup: Arc<startup::Startup>,
    watchdog: Option<JoinHandle<()>>,
}
impl<'a> Bootstrap<'a> {
    fn new(shared: &'a Shared) -> Result<Self> {
        let startup = Arc::new(startup::Startup::default());
        {
            let mut current = shared
                .startup
                .lock()
                .map_err(|_| anyhow::anyhow!("video bootstrap state unavailable"))?;
            if shared.closing.load(Ordering::Acquire) {
                bail!("video owner is closing")
            }
            *current = Some(Arc::clone(&startup));
        }
        let mut guard = Self {
            shared,
            startup,
            watchdog: None,
        };
        let watch = Arc::clone(&guard.startup);
        guard.watchdog = Some(
            thread::Builder::new()
                .name("tohru-video-bootstrap".into())
                .spawn(move || {
                    let _ = watch.watch(Duration::from_secs(2));
                })
                .context("start owned registry watchdog")?,
        );
        Ok(guard)
    }
}
impl Drop for Bootstrap<'_> {
    fn drop(&mut self) {
        self.startup.cancel();
        if let Some(watchdog) = self.watchdog.take() {
            let _ = watchdog.join();
        }
        if let Ok(mut current) = self.shared.startup.lock() {
            if current
                .as_ref()
                .is_some_and(|value| Arc::ptr_eq(value, &self.startup))
            {
                *current = None;
            }
        }
    }
}

fn supervise(shared: &Shared, receiver: &Receiver) -> Result<()> {
    let mut retry = None;
    while !shared.closing.load(Ordering::Acquire) {
        let snapshot = shared.snapshot()?;
        if !snapshot.enabled || snapshot.assignments.is_empty() {
            shared.report(inactive_report(&snapshot));
            thread::park();
            continue;
        }
        let started = Instant::now();
        match run(shared, receiver) {
            // An inactive map releases the private native graph but keeps the
            // owner available for a later accepted enable/assignment update.
            Ok(()) => continue,
            Err(_)
                if shared.closing.load(Ordering::Acquire)
                    && !shared.fatal.load(Ordering::Acquire) =>
            {
                break;
            }
            Err(error) => {
                shared.failed(message(&error));
                if shared.fatal.load(Ordering::Acquire) {
                    return Ok(());
                }
                if started.elapsed() >= Duration::from_secs(30) {
                    retry = None;
                }
                let next = Backoff::failed(retry, Instant::now());
                retry = Some(next);
                while !shared.closing.load(Ordering::Acquire) {
                    let left = next.retry_at().saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        break;
                    }
                    // Root updates/shutdown unpark; updates cannot bypass the
                    // retry deadline. There is no live backend during this wait.
                    thread::park_timeout(left);
                }
            }
        }
    }
    let snapshot = shared.snapshot()?;
    shared.report(Report {
        phase: Phase::Stopped,
        revision: snapshot.revision,
        logind: snapshot.logind,
        locker: snapshot.locker,
        ..Report::default()
    });
    Ok(())
}

fn inactive_report(snapshot: &Snapshot) -> Report {
    Report {
        revision: snapshot.revision,
        phase: Phase::Stopped,
        logind: snapshot.logind,
        locker: snapshot.locker,
        outputs: snapshot
            .assignments
            .iter()
            .map(|(name, path)| {
                (
                    name.clone(),
                    OutputReport {
                        path: path.clone(),
                        state: PlayerState::Disabled,
                        desired_pause: true,
                        ..OutputReport::default()
                    },
                )
            })
            .collect(),
        ..Report::default()
    }
}

fn run(shared: &Shared, receiver: &Receiver) -> Result<()> {
    // Secondary connection must never consume inherited WAYLAND_SOCKET, which
    // can belong to the root's primary rendering connection. Use the SDK's
    // from_socket facility with a fresh socket to its explicitly named endpoint.
    let bootstrap = Bootstrap::new(shared)?;
    let endpoint = display_path(
        std::env::var_os("WAYLAND_DISPLAY"),
        std::env::var_os("XDG_RUNTIME_DIR"),
    )?;
    let socket = UnixStream::connect(endpoint).context("connect owned video Wayland session")?;
    bootstrap.startup.publish(
        socket
            .try_clone()
            .context("duplicate video bootstrap cancellation socket")?,
    )?;
    let connection =
        Connection::from_socket(socket).context("initialize owned video Wayland connection")?;
    let initialized = wayland::Wayland::initialize(connection);
    bootstrap.startup.finish()?; // Watchdog can no longer close a connection used by GL.
    drop(bootstrap); // join watchdog BEFORE any EGL/core allocation.
    let wayland = initialized?;
    let mut engine = Engine {
        players: BTreeMap::new(),
        wayland,
        display: None,
        retries: BTreeMap::new(),
    };
    while !shared.closing.load(Ordering::Acquire) {
        receiver.drain().context("drain owned video wake")?;
        engine.wayland.dispatch()?;
        let snapshot = shared.snapshot()?;
        if !snapshot.enabled || snapshot.assignments.is_empty() {
            break; // Drop the complete private graph before parking in supervise.
        }
        let connected = engine.wayland.connected(&snapshot)?;
        let now = Instant::now();
        engine.retire(&snapshot, &connected, now);
        if shared.fatal.load(Ordering::Acquire) {
            bail!("video native teardown unproven; no new backend may be created")
        }
        let mut report = Report {
            phase: if snapshot.enabled && !snapshot.assignments.is_empty() {
                Phase::Running
            } else {
                Phase::Stopped
            },
            revision: snapshot.revision,
            logind: snapshot.logind,
            locker: snapshot.locker,
            ..Report::default()
        };
        let mut deadlines = Vec::new();
        for (name, path) in &snapshot.assignments {
            let action = snapshot.action(name, connected.contains_key(name));
            let Action::Keep { playback, .. } = action else {
                report.outputs.insert(
                    name.clone(),
                    OutputReport {
                        path: path.clone(),
                        state: if snapshot.enabled {
                            PlayerState::Disconnected
                        } else {
                            PlayerState::Disabled
                        },
                        desired_pause: true,
                        ..OutputReport::default()
                    },
                );
                continue;
            };
            let output = connected
                .get(name)
                .context("connected video output disappeared from snapshot")?;
            let result = engine.step(
                name,
                path,
                playback,
                output,
                shared,
                snapshot.revision,
                now,
                &mut deadlines,
            );
            match result {
                Ok(output) => {
                    report.outputs.insert(name.clone(), output);
                }
                Err(error) => {
                    engine.players.remove(name);
                    engine.wayland.state.layers.remove(name);
                    if shared.fatal.load(Ordering::Acquire) {
                        bail!(
                            "video native teardown unproven after backend error; no reuse permitted"
                        )
                    }
                    let error = message(&error);
                    let previous = engine.retries.get(name).map(|retry| retry.backoff);
                    let backoff = Backoff::failed(previous, now);
                    deadlines.push(backoff.retry_at());
                    engine.retries.insert(
                        name.clone(),
                        Retry {
                            path: path.clone(),
                            backoff,
                            error: error.clone(),
                        },
                    );
                    report.outputs.insert(
                        name.clone(),
                        OutputReport {
                            path: path.clone(),
                            state: PlayerState::Failed,
                            desired_pause: true,
                            error,
                            ..OutputReport::default()
                        },
                    );
                }
            }
        }
        if engine.players.is_empty() && engine.wayland.state.layers.is_empty() {
            engine.display = None;
        }
        if shared.fatal.load(Ordering::Acquire) {
            bail!(
                "video native context teardown unproven; stop the containing owned role before reuse"
            )
        }
        report.live_cores = engine.players.len();
        report.retained_cores = shared.retained_cores.load(Ordering::Acquire);
        shared.report(report);
        let timeout = deadlines
            .into_iter()
            .min()
            .map(|deadline| deadline.saturating_duration_since(Instant::now()));
        engine.wayland.wait(receiver, timeout)?;
    }
    drop(engine); // libmpv render -> core -> EGL window/surface -> Wayland roles/display.
    if shared.fatal.load(Ordering::Acquire) {
        bail!("video native cleanup unproven; containing owned role must exit")
    }
    // Supervisor reports the accepted inactive snapshot or final joined shutdown.
    Ok(())
}

impl Engine {
    fn retire(
        &mut self,
        snapshot: &Snapshot,
        connected: &BTreeMap<String, wayland::Connected>,
        now: Instant,
    ) {
        let stale: Vec<_> = self
            .wayland
            .state
            .layers
            .iter()
            .filter_map(|(name, layer)| {
                let keep = match snapshot.action(name, connected.contains_key(name)) {
                    Action::Stop => false,
                    Action::Keep { path, .. } => {
                        !layer.closed
                            && self
                                .players
                                .get(name)
                                .is_none_or(|player| player.path == path)
                            && connected
                                .get(name)
                                .is_some_and(|output| output.output == layer.output)
                    }
                };
                let closed = layer.closed && connected.get(name).is_some_and(|output|output.output == layer.output)
                    && matches!(snapshot.action(name, true), Action::Keep{path,..} if self.players.get(name).is_none_or(|player|player.path==path));
                (!keep).then(|| (name.clone(), closed))
            })
            .collect();
        for (name, closed) in stale {
            self.players.remove(&name);
            self.wayland.state.layers.remove(&name);
            if closed {
                if let Some(path) = snapshot.assignments.get(&name) {
                    let previous = self
                        .retries
                        .get(&name)
                        .filter(|retry| retry.path == *path)
                        .map(|retry| retry.backoff);
                    self.retries.insert(
                        name,
                        Retry {
                            path: path.clone(),
                            backoff: Backoff::failed(previous, now),
                            error: "compositor closed video layer".into(),
                        },
                    );
                }
            }
        }
        self.retries.retain(|name,retry|matches!(snapshot.action(name,connected.contains_key(name)),Action::Keep{path,..} if path==retry.path));
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "one per-output state transition receives current policy, topology and actor observations"
    )]
    fn step(
        &mut self,
        name: &str,
        path: &str,
        playback: Playback,
        output: &wayland::Connected,
        shared: &Shared,
        revision: u64,
        now: Instant,
        deadlines: &mut Vec<Instant>,
    ) -> Result<OutputReport> {
        if let Some(retry) = self.retries.get(name) {
            if retry.backoff.retry_at() > now {
                deadlines.push(retry.backoff.retry_at());
                return Ok(OutputReport {
                    path: path.into(),
                    state: PlayerState::Failed,
                    desired_pause: true,
                    error: retry.error.clone(),
                    ..OutputReport::default()
                });
            }
        }
        if !self.wayland.state.layers.contains_key(name) {
            self.wayland
                .create_layer(name, output.output.clone(), now)?;
        }
        let layer = self
            .wayland
            .state
            .layers
            .get(name)
            .context("video layer missing")?;
        let Some(geometry) = self.wayland.geometry(name, output)? else {
            let deadline = layer.created + Duration::from_secs(2);
            if now >= deadline {
                bail!("compositor did not configure video layer within2 seconds")
            }
            deadlines.push(deadline);
            return Ok(OutputReport {
                path: path.into(),
                state: PlayerState::WaitingForConfigure,
                desired_pause: true,
                ..OutputReport::default()
            });
        };
        if !shared.current(revision) {
            return Ok(OutputReport {
                path: path.into(),
                state: PlayerState::Superseded,
                desired_pause: true,
                ..OutputReport::default()
            });
        }
        if !self.players.contains_key(name) {
            if self.display.is_none() {
                self.display = Some(egl::Display::new(
                    self.wayland.connection.clone(),
                    Arc::clone(&shared.fatal),
                )?);
            }
            let display = Rc::clone(self.display.as_ref().context("video EGL display missing")?);
            let live = Live::new(display, layer, geometry, path, shared)?;
            self.players.insert(name.to_owned(), live);
        }
        let live = self.players.get_mut(name).context("video player missing")?;
        let now = Instant::now();
        let report = live.update(geometry, playback, shared, revision, now)?;
        if let Some(deadline) = live.deadline() {
            deadlines.push(deadline);
        }
        let layer = self
            .wayland
            .state
            .layers
            .get(name)
            .context("video layer missing")?;
        // Paused players may present their one frozen frame/resize without
        // advancing decoding. Never swap motion while a pause is unacknowledged.
        let present = matches!(playback, Playback::Playing) || report.observed_pause == Some(true);
        if present && live.dirty && layer.frame_ready {
            if live.due <= now && shared.current(revision) {
                self.wayland.request_frame(name)?;
                live.draw()?;
            } else if live.due > now {
                deadlines.push(live.due);
            }
        }
        // A successful enqueue/configure is not a healthy decoder. Preserve
        // accumulated backoff across early asynchronous failures/restarts.
        let stable = live.started + Duration::from_secs(30);
        if now >= stable {
            self.retries.remove(name);
        } else if self.retries.contains_key(name) {
            deadlines.push(stable);
        }
        Ok(report)
    }
}
