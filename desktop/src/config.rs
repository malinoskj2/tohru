use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use regex::Regex;
use toml::Value;

use crate::paths::{Xdg, xdg};

pub const CONFIG_LIMIT: u64 = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("configuration exceeds 1 MiB")]
    TooLarge,
    #[error("cannot read {}", .0.display())]
    Read(PathBuf, #[source] std::io::Error),
    #[error("configuration is not UTF-8")]
    Encoding,
    #[error("{0}")]
    Parse(String),
    #[error("managed config must be separate from writable config")]
    ManagedOverlap,
    #[error("managed config is missing")]
    ManagedMissing,
    #[error("{0}")]
    Invalid(String),
}

fn invalid<T>(message: impl Into<String>) -> Result<T, ConfigError> {
    Err(ConfigError::Invalid(message.into()))
}

/// Reads and parses one bounded TOML file.
pub fn read_toml(path: &Path) -> Result<toml::Table, ConfigError> {
    let file =
        std::fs::File::open(path).map_err(|error| ConfigError::Read(path.to_owned(), error))?;
    let metadata = file
        .metadata()
        .map_err(|error| ConfigError::Read(path.to_owned(), error))?;
    if metadata.len() > CONFIG_LIMIT {
        return Err(ConfigError::TooLarge);
    }
    let mut data = Vec::new();
    file.take(CONFIG_LIMIT + 1)
        .read_to_end(&mut data)
        .map_err(|error| ConfigError::Read(path.to_owned(), error))?;
    if data.len() as u64 > CONFIG_LIMIT {
        return Err(ConfigError::TooLarge);
    }
    let text = String::from_utf8(data).map_err(|_| ConfigError::Encoding)?;
    parse_toml(&text)
}

pub fn parse_toml(text: &str) -> Result<toml::Table, ConfigError> {
    text.parse::<toml::Table>()
        .map_err(|error| ConfigError::Parse(error.message().to_owned()))
}

/// Flattens nested tables into dotted keys; arrays stay values.
#[must_use]
pub fn flatten(table: &toml::Table) -> BTreeMap<String, Value> {
    let mut result = BTreeMap::new();
    flatten_into(table, "", &mut result);
    result
}

fn flatten_into(table: &toml::Table, prefix: &str, result: &mut BTreeMap<String, Value>) {
    for (key, value) in table {
        let name = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match value {
            Value::Table(inner) => flatten_into(inner, &name, result),
            other => {
                result.insert(name, other.clone());
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortcutsMode {
    Compositor,
    Portal,
    Disabled,
}

impl ShortcutsMode {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Compositor => "compositor",
            Self::Portal => "portal",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum IdleOwner {
    External,
    Tohru,
    Disabled,
}

impl IdleOwner {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::External => "external",
            Self::Tohru => "tohru",
            Self::Disabled => "disabled",
        }
    }

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "external" => Some(Self::External),
            "tohru" => Some(Self::Tohru),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum IdleSource {
    Passive,
    Callbacks,
}

impl IdleSource {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Passive => "passive",
            Self::Callbacks => "callbacks",
        }
    }

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "passive" => Some(Self::Passive),
            "callbacks" => Some(Self::Callbacks),
            _ => None,
        }
    }
}

pub const AFK_SCENES: [&str; 12] = [
    "screen_window",
    "corner_nap",
    "glass_inspection",
    "upside_down_peek",
    "window_perch",
    "tail_chase",
    "sleepy_drift",
    "distant_flight",
    "hide_peek",
    "wing_grooming",
    "waiting_for_you",
    "quiet_rest",
];

/// The validated standalone settings, with every flattened value for checks that read others.
#[derive(Clone, Debug)]
pub struct Config {
    pub values: BTreeMap<String, Value>,
    pub managed: BTreeSet<String>,
    pub shortcuts_mode: ShortcutsMode,
    pub wallpaper_enabled: bool,
    pub idle_owner: IdleOwner,
    pub idle_lock_seconds: u32,
    pub idle_suspend_seconds: u32,
    pub afk_enabled: bool,
    pub afk_idle_seconds: u32,
    pub afk_idle_source: IdleSource,
    pub voice: bool,
}

