mod broker;
mod client;
mod desktop;
mod receiver;
mod transport;

use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::paths::{self, Xdg};
use crate::pyvalue;

pub use broker::Broker;
pub use client::{Cancel, Client};
pub use desktop::{DesktopOutcome, Notifier, desktop_attempt};
pub use receiver::{Claimed, Receiver, TurnGuard};
pub use transport::{Body, HttpRequest, HttpResponse, Method, Transport, TransportError, Ureq};

pub const TOKEN_ENV: &str = "TOHRU_DISCORD_BOT_TOKEN";
pub const SOCKET_ENV: &str = "TOHRU_DISCORD_SOCKET";
pub const MAX_BODY: usize = 262_144;
pub const MAX_TOOL: usize = 48_000;
const OVERRIDES: [&str; 11] = [
    "enabled",
    "read",
    "post",
    "guild_id",
    "channels",
    "token_file",
    "allowed_user_id",
    "desktop_first",
    "desktop_timeout_seconds",
    "destination",
    "receive",
];

/// Only fixed, nonsecret messages cross the tool boundary.
#[derive(Debug, thiserror::Error)]
pub enum DiscordError {
    #[error("{0}")]
    Fixed(String),
    /// An accessible, revalidated channel no longer contains this exact message.
    #[error(
        "Discord incoming message was deleted; skipped that request, preserving the forward cursor."
    )]
    Deleted { channel: String },
    #[error("Cannot access Discord receiver state; check private state-directory permissions.")]
    Storage(#[source] std::io::Error),
}

impl DiscordError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self::Fixed(message.into())
    }

    pub(crate) fn cancelled() -> Self {
        Self::new("Discord request cancelled.")
    }
}

pub type Values = Map<String, Value>;

