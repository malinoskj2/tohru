//! Rolling captures of every monitor, kept only while the dragon is on screen.
//!
//! Captures live on tmpfs under `$XDG_RUNTIME_DIR/tohru-screens`, which persists so a bind mount
//! of it stays valid; its contents exist only while a runner owns them. Nothing here uploads,
//! sends or logs an image.

use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{BufRead, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rustix::fs::{FlockOperation, Mode, flock};
use rustix::process::{Pid, Signal};
use serde_json::{Value, json};

pub const DIRECTORY: &str = "tohru-screens";
pub const INDEX: &str = "index.json";
pub const LATEST: &str = "latest.jpg";
pub const COMBINED: &str = "all";
const CHILD_LIMIT: Duration = Duration::from_secs(5);
const PROBE_LIMIT: usize = 256 * 1024;
const LOCK_POLL: Duration = Duration::from_millis(250);
const OUTPUT_REFRESH_TICKS: u32 = 20;

#[derive(Debug, thiserror::Error)]
pub enum ScreensError {
    #[error("XDG_RUNTIME_DIR must be an existing directory owned by this user")]
    Runtime,
    #[error("{0} is not a private directory owned by this user")]
    Unsafe(PathBuf),
    #[error("{0} is not on tmpfs; captures never go to a persistent disk")]
    Persistent(PathBuf),
    #[error("another capture runner kept its lock")]
    Busy,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// The captures folder: `TOHRU_SCREENS_DIR` when set, else `$XDG_RUNTIME_DIR/tohru-screens`.
pub fn directory() -> Result<PathBuf, ScreensError> {
    if let Some(dir) = std::env::var_os("TOHRU_SCREENS_DIR").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    Ok(crate::paths::runtime_dir()
        .map_err(|_| ScreensError::Runtime)?
        .join(DIRECTORY))
}

fn lock_path(dir: &Path) -> PathBuf {
    let mut name = dir.file_name().unwrap_or_default().to_os_string();
    name.push(".lock");
    dir.with_file_name(name)
}

/// Keeps output names to a safe file name.
#[must_use]
pub fn file_name(output: &str) -> String {
    let name: String = output
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() || name.starts_with('.') {
        format!("_{name}")
    } else {
        name
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Makes `dir` an existing 0700 directory owned by this user, refusing a link or a foreign one.
fn ensure_private(dir: &Path) -> Result<(), ScreensError> {
    match fs::symlink_metadata(dir) {
        Ok(meta) => {
            if !meta.is_dir() || meta.uid() != crate::paths::uid() {
                return Err(ScreensError::Unsafe(dir.to_owned()));
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)?;
        }
        Err(err) => return Err(err.into()),
    }
    fs::set_permissions(dir, Permissions::from_mode(0o700))?;
    if !in_memory(dir) {
        return Err(ScreensError::Persistent(dir.to_owned()));
    }
    Ok(())
}

/// Whether `dir` is on tmpfs or ramfs, so no capture can outlive a reboot on disk.
#[must_use]
pub fn in_memory(dir: &Path) -> bool {
    const TMPFS: u64 = 0x0102_1994;
    const RAMFS: u64 = 0x8584_58f6;
    rustix::fs::statfs(dir).is_ok_and(|fs| {
        #[expect(
            clippy::cast_sign_loss,
            reason = "filesystem magic numbers are positive"
        )]
        let kind = fs.f_type as u64;
        kind == TMPFS || kind == RAMFS
    })
}

/// Removes everything inside `dir`, keeping the directory itself.
fn empty(dir: &Path) -> std::io::Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// Exclusive ownership of a captures folder; released when dropped.
#[derive(Debug)]
pub struct Owner {
    _lock: File,
}

fn open_lock(dir: &Path) -> Result<File, ScreensError> {
    Ok(OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits().cast_signed())
        .open(lock_path(dir))?)
}

fn try_lock(file: &File) -> bool {
    flock(file, FlockOperation::NonBlockingLockExclusive).is_ok()
}

impl Owner {
    /// Takes the folder, asking an earlier runner to stop and waiting up to `patience` for it.
    pub fn take(dir: &Path, patience: Duration) -> Result<Self, ScreensError> {
        if let Some(parent) = dir.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = open_lock(dir)?;
        if !try_lock(&file) {
            let pid = fs::read_to_string(lock_path(dir)).unwrap_or_default();
            if let Some(pid) = pid.trim().parse::<i32>().ok().and_then(Pid::from_raw) {
                let _ = rustix::process::kill_process(pid, Signal::TERM);
            }
            let deadline = Instant::now() + patience;
            while !try_lock(&file) {
                if Instant::now() >= deadline {
                    return Err(ScreensError::Busy);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        file.set_len(0)?;
        write!(file, "{}", std::process::id())?;
        ensure_private(dir)?;
        empty(dir)?;
        Ok(Self { _lock: file })
    }
}

/// Empties the folder unless a live runner holds it. True when it was emptied (or absent).
pub fn clear(dir: &Path) -> Result<bool, ScreensError> {
    if fs::symlink_metadata(dir).is_err() {
        return Ok(true);
    }
    let file = open_lock(dir)?;
    // A sibling's fork briefly shares the descriptor before its exec closes it.
    let deadline = Instant::now() + Duration::from_millis(250);
    while !try_lock(&file) {
        if Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    ensure_private(dir)?;
    empty(dir)?;
    Ok(true)
}

/// Writes one monitor's image to `path`, giving up as soon as `cancelled` is true.
pub trait Capture {
    /// The named outputs; `None` when `cancelled` became true while asking.
    fn outputs(&mut self, cancelled: &dyn Fn() -> bool) -> Option<Vec<String>>;
    fn grab(&mut self, output: Option<&str>, path: &Path, cancelled: &dyn Fn() -> bool) -> bool;
}

/// Whether the session is locked or inactive, when captures must not exist.
pub trait Gate {
    fn locked(&mut self) -> bool;
}

/// `grim` against the session's compositor, outputs from `wayland-info`.
#[derive(Debug)]
pub struct Grim {
    pub quality: u8,
}

impl Capture for Grim {
    fn outputs(&mut self, cancelled: &dyn Fn() -> bool) -> Option<Vec<String>> {
        let mut command = Command::new("wayland-info");
        command.stdout(Stdio::piped());
        let report = match finish(command, PROBE_LIMIT, cancelled) {
            Finished::Done(Some(report)) => report,
            Finished::Done(None) => Vec::new(),
            Finished::Cancelled => return None,
        };
        Some(
            crate::wayland::parse_outputs(&String::from_utf8_lossy(&report))
                .into_iter()
                .filter_map(|o| o.get("name").and_then(Value::as_str).map(str::to_owned))
                .collect(),
        )
    }

    fn grab(&mut self, output: Option<&str>, path: &Path, cancelled: &dyn Fn() -> bool) -> bool {
        let mut command = Command::new("grim");
        command.args(["-t", "jpeg", "-q", &self.quality.to_string()]);
        if let Some(output) = output {
            command.args(["-o", output]);
        }
        command.arg(path).stdout(Stdio::null());
        matches!(finish(command, 0, cancelled), Finished::Done(Some(_)))
    }
}

enum Finished {
    /// Its stdout when it succeeded in time, `None` when it failed, timed out or overflowed.
    Done(Option<Vec<u8>>),
    Cancelled,
}

/// Runs `command` for up to `CHILD_LIMIT`, killing it the moment `cancelled` is true.
fn finish(mut command: Command, limit: usize, cancelled: &dyn Fn() -> bool) -> Finished {
    let Ok(mut child) = command.stdin(Stdio::null()).stderr(Stdio::null()).spawn() else {
        return Finished::Done(None);
    };
    let reader = child.stdout.take().map(|stdout| {
        std::thread::spawn(move || {
            let mut out = Vec::new();
            let taken = std::io::Read::read_to_end(
                &mut std::io::Read::take(stdout, u64::try_from(limit).unwrap_or(0) + 1),
                &mut out,
            );
            (taken.is_ok() && out.len() <= limit).then_some(out)
        })
    });
    let deadline = Instant::now() + CHILD_LIMIT;
    let status = loop {
        if cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Finished::Cancelled;
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let out = reader.map_or(Some(Vec::new()), |r| r.join().ok().flatten());
    Finished::Done(status.filter(std::process::ExitStatus::success).and(out))
}

/// logind's view of this session, and Tohru's own lock cycle.
#[derive(Debug)]
pub struct Session {
    runtime: Option<PathBuf>,
}

impl Session {
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            runtime: crate::paths::runtime_dir().ok(),
        }
    }
}

impl Gate for Session {
    fn locked(&mut self) -> bool {
        if self
            .runtime
            .as_deref()
            .is_some_and(|r| crate::paths::lock_status_endpoint(r).exists())
        {
            return true;
        }
        if std::env::var("XDG_SESSION_ID").is_ok_and(|id| !id.is_empty()) {
            return !crate::afk::session_state();
        }
        false
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Policy {
    pub interval: Duration,
    pub keep: usize,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(3),
            keep: 10,
        }
    }
}

/// The ring of captures for every monitor in one owned folder.
#[derive(Debug)]
pub struct Store {
    dir: PathBuf,
    keep: usize,
    serial: u64,
    outputs: Vec<String>,
}

impl Store {
    #[must_use]
    pub fn new(dir: PathBuf, keep: usize) -> Self {
        Self {
            dir,
            keep: keep.max(1),
            serial: 0,
            outputs: Vec::new(),
        }
    }

    /// One capture of every output; falls back to one combined image when no output is named.
    /// `abort` is asked before each grab and before each image is published, and `cancelled`
    /// during a grab; either ends the batch with `None`, leaving nothing of it behind.
    /// Otherwise returns how many images were written.
    pub fn capture(
        &mut self,
        capture: &mut dyn Capture,
        refresh: bool,
        abort: &mut dyn FnMut() -> bool,
        cancelled: &dyn Fn() -> bool,
    ) -> std::io::Result<Option<usize>> {
        if refresh || self.outputs.is_empty() {
            match capture.outputs(cancelled) {
                Some(outputs) => self.outputs = outputs,
                None => return Ok(None),
            }
        }
        self.serial += 1;
        let taken = now_ms();
        let names: Vec<Option<String>> = if self.outputs.is_empty() {
            vec![None]
        } else {
            self.outputs.iter().cloned().map(Some).collect()
        };
        let mut written = 0;
        let mut failed = false;
        for output in &names {
            let folder = self.dir.join(
                output
                    .as_deref()
                    .map_or_else(|| COMBINED.to_owned(), file_name),
            );
            fs::DirBuilder::new()
                .mode(0o700)
                .recursive(true)
                .create(&folder)?;
            let name = format!("{taken}-{:06}.jpg", self.serial);
            let partial = folder.join(format!(".{name}.part"));
            if abort() {
                return Ok(None);
            }
            let grabbed = capture.grab(output.as_deref(), &partial, cancelled);
            if cancelled() || abort() {
                let _ = fs::remove_file(&partial);
                return Ok(None);
            }
            if !grabbed || !partial.is_file() {
                let _ = fs::remove_file(&partial);
                failed = true;
                continue;
            }
            fs::set_permissions(&partial, Permissions::from_mode(0o600))?;
            fs::rename(&partial, folder.join(&name))?;
            let link = folder.join(".latest.part");
            let _ = fs::remove_file(&link);
            std::os::unix::fs::symlink(&name, &link)?;
            fs::rename(&link, folder.join(LATEST))?;
            self.prune(&folder)?;
            written += 1;
        }
        if failed {
            self.outputs.clear();
        }
        self.prune_outputs(&names)?;
        self.write_index(&names)?;
        Ok(Some(written))
    }

    fn frames(folder: &Path) -> std::io::Result<Vec<String>> {
        let mut frames: Vec<String> = fs::read_dir(folder)?
            .filter_map(Result::ok)
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.as_bytes().first().is_some_and(u8::is_ascii_digit))
            .collect();
        frames.sort();
        Ok(frames)
    }

    fn prune(&self, folder: &Path) -> std::io::Result<()> {
        let frames = Self::frames(folder)?;
        for old in &frames[..frames.len().saturating_sub(self.keep)] {
            fs::remove_file(folder.join(old))?;
        }
        Ok(())
    }

    /// Drops folders of monitors that are gone, so an unplugged screen leaves nothing behind.
    fn prune_outputs(&self, current: &[Option<String>]) -> std::io::Result<()> {
        let wanted: Vec<String> = current
            .iter()
            .map(|o| o.as_deref().map_or_else(|| COMBINED.to_owned(), file_name))
            .collect();
        for entry in fs::read_dir(&self.dir)?.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type()?.is_dir() && !wanted.contains(&name) {
                fs::remove_dir_all(entry.path())?;
            }
        }
        Ok(())
    }

    fn write_index(&self, names: &[Option<String>]) -> std::io::Result<()> {
        let mut monitors = Vec::new();
        for output in names {
            let folder = self.dir.join(
                output
                    .as_deref()
                    .map_or_else(|| COMBINED.to_owned(), file_name),
            );
            let Ok(frames) = Self::frames(&folder) else {
                continue;
            };
            let Some(last) = frames.last() else { continue };
            let taken: u64 = last
                .split('-')
                .next()
                .and_then(|t| t.parse().ok())
                .unwrap_or(0);
            monitors.push(json!({
                "output": output,
                "latest": folder.join(LATEST),
                "captured_ms": taken,
                "frames": frames.iter().map(|f| folder.join(f)).collect::<Vec<_>>(),
            }));
        }
        let index = json!({"version": 1, "updated_ms": now_ms(), "monitors": monitors});
        let partial = self.dir.join(".index.part");
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&partial)?;
        file.write_all(serde_json::to_string_pretty(&index)?.as_bytes())?;
        drop(file);
        fs::rename(partial, self.dir.join(INDEX))
    }

    /// Removes every capture, keeping the folder.
    pub fn purge(&mut self) -> std::io::Result<()> {
        empty(&self.dir)
    }
}

/// What reaches the runner from stdin and signals.
#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    Capture,
    Stop,
}

