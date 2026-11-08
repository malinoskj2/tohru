//! Layered TOML settings whose managed overrides live outside scene reloads.
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use thiserror::Error;
use toml_edit::{DocumentMut, Item, Table, value};

const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Key {
    PreferredOutput,
    Mooncoil,
    Corners,
}

impl Key {
    pub const ALL: [Self; 3] = [Self::PreferredOutput, Self::Mooncoil, Self::Corners];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::PreferredOutput => "bar.preferred_output",
            Self::Mooncoil => "bar.mooncoil",
            Self::Corners => "appearance.corners",
        }
    }

    /// Parse a supported setting name.
    ///
    /// # Errors
    /// Returns an error for keys not owned by this schema.
    pub fn parse(name: &str) -> Result<Self, Error> {
        Self::ALL
            .into_iter()
            .find(|key| key.name() == name)
            .ok_or_else(|| Error::UnknownKey(name.to_owned()))
    }

    const fn location(self) -> (&'static str, &'static str) {
        match self {
            Self::PreferredOutput => ("bar", "preferred_output"),
            Self::Mooncoil => ("bar", "mooncoil"),
            Self::Corners => ("appearance", "corners"),
        }
    }

    fn default_value(self) -> Value {
        match self {
            Self::PreferredOutput => Value::Text(String::new()),
            Self::Mooncoil => Value::Bool(false),
            Self::Corners => Value::Bool(true),
        }
    }

    fn validate(self, candidate: &Value) -> Result<(), Error> {
        match (self, candidate) {
            (Self::PreferredOutput, Value::Text(text))
                if text.len() <= 256 && !text.chars().any(char::is_control) =>
            {
                Ok(())
            }
            (Self::Mooncoil | Self::Corners, Value::Bool(_)) => Ok(()),
            _ => Err(Error::InvalidValue(self.name())),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    Text(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Default,
    User,
    Nix,
    Runtime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Setting {
    pub key: Key,
    pub value: Value,
    pub origin: Origin,
    pub managed: bool,
    pub runtime_override: bool,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("unknown setting {0}")]
    UnknownKey(String),
    #[error("invalid type or value for {0}")]
    InvalidValue(&'static str),
    #[error("cannot read settings at {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid TOML settings at {path}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml_edit::TomlError,
    },
    #[error("settings file exceeds 1 MiB: {0}")]
    TooLarge(PathBuf),
    #[error("cannot persist settings at {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("writable and managed settings must be different files")]
    SameFile,
    #[error("cannot persist through a symbolic link: {0}")]
    Symlink(PathBuf),
}

/// A successful mutation distinguishes temporary state from disk commitment.
#[derive(Debug)]
pub enum SetOutcome {
    RuntimeOverride,
    Persisted,
    CommittedDurabilityUnconfirmed(DurabilityWarning),
}

#[derive(Debug, Error)]
#[error(
    "settings committed at {path}, but directory synchronization failed; crash durability is unconfirmed"
)]
pub struct DurabilityWarning {
    pub path: PathBuf,
    #[source]
    pub source: std::io::Error,
}

struct Layer {
    document: DocumentMut,
    values: BTreeMap<Key, Value>,
}

impl Layer {
    fn read(path: &Path, optional: bool) -> Result<Self, Error> {
        let mut bytes = String::new();
        match File::open(path) {
            Ok(file) => {
                file.take(MAX_CONFIG_BYTES + 1)
                    .read_to_string(&mut bytes)
                    .map_err(|source| Error::Read {
                        path: path.to_owned(),
                        source,
                    })?;
                if bytes.len() as u64 > MAX_CONFIG_BYTES {
                    return Err(Error::TooLarge(path.to_owned()));
                }
            }
            Err(error) if optional && error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(Error::Read {
                    path: path.to_owned(),
                    source,
                });
            }
        }
        let document = bytes
            .parse::<DocumentMut>()
            .map_err(|source| Error::Parse {
                path: path.to_owned(),
                source,
            })?;
        let mut values = BTreeMap::new();
        for key in Key::ALL {
            let (section, field) = key.location();
            let Some(table) = document.get(section) else {
                continue;
            };
            let Some(table) = table.as_table_like() else {
                return Err(Error::InvalidValue(key.name()));
            };
            let Some(item) = table.get(field) else {
                continue;
            };
            let candidate = if let Some(text) = item.as_str() {
                Value::Text(text.to_owned())
            } else if let Some(boolean) = item.as_bool() {
                Value::Bool(boolean)
            } else {
                return Err(Error::InvalidValue(key.name()));
            };
            key.validate(&candidate)?;
            values.insert(key, candidate);
        }
        Ok(Self { document, values })
    }
}

/// Exactly one owner is retained by the shell process, independently of Luau.
pub struct Owner {
    user_path: PathBuf,
    managed_path: Option<PathBuf>,
    user: Layer,
    managed: BTreeMap<Key, Value>,
    overrides: BTreeMap<Key, Value>,
}

impl Owner {
    /// Load user and optional managed layers without importing old shell state.
    ///
    /// # Errors
    /// Rejects invalid, oversized, unreadable or aliased configuration files.
    pub fn load(user_path: PathBuf, managed_path: Option<PathBuf>) -> Result<Self, Error> {
        if let Some(managed) = &managed_path {
            if managed == &user_path
                || matches!((fs::canonicalize(managed), fs::canonicalize(&user_path)), (Ok(a), Ok(b)) if a == b)
            {
                return Err(Error::SameFile);
            }
        }
        let user = Layer::read(&user_path, true)?;
        let managed = managed_path
            .as_ref()
            .map(|path| Layer::read(path, false))
            .transpose()?
            .map_or_else(BTreeMap::new, |layer| layer.values);
        Ok(Self {
            user_path,
            managed_path,
            user,
            managed,
            overrides: BTreeMap::new(),
        })
    }

