use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;

use crate::paths;

#[derive(Debug, thiserror::Error)]
pub enum UnitError {
    #[error("unknown component")]
    UnknownComponent,
    #[error("invalid target")]
    InvalidTarget,
    #[error("invalid unit value")]
    InvalidValue,
    #[error("cannot read the service specifications")]
    Specs(#[source] std::io::Error),
    #[error("invalid service specifications")]
    SpecFormat(#[source] serde_json::Error),
    #[error("cannot create {}", .0.display())]
    Create(PathBuf, #[source] std::io::Error),
}

/// One component's entry in `share/tohru/session/services.json`.
#[derive(Clone, Debug, Deserialize)]
pub struct ServiceSpec {
    pub program: String,
    pub description: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub kill_mode: Option<String>,
    pub timeout: Option<u64>,
    pub restart_prevent_exit_status: Option<u64>,
    #[serde(default = "install_default")]
    pub install: bool,
}

fn install_default() -> bool {
    true
}

pub type Specs = indexmap_like::Ordered<ServiceSpec>;

mod indexmap_like {
    use serde::Deserialize;

    /// Specs in file order, as the generator reports them.
    #[derive(Debug)]
    pub struct Ordered<T>(pub Vec<(String, T)>);

    impl<'de, T: Deserialize<'de>> Deserialize<'de> for Ordered<T> {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let map = serde_json::Map::deserialize(deserializer)?;
            map.into_iter()
                .map(|(key, value)| {
                    T::deserialize(value)
                        .map(|spec| (key, spec))
                        .map_err(serde::de::Error::custom)
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Ordered)
        }
    }

    impl<T> Ordered<T> {
        pub fn get(&self, name: &str) -> Option<&T> {
            self.0
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value)
        }
    }
}

pub fn load_specs() -> Result<Specs, UnitError> {
    let text = std::fs::read_to_string(paths::data_dir().join("session/services.json"))
        .map_err(UnitError::Specs)?;
    serde_json::from_str(&text).map_err(UnitError::SpecFormat)
}

/// systemd quoting: reject line breaks and NUL, escape backslashes, quotes and specifiers.
fn quote(value: &str, expand: bool) -> Result<String, UnitError> {
    if value.contains(['\n', '\r', '\0']) {
        return Err(UnitError::InvalidValue);
    }
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%");
    Ok(format!(
        "\"{}\"",
        if expand {
            escaped.replace('$', "$$")
        } else {
            escaped
        }
    ))
}

/// `path` made absolute and resolved as far as it exists.
#[must_use]
pub fn resolve(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
    if let Ok(real) = absolute.canonicalize() {
        return real;
    }
    let mut existing = absolute.clone();
    let mut rest = Vec::new();
    while let Some(name) = existing.file_name().map(ToOwned::to_owned) {
        rest.push(name);
        existing.pop();
        if let Ok(real) = existing.canonicalize() {
            return rest.iter().rev().fold(real, |path, name| path.join(name));
        }
    }
    absolute
}

/// The text of one generated user unit.
pub fn unit_text(
    spec: &ServiceSpec,
    prefix: &Path,
    target: &str,
    managed: Option<&str>,
) -> Result<String, UnitError> {
    let command: Vec<&str> = spec.program.split_whitespace().collect();
    let (program, arguments) = command.split_first().ok_or(UnitError::InvalidValue)?;
    let executable = resolve(prefix).join(program);
    let mut exec = vec![quote(&executable.to_string_lossy(), true)?];
    for argument in arguments {
        exec.push(quote(argument, true)?);
    }
    let kind = spec.kind.as_deref().unwrap_or("simple");
    let mut text = String::new();
    let _ = write!(
        text,
        "[Unit]\nDescription={}\nPartOf={target}\nAfter={target}\nConditionEnvironment=WAYLAND_DISPLAY\n\n[Service]\nType={kind}\nExecStart={}\nRestart=on-failure\nRestartSec=2\nKillMode={}\nStandardOutput=journal\nStandardError=journal\n",
        spec.description,
        exec.join(" "),
        spec.kill_mode.as_deref().unwrap_or("control-group"),
    );
    if kind == "notify" {
        let _ = write!(
            text,
            "NotifyAccess=all\nTimeoutStartSec={}\n",
            spec.timeout.unwrap_or_default()
        );
    }
    if let Some(status) = spec
        .restart_prevent_exit_status
        .filter(|status| *status != 0)
    {
        let _ = writeln!(text, "RestartPreventExitStatus={status}");
    }
    if let Some(managed) = managed.filter(|path| !path.is_empty()) {
        let _ = writeln!(
            text,
            "Environment={}",
            quote(&format!("TOHRU_MANAGED_CONFIG={managed}"), false)?
        );
    }
    if spec.install {
        let _ = write!(text, "\n[Install]\nWantedBy={target}\n");
    }
    Ok(text)
}

static TARGET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_.@:-]+\.target$").expect("fixed pattern"));

pub struct UnitRequest<'a> {
    pub output: &'a Path,
    pub prefix: &'a Path,
    pub target: &'a str,
    pub components: &'a str,
    pub managed: Option<&'a str>,
}

