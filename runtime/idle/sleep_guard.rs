//! Dedicated locker-local logind delay. No remote hint can acknowledge it.
use crate::scene::{Event, ToRender};
use anyhow::{Context, Result};
use std::os::fd::OwnedFd;
use std::sync::{
    Arc, Mutex, OnceLock, Weak,
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tohru_shell::idle::delay::Delay;
use zbus::blocking::{Connection, MessageIterator, connection::Builder};

const LOGIND: &str = "org.freedesktop.login1";
const PATH: &str = "/org/freedesktop/login1";
const MANAGER: &str = "org.freedesktop.login1.Manager";
static ACTIVE: OnceLock<Mutex<Weak<Mutex<Delay<OwnedFd>>>>> = OnceLock::new();

pub(super) struct Guard {
    connection: Option<Connection>,
    thread: Option<JoinHandle<()>>,
    _delay: Arc<Mutex<Delay<OwnedFd>>>,
    stopped: Arc<AtomicBool>,
}

fn acquire(connection: &Connection) -> Result<OwnedFd> {
    let reply = connection.call_method(
        Some(LOGIND),
        PATH,
        Some(MANAGER),
        "Inhibit",
        &(
            "sleep",
            "tohru-lock",
            "Wait for native compositor lock",
            "delay",
        ),
    )?;
    let fd: zbus::zvariant::OwnedFd = reply.body().deserialize()?;
    let fd = OwnedFd::from(fd);
    fd.try_clone()
        .context("make sleep delay descriptor close-on-exec")
}

/// Called ONLY while native engaged/cycle authority is held. No queries, sends,
/// callbacks or GPU work occur here: one mutex then a descriptor close.
pub(crate) fn native_held() {
    let Some(active) = ACTIVE.get() else {
        return;
    };
    let Some(delay) = active.lock().ok().and_then(|active| active.upgrade()) else {
        return;
    };
    if let Ok(mut delay) = delay.lock() {
        delay.acknowledge();
    }
}

impl Guard {
    pub(super) fn start(render: Sender<ToRender>, logic: Sender<Event>) -> Result<Self> {
        let connection = Builder::system()?
            .method_timeout(Duration::from_secs(5))
            .max_queued(16)
            .build()
            .context("connect locker sleep delay")?;
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(LOGIND)?
            .interface(MANAGER)?
            .member("PrepareForSleep")?
            .path(PATH)?
            .build();
        // Subscribe before acquiring the delay, preserving intervening signals.
        let changes = MessageIterator::for_match_rule(rule, &connection, Some(16))?;
        let delay = Arc::new(Mutex::new(Delay::new(
            acquire(&connection).context("acquire locker sleep delay")?,
        )));
        let active = ACTIVE.get_or_init(|| Mutex::new(Weak::new()));
        *active
            .lock()
            .map_err(|_| anyhow::anyhow!("sleep guard owner mutex poisoned"))? =
            Arc::downgrade(&delay);
        let stopped = Arc::new(AtomicBool::new(false));
        let thread_stopped = stopped.clone();
        let thread_delay = delay.clone();
        let thread_connection = connection.clone();
        let thread = thread::Builder::new()
            .name("tohru-lock-sleep".into())
            .spawn(move || {
                for message in changes {
                    if thread_stopped.load(Ordering::Acquire)
                        || super::SHUTTING_DOWN.load(Ordering::Acquire)
                    {
                        break;
                    }
                    let Ok(message) = message else {
                        break;
                    };
                    let Ok((sleeping,)) = message.body().deserialize::<(bool,)>() else {
                        continue;
                    };
                    if sleeping {
                        if let Ok(mut delay) = thread_delay.lock() {
                            delay.prepare();
                        }
                        // This native helper acquires engaged BEFORE guard. Held
                        // callbacks use the identical lock order, never the inverse.
                        if !crate::platform::acknowledge_idle_sleep() {
                            // Unlike a fact produced by Lua, this request must also
                            // synchronize Lua's fact table BEFORE a resulting held
                            // event, even while its VM is blocked in native PAM.
                            let _ = render.send(ToRender::ExternalFact("engaged", "true".into()));
                            let _ = render.send(ToRender::ExternalSignal("lock_request", None));
                        }
                    } else {
                        match acquire(&thread_connection) {
                            Ok(fd) => {
                                if let Ok(mut delay) = thread_delay.lock() {
                                    delay.resumed(fd);
                                }
                                let _ = logic.send(Event::Signal("system_resumed", None));
                            }
                            Err(error) => {
                                if let Ok(mut delay) = thread_delay.lock() {
                                    delay.resume_failed();
                                }
                                let _ = render.send(ToRender::Text(
                                    "status",
                                    format!("Sleep delay could not be reacquired: {error:#}"),
                                ));
                                // Resume still cancels stale sleep requests; absence
                                // of a new FD must not masquerade as protected state.
                                let _ = logic.send(Event::Signal("system_resumed", None));
                            }
                        }
                    }
                }
            })
            .context("watch locker sleep/resume")?;
        Ok(Self {
            connection: Some(connection),
            thread: Some(thread),
            _delay: delay,
            stopped,
        })
    }
}

impl Guard {
    pub(super) fn stop(&mut self) -> Result<()> {
        self.stopped.store(true, Ordering::Release);
        let mut failures = Vec::new();
        if let Some(connection) = self.connection.take() {
            if let Err(error) = connection.close() {
                failures.push(format!("close locker delay bus: {error}"));
            }
        }
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                failures.push("locker delay watcher panicked".into());
            }
        }
        // Only clear this guard's callback registration, never a newer owner.
        if let Some(active) = ACTIVE.get() {
            let mut current = match active.lock() {
                Ok(current) => current,
                Err(poisoned) => {
                    failures.push("active delay mutex poisoned; recovered registration".into());
                    poisoned.into_inner()
                }
            };
            if current
                .upgrade()
                .is_some_and(|delay| Arc::ptr_eq(&delay, &self._delay))
            {
                *current = Weak::new();
            }
        }
        match self._delay.lock() {
            Ok(mut delay) => delay.close(),
            Err(poisoned) => {
                poisoned.into_inner().close();
                failures.push("delay FD mutex poisoned; recovered descriptor".into());
            }
        }
        anyhow::ensure!(failures.is_empty(), "{}", failures.join("; "));
        Ok(())
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
