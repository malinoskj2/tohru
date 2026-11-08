//! Optional root-only Luau binding. One owner survives ordinary VM replacement.
//! Copied into pinned Pleamar; uses only its existing native dependencies.
#[path = "tohru_idle/logind.rs"]
mod logind;
#[path = "tohru_idle/observer.rs"]
mod observer;
#[path = "tohru_idle/screensaver.rs"]
mod screensaver;
#[path = "tohru_idle/sleep_guard.rs"]
mod sleep_guard;
use crate::scene::{Event, ToRender};
use anyhow::{Context, Result};
use mlua::{Lua, Table};
use observer::{Observer, Sink};
pub(crate) use sleep_guard::native_held;
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Sender, SyncSender},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tohru_shell::idle::input::Cancellation;
use tohru_shell::idle::teardown::{self, Failure, Stage};
use tohru_shell::idle::{Config, Timeouts};

static GUARD: OnceLock<Mutex<Option<sleep_guard::Guard>>> = OnceLock::new();
static OWNER: OnceLock<Mutex<Option<Owner>>> = OnceLock::new();
static SHUTDOWN: Mutex<()> = Mutex::new(());
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

struct Owner {
    logind: Option<logind::Watch>,
    service: Option<screensaver::Service>,
    observer: Option<Arc<Observer>>,
    _worker: Option<Worker>,
}

impl Owner {
    fn close_stage(&mut self, stage: Stage) -> Result<()> {
        match stage {
            Stage::CancelRequests => {
                if let Some(worker) = &self._worker {
                    worker.request_stop();
                }
                if let Some(observer) = &self.observer {
                    observer.request_stop();
                }
            }
            Stage::Logind => {
                if let Some(mut watch) = self.logind.take() {
                    watch.stop()?;
                }
            }
            Stage::ScreenSaver => {
                if let Some(mut service) = self.service.take() {
                    service.stop()?;
                }
            }
            Stage::Observer => {
                if let Some(observer) = self.observer.take() {
                    // Join the actual thread even if a closing bus callback
                    // briefly retains an Arc to the observation mailbox.
                    observer.stop()?;
                }
            }
            Stage::Actions => {
                if let Some(mut worker) = self._worker.take() {
                    worker.stop()?;
                }
            }
            Stage::Guard => {}
        }
        Ok(())
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        for failure in teardown::run(|stage| self.close_stage(stage)) {
            eprintln!("idle · cleanup {failure}");
        }
    }
}

fn close_owned(ordinary: bool, locker: bool) -> Vec<Failure> {
    let mut failures = Vec::new();
    let mut owner = if ordinary {
        OWNER
            .get()
            .and_then(|slot| teardown::take_owned(slot, Stage::CancelRequests, &mut failures))
    } else {
        None
    };
    // Cancel this process's work before waiting on the independent guard slot.
    if let Some(owner) = owner.as_mut() {
        if let Err(error) = owner.close_stage(Stage::CancelRequests) {
            failures.push(Failure {
                stage: Stage::CancelRequests,
                detail: format!("{error:#}"),
            });
        }
    }
    let mut guard = if locker {
        GUARD
            .get()
            .and_then(|slot| teardown::take_owned(slot, Stage::Guard, &mut failures))
    } else {
        None
    };
    failures.extend(teardown::run(|stage| -> Result<()> {
        if stage == Stage::Guard {
            if let Some(mut guard) = guard.take() {
                guard.stop()?;
            }
        } else if let Some(owner) = owner.as_mut() {
            owner.close_stage(stage)?;
        }
        Ok(())
    }));
    failures
}

/// Orderly process teardown. This takes only this process's registered owners;
/// it never sends a scene command, changes native lock authority or powers down.
pub(crate) fn shutdown() {
    let _serial = match SHUTDOWN.lock() {
        Ok(serial) => serial,
        Err(poisoned) => {
            eprintln!("idle · shutdown serial mutex poisoned; recovered");
            poisoned.into_inner()
        }
    };
    if SHUTTING_DOWN.swap(true, Ordering::AcqRel) {
        return;
    }
    for failure in close_owned(true, true) {
        eprintln!("idle · shutdown {failure}");
    }
}