fn defaults() -> BTreeMap<String, Value> {
    let text = |value: &str| Value::String(value.to_owned());
    [
        ("shortcuts.mode", text("compositor")),
        ("shortcuts.launcher_trigger", text("")),
        ("shortcuts.session_trigger", text("")),
        ("shortcuts.assistant_trigger", text("")),
        ("wallpaper.enabled", Value::Boolean(false)),
        ("wallpaper.backend", text("mpvpaper")),
        ("wallpaper.directory", text("")),
        ("wallpaper.assignments", text("")),
        ("wallpaper.executable", text("mpvpaper")),
        ("wallpaper.options", text("no-audio loop-file=inf")),
        ("idle.owner", text("external")),
        ("idle.lock_seconds", Value::Integer(300)),
        ("idle.suspend_seconds", Value::Integer(0)),
        ("appearance.font_text", text("Noto Sans")),
        ("appearance.font_display", text("Noto Sans")),
        ("appearance.font_mono", text("DejaVu Sans Mono")),
        ("assistant.voice_python", text("python3")),
        ("assistant.voice_environment", text("")),
        ("assistant.voice", Value::Boolean(true)),
        ("assistant.voice_source", text("")),
        ("assistant.tts_engine", text("kokoro")),
        ("assistant.afk_enabled", Value::Boolean(false)),
        ("assistant.afk_idle_seconds", Value::Integer(300)),
        ("assistant.afk_probability_percent", Value::Integer(50)),
        ("assistant.afk_seed", text("")),
        ("assistant.afk_scene", text("auto")),
        ("assistant.afk_scenes", text("all")),
        ("assistant.afk_quiet_rest", Value::Boolean(false)),
        ("assistant.afk_idle_source", text("passive")),
        ("assistant.vad_model", text("")),
        ("assistant.asr_model", text("")),
        ("assistant.turn_model", text("")),
        ("assistant.kws_model", text("")),
        ("assistant.tts_model", text("")),
        ("assistant.voice_wake", text("asr")),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_owned(), value))
    .collect()
}

/// The writable config path under `$XDG_CONFIG_HOME`.
#[must_use]
pub fn user_config_path() -> PathBuf {
    xdg(Xdg::Config).join("tohru/config.toml")
}

