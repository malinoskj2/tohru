use crate::{
    Args,
    acoustic::{self, Recognizer, Turn, TurnFeatures, Vad},
    frontend::English,
    policy,
    protocol::{emit, read_command},
    synthesis::Engine,
};
use anyhow::{Context, Result, ensure};
use ort::session::RunOptions;
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    path::Path,
    time::Instant,
};

fn audio(request: &Value) -> Result<Vec<f32>> {
    let path = request["audio"].as_str().context("audio path required")?;
    let wave = sherpa_onnx::Wave::read(path).context("read mono WAV")?;
    ensure!(
        wave.sample_rate() == 16000 && wave.samples().len() <= 16000 * 60,
        "input must be 16 kHz and at most 60 seconds"
    );
    Ok(wave.samples().to_vec())
}

pub fn run(args: &Args) -> Result<()> {
    let mut frontend = None;
    let mut engine = None;
    let mut asr = None;
    let mut vad = None;
    let mut turn = None;
    let mut initialized = false;
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    emit(json!({"event":"evaluation_ready","python":false}));
    while let Some(request) = read_command(&mut reader)? {
        let start = Instant::now();
        let result = (|| -> Result<Value> {
            let operation = request["op"].as_str().context("op required")?;
            let text = request["text"].as_str().unwrap_or("");
            ensure!(
                text.chars().count() <= 16384,
                "text exceeds 16 KiB character budget"
            );
            if ["synthesize", "recognize", "vad", "turn"].contains(&operation) && !initialized {
                acoustic::initialize()?;
                initialized = true;
            }
            match operation {
                "phonemes" => {
                    let phonemes = frontend
                        .get_or_insert_with(English::default)
                        .phonemes(text)?;
                    Ok(json!({"phonemes":phonemes}))
                }
                "fallback" => Ok(json!({"phonemes":crate::frontend::espeak_to_misaki(text)})),
                "synthesize" => {
                    if engine.is_none() {
                        engine = Some(Engine::load(&args.settings(), &args.assets)?);
                    }
                    let options = RunOptions::new()?;
                    let mut clip = if let Some(ipa) = request["phonemes"].as_str() {
                        match engine.as_mut().context("engine")? {
                            Engine::Kokoro(k) => k.from_phonemes(ipa, args.tts_speed, &options)?,
                            Engine::Espeak => anyhow::bail!("fixed phonemes require Kokoro"),
                        }
                    } else {
                        engine.as_mut().context("engine")?.synthesize(
                            text,
                            &args.settings(),
                            &options,
                        )?
                    };
                    if request["trim"] == true {
                        clip.trim();
                    }
                    let path = request["output"]
                        .as_str()
                        .context("output WAV path required")?;
                    clip.write(Path::new(path))?;
                    if let Some(raw) = request["raw_output"].as_str() {
                        let mut file = std::fs::File::create(raw)?;
                        for value in &clip.samples {
                            file.write_all(&value.to_le_bytes())?;
                        }
                    }
                    Ok(
                        json!({"rate":clip.rate,"samples":clip.samples.len(),"phonemes":clip.phonemes,"timeline":clip.timeline,"peak":clip.samples.iter().map(|s|s.abs()).fold(0.,f32::max),"output":path}),
                    )
                }
                "recognize" => {
                    if asr.is_none() {
                        asr = Some(Recognizer::new(&args.asr_model)?);
                    }
                    let recognizer = asr.as_mut().context("ASR")?;
                    recognizer.start();
                    let samples = audio(&request)?;
                    let mut partials = Vec::new();
                    for frame in samples.chunks(512) {
                        recognizer.feed(frame);
                        let text = recognizer.partial();
                        if partials.last() != Some(&text) {
                            partials.push(text);
                        }
                    }
                    Ok(json!({"text":recognizer.finish(),"partials":partials}))
                }
                "vad" => {
                    if vad.is_none() {
                        vad = Some(Vad::new(&args.vad_model)?);
                    }
                    let vad = vad.as_mut().context("VAD")?;
                    vad.reset();
                    let mut probabilities = Vec::new();
                    for frame in audio(&request)?.chunks(512) {
                        let mut padded = [0.; 512];
                        padded[..frame.len()].copy_from_slice(frame);
                        probabilities.push(vad.probability(&padded)?);
                    }
                    Ok(json!({"probabilities":probabilities}))
                }
                "turn" => {
                    if turn.is_none() {
                        turn = Some(Turn::new(&args.turn_model)?);
                    }
                    Ok(
                        json!({"probability":turn.as_mut().context("turn")?.probability(&audio(&request)?)?}),
                    )
                }
                "features" => {
                    let features = TurnFeatures::default().extract(&audio(&request)?);
                    let path = request["output"]
                        .as_str()
                        .context("output raw float32 path required")?;
                    let mut file = std::fs::File::create(path)?;
                    for value in &features {
                        file.write_all(&value.to_le_bytes())?;
                    }
                    Ok(json!({"output":path,"shape":[1,80,800]}))
                }
                "policy" => Ok(
                    json!({"wake":policy::wake_split(text),"sentence":policy::sentence(text),"backchannel":policy::backchannel(text),"unfinished":policy::unfinished(text),"hold":policy::explicit_hold(text),"handoff":policy::explicit_handoff(text),"speakable":policy::speakable(text),"chunks":policy::chunks(text)}),
                ),
                "stream" => {
                    let mut buffer = policy::SpeechBuffer::default();
                    let mut parts = Vec::new();
                    let chunks = request["chunks"]
                        .as_array()
                        .context("chunks array required")?;
                    ensure!(chunks.len() <= 1024, "too many chunks");
                    for (i, chunk) in chunks.iter().enumerate() {
                        buffer.feed(
                            chunk.as_str().context("text chunk required")?,
                            i + 1 == chunks.len(),
                        );
                        while let Some(part) = buffer.next_part() {
                            parts.push(part);
                        }
                    }
                    Ok(json!({"parts":parts}))
                }
                _ => anyhow::bail!("unknown evaluation operation {operation}"),
            }
        })();
        match result {
            Ok(mut value) => {
                value["ok"] = true.into();
                value["id"] = request["id"].clone();
                value["elapsed_ms"] = json!(start.elapsed().as_secs_f64() * 1000.);
                emit(value);
            }
            Err(error) => emit(json!({"ok":false,"id":request["id"],"error":format!("{error:#}")})),
        }
    }
    Ok(())
}