/// Writes the selected units (plus the on-demand locker) without enabling or starting any.
pub fn generate(specs: &Specs, request: &UnitRequest<'_>) -> Result<usize, UnitError> {
    let mut selected: Vec<&str> = request.components.split(',').collect();
    if selected.iter().any(|name| specs.get(name).is_none()) {
        return Err(UnitError::UnknownComponent);
    }
    if !TARGET.is_match(request.target) {
        return Err(UnitError::InvalidTarget);
    }
    // Lock is available on demand even when idle ownership stays external.
    selected.push("lock");
    let mut unique = Vec::new();
    for name in selected {
        if !unique.contains(&name) {
            unique.push(name);
        }
    }
    std::fs::create_dir_all(request.output)
        .map_err(|error| UnitError::Create(request.output.to_owned(), error))?;
    for name in &unique {
        let spec = specs.get(name).ok_or(UnitError::UnknownComponent)?;
        let text = unit_text(spec, request.prefix, request.target, request.managed)?;
        let path = request.output.join(format!("tohru-{name}.service"));
        let mut file = std::fs::File::create_new(&path)
            .map_err(|error| UnitError::Create(path.clone(), error))?;
        file.write_all(text.as_bytes())
            .map_err(|error| UnitError::Create(path.clone(), error))?;
    }
    Ok(unique.len())
}

pub const ECHO_CANCEL: &str = "pipewire/60-tohru-echo-cancel.conf";

/// Prints the `PipeWire` echo-cancel drop-in, or creates it at `output` without overwriting.
pub fn echo_cancel(output: Option<&Path>) -> Result<Option<PathBuf>, UnitError> {
    let source = paths::data_dir().join(ECHO_CANCEL);
    let content = std::fs::read_to_string(&source).map_err(UnitError::Specs)?;
    let Some(path) = output else {
        print!("{content}");
        return Ok(None);
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| UnitError::Create(parent.to_owned(), error))?;
    }
    let mut file = std::fs::File::create_new(path)
        .map_err(|error| UnitError::Create(path.to_owned(), error))?;
    file.write_all(content.as_bytes())
        .map_err(|error| UnitError::Create(path.to_owned(), error))?;
    Ok(Some(path.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs() -> Specs {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../share/tohru/session/services.json"
        ))
        .unwrap();
        serde_json::from_str(&text).unwrap()
    }

    #[test]
    fn unit_generation_is_explicit_and_child_lifecycle_preserved() {
        let specs = specs();
        let root = tempfile::tempdir().unwrap();
        let lock = unit_text(
            specs.get("lock").unwrap(),
            root.path(),
            "graphical-session.target",
            None,
        )
        .unwrap();
        assert!(
            lock.contains("Type=notify\n")
                && lock.contains("NotifyAccess=all\nTimeoutStartSec=30\n")
        );
        assert!(lock.contains("RestartPreventExitStatus=3\n") && !lock.contains("WantedBy="));
        let launcher = unit_text(
            specs.get("launcher").unwrap(),
            Path::new("/opt/Tohru % 1/bin"),
            "graphical-session.target",
            Some("/config/managed.toml"),
        )
        .unwrap();
        assert!(launcher.contains("KillMode=process"));
        assert!(launcher.contains("ExecStart=\"/opt/Tohru %% 1/bin/tohru-launcher-run\""));
        assert!(launcher.ends_with("Environment=\"TOHRU_MANAGED_CONFIG=/config/managed.toml\"\n\n[Install]\nWantedBy=graphical-session.target\n"));
        let output = root.path().join("units");
        let request = UnitRequest {
            output: &output,
            prefix: root.path(),
            target: "graphical-session.target",
            components: "bar,launcher",
            managed: None,
        };
        assert_eq!(generate(&specs, &request).unwrap(), 3);
        assert_eq!(std::fs::read_dir(&output).unwrap().count(), 3);
        assert!(matches!(
            generate(&specs, &request),
            Err(UnitError::Create(..))
        ));
        let bad = UnitRequest {
            components: "bar,unknown",
            ..request
        };
        assert!(matches!(
            generate(&specs, &bad),
            Err(UnitError::UnknownComponent)
        ));
        let bad = UnitRequest {
            target: "graphical session.target",
            components: "bar",
            ..bad
        };
        assert!(matches!(
            generate(&specs, &bad),
            Err(UnitError::InvalidTarget)
        ));
    }

    #[test]
    fn arguments_are_quoted_and_dollars_escaped() {
        let spec = ServiceSpec {
            program: "tohru-wallpaper serve".into(),
            description: "Tohru wallpaper owner".into(),
            kind: None,
            kill_mode: None,
            timeout: None,
            restart_prevent_exit_status: None,
            install: true,
        };
        let text = unit_text(
            &spec,
            Path::new("/opt/$bin"),
            "graphical-session.target",
            None,
        )
        .unwrap();
        assert!(text.contains("ExecStart=\"/opt/$$bin/tohru-wallpaper\" \"serve\"\n"));
        assert!(quote("a\nb", true).is_err());
    }
}
