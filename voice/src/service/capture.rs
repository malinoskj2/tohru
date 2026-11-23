use super::*;

pub fn devices() -> Result<Value> {
    let bytes = crate::process::output(
        Command::new("pactl").args(["--format=json", "list", "sources"]),
        b"",
        Duration::from_secs(5),
        1024 * 1024,
    )?;
    serde_json::from_slice(&bytes).context("parse Pulse source metadata")
}

pub fn device_report(args: &crate::Args) -> Result<Value> {
    let devices = devices()?;
    let (selected, aec) = source(args)?;
    let sources: Vec<_> = devices
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| d["name"].as_str())
        .collect();
    Ok(
        json!({"event":"devices","sources":sources,"selected":selected,"aec":if aec{"module-reported (effectiveness unmeasured)"}else{"unknown"}}),
    )
}

fn source(args: &crate::Args) -> Result<(String, bool)> {
    let devices = devices()?;
    let list = devices
        .as_array()
        .context("Pulse sources must be an array")?;
    let configured = std::env::var("TOHRU_VOICE_SOURCE")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| (!args.source.is_empty()).then(|| args.source.clone()));
    let selected = configured
        .or_else(|| {
            list.iter()
                .find(|d| {
                    d["name"].as_str().is_some_and(|name| {
                        name.contains("echo-cancel") && !name.ends_with(".monitor")
                    })
                })
                .and_then(|d| d["name"].as_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| {
            crate::process::output(
                Command::new("pactl").arg("get-default-source"),
                b"",
                Duration::from_secs(5),
                65536,
            )
            .map(|s| String::from_utf8_lossy(&s).trim().into())
            .unwrap_or_default()
        });
    let aec = list
        .iter()
        .find(|d| d["name"] == selected)
        .is_some_and(|d| {
            d["properties"]["factory.name"] != "support.null-audio-sink"
                && (d["driver"] == "module-echo-cancel.c"
                    || d["properties"]["aec.method"] == "webrtc")
        });
    Ok((selected, aec))
}

enum Input {
    Command(Value),
    Frame(u64, Vec<f32>),
    CaptureEnd(u64),
    Eof,
    Error(String),
}

fn capture(
    sender: mpsc::SyncSender<Input>,
    epoch: u64,
    source: &str,
) -> Result<(OwnedChild, thread::JoinHandle<()>)> {
    let mut command = Command::new("parec");
    command.args([
        "--raw",
        "--format=s16le",
        "--rate=16000",
        "--channels=1",
        "--latency-msec=32",
        "--client-name=tohru-voice",
        "--stream-name=Wake word",
    ]);
    if !source.is_empty() {
        command.arg(format!("--device={source}"));
    }
    let mut child = OwnedChild(
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("start microphone capture")?,
    );
    let mut stdout = child.0.stdout.take().context("capture stdout")?;
    let reader = thread::spawn(move || {
        loop {
            let mut bytes = [0_u8; WINDOW * 2];
            if stdout.read_exact(&mut bytes).is_err() {
                let _ = sender.send(Input::CaptureEnd(epoch));
                return;
            }
            let samples = bytes
                .chunks_exact(2)
                .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.)
                .collect();
            if sender.send(Input::Frame(epoch, samples)).is_err() {
                return;
            }
        }
    });
    Ok((child, reader))
}

fn locked(path: &Path) -> bool {
    use std::fs::OpenOptions;
    let Ok(file) = OpenOptions::new().read(true).open(path) else {
        return false;
    };
    rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockShared).is_err()
}

