mod serve;

use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::config::{Config, ConfigError, IdleOwner, IdleSource};
use crate::paths::{self, SessionError};
use crate::probe::probe;
use crate::{pyjson, shell};

pub use serve::serve;

pub const PROGRAM: &str = "tohru-afk-idle";
pub const SOCKET: &str = "afk.socket";
pub const REGISTRATION: &str = "afk.json";
pub const EFFECTIVE: &str = "afk-effective.json";
pub const EPISODE: &str = "afk-episode.json";
pub const LOCK: &str = "afk.lock";
pub const ARTWORK: [&str; 3] = [
    "afk.webp",
    "afk-closeup-motion.webp",
    "afk-bust-motion.webp",
];
const LIMIT: i64 = 2_147_483_646;

#[derive(Debug, thiserror::Error)]
pub enum AfkError {
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("AFK runtime directory must be private and user-owned")]
    Directory,
    #[error("invalid AFK registration")]
    Registration,
    #[error("invalid effective AFK configuration")]
    Effective,
    #[error("invalid effective AFK source")]
    Source,
    #[error("invalid AFK episode state")]
    Episode,
    #[error("AFK episode counter exhausted")]
    Exhausted,
    #[error("idle manager must belong to this user")]
    Foreign,
    #[error("invalid process status")]
    ProcessStatus,
    #[error("AFK artwork unavailable")]
    Artwork,
    #[error("AFK inactive: ext_idle_notifier_v1 unavailable or unknown ({0})")]
    Protocol(String),
    #[error("AFK idle observer exited; current idle state is unknown")]
    ObserverExited,
    #[error("AFK idle observer exited; ignoring its shutdown callbacks")]
    ObserverShutdown,
    #[error("AFK service {0}; install the optional generated unit")]
    Service(String),
}

/// A swayidle process named by PID and kernel start time, so a reused PID never matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Manager {
    pub pid: i64,
    pub start: u64,
}

impl Manager {
    /// The process as it is now.
    pub fn current(pid: i64) -> Result<Self, AfkError> {
        Ok(Self {
            pid,
            start: process_start(pid)?,
        })
    }

    #[must_use]
    pub fn alive(self) -> bool {
        process_start(self.pid).is_ok_and(|start| start == self.start)
    }

    fn to_json(self) -> Value {
        json!([self.pid, self.start])
    }
}

/// What a callback claims about the idle manager that ran it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Claim {
    Absent,
    Manager(Manager),
    Invalid,
}

impl Claim {
    fn parse(value: Option<&Value>) -> Self {
        match value {
            None | Some(Value::Null) => Self::Absent,
            Some(Value::Array(items)) => match items.as_slice() {
                [pid, start] => match (pid.as_i64(), start.as_u64()) {
                    (Some(pid), Some(start)) => Self::Manager(Manager { pid, start }),
                    _ => Self::Invalid,
                },
                _ => Self::Invalid,
            },
            Some(_) => Self::Invalid,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Event {
    Idle,
    Resume,
}

impl Event {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Resume => "resume",
        }
    }
}

/// The socket and registration paths in the private per-session directory.
pub fn endpoint() -> Result<(PathBuf, PathBuf), AfkError> {
    let directory = paths::session_directory()?;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)?;
    let info = std::fs::metadata(&directory)?;
    if info.uid() != paths::uid() || info.mode() & 0o077 != 0 {
        return Err(AfkError::Directory);
    }
    Ok((directory.join(SOCKET), directory.join(REGISTRATION)))
}

fn session_files() -> Result<PathBuf, AfkError> {
    let (socket, _) = endpoint()?;
    socket
        .parent()
        .map(Path::to_owned)
        .ok_or(AfkError::Directory)
}

fn small_json(path: &Path, limit: u64, invalid: fn() -> AfkError) -> Result<Value, AfkError> {
    if std::fs::metadata(path)?.len() > limit {
        return Err(invalid());
    }
    serde_json::from_slice(&std::fs::read(path)?).map_err(|_| invalid())
}

fn bounded(value: &Value, low: i64, high: i64) -> Option<i64> {
    value
        .as_i64()
        .filter(|number| (low..=high).contains(number))
}

