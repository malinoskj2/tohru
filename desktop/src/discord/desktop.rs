use std::fs::File;
use std::os::unix::fs::FileExt;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use super::client::Cancel;
use super::{DiscordError, SOCKET_ENV, TOKEN_ENV};

const TITLE: &str = "Tohru — acknowledge here to skip Discord";
const OUTPUT_LIMIT: u64 = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopOutcome {
    Acknowledged,
    Unacknowledged,
    Unavailable,
}

impl DesktopOutcome {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Acknowledged => "acknowledged",
            Self::Unacknowledged => "unacknowledged",
            Self::Unavailable => "unavailable",
        }
    }
}

/// Offers `content` on the desktop for `seconds`, rechecking `verify` throughout.
pub type Notifier = fn(
    &str,
    Duration,
    &dyn Fn() -> Result<(), DiscordError>,
    &Cancel,
) -> Result<DesktopOutcome, DiscordError>;

/// `html.escape`: notification bodies are markup.
fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#x27;"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Ends the helper's whole process group: TERM, then KILL after a second.
struct Helper(Child);

impl Drop for Helper {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(None)) {
            return;
        }
        let Some(group) = rustix::process::Pid::from_raw(self.0.id().cast_signed()) else {
            return;
        };
        let _ = rustix::process::kill_process_group(group, rustix::process::Signal::TERM);
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if !matches!(self.0.try_wait(), Ok(None)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
        let _ = self.0.wait();
    }
}

/// A notification submission is not acknowledgement; the helper's lifetime is bounded and owned.
pub fn desktop_attempt(
    content: &str,
    timeout: Duration,
    verify: &dyn Fn() -> Result<(), DiscordError>,
    cancel: &Cancel,
) -> Result<DesktopOutcome, DiscordError> {
    attempt_with(
        crate::probe::which("notify-send"),
        content,
        timeout,
        verify,
        cancel,
    )
}

/// [`desktop_attempt`] with a resolved helper.
pub(crate) fn attempt_with(
    executable: Option<std::path::PathBuf>,
    content: &str,
    timeout: Duration,
    verify: &dyn Fn() -> Result<(), DiscordError>,
    cancel: &Cancel,
) -> Result<DesktopOutcome, DiscordError> {
    verify()?;
    let Some(executable) = executable else {
        return Ok(DesktopOutcome::Unavailable);
    };
    // A regular temporary file bounds memory even if a broken helper floods stdout.
    let Ok(output) = tempfile::tempfile() else {
        return Ok(DesktopOutcome::Unavailable);
    };
    let Ok(stdout) = output.try_clone() else {
        return Ok(DesktopOutcome::Unavailable);
    };
    let spawned = Command::new(executable)
        .args([
            "--app-name=Tohru".to_owned(),
            "--wait".to_owned(),
            "--transient".to_owned(),
            "--action=default=Got it".to_owned(),
            "--action=ack=Got it".to_owned(),
            format!("--expire-time={}", timeout.as_millis()),
            "--".to_owned(),
            TITLE.to_owned(),
            escape(content),
        ])
        .env_remove(TOKEN_ENV)
        .env_remove(SOCKET_ENV)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(Stdio::null())
        .process_group(0)
        .spawn();
    let Ok(child) = spawned else {
        return Ok(DesktopOutcome::Unavailable);
    };
    let mut helper = Helper(child);
    let deadline = Instant::now() + timeout;
    loop {
        verify()?;
        let size = output.metadata().map_or(u64::MAX, |info| info.len());
        if size > OUTPUT_LIMIT {
            return Err(DiscordError::new(
                "Desktop notification returned invalid output; no Discord fallback sent.",
            ));
        }
        let code = helper
            .0
            .try_wait()
            .ok()
            .flatten()
            .map(|status| status.code());
        if matches!(code, None | Some(Some(0))) && acknowledged(&output) {
            return Ok(DesktopOutcome::Acknowledged);
        }
        // Dismissal or expiry is not evidence that the message was read.
        if code.is_some_and(|code| code != Some(0)) {
            return Ok(DesktopOutcome::Unavailable);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(DesktopOutcome::Unacknowledged);
        }
        if cancel.wait(remaining.min(Duration::from_millis(100))) {
            return Err(DiscordError::cancelled());
        }
    }
}

fn acknowledged(output: &File) -> bool {
    let mut buffer = [0; 4096];
    let Ok(read) = output.read_at(&mut buffer, 0) else {
        return false;
    };
    String::from_utf8_lossy(&buffer[..read])
        .lines()
        .any(|action| action == "ack" || action == "default")
}

#[cfg(test)]
mod tests {
    #[test]
    fn escapes_markup_like_python() {
        assert_eq!(
            super::escape("<b>\"A&B'</b>"),
            "&lt;b&gt;&quot;A&amp;B&#x27;&lt;/b&gt;"
        );
    }
}
