use std::collections::{BTreeSet, HashMap};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use crate::config::{Config, ConfigError};
use crate::paths::{self, SessionError, Xdg};
use crate::{probe, pyjson, shell, wayland};

pub const VIDEO: [&str; 5] = ["mp4", "webm", "mkv", "mov", "gif"];
pub const STILL: [&str; 4] = ["jpg", "jpeg", "png", "webp"];
pub const CONTROL: &str = "control.sock";
const USAGE: &str =
    "usage: tohru-wallpaper serve|scan|status|apply|load|pause|seek|stop|thumbs|snapshot";

#[derive(Debug, thiserror::Error)]
pub enum WallpaperError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{USAGE}")]
    Usage,
    #[error("wallpaper assignments exceed 1 MiB")]
    TooLarge,
    #[error("invalid wallpaper assignments")]
    Assignments,
    #[error("wallpaper must be a supported regular file directly in wallpaper.directory")]
    File,
    #[error("wallpaper backend refused command")]
    Refused,
    #[error("wallpaper backend did not answer")]
    Silent,
    #[error("wallpaper.options cannot override IPC, config or scripts")]
    Options,
    #[error("mpvpaper failed to create its IPC socket; see journal")]
    Socket,
    #[error("output is not currently advertised")]
    Output,
    #[error("apply/load requires output, file and optional start seconds")]
    Arity,
    #[error("seconds must be finite and in 0..864000")]
    Seconds,
    #[error("unsupported wallpaper command")]
    Unsupported,
    #[error("configured mpvpaper executable is missing")]
    Missing,
    #[error("invalid command")]
    Command,
    #[error(
        "Tohru wallpaper owner unavailable; explicitly enable wallpaper.enabled and start tohru-wallpaper.service"
    )]
    Unavailable,
    #[error("{0}")]
    Owner(String),
}

/// Where wallpapers come from, where assignments persist and where the session's sockets live.
#[derive(Clone, Debug)]
pub struct Places {
    pub directory: PathBuf,
    pub state: PathBuf,
    pub runtime: PathBuf,
}

impl Places {
    pub fn new(config: &Config) -> Result<Self, WallpaperError> {
        let configured = |name: &str| {
            Some(config.text(name))
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        Ok(Self {
            directory: configured("wallpaper.directory")
                .unwrap_or_else(|| paths::home().join("Pictures/Wallpapers")),
            state: configured("wallpaper.assignments")
                .unwrap_or_else(|| paths::xdg(Xdg::State).join("tohru/wallpaper/assignments.json")),
            runtime: paths::session_directory()?.join("wallpaper"),
        })
    }
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .map(|extension| extension.to_string_lossy().to_lowercase())
}

fn supported(path: &Path) -> bool {
    extension(path).is_some_and(|extension| {
        VIDEO.contains(&extension.as_str()) || STILL.contains(&extension.as_str())
    })
}

/// The output → file assignments; missing means none.
pub fn assignment_data(file: &Path) -> Result<Map<String, Value>, WallpaperError> {
    if !file.exists() {
        return Ok(Map::new());
    }
    if std::fs::metadata(file)?.len() > 1024 * 1024 {
        return Err(WallpaperError::TooLarge);
    }
    let data: Value =
        serde_json::from_slice(&std::fs::read(file)?).map_err(|_| WallpaperError::Assignments)?;
    match data.get("assignments") {
        None => Ok(Map::new()),
        Some(Value::Object(map)) if map.values().all(Value::is_string) => Ok(map.clone()),
        Some(_) => Err(WallpaperError::Assignments),
    }
}

fn save(file: &Path, assignments: &Map<String, Value>) -> Result<(), WallpaperError> {
    let parent = file.parent().ok_or(WallpaperError::Assignments)?;
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(pyjson::dumps(&json!({"assignments": assignments})).as_bytes())?;
    temporary.as_file().sync_all()?;
    temporary.persist(file).map_err(|error| error.error)?;
    Ok(())
}

/// Sets or clears one output's assignment under the assignments lock.
pub fn update_assignment(
    file: &Path,
    name: &str,
    path: Option<&str>,
) -> Result<Map<String, Value>, WallpaperError> {
    let parent = file.parent().ok_or(WallpaperError::Assignments)?;
    std::fs::create_dir_all(parent)?;
    let mut lock_name = file.as_os_str().to_owned();
    lock_name.push(".lock");
    let lock = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(PathBuf::from(lock_name))?;
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::LockExclusive)
        .map_err(std::io::Error::from)?;
    let mut data = assignment_data(file)?;
    match path {
        Some(path) => {
            data.insert(name.to_owned(), Value::String(path.to_owned()));
        }
        None => {
            data.shift_remove(name);
        }
    }
    save(file, &data)?;
    Ok(data)
}