fn write_atomic(directory: &Path, prefix: &str, name: &str, value: &Value) -> Result<(), AfkError> {
    let mut file = tempfile::Builder::new()
        .prefix(prefix)
        .tempfile_in(directory)?;
    file.write_all(pyjson::dumps(value).as_bytes())?;
    file.persist(directory.join(name))
        .map_err(|error| error.error)?;
    Ok(())
}

/// The serving generation's announcement, written by `serve`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Registration {
    pub generation: i64,
    pub seconds: i64,
    pub observer: Option<Manager>,
}

impl Registration {
    fn to_json(self) -> Value {
        let mut value =
            json!({"schema": 1, "generation": self.generation, "seconds": self.seconds});
        if let Some(observer) = self.observer {
            value["observer_pid"] = json!(observer.pid);
            value["observer_start"] = json!(observer.start);
        }
        value
    }

    fn write(self, path: &Path) -> Result<(), AfkError> {
        std::fs::write(path, pyjson::dumps(&self.to_json()))?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(())
    }
}

pub fn read_registration() -> Result<Registration, AfkError> {
    let (_, path) = endpoint()?;
    let value = small_json(&path, 4096, || AfkError::Registration)?;
    let generation = bounded(&value["generation"], 1, LIMIT);
    let seconds = bounded(&value["seconds"], 1, 86_400);
    let (Some(1), Some(generation), Some(seconds)) =
        (value["schema"].as_i64(), generation, seconds)
    else {
        return Err(AfkError::Registration);
    };
    let observer = match Claim::parse(Some(&json!([
        value["observer_pid"],
        value["observer_start"]
    ]))) {
        Claim::Manager(manager) => Some(manager),
        Claim::Absent | Claim::Invalid => None,
    };
    Ok(Registration {
        generation,
        seconds,
        observer,
    })
}

/// The kernel start time of one of this user's processes.
pub fn process_start(pid: i64) -> Result<u64, AfkError> {
    let path = PathBuf::from(format!("/proc/{pid}"));
    if std::fs::metadata(&path)?.uid() != paths::uid() {
        return Err(AfkError::Foreign);
    }
    let stat = std::fs::read_to_string(path.join("stat"))?;
    stat.rsplit_once(')')
        .and_then(|(_, fields)| fields.split_whitespace().nth(19))
        .and_then(|start| start.parse().ok())
        .ok_or(AfkError::ProcessStatus)
}

/// The shell command an idle manager runs for one event of this generation.
#[must_use]
pub fn callback(generation: i64, event: Event, manager: Option<Manager>) -> String {
    let mut args = vec![
        paths::bin_dir()
            .join(PROGRAM)
            .to_string_lossy()
            .into_owned(),
        "notify".to_owned(),
        event.name().to_owned(),
        "--generation".to_owned(),
        generation.to_string(),
    ];
    if let Some(manager) = manager {
        args.extend(["--manager-pid".to_owned(), manager.pid.to_string()]);
        args.extend(["--manager-start".to_owned(), manager.start.to_string()]);
    }
    shell::join(&args)
}

/// A passive swayidle notification client: deliberately no `-w`, lock, unlock or sleep hooks.
///
/// swayidle delegates input observation and inhibitors to the compositor's idle protocol.
#[must_use]
pub fn sway_arguments(seconds: i64, generation: i64, manager: Option<Manager>) -> Vec<String> {
    vec![
        "swayidle".to_owned(),
        "-C".to_owned(),
        "/dev/null".to_owned(),
        "timeout".to_owned(),
        seconds.to_string(),
        callback(generation, Event::Idle, manager),
        "resume".to_owned(),
        callback(generation, Event::Resume, manager),
    ]
}

/// The AFK source the bar last asked for, overriding the stored settings while it runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effective {
    pub seconds: i64,
    pub source: IdleSource,
    pub owner: IdleOwner,
}

impl Effective {
    fn read(directory: &Path) -> Result<Option<Self>, AfkError> {
        let path = directory.join(EFFECTIVE);
        if !path.exists() {
            return Ok(None);
        }
        let data = small_json(&path, 2048, || AfkError::Effective)?;
        let seconds = bounded(&data["seconds"], 1, 86_400);
        let source = data["source"].as_str().and_then(IdleSource::parse);
        let owner = data["owner"].as_str().and_then(IdleOwner::parse);
        match (data["enabled"].as_bool(), seconds, source, owner) {
            (Some(true), Some(seconds), Some(source), Some(owner)) => Ok(Some(Self {
                seconds,
                source,
                owner,
            })),
            _ => Err(AfkError::Effective),
        }
    }

