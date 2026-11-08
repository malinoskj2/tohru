//! Root observer for external logind idle inhibitors and system resume.
//! Caffeine owns an independent logind idle-block FD; it cannot delay sleep.
use super::observer::Observer;
use anyhow::{Context, Result};
use std::os::fd::OwnedFd;
use std::sync::{Arc, Mutex, Weak};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use zbus::blocking::{Connection, MessageIterator, connection::Builder};
const LOGIND: &str = "org.freedesktop.login1";
const PATH: &str = "/org/freedesktop/login1";
const MANAGER: &str = "org.freedesktop.login1.Manager";

pub(super) struct Watch {
    connection: Option<Connection>,
    thread: Option<JoinHandle<()>>,
    caffeine: Mutex<Option<OwnedFd>>,
    observer: Weak<Observer>,
}
fn blocked(connection: &Connection) -> Result<bool> {
    let reply = connection.call_method(
        Some(LOGIND),
        PATH,
        Some("org.freedesktop.DBus.Properties"),
        "Get",
        &(MANAGER, "BlockInhibited"),
    )?;
    let value: zbus::zvariant::OwnedValue = reply.body().deserialize()?;
    let value = String::try_from(value).context("read logind BlockInhibited")?;
    Ok(value.split(':').any(|kind| kind == "idle"))
}
impl Watch {
    pub(super) fn start(observer: &Arc<Observer>) -> Result<Self> {
        let connection = Builder::system()?
            .method_timeout(Duration::from_secs(5))
            .max_queued(32)
            .build()?;
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(LOGIND)?
            .path(PATH)?
            .build();
        let changes = MessageIterator::for_match_rule(rule, &connection, Some(32))?;
        observer.observe(|state| state.inhibitors.logind_idle = true)?;
        let initial = blocked(&connection).context("query external logind idle inhibition")?;
        observer.observe(|state| state.inhibitors.logind_idle = initial)?;
        let weak = Arc::downgrade(observer);
        let (thread_weak, thread_connection) = (weak.clone(), connection.clone());
        let thread = thread::Builder::new()
            .name("tohru-idle-logind".into())
            .spawn(move || {
                for message in changes {
                    let Ok(message) = message else {
                        break;
                    };
                    let header = message.header();
                    let member = header.member().map(|member| member.as_str());
                    let Some(observer) = thread_weak.upgrade() else {
                        break;
                    };
                    if member == Some("PrepareForSleep")
                        && header
                            .interface()
                            .is_some_and(|interface| interface.as_str() == MANAGER)
                    {
                        if let Ok((false,)) = message.body().deserialize::<(bool,)>() {
                            let _ = observer
                                .observe(|state| state.resume = state.resume.wrapping_add(1));
                        }
                    } else if member == Some("PropertiesChanged")
                        && header.interface().is_some_and(|interface| {
                            interface.as_str() == "org.freedesktop.DBus.Properties"
                        })
                    {
                        let inhibited = blocked(&thread_connection).unwrap_or(true);
                        let _ = observer.observe(|state| state.inhibitors.logind_idle = inhibited);
                    }
                }
                if let Some(observer) = thread_weak.upgrade() {
                    // Unknown is conservatively inhibited, never fabricated idle.
                    let _ = observer.observe(|state| state.inhibitors.logind_idle = true);
                }
            })
            .context("watch native logind inhibitors/resume")?;
        Ok(Self {
            connection: Some(connection),
            thread: Some(thread),
            caffeine: Mutex::new(None),
            observer: weak,
        })
    }

    pub(super) fn caffeine(&self, enabled: bool) -> Result<()> {
        let mut held = self
            .caffeine
            .lock()
            .map_err(|_| anyhow::anyhow!("caffeine FD owner mutex poisoned"))?;
        if enabled && held.is_none() {
            let connection = self
                .connection
                .as_ref()
                .context("logind observer stopped")?;
            let reply = connection.call_method(
                Some(LOGIND),
                PATH,
                Some(MANAGER),
                "Inhibit",
                &("idle", "tohru", "Caffeine", "block"),
            )?;
            let fd: zbus::zvariant::OwnedFd = reply.body().deserialize()?;
            let fd = OwnedFd::from(fd);
            *held = Some(
                fd.try_clone()
                    .context("make caffeine descriptor close-on-exec")?,
            );
        } else if !enabled {
            held.take();
        }
        if let Some(observer) = self.observer.upgrade() {
            observer.observe(|state| state.inhibitors.caffeine = enabled)?;
        }
        Ok(())
    }
}
impl Drop for Watch {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            let _ = connection.close();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        // OwnedFd closes when Watch is dropped; no child inherits it.
    }
}
