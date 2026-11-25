use serde_json::{Map, Value};
use std::{path::PathBuf, time::Duration};

const PREFIX: &str = "assistant.voice_profiles";
const MAX_VRAM_MIB: u64 = 262_144;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("voice profiles must be a JSON object")]
    NotObject,
    #[error("{PREFIX}.low and .high must be tables")]
    NotTables,
    #[error("{PREFIX}.{key} must be one of {allowed}, not {value}")]
    Choice {
        key: &'static str,
        allowed: &'static str,
        value: String,
    },
    #[error("{PREFIX}.{key} must be a whole number from {low} to {high}, not {value}")]
    Integer {
        key: &'static str,
        low: u64,
        high: u64,
        value: String,
    },
    #[error("{PREFIX}.model_root must be empty ($MODEL_ROOT) or an absolute path, not {0}")]
    ModelRoot(String),
    #[error("{PREFIX}.high.enabled must be true or false")]
    Enabled,
    #[error("{PREFIX}.high.tts_model must be a $MODEL_ROOT alias such as voice-tts, not {0}")]
    TtsModel(String),
    #[error("{PREFIX}.high.asr_model must be empty or a $MODEL_ROOT alias, not {0}")]
    AsrModel(String),
    #[error("{PREFIX}.high.tts_speaker must be a speaker name such as Serena, not {0}")]
    Speaker(String),
    #[error("{PREFIX}.high.python must be an absolute interpreter path, not {0}")]
    Python(String),
    #[error(
        "{PREFIX}.high.python is not set: name the interpreter with torch (CUDA) and faster-qwen3-tts"
    )]
    PythonMissing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LowPrecision {
    Int8,
    Float32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HighPrecision {
    BFloat16,
    Float16,
    Float32,
}

impl HighPrecision {
    pub fn name(self) -> &'static str {
        match self {
            Self::BFloat16 => "bfloat16",
            Self::Float16 => "float16",
            Self::Float32 => "float32",
        }
    }
}

/// A `$MODEL_ROOT/by-name` entry name; never a path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alias(String);