    fn to_json(self) -> Value {
        json!({"enabled": true, "seconds": self.seconds, "source": self.source.name(), "owner": self.owner.name()})
    }
}

/// The settings with any bar-requested AFK source applied.
pub fn effective_config(config: &Config) -> Result<Config, AfkError> {
    let mut config = config.clone();
    if let Some(effective) = Effective::read(&session_files()?)? {
        config.afk_enabled = true;
        config.afk_idle_seconds =
            u32::try_from(effective.seconds).map_err(|_| AfkError::Effective)?;
        config.afk_idle_source = effective.source;
        config.idle_owner = effective.owner;
    }
    Ok(config)
}

/// Callbacks for Tohru's own swayidle, captured from the generation registered right now.
///
/// A missing or restarted adapter leaves AFK inactive until that owner restarts. Reading a fresh
/// token inside a delayed callback would let an old callback impersonate a newer generation.
pub fn owned_callbacks(config: &Config) -> Result<Vec<String>, AfkError> {
    let config = effective_config(config)?;
    if !config.afk_enabled || config.idle_owner != IdleOwner::Tohru {
        return Ok(Vec::new());
    }
    let Ok(registration) = read_registration() else {
        return Ok(Vec::new());
    };
    if registration.seconds != i64::from(config.afk_idle_seconds) {
        return Ok(Vec::new());
    }
    let manager = Manager::current(i64::from(std::process::id()))?;
    Ok(sway_arguments(registration.seconds, registration.generation, Some(manager)).split_off(3))
}

fn random_below_limit() -> i64 {
    let bytes = uuid::Uuid::new_v4().into_bytes();
    let mut word = [0_u8; 8];
    word.copy_from_slice(&bytes[..8]);
    let value = u64::from_le_bytes(word) % LIMIT.unsigned_abs();
    i64::try_from(value).unwrap_or_default() + 1
}

/// The durable idle-episode counter that keeps one observed idle period to a single roll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Episode {
    pub seed: i64,
    pub episode: i64,
    pub idle: bool,
}

impl Episode {
    pub fn load(directory: &Path) -> Result<Self, AfkError> {
        let path = directory.join(EPISODE);
        if !path.exists() {
            return Ok(Self {
                seed: random_below_limit(),
                episode: 0,
                idle: false,
            });
        }
        let state = small_json(&path, 1024, || AfkError::Episode)?;
        match (
            bounded(&state["seed"], 0, LIMIT),
            bounded(&state["episode"], 0, LIMIT),
            state["idle"].as_bool(),
        ) {
            (Some(seed), Some(episode), Some(idle)) => Ok(Self {
                seed,
                episode,
                idle,
            }),
            _ => Err(AfkError::Episode),
        }
    }

    /// Records an event; false when an idle repeats inside the same episode.
    ///
    /// The state is persisted before any notification: a crash may skip a gag but never rolls
    /// twice for one idle period. A resume missed while the helper is down waits for a real one.
    pub fn record(&mut self, directory: &Path, event: Event) -> Result<bool, AfkError> {
        match event {
            Event::Idle => {
                if self.idle {
                    return Ok(false);
                }
                if self.episode >= LIMIT {
                    return Err(AfkError::Exhausted);
                }
                self.episode += 1;
                self.idle = true;
            }
            Event::Resume => self.idle = false,
        }
        let value = json!({"seed": self.seed, "episode": self.episode, "idle": self.idle});
        write_atomic(directory, "afk-episode-", EPISODE, &value)?;
        Ok(true)
    }
}

/// Tells the bar about one AFK fact; true when the runtime accepted it.
#[expect(
    clippy::must_use_candidate,
    reason = "most facts are fire-and-forget; only the heartbeat reads delivery"
)]
pub fn emit(name: &str, value: i64) -> bool {
    let runtime = std::env::var("TOHRU_RUNTIME").unwrap_or_else(|_| "pleamar".to_owned());
    probe(
        &runtime,
        &["--say", "tohru", &format!("emit afk_{name} {value}")],
        Duration::from_secs(1),
    )
    .is_ok()
}