fn can_start() -> mlua::Result<()> {
    if SHUTTING_DOWN.load(Ordering::Acquire) {
        return Err(mlua::Error::runtime("idle process is shutting down"));
    }
    Ok(())
}

fn stopped(failures: Vec<Failure>) -> mlua::Result<()> {
    if failures.is_empty() {
        Ok(())
    } else {
        Err(mlua::Error::runtime(
            failures
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; "),
        ))
    }
}

#[derive(Clone, Copy)]
enum Request {
    Lock,
    Suspend,
    State,
}

struct Task {
    generation: u64,
    request: Request,
}

struct Worker {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    sender: SyncSender<Task>,
    cancellation: Cancellation,
}

struct NativeSink {
    render: Sender<ToRender>,
    sender: SyncSender<Task>,
    generation: Cancellation,
}

impl NativeSink {
    fn enqueue(&self, request: Request, stamp: u64) {
        if self
            .sender
            .try_send(Task {
                generation: stamp,
                request,
            })
            .is_err()
        {
            self.status("idle action queue unavailable; request not delivered");
        }
    }
}

impl Sink for NativeSink {
    fn opacity(&self, alpha: f32) {
        let _ = self.render.send(ToRender::Fact("idle_alpha", alpha));
    }
    fn status(&self, status: &str) {
        let _ = self
            .render
            .send(ToRender::Text("idle_status", status.to_owned()));
    }
    fn rearmed(&self) {
        self.generation.invalidate();
    }
    fn lock(&self, stamp: u64) {
        self.enqueue(Request::Lock, stamp);
    }
    fn suspend(&self, stamp: u64) {
        self.enqueue(Request::Suspend, stamp);
    }

    fn state(&self, stamp: u64) {
        self.enqueue(Request::State, stamp);
    }
}

impl Worker {
    fn start(
        render: Sender<ToRender>,
        generation: Cancellation,
    ) -> Result<(Self, Arc<NativeSink>)> {
        let (sender, receiver) = mpsc::sync_channel::<Task>(4);
        let stop = Arc::new(AtomicBool::new(false));
        let (worker_generation, worker_stop) = (generation.clone(), stop.clone());
        let worker_render = render.clone();
        let thread = thread::Builder::new()
            .name("tohru-idle-actions".into())
            .spawn(move || {
                while let Ok(task) = receiver.recv() {
                    if !teardown::dispatch_allowed(
                        &worker_stop,
                        &worker_generation,
                        task.generation,
                        matches!(task.request, Request::State),
                    ) {
                        if worker_stop.load(Ordering::Acquire) {
                            break;
                        }
                        continue;
                    }
                    let event = match task.request {
                        Request::Lock => "emit lock_request",
                        Request::Suspend => "emit lock_and_suspend",
                        Request::State => "emit state_request",
                    };
                    if let Err(error) = crate::platform::say("tohru-lock", event) {
                        let _ = worker_render.send(ToRender::Text(
                            "idle_status",
                            format!("locker request not delivered: {error}"),
                        ));
                    }
                }
            })
            .context("start native idle action dispatcher")?;
        let sink = Arc::new(NativeSink {
            render,
            sender: sender.clone(),
            generation: generation.clone(),
        });
        Ok((
            Self {
                stop,
                thread: Some(thread),
                sender,
                cancellation: generation,
            },
            sink,
        ))
    }
}

impl Worker {
    fn request_stop(&self) {
        teardown::request_stop(&self.stop, &self.cancellation, || {
            let _ = self.sender.try_send(Task {
                generation: 0,
                request: Request::State,
            });
        });
    }

