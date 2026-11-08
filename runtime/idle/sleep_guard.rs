//! Dedicated locker-local logind delay. No remote hint can acknowledge it.
use crate::scene::{Event, ToRender};
use anyhow::{Context, Result};
use std::os::fd::OwnedFd;
use std::sync::{Arc, Mutex, OnceLock, Weak, mpsc::Sender};
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
        let thread_delay = delay.clone();
        let thread_connection = connection.clone();
        let thread = thread::Builder::new()
            .name("tohru-lock-sleep".into())
            .spawn(move || {
                for message in changes {
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
        })
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            let _ = connection.close();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        // No explicit unlock or convenience power action on teardown.
    }
}