/// True only for an active, unlocked logind session.
#[must_use]
pub fn session_state() -> bool {
    let Some(session) = std::env::var("XDG_SESSION_ID")
        .ok()
        .filter(|id| !id.is_empty())
    else {
        return false;
    };
    let args = [
        "show-session",
        session.as_str(),
        "-p",
        "Active",
        "-p",
        "LockedHint",
        "-p",
        "State",
    ];
    let Ok(out) = probe("loginctl", &args, Duration::from_millis(500)) else {
        return false;
    };
    let field = |name: &str| {
        out.lines().find_map(|line| {
            line.split_once('=')
                .filter(|(key, _)| *key == name)
                .map(|(_, value)| value)
        })
    };
    field("Active") == Some("yes")
        && field("LockedHint") == Some("no")
        && field("State") == Some("active")
}

/// Forwards a swayidle callback to the serving generation.
///
/// Resume also stops the presentation directly, so input cancellation never waits behind the
/// server's probes; durable episode rearm still belongs to the server's liveness check.
pub fn notify(event: Event, generation: i64, manager: Option<Manager>) -> Result<(), AfkError> {
    let (address, _) = endpoint()?;
    if event == Event::Resume
        && let Ok(registration) = read_registration()
        && registration.generation == generation
        && manager
            .or(registration.observer)
            .is_some_and(Manager::alive)
    {
        emit("resume", generation);
    }
    let client = UnixDatagram::unbound()?;
    client.set_write_timeout(Some(Duration::from_millis(300)))?;
    client.connect(&address)?;
    let manager = manager.map_or(Value::Null, Manager::to_json);
    client.send(
        pyjson::dumps(
            &json!({"event": event.name(), "generation": generation, "manager": manager}),
        )
        .as_bytes(),
    )?;
    Ok(())
}

/// The `callbacks` command: the current generation's commands for an external manager.
pub fn callbacks(manager_pid: i64) -> Result<Value, AfkError> {
    let registration = read_registration()?;
    let manager = Some(Manager::current(manager_pid)?);
    Ok(json!({"seconds": registration.seconds,
              "idle": callback(registration.generation, Event::Idle, manager),
              "resume": callback(registration.generation, Event::Resume, manager)}))
}

