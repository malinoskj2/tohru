use super::{
    catalog::Models,
    config::{HighProfile, ProfileConfig},
    spawn,
    wire::{self, WireError},
};
use crate::protocol::log;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader},
    os::{
        fd::AsFd,
        unix::{
            fs::DirBuilderExt,
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const RESERVATION_NAME: &str = "Tohru voice (high VRAM)";
const SPEAK_TIMEOUT: Duration = Duration::from_secs(30);
const FRAME_TIMEOUT: Duration = Duration::from_secs(10);
/// Beyond the scheduler queue: loading the checkpoints and warming CUDA graphs.
const LOAD_ALLOWANCE: Duration = Duration::from_secs(300);
const STOP_GRACE: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(50);

#[derive(Debug, thiserror::Error)]
pub enum HighError {
    /// The worker is healthy but could not answer this one request.
    #[error("{0}")]
    Reply(String),
    #[error("the high-VRAM voice worker failed: {0}")]
    Broken(String),
}

/// A loaded high profile, as the speaker and the recogniser use it.
pub trait HighVoice: Send + Sync {
    fn transcribes(&self) -> bool;

    /// The whole clip at its rate, or `None` once `stale` asked for cancellation.
    fn speak(
        &self,
        text: &str,
        stale: &dyn Fn() -> bool,
    ) -> Result<Option<(Vec<f32>, u32)>, HighError>;

    /// `None` when the worker stayed busy past the deadline; the caller keeps its own transcript.
    fn transcribe(&self, audio: &[f32], deadline: Instant) -> Result<Option<String>, HighError>;
}

fn readable(stream: &UnixStream, timeout: Duration) -> Result<bool, HighError> {
    let fd = stream.as_fd();
    let mut fds = [rustix::event::PollFd::new(
        &fd,
        rustix::event::PollFlags::IN,
    )];
    let timeout = rustix::event::Timespec::try_from(timeout).ok();
    match rustix::event::poll(&mut fds, timeout.as_ref()) {
        Ok(count) => Ok(count > 0),
        Err(rustix::io::Errno::INTR) => Ok(false),
        Err(error) => Err(HighError::Broken(error.to_string())),
    }
}

pub(super) struct Client {
    stream: Mutex<UnixStream>,
    asr: bool,
    broken: AtomicBool,
    next: AtomicU64,
}

impl Client {
    fn new(stream: UnixStream, asr: bool) -> std::io::Result<Self> {
        stream.set_read_timeout(Some(FRAME_TIMEOUT))?;
        stream.set_write_timeout(Some(FRAME_TIMEOUT))?;
        Ok(Self {
            stream: Mutex::new(stream),
            asr,
            broken: AtomicBool::new(false),
            next: AtomicU64::new(1),
        })
    }

    fn broken(&self, error: impl std::fmt::Display) -> HighError {
        self.broken.store(true, Ordering::Relaxed);
        HighError::Broken(error.to_string())
    }

    fn check(&self) -> Result<(), HighError> {
        if self.broken.load(Ordering::Relaxed) {
            return Err(HighError::Broken("the connection was already lost".into()));
        }
        Ok(())
    }

    fn reply(&self, header: &Value, id: u64) -> Result<(), HighError> {
        if header["id"].as_u64() != Some(id) {
            return Err(self.broken(format!(
                "reply for request {} while waiting for {id}",
                header["id"]
            )));
        }
        Ok(())
    }

    fn close(&self) {
        let stream = self
            .stream
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = stream.shutdown(std::net::Shutdown::Both);
    }
}

impl HighVoice for Client {
    fn transcribes(&self) -> bool {
        self.asr
    }

    fn speak(
        &self,
        text: &str,
        stale: &dyn Fn() -> bool,
    ) -> Result<Option<(Vec<f32>, u32)>, HighError> {
        self.check()?;
        let mut stream = self
            .stream
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let text: String = text.chars().take(2000).collect();
        wire::write_frame(&mut *stream, json!({"op":"tts","id":id,"text":text}), &[])
            .map_err(|e| self.broken(e))?;
        let deadline = Instant::now() + SPEAK_TIMEOUT;
        let (mut audio, mut rate, mut cancelled) = (Vec::new(), 0, false);
        loop {
            if !cancelled && stale() {
                cancelled = true;
                wire::write_frame(&mut *stream, json!({"op":"cancel","id":id}), &[])
                    .map_err(|e| self.broken(e))?;
            }
            if Instant::now() >= deadline {
                return Err(self.broken("timed out"));
            }
            if !readable(&stream, POLL)? {
                continue;
            }
            let (header, body) = wire::read_frame(&mut *stream).map_err(|e| self.broken(e))?;
            self.reply(&header, id)?;
            match header["type"].as_str() {
                Some("audio") => {
                    let chunk_rate = header["rate"]
                        .as_u64()
                        .and_then(|r| u32::try_from(r).ok())
                        .filter(|r| *r > 0);
                    let Some(chunk_rate) = chunk_rate.filter(|r| rate == 0 || *r == rate) else {
                        return Err(self.broken("audio chunk with a missing or changed rate"));
                    };
                    rate = chunk_rate;
                    if audio.len() * 4 + body.len() > wire::BODY_LIMIT {
                        return Err(self.broken("answer audio exceeds one minute"));
                    }
                    audio.extend(wire::samples(&body));
                }
                Some("done") if cancelled => return Ok(None),
                Some("done") if rate == 0 => {
                    return Err(HighError::Reply("the worker produced no audio".into()));
                }
                Some("done") => return Ok(Some((audio, rate))),
                Some("error") => {
                    return Err(HighError::Reply(
                        header["message"]
                            .as_str()
                            .unwrap_or("unknown")
                            .chars()
                            .take(300)
                            .collect(),
                    ));
                }
                other => return Err(self.broken(format!("unexpected reply {other:?}"))),
            }
        }
    }

    fn transcribe(&self, audio: &[f32], deadline: Instant) -> Result<Option<String>, HighError> {
        self.check()?;
        let mut stream = loop {
            match self.stream.try_lock() {
                Ok(stream) => break stream,
                Err(std::sync::TryLockError::Poisoned(poisoned)) => break poisoned.into_inner(),
                Err(std::sync::TryLockError::WouldBlock) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(std::sync::TryLockError::WouldBlock) => return Ok(None),
            }
        };
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        wire::write_frame(
            &mut *stream,
            json!({"op":"asr","id":id}),
            &wire::bytes(audio),
        )
        .map_err(|e| self.broken(e))?;
        if !readable(&stream, deadline.saturating_duration_since(Instant::now()))? {
            return Err(self.broken("transcription timed out"));
        }
        let (header, _) = wire::read_frame(&mut *stream).map_err(|e| self.broken(e))?;
        self.reply(&header, id)?;
        match header["type"].as_str() {
            Some("text") => Ok(Some(
                header["text"].as_str().unwrap_or("").trim().to_owned(),
            )),
            Some("error") => Err(HighError::Reply(
                header["message"]
                    .as_str()
                    .unwrap_or("unknown")
                    .chars()
                    .take(300)
                    .collect(),
            )),
            other => Err(self.broken(format!("unexpected reply {other:?}"))),
        }
    }
}

/// A private socket directory that only exists for one worker.
struct SocketDir(PathBuf);

impl SocketDir {
    fn create() -> std::io::Result<Self> {
        static COUNT: AtomicU64 = AtomicU64::new(0);
        let base =
            std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
        let name = format!(
            "tohru-voice-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        );
        let path = base.join(name);
        std::fs::DirBuilder::new().mode(0o700).create(&path)?;
        Ok(Self(path))
    }

    fn socket(&self) -> PathBuf {
        self.0.join("worker.sock")
    }
}

impl Drop for SocketDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.socket());
        let _ = std::fs::remove_dir(&self.0);
    }
}