/// Parses one stdin line: `capture` takes one now, `stop` ends; anything else is ignored.
#[must_use]
pub fn parse(line: &str) -> Option<Event> {
    match line.trim() {
        "capture" => Some(Event::Capture),
        "stop" => Some(Event::Stop),
        _ => None,
    }
}

/// Captures until a stop, then deletes every capture. The first capture is immediate.
///
/// `stop` is set by whoever sends `Event::Stop`, so a batch in flight ends at once rather than
/// after its remaining monitors.
pub fn serve(
    dir: &Path,
    policy: Policy,
    capture: &mut dyn Capture,
    gate: &mut dyn Gate,
    events: &mpsc::Receiver<Event>,
    stop: &AtomicBool,
) -> std::io::Result<()> {
    let mut store = Store::new(dir.to_owned(), policy.keep);
    let mut ticks: u32 = 0;
    let mut next = Instant::now();
    let stopped = || stop.load(Ordering::SeqCst);
    let gate = std::cell::RefCell::new(gate);
    let locked = || gate.borrow_mut().locked();
    // While a child runs, the lock is asked at most every LOCK_POLL, so it revokes promptly.
    let polled = std::cell::Cell::new(Instant::now());
    let revoked = || {
        if stopped() {
            return true;
        }
        if polled.get().elapsed() < LOCK_POLL {
            return false;
        }
        polled.set(Instant::now());
        locked()
    };
    let result = loop {
        if stopped() {
            break Ok(());
        }
        let wait = next.saturating_duration_since(Instant::now());
        let forced = match events.recv_timeout(wait) {
            Ok(Event::Stop) | Err(RecvTimeoutError::Disconnected) => break Ok(()),
            Ok(Event::Capture) => true,
            Err(RecvTimeoutError::Timeout) => false,
        };
        if stopped() {
            break Ok(());
        }
        if !forced && Instant::now() < next {
            continue;
        }
        next = Instant::now() + policy.interval;
        let refresh = ticks.is_multiple_of(OUTPUT_REFRESH_TICKS);
        ticks = ticks.wrapping_add(1);
        let mut abort = || stopped() || locked();
        polled.set(Instant::now());
        match store.capture(capture, refresh, &mut abort, &revoked) {
            Ok(Some(_)) => {}
            Ok(None) if stopped() => break Ok(()),
            Ok(None) => {
                if let Err(err) = store.purge() {
                    break Err(err);
                }
            }
            Err(err) => break Err(err),
        }
    };
    let purged = store.purge();
    result.and(purged)
}