/// The directory holding the AFK artwork.
pub fn assets() -> Result<PathBuf, AfkError> {
    paths::shell_candidates("assistant/dragon")
        .into_iter()
        .find(|path| ARTWORK.iter().all(|name| path.join(name).is_file()))
        .and_then(|path| path.canonicalize().ok())
        .ok_or(AfkError::Artwork)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum ControlAction {
    Start,
    Stop,
}

/// Records the bar's requested source and restarts or stops the AFK service.
///
/// Only an already active Tohru idle owner is refreshed: `try-restart` never starts another
/// lock/suspend owner or touches an external idle manager.
pub fn control(
    action: ControlAction,
    requested: Option<(i64, Option<IdleSource>, Option<IdleOwner>)>,
) -> Result<(), AfkError> {
    let directory = session_files()?;
    let before = match action {
        ControlAction::Start => effective_config(&Config::load()?)?.idle_owner,
        ControlAction::Stop => Effective::read(&directory)
            .ok()
            .flatten()
            .map_or(IdleOwner::External, |effective| effective.owner),
    };
    let mut owner = None;
    let mut seconds = None;
    if action == ControlAction::Start
        && let Some((idle_seconds, source, requested_owner)) = requested
    {
        let (Some(source), Some(requested_owner)) = (source, requested_owner) else {
            return Err(AfkError::Source);
        };
        if !(1..=86_400).contains(&idle_seconds) {
            return Err(AfkError::Source);
        }
        let effective = Effective {
            seconds: idle_seconds,
            source,
            owner: requested_owner,
        };
        write_atomic(
            &directory,
            "afk-effective-",
            EFFECTIVE,
            &effective.to_json(),
        )?;
        owner = Some(requested_owner);
        seconds = Some(idle_seconds);
    }
    let verb = match action {
        ControlAction::Start => "restart",
        ControlAction::Stop => "stop",
    };
    let result = probe(
        "systemctl",
        &["--user", verb, "tohru-afk.service"],
        Duration::from_secs(4),
    );
    if action == ControlAction::Stop {
        let _ = std::fs::remove_file(directory.join(EFFECTIVE));
        owner = Some(before);
    }
    result.map_err(|failure| AfkError::Service(failure.state()))?;
    if owner != Some(IdleOwner::Tohru) {
        return Ok(());
    }
    if action == ControlAction::Start {
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if read_registration().is_ok_and(|registration| Some(registration.seconds) == seconds) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    let _ = probe(
        "systemctl",
        &["--user", "try-restart", "tohru-idle.service"],
        Duration::from_secs(4),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passive_client_never_owns_lock_or_suspend() {
        let args = sway_arguments(300, 123, None);
        assert_eq!(args[..5], ["swayidle", "-C", "/dev/null", "timeout", "300"]);
        for hook in ["-w", "before-sleep", "lock", "unlock", "after-resume"] {
            assert!(!args.iter().any(|arg| arg == hook));
        }
        assert_eq!(args[6], "resume");
        for command in [&args[5], &args[7]] {
            let parsed = shell::split(command).unwrap();
            assert_eq!(parsed[parsed.len() - 2..], ["--generation", "123"]);
        }
        let owned = callback(7, Event::Idle, Some(Manager { pid: 41, start: 99 }));
        assert!(owned.ends_with("notify idle --generation 7 --manager-pid 41 --manager-start 99"));
    }

    #[test]
    fn episode_survives_restart_without_another_roll() {
        let directory = tempfile::tempdir().unwrap();
        let mut state = Episode::load(directory.path()).unwrap();
        assert!((1..=LIMIT).contains(&state.seed));
        assert!(state.record(directory.path(), Event::Idle).unwrap());
        assert_eq!(state.episode, 1);
        let mut restored = Episode::load(directory.path()).unwrap();
        assert_eq!(restored, state);
        assert!(!restored.record(directory.path(), Event::Idle).unwrap());
        assert!(restored.record(directory.path(), Event::Resume).unwrap());
        assert!(restored.record(directory.path(), Event::Idle).unwrap());
        assert_eq!(Episode::load(directory.path()).unwrap().episode, 2);
        let text = std::fs::read_to_string(directory.path().join(EPISODE)).unwrap();
        assert_eq!(
            text,
            format!(r#"{{"seed": {}, "episode": 2, "idle": true}}"#, state.seed)
        );
        std::fs::write(
            directory.path().join(EPISODE),
            r#"{"seed": 1, "episode": 0, "idle": 1}"#,
        )
        .unwrap();
        assert!(matches!(
            Episode::load(directory.path()),
            Err(AfkError::Episode)
        ));
    }

    #[test]
    fn effective_source_is_validated() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(Effective::read(directory.path()).unwrap(), None);
        let path = directory.path().join(EFFECTIVE);
        std::fs::write(
            &path,
            r#"{"enabled": true, "seconds": 420, "source": "callbacks", "owner": "tohru"}"#,
        )
        .unwrap();
        let effective = Effective::read(directory.path()).unwrap().unwrap();
        assert_eq!(
            effective,
            Effective {
                seconds: 420,
                source: IdleSource::Callbacks,
                owner: IdleOwner::Tohru
            }
        );
        for invalid in [
            r#"{"enabled": true, "seconds": 0, "source": "callbacks", "owner": "tohru"}"#,
            r#"{"enabled": 1, "seconds": 420, "source": "callbacks", "owner": "tohru"}"#,
            r#"{"enabled": true, "seconds": 420, "source": "loud", "owner": "tohru"}"#,
            "[]",
        ] {
            std::fs::write(&path, invalid).unwrap();
            assert!(
                matches!(Effective::read(directory.path()), Err(AfkError::Effective)),
                "{invalid}"
            );
        }
    }

    #[test]
    fn manager_claims_need_two_integers() {
        assert_eq!(Claim::parse(None), Claim::Absent);
        assert_eq!(Claim::parse(Some(&Value::Null)), Claim::Absent);
        assert_eq!(
            Claim::parse(Some(&json!([3, 4]))),
            Claim::Manager(Manager { pid: 3, start: 4 })
        );
        assert_eq!(Claim::parse(Some(&json!([3, true]))), Claim::Invalid);
        assert_eq!(Claim::parse(Some(&json!([3]))), Claim::Invalid);
        let me = Manager::current(i64::from(std::process::id())).unwrap();
        assert!(me.alive());
        assert!(
            !Manager {
                start: me.start + 1,
                ..me
            }
            .alive()
        );
    }
}
