use clap::Parser;
use std::path::PathBuf;

fn model_path(value: &str) -> Result<PathBuf, std::convert::Infallible> {
    Ok(PathBuf::from(value))
}

#[derive(Debug, Parser)]
#[command(
    about = "Native voice worker; assistant/history and desktop routing retain their existing owners"
)]
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
}

impl Args {
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
    }
}