    #[must_use]
    pub fn get(&self, key: Key) -> Setting {
        let managed = self.managed.contains_key(&key);
        let (value, origin) = if let Some(value) = self.overrides.get(&key) {
            (value.clone(), Origin::Runtime)
        } else if let Some(value) = self.managed.get(&key) {
            (value.clone(), Origin::Nix)
        } else if let Some(value) = self.user.values.get(&key) {
            (value.clone(), Origin::User)
        } else {
            (key.default_value(), Origin::Default)
        };
        Setting {
            key,
            value,
            origin,
            managed,
            runtime_override: self.overrides.contains_key(&key),
        }
    }

    /// Reload both layers atomically while retaining process overrides.
    ///
    /// # Errors
    /// Leaves the last valid state intact if either layer is invalid.
    pub fn reload(&mut self) -> Result<(), Error> {
        let next = Self::load(self.user_path.clone(), self.managed_path.clone())?;
        self.user = next.user;
        self.managed = next.managed;
        Ok(())
    }

    /// Apply a setting, persisting only an unmanaged edit.
    ///
    /// # Errors
    /// Rejects invalid values and pre-commit write failures without changing state.
    /// A post-rename sync failure returns a committed outcome and updates the cache.
    pub fn set(&mut self, key: Key, candidate: Value) -> Result<SetOutcome, Error> {
        self.set_with_sync(key, candidate, |directory| {
            File::open(directory)?.sync_all()
        })
    }

    fn set_with_sync(
        &mut self,
        key: Key,
        candidate: Value,
        sync_directory: impl FnOnce(&Path) -> std::io::Result<()>,
    ) -> Result<SetOutcome, Error> {
        key.validate(&candidate)?;
        if self.managed.contains_key(&key) || self.overrides.contains_key(&key) {
            self.overrides.insert(key, candidate);
            return Ok(SetOutcome::RuntimeOverride);
        }
        // Read anew so external edits and unknown keys are not overwritten.
        let mut next = Layer::read(&self.user_path, true)?;
        let (section, field) = key.location();
        if !next.document.contains_key(section) {
            next.document[section] = Item::Table(Table::new());
        }
        let table = next
            .document
            .get_mut(section)
            .and_then(Item::as_table_like_mut)
            .ok_or(Error::InvalidValue(key.name()))?;
        table.insert(
            field,
            match &candidate {
                Value::Bool(boolean) => value(*boolean),
                Value::Text(text) => value(text),
            },
        );
        let serialized = next.document.to_string();
        if serialized.len() as u64 > MAX_CONFIG_BYTES {
            return Err(Error::TooLarge(self.user_path.clone()));
        }
        let outcome = persist(&self.user_path, serialized.as_bytes(), sync_directory)?;
        // Returning from persist means rename committed, even if durability is unknown.
        next.values.insert(key, candidate);
        self.user = next;
        Ok(outcome)
    }
}

fn persist(
    path: &Path,
    bytes: &[u8],
    sync_directory: impl FnOnce(&Path) -> std::io::Result<()>,
) -> Result<SetOutcome, Error> {
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(Error::Symlink(path.to_owned()));
    }
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let write = || -> std::io::Result<()> {
        fs::create_dir_all(directory)?;
        // create_new keeps concurrent attempts from sharing a temporary file.
        let temporary = directory.join(format!(".tohru-settings-{}.tmp", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        let result = (|| {
            file.write_all(bytes)?;
            file.sync_all()?;
            // Commit point: later failures cannot roll back the new destination.
            fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    };
    write().map_err(|source| Error::Write {
        path: path.to_owned(),
        source,
    })?;
    Ok(match sync_directory(directory) {
        Ok(()) => SetOutcome::Persisted,
        Err(source) => SetOutcome::CommittedDurabilityUnconfirmed(DurabilityWarning {
            path: path.to_owned(),
            source,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postcommit_sync_failure_updates_memory_and_reports_uncertain_durability() {
        let directory =
            std::env::temp_dir().join(format!("tohru-postcommit-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("config.toml");
        fs::write(&path, "# preserved\n[future]\nvalue = 42\n").unwrap();
        let mut owner = Owner::load(path.clone(), None).unwrap();
        let outcome = owner
            .set_with_sync(Key::Mooncoil, Value::Bool(true), |parent| {
                assert_eq!(parent, directory);
                // Fault occurs after the real file sync/rename, without replacing those effects.
                assert!(
                    fs::read_to_string(&path)
                        .unwrap()
                        .contains("mooncoil = true")
                );
                Err(std::io::Error::other("injected directory fsync failure"))
            })
            .unwrap();
        let SetOutcome::CommittedDurabilityUnconfirmed(warning) = outcome else {
            panic!("committed write must report uncertain crash durability");
        };
        assert_eq!(warning.path, path);
        assert!(warning.to_string().contains("settings committed"));
        assert!(
            warning
                .to_string()
                .contains("crash durability is unconfirmed")
        );
        assert_eq!(
            std::error::Error::source(&warning).unwrap().to_string(),
            "injected directory fsync failure"
        );
        assert_eq!(owner.get(Key::Mooncoil).value, Value::Bool(true));
        assert_eq!(owner.get(Key::Mooncoil).origin, Origin::User);
        let restarted = Owner::load(path.clone(), None).unwrap();
        assert_eq!(owner.get(Key::Mooncoil), restarted.get(Key::Mooncoil));
        assert!(fs::read_to_string(path).unwrap().contains("# preserved"));
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        assert!(matches!(
            owner.set(Key::Mooncoil, Value::Bool(false)).unwrap(),
            SetOutcome::Persisted
        ));
        fs::remove_dir_all(directory).unwrap();
    }
}
