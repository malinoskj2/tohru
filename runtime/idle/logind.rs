//! Root observer for external logind idle inhibitors and system resume.
//! Caffeine owns an independent logind idle-block FD; it cannot delay sleep.
use super::observer::Observer;
use anyhow::{Context, Result};
use std::os::fd::OwnedFd;
use std::sync::{Arc, Mutex, Weak};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tohru_shell::idle::logind_state::{Owner, owner_signal};
use zbus::blocking::{Connection, MessageIterator, connection::Builder};
const LOGIND: &str = "org.freedesktop.login1";
const PATH: &str = "/org/freedesktop/login1";
const MANAGER: &str = "org.freedesktop.login1.Manager";

pub(super) struct Watch {
    connection: Option<Connection>,
    threads: Vec<JoinHandle<()>>,
    caffeine: Mutex<Option<OwnedFd>>,
    observer: Weak<Observer>,
}

fn blocked(connection: &Connection, owner: &str) -> Result<bool> {
    let reply = connection.call_method(
        Some(owner),
        PATH,
        Some("org.freedesktop.DBus.Properties"),
        "Get",
        &(MANAGER, "BlockInhibited"),
    )?;
    let value: zbus::zvariant::OwnedValue = reply.body().deserialize()?;
    let value = String::try_from(value).context("read logind BlockInhibited")?;
    Ok(value.split(':').any(|kind| kind == "idle"))
}

fn refresh(connection: &Connection, tracker: &Mutex<Owner>, observer: &Observer) {
    let query = tracker.lock().ok().and_then(|mut tracker| {
        let query = tracker.query();
        let _ = observer.observe(|state| state.inhibitors.logind_idle = true);
        query
    });
    let Some(query) = query else {
        return;
    };
    let result = blocked(connection, &query.owner).ok();
    if let Ok(tracker) = tracker.lock() {
        if let Some(blocked) = tracker.complete(&query, result) {
            let _ = observer.observe(|state| state.inhibitors.logind_idle = blocked);
        }
    }
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
        let owner_rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender("org.freedesktop.DBus")?
            .interface("org.freedesktop.DBus")?
            .member("NameOwnerChanged")?
            .path("/org/freedesktop/DBus")?
            .arg(0, LOGIND)?
            .build();
        let owners = MessageIterator::for_match_rule(owner_rule, &connection, Some(32))?;
        let tracker = Arc::new(Mutex::new(Owner::default()));
        let initial_revision = tracker
            .lock()
            .map_err(|_| anyhow::anyhow!("logind owner mutex poisoned"))?
            .revision();
        observer.observe(|state| state.inhibitors.logind_idle = true)?;
        let mut watch = Self {
            connection: Some(connection.clone()),
            threads: Vec::with_capacity(2),
            caffeine: Mutex::new(None),
            observer: Arc::downgrade(observer),
        };
        let (weak, owner_connection, owner_tracker) = (
            Arc::downgrade(observer),
            connection.clone(),
            tracker.clone(),
        );
        watch.threads.push(
            thread::Builder::new()
                .name("tohru-logind-owner".into())
                .spawn(move || {
                    for message in owners {
                        let Ok(message) = message else {
                            break;
                        };
                        let header = message.header();
                        let Ok((name, _, new)) =
                            message.body().deserialize::<(String, String, String)>()
                        else {
                            continue;
                        };
                        if !owner_signal(
                            header.sender().map_or("", |v| v.as_str()),
                            header.interface().map_or("", |v| v.as_str()),
                            header.member().map_or("", |v| v.as_str()),
                            header.path().map_or("", |v| v.as_str()),
                            &name,
                        ) {
                            continue;
                        }
                        let Some(observer) = weak.upgrade() else {
                            break;
                        };
                        if let Ok(mut tracker) = owner_tracker.lock() {
                            tracker.changed(&new);
                            let _ = observer.observe(|state| state.inhibitors.logind_idle = true);
                        }
                        refresh(&owner_connection, &owner_tracker, &observer);
                    }
                    if let Some(observer) = weak.upgrade() {
                        if let Ok(mut tracker) = owner_tracker.lock() {
                            tracker.changed("");
                            let _ = observer.observe(|state| state.inhibitors.logind_idle = true);
                        }
                    }
                })
                .context("watch logind service owner")?,
        );
        // Both subscriptions are active before querying or publishing initial
        // state. An intervening owner event supersedes this initial result.
        let initial = connection
            .call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "GetNameOwner",
                &(LOGIND,),
            )
            .ok()
            .and_then(|message| message.body().deserialize::<String>().ok());
        tracker
            .lock()
            .map_err(|_| anyhow::anyhow!("logind owner mutex poisoned"))?
            .initialize(initial_revision, initial.as_deref());
        refresh(&connection, &tracker, observer);
        let (weak, thread_connection) = (Arc::downgrade(observer), connection.clone());
        watch.threads.push(
            thread::Builder::new()
                .name("tohru-idle-logind".into())
                .spawn(move || {
                    for message in changes {
                        let Ok(message) = message else {
                            break;
                        };
                        let header = message.header();
                        let member = header.member().map(|member| member.as_str());
                        let Some(observer) = weak.upgrade() else {
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
                            refresh(&thread_connection, &tracker, &observer);
                        }
                    }
                    if let Some(observer) = weak.upgrade() {
                        let _ = observer.observe(|state| state.inhibitors.logind_idle = true);
                    }
                })
                .context("watch native logind inhibitors/resume")?,
        );
        Ok(watch)
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
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
        // OwnedFd closes when Watch is dropped; no child inherits it.
    }
}