/// The canonical path of a supported file directly inside `directory`.
pub fn valid_file(directory: &Path, name: &str) -> Result<String, WallpaperError> {
    let path = Path::new(name).canonicalize()?;
    let parent_matches = directory
        .canonicalize()
        .is_ok_and(|directory| path.parent() == Some(directory.as_path()));
    if !path.is_file() || !parent_matches || !supported(&path) {
        return Err(WallpaperError::File);
    }
    Ok(path.to_string_lossy().into_owned())
}

/// One mpv IPC request, answered within a second.
pub fn ipc(path: &Path, command: &Value) -> Result<Value, WallpaperError> {
    let mut connection = UnixStream::connect(path)?;
    connection.set_write_timeout(Some(Duration::from_secs(1)))?;
    connection.write_all(
        format!(
            "{}\n",
            pyjson::dumps(&json!({"command": command, "request_id": 1}))
        )
        .as_bytes(),
    )?;
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut data = Vec::new();
    let mut received = 0;
    let mut chunk = [0_u8; 4096];
    while received <= 65_536 {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        connection.set_read_timeout(Some(remaining.max(Duration::from_millis(1))))?;
        let count = match connection.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break;
            }
            Err(error) => return Err(error.into()),
        };
        data.extend_from_slice(&chunk[..count]);
        received += count;
        while let Some(end) = data.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = data.drain(..=end).collect();
            let response: Value =
                serde_json::from_slice(&line).map_err(|_| WallpaperError::Silent)?;
            if response.get("request_id") == Some(&json!(1)) {
                if response.get("error") != Some(&json!("success")) {
                    return Err(WallpaperError::Refused);
                }
                return Ok(response.get("data").cloned().unwrap_or(Value::Null));
            }
        }
    }
    Err(WallpaperError::Silent)
}

/// Seconds as mpv takes them: finite and at most ten days.
pub fn seconds(value: &str) -> Result<f64, WallpaperError> {
    let result: f64 = value.trim().parse().map_err(|_| WallpaperError::Seconds)?;
    if !result.is_finite() || !(0.0..=864_000.0).contains(&result) {
        return Err(WallpaperError::Seconds);
    }
    Ok(result)
}

/// The wallpaper directory listing, assignments and outputs.
pub fn scan(
    places: &Places,
    monitors: &[Map<String, Value>],
) -> Result<Map<String, Value>, WallpaperError> {
    let mut files = Vec::new();
    if places.directory.is_dir() {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&places.directory)?
            .flatten()
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for path in entries {
            if !path.is_file() || !supported(&path) {
                continue;
            }
            let info = std::fs::metadata(&path)?;
            let kind =
                if extension(&path).is_some_and(|extension| VIDEO.contains(&extension.as_str())) {
                    "video"
                } else {
                    "still"
                };
            files.push(json!({"path": path.to_string_lossy(), "size": info.len(), "mtime": info.mtime(), "kind": kind}));
        }
    }
    let mut result = Map::new();
    result.insert("dir".into(), json!(places.directory.to_string_lossy()));
    result.insert("files".into(), Value::Array(files));
    result.insert(
        "assignments".into(),
        Value::Object(assignment_data(&places.state)?),
    );
    result.insert(
        "monitors".into(),
        Value::Array(monitors.iter().cloned().map(Value::Object).collect()),
    );
    Ok(result)
}

/// The session's one mpvpaper supervisor. Only children it created are ever terminated.
pub struct Owner {
    executable: String,
    options: String,
    places: Places,
    assignments: Map<String, Value>,
    children: HashMap<String, Child>,
    monitors: Vec<Map<String, Value>>,
    discovery: String,
    errors: Map<String, Value>,
    retry_at: HashMap<String, Instant>,
}

impl Owner {
    pub fn new(config: &Config, places: Places) -> Result<Self, WallpaperError> {
        Ok(Self {
            executable: config.text("wallpaper.executable").to_owned(),
            options: config.text("wallpaper.options").to_owned(),
            assignments: assignment_data(&places.state)?,
            places,
            children: HashMap::new(),
            monitors: Vec::new(),
            discovery: "unknown".to_owned(),
            errors: Map::new(),
            retry_at: HashMap::new(),
        })
    }

