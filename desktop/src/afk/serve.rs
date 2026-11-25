use std::io::Read;
use std::os::unix::net::UnixDatagram;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::{
    AfkError, Claim, Episode, Event, LOCK, Manager, Registration, effective_config, emit, endpoint,
    random_below_limit, session_state, sway_arguments,
};
use crate::config::{Config, IdleOwner, IdleSource};

const LOGIND_MATCH: &str = "--match=type='signal',sender='org.freedesktop.login1'";

/// A child in its own process group, stopped with its group when dropped.
struct Group(Child);

impl Group {
    fn alive(&mut self) -> bool {
        matches!(self.0.try_wait(), Ok(None))
    }

    fn pid(&self) -> i64 {
        i64::from(self.0.id())
    }

    fn wait_for(&mut self, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        while Instant::now() < deadline {
            if !self.alive() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        !self.alive()
    }

    fn signal(&self, signal: rustix::process::Signal) {
        if let Some(pid) = i32::try_from(self.0.id())
            .ok()
            .and_then(rustix::process::Pid::from_raw)
        {
            let _ = rustix::process::kill_process_group(pid, signal);
        }
    }
}

impl Drop for Group {
    fn drop(&mut self) {
        self.signal(rustix::process::Signal::TERM);
        if !self.wait_for(Duration::from_secs(2)) {
            self.signal(rustix::process::Signal::KILL);
            self.wait_for(Duration::from_secs(2));
        }
    }
}

fn session_monitor() -> Option<Group> {
    let child = Command::new("busctl")
        .args(["--system", LOGIND_MATCH, "monitor"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .ok()?;
    if let Some(stdout) = &child.stdout {
        rustix::io::ioctl_fionbio(stdout, true).ok()?;
    }
    Some(Group(child))
}

struct Server {
    directory: PathBuf,
    generation: i64,
    seconds: i64,
    episode: Episode,
    observer: Option<Group>,
    monitor: Option<Group>,
    last_manager: Claim,
    pending_resume: Option<(Instant, Claim)>,
}

impl Server {
    fn manager_alive(&mut self, claim: Claim) -> bool {
        match claim {
            Claim::Absent => self.observer.as_mut().is_some_and(Group::alive),
            Claim::Manager(manager) => {
                self.observer
                    .as_ref()
                    .is_none_or(|observer| observer.pid() == manager.pid)
                    && manager.alive()
            }
            Claim::Invalid => false,
        }
    }

    fn observer_exited(&mut self) -> bool {
        self.observer
            .as_mut()
            .is_some_and(|observer| !observer.alive())
    }

    fn monitor_alive(&mut self) -> bool {
        self.monitor.as_mut().is_some_and(Group::alive)
    }

    /// Whether logind said anything since the last look; any signal voids the previous proof.
    fn logind_changed(&mut self) -> bool {
        if !self.monitor_alive() {
            return false;
        }
        let Some(stdout) = self
            .monitor
            .as_mut()
            .and_then(|monitor| monitor.0.stdout.as_mut())
        else {
            return false;
        };
        let mut buffer = [0_u8; 16_384];
        matches!(stdout.read(&mut buffer), Ok(read) if read > 0)
    }

    fn safe(&mut self, claim: Claim) -> bool {
        self.manager_alive(claim) && self.monitor_alive() && session_state()
    }

    /// A heartbeat proves source liveness only; it never reports idle or resets an episode.
    fn heartbeat(&mut self, unreachable: &mut u32) -> bool {
        let reachable = emit("source", self.generation);
        emit("seed", self.episode.seed);
        *unreachable = if reachable { 0 } else { *unreachable + 1 };
        if *unreachable >= 3 {
            eprintln!("AFK native bar unavailable; stopping passive observers");
            return false;
        }
        emit("threshold", self.seconds);
        let safe = self.safe(self.last_manager);
        emit(if safe { "safe" } else { "unsafe" }, self.generation);
        true
    }

    fn message(&mut self, raw: &[u8]) -> Result<(), AfkError> {
        let Ok(message) = serde_json::from_slice::<Value>(raw) else {
            return Ok(());
        };
        if !message.is_object() || message["generation"].as_i64() != Some(self.generation) {
            return Ok(());
        }
        let claim = Claim::parse(message.get("manager"));
        if !self.manager_alive(claim) {
            return Ok(());
        }
        self.last_manager = claim;
        match message["event"].as_str() {
            // Stop the gag at once, but rearm the episode only once the source proves alive:
            // swayidle also runs resume commands while shutting down, which is not input.
            Some("resume") => {
                emit("resume", self.generation);
                self.pending_resume = Some((Instant::now() + Duration::from_millis(200), claim));
            }
            Some("idle") if self.episode.record(&self.directory, Event::Idle)? => {
                emit("source", self.generation);
                emit("seed", self.episode.seed);
                emit("threshold", self.seconds);
                emit("episode", self.episode.episode);
                let safe = self.monitor_alive() && session_state();
                emit(if safe { "safe" } else { "unsafe" }, self.generation);
                emit("idle", self.generation);
            }
            _ => {}
        }
        Ok(())
    }

    fn run(&mut self, server: &UnixDatagram, stopping: &AtomicBool) -> Result<(), AfkError> {
        let mut next_heartbeat = Instant::now();
        let mut unreachable = 0;
        let mut buffer = [0_u8; 1024];
        while !stopping.load(Ordering::SeqCst) {
            if self.observer_exited() {
                return Err(AfkError::ObserverExited);
            }
            if let Some((due, claim)) = self.pending_resume
                && Instant::now() >= due
            {
                if self.manager_alive(claim) {
                    self.episode.record(&self.directory, Event::Resume)?;
                }
                self.pending_resume = None;
            }
            if self.logind_changed() {
                emit("unsafe", self.generation);
                next_heartbeat = Instant::now();
            }
            if Instant::now() >= next_heartbeat {
                if !self.heartbeat(&mut unreachable) {
                    break;
                }
                next_heartbeat = Instant::now() + Duration::from_secs(2);
            }
            let Ok(read) = server.recv(&mut buffer) else {
                continue;
            };
            if stopping.load(Ordering::SeqCst) {
                break;
            }
            if self.observer_exited() {
                return Err(AfkError::ObserverShutdown);
            }
            self.message(&buffer[..read])?;
        }
        Ok(())
    }
}

fn remove(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// The systemd-owned AFK helper: never a lock or suspend owner.
///
/// External ownership runs a passive swayidle notification client with only timeout and resume
/// callbacks; Tohru ownership adds the same callbacks to its own swayidle instead. No timestamps
/// infer input activity.
pub fn serve() -> Result<(), AfkError> {
    let config: Config = effective_config(&Config::load()?)?;
    if !config.afk_enabled || config.idle_owner == IdleOwner::Disabled {
        return Ok(());
    }
    let capabilities =
        crate::wayland::protocols().map_err(|failure| AfkError::Protocol(failure.state()))?;
    if !capabilities
        .iter()
        .any(|name| name == "ext_idle_notifier_v1")
    {
        return Err(AfkError::Protocol("ok".to_owned()));
    }
    let (address, registration_path) = endpoint()?;
    let directory = address
        .parent()
        .map(Path::to_owned)
        .ok_or(AfkError::Directory)?;
    let generation = random_below_limit();
    let seconds = i64::from(config.afk_idle_seconds);
    let stopping = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        signal_hook::flag::register(signal, Arc::clone(&stopping))?;
    }
    let lock = std::fs::File::create(directory.join(LOCK))?;
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)
        .map_err(std::io::Error::from)?;
    let episode = Episode::load(&directory)?;
    remove(&address);
    let socket = UnixDatagram::bind(&address)?;
    socket.set_read_timeout(Some(Duration::from_millis(200)))?;
    let registration = Registration {
        generation,
        seconds,
        observer: None,
    };
    registration.write(&registration_path)?;
    let mut server = Server {
        directory,
        generation,
        seconds,
        episode,
        observer: None,
        monitor: None,
        last_manager: Claim::Absent,
        pending_resume: None,
    };
    let outcome = start_observers(&mut server, &config, &registration_path, registration)
        .and_then(|()| server.run(&socket, &stopping));
    emit("lost", generation);
    // Unregister first: swayidle runs its resume command while stopping, which is not input.
    remove(&address);
    remove(&registration_path);
    // Both observer groups stay inside the systemd helper's lifetime.
    server.observer = None;
    server.monitor = None;
    outcome
}

fn start_observers(
    server: &mut Server,
    config: &Config,
    path: &Path,
    registration: Registration,
) -> Result<(), AfkError> {
    server.monitor = session_monitor();
    if config.idle_owner != IdleOwner::External || config.afk_idle_source != IdleSource::Passive {
        return Ok(());
    }
    let arguments = sway_arguments(server.seconds, server.generation, None);
    let child = Command::new(&arguments[0])
        .args(&arguments[1..])
        .process_group(0)
        .spawn()?;
    let observer = Group(child);
    let manager = Manager::current(observer.pid());
    server.observer = Some(observer);
    Registration {
        observer: Some(manager?),
        ..registration
    }
    .write(path)
}