    fn stop(&mut self) -> Result<()> {
        self.request_stop();
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| anyhow::anyhow!("idle action dispatcher panicked"))?;
        }
        Ok(())
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn config(table: &Table) -> mlua::Result<Config> {
    let duration = |key, default| -> mlua::Result<Option<Duration>> {
        let value = table.get::<Option<u64>>(key)?.unwrap_or(default);
        Ok(if value == 0 {
            None
        } else {
            Some(Duration::from_secs(value))
        })
    };
    Ok(Config {
        lock: Timeouts {
            unlocked: duration("lock_seconds", 1800)?,
            locked: None,
        },
        screen_off: Timeouts {
            unlocked: duration("screen_off_unlocked_seconds", 0)?,
            locked: duration("screen_off_locked_seconds", 60)?,
        },
        suspend: Timeouts {
            unlocked: duration("suspend_unlocked_seconds", 7200)?,
            locked: duration("suspend_locked_seconds", 5400)?,
        },
        fade: Duration::from_millis(
            table
                .get::<Option<u64>>("fade_milliseconds")?
                .unwrap_or(2000),
        ),
    })
}

fn report(error: impl Into<anyhow::Error>) -> mlua::Error {
    mlua::Error::runtime(format!("{:#}", error.into()))
}

fn owner() -> mlua::Result<std::sync::MutexGuard<'static, Option<Owner>>> {
    OWNER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| mlua::Error::runtime("idle owner mutex poisoned"))
}

pub(crate) fn install(
    lua: &Lua,
    render: Sender<ToRender>,
    logic: Sender<Event>,
) -> mlua::Result<()> {
    let api = lua.create_table()?;
    let guard_render = render.clone();
    api.set(
        "start_guard",
        lua.create_function(move |_, ()| {
            let mut guard = GUARD
                .get_or_init(|| Mutex::new(None))
                .lock()
                .map_err(|_| mlua::Error::runtime("locker delay owner mutex poisoned"))?;
            can_start()?;
            if guard.is_none() {
                *guard = Some(
                    sleep_guard::Guard::start(guard_render.clone(), logic.clone())
                        .map_err(report)?,
                );
            }
            Ok(())
        })?,
    )?;
    api.set(
        "stop_guard",
        lua.create_function(|_, ()| stopped(close_owned(false, true)))?,
    )?;

    api.set(
        "start",
        lua.create_function(move |_, table: Table| {
            let config = config(&table)?;
            let mut owner = owner()?;
            can_start()?;
            if let Some(owner) = owner.as_ref() {
                return owner
                    .observer
                    .as_ref()
                    .ok_or_else(|| mlua::Error::runtime("idle owner stopping"))?
                    .configure(config)
                    .map_err(report);
            }
            let cancellation = Cancellation::default();
            let (worker, sink) =
                Worker::start(render.clone(), cancellation.clone()).map_err(report)?;
            let observer =
                Arc::new(Observer::spawn(config, sink.clone(), cancellation).map_err(report)?);
            let service = screensaver::Service::start(&observer).map_err(report)?;
            let logind = logind::Watch::start(&observer).map_err(report)?;
            // Query on each root-process start; hints affect timeouts only. Dedicated
            // locker still checks immutable process-native held before sleep.

            *owner = Some(Owner {
                logind: Some(logind),
                service: Some(service),
                observer: Some(observer),
                _worker: Some(worker),
            });
            Ok(())
        })?,
    )?;
    api.set(
        "held",
        lua.create_function(|_, held: bool| {
            if let Some(observer) = owner()?.as_ref().and_then(|owner| owner.observer.as_ref()) {
                observer
                    .observe(|observed| observed.held = Some(held))
                    .map_err(report)?;
            }
            Ok(())
        })?,
    )?;
    api.set(
        "caffeine",
        lua.create_function(|_, enabled: bool| {
            let owner = owner()?;
            let watch = owner
                .as_ref()
                .and_then(|owner| owner.logind.as_ref())
                .ok_or_else(|| mlua::Error::runtime("idle owner not started"))?;
            watch.caffeine(enabled).map_err(report)
        })?,
    )?;
    api.set(
        "resume",
        lua.create_function(|_, ()| {
            if let Some(observer) = owner()?.as_ref().and_then(|owner| owner.observer.as_ref()) {
                observer
                    .observe(|observed| observed.resume = observed.resume.wrapping_add(1))
                    .map_err(report)?;
            }
            Ok(())
        })?,
    )?;
    api.set(
        "stop",
        lua.create_function(|_, ()| stopped(close_owned(true, false)))?,
    )?;
    lua.globals().set("tohru_idle", api)
}