    fn socket(&self, name: &str) -> PathBuf {
        self.places.runtime.join(format!(
            "{}.sock",
            &paths::sha256_hex(name.as_bytes())[..16]
        ))
    }

    fn running(&mut self, name: &str) -> bool {
        self.children
            .get_mut(name)
            .is_some_and(|child| matches!(child.try_wait(), Ok(None)))
    }

    fn advertised(&self) -> BTreeSet<String> {
        self.monitors
            .iter()
            .filter_map(|monitor| monitor.get("name")?.as_str().map(str::to_owned))
            .collect()
    }

    pub fn stop(&mut self, name: &str) {
        if let Some(mut child) = self.children.remove(name) {
            if let Some(pid) = i32::try_from(child.id())
                .ok()
                .and_then(rustix::process::Pid::from_raw)
            {
                let _ = rustix::process::kill_process(pid, rustix::process::Signal::TERM);
            }
            let deadline = Instant::now() + Duration::from_secs(3);
            while matches!(child.try_wait(), Ok(None)) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            if matches!(child.try_wait(), Ok(None)) {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
        let _ = std::fs::remove_file(self.socket(name));
    }

    fn start(&mut self, name: &str, file: &str) -> Result<(), WallpaperError> {
        self.stop(name);
        // Configured options must never replace our IPC endpoint or launch scripts.
        if ["input-ipc", "script", "config", "include"]
            .iter()
            .any(|word| self.options.contains(word))
        {
            return Err(WallpaperError::Options);
        }
        let socket = self.socket(name);
        let options = format!(
            "--no-config --idle=yes --image-display-duration=inf --input-ipc-server={} {}",
            shell::quote(&socket.to_string_lossy()),
            self.options
        );
        let child = Command::new(&self.executable)
            .args(["-o", &options, name, file])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()?;
        self.children.insert(name.to_owned(), child);
        let deadline = Instant::now() + Duration::from_secs(4);
        while !socket.exists() {
            if !self.running(name) || Instant::now() > deadline {
                self.stop(name);
                return Err(WallpaperError::Socket);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Ok(())
    }

    /// Follows the advertised outputs; a failed discovery leaves live wallpapers alone.
    pub fn reconcile(&mut self) {
        let (monitors, discovery) = wayland::outputs();
        self.discovery = discovery.state();
        if discovery != wayland::Discovery::Ok {
            return;
        }
        self.monitors = monitors;
        let names = self.advertised();
        self.retry_at.retain(|name, _| names.contains(name));
        self.errors.retain(|name, _| names.contains(name));
        let gone: Vec<String> = self
            .children
            .keys()
            .filter(|name| !names.contains(*name))
            .cloned()
            .collect();
        for name in gone {
            self.stop(&name);
        }
        for name in &names {
            if self.running(name)
                || self
                    .retry_at
                    .get(name)
                    .is_some_and(|at| Instant::now() < *at)
            {
                continue;
            }
            let Some(assigned) = self
                .assignments
                .get(name)
                .and_then(Value::as_str)
                .map(str::to_owned)
            else {
                continue;
            };
            match valid_file(&self.places.directory, &assigned)
                .and_then(|file| self.start(name, &file))
            {
                Ok(()) => {
                    self.errors.remove(name);
                }
                Err(error) => {
                    self.errors
                        .insert(name.clone(), Value::String(error.to_string()));
                    self.retry_at
                        .insert(name.clone(), Instant::now() + Duration::from_secs(5));
                }
            }
        }
    }

    fn playing(&mut self, name: &str) -> Value {
        if !self.running(name) {
            return Value::Null;
        }
        let socket = self.socket(name);
        let Ok(path) = ipc(&socket, &json!(["get_property", "path"])) else {
            return Value::Null;
        };
        let mut result = json!({"path": path});
        for (key, property, default) in [
            ("pos", "time-pos", json!(0)),
            ("duration", "duration", json!(0)),
            ("paused", "pause", json!(false)),
        ] {
            let value = ipc(&socket, &json!(["get_property", property]))
                .ok()
                .filter(|value| !value.is_null());
            result[key] = value.unwrap_or(default);
        }
        result
    }

    fn status(&mut self) -> Map<String, Value> {
        let names: Vec<String> = self.advertised().into_iter().collect();
        let mut playing = Map::new();
        for name in names {
            let state = self.playing(&name);
            playing.insert(name, state);
        }
        let mut result = Map::new();
        result.insert("playing".into(), Value::Object(playing));
        result.insert("backend".into(), json!("mpvpaper"));
        result.insert("enabled".into(), json!(true));
        result.insert("owner".into(), json!("tohru"));
        result.insert("discovery".into(), json!(self.discovery));
        result.insert("errors".into(), Value::Object(self.errors.clone()));
        result
    }

    /// One control-socket command.
    pub fn command(&mut self, args: &[String]) -> Result<Value, WallpaperError> {
        let action = args.first().ok_or(WallpaperError::Command)?.as_str();
        if matches!(action, "scan" | "status") {
            let mut result = self.status();
            if action == "scan" {
                result.extend(scan(&self.places, &self.monitors)?);
            }
            return Ok(Value::Object(result));
        }
        let Some(name) = args
            .get(1)
            .filter(|name| self.advertised().contains(*name))
            .cloned()
        else {
            return Err(WallpaperError::Output);
        };
        let socket = self.socket(&name);
        match (action, args.len()) {
            ("playing", _) => return Ok(self.playing(&name)),
            ("apply" | "load", 3 | 4) => {
                let file = valid_file(&self.places.directory, &args[2])?;
                if !self.running(&name) {
                    self.start(&name, &file)?;
                }
                ipc(&socket, &json!(["loadfile", file, "replace"]))?;
                if let Some(start) = args.get(3) {
                    ipc(&socket, &json!(["seek", seconds(start)?, "absolute"]))?;
                }
                if action == "apply" {
                    self.assignments = update_assignment(&self.places.state, &name, Some(&file))?;
                }
            }
            ("apply" | "load", _) => return Err(WallpaperError::Arity),
            ("pause", 3) if matches!(args[2].as_str(), "yes" | "no") => {
                ipc(&socket, &json!(["set_property", "pause", args[2] == "yes"]))?;
            }
            ("seek", 3) => {
                ipc(&socket, &json!(["seek", seconds(&args[2])?, "absolute"]))?;
            }
            ("stop", 2) => {
                self.stop(&name);
                self.assignments = update_assignment(&self.places.state, &name, None)?;
            }
            _ => return Err(WallpaperError::Unsupported),
        }
        Ok(json!({"ok": true}))
    }

    fn answer(&mut self, connection: &mut UnixStream) {
        let _ = connection.set_read_timeout(Some(Duration::from_secs(2)));
        let _ = connection.set_write_timeout(Some(Duration::from_secs(2)));
        let mut data = Vec::new();
        let _ = BufReader::new(Read::take(&mut *connection, 65_536)).read_until(b'\n', &mut data);
        let response = match serde_json::from_slice::<Vec<String>>(&data) {
            Ok(args) if !args.is_empty() => match self.command(&args) {
                Ok(result) => json!({"result": result}),
                Err(error) => json!({"error": error.to_string()}),
            },
            _ => json!({"error": WallpaperError::Command.to_string()}),
        };
        let _ = connection.write_all(format!("{}\n", pyjson::dumps(&response)).as_bytes());
    }

    fn shutdown(&mut self) {
        let names: Vec<String> = self.children.keys().cloned().collect();
        for name in names {
            self.stop(&name);
        }
    }
}

/// The foreground supervisor for this Wayland session.
pub fn serve(config: &Config, places: Places) -> Result<(), WallpaperError> {
    if !config.wallpaper_enabled {
        eprintln!("tohru-wallpaper: wallpaper.enabled is false; no owner started");
        return Ok(());
    }
    if probe::which(config.text("wallpaper.executable")).is_none() {
        return Err(WallpaperError::Missing);
    }
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&places.runtime)?;
    let lock = std::fs::File::create(places.runtime.join("owner.lock"))?;
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)
        .map_err(std::io::Error::from)?;
    let control = places.runtime.join(CONTROL);
    let _ = std::fs::remove_file(&control);
    let mut owner = Owner::new(config, places)?;
    let stopping = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        signal_hook::flag::register(signal, Arc::clone(&stopping))?;
    }
    let listener = UnixListener::bind(&control)?;
    std::fs::set_permissions(&control, std::fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let mut refresh = Instant::now();
    while !stopping.load(Ordering::SeqCst) {
        if Instant::now() >= refresh {
            owner.reconcile();
            refresh = Instant::now() + Duration::from_secs(3);
        }
        if !probe::wait_readable(&listener, Instant::now() + Duration::from_millis(500)) {
            continue;
        }
        let Ok((mut connection, _)) = listener.accept() else {
            continue;
        };
        if connection.set_nonblocking(false).is_ok() {
            owner.answer(&mut connection);
        }
    }
    owner.shutdown();
    let _ = std::fs::remove_file(&control);
    Ok(())
}

/// Sends one command to the running owner.
pub fn request(places: &Places, args: &[String]) -> Result<Value, WallpaperError> {
    let mut connection = UnixStream::connect(places.runtime.join(CONTROL))?;
    connection.set_read_timeout(Some(Duration::from_secs(15)))?;
    connection.set_write_timeout(Some(Duration::from_secs(15)))?;
    connection.write_all(format!("{}\n", pyjson::dumps(&json!(args))).as_bytes())?;
    let mut data = Vec::new();
    BufReader::new(Read::take(&mut connection, 1024 * 1024)).read_until(b'\n', &mut data)?;
    let response: Value = serde_json::from_slice(&data).map_err(|_| WallpaperError::Silent)?;
    if let Some(error) = response.get("error") {
        return Err(WallpaperError::Owner(
            error
                .as_str()
                .map_or_else(|| error.to_string(), str::to_owned),
        ));
    }
    Ok(response.get("result").cloned().unwrap_or(Value::Null))
}

/// What `scan` and `status` report while no owner is running.
pub fn offline(config: &Config, places: &Places, scanning: bool) -> Result<Value, WallpaperError> {
    let (monitors, discovery) = wayland::outputs();
    let owner = if config.wallpaper_enabled {
        "unavailable"
    } else {
        "disabled"
    };
    let mut result = Map::new();
    result.insert("playing".into(), json!({}));
    result.insert("enabled".into(), json!(config.wallpaper_enabled));
    result.insert("owner".into(), json!(owner));
    result.insert("discovery".into(), json!(discovery.state()));
    if scanning {
        result.extend(scan(places, &monitors)?);
    }
    Ok(Value::Object(result))
}

/// The CLI: `serve`, media helpers, or one command sent to the owner.
pub fn main(args: &[String]) -> Result<i32, WallpaperError> {
    let config = Config::load()?;
    let places = Places::new(&config)?;
    let Some(action) = args.first().map(String::as_str) else {
        return Err(WallpaperError::Usage);
    };
    if action == "serve" {
        serve(&config, places)?;
        return Ok(0);
    }
    if matches!(action, "thumbs" | "snapshot") {
        let status = Command::new(paths::bin_dir().join("tohru-wallpaper-media"))
            .args(args)
            .env("TOHRU_WALLPAPER_DIR", &places.directory)
            .status()?;
        return Ok(status.code().unwrap_or(1));
    }
    let result = match request(&places, args) {
        Ok(result) => result,
        Err(WallpaperError::Io(error))
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
            ) =>
        {
            if !matches!(action, "scan" | "status") {
                return Err(WallpaperError::Unavailable);
            }
            offline(&config, &places, action == "scan")?
        }
        Err(error) => return Err(error),
    };
    println!("{}", pyjson::dumps(&result));
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_are_finite_and_bounded() {
        assert!((seconds("12.5").unwrap() - 12.5).abs() < f64::EPSILON);
        for invalid in ["nan", "inf", "-1", "864001", "x"] {
            assert!(seconds(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn assignments_round_trip_under_the_lock() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("state/assignments.json");
        assert!(assignment_data(&file).unwrap().is_empty());
        update_assignment(&file, "DP-1", Some("/w/a.png")).unwrap();
        update_assignment(&file, "HDMI-A-1", Some("/w/b.mp4")).unwrap();
        let data = update_assignment(&file, "DP-1", None).unwrap();
        assert_eq!(Value::Object(data), json!({"HDMI-A-1": "/w/b.mp4"}));
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            r#"{"assignments": {"HDMI-A-1": "/w/b.mp4"}}"#
        );
        std::fs::write(&file, r#"{"assignments": {"DP-1": 3}}"#).unwrap();
        assert!(matches!(
            assignment_data(&file),
            Err(WallpaperError::Assignments)
        ));
    }

    #[test]
    fn only_supported_files_directly_in_the_directory() {
        let directory = tempfile::tempdir().unwrap();
        let nested = directory.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        for name in ["a.PNG", "b.txt", "nested/c.png"] {
            std::fs::write(directory.path().join(name), b"x").unwrap();
        }
        let ok = valid_file(
            directory.path(),
            &directory.path().join("a.PNG").to_string_lossy(),
        )
        .unwrap();
        assert!(ok.ends_with("a.PNG"));
        for name in ["b.txt", "nested/c.png", "missing.png"] {
            assert!(
                valid_file(
                    directory.path(),
                    &directory.path().join(name).to_string_lossy()
                )
                .is_err(),
                "{name}"
            );
        }
    }
}