/// A Discord ID: one to twenty ASCII digits.
#[must_use]
pub fn is_id(text: &str) -> bool {
    (1..=20).contains(&text.len()) && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// A Discord user ID: an ID without a leading zero.
#[must_use]
pub fn is_user_id(text: &str) -> bool {
    is_id(text) && !text.starts_with('0')
}

pub(crate) fn id_number(text: &str) -> Option<u128> {
    is_id(text).then(|| text.parse().ok()).flatten()
}

fn read_table(path: &Path) -> Result<Option<Values>, DiscordError> {
    let invalid = || DiscordError::new("Cannot read Discord configuration; check config.toml.");
    let mut raw = Vec::new();
    match std::fs::File::open(path) {
        Ok(file) => {
            file.take(1_048_577)
                .read_to_end(&mut raw)
                .map_err(|_| invalid())?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(invalid()),
    }
    if raw.len() > 1_048_576 {
        return Err(invalid());
    }
    let text = String::from_utf8(raw).map_err(|_| invalid())?;
    let table: toml::Table = toml::from_str(&text).map_err(|_| invalid())?;
    let discord = match table.get("assistant") {
        None => None,
        Some(toml::Value::Table(assistant)) => assistant.get("discord"),
        Some(_) => return Err(invalid()),
    };
    match discord {
        None => Ok(Some(Values::new())),
        Some(toml::Value::Table(discord)) => match serde_json::to_value(discord) {
            Ok(Value::Object(values)) => Ok(Some(values)),
            _ => Err(invalid()),
        },
        Some(_) => Err(invalid()),
    }
}

/// The user config and the optional managed overlay that `[assistant.discord]` is read from.
#[derive(Clone, Debug)]
pub struct Sources {
    user: PathBuf,
    managed: Option<PathBuf>,
}

impl Sources {
    #[must_use]
    pub fn new(user: PathBuf, managed: Option<PathBuf>) -> Self {
        Self { user, managed }
    }

    /// `$XDG_CONFIG_HOME/tohru/config.toml` and `TOHRU_MANAGED_CONFIG`.
    #[must_use]
    pub fn from_env() -> Self {
        let managed = std::env::var_os("TOHRU_MANAGED_CONFIG")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        Self::new(paths::xdg(Xdg::Config).join("tohru/config.toml"), managed)
    }

    /// The managed overlay's keys replace the user's.
    pub fn read(&self) -> Result<Values, DiscordError> {
        let mut values = Values::new();
        for path in std::iter::once(&self.user).chain(self.managed.as_ref()) {
            if let Some(table) = read_table(path)? {
                values.extend(table);
            }
        }
        Ok(values)
    }
}

/// `[assistant.discord]` from the environment's config files.
pub fn config() -> Result<Values, DiscordError> {
    Sources::from_env().read()
}

/// Where a turn's settings come from: disk, or native Settings' process-local overrides.
#[derive(Clone, Debug)]
pub enum Loader {
    Disk(Sources),
    /// Overrides on top of the disk settings seen when the turn began; disk changes end the turn.
    Overrides {
        sources: Sources,
        baseline: Values,
        overrides: Values,
    },
    Invalid,
    /// An incoming DM turn: any change from the settings it was claimed under cancels it.
    Guarded {
        inner: Box<Loader>,
        expected: Values,
    },
    #[cfg(test)]
    Shared(std::sync::Arc<std::sync::Mutex<Values>>),
}

impl Loader {
    /// The loader for a turn given native Settings' JSON overrides (empty for none).
    #[must_use]
    pub fn effective(sources: Sources, raw: &str) -> Self {
        if raw.is_empty() {
            return Self::Disk(sources);
        }
        let Ok(Value::Object(overrides)) = serde_json::from_str::<Value>(raw) else {
            return Self::Invalid;
        };
        if overrides
            .keys()
            .any(|key| !OVERRIDES.contains(&key.as_str()))
        {
            return Self::Invalid;
        }
        match sources.read() {
            Ok(baseline) => Self::Overrides {
                sources,
                baseline,
                overrides,
            },
            Err(_) => Self::Invalid,
        }
    }

    pub fn load(&self) -> Result<Values, DiscordError> {
        match self {
            Self::Disk(sources) => sources.read(),
            Self::Invalid => Err(DiscordError::new(
                "Invalid Discord settings; check config.toml.",
            )),
            Self::Overrides {
                sources,
                baseline,
                overrides,
            } => {
                let mut current = sources.read()?;
                if current != *baseline {
                    return Err(DiscordError::new(
                        "Discord configuration changed; start a new assistant turn.",
                    ));
                }
                current.extend(overrides.clone());
                Ok(current)
            }
            Self::Guarded { inner, expected } => {
                let current = inner.load()?;
                if current != *expected {
                    return Err(DiscordError::new(
                        "Discord reception configuration changed; request cancelled.",
                    ));
                }
                Ok(current)
            }
            #[cfg(test)]
            Self::Shared(values) => Ok(values
                .lock()
                .map(|values| values.clone())
                .unwrap_or_default()),
        }
    }
}

fn is_true(value: Option<&Value>) -> bool {
    matches!(value, Some(Value::Bool(true)))
}

pub(crate) fn enabled(values: &Values, name: &str) -> bool {
    is_true(values.get(name))
}

/// The configured guild and allowed channels; a DM destination has no guild and the channel `dm`.
pub fn scope(values: &Values) -> Result<(Option<String>, Vec<String>), DiscordError> {
    if !enabled(values, "enabled") {
        return Err(DiscordError::new("Discord is disabled."));
    }
    desktop_policy(values)?;
    let allowed = values.get("allowed_user_id").and_then(Value::as_str);
    if !allowed.is_some_and(|id| {
        is_user_id(id) && id_number(id).is_some_and(|number| number <= u128::from(u64::MAX))
    }) {
        return Err(DiscordError::new(
            "Set assistant.discord.allowed_user_id to one explicit valid Discord user ID; Discord is unavailable until configured.",
        ));
    }
    if !matches!(values.get("receive"), None | Some(Value::Bool(_))) {
        return Err(DiscordError::new("Discord receive must be Boolean."));
    }
    match values.get("destination") {
        Some(Value::String(destination)) if destination == "dm" => {
            return Ok((None, vec!["dm".to_owned()]));
        }
        None => {}
        Some(Value::String(destination)) if destination == "channel" => {}
        Some(_) => {
            return Err(DiscordError::new(
                "Discord destination must be channel or dm.",
            ));
        }
    }
    let guild = match values.get("guild_id") {
        None => "",
        Some(Value::String(guild)) => guild.as_str(),
        Some(_) => {
            return Err(DiscordError::new(
                "Set assistant.discord.guild_id to a server ID.",
            ));
        }
    };
    if !is_id(guild) {
        return Err(DiscordError::new(
            "Set assistant.discord.guild_id to a server ID.",
        ));
    }
    let channels = match values.get("channels") {
        None => "",
        Some(Value::String(channels)) => channels.as_str(),
        Some(_) => {
            return Err(DiscordError::new(
                "Set assistant.discord.channels to comma-separated channel IDs.",
            ));
        }
    };
    let channels: Vec<String> = channels
        .split(',')
        .map(|item| item.trim().to_owned())
        .collect();
    if !(1..=32).contains(&channels.len()) || channels.iter().any(|item| !is_id(item)) {
        return Err(DiscordError::new(
            "Configure 1–32 allowed Discord channel IDs.",
        ));
    }
    Ok((Some(guild.to_owned()), channels))
}

/// Whether posts first offer a desktop notification, and how long it waits.
pub fn desktop_policy(values: &Values) -> Result<(bool, u64), DiscordError> {
    let enabled = match values.get("desktop_first") {
        None => Some(false),
        Some(Value::Bool(enabled)) => Some(*enabled),
        Some(_) => None,
    };
    let timeout = match values.get("desktop_timeout_seconds") {
        None => Some(60),
        Some(value) => value
            .as_u64()
            .filter(|seconds| (10..=300).contains(seconds)),
    };
    match (enabled, timeout) {
        (Some(enabled), Some(timeout)) => Ok((enabled, timeout)),
        _ => Err(DiscordError::new(
            "Discord desktop_first must be Boolean and desktop_timeout_seconds must be 10–300.",
        )),
    }
}

/// The bot token from a private `token_file`, else `TOHRU_DISCORD_BOT_TOKEN`.
pub fn token(values: &Values) -> Result<String, DiscordError> {
    let path = values.get("token_file");
    let value = match path {
        _ if !pyvalue::truthy(path) => std::env::var(TOKEN_ENV).unwrap_or_default(),
        Some(Value::String(path)) if Path::new(path).is_absolute() => read_token(Path::new(path))?,
        _ => {
            return Err(DiscordError::new(
                "Discord token_file must be an absolute path.",
            ));
        }
    };
    if value.is_empty()
        || value.len() > 4096
        || value.bytes().any(|byte| !(33..=126).contains(&byte))
    {
        return Err(DiscordError::new(
            "Provide a raw bot token in token_file or TOHRU_DISCORD_BOT_TOKEN.",
        ));
    }
    Ok(value)
}

fn read_token(path: &Path) -> Result<String, DiscordError> {
    let unreadable = || DiscordError::new("Cannot read Discord token file.");
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK)
                .bits()
                .cast_signed(),
        )
        .open(path)
        .map_err(|_| unreadable())?;
    let info = file.metadata().map_err(|_| unreadable())?;
    if !info.is_file() || info.uid() != paths::uid() || info.mode() & 0o077 != 0 {
        return Err(DiscordError::new(
            "Discord token file must be owned by you with mode 0600 (or 0400).",
        ));
    }
    let mut raw = Vec::new();
    file.take(4097)
        .read_to_end(&mut raw)
        .map_err(|_| unreadable())?;
    let text = String::from_utf8(raw).map_err(|_| unreadable())?;
    Ok(text.trim().to_owned())
}

