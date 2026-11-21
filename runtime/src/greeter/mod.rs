//! Pre-session greeter domain logic: `greetd` IPC, session/user/avatar discovery,
//! read-only `[greeter]` config, and last-choice persistence.
pub mod config;
pub mod discovery;
pub mod ipc;
pub mod state;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use config::GreeterConfig;
use discovery::{SessionEntry, UserAccount};
use state::LastState;

/// Initial immutable environment snapshot handed to the greeter scene at startup.
#[derive(Clone, Debug)]
pub struct GreeterSnapshot {
    pub config: GreeterConfig,
    pub users: Vec<UserAccount>,
    pub sessions: Vec<SessionEntry>,
    pub selected_user: String,
    pub selected_session: String,
    pub hostname: String,
}

impl GreeterSnapshot {
    /// Gather greeter config, users, avatars, sessions, hostname, and initial selection.
    #[must_use]
    pub fn load_live() -> Self {
        let managed = std::env::var_os("TOHRU_MANAGED_CONFIG")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from);
        let state_dir = std::env::var_os("TOHRU_GREETER_STATE_DIR")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from);
        let session_dirs = discovery::default_session_dirs();
        let path_env = std::env::var("PATH").ok();
        let hostname = crate::info::snapshot().system.hostname.unwrap_or_default();

        Self::load_from(
            Path::new("/"),
            managed.as_deref(),
            state_dir.as_deref(),
            &session_dirs,
            path_env.as_deref(),
            hostname,
        )
    }

    /// Deterministic loader for tests and isolated roots.
    #[must_use]
    pub fn load_from(
        root: &Path,
        managed_config: Option<&Path>,
        state_dir: Option<&Path>,
        session_dirs: &[PathBuf],
        path_env: Option<&str>,
        hostname: String,
    ) -> Self {
        let config = GreeterConfig::load(managed_config);
        let last = state_dir.map(state::load).unwrap_or_default();
        let users = discovery::discover_users(root, &config.hidden_users, &config.avatars);
        let sessions = discovery::discover_sessions(session_dirs, path_env);
        let selected_user = pick_initial_user(&config, &last, &users);
        let selected_session = pick_initial_session(&config, &last, &sessions);

        Self {
            config,
            users,
            sessions,
            selected_user,
            selected_session,
            hostname,
        }
    }
}

fn pick_initial_user(config: &GreeterConfig, last: &LastState, users: &[UserAccount]) -> String {
    if let Some(user) = last
        .last_user
        .as_deref()
        .filter(|name| users.iter().any(|u| u.name == *name))
    {
        return user.to_owned();
    }
    if let Some(user) = config
        .default_user
        .as_deref()
        .filter(|name| users.iter().any(|u| u.name == *name) || users.is_empty())
    {
        return user.to_owned();
    }
    users
        .first()
        .map(|user| user.name.clone())
        .unwrap_or_default()
}

fn pick_initial_session(
    config: &GreeterConfig,
    last: &LastState,
    sessions: &[SessionEntry],
) -> String {
    if let Some(id) = last
        .last_session
        .as_deref()
        .filter(|id| sessions.iter().any(|s| s.id == *id))
    {
        return id.to_owned();
    }
    if let Some(id) = config
        .default_session
        .as_deref()
        .filter(|id| sessions.iter().any(|s| s.id == *id))
    {
        return id.to_owned();
    }
    sessions
        .first()
        .map(|session| session.id.clone())
        .unwrap_or_default()
}
