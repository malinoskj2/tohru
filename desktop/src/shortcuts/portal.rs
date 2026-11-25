use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde_json::{Value, json};
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type as MessageType;
use zbus::zvariant::{self, OwnedObjectPath, OwnedValue};

pub const PORTAL: &str = "org.freedesktop.portal.Desktop";
pub const PATH: &str = "/org/freedesktop/portal/desktop";
pub const GLOBAL: &str = "org.freedesktop.portal.GlobalShortcuts";
pub const REQUEST: &str = "org.freedesktop.portal.Request";
pub const SESSION: &str = "org.freedesktop.portal.Session";
pub const BUS_NAME: &str = "dev.tohru.Shortcuts";
pub const MATCH_RULES: [&str; 2] = [
    "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='org.freedesktop.portal.Desktop'",
    "type='signal',sender='org.freedesktop.portal.Desktop'",
];
const NOTICE: &str = "Preferred triggers are requests; displayed triggers are backend-confirmed. Compositor bindings remain separately owned.";

/// The fixed actions a portal grant may dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Action {
    Launcher,
    Session,
    Assistant,
}

impl Action {
    pub const ALL: [Self; 3] = [Self::Launcher, Self::Session, Self::Assistant];

    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Launcher => "launcher",
            Self::Session => "session",
            Self::Assistant => "assistant",
        }
    }

    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Self::Launcher => "Application launcher",
            Self::Session => "Session menu",
            Self::Assistant => "Assistant",
        }
    }

    #[must_use]
    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.id() == id)
    }
}

/// What the owner last observed of its portal session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalState {
    Connecting,
    PortalUnavailable,
    NotConfigured,
    Granted,
    Partial,
    Revoked,
    PreviousGrants,
    RequestPending,
    RequestTimeout,
    Cancelled,
    Denied,
    ActionFailed,
    ConfigurationRequested,
    Disconnected,
}

