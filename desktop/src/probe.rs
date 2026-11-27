use std::ffi::OsStr;
use std::io::{ErrorKind, Read};
use std::os::fd::AsFd;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::process::{Pid, Signal, kill_process_group};

pub const PROBE_LIMIT: usize = 256 * 1024;

/// Why a bounded probe produced no output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProbeFailure {
    #[error("timeout")]
    Timeout,
    #[error("missing")]
    Missing,
    #[error("unavailable")]
    Unavailable,
    #[error("oversized response")]
    Oversized,
    #[error("permission denied or unavailable")]
    Denied,
    #[error("invalid response")]
    Invalid,
    #[error("cancelled")]
    Cancelled,
}

impl ProbeFailure {
    #[must_use]
    pub fn state(self) -> String {
        self.to_string()
    }
}

/// Kills a child's whole process group (it may have left descendants holding pipes) and reaps it.
pub fn kill_group(child: &mut Child) {
    if let Some(pid) = Pid::from_raw(child.id().cast_signed()) {
        let _ = kill_process_group(pid, Signal::KILL);
    }
    let _ = child.wait();
}

/// Waits for readability on `fd` until `deadline`; false on timeout.
pub fn wait_readable(fd: impl AsFd, deadline: Instant) -> bool {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        let timeout = Timespec::try_from(remaining).unwrap_or(Timespec {
            tv_sec: 1,
            tv_nsec: 0,
        });
        let mut fds = [PollFd::new(&fd, PollFlags::IN)];
        match poll(&mut fds, Some(&timeout)) {
            Ok(0) | Err(rustix::io::Errno::INTR) => {}
            Ok(_) | Err(_) => return true,
        }
    }
}

/// Runs `program` in its own process group with bounded time and stored output.
///
/// The output is returned only for a zero exit, decoded lossily.
pub fn probe<S: AsRef<OsStr>>(
    program: impl AsRef<OsStr>,
    args: &[S],
    timeout: Duration,
) -> Result<String, ProbeFailure> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0);
    probe_command(&mut command, timeout)
}

pub fn probe_command(command: &mut Command, timeout: Duration) -> Result<String, ProbeFailure> {
    cancellable(command, timeout, PROBE_LIMIT, &|| false)
        .map(|data| String::from_utf8_lossy(&data).into_owned())
}

/// Runs `command` like [`probe_command`], up to `limit` bytes of stdout, and also gives up
/// within a few milliseconds of `cancelled` becoming true.
///
/// The command is put in its own process group. On any failure the whole group is killed, so
/// a descendant still holding stdout neither keeps the call waiting nor outlives it.
pub fn cancellable(
    command: &mut Command,
    timeout: Duration,
    limit: usize,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<u8>, ProbeFailure> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) if error.kind() == ErrorKind::NotFound => return Err(ProbeFailure::Missing),
        Err(_) => return Err(ProbeFailure::Denied),
    };
    let result = collect(&mut child, Instant::now() + timeout, limit, cancelled);
    if result.is_err() {
        kill_group(&mut child);
    }
    result
}

const CANCEL_POLL: Duration = Duration::from_millis(10);

fn collect(
    child: &mut Child,
    deadline: Instant,
    limit: usize,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<u8>, ProbeFailure> {
    let Some(mut stdout) = child.stdout.take() else {
        return Err(ProbeFailure::Denied);
    };
    let mut data = Vec::new();
    let mut chunk = vec![0_u8; 65536];
    loop {
        if cancelled() {
            return Err(ProbeFailure::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(ProbeFailure::Timeout);
        }
        if !wait_readable(&stdout, deadline.min(Instant::now() + CANCEL_POLL)) {
            continue;
        }
        let count = match stdout.read(&mut chunk) {
            Ok(count) => count,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return Err(ProbeFailure::Denied),
        };
        if count == 0 {
            break;
        }
        data.extend_from_slice(&chunk[..count]);
        if data.len() > limit {
            return Err(ProbeFailure::Oversized);
        }
    }
    let status = wait_until(child, deadline, cancelled)?;
    if status.success() {
        Ok(data)
    } else {
        Err(ProbeFailure::Unavailable)
    }
}

fn wait_until(
    child: &mut Child,
    deadline: Instant,
    cancelled: &dyn Fn() -> bool,
) -> Result<std::process::ExitStatus, ProbeFailure> {
    loop {
        if cancelled() {
            return Err(ProbeFailure::Cancelled);
        }
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(_) => return Err(ProbeFailure::Denied),
        }
        if Instant::now() >= deadline {
            return Err(ProbeFailure::Timeout);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// A bounded probe whose output must be JSON.
pub fn json_probe<S: AsRef<OsStr>>(
    program: impl AsRef<OsStr>,
    args: &[S],
    timeout: Duration,
) -> Result<serde_json::Value, ProbeFailure> {
    let text = probe(program, args, timeout)?;
    serde_json::from_str(&text).map_err(|_| ProbeFailure::Invalid)
}

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3);

/// Whether `name` resolves to an executable on `PATH` (or is an executable path).
#[must_use]
pub fn which(name: &str) -> Option<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let executable = |path: &std::path::Path| {
        std::fs::metadata(path)
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    };
    if name.contains('/') {
        let path = std::path::PathBuf::from(name);
        return executable(&path).then_some(path);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .filter(|directory| !directory.as_os_str().is_empty())
        .map(|directory| directory.join(name))
        .find(|candidate| executable(candidate))
}