impl Alias {
    fn parse(value: &str) -> Option<Self> {
        let mut bytes = value.bytes();
        let first = bytes.next()?;
        let valid = value.len() <= 128
            && (first.is_ascii_lowercase() || first.is_ascii_digit())
            && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
            && !value.contains("..");
        valid.then(|| Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HighProfile {
    pub precision: HighPrecision,
    pub python: PathBuf,
    pub tts_model: Alias,
    pub tts_speaker: String,
    pub asr_model: Option<Alias>,
    /// `None` estimates the budget from the catalog's file sizes.
    pub vram_mib: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileConfig {
    pub model_root: Option<PathBuf>,
    pub threshold_percent: u64,
    pub unload_after: Duration,
    pub queue_timeout: Duration,
    pub low_precision: LowPrecision,
    /// `None` when the high profile is disabled.
    pub high: Option<HighProfile>,
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            model_root: None,
            threshold_percent: 50,
            unload_after: Duration::from_secs(120),
            queue_timeout: Duration::from_secs(60),
            low_precision: LowPrecision::Int8,
            high: None,
        }
    }
}

fn shown(value: &Value) -> String {
    match value {
        Value::String(text) => format!("'{text}'"),
        other => other.to_string(),
    }
}

fn choice<T: Copy>(
    table: &Map<String, Value>,
    key: &'static str,
    allowed: &'static str,
    options: &[(&str, T)],
) -> Result<T, ConfigError> {
    let field = key.rsplit('.').next().unwrap_or(key);
    let Some(value) = table.get(field) else {
        return Ok(options[0].1);
    };
    value
        .as_str()
        .and_then(|text| options.iter().find(|(name, _)| *name == text))
        .map(|(_, option)| *option)
        .ok_or_else(|| ConfigError::Choice {
            key,
            allowed,
            value: shown(value),
        })
}

fn integer(
    table: &Map<String, Value>,
    key: &'static str,
    low: u64,
    high: u64,
    default: u64,
) -> Result<u64, ConfigError> {
    let field = key.rsplit('.').next().unwrap_or(key);
    let Some(value) = table.get(field) else {
        return Ok(default);
    };
    value
        .as_u64()
        .filter(|n| (low..=high).contains(n))
        .ok_or_else(|| ConfigError::Integer {
            key,
            low,
            high,
            value: shown(value),
        })
}

fn text<'a>(table: &'a Map<String, Value>, key: &str, default: &'a str) -> Option<&'a str> {
    match table.get(key) {
        None => Some(default),
        Some(value) => value.as_str(),
    }
}

/// The profile settings the shell passes as JSON, validated as strictly as the settings file.
pub fn parse(raw: &Value) -> Result<ProfileConfig, ConfigError> {
    let root = raw.as_object().ok_or(ConfigError::NotObject)?;
    let empty = Map::new();
    let table = |key| match root.get(key) {
        None | Some(Value::Null) => Ok(&empty),
        Some(Value::Object(table)) => Ok(table),
        Some(_) => Err(ConfigError::NotTables),
    };
    let (low, high) = (table("low")?, table("high")?);

    let model_root = match root.get("model_root") {
        None => None,
        Some(Value::String(path)) if path.is_empty() => None,
        Some(Value::String(path)) if path.starts_with('/') => Some(PathBuf::from(path)),
        Some(other) => return Err(ConfigError::ModelRoot(shown(other))),
    };
    let threshold_percent = integer(root, "vram_threshold_percent", 1, 100, 50)?;
    let unload_after = integer(root, "hidden_unload_seconds", 0, 86_400, 120)?;
    let queue_timeout = integer(root, "queue_timeout_seconds", 1, 3600, 60)?;
    choice(low, "low.device", "cpu", &[("cpu", ())])?;
    let low_precision = choice(
        low,
        "low.precision",
        "int8, float32",
        &[
            ("int8", LowPrecision::Int8),
            ("float32", LowPrecision::Float32),
        ],
    )?;

    let enabled = match high.get("enabled") {
        None => false,
        Some(Value::Bool(enabled)) => *enabled,
        Some(_) => return Err(ConfigError::Enabled),
    };
    let field = |key| high.get(key).map_or_else(|| "null".into(), shown);
    let tts_model = text(high, "tts_model", "voice-tts")
        .and_then(Alias::parse)
        .ok_or_else(|| ConfigError::TtsModel(field("tts_model")))?;
    let asr_model = match text(high, "asr_model", "") {
        Some("") => None,
        Some(alias) => {
            Some(Alias::parse(alias).ok_or_else(|| ConfigError::AsrModel(field("asr_model")))?)
        }
        None => return Err(ConfigError::AsrModel(field("asr_model"))),
    };
    let tts_speaker = text(high, "tts_speaker", "Serena")
        .filter(|name| {
            (1..=64).contains(&name.len())
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
        .ok_or_else(|| ConfigError::Speaker(field("tts_speaker")))?
        .to_owned();
    let python = match text(high, "python", "") {
        Some(path) if path.is_empty() || path.starts_with('/') => PathBuf::from(path),
        _ => return Err(ConfigError::Python(field("python"))),
    };
    if enabled && python.as_os_str().is_empty() {
        return Err(ConfigError::PythonMissing);
    }
    choice(high, "high.device", "cuda", &[("cuda", ())])?;
    let precision = choice(
        high,
        "high.precision",
        "bfloat16, float16, float32",
        &[
            ("bfloat16", HighPrecision::BFloat16),
            ("float16", HighPrecision::Float16),
            ("float32", HighPrecision::Float32),
        ],
    )?;
    let vram_mib = integer(high, "high.vram_mib", 0, MAX_VRAM_MIB, 0)?;

    Ok(ProfileConfig {
        model_root,
        threshold_percent,
        unload_after: Duration::from_secs(unload_after),
        queue_timeout: Duration::from_secs(queue_timeout),
        low_precision,
        high: enabled.then_some(HighProfile {
            precision,
            python,
            tts_model,
            tts_speaker,
            asr_model,
            vram_mib: (vram_mib > 0).then_some(vram_mib),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_keep_the_cpu_profile_and_leave_high_off() {
        let parsed = parse(&json!({})).unwrap();
        assert_eq!(parsed, ProfileConfig::default());
        let high = parse(&json!({"high":{"enabled":true,"python":"/gpu/bin/python3"}}))
            .unwrap()
            .high
            .unwrap();
        assert_eq!(high.precision, HighPrecision::BFloat16);
        assert_eq!(high.tts_model.as_str(), "voice-tts");
        assert_eq!((high.asr_model, high.vram_mib), (None, None));
        assert_eq!(high.tts_speaker, "Serena");
    }

    #[test]
    fn errors_name_the_setting() {
        let cases = [
            (
                json!({"vram_threshold_percent":0}),
                "assistant.voice_profiles.vram_threshold_percent",
            ),
            (
                json!({"vram_threshold_percent":true}),
                "vram_threshold_percent",
            ),
            (
                json!({"low":{"device":"cuda"}}),
                "assistant.voice_profiles.low.device",
            ),
            (
                json!({"low":{"precision":"bfloat16"}}),
                "assistant.voice_profiles.low.precision",
            ),
            (
                json!({"high":{"precision":"int4"}}),
                "assistant.voice_profiles.high.precision",
            ),
            (
                json!({"high":{"tts_model":"/mnt/models/voice-tts"}}),
                "high.tts_model must be a $MODEL_ROOT alias",
            ),
            (json!({"high":{"asr_model":"../x"}}), "high.asr_model"),
            (
                json!({"high":{"tts_speaker":"two words"}}),
                "high.tts_speaker",
            ),
            (json!({"high":{"enabled":true}}), "high.python is not set"),
            (
                json!({"high":{"enabled":"yes"}}),
                "high.enabled must be true or false",
            ),
            (
                json!({"high":{"python":"python3"}}),
                "high.python must be an absolute",
            ),
            (json!({"model_root":"models"}), "model_root"),
            (json!({"low":[]}), "must be tables"),
            (json!([]), "JSON object"),
        ];
        for (raw, message) in cases {
            let error = parse(&raw).unwrap_err().to_string();
            assert!(error.contains(message), "{raw}: {error}");
        }
    }

    #[test]
    fn explicit_values_are_carried() {
        let parsed = parse(&json!({
            "model_root":"/models","vram_threshold_percent":70,"hidden_unload_seconds":0,
            "queue_timeout_seconds":5,"low":{"precision":"float32"},
            "high":{"enabled":true,"python":"/gpu/bin/python3","tts_model":"voice-tomoko",
                    "tts_speaker":"kuroki_tomoko","asr_model":"whisper-x","vram_mib":9000,"precision":"float16"}
        }))
        .unwrap();
        assert_eq!(parsed.model_root, Some(PathBuf::from("/models")));
        assert_eq!(parsed.threshold_percent, 70);
        assert_eq!(parsed.unload_after, Duration::ZERO);
        assert_eq!(parsed.low_precision, LowPrecision::Float32);
        let high = parsed.high.unwrap();
        assert_eq!(high.tts_model.as_str(), "voice-tomoko");
        assert_eq!(high.asr_model.unwrap().as_str(), "whisper-x");
        assert_eq!(
            (high.vram_mib, high.precision),
            (Some(9000), HighPrecision::Float16)
        );
    }
}