/// Sets `stop` and wakes the runner; a full queue already holds a wake-up.
fn send_stop(send: &mpsc::SyncSender<Event>, stop: &AtomicBool) {
    stop.store(true, Ordering::SeqCst);
    let _ = send.try_send(Event::Stop);
}

/// The `run` command: owns the folder, captures until stdin closes or a signal, then cleans up.
pub fn run(policy: Policy, quality: u8) -> Result<(), ScreensError> {
    rustix::process::umask(Mode::from_raw_mode(0o077));
    let _ = rustix::process::set_parent_process_death_signal(Some(Signal::TERM));
    let dir = directory()?;
    let owner = Owner::take(&dir, Duration::from_secs(3))?;
    let _reaper = reaper();
    let (send, events) = mpsc::sync_channel(8);
    let stop = Arc::new(AtomicBool::new(false));
    let (signals, signal_stop) = (send.clone(), stop.clone());
    let mut hooks = signal_hook::iterator::Signals::new([
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGHUP,
    ])?;
    // Both threads block for the whole run; the process exits without joining them.
    let _signal_thread = std::thread::spawn(move || {
        if hooks.forever().next().is_some() {
            send_stop(&signals, &signal_stop);
        }
    });
    let input_stop = stop.clone();
    let _input_thread = std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            match parse(&line) {
                Some(Event::Stop) => break,
                Some(Event::Capture) => {
                    let _ = send.try_send(Event::Capture);
                }
                None => {}
            }
        }
        send_stop(&send, &input_stop);
    });
    let result = serve(
        &dir,
        policy,
        &mut Grim { quality },
        &mut Session::from_env(),
        &events,
        &stop,
    );
    drop(owner);
    Ok(result?)
}

