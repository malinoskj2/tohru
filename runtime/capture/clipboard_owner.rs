//! Native Linux ownership of the one foreground clipboard helper, with automatic reaping.
#![expect(
    unsafe_code,
    reason = "fcntl, parent-death protection and pidfd ownership signals"
)]
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Mutex,
    mpsc::{self, SyncSender},
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

static MANAGER: Mutex<Option<Manager>> = Mutex::new(None);

struct Pending(Option<Child>);

impl Drop for Pending {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

struct Owner {
    pidfd: OwnedFd,
    waiter: Option<JoinHandle<()>>,
}

impl Drop for Owner {
    fn drop(&mut self) {
        // SAFETY: pidfd names this child even after exit/reaping, never a reused PID.
        unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                self.pidfd.as_raw_fd(),
                libc::SIGKILL,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        };
        if let Some(waiter) = self.waiter.take() {
            let _ = waiter.join();
        }
    }
}

fn nonblocking(pipe: &impl AsRawFd) -> io::Result<()> {
    // SAFETY: pipe owns a live descriptor; F_GETFL uses no pointer.
    let flags = unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: same live descriptor with its original flags plus nonblocking mode.
    if unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn drain(pipe: &mut impl Read, bytes: &mut Vec<u8>, limit: usize) -> io::Result<()> {
    let mut buffer = [0_u8; 512];
    loop {
        match pipe.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(n) if bytes.len() + n <= limit => bytes.extend_from_slice(&buffer[..n]),
            Ok(_) => return Err(io::Error::other("clipboard helper response over budget")),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

fn protect_parent(parent: libc::pid_t) -> io::Result<()> {
    // SAFETY: prctl takes scalar options only; no pointer is passed.
    if unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: getppid takes no arguments and inspects this child process only.
    if unsafe { libc::getppid() } != parent {
        return Err(io::Error::from_raw_os_error(libc::ESRCH));
    }
    Ok(())
}

fn start(executable: &Path, png: &[u8], timeout: Duration) -> Result<Owner, String> {
    // SAFETY: getpid takes no arguments and returns the caller's PID.
    let parent = unsafe { libc::getpid() };
    let mut command = Command::new(executable);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // SAFETY: the callback delegates to an async-signal-safe syscall-only routine.
    unsafe {
        command.pre_exec(move || protect_parent(parent));
    }
    let mut pending =
        Pending(Some(command.spawn().map_err(|e| {
            format!("starting packaged clipboard helper: {e}")
        })?));
    let child = pending.0.as_mut().ok_or("clipboard child missing")?;
    let mut input = Some(child.stdin.take().ok_or("clipboard stdin missing")?);
    let mut output = child.stdout.take().ok_or("clipboard stdout missing")?;
    let mut errors = child.stderr.take().ok_or("clipboard stderr missing")?;
    nonblocking(input.as_ref().ok_or("clipboard stdin missing")?).map_err(|e| e.to_string())?;
    nonblocking(&output).map_err(|e| e.to_string())?;
    nonblocking(&errors).map_err(|e| e.to_string())?;
    let until = Instant::now() + timeout;
    let (mut offset, mut ack, mut diagnostic) = (0, Vec::new(), Vec::new());
    loop {
        if let Some(pipe) = &mut input {
            match pipe.write(&png[offset..]) {
                Ok(n) => {
                    offset += n;
                    if offset == png.len() {
                        input = None;
                    }
                }
                Err(e)
                    if e.kind() == io::ErrorKind::WouldBlock
                        || e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(format!("sending capture PNG to clipboard: {e}")),
            }
        }
        drain(&mut output, &mut ack, 64).map_err(|e| e.to_string())?;
        drain(&mut errors, &mut diagnostic, 4096).map_err(|e| e.to_string())?;
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Err(format!(
                "clipboard helper exited before confirmed ownership ({status}): {}",
                String::from_utf8_lossy(&diagnostic).trim()
            ));
        }
        if input.is_none() && ack == b"READY\n" {
            break;
        }
        if Instant::now() >= until {
            return Err("clipboard ownership readiness deadline exceeded".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    // SAFETY: child is ours, unreaped and still present; pidfd_open returns a new owned fd.
    let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, child.id(), 0) };
    let raw = i32::try_from(raw).map_err(|_| "invalid clipboard pidfd")?;
    if raw < 0 {
        return Err(format!(
            "opening clipboard child pidfd: {}",
            io::Error::last_os_error()
        ));
    }
    // SAFETY: successful pidfd_open created this descriptor, transferred exactly once.
    let pidfd = unsafe { OwnedFd::from_raw_fd(raw) };
    let waiter = std::thread::Builder::new()
        .name("capture-clipboard-reap".into())
        .spawn(move || {
            if let Some(child) = &mut pending.0 {
                let _ = child.wait();
            }
        })
        .map_err(|e| format!("starting clipboard child reaper: {e}"))?;
    Ok(Owner {
        pidfd,
        waiter: Some(waiter),
    })
}

enum Request {
    Copy(Vec<u8>, SyncSender<Result<(), String>>),
    Shutdown,
}

struct Manager {
    sender: SyncSender<Request>,
    thread: Option<JoinHandle<()>>,
}

impl Manager {
    fn new(executable: PathBuf) -> Result<Self, String> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("capture-clipboard-owner".into())
            .spawn(move || {
                // This stable process-owned thread, not a reloadable Lua caller,
                // parents the foreground helper for Linux PR_SET_PDEATHSIG.
                let mut current = None;
                while let Ok(request) = receiver.recv() {
                    match request {
                        Request::Copy(png, reply) => {
                            let outcome = match start(&executable, &png, Duration::from_secs(4)) {
                                Ok(next) => {
                                    current = Some(next);
                                    Ok(())
                                }
                                Err(error) => Err(error),
                            };
                            let _ = reply.send(outcome);
                        }
                        Request::Shutdown => break,
                    }
                }
                drop(current);
            })
            .map_err(|e| format!("starting process-owned clipboard manager: {e}"))?;
        Ok(Self {
            sender,
            thread: Some(thread),
        })
    }

    fn copy(&self, png: Vec<u8>) -> Result<(), String> {
        let (reply, received) = mpsc::sync_channel(1);
        self.sender
            .send(Request::Copy(png, reply))
            .map_err(|_| "clipboard manager ended")?;
        received
            .recv()
            .map_err(|_| "clipboard manager ended before reporting ownership")?
    }
}

impl Drop for Manager {
    fn drop(&mut self) {
        let _ = self.sender.send(Request::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(super) fn copy(png: Vec<u8>) -> Result<(), String> {
    let limit = tohru_shell::capture::MAX_PIXELS * 4 + 1024 * 1024;
    if !png.starts_with(b"\x89PNG\r\n\x1a\n") || png.len() as u64 > limit {
        return Err("invalid clipboard PNG bytes".into());
    }
    let mut manager = MANAGER
        .lock()
        .map_err(|_| "clipboard manager unavailable")?;
    if manager.is_none() {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let executable = executable
            .parent()
            .ok_or("clipboard runtime has no parent")?
            .join("tohru-capture-clipboard");
        *manager = Some(Manager::new(executable)?);
    }
    manager
        .as_ref()
        .ok_or("clipboard manager unavailable")?
        .copy(png)
}

pub(super) fn shutdown() {
    if let Ok(mut manager) = MANAGER.lock() {
        *manager = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Helper(PathBuf);
    impl Helper {
        fn new(body: &str) -> Self {
            let path = std::env::current_dir().unwrap().join(".tmp").join(format!(
                "capture-clipboard-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            let source = format!(
                "#!/usr/bin/env python3\nimport os,sys,time\nopen(os.path.join(os.path.dirname(__file__),'pid'),'w').write(str(os.getpid()))\n{body}\n"
            );
            fs::write(path.join("helper"), source).unwrap();
            fs::set_permissions(path.join("helper"), fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
        fn program(&self) -> PathBuf {
            self.0.join("helper")
        }
        fn pid(&self) -> String {
            fs::read_to_string(self.0.join("pid")).unwrap()
        }
    }
    impl Drop for Helper {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn gone(pid: &str) -> bool {
        !Path::new("/proc").join(pid).exists()
    }
    #[test]
    fn owned_replacement_and_drop_stop_and_reap_old_server() {
        let first =
            Helper::new("sys.stdin.buffer.read(); print('READY',flush=True); time.sleep(10)");
        let second =
            Helper::new("sys.stdin.buffer.read(); print('READY',flush=True); time.sleep(10)");
        let mut owner = Some(start(&first.program(), b"PNG", Duration::from_secs(1)).unwrap());
        let old = first.pid();
        assert!(!gone(&old));
        // New ownership must be acknowledged before replacing this handle.
        let next = start(&second.program(), b"PNG", Duration::from_secs(1)).unwrap();
        owner.replace(next);
        assert!(gone(&old));
        let current = second.pid();
        drop(owner);
        assert!(gone(&current));
    }
    #[test]
    fn missing_readiness_and_malformed_helper_are_not_success() {
        let helper = Helper::new("sys.stdin.buffer.read(); time.sleep(10)");
        assert!(start(&helper.program(), b"PNG", Duration::from_millis(150)).is_err());
        assert!(gone(&helper.pid()));
        let helper =
            Helper::new("sys.stdin.buffer.read(); print('wrong',flush=True); time.sleep(10)");
        assert!(start(&helper.program(), b"PNG", Duration::from_millis(150)).is_err());
        assert!(gone(&helper.pid()));
    }
    #[test]
    fn ownership_loss_is_reaped_without_polling_or_next_copy() {
        let helper =
            Helper::new("sys.stdin.buffer.read(); print('READY',flush=True); time.sleep(0.1)");
        let owner = start(&helper.program(), b"PNG", Duration::from_secs(1)).unwrap();
        let pid = helper.pid();
        std::thread::sleep(Duration::from_millis(200));
        assert!(gone(&pid));
        drop(owner);
    }
    #[test]
    fn requester_thread_reload_does_not_end_process_owned_clipboard() {
        let helper =
            Helper::new("sys.stdin.buffer.read(); print('READY',flush=True); time.sleep(10)");
        let executable = helper.program();
        let manager = std::thread::spawn(move || {
            let manager = Manager::new(executable).unwrap();
            manager.copy(b"fixture".to_vec()).unwrap();
            manager
        })
        .join()
        .unwrap();
        let pid = helper.pid();
        std::thread::sleep(Duration::from_millis(30));
        assert!(
            !gone(&pid),
            "only the stable manager thread parents the helper"
        );
        drop(manager);
        assert!(gone(&pid));
    }

    #[test]
    fn parent_exit_race_refuses_exec_and_png_validation_spawns_nothing() {
        // SAFETY: getpid has no arguments or side effects.
        let wrong = unsafe { libc::getpid() } + 1;
        let mut command = Command::new("/does-not-run");
        // SAFETY: routine uses only prctl/getppid and returns ESRCH before exec.
        unsafe {
            command.pre_exec(move || protect_parent(wrong));
        }
        assert_eq!(
            command.spawn().unwrap_err().raw_os_error(),
            Some(libc::ESRCH)
        );
        assert!(copy(b"invalid PNG".to_vec()).is_err());
        shutdown();
    }
}
