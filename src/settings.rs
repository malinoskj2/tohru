//! Layered TOML settings whose managed overrides live outside scene reloads.
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use thiserror::Error;
use toml_edit::{DocumentMut, Item, Table, value};

use crate::wallpaper::preset::{Palette, Preset};

const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Key {
    PreferredOutput,
    Mooncoil,
    Corners,
    CornerSize,
    WallpaperEnabled,
    WallpaperAssignments,
    WallpaperFolder,
    WallpaperFit,
    WallpaperPreset,
    WallpaperChange,
    Palette,
    CaptureSave,
    CaptureCopy,
    CaptureDirectory,
    CaptureFilenamePattern,
    IdleLockSeconds,
    IdleScreenOffUnlockedSeconds,
    IdleScreenOffLockedSeconds,
    IdleSuspendUnlockedSeconds,
    IdleSuspendLockedSeconds,
    IdleFadeMilliseconds,
}

impl Key {
    pub const ALL: [Self; 21] = [
        Self::PreferredOutput,
        Self::Mooncoil,
        Self::Corners,
        Self::CornerSize,
        Self::WallpaperEnabled,
        Self::WallpaperAssignments,
        Self::WallpaperFolder,
        Self::WallpaperFit,
        Self::WallpaperPreset,
        Self::WallpaperChange,
        Self::Palette,
        Self::CaptureSave,
        Self::CaptureCopy,
        Self::CaptureDirectory,
        Self::CaptureFilenamePattern,
        Self::IdleLockSeconds,
        Self::IdleScreenOffUnlockedSeconds,
        Self::IdleScreenOffLockedSeconds,
        Self::IdleSuspendUnlockedSeconds,
        Self::IdleSuspendLockedSeconds,
        Self::IdleFadeMilliseconds,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::PreferredOutput => "bar.preferred_output",
            Self::Mooncoil => "bar.mooncoil",
            Self::Corners => "appearance.corners",
            Self::CornerSize => "appearance.corner_size",
            Self::WallpaperEnabled => "wallpaper.enabled",
            Self::WallpaperAssignments => "wallpaper.assignments",
            Self::WallpaperFolder => "wallpaper.folder",
            Self::WallpaperFit => "wallpaper.fit",
            Self::WallpaperPreset => "wallpaper.preset",
            Self::WallpaperChange => "wallpaper.change",
            Self::Palette => "appearance.palette",
            Self::CaptureSave => "capture.save",
            Self::CaptureCopy => "capture.copy",
            Self::CaptureDirectory => "capture.directory",
            Self::CaptureFilenamePattern => "capture.filename_pattern",
            Self::IdleLockSeconds => "idle.lock_seconds",
            Self::IdleScreenOffUnlockedSeconds => "idle.screen_off_unlocked_seconds",
            Self::IdleScreenOffLockedSeconds => "idle.screen_off_locked_seconds",
            Self::IdleSuspendUnlockedSeconds => "idle.suspend_unlocked_seconds",
            Self::IdleSuspendLockedSeconds => "idle.suspend_locked_seconds",
            Self::IdleFadeMilliseconds => "idle.fade_milliseconds",
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

    /// Convert canonical string input at a TOML/Lua boundary.
    ///
    /// # Errors
    /// Rejects unknown closed-domain names; `Owner` validates other text bounds.
    pub fn parse_text(self, text: &str) -> Result<Value, Error> {
        match self {
            Self::WallpaperFit => WallpaperFit::parse(text).map(Value::WallpaperFit),
            Self::WallpaperPreset => text
                .parse::<Preset>()
                .map(|preset| Value::WallpaperPreset(Some(preset)))
                .map_err(|_| Error::InvalidValue(self.name())),
            Self::WallpaperChange => WallpaperChange::parse(text).map(Value::WallpaperChange),
            Self::Palette => text
                .parse::<Palette>()
                .map(Value::Palette)
                .map_err(|_| Error::InvalidValue(self.name())),
            _ => Ok(Value::Text(text.to_owned())),
        }
    }

    const fn location(self) -> (&'static str, &'static str) {
        match self {
            Self::PreferredOutput => ("bar", "preferred_output"),
            Self::Mooncoil => ("bar", "mooncoil"),
            Self::Corners => ("appearance", "corners"),
            Self::CornerSize => ("appearance", "corner_size"),
            Self::WallpaperEnabled => ("wallpaper", "enabled"),
            Self::WallpaperAssignments => ("wallpaper", "assignments"),
            Self::WallpaperFolder => ("wallpaper", "folder"),
            Self::WallpaperFit => ("wallpaper", "fit"),
            Self::WallpaperPreset => ("wallpaper", "preset"),
            Self::WallpaperChange => ("wallpaper", "change"),
            Self::Palette => ("appearance", "palette"),
            Self::CaptureSave => ("capture", "save"),
            Self::CaptureCopy => ("capture", "copy"),
            Self::CaptureDirectory => ("capture", "directory"),
            Self::CaptureFilenamePattern => ("capture", "filename_pattern"),
            Self::IdleLockSeconds => ("idle", "lock_seconds"),
            Self::IdleScreenOffUnlockedSeconds => ("idle", "screen_off_unlocked_seconds"),
            Self::IdleScreenOffLockedSeconds => ("idle", "screen_off_locked_seconds"),
            Self::IdleSuspendUnlockedSeconds => ("idle", "suspend_unlocked_seconds"),
            Self::IdleSuspendLockedSeconds => ("idle", "suspend_locked_seconds"),
            Self::IdleFadeMilliseconds => ("idle", "fade_milliseconds"),
        }
    }

    #[must_use]
    pub const fn integer_bounds(self) -> Option<(i64, i64)> {
        match self {
            Self::CornerSize => Some((1, 100)),
            Self::IdleFadeMilliseconds => Some((0, 120_000)),
            Self::IdleLockSeconds
            | Self::IdleScreenOffUnlockedSeconds
            | Self::IdleScreenOffLockedSeconds
            | Self::IdleSuspendUnlockedSeconds
            | Self::IdleSuspendLockedSeconds => Some((0, 4_294_967)),
            _ => None,
        }
    }

    fn default_value(self) -> Value {
        match self {
            Self::PreferredOutput
            | Self::WallpaperFolder
            | Self::CaptureDirectory
            | Self::CaptureFilenamePattern => Value::Text(String::new()),
            Self::Mooncoil | Self::WallpaperEnabled => Value::Bool(false),
            Self::Corners | Self::CaptureSave | Self::CaptureCopy => Value::Bool(true),
            Self::CornerSize => Value::Integer(32),
            Self::WallpaperAssignments => Value::Assignments(BTreeMap::new()),
            Self::WallpaperFit => Value::WallpaperFit(WallpaperFit::Fill),
            Self::WallpaperPreset => Value::WallpaperPreset(None),
            Self::WallpaperChange => Value::WallpaperChange(WallpaperChange::Never),
            Self::Palette => Value::Palette(Palette::Mocha),
            Self::IdleLockSeconds => Value::Integer(1800),
            Self::IdleScreenOffUnlockedSeconds => Value::Integer(0),
            Self::IdleScreenOffLockedSeconds => Value::Integer(60),
            Self::IdleSuspendUnlockedSeconds => Value::Integer(7200),
            Self::IdleSuspendLockedSeconds => Value::Integer(5400),
            Self::IdleFadeMilliseconds => Value::Integer(2000),
        }
    }

    fn validate(self, candidate: &Value) -> Result<(), Error> {
        match (self, candidate) {
            (Self::PreferredOutput, Value::Text(text))
                if text.len() <= 256 && !text.chars().any(char::is_control) =>
            {
                Ok(())
            }
            (
                Self::Mooncoil
                | Self::Corners
                | Self::WallpaperEnabled
                | Self::CaptureSave
                | Self::CaptureCopy,
                Value::Bool(_),
            ) => Ok(()),
            (Self::WallpaperFolder, Value::Text(text))
                if text.len() <= 4096 && !text.contains('\0') =>
            {
                Ok(())
            }
            (Self::WallpaperFit, Value::WallpaperFit(_))
            | (Self::WallpaperPreset, Value::WallpaperPreset(_))
            | (Self::WallpaperChange, Value::WallpaperChange(_))
            | (Self::Palette, Value::Palette(_)) => Ok(()),
            (Self::CaptureDirectory, Value::Text(text)) if valid_capture_directory(text) => Ok(()),
            (Self::CaptureFilenamePattern, Value::Text(text))
                if text.len() <= 1024 && !text.contains(['/', '\\', '\0']) =>
            {
                Ok(())
            }
            (Self::WallpaperAssignments, Value::Assignments(assignments))
                if assignments.len() <= 32
                    && assignments
                        .iter()
                        .all(|(output, path)| valid_output(output) && valid_wallpaper(path)) =>
            {
                Ok(())
            }
            (_, Value::Integer(value))
                if self
                    .integer_bounds()
                    .is_some_and(|(min, max)| (min..=max).contains(value)) =>
            {
                Ok(())
            }
            _ => Err(Error::InvalidValue(self.name())),
        }
    }
}

/// Static wallpaper image placement, parsed at configuration boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WallpaperFit {
    Fill,
    Fit,
    Centre,
    Tile,
}