/// Never echo the token, even if an upstream body unexpectedly contains it.
pub(crate) fn redact(value: Value, secret: &str) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace(secret, "[redacted]")),
        Value::Array(items) => {
            Value::Array(items.into_iter().map(|item| redact(item, secret)).collect())
        }
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, item)| (key, redact(item, secret)))
                .collect(),
        ),
        other => other,
    }
}

/// Local setup validation; never contacts Discord.
#[must_use]
pub fn status() -> Value {
    status_of(config())
}

/// [`status`] for already-loaded settings, or their load failure.
#[must_use]
pub fn status_of(values: Result<Values, DiscordError>) -> Value {
    match checked_status(values) {
        Ok(value) => value,
        Err(error) => json!({"enabled": false, "status": error.to_string(), "ready": false}),
    }
}

fn checked_status(values: Result<Values, DiscordError>) -> Result<Value, DiscordError> {
    let values = values?;
    if !enabled(&values, "enabled") {
        return Ok(json!({"enabled": false, "status": "disabled"}));
    }
    let (guild, channels) = scope(&values)?;
    let (read, post) = (enabled(&values, "read"), enabled(&values, "post"));
    if !read && !post {
        return Ok(
            json!({"enabled": true, "read": false, "post": false, "status": "capabilities disabled"}),
        );
    }
    token(&values)?;
    let (desktop_first, desktop_timeout) = desktop_policy(&values)?;
    let helper = if crate::probe::which("notify-send").is_some() {
        "available"
    } else {
        "unavailable"
    };
    Ok(
        json!({"enabled": true, "read": read, "post": post, "guild_id": guild, "channels": channels,
              "allowed_user_id": values["allowed_user_id"],
              "destination": values.get("destination").cloned().unwrap_or(json!("channel")),
              "receive": values.get("receive").cloned().unwrap_or(json!(false)),
              "receiver_owner": "Tohru shell assistant controller (local backend only)",
              "desktop_first": desktop_first, "desktop_timeout_seconds": desktop_timeout,
              "desktop_helper": helper, "status": "configured (credentials not verified)"}),
    )
}

/// `$XDG_STATE_HOME/tohru/discord`, shared by every turn and process.
#[must_use]
pub fn state_directory() -> PathBuf {
    paths::xdg(Xdg::State).join("tohru/discord")
}

#[cfg(test)]
mod tests;
