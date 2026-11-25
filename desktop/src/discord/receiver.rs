use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rustix::fs::FlockOperation;
use serde_json::{Value, json};

use super::client::{Client, private_directory};
use super::{DiscordError, Values, enabled, id_number, is_id, is_user_id, scope};
use crate::{paths, pyjson, pyvalue};

const STATE: &str = "receiver.json";
const STATE_LIMIT: u64 = 16_000;
/// Discord's epoch, in Unix milliseconds.
const DISCORD_EPOCH: u64 = 1_420_070_400_000;

fn busy() -> DiscordError {
    DiscordError::new("Discord receiver is busy; pending request retained.")
}

fn invalid_state() -> DiscordError {
    DiscordError::new(
        "Discord receiver state is invalid; disable/re-enable receive to bootstrap safely.",
    )
}

/// An exclusive `flock` held until drop.
#[derive(Debug)]
pub struct TurnGuard(File);

impl Drop for TurnGuard {
    fn drop(&mut self) {
        let _ = rustix::fs::flock(&self.0, FlockOperation::Unlock);
    }
}

/// A claimed incoming request: the fresh message, its conversation key and the settings it was read under.
#[derive(Debug)]
pub struct Claimed {
    pub message: Value,
    pub conversation: String,
    pub values: Values,
}

fn integer(value: &Value) -> u128 {
    value.as_str().and_then(id_number).unwrap_or(0)
}

/// One pending request, owned and polled by the shell; no provider or token crosses IPC.
pub struct Receiver {
    client: Client,
    path: PathBuf,
}

impl Receiver {
    #[must_use]
    pub fn new(client: Client) -> Self {
        let path = client.state().join(STATE);
        Self { client, path }
    }

    fn open_lock(&self, name: &str) -> Result<File, DiscordError> {
        private_directory(self.client.state())?;
        OpenOptions::new()
            .append(true)
            .create(true)
            .open(self.client.state().join(name))
            .map_err(DiscordError::Storage)
    }

    fn locked(&self, wait: Duration) -> Result<TurnGuard, DiscordError> {
        let file = self.open_lock("receiver.lock")?;
        let deadline = Instant::now() + wait;
        loop {
            match rustix::fs::flock(&file, FlockOperation::NonBlockingLockExclusive) {
                Ok(()) => return Ok(TurnGuard(file)),
                Err(rustix::io::Errno::WOULDBLOCK) => {
                    if Instant::now() >= deadline {
                        return Err(busy());
                    }
                    self.client.pause(0.05, deadline)?;
                }
                Err(error) => return Err(DiscordError::Storage(error.into())),
            }
        }
    }

