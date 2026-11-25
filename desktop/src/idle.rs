use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::os::unix::process::CommandExt;
use std::process::Command;

use crate::afk::{self, AfkError};
use crate::config::{Config, ConfigError, IdleOwner};
use crate::paths::{self, SessionError};
use crate::shell;

const MANAGERS: [&str; 4] = ["hypridle", "swayidle", "wayidle", "xidlehook"];
const OWNER_HOOKS: [&str; 4] = ["lock", "before-sleep", "after-resume", "unlock"];

#[derive(Debug, thiserror::Error)]
pub enum IdleError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error(transparent)]
    Afk(#[from] AfkError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("idle.suspend_seconds must exceed idle.lock_seconds")]
    Order,
    #[error("another idle manager is running: {0}; stop it or keep idle.owner=external")]
    Conflict(String),
}

/// Whether a swayidle command line is Tohru's passive AFK observer, which owns no lock or sleep.
#[must_use]
pub fn passive_observer(arguments: &[&str]) -> bool {
    arguments.iter().any(|arg| arg.contains(afk::PROGRAM))
        && !arguments.iter().any(|arg| OWNER_HOOKS.contains(arg))
}

/// This user's other idle managers, by process name.
#[must_use]
pub fn conflicting_owners() -> Vec<String> {
    let mut names = Vec::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return names;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !entry
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|byte| byte.is_ascii_digit())
        {
            continue;
        }
        if std::fs::metadata(&path).map_or(true, |info| info.uid() != paths::uid()) {
            continue;
        }
        let Ok(name) = std::fs::read_to_string(path.join("comm")) else {
            continue;
        };
        let name = name.trim();
        if name == "swayidle" {
            let Ok(raw) = std::fs::read(path.join("cmdline")) else {
                continue;
            };
            let text = String::from_utf8_lossy(&raw);
            let arguments: Vec<&str> = text.split('\0').collect();
            if passive_observer(&arguments) {
                continue;
            }
        }
        if MANAGERS.contains(&name) {
            names.push(name.to_owned());
        }
    }
    names.sort();
    names.dedup();
    names
}

/// The swayidle command line Tohru owns: confirmed locks before sleep and on timeouts.
pub fn command(config: &Config) -> Result<Vec<String>, IdleError> {
    let action = shell::quote(
        &paths::bin_dir()
            .join("tohru-session-action")
            .to_string_lossy(),
    );
    let lock = format!("{action} lock");
    let sleep = format!("{action} sleep");
    let mut args: Vec<String> = ["swayidle", "-w", "lock", &lock, "before-sleep", &lock]
        .map(str::to_owned)
        .into();
    args.extend(afk::owned_callbacks(config)?);
    if config.idle_lock_seconds > 0 {
        args.extend([
            "timeout".to_owned(),
            config.idle_lock_seconds.to_string(),
            lock.clone(),
        ]);
    }
    if config.idle_suspend_seconds > 0 {
        if config.idle_lock_seconds > 0 && config.idle_suspend_seconds <= config.idle_lock_seconds {
            return Err(IdleError::Order);
        }
        args.extend([
            "timeout".to_owned(),
            config.idle_suspend_seconds.to_string(),
            sleep,
        ]);
    }
    Ok(args)
}

/// Becomes the session's swayidle, holding the idle lock across `exec`.
///
/// The foreground swayidle owns the logind delay inhibitor; `-w` waits for the confirmed-lock
/// helper before releasing it, bounded by logind's delay limit.
pub fn run() -> Result<(), IdleError> {
    let config = Config::load()?;
    if config.idle_owner != IdleOwner::Tohru {
        println!("tohru-idle: idle ownership is {}", config.idle_owner.name());
        return Ok(());
    }
    let directory = paths::session_directory()?;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)?;
    let lock = std::fs::File::create(directory.join("idle.lock"))?;
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)
        .map_err(std::io::Error::from)?;
    let owners = conflicting_owners();
    if !owners.is_empty() {
        return Err(IdleError::Conflict(owners.join(", ")));
    }
    rustix::io::fcntl_setfd(&lock, rustix::io::FdFlags::empty()).map_err(std::io::Error::from)?;
    let args = command(&config)?;
    let error = Command::new(&args[0]).args(&args[1..]).exec();
    drop(lock);
    Err(error.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passive_afk_observer_is_not_a_competing_owner() {
        let callback = "/bin/tohru-afk-idle notify idle --generation 4";
        assert!(passive_observer(&[
            "swayidle",
            "-C",
            "/dev/null",
            "timeout",
            "300",
            callback
        ]));
        assert!(!passive_observer(&[
            "swayidle", "-w", "lock", "x", "timeout", "300", callback
        ]));
        assert!(!passive_observer(&["swayidle", "timeout", "300", "true"]));
    }
}
