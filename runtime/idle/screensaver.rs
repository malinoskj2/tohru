//! Standard per-unique-bus-client inhibition. Never replaces an existing owner.
use super::observer::Observer;
use anyhow::{Context, Result};
use std::sync::{Arc, Mutex, Weak};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tohru_shell::idle::cookies::Cookies;
use zbus::{
    blocking::{Connection, MessageIterator, connection::Builder},
    fdo,
    message::Header,
};

struct Shared {
    cookies: Mutex<Cookies>,
    observer: Weak<Observer>,
}

impl Shared {
    fn change<T>(&self, change: impl FnOnce(&mut Cookies) -> fdo::Result<T>) -> fdo::Result<T> {
        let mut cookies = self
            .cookies
            .lock()
            .map_err(|_| fdo::Error::Failed("idle cookie mutex poisoned".into()))?;
        let result = change(&mut cookies);
        if let Some(observer) = self.observer.upgrade() {
            let count = cookies.count() as u32;
            observer
                .observe(|observed| observed.inhibitors.screen_saver_cookies = count)
                .map_err(|error| fdo::Error::Failed(format!("{error:#}")))?;
        }
        result
    }
}

struct ScreenSaver {
    shared: Arc<Shared>,
}

fn sender(header: &Header<'_>) -> fdo::Result<String> {
    header
        .sender()
        .map(|name| name.as_str().to_owned())
        .ok_or_else(|| fdo::Error::AccessDenied("missing bus-authenticated sender".into()))
}

#[zbus::interface(name = "org.freedesktop.ScreenSaver")]
impl ScreenSaver {
    fn inhibit(
        &self,
        _application_name: &str,
        _reason_for_inhibit: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<u32> {
        let owner = sender(&header)?;
        self.shared.change(|cookies| {
            cookies
                .inhibit(&owner)
                .map_err(|error| fdo::Error::LimitsExceeded(error.to_string()))
        })
    }

    fn un_inhibit(&self, cookie: u32, #[zbus(header)] header: Header<'_>) -> fdo::Result<()> {
        let owner = sender(&header)?;
        self.shared.change(|cookies| {
            let _ = cookies.release(&owner, cookie);
            Ok(())
        })
    }
}

pub(super) struct Service {
    connection: Option<Connection>,
    departures: Option<JoinHandle<()>>,
}

impl Service {
    pub(super) fn start(observer: &Arc<Observer>) -> Result<Self> {
        let shared = Arc::new(Shared {
            cookies: Mutex::new(Cookies::default()),
            observer: Arc::downgrade(observer),
        });
        let builder = Builder::session()?
            .method_timeout(Duration::from_secs(5))
            .max_queued(32);
        let connection = builder
            .serve_at(
                "/org/freedesktop/ScreenSaver",
                ScreenSaver {
                    shared: shared.clone(),
                },
            )?
            .serve_at(
                "/ScreenSaver",
                ScreenSaver {
                    shared: shared.clone(),
                },
            )?
            .build()
            .context("serve ScreenSaver inhibition")?;
        // Subscribe BEFORE publishing name; a fast client may inhibit then leave.
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender("org.freedesktop.DBus")?
            .interface("org.freedesktop.DBus")?
            .member("NameOwnerChanged")?
            .arg(2, "")?
            .build();
        let changes = MessageIterator::for_match_rule(rule, &connection, Some(32))?;
        let reply = connection.request_name_with_flags(
            "org.freedesktop.ScreenSaver",
            fdo::RequestNameFlags::DoNotQueue.into(),
        )?;
        anyhow::ensure!(
            reply == fdo::RequestNameReply::PrimaryOwner,
            "another process owns ScreenSaver inhibition"
        );
        let departures = thread::Builder::new()
            .name("tohru-idle-clients".into())
            .spawn(move || {
                for message in changes {
                    let Ok(message) = message else {
                        break;
                    };
                    if let Ok((name, _, new)) =
                        message.body().deserialize::<(String, String, String)>()
                    {
                        if name.starts_with(':') && new.is_empty() {
                            let _ = shared.change(|cookies| {
                                cookies.client_gone(&name);
                                Ok(())
                            });
                        }
                    }
                }
            })
            .context("watch idle inhibitor client lifetime")?;
        Ok(Self {
            connection: Some(connection),
            departures: Some(departures),
        })
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            let _ = connection.close();
        }
        if let Some(thread) = self.departures.take() {
            let _ = thread.join();
        }
    }
}