    pub fn reset(&self) -> Result<(), DiscordError> {
        let _guard = self.locked(Duration::from_secs(15))?;
        match std::fs::remove_file(&self.path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                Err(DiscordError::Storage(error))
            }
            _ => Ok(()),
        }
    }

    /// Keeps multi-output shells from running two incoming turns concurrently.
    pub fn turn(&self) -> Result<TurnGuard, DiscordError> {
        let file = self.open_lock("receiver-turn.lock")?;
        match rustix::fs::flock(&file, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => Ok(TurnGuard(file)),
            Err(rustix::io::Errno::WOULDBLOCK) => Err(busy()),
            Err(error) => Err(DiscordError::Storage(error.into())),
        }
    }

    fn settings(&self) -> Result<Values, DiscordError> {
        let values = self.client.load()?;
        scope(&values)?;
        let direct = values
            .get("destination")
            .is_some_and(|destination| destination == "dm");
        if !direct || !enabled(&values, "receive") || !enabled(&values, "read") {
            return Err(DiscordError::new(
                "Discord reception requires destination=dm, enabled, read and receive.",
            ));
        }
        Ok(values)
    }

    /// `json.dumps(values, sort_keys=True)`'s digest, so a settings change rebootstraps.
    pub(crate) fn fingerprint(values: &Values) -> String {
        let style = pyjson::Style {
            ascii: true,
            sort_keys: true,
            compact: false,
        };
        paths::sha256_hex(pyjson::render(&Value::Object(values.clone()), style).as_bytes())
    }

    fn read_state(&self, values: &Values) -> Result<Values, DiscordError> {
        let fingerprint = Self::fingerprint(values);
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Values::from_iter([(
                    "scope".to_owned(),
                    json!(fingerprint),
                )]));
            }
            Err(_) => return Err(invalid_state()),
        };
        let mut raw = Vec::new();
        file.take(STATE_LIMIT + 1)
            .read_to_end(&mut raw)
            .map_err(|_| invalid_state())?;
        if raw.len() as u64 > STATE_LIMIT {
            return Err(invalid_state());
        }
        let Ok(Value::Object(state)) = serde_json::from_slice::<Value>(&raw) else {
            return Err(invalid_state());
        };
        if state.get("scope").and_then(Value::as_str) != Some(fingerprint.as_str()) {
            return Ok(Values::from_iter([(
                "scope".to_owned(),
                json!(fingerprint),
            )]));
        }
        let Some(cursor) = state
            .get("cursor")
            .and_then(Value::as_str)
            .filter(|cursor| is_id(cursor))
        else {
            return Err(invalid_state());
        };
        if let Some(pending) = state.get("pending").filter(|pending| !pending.is_null()) {
            let valid = pending.get("id").and_then(Value::as_str) == Some(cursor)
                && pyvalue::same(pending.get("author"), values.get("allowed_user_id"))
                && pending
                    .get("content")
                    .and_then(Value::as_str)
                    .is_some_and(|content| content.chars().count() <= 2000)
                && pending.get("timestamp").is_some_and(Value::is_string)
                && state
                    .get("channel")
                    .and_then(Value::as_str)
                    .is_some_and(is_user_id);
            if !valid {
                return Err(invalid_state());
            }
        }
        Ok(state)
    }

    fn save(&self, state: &Values, expected: &Values) -> Result<(), DiscordError> {
        if self.settings()? != *expected {
            return Err(DiscordError::new(
                "Discord reception configuration changed.",
            ));
        }
        let directory = self.client.state();
        let mut file = tempfile::Builder::new()
            .prefix("receiver-")
            .tempfile_in(directory)
            .map_err(DiscordError::Storage)?;
        file.write_all(pyjson::dumps(&Value::Object(state.clone())).as_bytes())
            .and_then(|()| file.as_file().sync_all())
            .map_err(DiscordError::Storage)?;
        file.persist(&self.path)
            .map_err(|error| DiscordError::Storage(error.error))?;
        File::open(directory)
            .and_then(|directory| directory.sync_all())
            .map_err(DiscordError::Storage)
    }

    fn incoming(state: &Values) -> Value {
        match state
            .get("pending")
            .filter(|pending| pyvalue::truthy(Some(pending)))
        {
            Some(pending) => json!({"event": "incoming", "id": pending["id"]}),
            None => json!({"event": "idle"}),
        }
    }

    /// Bootstraps the forward cursor, or claims the next text-bearing DM into the pending slot.
    pub fn poll(&mut self) -> Result<Value, DiscordError> {
        let _guard = self.locked(Duration::ZERO)?;
        let values = self.settings()?;
        let mut state = self.read_state(&values)?;
        if pyvalue::truthy(state.get("pending")) {
            return Ok(Self::incoming(&state));
        }
        let cursor = state
            .get("cursor")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let mut request = json!({"action": "read", "channel": "dm", "limit": if cursor.is_some() { 50 } else { 1 }});
        if let Some(cursor) = &cursor {
            request["after"] = json!(cursor);
        }
        let page = self.client.execute(&request)?;
        let next_after = page
            .get("next_after")
            .and_then(Value::as_str)
            .filter(|next| !next.is_empty());
        match cursor {
            None => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |elapsed| elapsed.as_millis());
                let snowflake = (now.saturating_sub(u128::from(DISCORD_EPOCH))) << 22;
                state.insert(
                    "cursor".to_owned(),
                    json!(next_after.map_or_else(|| snowflake.to_string(), str::to_owned)),
                );
            }
            Some(cursor) => {
                let mut messages: Vec<&Value> = page["messages"]
                    .as_array()
                    .map(|items| items.iter().collect())
                    .unwrap_or_default();
                messages.sort_by_key(|message| integer(&message["id"]));
                let current = id_number(&cursor).unwrap_or(0);
                let next = messages.into_iter().find(|message| {
                    integer(&message["id"]) > current
                        && message["content"]
                            .as_str()
                            .is_some_and(|content| !content.trim().is_empty())
                });
                if let Some(message) = next {
                    state.insert("cursor".to_owned(), message["id"].clone());
                    state.insert("pending".to_owned(), message.clone());
                    state.insert("channel".to_owned(), page["channel"].clone());
                } else {
                    state.insert("cursor".to_owned(), json!(next_after.unwrap_or(&cursor)));
                }
            }
        }
        self.save(&state, &values)?;
        Ok(Self::incoming(&state))
    }

    /// Claims the pending request `ident`, rechecking its channel, author and current content.
    pub fn claim(&mut self, ident: &str) -> Result<Claimed, DiscordError> {
        let _guard = self.locked(Duration::ZERO)?;
        let values = self.settings()?;
        let mut state = self.read_state(&values)?;
        let Some(message) = state
            .get("pending")
            .filter(|pending| pending.get("id").and_then(Value::as_str) == Some(ident))
            .cloned()
        else {
            return Err(DiscordError::new(
                "Discord incoming request is no longer pending.",
            ));
        };
        let saved_channel = state.get("channel").cloned();
        let request = json!({"action": "read", "channel": "dm", "limit": 1, "message_id": ident});
        let page = match self.client.execute(&request) {
            Err(DiscordError::Deleted { channel }) => {
                if saved_channel.as_ref().and_then(Value::as_str) != Some(channel.as_str()) {
                    return Err(DiscordError::new(
                        "Discord incoming channel changed; pending request retained.",
                    ));
                }
                state.remove("pending");
                state.insert(
                    "last_status".to_owned(),
                    json!({"id": ident, "status": "deleted"}),
                );
                self.save(&state, &values)?;
                return Err(DiscordError::Deleted { channel });
            }
            result => result?,
        };
        let changed = || DiscordError::new("Discord incoming request changed or is inaccessible.");
        let messages = page["messages"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let [fresh] = messages else {
            return Err(changed());
        };
        if Some(&page["channel"]) != saved_channel.as_ref() {
            return Err(changed());
        }
        if fresh["id"].as_str() != Some(ident)
            || !pyvalue::same(Some(&fresh["author"]), values.get("allowed_user_id"))
        {
            return Err(changed());
        }
        state.remove("pending");
        if fresh["content"]
            .as_str()
            .is_none_or(|content| content.trim().is_empty())
        {
            state.insert(
                "last_status".to_owned(),
                json!({"id": ident, "status": "empty after edit"}),
            );
            self.save(&state, &values)?;
            return Err(DiscordError::new(
                "Discord incoming message now has no text; skipped that request, preserving the forward cursor.",
            ));
        }
        if *fresh != message {
            state.insert(
                "last_status".to_owned(),
                json!({"id": ident, "status": "refreshed after edit"}),
            );
        }
        self.save(&state, &values)?;
        let channel = page["channel"].as_str().unwrap_or_default();
        let user = values
            .get("allowed_user_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let conversation =
            paths::sha256_hex(format!("{channel}:{user}").as_bytes())[..32].to_owned();
        Ok(Claimed {
            message: fresh.clone(),
            conversation,
            values,
        })
    }
}
