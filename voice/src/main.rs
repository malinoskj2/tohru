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
    if args.first_audio_report {
        let input = std::io::read_to_string(std::io::stdin())?;
        let samples = tohru_voice_native::latency::samples(input.lines());
        emit(tohru_voice_native::latency::report(&samples));
        return Ok(());
    }
    if args.evaluate {
        return tohru_voice_native::evaluate::run(&args);
    }
    let check = args.check_models();
    ensure!(
        check.errors.is_empty(),
        "Voice is off until its models are set. {}",
        check.errors.join("; ")
    );
    tohru_voice_native::service::run(&args, check)
}

fn main() {
    if let Err(error) = run() {
        emit(json!({"event":"error","message":format!("{error:#}")}));
        std::process::exit(3);
    }
}