pub fn run(args: &crate::Args) -> Result<()> {
    acoustic::initialize()?;
    let (speaker, worker) = Control::spawn(args.settings(), args.assets.clone());
    let mut voice = Voice::new(args, speaker.clone())?;
    emit(
        json!({"event":"ready","turn":voice.turn.is_some(),"warning":speaker.warning(),"dragon":args.dragon_art.clone().unwrap_or_else(||args.assets.parent().unwrap_or(&args.assets).join("dragon"))}),
    );
    if let Some(path) = &args.replay {
        let wave = sherpa_onnx::Wave::read(&path.to_string_lossy()).context("read replay WAV")?;
        anyhow::ensure!(
            wave.sample_rate() == 16000,
            "replay requires 16 kHz mono WAV"
        );
        for frame in wave.samples().chunks(WINDOW) {
            let mut padded = [0.; WINDOW];
            padded[..frame.len()].copy_from_slice(frame);
            voice.step(&padded)?;
        }
        for _ in 0..200 {
            voice.step(&[0.; WINDOW])?;
        }
        speaker.shutdown();
        worker
            .join()
            .map_err(|_| anyhow::anyhow!("speaker worker panicked"))?;
        emit(json!({"event":"replay_done"}));
        return Ok(());
    }
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, stop.clone())?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, stop.clone())?;
    let (sender, receiver) = mpsc::sync_channel(64);
    let commands = sender.clone();
    thread::spawn(move || {
        let stdin = io::stdin();
        let mut input = stdin.lock();
        loop {
            match crate::protocol::read_command(&mut input) {
                Ok(Some(value)) => {
                    if commands.send(Input::Command(value)).is_err() {
                        return;
                    }
                }
                Ok(None) => {
                    let _ = commands.send(Input::Eof);
                    return;
                }
                Err(error) => {
                    let _ = commands.send(Input::Error(error.to_string()));
                    return;
                }
            }
        }
    });
    let lock_path = std::env::var_os("TOHRU_VOICE_LOCK_FILE")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_RUNTIME_DIR").map(|p| PathBuf::from(p).join("tohru-lock.lock"))
        })
        .unwrap_or_default();
    let mut capture_child = None;
    let mut readers = Vec::new();
    let mut epoch = 0;
    let mut retry = Instant::now();
    let mut lock_check = Instant::now() - Duration::from_secs(2);
    let result = (|| -> Result<()> {
        while !stop.load(Ordering::Relaxed) && !crate::protocol::closed() {
            voice.playback_changed();
            readers.retain(|reader: &thread::JoinHandle<()>| !reader.is_finished());
            if lock_check.elapsed() >= Duration::from_secs(2) {
                lock_check = Instant::now();
                let paused = locked(&lock_path);
                if paused != voice.paused {
                    voice.paused = paused;
                    if paused {
                        voice.cancelled = false;
                        speaker.hush();
                        capture_child.take();
                        epoch += 1;
                        voice.vad.reset();
                        voice.reset();
                        emit(json!({"event":"listening","on":false}));
                    }
                    emit(json!({"event":"paused","reason":if paused{"locked"}else{""}}));
                }
            }
            if capture_child.is_none() && !voice.paused && Instant::now() >= retry {
                match source(args).and_then(|(source,aec)|{voice.aec=aec;let result=capture(sender.clone(),epoch,&source)?;emit(json!({"event":"listening","on":true,"source":source,"aec":if aec{"module-reported"}else{"unknown"}}));diagnostic("capture_start",json!({"aec":aec}));Ok(result)}){Ok((child,reader))=>{capture_child=Some(child);readers.push(reader);},Err(error)=>{emit(json!({"event":"error","message":format!("capture: {error:#}")}));retry=Instant::now()+Duration::from_secs(2);}}
            }
            match receiver.recv_timeout(Duration::from_millis(20)) {
                Ok(Input::Command(message)) => voice.command(message)?,
                Ok(Input::Frame(id, frame)) => {
                    if id == epoch && !voice.paused {
                        voice.step(&frame)?;
                    }
                }
                Ok(Input::CaptureEnd(id)) => {
                    if id == epoch {
                        capture_child.take();
                        epoch += 1;
                        voice.vad.reset();
                        voice.reset();
                        emit(json!({"event":"listening","on":false}));
                        emit(json!({"event":"error","message":"the microphone stream ended"}));
                        retry = Instant::now() + Duration::from_secs(2);
                    }
                }
                Ok(Input::Eof) => break,
                Ok(Input::Error(error)) => {
                    emit(json!({"event":"error","message":error}));
                    break;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        Ok(())
    })();
    capture_child.take();
    drop(receiver);
    drop(sender);
    for reader in readers {
        let _ = reader.join();
    }
    speaker.shutdown();
    worker
        .join()
        .map_err(|_| anyhow::anyhow!("speaker worker panicked"))?;
    result
}