/// A process outside this one's group that clears the folder once this runner is gone, even
/// when it was killed outright. Its stdin is the pipe this process holds open until it dies.
fn reaper() -> Option<std::process::ChildStdin> {
    let exe = std::env::current_exe().ok()?;
    let mut child = Command::new(exe)
        .arg("reap")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .ok()?;
    child.stdin.take()
}

/// The `reap` command: waits for stdin to close, then clears the folder unless a runner owns it.
pub fn reap() -> Result<(), ScreensError> {
    let _ = std::io::copy(&mut std::io::stdin().lock(), &mut std::io::sink());
    let dir = directory()?;
    // Lets a capture the runner had in flight land before clearing.
    std::thread::sleep(Duration::from_millis(300));
    clear(&dir).map(drop)
}

/// A prompt line naming the newest captures, or empty when none exist.
#[must_use]
pub fn prompt_note() -> String {
    let Ok(dir) = directory() else {
        return String::new();
    };
    note_for(&dir, now_ms())
}

#[must_use]
pub fn note_for(dir: &Path, now: u64) -> String {
    let Ok(raw) = fs::read_to_string(dir.join(INDEX)) else {
        return String::new();
    };
    let Ok(index) = serde_json::from_str::<Value>(&raw) else {
        return String::new();
    };
    let lines: Vec<String> = index["monitors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let latest = m["latest"].as_str()?;
            let age = now.saturating_sub(m["captured_ms"].as_u64()?) / 1000;
            let name = m["output"].as_str().unwrap_or("all monitors");
            Some(format!("- {name}: {latest} ({age} s old)"))
        })
        .collect();
    if lines.is_empty() {
        return String::new();
    }
    format!(
        "[Screen captures: the desktop keeps a capture of each monitor every few seconds while you are \
on screen. For a question about the screen, read these images instead of taking a new screenshot; \
{} lists older frames. Never upload or send them anywhere.]\n{}",
        dir.join(INDEX).display(),
        lines.join("\n")
    )
}