impl PortalState {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Connecting => "connecting",
            Self::PortalUnavailable => "portal-unavailable",
            Self::NotConfigured => "not-configured",
            Self::Granted => "granted",
            Self::Partial => "partial",
            Self::Revoked => "revoked",
            Self::PreviousGrants => "previous-grants; setup required",
            Self::RequestPending => "request-pending",
            Self::RequestTimeout => "request-timeout",
            Self::Cancelled => "cancelled",
            Self::Denied => "denied",
            Self::ActionFailed => "action-failed",
            Self::ConfigurationRequested => "configuration-requested",
            Self::Disconnected => "disconnected",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PortalError {
    #[error("portal call unavailable")]
    Call(#[source] zbus::Error),
    #[error("portal session ended")]
    Ended,
    #[error("unsupported portal version")]
    Version,
    #[error("unexpected portal request path")]
    RequestPath,
    #[error("unexpected portal session path")]
    SessionPath,
    #[error("obsolete portal request")]
    Obsolete,
    #[error("malformed portal response")]
    Malformed,
    #[error("portal request timed out")]
    Timeout,
    #[error("configuration unavailable; portal v2 and existing grants required")]
    ConfigureUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Binding {
    shortcut: String,
    previous: bool,
}

type Response = (u32, HashMap<String, OwnedValue>);

#[derive(Debug)]
struct State {
    portal_owner: String,
    generation: u64,
    version: u32,
    session: String,
    phase: PortalState,
    bindings: BTreeMap<Action, Binding>,
    pressed: BTreeSet<Action>,
    requests: HashMap<String, Sender<Result<Response, PortalError>>>,
    pending: bool,
    registered: bool,
    bound_once: bool,
}

pub type Runner = Box<dyn Fn(Action) -> bool + Send + Sync>;

/// The application-scoped `GlobalShortcuts` portal session and its backend-confirmed grants.
///
/// Requests never run on the compositor-managed path. Preferred triggers are hints; only
/// backend-confirmed registrations are published or allowed to dispatch.
pub struct PortalOwner {
    connection: Connection,
    state: Mutex<State>,
    preferred: BTreeMap<Action, String>,
    runner: Runner,
    request_timeout: Duration,
    running_actions: AtomicUsize,
}

fn token() -> String {
    let bytes = uuid::Uuid::new_v4().into_bytes();
    format!("tohru{}", crate::paths::hex(&bytes[..8]))
}

fn text(value: &OwnedValue) -> Option<String> {
    match &**value {
        zvariant::Value::Str(text) => Some(text.as_str().to_owned()),
        zvariant::Value::ObjectPath(path) => Some(path.as_str().to_owned()),
        _ => None,
    }
}

type Records = Vec<(String, HashMap<String, OwnedValue>)>;

fn records(value: Option<&OwnedValue>) -> Records {
    value
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| Records::try_from(value).ok())
        .unwrap_or_default()
}

impl PortalOwner {
    #[must_use]
    pub fn new(
        connection: Connection,
        preferred: BTreeMap<Action, String>,
        runner: Runner,
        request_timeout: Duration,
    ) -> Arc<Self> {
        Arc::new(Self {
            connection,
            state: Mutex::new(State {
                portal_owner: String::new(),
                generation: 0,
                version: 0,
                session: String::new(),
                phase: PortalState::Connecting,
                bindings: BTreeMap::new(),
                pressed: BTreeSet::new(),
                requests: HashMap::new(),
                pending: false,
                registered: false,
                bound_once: false,
            }),
            preferred,
            runner,
            request_timeout,
            running_actions: AtomicUsize::new(0),
        })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn sender_path(&self) -> String {
        self.connection
            .unique_name()
            .map(|name| name.as_str().trim_start_matches(':').replace('.', "_"))
            .unwrap_or_default()
    }

    fn call<B>(
        &self,
        interface: &str,
        member: &str,
        body: &B,
        path: &str,
        destination: Option<&str>,
    ) -> Result<zbus::Message, PortalError>
    where
        B: serde::Serialize + zvariant::DynamicType,
    {
        let owner = self.lock().portal_owner.clone();
        let destination = destination.map_or_else(
            || {
                if owner.is_empty() {
                    PORTAL.to_owned()
                } else {
                    owner
                }
            },
            str::to_owned,
        );
        self.connection
            .call_method(
                Some(destination.as_str()),
                path,
                Some(interface),
                member,
                body,
            )
            .map_err(PortalError::Call)
    }

    /// Subscribes to the portal's signals and its owner changes.
    pub fn subscribe(&self) -> Result<(), PortalError> {
        for rule in MATCH_RULES {
            self.call(
                "org.freedesktop.DBus",
                "AddMatch",
                &(rule,),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
            )?;
        }
        Ok(())
    }

    #[must_use]
    pub fn state(&self) -> PortalState {
        self.lock().phase
    }

    #[must_use]
    pub fn session(&self) -> String {
        self.lock().session.clone()
    }

    #[must_use]
    pub fn pending(&self) -> bool {
        self.lock().pending
    }

    #[must_use]
    pub fn version(&self) -> u32 {
        self.lock().version
    }

    #[must_use]
    pub fn registered(&self) -> bool {
        self.lock().registered
    }

    #[must_use]
    pub fn open_requests(&self) -> usize {
        self.lock().requests.len()
    }

    #[must_use]
    pub fn bound_actions(&self) -> Vec<&'static str> {
        self.lock()
            .bindings
            .keys()
            .map(|action| action.id())
            .collect()
    }

    fn invalidate(state: &mut State, next: PortalState) {
        state.generation += 1;
        state.session.clear();
        state.bindings.clear();
        state.registered = false;
        state.pressed.clear();
        state.phase = next;
        for (_, waiter) in state.requests.drain() {
            let _ = waiter.send(Err(PortalError::Ended));
        }
    }

    fn update_bindings(state: &mut State, records: &Records, changed: bool) {
        let mut granted = BTreeMap::new();
        for (action, properties) in records.iter().take(32) {
            let Some(action) = Action::parse(action) else {
                continue;
            };
            let Some(trigger) = properties.get("trigger_description").and_then(text) else {
                continue;
            };
            if trigger.is_empty() || trigger.chars().count() > 512 {
                continue;
            }
            granted.insert(
                action,
                Binding {
                    shortcut: trigger,
                    previous: false,
                },
            );
        }
        let previous = !state.bindings.is_empty();
        state.pressed.retain(|action| granted.contains_key(action));
        state.bindings = granted;
        if !state.bindings.is_empty() && !state.registered {
            state.phase = PortalState::PreviousGrants;
            for binding in state.bindings.values_mut() {
                binding.previous = true;
            }
            return;
        }
        state.phase = if state.bindings.len() == Action::ALL.len() {
            PortalState::Granted
        } else if !state.bindings.is_empty() {
            PortalState::Partial
        } else if changed && previous {
            PortalState::Revoked
        } else {
            PortalState::NotConfigured
        };
    }

    /// Handles one incoming message; signals from anyone but the current portal owner are ignored.
    pub fn handle(self: &Arc<Self>, message: &zbus::Message) {
        if message.message_type() != MessageType::Signal {
            return;
        }
        let header = message.header();
        let sender = header
            .sender()
            .map(|name| name.as_str().to_owned())
            .unwrap_or_default();
        let interface = header
            .interface()
            .map(|name| name.as_str().to_owned())
            .unwrap_or_default();
        let member = header
            .member()
            .map(|name| name.as_str().to_owned())
            .unwrap_or_default();
        let path = header
            .path()
            .map(|path| path.as_str().to_owned())
            .unwrap_or_default();
        let mut state = self.lock();
        if sender == "org.freedesktop.DBus"
            && interface == "org.freedesktop.DBus"
            && member == "NameOwnerChanged"
        {
            if let Ok((name, _, owner)) = message.body().deserialize::<(String, String, String)>()
                && name == PORTAL
            {
                let next = if owner.is_empty() {
                    PortalState::PortalUnavailable
                } else {
                    PortalState::Connecting
                };
                state.portal_owner = owner;
                state.version = 0;
                Self::invalidate(&mut state, next);
            }
            return;
        }
        if state.portal_owner.is_empty() || sender != state.portal_owner {
            return;
        }
        if interface == REQUEST && member == "Response" {
            if let Some(waiter) = state.requests.get(&path)
                && let Ok(response) = message.body().deserialize::<Response>()
            {
                let _ = waiter.send(Ok(response));
            }
            return;
        }
        if interface == SESSION
            && member == "Closed"
            && path == state.session
            && !state.session.is_empty()
        {
            Self::invalidate(&mut state, PortalState::Revoked);
            return;
        }
        if interface != GLOBAL || path != PATH || state.session.is_empty() {
            return;
        }
        if member == "ShortcutsChanged" {
            if let Ok((session, records)) =
                message.body().deserialize::<(OwnedObjectPath, Records)>()
                && session.as_str() == state.session
            {
                Self::update_bindings(&mut state, &records, true);
            }
            return;
        }
        let Ok((session, action, _, _)) =
            message
                .body()
                .deserialize::<(OwnedObjectPath, String, u64, HashMap<String, OwnedValue>)>()
        else {
            return;
        };
        if session.as_str() != state.session || !state.registered {
            return;
        }
        let Some(action) =
            Action::parse(&action).filter(|action| state.bindings.contains_key(action))
        else {
            return;
        };
        if member == "Activated"
            && !state.pressed.contains(&action)
            && self.running_actions.load(Ordering::SeqCst) < 4
        {
            state.pressed.insert(action);
            drop(state);
            self.dispatch(action);
        } else if member == "Deactivated" {
            state.pressed.remove(&action);
        }
    }

    fn dispatch(self: &Arc<Self>, action: Action) {
        self.running_actions.fetch_add(1, Ordering::SeqCst);
        let owner = Arc::clone(self);
        std::thread::spawn(move || {
            if !(owner.runner)(action) {
                owner.lock().phase = PortalState::ActionFailed;
            }
            owner.running_actions.fetch_sub(1, Ordering::SeqCst);
        });
    }

    /// One portal request: its `Response` signal arrives on a path derived from our token.
    fn request<B>(
        &self,
        member: &str,
        body: impl FnOnce(&str) -> B,
    ) -> Result<Option<HashMap<String, OwnedValue>>, PortalError>
    where
        B: serde::Serialize + zvariant::DynamicType,
    {
        let token = token();
        let path = format!("{PATH}/request/{}/{token}", self.sender_path());
        let (waiter, response): (Sender<_>, Receiver<_>) = mpsc::channel();
        let generation = {
            let mut state = self.lock();
            state.requests.insert(path.clone(), waiter);
            state.generation
        };
        let result = self.await_request(member, &body(&token), &path, &response, generation);
        self.lock().requests.remove(&path);
        if matches!(
            result,
            Err(PortalError::Timeout
                | PortalError::Ended
                | PortalError::Call(_)
                | PortalError::RequestPath)
        ) {
            let _ = self.call(REQUEST, "Close", &(), &path, None);
        }
        result
    }

    fn await_request<B>(
        &self,
        member: &str,
        body: &B,
        path: &str,
        response: &Receiver<Result<Response, PortalError>>,
        generation: u64,
    ) -> Result<Option<HashMap<String, OwnedValue>>, PortalError>
    where
        B: serde::Serialize + zvariant::DynamicType,
    {
        let reply = self.call(GLOBAL, member, body, PATH, None)?;
        let handle: OwnedObjectPath = reply
            .body()
            .deserialize()
            .map_err(|_| PortalError::RequestPath)?;
        if handle.as_str() != path {
            return Err(PortalError::RequestPath);
        }
        let (code, results) = match response.recv_timeout(self.request_timeout) {
            Ok(answer) => answer?,
            Err(_) => return Err(PortalError::Timeout),
        };
        let mut state = self.lock();
        if generation != state.generation {
            return Err(PortalError::Obsolete);
        }
        if code != 0 {
            state.phase = if code == 1 {
                PortalState::Cancelled
            } else {
                PortalState::Denied
            };
            return Ok(None);
        }
        Ok(Some(results))
    }

    /// Opens a session and lists previous grants; false when the portal declined.
    pub fn open_session(&self) -> Result<bool, PortalError> {
        let generation = self.lock().generation;
        let reply = self.call(
            "org.freedesktop.DBus.Properties",
            "Get",
            &(GLOBAL, "version"),
            PATH,
            Some(PORTAL),
        )?;
        let version = reply
            .body()
            .deserialize::<OwnedValue>()
            .ok()
            .and_then(|value| u32::try_from(&value).ok())
            .unwrap_or(0);
        {
            let mut state = self.lock();
            if generation != state.generation || version < 1 {
                return Err(PortalError::Version);
            }
            state.version = version;
            state.portal_owner = reply
                .header()
                .sender()
                .map(|name| name.as_str().to_owned())
                .unwrap_or_default();
        }
        let session_token = token();
        let result = self.request("CreateSession", |token| {
            let mut options: HashMap<&str, zvariant::Value<'_>> = HashMap::new();
            options.insert("handle_token", token.to_owned().into());
            options.insert("session_handle_token", session_token.clone().into());
            (options,)
        })?;
        let Some(result) = result else {
            return Ok(false);
        };
        let session = result.get("session_handle").and_then(text);
        let expected = format!("{PATH}/session/{}/{session_token}", self.sender_path());
        let Some(session) = session.filter(|session| *session == expected) else {
            return Err(PortalError::SessionPath);
        };
        {
            let mut state = self.lock();
            state.session.clone_from(&session);
            state.bound_once = false;
        }
        let path = OwnedObjectPath::try_from(session).map_err(|_| PortalError::SessionPath)?;
        let listed = self.request("ListShortcuts", |token| {
            let mut options: HashMap<&str, zvariant::Value<'_>> = HashMap::new();
            options.insert("handle_token", token.to_owned().into());
            (path.clone(), options)
        })?;
        if let Some(listed) = listed {
            Self::update_bindings(&mut self.lock(), &records(listed.get("shortcuts")), false);
        }
        Ok(true)
    }

    pub fn close_session(&self) {
        let session = {
            let mut state = self.lock();
            state.registered = false;
            state.bindings.clear();
            state.pressed.clear();
            std::mem::take(&mut state.session)
        };
        if !session.is_empty() {
            let _ = self.call(SESSION, "Close", &(), &session, None);
        }
    }

    /// The explicit, owner-requested grant prompt. Never called automatically.
    pub fn setup(&self) {
        {
            let mut state = self.lock();
            if state.pending {
                return;
            }
            state.pending = true;
            state.phase = PortalState::RequestPending;
        }
        let outcome = self.bind();
        let mut state = self.lock();
        match outcome {
            Ok(()) => {}
            Err(PortalError::Timeout) => state.phase = PortalState::RequestTimeout,
            Err(_) => state.phase = PortalState::PortalUnavailable,
        }
        state.pending = false;
    }

    fn bind(&self) -> Result<(), PortalError> {
        let reopen = {
            let state = self.lock();
            state.bound_once || state.session.is_empty()
        };
        if reopen {
            self.close_session();
            if !self.open_session()? {
                return Ok(());
            }
        }
        let session = {
            let mut state = self.lock();
            state.phase = PortalState::RequestPending;
            state.bound_once = true;
            state.session.clone()
        };
        let session = OwnedObjectPath::try_from(session).map_err(|_| PortalError::SessionPath)?;
        let choices: Vec<(&str, HashMap<&str, zvariant::Value<'_>>)> = Action::ALL
            .iter()
            .map(|action| {
                let mut properties: HashMap<&str, zvariant::Value<'_>> = HashMap::new();
                properties.insert("description", action.description().into());
                if let Some(preferred) = self
                    .preferred
                    .get(action)
                    .filter(|trigger| !trigger.is_empty())
                {
                    properties.insert("preferred_trigger", preferred.as_str().into());
                }
                (action.id(), properties)
            })
            .collect();
        let result = self.request("BindShortcuts", |token| {
            let mut options: HashMap<&str, zvariant::Value<'_>> = HashMap::new();
            options.insert("handle_token", token.to_owned().into());
            (session.clone(), choices.clone(), "", options)
        })?;
        let mut state = self.lock();
        if let Some(result) = result {
            state.registered = true;
            Self::update_bindings(&mut state, &records(result.get("shortcuts")), false);
            if state.bindings.is_empty() {
                state.phase = PortalState::Denied;
            }
        } else {
            state.registered = false;
            state.bindings.clear();
        }
        Ok(())
    }

    /// Opens the portal's own editor for the existing grants (portal v2).
    pub fn configure(&self) -> Result<(), PortalError> {
        let session = {
            let state = self.lock();
            if state.version < 2
                || state.session.is_empty()
                || state.bindings.is_empty()
                || !state.registered
            {
                return Err(PortalError::ConfigureUnavailable);
            }
            state.session.clone()
        };
        let session = OwnedObjectPath::try_from(session).map_err(|_| PortalError::SessionPath)?;
        let options: HashMap<&str, zvariant::Value<'_>> = HashMap::new();
        self.call(
            GLOBAL,
            "ConfigureShortcuts",
            &(session, "", options),
            PATH,
            None,
        )?;
        self.lock().phase = PortalState::ConfigurationRequested;
        Ok(())
    }

    /// Reconnects after a portal start or restart without prompting.
    pub fn connect_portal(&self) {
        self.lock().pending = true;
        let outcome = self.open_session();
        let mut state = self.lock();
        match outcome {
            Ok(_) => {}
            Err(PortalError::Timeout) => state.phase = PortalState::RequestTimeout,
            Err(_) => state.phase = PortalState::PortalUnavailable,
        }
        state.pending = false;
    }

    pub fn mark_disconnected(&self) {
        Self::invalidate(&mut self.lock(), PortalState::Disconnected);
    }

    #[must_use]
    pub fn snapshot(&self) -> Value {
        let state = self.lock();
        let bindings: Vec<Value> = state
            .bindings
            .iter()
            .map(|(action, binding)| {
                let flags = if binding.previous { "previous grant; not registered in current session" } else { "portal grant" };
                json!({"action": action.id(), "shortcut": binding.shortcut, "description": action.description(),
                       "arguments": "", "submap": "", "conflict": false, "flags": flags})
            })
            .collect();
        let active = state.registered && !state.bindings.is_empty();
        json!({"owner": "Tohru actions via desktop portal", "state": state.phase.name(), "editable": state.version >= 1,
               "portal_version": state.version, "setup_available": state.version >= 1 && !state.pending,
               "configure_available": state.version >= 2 && active && !state.pending,
               "active_registration": active, "bindings": bindings, "registration": "portal-session", "notice": NOTICE})
    }

    pub fn close(&self) {
        let paths: Vec<String> = self.lock().requests.drain().map(|(path, _)| path).collect();
        for path in paths {
            let _ = self.call(REQUEST, "Close", &(), &path, None);
        }
        self.close_session();
    }

    /// Feeds every incoming message to [`Self::handle`] until the bus connection closes.
    pub fn listen(self: &Arc<Self>) -> std::thread::JoinHandle<()> {
        let owner = Arc::clone(self);
        let messages = MessageIterator::from(&self.connection);
        std::thread::spawn(move || {
            for message in messages {
                match message {
                    Ok(message) => owner.handle(&message),
                    Err(_) => break,
                }
            }
        })
    }
}

/// Runs one allow-listed action through the `tohru` CLI beside this executable.
#[must_use]
pub fn tohru_runner() -> Runner {
    let tohru: PathBuf = crate::paths::bin_dir().join("tohru");
    Box::new(move |action| {
        crate::probe::probe(&tohru, &[action.id(), "toggle"], Duration::from_secs(3)).is_ok()
    })
}

fn answer(owner: &Arc<PortalOwner>, stream: &UnixStream) -> Value {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut line = String::new();
    let unavailable = json!({"error": "shortcut operation unavailable; inspect portal state"});
    if BufReader::new(std::io::Read::take(stream, 8192))
        .read_line(&mut line)
        .is_err()
    {
        return unavailable;
    }
    match line.trim() {
        "setup" => {
            if !owner.pending() && owner.version() >= 1 {
                let worker = Arc::clone(owner);
                std::thread::spawn(move || worker.setup());
            }
        }
        "configure" => {
            if owner.configure().is_err() {
                return unavailable;
            }
        }
        "status" => {}
        _ => return unavailable,
    }
    owner.snapshot()
}

/// Serves the private control socket with at most eight concurrent clients.
pub fn serve_control(
    owner: &Arc<PortalOwner>,
    listener: &UnixListener,
    stopping: &Arc<AtomicBool>,
) {
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        if stopping.load(Ordering::SeqCst) {
            break;
        }
        let Ok(mut stream) = stream else { continue };
        if active.load(Ordering::SeqCst) >= 8 {
            continue;
        }
        active.fetch_add(1, Ordering::SeqCst);
        let (owner, active) = (Arc::clone(owner), Arc::clone(&active));
        std::thread::spawn(move || {
            let result = answer(&owner, &stream);
            let _ = stream.write_all(format!("{}\n", crate::pyjson::dumps(&result)).as_bytes());
            active.fetch_sub(1, Ordering::SeqCst);
        });
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error(transparent)]
    Session(#[from] crate::paths::SessionError),
    #[error("cannot prepare the shortcut owner's runtime files")]
    Runtime(#[source] std::io::Error),
    #[error("another Tohru portal owner is running")]
    Duplicate,
    #[error("session bus unavailable")]
    Bus(#[source] zbus::Error),
    #[error(transparent)]
    Portal(#[from] PortalError),
    #[error("session bus disconnected")]
    Disconnected,
}

/// The `tohru-shortcuts` service: owns one portal session for this Wayland session.
pub fn serve(preferred: BTreeMap<Action, String>) -> Result<(), ServeError> {
    let directory = crate::paths::session_directory()?;
    std::fs::create_dir_all(&directory).map_err(ServeError::Runtime)?;
    let lock =
        std::fs::File::create(directory.join("shortcuts.lock")).map_err(ServeError::Runtime)?;
    match rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => {}
        Err(rustix::io::Errno::WOULDBLOCK) => return Err(ServeError::Duplicate),
        Err(error) => return Err(ServeError::Runtime(error.into())),
    }
    let connection = zbus::blocking::connection::Builder::session()
        .and_then(|builder| builder.method_timeout(Duration::from_secs(3)).build())
        .map_err(ServeError::Bus)?;
    match connection
        .request_name_with_flags(BUS_NAME, zbus::fdo::RequestNameFlags::DoNotQueue.into())
    {
        Ok(zbus::fdo::RequestNameReply::PrimaryOwner) => {}
        Ok(_) | Err(zbus::Error::NameTaken) => return Err(ServeError::Duplicate),
        Err(error) => return Err(ServeError::Bus(error)),
    }
    let owner = PortalOwner::new(
        connection,
        preferred,
        tohru_runner(),
        Duration::from_secs(90),
    );
    let listener_thread = owner.listen();
    owner.subscribe()?;
    let endpoint = directory.join(super::SOCKET);
    let _ = std::fs::remove_file(&endpoint);
    let listener = UnixListener::bind(&endpoint).map_err(ServeError::Runtime)?;
    std::fs::set_permissions(&endpoint, std::fs::Permissions::from_mode(0o600))
        .map_err(ServeError::Runtime)?;
    let stopping = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        signal_hook::flag::register(signal, Arc::clone(&stopping)).map_err(ServeError::Runtime)?;
    }
    {
        let (owner, stopping) = (Arc::clone(&owner), Arc::clone(&stopping));
        std::thread::spawn(move || serve_control(&owner, &listener, &stopping));
    }
    let mut next_check = std::time::Instant::now();
    let outcome = loop {
        if stopping.load(Ordering::SeqCst) {
            break Ok(());
        }
        if listener_thread.is_finished() {
            owner.mark_disconnected();
            break Err(ServeError::Disconnected);
        }
        if std::time::Instant::now() >= next_check {
            if !owner.pending()
                && matches!(
                    owner.state(),
                    PortalState::Connecting | PortalState::PortalUnavailable
                )
            {
                owner.connect_portal();
            }
            next_check = std::time::Instant::now() + Duration::from_secs(3);
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let _ = std::fs::remove_file(&endpoint);
    owner.close();
    outcome
}
