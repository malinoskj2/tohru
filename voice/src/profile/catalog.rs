use super::config::{Alias, HighPrecision, HighProfile};
use serde_json::Value;
use std::{
    fmt,
    io::Read,
    path::{Path, PathBuf},
};

const CATALOG_LIMIT: u64 = 64 * 1024 * 1024;
const CUDA_CONTEXT_MIB: f64 = 512.;
const TTS_WORKING_MIB: f64 = 1536.;
const ASR_WORKING_MIB: f64 = 1024.;
const WEIGHT_MARGIN: f64 = 1.10;
const BUDGET_STEP_MIB: f64 = 512.;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Model {
    pub alias: Alias,
    pub path: PathBuf,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Models {
    pub tts: Model,
    pub asr: Option<Model>,
}

impl Models {
    pub(super) fn aliases(&self) -> String {
        std::iter::once(&self.tts)
            .chain(&self.asr)
            .map(|m| m.alias.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Every alias that could not be used, each with the reason, reported by name.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ModelsNotReady(pub Vec<(String, String)>);

impl ModelsNotReady {
    pub(super) fn aliases(&self) -> String {
        self.0
            .iter()
            .map(|(alias, _)| alias.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl fmt::Display for ModelsNotReady {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<_> = self
            .0
            .iter()
            .map(|(a, why)| format!("{a}: {why}"))
            .collect();
        f.write_str(&parts.join("; "))
    }
}

impl std::error::Error for ModelsNotReady {}

fn read_json(path: &Path) -> Result<Value, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut data = Vec::new();
    file.take(CATALOG_LIMIT + 1)
        .read_to_end(&mut data)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if data.len() as u64 > CATALOG_LIMIT {
        return Err(format!("{} exceeds 64 MiB", path.display()));
    }
    serde_json::from_slice(&data).map_err(|e| format!("{}: {e}", path.display()))
}

fn resolve_one(
    root: &Path,
    alias: &Alias,
    catalog: &Value,
    readiness: &Value,
) -> Result<Model, String> {
    let name = alias.as_str();
    let entry = catalog.get(name).filter(|e| e.is_object()).ok_or_else(|| {
        "not in $MODEL_ROOT/catalog.json; add it to the host catalog and run local-models-sync"
            .to_owned()
    })?;
    if entry["enabled"] == false {
        return Err("disabled in the catalog".into());
    }
    let ready = &readiness[name];
    let status = ready["status"].as_str();
    let files_ready = ready["files"]
        .as_array()
        .into_iter()
        .flatten()
        .all(|f| !f.is_object() || f["status"] == "ready");
    if status != Some("ready") || !files_ready {
        return Err(format!(
            "readiness is {}; run systemctl --user start local-models-sync.service on the host",
            status.unwrap_or("missing")
        ));
    }
    let folder = ready["path"]
        .as_str()
        .filter(|p| !p.is_empty())
        .map_or_else(|| root.join("by-name").join(name), PathBuf::from);
    let files: Vec<&Value> = entry["files"].as_array().into_iter().flatten().collect();
    let absent: Vec<&str> = files
        .iter()
        .filter_map(|f| f["target"].as_str().filter(|t| !t.is_empty()))
        .filter(|target| !folder.join(target).exists())
        .collect();
    if !folder.is_dir() || !absent.is_empty() {
        let listed = if absent.is_empty() {
            "the folder".to_owned()
        } else {
            absent[..absent.len().min(3)].join(", ")
        };
        return Err(format!(
            "files missing under {}: {listed}",
            folder.display()
        ));
    }
    let bytes = files.iter().filter_map(|f| f["size"].as_u64()).sum();
    Ok(Model {
        alias: alias.clone(),
        path: folder,
        bytes,
    })
}

/// Each high-profile alias's ready folder and catalog size, from the model root's catalog and readiness.
/// Nothing is downloaded or copied.
pub(super) fn resolve(high: &HighProfile, root: Option<&Path>) -> Result<Models, ModelsNotReady> {
    let roles: Vec<&Alias> = std::iter::once(&high.tts_model)
        .chain(&high.asr_model)
        .collect();
    let every = |why: &str| {
        ModelsNotReady(
            roles
                .iter()
                .map(|a| (a.as_str().to_owned(), why.to_owned()))
                .collect(),
        )
    };
    let root = match root {
        Some(root) => root.to_path_buf(),
        None => match std::env::var_os("MODEL_ROOT").filter(|r| !r.is_empty()) {
            Some(root) => PathBuf::from(root),
            None => return Err(every("MODEL_ROOT is not set")),
        },
    };
    let load = |name| read_json(&root.join(name)).map(|v| v["entries"].clone());
    let (catalog, readiness) = match (load("catalog.json"), load("readiness.json")) {
        (Ok(catalog), Ok(readiness)) => (catalog, readiness),
        (Err(error), _) | (_, Err(error)) => {
            return Err(every(&format!("cannot read the model catalog: {error}")));
        }
    };

    let mut problems = Vec::new();
    let mut found = Vec::new();
    for alias in &roles {
        match resolve_one(&root, alias, &catalog, &readiness) {
            Ok(model) => found.push(model),
            Err(why) => problems.push((alias.as_str().to_owned(), why)),
        }
    }
    if !problems.is_empty() {
        return Err(ModelsNotReady(problems));
    }
    let mut found = found.into_iter();
    let (Some(tts), asr) = (found.next(), found.next()) else {
        return Err(every("no model resolved"));
    };
    Ok(Models { tts, asr })
}

/// A conservative peak: weights stored in 16 bits (doubled for float32) with a margin, each
/// model's working memory and one CUDA context, rounded up to 512 MiB.
pub(super) fn estimate_vram_mib(models: &Models, precision: HighPrecision) -> u64 {
    let factor = if precision == HighPrecision::Float32 {
        2.
    } else {
        1.
    };
    let weights = |m: &Model| m.bytes as f64 / f64::from(1 << 20) * factor * WEIGHT_MARGIN;
    let mut total = CUDA_CONTEXT_MIB + weights(&models.tts) + TTS_WORKING_MIB;
    if let Some(asr) = &models.asr {
        total += weights(asr) + ASR_WORKING_MIB;
    }
    ((total / BUDGET_STEP_MIB).ceil() * BUDGET_STEP_MIB) as u64
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::profile::config::parse;
    use serde_json::json;

    pub(crate) fn high(extra: Value) -> HighProfile {
        let mut table = json!({"enabled":true,"python":"/gpu/bin/python3"});
        table
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        parse(&json!({ "high": table })).unwrap().high.unwrap()
    }

    /// alias, readiness status, (target, bytes) files, and whether to write the files
    pub(crate) type Entry<'a> = (&'a str, &'a str, &'a [(&'a str, u64)], bool);

    pub(crate) fn model_root(base: &Path, entries: &[Entry<'_>]) {
        let (mut catalog, mut readiness) = (serde_json::Map::new(), serde_json::Map::new());
        for (alias, state, files, write) in entries {
            let folder = base.join("by-name").join(alias);
            catalog.insert((*alias).into(), json!({"enabled":true,"files":files.iter().map(|(t,n)|json!({"target":t,"size":n})).collect::<Vec<_>>()}));
            readiness.insert((*alias).into(), json!({"path":folder,"status":state,"files":files.iter().map(|(t,_)|json!({"target":t,"status":state})).collect::<Vec<_>>()}));
            if *write {
                for (target, _) in *files {
                    let path = folder.join(target);
                    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                    std::fs::write(path, b"x").unwrap();
                }
            }
        }
        std::fs::write(
            base.join("catalog.json"),
            json!({"entries":catalog}).to_string(),
        )
        .unwrap();
        std::fs::write(
            base.join("readiness.json"),
            json!({"entries":readiness}).to_string(),
        )
        .unwrap();
    }

    pub(crate) fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tohru-voice-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn ready_alias_resolves_to_its_folder_and_catalog_size() {
        let base = scratch("ready");
        model_root(
            &base,
            &[(
                "voice-tts",
                "ready",
                &[
                    ("model.safetensors", 3_833_402_552),
                    ("speech_tokenizer/model.safetensors", 682_293_092),
                ],
                true,
            )],
        );
        let models = resolve(&high(json!({})), Some(&base)).unwrap();
        assert_eq!(models.tts.path, base.join("by-name/voice-tts"));
        assert_eq!(models.tts.bytes, 4_515_695_644);
        assert!(models.asr.is_none());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn missing_unready_and_absent_aliases_are_reported_by_name() {
        let base = scratch("missing");
        model_root(
            &base,
            &[
                ("voice-tts", "missing", &[("model.safetensors", 1)], false),
                ("whisper-x", "ready", &[("model.bin", 1)], false),
            ],
        );
        let error = resolve(&high(json!({"asr_model":"whisper-x"})), Some(&base)).unwrap_err();
        assert_eq!(error.aliases(), "voice-tts, whisper-x");
        assert!(error.to_string().contains("local-models-sync"));
        assert!(error.to_string().contains("files missing"));
        let error = resolve(&high(json!({"tts_model":"voice-nope"})), Some(&base)).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("voice-nope: not in $MODEL_ROOT/catalog.json")
        );
        let error = resolve(&high(json!({})), Some(&base.join("absent"))).unwrap_err();
        assert!(error.to_string().contains("cannot read the model catalog"));
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn vram_budget_is_conservative_and_rounded() {
        let model = |bytes| Model {
            alias: high(json!({})).tts_model,
            path: PathBuf::from("/m"),
            bytes,
        };
        let tts = Models {
            tts: model(4_520_159_586),
            asr: None,
        };
        assert_eq!(estimate_vram_mib(&tts, HighPrecision::BFloat16), 7168);
        assert_eq!(estimate_vram_mib(&tts, HighPrecision::Float32), 11776);
        let both = Models {
            asr: Some(model(1_620_000_000)),
            ..tts
        };
        assert_eq!(estimate_vram_mib(&both, HighPrecision::Float16), 9728);
    }
}
