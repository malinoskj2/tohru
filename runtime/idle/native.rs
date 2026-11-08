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
pub(crate) use sleep_guard::native_held;
static GUARD: OnceLock<Mutex<Option<sleep_guard::Guard>>> = OnceLock::new();
use crate::scene::{Event, ToRender};
use anyhow::{Context, Result};
use mlua::{Lua, Table};
use observer::{Observer, Sink};
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Sender, SyncSender},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tohru_shell::idle::input::Cancellation;
use tohru_shell::idle::{Config, Timeouts};

static OWNER: OnceLock<Mutex<Option<Owner>>> = OnceLock::new();
struct Owner {
    logind: Option<logind::Watch>,
    service: Option<screensaver::Service>,
    observer: Option<Arc<Observer>>,
    _worker: Worker,
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.logind.take();
        self.service.take();
        self.observer.take();
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
                    if worker_stop.load(Ordering::Acquire) {
                        break;
                    }
                    if !matches!(task.request, Request::State)
                        && !worker_generation.current(task.generation)
                    {
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
            generation,
        });
        Ok((
            Self {
                stop,
                thread: Some(thread),
                sender,
            },
            sink,
        ))
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.sender.try_send(Task {
            generation: 0,
            request: Request::State,
        });
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
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
        lua.create_function(|_, ()| {
            if let Some(guard) = GUARD.get() {
                guard
                    .lock()
                    .map_err(|_| mlua::Error::runtime("locker delay owner mutex poisoned"))?
                    .take();
            }
            Ok(())
        })?,
    )?;

    api.set(
        "start",
        lua.create_function(move |_, table: Table| {
            let config = config(&table)?;
            let mut owner = owner()?;
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
                _worker: worker,
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
        lua.create_function(|_, ()| {
            owner()?.take();
            Ok(())
        })?,
    )?;
    lua.globals().set("tohru_idle", api)
}