impl WallpaperFit {
    /// Parse the exact public TOML/Lua representation.
    ///
    /// # Errors
    /// Rejects aliases, noncanonical case and unknown modes.
    pub fn parse(value: &str) -> Result<Self, Error> {
        match value {
            "fill" => Ok(Self::Fill),
            "fit" => Ok(Self::Fit),
            "centre" => Ok(Self::Centre),
            "tile" => Ok(Self::Tile),
            _ => Err(Error::InvalidValue(Key::WallpaperFit.name())),
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fill => "fill",
            Self::Fit => "fit",
            Self::Centre => "centre",
            Self::Tile => "tile",
        }
    }
}

/// Native timer policy; never implies an observation or wall-clock scheduler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WallpaperChange {
    Never,
    Hourly,
    Daily,
    Login,
}

impl WallpaperChange {
    /// # Errors
    /// Rejects noncanonical timer policy names.
    pub fn parse(text: &str) -> Result<Self, Error> {
        match text {
            "never" => Ok(Self::Never),
            "hourly" => Ok(Self::Hourly),
            "daily" => Ok(Self::Daily),
            "login" => Ok(Self::Login),
            _ => Err(Error::InvalidValue(Key::WallpaperChange.name())),
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::Hourly => "hourly",
            Self::Daily => "daily",
            Self::Login => "login",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    Text(String),
    WallpaperFit(WallpaperFit),
    WallpaperPreset(Option<Preset>),
    WallpaperChange(WallpaperChange),
    Palette(Palette),
    Integer(i64),
    Assignments(BTreeMap<String, String>),
}

// Keep HOME expansion and native chrono formatting at the operation boundary.
// This lexical validation never reads the filesystem or the process HOME.
fn valid_capture_directory(directory: &str) -> bool {
    if directory.len() > 4096 || directory.contains('\0') {
        return false;
    }
    let tail = directory
        .strip_prefix("~/")
        .or_else(|| directory.strip_prefix("$HOME/"));
    (directory.is_empty()
        || directory == "~"
        || tail.is_some()
        || Path::new(directory).is_absolute())
        && !Path::new(tail.unwrap_or(directory))
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
}

fn valid_output(output: &str) -> bool {
    !output.is_empty()
        && output.len() <= 255
        && output.trim() == output
        && !output.contains(',')
        && !output.chars().any(char::is_control)
}

fn valid_wallpaper(path: &str) -> bool {
    path.len() <= 4096
        && Path::new(path).is_absolute()
        && !path.contains('\0')
        && Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                matches!(
                    ext.to_ascii_lowercase().as_str(),
                    "png" | "jpg" | "jpeg" | "webp" | "mp4" | "webm" | "mkv" | "mov" | "gif"
                )
            })
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
    #[error("settings transaction must contain at least one change")]
    EmptyTransaction,
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
            let candidate = if key == Key::WallpaperAssignments {
                let entries = item
                    .as_table_like()
                    .ok_or(Error::InvalidValue(key.name()))?;
                if entries.len() > 32 {
                    return Err(Error::InvalidValue(key.name()));
                }
                let mut assignments = BTreeMap::new();
                for (output, path) in entries.iter() {
                    let path = path.as_str().ok_or(Error::InvalidValue(key.name()))?;
                    assignments.insert(output.to_owned(), path.to_owned());
                }
                Value::Assignments(assignments)
            } else if let Some(text) = item.as_str() {
                key.parse_text(text)?
            } else if let Some(boolean) = item.as_bool() {
                Value::Bool(boolean)
            } else if let Some(integer) = item.as_integer() {
                Value::Integer(integer)
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
    /// Atomically edit one output assignment through the effective managed/user layer.
    ///
    /// # Errors
    /// Rejects invalid selectors, unsupported paths, map overflow and persistence errors.
    pub fn assign_wallpaper(
        &mut self,
        output: &str,
        path: Option<&str>,
    ) -> Result<SetOutcome, Error> {
        if !valid_output(output) {
            return Err(Error::InvalidValue(Key::WallpaperAssignments.name()));
        }
        let Value::Assignments(mut assignments) = self.get(Key::WallpaperAssignments).value else {
            return Err(Error::InvalidValue(Key::WallpaperAssignments.name()));
        };
        if let Some(path) = path {
            assignments.insert(output.to_owned(), path.to_owned());
        } else {
            assignments.remove(output);
        }
        self.set(Key::WallpaperAssignments, Value::Assignments(assignments))
    }

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
        self.set_many_with_sync(BTreeMap::from([(key, candidate)]), sync_directory)
    }