impl Config {
    /// Loads the XDG config and the optional managed overlay named by `TOHRU_MANAGED_CONFIG`.
    pub fn load() -> Result<Self, ConfigError> {
        let user = user_config_path();
        let mut layers = Vec::new();
        if user.exists() {
            layers.push((read_toml(&user)?, false));
        }
        if let Some(managed) =
            std::env::var_os("TOHRU_MANAGED_CONFIG").filter(|value| !value.is_empty())
        {
            let managed = PathBuf::from(managed);
            let resolve = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_owned());
            if resolve(&managed) == resolve(&user) {
                return Err(ConfigError::ManagedOverlap);
            }
            if !managed.exists() {
                return Err(ConfigError::ManagedMissing);
            }
            layers.push((read_toml(&managed)?, true));
        }
        Self::from_layers(&layers)
    }

    /// Builds and validates settings from ordered layers; `true` marks a managed layer.
    pub fn from_layers(layers: &[(toml::Table, bool)]) -> Result<Self, ConfigError> {
        let mut values = defaults();
        let mut managed = BTreeSet::new();
        for (table, owned) in layers {
            let layer = flatten(table);
            if *owned {
                managed.extend(layer.keys().cloned());
            }
            values.extend(layer);
        }
        Self::validate(values, managed)
    }

    fn validate_text(values: &BTreeMap<String, Value>) -> Result<(), ConfigError> {
        let text = |values: &BTreeMap<String, Value>, name: &str| {
            values
                .get(name)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        for (name, default) in defaults() {
            if default.is_str() {
                match values.get(&name) {
                    Some(Value::String(value)) if !value.chars().any(char::is_control) => {}
                    _ => return invalid(format!("invalid {name}")),
                }
            }
        }
        for name in [
            "appearance.font_text",
            "appearance.font_display",
            "appearance.font_mono",
        ] {
            let value = text(values, name);
            if value.trim().is_empty() || value.len() > 256 || value.contains(':') {
                return invalid(format!("invalid {name}"));
            }
        }
        for name in [
            "wallpaper.executable",
            "wallpaper.options",
            "assistant.voice_python",
        ] {
            let value = text(values, name);
            if value.is_empty() || value.len() > 4096 {
                return invalid(format!("invalid {name}"));
            }
        }
        for name in [
            "wallpaper.directory",
            "wallpaper.assignments",
            "assistant.voice_environment",
        ] {
            let value = text(values, name);
            if value.len() > 4096 || (!value.is_empty() && !Path::new(&value).is_absolute()) {
                return invalid(format!("{name} must be an absolute path"));
            }
        }
        Ok(())
    }

    fn validate(
        values: BTreeMap<String, Value>,
        managed: BTreeSet<String>,
    ) -> Result<Self, ConfigError> {
        let flag = |name: &str| match values.get(name) {
            Some(Value::Boolean(value)) => Ok(*value),
            _ => invalid(format!("{name} must be boolean")),
        };
        let wallpaper_enabled = flag("wallpaper.enabled")?;
        let voice = flag("assistant.voice")?;
        let afk_enabled = flag("assistant.afk_enabled")?;
        flag("assistant.afk_quiet_rest")?;
        let shortcuts_mode = match values.get("shortcuts.mode").and_then(Value::as_str) {
            Some("compositor") => ShortcutsMode::Compositor,
            Some("portal") => ShortcutsMode::Portal,
            Some("disabled") => ShortcutsMode::Disabled,
            _ => return invalid("shortcuts.mode must be compositor, portal or disabled"),
        };
        for action in ["launcher", "session", "assistant"] {
            match values.get(&format!("shortcuts.{action}_trigger")) {
                Some(Value::String(trigger)) if trigger.len() <= 256 => {}
                _ => return invalid("invalid shortcut preferred trigger"),
            }
        }
        if values.get("wallpaper.backend").and_then(Value::as_str) != Some("mpvpaper") {
            return invalid("supported wallpaper backend: mpvpaper");
        }
        let Some(idle_owner) = values
            .get("idle.owner")
            .and_then(Value::as_str)
            .and_then(IdleOwner::parse)
        else {
            return invalid("idle.owner must be external, tohru or disabled");
        };
        let Some(afk_idle_source) = values
            .get("assistant.afk_idle_source")
            .and_then(Value::as_str)
            .and_then(IdleSource::parse)
        else {
            return invalid("assistant.afk_idle_source must be passive or callbacks");
        };
        let ranged = |name: &str, minimum: i64, maximum: i64| match values.get(name) {
            Some(Value::Integer(value)) if (minimum..=maximum).contains(value) => {
                Ok(u32::try_from(*value).unwrap_or_default())
            }
            _ => invalid(format!("{name} must be an integer in {minimum}..{maximum}")),
        };
        let idle_lock_seconds = ranged("idle.lock_seconds", 0, 86400)?;
        let idle_suspend_seconds = ranged("idle.suspend_seconds", 0, 86400)?;
        let afk_idle_seconds = ranged("assistant.afk_idle_seconds", 1, 86400)?;
        ranged("assistant.afk_probability_percent", 0, 100)?;
        Self::validate_afk(&values)?;
        Self::validate_text(&values)?;
        if idle_owner == IdleOwner::Tohru
            && idle_suspend_seconds != 0
            && idle_lock_seconds != 0
            && idle_suspend_seconds <= idle_lock_seconds
        {
            return invalid("idle.suspend_seconds must exceed idle.lock_seconds");
        }
        Ok(Self {
            values,
            managed,
            shortcuts_mode,
            wallpaper_enabled,
            idle_owner,
            idle_lock_seconds,
            idle_suspend_seconds,
            afk_enabled,
            afk_idle_seconds,
            afk_idle_source,
            voice,
        })
    }

    fn validate_afk(values: &BTreeMap<String, Value>) -> Result<(), ConfigError> {
        let seed_ok = match values.get("assistant.afk_seed") {
            Some(Value::String(seed)) => {
                seed.is_empty()
                    || Regex::new("^[0-9]{1,10}$").is_ok_and(|pattern| pattern.is_match(seed))
                        && seed.parse::<u64>().is_ok_and(|seed| seed <= 2_147_483_646)
            }
            _ => false,
        };
        if !seed_ok {
            return invalid("assistant.afk_seed must be empty or decimal text in 0..2147483646");
        }
        match values.get("assistant.afk_scene").and_then(Value::as_str) {
            Some(scene) if scene == "auto" || AFK_SCENES.contains(&scene) => {}
            _ => return invalid("unknown assistant.afk_scene"),
        }
        let scenes_ok = match values.get("assistant.afk_scenes").and_then(Value::as_str) {
            Some("all" | "none") => true,
            Some(list) => {
                !list.is_empty()
                    && list.len() <= 256
                    && list.split(',').all(|scene| AFK_SCENES.contains(&scene))
            }
            None => false,
        };
        if !scenes_ok {
            return invalid("assistant.afk_scenes must be all, none, or comma-separated scene IDs");
        }
        Ok(())
    }

    /// A validated string setting (every defaulted string is checked to be one).
    #[must_use]
    pub fn text(&self, name: &str) -> &str {
        self.values
            .get(name)
            .and_then(Value::as_str)
            .unwrap_or_default()
    }

    /// Python truthiness of any flattened setting, including ones this module does not validate.
    #[must_use]
    pub fn truthy(&self, name: &str) -> bool {
        match self.values.get(name) {
            None => false,
            Some(Value::Boolean(value)) => *value,
            Some(Value::Integer(value)) => *value != 0,
            Some(Value::Float(value)) => *value != 0.0,
            Some(Value::String(value)) => !value.is_empty(),
            Some(Value::Array(items)) => !items.is_empty(),
            Some(Value::Table(table)) => !table.is_empty(),
            Some(Value::Datetime(_)) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(text: &str) -> toml::Table {
        parse_toml(text).unwrap()
    }

    #[test]
    fn defaults_and_managed_lifecycle_values() {
        let config = Config::from_layers(&[]).unwrap();
        assert!(!config.wallpaper_enabled);
        assert_eq!(config.idle_owner, IdleOwner::External);
        let config = Config::from_layers(&[
            (layer("[wallpaper]\nenabled=true\n"), false),
            (
                layer("[wallpaper]\nenabled=false\n[appearance]\nfont_text=\"Custom Sans\"\n"),
                true,
            ),
        ])
        .unwrap();
        assert!(!config.wallpaper_enabled);
        assert!(config.managed.contains("appearance.font_text"));
        assert_eq!(config.text("appearance.font_text"), "Custom Sans");
    }

    #[test]
    fn invalid_native_helper_types_are_rejected() {
        for data in [
            "[wallpaper]\nenabled=\"true\"",
            "[idle]\nlock_seconds=-1",
            "[idle]\nlock_seconds=true",
            "[idle]\nowner=\"random\"",
            "[wallpaper]\ndirectory=\"relative\"",
            "[assistant]\nafk_seed=\"2147483647\"",
            "[assistant]\nafk_scenes=\"corner_nap,unknown\"",
            "[appearance]\nfont_mono=\"Mono:bold\"",
            "[idle]\nowner=\"tohru\"\nlock_seconds=600\nsuspend_seconds=300",
        ] {
            assert!(
                Config::from_layers(&[(layer(data), false)]).is_err(),
                "{data}"
            );
        }
        let config = Config::from_layers(&[(
            layer("[assistant]\nafk_scenes=\"corner_nap,hide_peek\"\nafk_seed=\"7\""),
            false,
        )]);
        assert!(config.is_ok());
    }
}
