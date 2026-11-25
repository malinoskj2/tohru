use clap::Parser;
use std::path::{Path, PathBuf};

fn model_path(value: &str) -> Result<PathBuf, std::convert::Infallible> {
    Ok(PathBuf::from(value))
}

#[derive(Debug, Parser)]
#[command(about = "Tohru voice service: wake, turn taking, recognition and speech")]
pub struct Args {
    #[arg(long,conflicts_with_all=["evaluate","replay"])]
    pub devices: bool,
    #[arg(
        long,
        conflicts_with = "replay",
        help = "Read isolated verification operations as JSON lines; no microphone or playback"
    )]
    pub evaluate: bool,
    #[arg(
        long,
        help = "Run a 16 kHz mono WAV through the actual wake/turn state machine without a microphone"
    )]
    pub replay: Option<PathBuf>,
    #[arg(long, default_value = "")]
    pub source: String,
    #[arg(long, default_value = "")]
    #[arg(value_parser=model_path)]
    pub vad_model: PathBuf,
    #[arg(long, default_value = "")]
    #[arg(value_parser=model_path)]
    pub asr_model: PathBuf,
    #[arg(long, default_value = "")]
    #[arg(value_parser=model_path)]
    pub turn_model: PathBuf,
    #[arg(long,default_value="kokoro",value_parser=["kokoro","espeak-ng"])]
    pub tts_engine: String,
    #[arg(long, default_value = "")]
    #[arg(value_parser=model_path)]
    pub tts_model: PathBuf,
    #[arg(long, default_value = "tide")]
    pub tts_voicepack: String,
    #[arg(long, default_value = "en-us+f3")]
    pub tts_voice: String,
    #[arg(long, default_value_t = 100, allow_negative_numbers = true)]
    pub tts_speed: i32,
    #[arg(long, default_value_t = 3, allow_negative_numbers = true)]
    pub tts_pitch: i32,
    #[arg(long)]
    pub tts_load_on_conversation: bool,
    #[arg(long,default_value="asr",value_parser=["asr","kws"])]
    pub wake: String,
    #[arg(long, default_value = "")]
    #[arg(value_parser=model_path)]
    pub kws_model: PathBuf,
    #[arg(long,default_value=concat!(env!("CARGO_MANIFEST_DIR"),"/../shell/assistant/voice"))]
    pub assets: PathBuf,
    #[arg(long, help = "Dragon artwork supplied by the embedding Tohru package")]
    pub dragon_art: Option<PathBuf>,
    #[arg(long, default_value = "", help = "assistant.voice_profiles as JSON")]
    pub profiles: String,
    #[arg(
        long,
        default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/../bin/tohru-voice-qwen3-worker"),
        help = "The high profile's worker script, run by assistant.voice_profiles.high.python"
    )]
    pub high_worker: PathBuf,
}

const TRANSDUCER_FILES: &str = "tokens.txt, encoder*.onnx, decoder*.onnx, joiner*.onnx";

fn onnx_problem(setting: &str, configured: &Path, what: &str) -> Option<String> {
    if configured.as_os_str().is_empty() {
        return Some(format!("{setting} is not set ({what})"));
    }
    (!configured.is_file() || configured.extension().is_none_or(|e| e != "onnx")).then(|| {
        format!(
            "{setting}: no .onnx file at {} ({what})",
            configured.display()
        )
    })
}

fn folder_problem(setting: &str, configured: &Path, what: &str) -> Option<String> {
    if configured.as_os_str().is_empty() {
        return Some(format!("{setting} is not set ({what}: {TRANSDUCER_FILES})"));
    }
    let Ok(entries) = std::fs::read_dir(configured) else {
        return Some(format!(
            "{setting}: {} is not a folder ({what}: {TRANSDUCER_FILES})",
            configured.display()
        ));
    };
    let names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let mut missing = Vec::new();
    if !configured.join("tokens.txt").is_file() {
        missing.push("tokens.txt".to_owned());
    }
    for part in ["encoder", "decoder", "joiner"] {
        if !names
            .iter()
            .any(|n| n.starts_with(part) && n.ends_with(".onnx"))
        {
            missing.push(format!("{part}*.onnx"));
        }
    }
    (!missing.is_empty()).then(|| {
        format!(
            "{setting}: {} has no {} ({what})",
            configured.display(),
            missing.join(", ")
        )
    })
}

/// The configured model paths, checked without loading anything.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ModelCheck {
    /// Problems that keep voice off.
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub turn: bool,
}