    /// Accept related settings in one disk commit and one cached-state transition.
    ///
    /// # Errors
    /// Pre-commit failure leaves every managed override and writable value intact.
    /// Any managed participant makes the entire related proposal process-local.
    pub fn set_many(&mut self, candidates: BTreeMap<Key, Value>) -> Result<SetOutcome, Error> {
        self.set_many_with_sync(candidates, |directory| File::open(directory)?.sync_all())
    }

    fn set_many_with_sync(
        &mut self,
        candidates: BTreeMap<Key, Value>,
        sync_directory: impl FnOnce(&Path) -> std::io::Result<()>,
    ) -> Result<SetOutcome, Error> {
        if candidates.is_empty() {
            return Err(Error::EmptyTransaction);
        }
        for (key, candidate) in &candidates {
            key.validate(candidate)?;
        }
        // A derived operation must not leave only half of its state after restart.
        if candidates
            .keys()
            .any(|key| self.managed.contains_key(key) || self.overrides.contains_key(key))
        {
            self.overrides.extend(candidates);
            return Ok(SetOutcome::RuntimeOverride);
        }
        // Preserve unknown keys and reject concurrent invalid edits before overrides change.
        let mut next = Layer::read(&self.user_path, true)?;
        for (key, candidate) in candidates {
            let layer = &mut next;
            let (section, field) = key.location();
            if !layer.document.contains_key(section) {
                layer.document[section] = Item::Table(Table::new());
            }
            let table = layer
                .document
                .get_mut(section)
                .and_then(Item::as_table_like_mut)
                .ok_or(Error::InvalidValue(key.name()))?;
            let item = match &candidate {
                Value::Bool(boolean) => value(*boolean),
                Value::Text(text) => value(text),
                Value::WallpaperFit(fit) => value(fit.as_str()),
                Value::WallpaperPreset(preset) => {
                    preset.map_or(Item::None, |preset| value(preset.name()))
                }
                Value::WallpaperChange(change) => value(change.name()),
                Value::Palette(palette) => value(palette.name()),
                Value::Integer(integer) => value(*integer),
                Value::Assignments(assignments) => {
                    let mut entries = Table::new();
                    for (output, path) in assignments {
                        entries.insert(output, value(path));
                    }
                    Item::Table(entries)
                }
            };
            if item.is_none() {
                table.remove(field);
                layer.values.remove(&key);
            } else {
                table.insert(field, item);
                layer.values.insert(key, candidate);
            }
        }
        let serialized = next.document.to_string();
        if serialized.len() as u64 > MAX_CONFIG_BYTES {
            return Err(Error::TooLarge(self.user_path.clone()));
        }
        let outcome = persist(&self.user_path, serialized.as_bytes(), sync_directory)?;
        // Returning from persist means rename committed, even if durability is unknown.
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
    fn wallpaper_batch_postcommit_sync_failure_retains_entire_committed_state() {
        let directory =
            std::env::temp_dir().join(format!("tohru-wallpaper-batch-sync-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("config.toml");
        let mut owner = Owner::load(path.clone(), None).unwrap();
        let proposal = BTreeMap::from([
            (
                Key::WallpaperAssignments,
                Value::Assignments(BTreeMap::from([("A".into(), "/image.png".into())])),
            ),
            (
                Key::WallpaperPreset,
                Value::WallpaperPreset(Some(Preset::Solid)),
            ),
            (Key::Palette, Value::Palette(Palette::Nord)),
        ]);
        let outcome = owner
            .set_many_with_sync(proposal.clone(), |_| {
                Err(std::io::Error::other("injected fsync failure"))
            })
            .unwrap();
        assert!(matches!(
            outcome,
            SetOutcome::CommittedDurabilityUnconfirmed(_)
        ));
        let restarted = Owner::load(path, None).unwrap();
        for (key, value) in proposal {
            assert_eq!(owner.get(key).value, value);
            assert_eq!(owner.get(key), restarted.get(key));
        }
        fs::remove_dir_all(directory).unwrap();
    }

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