/// `gpu-run` and the worker under it: the reservation lives exactly as long as this child.
struct Reservation {
    child: Child,
}

impl Reservation {
    fn exited(&mut self) -> Option<std::process::ExitStatus> {
        self.child.try_wait().ok().flatten()
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if self.exited().is_some() {
            return;
        }
        let pid = rustix::process::Pid::from_child(&self.child);
        let _ = rustix::process::kill_process(pid, rustix::process::Signal::TERM);
        let deadline = Instant::now() + STOP_GRACE;
        while Instant::now() < deadline {
            if self.exited().is_some() {
                return;
            }
            thread::sleep(POLL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A requested reservation whose worker has not reported ready yet.
pub(super) struct Pending {
    reservation: Reservation,
    listener: UnixListener,
    dir: SocketDir,
    budget_mib: u64,
    queue_timeout: Duration,
}

/// A worker that reported ready; dropping it closes the connection and releases the reservation.
pub(super) struct Running {
    pub client: Arc<Client>,
    _reservation: Reservation,
    _dir: SocketDir,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.client.close();
    }
}

pub(super) struct Plan<'a> {
    pub config: &'a ProfileConfig,
    pub high: &'a HighProfile,
    pub models: &'a Models,
    pub budget_mib: u64,
    pub script: &'a Path,
    /// `gpu-run`, or a stand-in in tests.
    pub launcher: &'a Path,
}

fn spec(plan: &Plan<'_>) -> Value {
    let mut models = json!({"tts": plan.models.tts.path});
    if let Some(asr) = &plan.models.asr {
        models["asr"] = json!(asr.path);
    }
    json!({"protocol":wire::PROTOCOL,"device":"cuda","precision":plan.high.precision.name(),"speaker":plan.high.tts_speaker,"models":models})
}

fn command(plan: &Plan<'_>, socket: &Path) -> Command {
    let mut command = Command::new(plan.launcher);
    command
        .arg("--vram")
        .arg(format!("{}MiB", plan.budget_mib))
        .args(["--name", RESERVATION_NAME, "--timeout"])
        .arg(plan.config.queue_timeout.as_secs().to_string())
        .arg("--")
        .arg(&plan.high.python)
        .arg(plan.script)
        .arg("--socket")
        .arg(socket)
        .arg("--spec")
        .arg(spec(plan).to_string())
        .env_remove("GPU_LEASE_ID")
        .env("HF_HUB_OFFLINE", "1")
        .env("TRANSFORMERS_OFFLINE", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    command
}

fn watch_stderr(child: &mut Child, budget_mib: u64) {
    let Some(stderr) = child.stderr.take() else {
        return;
    };
    thread::spawn(move || {
        for line in BufReader::new(stderr).lines() {
            let Ok(line) = line else { return };
            let line = line.trim();
            if let Some(reason) = line.strip_prefix("gpu-run: waiting:") {
                log(&format!(
                    "voice profile high: reservation queued ({})",
                    reason.trim()
                ));
            } else if line.starts_with("gpu-run: granted") {
                log(&format!(
                    "voice profile high: reservation granted for {budget_mib} MiB; loading models"
                ));
            } else if !line.is_empty() {
                log(&format!(
                    "voice profile high: {}",
                    line.chars().take(300).collect::<String>()
                ));
            }
        }
    });
}

/// Requests the reservation. Call from a thread that outlives the worker: the child is told to
/// exit when the thread that started it goes away.
pub(super) fn start(plan: &Plan<'_>) -> std::io::Result<Pending> {
    let dir = SocketDir::create()?;
    let listener = UnixListener::bind(dir.socket())?;
    listener.set_nonblocking(true)?;
    let mut command = command(plan, &dir.socket());
    spawn::die_with_parent(&mut command);
    let mut child = command.spawn()?;
    watch_stderr(&mut child, plan.budget_mib);
    Ok(Pending {
        reservation: Reservation { child },
        listener,
        dir,
        budget_mib: plan.budget_mib,
        queue_timeout: plan.config.queue_timeout,
    })
}

impl Pending {
    pub(super) fn budget_mib(&self) -> u64 {
        self.budget_mib
    }

    /// Waits through the scheduler queue and the model load for the worker's hello.
    pub(super) fn wait(mut self, cancelled: &AtomicBool) -> Result<Running, String> {
        let deadline = Instant::now() + self.queue_timeout + LOAD_ALLOWANCE;
        let stream = loop {
            if cancelled.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            // A worker that connected and then exited still left its report in the socket.
            match self.listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(format!("accept the worker connection: {error}")),
            }
            if let Some(status) = self.reservation.exited() {
                return Err(format!(
                    "gpu-run or the worker exited with {status} (no reservation, or the load failed)"
                ));
            }
            if Instant::now() >= deadline {
                return Err("the high-VRAM voice worker timed out".into());
            }
            thread::sleep(POLL);
        };
        let hello = (|| -> Result<Value, WireError> {
            stream.set_nonblocking(false)?;
            let mut reader = &stream;
            loop {
                if cancelled.load(Ordering::Relaxed) {
                    return Ok(json!({"type":"error","message":"cancelled"}));
                }
                if Instant::now() >= deadline {
                    return Ok(json!({"type":"error","message":"the high-VRAM voice worker timed out while loading"}));
                }
                if readable(&stream, POLL).map_err(|e| std::io::Error::other(e.to_string()))? {
                    stream.set_read_timeout(Some(FRAME_TIMEOUT))?;
                    return Ok(wire::read_frame(&mut reader)?.0);
                }
            }
        })()
        .map_err(|error| format!("{error}"))?;
        match hello["type"].as_str() {
            Some("ready") if hello["protocol"] == wire::PROTOCOL => {
                let asr = hello["asr"] == true;
                let client = Client::new(stream, asr).map_err(|e| e.to_string())?;
                Ok(Running {
                    client: Arc::new(client),
                    _reservation: self.reservation,
                    _dir: self.dir,
                })
            }
            Some("ready") => Err(format!(
                "the worker speaks protocol {}, not {}",
                hello["protocol"],
                wire::PROTOCOL
            )),
            Some("error") => Err(hello["message"]
                .as_str()
                .unwrap_or("the worker failed to load")
                .chars()
                .take(400)
                .collect()),
            _ => Err("the worker did not report ready".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{
        catalog::{Model, tests::high},
        config::ProfileConfig,
    };
    use std::os::unix::fs::PermissionsExt;

    const FAKE_TORCH: &str = "class _Cuda:\n    @staticmethod\n    def is_available():\n        import os\n        return os.environ.get('FAKE_CUDA') != 'none'\n    @staticmethod\n    def empty_cache():\n        pass\ncuda = _Cuda()\nbfloat16 = float16 = float32 = object()\n";
    const FAKE_NUMPY: &str = "import array\nclass _A:\n    def __init__(self, data):\n        self.data = data if isinstance(data, array.array) else array.array('f', data)\n    def reshape(self, *_):\n        return self\n    def tobytes(self):\n        return self.data.tobytes()\ndef asarray(data, dtype=None):\n    return data if isinstance(data, _A) else _A(data)\ndef frombuffer(body, dtype=None):\n    return array.array('f', body)\n";
    const FAKE_QWEN: &str = "import time\nclass FasterQwen3TTS:\n    @classmethod\n    def from_pretrained(cls, path, **kw):\n        return cls()\n    def warmup(self, **kw):\n        pass\n    def generate_custom_voice_streaming(self, text, speaker, **kw):\n        if text == 'bad':\n            raise ValueError('unspeakable')\n        for _ in range(200 if text == 'long' else 3):\n            time.sleep(0.01)\n            yield [0.25] * 80, 24000, None\n";
    const FAKE_WHISPER: &str = "class _S:\n    text = ' turn on the lights '\nclass WhisperModel:\n    def __init__(self, *a, **kw):\n        pass\n    def transcribe(self, audio, **kw):\n        return [_S()], None\n";

    fn python() -> Option<PathBuf> {
        std::env::var_os("PATH")?
            .to_str()?
            .split(':')
            .map(|dir| Path::new(dir).join("python3"))
            .find(|p| p.is_file())
    }

    struct Stage {
        dir: PathBuf,
        python: PathBuf,
    }

    impl Stage {
        /// A fake `gpu-run` that writes its arguments, then either grants and runs the real worker
        /// against stub model libraries, or times out in the queue.
        fn new(name: &str, queue_timeout: bool, cuda: bool) -> Option<Self> {
            let python = python()?;
            let dir = crate::profile::catalog::tests::scratch(name);
            let fakes = dir.join("fakes");
            std::fs::create_dir(&fakes).unwrap();
            for (file, text) in [
                ("torch.py", FAKE_TORCH),
                ("numpy.py", FAKE_NUMPY),
                ("faster_qwen3_tts.py", FAKE_QWEN),
                ("faster_whisper.py", FAKE_WHISPER),
            ] {
                std::fs::write(fakes.join(file), text).unwrap();
            }
            let run = if queue_timeout {
                "echo 'gpu-run: waiting: behind Video render in the queue' >&2\nsleep 0.2\necho 'gpu-run: error: Timed out waiting for a GPU reservation' >&2\nexit 1".to_owned()
            } else {
                format!(
                    "echo 'gpu-run: granted shared reservation for Tohru voice (high VRAM)' >&2\nwhile [ \"$1\" != -- ]; do shift; done\nshift\nexport PYTHONPATH='{}' FAKE_CUDA='{}'\nexec \"$@\"",
                    fakes.display(),
                    if cuda { "yes" } else { "none" }
                )
            };
            let launcher = dir.join("gpu-run");
            std::fs::write(
                &launcher,
                format!(
                    "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n{run}\n",
                    dir.join("args").display()
                ),
            )
            .unwrap();
            std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
            Some(Self { dir, python })
        }

        fn start(&self, asr: bool) -> std::io::Result<Pending> {
            let mut high = high(serde_json::json!({"python": self.python}));
            if asr {
                high.asr_model = high.tts_model.clone().into();
            }
            let model = |path: &str| Model {
                alias: high.tts_model.clone(),
                path: path.into(),
                bytes: 1,
            };
            let models = Models {
                tts: model("/m/voice-tts"),
                asr: asr.then(|| model("/m/whisper")),
            };
            let config = ProfileConfig::default();
            let script =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../bin/tohru-voice-qwen3-worker");
            start(&Plan {
                config: &config,
                high: &high,
                models: &models,
                budget_mib: 7168,
                script: &script,
                launcher: &self.dir.join("gpu-run"),
            })
        }

        fn args(&self) -> Vec<String> {
            std::fs::read_to_string(self.dir.join("args"))
                .unwrap()
                .lines()
                .map(str::to_owned)
                .collect()
        }
    }

    impl Drop for Stage {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn the_reservation_is_requested_held_while_loaded_and_released_on_drop() {
        let Some(stage) = Stage::new("grant", false, true) else {
            return eprintln!("python3 is not on PATH; skipped");
        };
        let running = stage
            .start(true)
            .unwrap()
            .wait(&AtomicBool::new(false))
            .unwrap();
        let args = stage.args();
        assert_eq!(
            args[..7],
            [
                "--vram",
                "7168MiB",
                "--name",
                RESERVATION_NAME,
                "--timeout",
                "60",
                "--"
            ]
        );
        assert_eq!(Path::new(&args[7]), stage.python);
        assert_eq!(args[9..11], ["--socket".to_owned(), args[10].clone()]);
        let spec: Value = serde_json::from_str(args.last().unwrap()).unwrap();
        assert_eq!(
            spec["models"],
            serde_json::json!({"tts":"/m/voice-tts","asr":"/m/whisper"})
        );
        assert_eq!(
            (spec["precision"].as_str(), spec["protocol"].as_u64()),
            (Some("bfloat16"), Some(wire::PROTOCOL))
        );

        let client = running.client.clone();
        assert!(client.transcribes());
        let (audio, rate) = client.speak("Hello.", &|| false).unwrap().unwrap();
        assert_eq!((audio.len(), rate), (240, 24000));
        assert!(audio.iter().all(|s| (*s - 0.25).abs() < 1e-6));
        assert!(
            matches!(client.speak("bad", &|| false), Err(HighError::Reply(e)) if e.contains("unspeakable"))
        );
        let text = client
            .transcribe(&[0.; 512], Instant::now() + Duration::from_secs(3))
            .unwrap();
        assert_eq!(
            text.as_deref(),
            Some("turn on the lights"),
            "one failed sentence keeps the worker"
        );

        let started = Instant::now();
        assert!(
            client
                .speak("long", &|| started.elapsed() > Duration::from_millis(100))
                .unwrap()
                .is_none()
        );
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "cancellation stops generation between chunks"
        );
        assert_eq!(
            client.speak("Hello.", &|| false).unwrap().unwrap().0.len(),
            240,
            "the stream stays in step after a cancel"
        );

        let pid = rustix::process::Pid::from_child(&running._reservation.child);
        let socket = running._dir.socket();
        drop(running);
        assert!(
            rustix::process::test_kill_process(pid).is_err(),
            "the worker is gone"
        );
        assert!(!socket.exists());
        assert!(matches!(
            client.speak("Hello.", &|| false),
            Err(HighError::Broken(_))
        ));
    }

    #[test]
    #[ignore = "loads TOHRU_VOICE_GPU_PYTHON's Qwen3-TTS from $MODEL_ROOT under a real gpu-run reservation"]
    fn the_real_worker_speaks_through_a_real_reservation() {
        let python = std::env::var("TOHRU_VOICE_GPU_PYTHON").expect("TOHRU_VOICE_GPU_PYTHON");
        let model =
            std::env::var("TOHRU_VOICE_GPU_MODEL").unwrap_or_else(|_| "voice-tomoko".into());
        let speaker =
            std::env::var("TOHRU_VOICE_GPU_SPEAKER").unwrap_or_else(|_| "kuroki_tomoko".into());
        let high =
            high(serde_json::json!({"python":python,"tts_model":model,"tts_speaker":speaker}));
        let models = crate::profile::catalog::resolve(&high, None).unwrap();
        let config = ProfileConfig::default();
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../bin/tohru-voice-qwen3-worker");
        let budget_mib = crate::profile::catalog::estimate_vram_mib(&models, high.precision);
        let started = Instant::now();
        let running = start(&Plan {
            config: &config,
            high: &high,
            models: &models,
            budget_mib,
            script: &script,
            launcher: Path::new("gpu-run"),
        })
        .unwrap()
        .wait(&AtomicBool::new(false))
        .unwrap();
        eprintln!("budget {budget_mib} MiB, loaded in {:?}", started.elapsed());

        for text in [
            "Hello, I'm here. What do you need?",
            "It's about eight fluid ounces, so a quart is four cups.",
        ] {
            let begin = Instant::now();
            let (audio, rate) = running.client.speak(text, &|| false).unwrap().unwrap();
            let seconds = audio.len() as f64 / f64::from(rate);
            eprintln!(
                "{:?} for {seconds:.2} s of audio at {rate} Hz",
                begin.elapsed()
            );
            assert_eq!(rate, 24000);
            assert!(seconds > 1.0 && audio.iter().any(|s| s.abs() > 0.01));
        }

        let begin = Instant::now();
        let long = "This is a much longer answer that keeps going so that there is plenty of time to cancel it before it could possibly finish speaking.";
        assert!(
            running
                .client
                .speak(long, &|| begin.elapsed() > Duration::from_millis(200))
                .unwrap()
                .is_none()
        );
        eprintln!("cancelled after {:?}", begin.elapsed());
        assert!(begin.elapsed() < Duration::from_secs(1));
        assert!(running.client.speak("Okay.", &|| false).unwrap().is_some());

        let pid = rustix::process::Pid::from_child(&running._reservation.child);
        drop(running);
        assert!(rustix::process::test_kill_process(pid).is_err());
    }

    #[test]
    fn a_queue_timeout_or_load_error_is_reported_and_leaves_nothing_running() {
        let Some(queued) = Stage::new("queued", true, true) else {
            return eprintln!("python3 is not on PATH; skipped");
        };
        let error = queued
            .start(false)
            .unwrap()
            .wait(&AtomicBool::new(false))
            .err()
            .unwrap();
        assert!(error.contains("exited with exit status: 1"), "{error}");

        let Some(no_cuda) = Stage::new("no-cuda", false, false) else {
            return;
        };
        let error = no_cuda
            .start(false)
            .unwrap()
            .wait(&AtomicBool::new(false))
            .err()
            .unwrap();
        assert!(error.contains("torch reports no CUDA device"), "{error}");
    }

    #[test]
    fn a_cancelled_wait_stops_at_once() {
        let Some(stage) = Stage::new("cancel", true, true) else {
            return eprintln!("python3 is not on PATH; skipped");
        };
        let started = Instant::now();
        assert_eq!(
            stage
                .start(false)
                .unwrap()
                .wait(&AtomicBool::new(true))
                .err()
                .as_deref(),
            Some("cancelled")
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
