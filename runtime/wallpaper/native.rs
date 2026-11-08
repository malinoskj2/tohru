//! Installed in the pinned runtime as `tohru_wallpaper`; permission dispatch stays native.
#![expect(
    unsafe_code,
    reason = "fcntl sets nonblocking mode on owned probe pipes"
)]
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::platform::SysValue;

const DEADLINE: Duration = Duration::from_secs(5);
const STDERR_LIMIT: usize = 4096;

struct Probe(Child);

impl Drop for Probe {
    fn drop(&mut self) {
        // The fixed sibling probe never spawns descendants. Always kill/reap our
        // own child on timeout/read/parse failure as well as ordinary completion.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn nonblocking(pipe: &impl AsRawFd) -> Result<(), String> {
    // SAFETY: pipe owns a valid descriptor; F_GETFL consumes no pointer argument.
    let flags = unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_GETFL) };
    if flags < 0 {
        return Err(format!(
            "reading wallpaper probe pipe flags: {}",
            io::Error::last_os_error()
        ));
    }
    // SAFETY: the same owned descriptor remains live; flags are from F_GETFL.
    if unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(format!(
            "setting wallpaper probe pipe flags: {}",
            io::Error::last_os_error()
        ));
    }
    Ok(())
}

fn drain(pipe: &mut impl Read, bytes: &mut Vec<u8>, limit: usize) -> Result<bool, String> {
    let mut buffer = [0_u8; 4096];
    loop {
        match pipe.read(&mut buffer) {
            Ok(0) => return Ok(true),
            Ok(n) => {
                if bytes.len().saturating_add(n) > limit {
                    return Err("wallpaper probe response exceeds its byte limit".into());
                }
                bytes.extend_from_slice(&buffer[..n]);
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(format!("reading wallpaper probe response: {error}")),
        }
    }
}

/// Only called for `sys.ask("wallpaper.catalog", absolute_chosen_folder)`.
pub(crate) fn query(name: &str, args: &[SysValue]) -> Result<SysValue, String> {
    let ("wallpaper.catalog", [SysValue::Text(folder)]) = (name, args) else {
        return Err("wallpaper.catalog takes one explicitly chosen absolute folder".into());
    };
    tohru_shell::wallpaper::validate_folder(Path::new(folder)).map_err(|e| e.to_string())?;
    let executable =
        std::env::current_exe().map_err(|e| format!("locating wallpaper probe: {e}"))?;
    let executable = executable
        .parent()
        .ok_or("wallpaper runtime executable has no parent")?
        .join("tohru-wallpaper-probe");
    probe(&executable, folder, DEADLINE)
}

fn probe(executable: &Path, folder: &str, deadline: Duration) -> Result<SysValue, String> {
    let child = Command::new(executable)
        .arg(folder)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("starting packaged wallpaper probe: {e}"))?;
    let mut child = Probe(child);
    let mut stdout = child
        .0
        .stdout
        .take()
        .ok_or("wallpaper probe stdout missing")?;
    let mut stderr = child
        .0
        .stderr
        .take()
        .ok_or("wallpaper probe stderr missing")?;
    nonblocking(&stdout)?;
    nonblocking(&stderr)?;
    let until = Instant::now() + deadline;
    let (mut output, mut errors) = (Vec::new(), Vec::new());
    loop {
        let out_done = drain(
            &mut stdout,
            &mut output,
            tohru_shell::wallpaper::MAX_RESPONSE_BYTES,
        )?;
        let err_done = drain(&mut stderr, &mut errors, STDERR_LIMIT)?;
        if let Some(status) = child
            .0
            .try_wait()
            .map_err(|e| format!("waiting for wallpaper probe: {e}"))?
        {
            if out_done && err_done {
                if !status.success() {
                    return Err(format!(
                        "wallpaper folder probe failed ({status}): {}",
                        String::from_utf8_lossy(&errors).trim()
                    ));
                }
                let files: Vec<String> = serde_json::from_slice(&output)
                    .map_err(|e| format!("decoding wallpaper catalog: {e}"))?;
                if files.len() > tohru_shell::wallpaper::MAX_ENTRIES {
                    return Err("wallpaper catalog exceeds 4096 files".into());
                }
                for file in &files {
                    tohru_shell::wallpaper::validate_folder(Path::new(file))
                        .map_err(|e| e.to_string())?;
                    if tohru_shell::wallpaper::kind(Path::new(file)).is_none() {
                        return Err("wallpaper catalog returned an unsupported media type".into());
                    }
                }
                return Ok(SysValue::List(
                    files.into_iter().map(SysValue::Text).collect(),
                ));
            }
        }
        if Instant::now() >= until {
            return Err("wallpaper folder probe exceeded its five-second deadline".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);

    impl Fixture {
        fn new(source: &str) -> Self {
            let root = std::env::current_dir().unwrap().join(".tmp");
            fs::create_dir_all(&root).unwrap();
            let path = root.join(format!(
                "wallpaper-probe-fixture-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            let program = path.join("probe");
            fs::write(&program, format!("#!/usr/bin/env python3\n{source}\n")).unwrap();
            fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }

        fn program(&self) -> PathBuf {
            self.0.join("probe")
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn query_rejects_wrong_contract_before_spawning() {
        assert!(query("wallpaper.wrong", &[]).is_err());
        assert!(query("wallpaper.catalog", &[SysValue::Text("relative".into())]).is_err());
    }

    #[test]
    fn catalog_transport_validates_real_helper_output_and_failures() {
        let helper = Fixture::new("print('[\"/pictures/a.png\",\"/movies/b.webm\"]')");
        assert!(
            matches!(probe(&helper.program(), "/chosen", Duration::from_secs(1)), Ok(SysValue::List(v)) if v.len() == 2)
        );
        let helper = Fixture::new("print('[\"relative.png\"]')");
        assert!(probe(&helper.program(), "/chosen", Duration::from_secs(1)).is_err());
        let helper = Fixture::new("print('not json')");
        assert!(probe(&helper.program(), "/chosen", Duration::from_secs(1)).is_err());
        let helper =
            Fixture::new("import sys; print('permission denied', file=sys.stderr); sys.exit(1)");
        assert!(
            probe(&helper.program(), "/chosen", Duration::from_secs(1))
                .unwrap_err()
                .contains("permission denied")
        );
    }

    #[test]
    fn timeout_kills_and_reaps_only_the_owned_probe() {
        let helper = Fixture::new(
            "import os, pathlib, time; pathlib.Path(__file__).with_name('pid').write_text(str(os.getpid())); time.sleep(10)",
        );
        let start = Instant::now();
        let error = probe(&helper.program(), "/chosen", Duration::from_millis(150)).unwrap_err();
        assert!(error.contains("deadline"), "{error}");
        assert!(start.elapsed() < Duration::from_secs(2));
        let pid = fs::read_to_string(helper.0.join("pid")).unwrap();
        assert!(
            !Path::new("/proc").join(pid).exists(),
            "probe must be reaped before returning"
        );
    }

    #[test]
    fn oversized_stdout_and_stderr_stop_the_probe() {
        let helper = Fixture::new("import os; os.write(1, b'x' * (1024 * 1024 + 1))");
        assert!(
            probe(&helper.program(), "/chosen", Duration::from_secs(1))
                .unwrap_err()
                .contains("byte limit")
        );
        let helper = Fixture::new("import os; os.write(2, b'x' * 4097)");
        assert!(
            probe(&helper.program(), "/chosen", Duration::from_secs(1))
                .unwrap_err()
                .contains("byte limit")
        );
    }
}
