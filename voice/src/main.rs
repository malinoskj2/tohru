use anyhow::{Result, ensure};
use clap::Parser;
use serde_json::json;
use tohru_voice_native::{Args, protocol::emit};

fn run() -> Result<()> {
    let args = Args::parse();
    if args.devices {
        emit(tohru_voice_native::service::device_report(&args)?);
        return Ok(());
    }
    if args.evaluate {
        return tohru_voice_native::evaluate::run(&args);
    }
    ensure!(
        args.vad_model.is_file() && args.asr_model.is_dir(),
        "voice requires --vad-model and --asr-model"
    );
    tohru_voice_native::service::run(&args)
}

fn main() {
    if let Err(error) = run() {
        emit(json!({"event":"error","message":format!("{error:#}")}));
        std::process::exit(3);
    }
}