impl Args {
    pub fn check_models(&self) -> ModelCheck {
        let errors = [
            onnx_problem(
                "assistant.vad_model",
                &self.vad_model,
                "the Silero VAD .onnx file",
            ),
            folder_problem(
                "assistant.asr_model",
                &self.asr_model,
                "the sherpa-onnx streaming Zipformer folder",
            ),
            (self.wake == "kws")
                .then(|| {
                    folder_problem(
                        "assistant.kws_model",
                        &self.kws_model,
                        "the sherpa-onnx keyword-spotter folder",
                    )
                })
                .flatten(),
        ]
        .into_iter()
        .flatten()
        .collect();
        let configured = !self.turn_model.as_os_str().is_empty();
        let turn_problem = configured
            .then(|| {
                onnx_problem(
                    "assistant.turn_model",
                    &self.turn_model,
                    "the Smart Turn v3.2 .onnx file",
                )
            })
            .flatten();
        ModelCheck {
            errors,
            turn: configured && turn_problem.is_none(),
            warnings: turn_problem
                .map(|p| format!("{p}; turns end on silence only"))
                .into_iter()
                .collect(),
        }
    }

    pub fn settings(&self) -> crate::synthesis::Settings {
        crate::synthesis::Settings {
            engine: self.tts_engine.clone(),
            model: self.tts_model.clone(),
            voicepack: self.tts_voicepack.clone(),
            voice: self.tts_voice.clone(),
            speed: self.tts_speed,
            pitch: self.tts_pitch,
            load_on_conversation: self.tts_load_on_conversation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_modes_cannot_open_a_microphone_accidentally() {
        assert!(Args::try_parse_from(["voice", "--evaluate"]).is_ok());
        assert!(Args::try_parse_from(["voice", "--evaluate", "--devices"]).is_err());
        assert!(Args::try_parse_from(["voice", "--evaluate", "--replay", "x.wav"]).is_err());
        assert!(Args::try_parse_from(["voice", "--wake", "unknown"]).is_err());
        assert!(Args::try_parse_from(["voice", "--profiles", "{}"]).is_ok());
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tohru-voice-models-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn args(extra: &[&str]) -> Args {
        Args::try_parse_from(["voice"].into_iter().chain(extra.iter().copied())).unwrap()
    }

    #[test]
    fn complete_paths_pass_and_unset_ones_name_each_setting_and_file() {
        let dir = scratch("complete");
        let asr = dir.join("asr");
        std::fs::create_dir(&asr).unwrap();
        for name in [
            "tokens.txt",
            "encoder.onnx",
            "decoder.onnx",
            "joiner.int8.onnx",
        ] {
            std::fs::write(asr.join(name), "x").unwrap();
        }
        let vad = dir.join("vad.onnx");
        std::fs::write(&vad, "x").unwrap();
        let (vad, asr) = (vad.to_str().unwrap(), asr.to_str().unwrap());
        assert_eq!(
            args(&["--vad-model", vad, "--asr-model", asr]).check_models(),
            ModelCheck::default()
        );

        let unset = args(&[]).check_models();
        assert_eq!(unset.errors.len(), 2);
        assert!(
            unset.errors[0].contains("assistant.vad_model is not set (the Silero VAD .onnx file)")
        );
        assert!(unset.errors[1].contains("assistant.asr_model is not set"));
        assert!(unset.errors[1].contains(TRANSDUCER_FILES));
        assert!(args(&["--wake", "kws"]).check_models().errors[2].contains("assistant.kws_model"));

        std::fs::remove_file(dir.join("asr/joiner.int8.onnx")).unwrap();
        let broken = args(&["--vad-model", asr, "--asr-model", asr]).check_models();
        assert!(broken.errors[0].contains("no .onnx file at"));
        assert!(
            broken.errors[1]
                .ends_with("has no joiner*.onnx (the sherpa-onnx streaming Zipformer folder)")
        );
        assert!(
            args(&["--vad-model", vad, "--asr-model", vad])
                .check_models()
                .errors[0]
                .contains("is not a folder")
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_bad_turn_model_warns_and_falls_back_to_silence() {
        let check = args(&["--turn-model", "/nonexistent/turn.onnx"]).check_models();
        assert!(!check.turn);
        assert_eq!(check.warnings.len(), 1);
        assert!(check.warnings[0].ends_with("; turns end on silence only"));
    }
}
