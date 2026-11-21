//! Read-only `[greeter]` configuration parsed from `TOHRU_MANAGED_CONFIG`.
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item};

use crate::settings::LockStyle;

const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_AVATARS: usize = 64;
const MAX_HIDDEN_USERS: usize = 64;

/// Greeter visual style: any of the ten lock styles, or the zero-shader plain style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GreeterStyle {
    Plain,
    Styled(LockStyle),
}

impl GreeterStyle {
    pub const DEFAULT: Self = Self::Styled(LockStyle::DEFAULT);

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        if name == "plain" {
            Some(Self::Plain)
        } else {
            LockStyle::parse(name).map(Self::Styled)
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Styled(style) => style.name(),
        }
    }
}

/// Static configuration supplied by the NixOS module (or defaults when absent).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GreeterConfig {
    pub style: GreeterStyle,
    pub wallpaper: Option<PathBuf>,
    pub preferred_output: Option<String>,
    pub default_user: Option<String>,
    pub default_session: Option<String>,
    pub allow_power_actions: bool,
    pub hidden_users: Vec<String>,
    pub avatars: BTreeMap<String, PathBuf>,
    pub warnings: Vec<String>,
}

impl Default for GreeterConfig {
    fn default() -> Self {
        Self {
            style: GreeterStyle::DEFAULT,
            wallpaper: None,
            preferred_output: None,
            default_user: None,
            default_session: None,
            allow_power_actions: true,
            hidden_users: vec!["nobody".into(), "greeter".into()],
            avatars: BTreeMap::new(),
            warnings: Vec::new(),
        }
    }
}

impl GreeterConfig {
    /// Load `[greeter]` (falling back to `[lock].style` when `[greeter].style` is absent).
    /// Never fails hard: broken or unreadable files produce defaults plus warnings so
    /// the login screen always starts.
    #[must_use]
    pub fn load(path: Option<&Path>) -> Self {
        let Some(path) = path else {
            return Self::default();
        };

        let mut cfg = Self::default();
        let mut raw = String::new();
        match File::open(path) {
            Ok(file) => {
                if let Err(err) = file.take(MAX_CONFIG_BYTES + 1).read_to_string(&mut raw) {
                    cfg.warnings
                        .push(format!("{}: cannot read config: {err}", path.display()));
                    return cfg;
                }
                if raw.len() as u64 > MAX_CONFIG_BYTES {
                    cfg.warnings.push(format!(
                        "{}: config exceeds 1 MiB; using defaults",
                        path.display()
                    ));
                    return cfg;
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return cfg,
            Err(err) => {
                cfg.warnings
                    .push(format!("{}: cannot open config: {err}", path.display()));
                return cfg;
            }
        }

        let document = match raw.parse::<DocumentMut>() {
            Ok(doc) => doc,
            Err(err) => {
                cfg.warnings.push(format!(
                    "{}: invalid TOML ({err}); using defaults",
                    path.display()
                ));
                return cfg;
            }
        };

        if let Some(lock_style) = document
            .get("lock")
            .and_then(Item::as_table_like)
            .and_then(|table| table.get("style"))
            .and_then(Item::as_str)
            .and_then(GreeterStyle::parse)
        {
            cfg.style = lock_style;
        }

        let Some(table) = document.get("greeter").and_then(Item::as_table_like) else {
            return cfg;
        };

        cfg.apply_greeter_table(path, table);
        cfg
    }

    fn apply_greeter_table(&mut self, path: &Path, table: &dyn toml_edit::TableLike) {
        if let Some(item) = table.get("style") {
            match item.as_str().and_then(GreeterStyle::parse) {
                Some(style) => self.style = style,
                None => self.warnings.push(format!(
                    "{}: unknown greeter.style {}; using \"{}\"",
                    path.display(),
                    item.to_string().trim(),
                    self.style.name()
                )),
            }
        }

        if let Some(text) = table.get("wallpaper").and_then(Item::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                self.wallpaper = Some(PathBuf::from(trimmed));
            }
        }

        if let Some(text) = table.get("preferred_output").and_then(Item::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() && trimmed.len() <= 256 && !trimmed.chars().any(char::is_control)
            {
                self.preferred_output = Some(trimmed.to_owned());
            }
        }

        if let Some(text) = table.get("default_user").and_then(Item::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() && trimmed.len() <= 128 && !trimmed.chars().any(char::is_control)
            {
                self.default_user = Some(trimmed.to_owned());
            }
        }

        if let Some(text) = table.get("default_session").and_then(Item::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() && trimmed.len() <= 256 && !trimmed.chars().any(char::is_control)
            {
                self.default_session = Some(trimmed.to_owned());
            }
        }

        if let Some(allow) = table.get("allow_power_actions").and_then(Item::as_bool) {
            self.allow_power_actions = allow;
        }

        if let Some(array) = table.get("hidden_users").and_then(Item::as_array) {
            let mut hidden = Vec::new();
            for item in array.iter().take(MAX_HIDDEN_USERS) {
                if let Some(name) = item.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                    hidden.push(name.to_owned());
                }
            }
            if !hidden.is_empty() {
                self.hidden_users = hidden;
            }
        }

        if let Some(avatars) = table.get("avatars").and_then(Item::as_table_like) {
            for (user, item) in avatars.iter().take(MAX_AVATARS) {
                let user = user.trim();
                let Some(raw_path) = item.as_str().map(str::trim).filter(|s| !s.is_empty()) else {
                    continue;
                };
                if !user.is_empty() && !user.chars().any(char::is_control) {
                    self.avatars
                        .insert(user.to_owned(), PathBuf::from(raw_path));
                }
            }
        }
    }
}
