use crate::{
    policy::{self, SpeechBuffer},
    process::OwnedChild,
    profile::{HighError, HighVoice, Profiles},
    protocol::{diagnostic, emit, log},
    synthesis::{self, Clip, Engine, Settings},
};
use anyhow::{Context, Result};
use ort::session::RunOptions;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    io::Write,
    path::PathBuf,
    process::{ChildStdin, Command, Stdio},
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct Job {
    id: u64,
    generation: Value,
    conversation: Value,
    buffer: Mutex<SpeechBuffer>,
}

#[derive(Default)]
struct State {
    generation: u64,
    pending: Option<Arc<Job>>,
    active: Option<Arc<Job>>,
    settings: Option<Settings>,
    shutdown: bool,
    paused: bool,
    speaking: bool,
    infer: Option<Arc<RunOptions>>,
    completed: Vec<String>,
    recent: Vec<(Instant, f32)>,
    last_audio: Option<Instant>,
    unload: bool,
    preload: bool,
    warning: String,
    cacheable: Vec<String>,
    clips: HashMap<(String, String), Clip>,
    warm: VecDeque<String>,
}

const CACHEABLE: usize = 32;
/// Streamed high-voice audio buffered before a sentence starts, so the worker stays ahead.
const PREBUFFER_SECONDS: f64 = 0.36;

/// Part of one sentence's audio on its way to the player.
struct Piece {
    text: String,
    clip: Clip,
    profile: &'static str,
    first: bool,
    last: bool,
}

type Pieces = std::sync::mpsc::SyncSender<Result<Piece>>;

fn streams() -> bool {
    std::env::var_os("TOHRU_VOICE_HIGH_STREAM").is_none_or(|v| v != "0")
}

/// Where speech yields to the owner: the first inter-word dip within 300 ms of `index`, else
/// the quietest point there, with the fade length that follows it.
fn yield_point(samples: &[f32], rate: u32, index: usize) -> (usize, usize) {
    let frame = (rate as usize / 100).max(1);
    let rms = |from: usize| {
        let window = &samples[from..(from + frame).min(samples.len())];
        if window.is_empty() {
            return 0.;
        }
        (window.iter().map(|s| s * s).sum::<f32>() / window.len() as f32).sqrt()
    };
    let around = index.saturating_sub(rate as usize / 2);
    let reach = (index + rate as usize * 3 / 10).min(samples.len());
    let peak = (around..reach)
        .step_by(frame)
        .map(rms)
        .fold(0f32, f32::max);
    let quiet = (peak * 0.12).max(0.006);
    let frames = (index..reach).step_by(frame);
    let (stop, fade) = match frames.clone().find(|&at| rms(at) < quiet) {
        Some(at) => (at, rate as usize * 3 / 100),
        None => (
            frames
                .min_by(|a, b| rms(*a).total_cmp(&rms(*b)))
                .unwrap_or(index),
            rate as usize * 6 / 100,
        ),
    };
    let stop = stop.min(samples.len());
    (stop, fade.min(samples.len() - stop))
}

/// A streamed piece's mouth shapes, from the sentence's phonemes spread over an estimated length.
fn piece_timeline(samples: &[f32], rate: u32, phonemes: &str, offset: f64) -> Vec<(f64, f64, u8)> {
    let chars: Vec<char> = phonemes.chars().collect();
    if chars.is_empty() || rate == 0 {
        return Vec::new();
    }
    let estimate = (chars.len() as f64 * 0.075).max(0.5);
    let length = samples.len() as f64 / f64::from(rate);
    let at = |t: f64| (((t / estimate).clamp(0., 1.)) * chars.len() as f64) as usize;
    let slice: String = chars[at(offset)..at(offset + length).max(at(offset))].iter().collect();
    synthesis::timeline(samples, rate, &slice)
}

#[derive(Clone)]
pub struct Control {
    state: Arc<(Mutex<State>, Condvar)>,
    player: Arc<Mutex<Option<OwnedChild>>>,
    profiles: Profiles,
}

/// One answer's voice: the high profile when it was loaded as the answer began.
struct Voices<'a> {
    low: &'a mut Engine,
    high: Option<(u64, Arc<dyn HighVoice>)>,
}

/// With `TOHRU_VOICE_CLIP_DIR` set, keeps each synthesized clip, untrimmed, named for the
/// answer, its place in it and the profile and engine that made it.
fn record_clip(clip: &Clip, job: &Job, index: usize, profile: &str, engine: &str) {
    let Some(dir) = std::env::var_os("TOHRU_VOICE_CLIP_DIR").filter(|d| !d.is_empty()) else {
        return;
    };
    let name = clip_name(&job.generation, job.id, index, profile, engine);
    let path = PathBuf::from(dir).join(name);
    if let Err(error) = clip.write(&path) {
        log(&format!(
            "cannot keep the clip {}: {error:#}",
            path.display()
        ));
    }
}

fn clip_name(generation: &Value, id: u64, index: usize, profile: &str, engine: &str) -> String {
    let answer = generation
        .as_u64()
        .map_or_else(|| format!("job{id}"), |g| format!("g{g}"));
    let engine: String = engine
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    format!("{answer}-{index:02}-{profile}-{engine}.wav")
}

/// The echo-cancelling device in a `pactl --format=json list` result, never its monitor.
pub fn echo_cancel(devices: &Value) -> Option<String> {
    devices
        .as_array()?
        .iter()
        .filter_map(|d| d["name"].as_str())
        .find(|name| name.contains("echo-cancel") && !name.ends_with(".monitor"))
        .map(str::to_owned)
}

/// What the voice state machine asks of the speaker.
pub trait Speech {
    fn start(&self, conversation: Value, generation: Value) -> u64;
    fn feed(&self, id: u64, text: &str, final_part: bool);
    fn hush(&self);
    fn pause(&self);
    fn resume(&self);
    fn speaking(&self) -> bool;
    fn echo_level(&self) -> f32;
    fn deaf(&self) -> bool;
    fn spoken(&self) -> String;
    fn configure(&self, settings: Settings);
    fn unload(&self);
    fn preload(&self);
    /// Lines worth keeping synthesized, such as acknowledgements: they play without inference.
    fn prepare(&self, _texts: Vec<String>) {}
}

impl Speech for Control {
    fn start(&self, conversation: Value, generation: Value) -> u64 {
        Control::start(self, conversation, generation)
    }

    fn feed(&self, id: u64, text: &str, final_part: bool) {
        Control::feed(self, id, text, final_part);
    }

    fn hush(&self) {
        Control::hush(self);
    }

    fn pause(&self) {
        Control::pause(self);
    }

    fn resume(&self) {
        Control::resume(self);
    }

    fn speaking(&self) -> bool {
        Control::speaking(self)
    }

    fn echo_level(&self) -> f32 {
        Control::echo_level(self)
    }

    fn deaf(&self) -> bool {
        Control::deaf(self)
    }

    fn spoken(&self) -> String {
        Control::spoken(self)
    }

    fn configure(&self, settings: Settings) {
        Control::configure(self, settings);
    }

    fn unload(&self) {
        Control::unload(self);
    }

    fn preload(&self) {
        Control::preload(self);
    }

    fn prepare(&self, texts: Vec<String>) {
        Control::prepare(self, texts);
    }
}

impl Control {
    pub fn spawn(
        settings: Settings,
        assets: PathBuf,
        profiles: Profiles,
    ) -> (Self, JoinHandle<()>) {
        let control = Self {
            state: Arc::new((
                Mutex::new(State {
                    settings: Some(settings),
                    ..Default::default()
                }),
                Condvar::new(),
            )),
            player: Arc::new(Mutex::new(None)),
            profiles,
        };
        let worker = control.clone();
        let handle = thread::spawn(move || worker.run(assets));
        (control, handle)
    }

    pub fn prepare(&self, texts: Vec<String>) {
        let texts: Vec<String> = texts
            .into_iter()
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty() && t.len() <= 200)
            .take(CACHEABLE)
            .collect();
        let mut state = lock(&self.state.0);
        if state.cacheable != texts {
            state.clips.clear();
            state.cacheable.clone_from(&texts);
        }
        state.warm = texts.into();
        self.state.1.notify_all();
    }

    fn cached(&self, profile: &str, text: &str) -> Option<Clip> {
        lock(&self.state.0)
            .clips
            .get(&(profile.to_owned(), text.trim().to_owned()))
            .cloned()
    }

    fn keep(&self, profile: &str, text: &str, clip: &Clip) {
        let mut state = lock(&self.state.0);
        if state.cacheable.iter().any(|t| t == text.trim()) {
            state
                .clips
                .insert((profile.to_owned(), text.trim().to_owned()), clip.clone());
        }
    }

    /// Synthesizes one prepared line while nothing else waits; true when it did work.
    fn warm_one(&self, engine: &mut Engine, settings: &Settings) -> bool {
        let text = {
            let mut state = lock(&self.state.0);
            if state.pending.is_some() || state.active.is_some() {
                return false;
            }
            let Some(text) = state.warm.pop_front() else {
                return false;
            };
            text
        };
        let high = self.profiles.high();
        let profile = high
            .as_ref()
            .map_or_else(|| "low".to_owned(), |(id, _)| format!("high-{id}"));
        if self.cached(&profile, &text).is_some() {
            return true;
        }
        let interrupted = || lock(&self.state.0).pending.is_some();
        let clip = match &high {
            Some((_, voice)) => match voice.speak(&text, &interrupted) {
                Ok(Some((samples, rate))) => {
                    let phonemes = synthesis::espeak_phonemes(&text, "en-us+f3", 175, 50);
                    let timeline = synthesis::timeline(&samples, rate, &phonemes);
                    Some(Clip {
                        samples,
                        rate,
                        phonemes,
                        timeline,
                    })
                }
                _ => None,
            },
            None => RunOptions::new()
                .ok()
                .and_then(|options| engine.synthesize(&text, settings, &options).ok()),
        };
        match clip {
            Some(mut clip) => {
                clip.trim();
                self.keep(&profile, &text, &clip);
                diagnostic(
                    "clip_prepared",
                    json!({"profile":if high.is_some(){"high"}else{"low"}}),
                );
            }
            None => lock(&self.state.0).warm.push_front(text),
        }
        true
    }

    pub fn start(&self, conversation: Value, generation: Value) -> u64 {
        self.hush();
        let mut state = lock(&self.state.0);
        let id = state.generation;
        let job = Arc::new(Job {
            id,
            generation,
            conversation,
            buffer: Mutex::new(SpeechBuffer::default()),
        });
        state.completed.clear();
        state.active = Some(job.clone());
        state.pending = Some(job);
        self.state.1.notify_all();
        id
    }

    pub fn feed(&self, id: u64, text: &str, final_part: bool) {
        let state = lock(&self.state.0);
        if state.generation == id
            && let Some(job) = &state.active
        {
            lock(&job.buffer).feed(text, final_part);
        }
        self.state.1.notify_all();
    }

    pub fn hush(&self) {
        let mut state = lock(&self.state.0);
        let speaking = state.speaking;
        let conversation = state.active.as_ref().map(|j| j.conversation.clone());
        let generation = state.active.as_ref().map(|j| j.generation.clone());
        state.generation = state.generation.wrapping_add(1);
        state.pending = None;
        state.active = None;
        state.paused = false;
        state.speaking = false;
        state.recent.clear();
        if let Some(run) = &state.infer {
            let _ = run.terminate();
        }
        drop(state);
        self.kill_player();
        self.state.1.notify_all();
        if speaking {
            emit(json!({"event":"mouth","level":0,"viseme":0}));
            emit(
                json!({"event":"speaking","on":false,"interrupted":true,"conversation":conversation,"generation":generation}),
            );
        }
        diagnostic("audio_cancel_requested", json!({}));
    }

    pub fn configure(&self, settings: Settings) {
        self.hush();
        lock(&self.state.0).settings = Some(settings);
        self.state.1.notify_all();
    }

    pub fn unload(&self) {
        let mut state = lock(&self.state.0);
        state.unload = true;
        state.preload = false;
        drop(state);
        self.state.1.notify_all();
    }

    pub fn preload(&self) {
        let mut state = lock(&self.state.0);
        state.preload = true;
        state.unload = false;
        drop(state);
        self.state.1.notify_all();
    }

    pub fn warning(&self) -> String {
        lock(&self.state.0).warning.clone()
    }

    pub fn pause(&self) {
        let mut state = lock(&self.state.0);
        if state.speaking && !state.paused {
            state.paused = true;
            state.recent.clear();
            let conversation = state.active.as_ref().map(|j| j.conversation.clone());
            let generation = state.active.as_ref().map(|j| j.generation.clone());
            drop(state);
            emit(json!({"event":"mouth","level":0,"viseme":0}));
            emit(
                json!({"event":"speaking","on":false,"paused":true,"conversation":conversation,"generation":generation}),
            );
        }
    }

    pub fn resume(&self) {
        let mut state = lock(&self.state.0);
        if state.paused {
            state.paused = false;
            let conversation = state.active.as_ref().map(|j| j.conversation.clone());
            let generation = state.active.as_ref().map(|j| j.generation.clone());
            drop(state);
            emit(
                json!({"event":"speaking","on":true,"conversation":conversation,"generation":generation}),
            );
            self.state.1.notify_all();
        }
    }

    pub fn speaking(&self) -> bool {
        lock(&self.state.0).speaking
    }

    pub fn echo_level(&self) -> f32 {
        lock(&self.state.0)
            .recent
            .iter()
            .filter(|(at, _)| at.elapsed() < Duration::from_millis(400))
            .map(|(_, rms)| *rms)
            .fold(0., f32::max)
    }

    pub fn deaf(&self) -> bool {
        let state = lock(&self.state.0);
        state
            .last_audio
            .is_some_and(|at| at.elapsed() < Duration::from_millis(400))
    }

    pub fn spoken(&self) -> String {
        format!(
            "Completed application-paced segments: {}. Current segment playback uncertain; remaining generated text may be unheard.",
            {
                let state = lock(&self.state.0);
                if state.completed.is_empty() {
                    "none".into()
                } else {
                    state.completed.join(" ")
                }
            }
        )
    }

    pub fn shutdown(&self) {
        self.hush();
        lock(&self.state.0).shutdown = true;
        self.state.1.notify_all();
    }

    fn stale(&self, id: u64) -> bool {
        let state = lock(&self.state.0);
        state.shutdown || state.generation != id
    }

    fn kill_player(&self) {
        lock(&self.player).take();
        diagnostic("player_stop_requested", json!({}));
    }

    fn run(&self, assets: PathBuf) {
        let mut settings = Settings::default();
        let mut engine = None;
        loop {
            let job = {
                let mut state = lock(&self.state.0);
                loop {
                    if state.shutdown {
                        return;
                    }
                    if let Some(next) = state.settings.take() {
                        settings = next;
                        engine = None;
                        state.clips.clear();
                        let lines = state.cacheable.clone();
                        state.warm = lines.into();
                    }
                    if state.unload {
                        engine = None;
                        state.unload = false;
                    }
                    if state.preload {
                        state.preload = false;
                        if engine.is_none() {
                            drop(state);
                            engine = self.load_engine(&settings, &assets);
                            state = lock(&self.state.0);
                        }
                        continue;
                    }
                    if let Some(job) = state.pending.take() {
                        break job;
                    }
                    if !state.warm.is_empty()
                        && state.active.is_none()
                        && let Some(engine) = &mut engine
                    {
                        drop(state);
                        let worked = self.warm_one(engine, &settings);
                        state = lock(&self.state.0);
                        if worked {
                            continue;
                        }
                    }
                    if engine.is_none() && !settings.load_on_conversation {
                        drop(state);
                        engine = self.load_engine(&settings, &assets);
                        state = lock(&self.state.0);
                        if engine.is_none() {
                            state = self
                                .state
                                .1
                                .wait(state)
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                        }
                        continue;
                    }
                    state = self
                        .state
                        .1
                        .wait(state)
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                }
            };
            if self.stale(job.id) {
                continue;
            }
            if engine.is_none() {
                engine = self.load_engine(&settings, &assets);
            }
            let result = if let Some(engine) = &mut engine {
                self.play(&job, engine, &settings)
            } else {
                Err(anyhow::anyhow!("speech engine unavailable"))
            };
            self.kill_player();
            if !self.stale(job.id) {
                if let Err(error) = result {
                    emit(json!({"event":"error","message":format!("speech failed: {error:#}")}));
                }
                let mut state = lock(&self.state.0);
                state.speaking = false;
                state.paused = false;
                state.active = None;
                state.infer = None;
                drop(state);
                emit(json!({"event":"mouth","level":0,"viseme":0}));
                emit(
                    json!({"event":"speaking","on":false,"conversation":job.conversation,"generation":job.generation}),
                );
            }
        }
    }

    fn load_engine(&self, settings: &Settings, assets: &std::path::Path) -> Option<Engine> {
        match Engine::load(settings, assets) {
            Ok(engine) => {
                lock(&self.state.0).warning.clear();
                Some(engine)
            }
            Err(error) => {
                let warning = format!("speech settings: {error:#}; using espeak-ng");
                lock(&self.state.0).warning = warning.clone();
                emit(json!({"event":"error","message":warning}));
                Some(Engine::Espeak)
            }
        }
    }

    fn next_part(&self, job: &Job) -> Option<String> {
        loop {
            let mut state = lock(&self.state.0);
            if state.shutdown || state.generation != job.id {
                return None;
            }
            let mut buffer = lock(&job.buffer);
            if let Some(part) = buffer.next_part() {
                return Some(part);
            }
            if buffer.finished() {
                return None;
            }
            drop(buffer);
            state = self
                .state
                .1
                .wait_timeout(state, Duration::from_millis(50))
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
            drop(state);
        }
    }

    fn player(&self, rate: u32) -> Result<ChildStdin> {
        let mut command = Command::new("pacat");
        command.args([
            "--playback",
            "--raw",
            "--format=s16le",
            &format!("--rate={rate}"),
            "--channels=1",
            "--client-name=tohru-voice",
            "--stream-name=Dragon",
            "--latency-msec=60",
        ]);
        let sink = std::env::var("TOHRU_VOICE_SINK")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                let bytes = crate::process::output(
                    Command::new("pactl").args(["--format=json", "list", "sinks"]),
                    b"",
                    Duration::from_secs(5),
                    1024 * 1024,
                )
                .ok()?;
                echo_cancel(&serde_json::from_slice(&bytes).ok()?)
            });
        if let Some(sink) = sink {
            command.arg(format!("--device={sink}"));
        }
        let mut child = OwnedChild(
            command
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .context("start pacat")?,
        );
        let stdin = child.0.stdin.take().context("pacat stdin")?;
        *lock(&self.player) = Some(child);
        Ok(stdin)
    }

    fn play(&self, job: &Job, engine: &mut Engine, settings: &Settings) -> Result<()> {
        let waiting = std::sync::atomic::AtomicUsize::new(0);
        let waiting = &waiting;
        thread::scope(|scope| {
            let (sender, receiver) = std::sync::mpsc::sync_channel(64);
            let producer = scope.spawn(move || {
                if let Err(error) = self.produce(job, engine, settings, &sender, waiting) {
                    let _ = sender.send(Err(error));
                }
            });
            let result = self.play_clips(job, receiver, waiting);
            if result.is_err() && !self.stale(job.id) {
                if let Err(error) = &result {
                    emit(json!({"event":"error","message":format!("speech failed: {error:#}")}));
                }
                self.hush();
            }
            producer
                .join()
                .map_err(|_| anyhow::anyhow!("synthesis producer panicked"))?;
            result
        })
    }

    /// The high profile's clip for one sentence and the engine that made it; `Ok(None)` hands
    /// the sentence to the low engine. With `pieces`, audio goes to the player as it arrives, and
    /// `Ok(Some((clip, true, _)))` says the whole sentence was already sent.
    fn high_clip(
        &self,
        job: &Job,
        voices: &mut Voices<'_>,
        text: &str,
        pieces: Option<&Pieces>,
    ) -> Result<Option<(Clip, bool, String)>, ()> {
        let Some((id, high)) = voices.high.clone() else {
            return Ok(None);
        };
        let spoken = thread::scope(|scope| {
            let phonemes = scope.spawn(|| synthesis::espeak_phonemes(text, "en-us+f3", 175, 50));
            let mut phonemes = Some(phonemes);
            let mut known = String::new();
            let (mut held, mut offset, mut sent) = (Vec::<f32>::new(), 0., false);
            let mut send = |samples: Vec<f32>, rate: u32, last: bool, known: &str, sent: &mut bool| {
                let Some(pieces) = pieces else { return };
                let mut clip = Clip {
                    timeline: piece_timeline(&samples, rate, known, offset),
                    samples,
                    rate,
                    phonemes: String::new(),
                };
                if !*sent {
                    clip.trim_start();
                }
                if last {
                    clip.trim_end();
                }
                offset += clip.samples.len() as f64 / f64::from(rate);
                let _ = pieces.send(Ok(Piece {
                    text: text.to_owned(),
                    clip,
                    profile: "high",
                    first: !*sent,
                    last,
                }));
                *sent = true;
            };
            let mut rate_seen = 0;
            let result = high.speak_streaming(text, &|| self.stale(job.id), &mut |chunk, rate| {
                if pieces.is_none() {
                    return;
                }
                rate_seen = rate;
                held.extend_from_slice(chunk);
                if !sent && (held.len() as f64) < f64::from(rate) * PREBUFFER_SECONDS {
                    return;
                }
                if let Some(handle) = phonemes.take() {
                    known = handle.join().unwrap_or_default();
                    diagnostic("high_stream_start", json!({"generation":job.generation}));
                }
                send(std::mem::take(&mut held), rate, false, &known, &mut sent);
            });
            let phonemes = match phonemes.take() {
                Some(handle) => handle.join().unwrap_or_default(),
                None => known.clone(),
            };
            if let Ok(Some(_)) = &result
                && (sent || !held.is_empty())
                && pieces.is_some()
            {
                send(std::mem::take(&mut held), rate_seen, true, &phonemes, &mut sent);
            }
            (result, phonemes, sent)
        });
        let (result, phonemes, streamed) = spoken;
        match result {
            Ok(Some((samples, rate))) => {
                let timeline = synthesis::timeline(&samples, rate, &phonemes);
                Ok(Some((
                    Clip {
                        samples,
                        rate,
                        phonemes,
                        timeline,
                    },
                    streamed,
                    high.engine().to_owned(),
                )))
            }
            Ok(None) => Err(()),
            Err(HighError::Reply(error)) if !streamed => {
                log(&format!(
                    "voice profile: high ({}) could not speak a sentence ({error}); the low engine speaks it instead",
                    high.engine()
                ));
                Ok(None)
            }
            Err(HighError::Reply(error)) => {
                log(&format!(
                    "voice profile: high ({}) stopped mid-sentence ({error})",
                    high.engine()
                ));
                Err(())
            }
            Err(error @ HighError::Broken(_)) => {
                log(&format!(
                    "voice profile: high ({}) broke mid-answer ({error}); the low engine speaks the rest",
                    high.engine()
                ));
                self.profiles.failed(id, error.to_string());
                voices.high = None;
                if streamed { Err(()) } else { Ok(None) }
            }
        }
    }

    fn produce(
        &self,
        job: &Job,
        engine: &mut Engine,
        settings: &Settings,
        sender: &Pieces,
        waiting: &std::sync::atomic::AtomicUsize,
    ) -> Result<()> {
        use std::sync::atomic::Ordering;
        let mut voices = Voices {
            low: engine,
            high: self.profiles.high(),
        };
        let profile_key = |voices: &Voices<'_>| {
            voices
                .high
                .as_ref()
                .map_or_else(|| "low".to_owned(), |(id, _)| format!("high-{id}"))
        };
        let mut reported: Option<(&str, String)> = None;
        let mut index = 0;
        while let Some(part) = self.next_part(job) {
            for text in policy::chunks(&part) {
                while waiting.load(Ordering::Acquire) > 0 {
                    if self.stale(job.id) {
                        return Ok(());
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                if self.stale(job.id) {
                    return Ok(());
                }
                let options = Arc::new(RunOptions::new()?);
                {
                    let mut state = lock(&self.state.0);
                    if state.generation != job.id {
                        return Ok(());
                    }
                    state.infer = Some(options.clone());
                }
                diagnostic("synthesis_start", json!({"generation":job.generation}));
                let key = profile_key(&voices);
                let mut streamed = false;
                index += 1;
                let (clip, profile, engine) = if let Some(clip) = self.cached(&key, &text) {
                    match &voices.high {
                        Some((_, high)) => (clip, "high", high.engine().to_owned()),
                        None => (clip, "low", voices.low.name().to_owned()),
                    }
                } else {
                    waiting.fetch_add(1, Ordering::AcqRel);
                    let high = self.high_clip(job, &mut voices, &text, streams().then_some(sender));
                    if !matches!(high, Ok(Some((_, true, _)))) {
                        waiting.fetch_sub(1, Ordering::AcqRel);
                    }
                    let (mut clip, profile, engine) = match high {
                        Ok(Some((clip, sent, engine))) => {
                            streamed = sent;
                            (clip, "high", engine)
                        }
                        Ok(None) => (
                            voices.low.synthesize(&text, settings, &options)?,
                            "low",
                            voices.low.name().to_owned(),
                        ),
                        Err(()) => return Ok(()),
                    };
                    record_clip(&clip, job, index, profile, &engine);
                    clip.trim();
                    let kept = if profile == "high" {
                        key.clone()
                    } else {
                        "low".to_owned()
                    };
                    self.keep(&kept, &text, &clip);
                    (clip, profile, engine)
                };
                lock(&self.state.0).infer = None;
                if self.stale(job.id) {
                    return Ok(());
                }
                if reported
                    .as_ref()
                    .is_none_or(|(p, e)| *p != profile || *e != engine)
                {
                    diagnostic(
                        "speech_profile",
                        json!({"generation":job.generation,"profile":profile,"engine":engine}),
                    );
                    reported = Some((profile, engine.clone()));
                }
                diagnostic(
                    "synthesis_ready",
                    json!({"generation":job.generation,"samples":clip.samples.len(),"rate":clip.rate,"profile":profile,"engine":engine}),
                );
                if streamed {
                    continue;
                }
                waiting.fetch_add(1, Ordering::AcqRel);
                let piece = Piece {
                    text,
                    clip,
                    profile,
                    first: true,
                    last: true,
                };
                if sender.send(Ok(piece)).is_err() {
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    fn play_clips(
        &self,
        job: &Job,
        receiver: std::sync::mpsc::Receiver<Result<Piece>>,
        waiting: &std::sync::atomic::AtomicUsize,
    ) -> Result<()> {
        let mut player = None;
        let mut rate = 0;
        let mut first_audio = true;
        let mut due: Option<Instant> = None;
        loop {
            if self.stale(job.id) {
                return Ok(());
            }
            let Piece {
                text,
                clip,
                profile,
                first,
                last,
            } = match receiver.recv_timeout(Duration::from_millis(20)) {
                Ok(piece) => piece?,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            };
            if player.is_none() || rate != clip.rate {
                self.kill_player();
                player = Some(self.player(clip.rate)?);
                rate = clip.rate;
            }
            {
                let mut state = lock(&self.state.0);
                if state.generation != job.id {
                    return Ok(());
                }
                if !state.speaking {
                    state.speaking = true;
                    emit(
                        json!({"event":"speaking","on":true,"conversation":job.conversation,"generation":job.generation}),
                    );
                }
            }
            if first {
                waiting.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
                emit(json!({"event":"sentence","text":text}));
            } else if let Some(due) = due
                && Instant::now() > due + Duration::from_millis(30)
            {
                diagnostic(
                    "high_stream_gap",
                    json!({"generation":job.generation,"ms":(Instant::now() - due).as_secs_f64() * 1e3}),
                );
            }
            let step = rate as usize / 30;
            let mut next = Instant::now();
            let mut index = 0;
            while index < clip.samples.len() {
                let mut resumed = false;
                if self.stale(job.id) {
                    return Ok(());
                }
                if lock(&self.state.0).paused {
                    if index > 0 {
                        let (stop, count) = yield_point(&clip.samples, rate, index);
                        diagnostic(
                            "speech_yield",
                            json!({"generation":job.generation,"ms":(stop + count - index) as f64 * 1e3 / f64::from(rate)}),
                        );
                        let tail: Vec<f32> = clip.samples[index..stop]
                            .iter()
                            .copied()
                            .chain(clip.samples[stop..stop + count].iter().enumerate().map(|(n, s)| {
                                s * (1. - n as f32 / count.saturating_sub(1).max(1) as f32)
                            }))
                            .collect();
                        let mut at = Instant::now();
                        for chunk in tail.chunks(step.max(1)) {
                            if self.stale(job.id) {
                                return Ok(());
                            }
                            let bytes: Vec<u8> = chunk
                                .iter()
                                .map(|s| (s.clamp(-1., 1.) * 32767.) as i16)
                                .flat_map(i16::to_le_bytes)
                                .collect();
                            if let Some(pipe) = &mut player {
                                let _ = pipe.write_all(&bytes);
                                let _ = pipe.flush();
                            }
                            at += Duration::from_secs_f64(chunk.len() as f64 / f64::from(rate));
                            thread::sleep(at.saturating_duration_since(Instant::now()));
                        }
                        // pacat still holds its device latency when the pipe closes.
                        thread::sleep(Duration::from_millis(80));
                        index = stop;
                    }
                    self.kill_player();
                    drop(player.take());
                    let mut state = lock(&self.state.0);
                    while state.paused && state.generation == job.id && !state.shutdown {
                        state = self
                            .state
                            .1
                            .wait_timeout(state, Duration::from_millis(20))
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .0;
                    }
                    drop(state);
                    if self.stale(job.id) {
                        return Ok(());
                    }
                    player = Some(self.player(rate)?);
                    next = Instant::now();
                    resumed = true;
                }
                let end = (index + step).min(clip.samples.len());
                let chunk = &clip.samples[index..end];
                let bytes: Vec<u8> = chunk
                    .iter()
                    .enumerate()
                    .map(|(n, s)| {
                        let ramp = (rate as usize * 25 / 1000).min(chunk.len());
                        let factor = if resumed && n < ramp {
                            n as f32 / ramp.saturating_sub(1).max(1) as f32
                        } else {
                            1.
                        };
                        ((*s * factor).clamp(-1., 1.) * 32767.) as i16
                    })
                    .flat_map(i16::to_le_bytes)
                    .collect();
                if let Some(pipe) = &mut player {
                    pipe.write_all(&bytes)?;
                    pipe.flush()?;
                }
                let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
                let now = Instant::now();
                {
                    let mut state = lock(&self.state.0);
                    state.last_audio = Some(now);
                    state
                        .recent
                        .retain(|(at, _)| at.elapsed() < Duration::from_millis(400));
                    state.recent.push((now, rms));
                }
                let time = index as f64 / rate as f64 + 0.04;
                let mouth = clip
                    .timeline
                    .iter()
                    .find(|(a, b, _)| *a <= time && time < *b)
                    .map_or(0, |(_, _, m)| *m);
                let level = if rms < 0.0001 {
                    0.
                } else {
                    ((20. * rms.log10() + 42.) / 30.).clamp(0., 1.)
                };
                emit(
                    json!({"event":"mouth","level":(level*100.).round_ties_even()/100.,"viseme":if level>0.05{mouth}else{0}}),
                );
                if index == 0 && first {
                    diagnostic("sentence_first_write", json!({"generation":job.generation}));
                    if first_audio {
                        first_audio = false;
                        diagnostic(
                            "first_audio",
                            json!({"generation":job.generation,"profile":profile,"onset_ms":clip.onset_ms()}),
                        );
                    }
                }
                index = end;
                next += Duration::from_secs_f64(chunk.len() as f64 / rate as f64);
                while Instant::now() < next && !self.stale(job.id) && !lock(&self.state.0).paused {
                    thread::sleep(
                        next.saturating_duration_since(Instant::now())
                            .min(Duration::from_millis(5)),
                    );
                }
            }
            due = Some(next);
            if last {
                lock(&self.state.0).completed.push(text);
            }
        }
        drop(player);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{capture, captured};

    #[test]
    fn the_echo_cancel_sink_is_chosen_but_never_a_monitor() {
        let devices = json!([{"name":"alsa_output.pci"},{"name":"tohru-echo-cancel-sink.monitor"},{"name":"tohru-echo-cancel-sink"}]);
        assert_eq!(
            echo_cancel(&devices).as_deref(),
            Some("tohru-echo-cancel-sink")
        );
        assert_eq!(echo_cancel(&json!([{"name":"alsa_output.pci"}])), None);
        assert_eq!(echo_cancel(&json!({})), None);
    }

    fn tone(seconds: f64, level: f32) -> Vec<f32> {
        (0..(24000. * seconds) as usize)
            .map(|n| level * (n as f32 * 0.07).sin())
            .collect()
    }

    #[test]
    fn speech_yields_at_the_next_gap_between_words() {
        let samples = [tone(0.2, 0.5), tone(0.05, 0.), tone(0.4, 0.5)].concat();
        let (stop, fade) = yield_point(&samples, 24000, 2400);
        assert!((4800..=4900).contains(&stop), "{stop}");
        assert_eq!(fade, 720);
    }

    #[test]
    fn without_a_gap_speech_yields_at_its_quietest_point_within_300_ms() {
        let samples = [tone(0.3, 0.5), tone(0.03, 0.1), tone(0.5, 0.5)].concat();
        let (stop, fade) = yield_point(&samples, 24000, 4800);
        assert!((7200..7920).contains(&stop), "{stop}");
        assert_eq!(fade, 1440);
        let (stop, _) = yield_point(&tone(1., 0.5), 24000, 24000 - 100);
        assert!(stop <= 24000);
    }

    #[test]
    fn a_streamed_piece_takes_its_share_of_the_sentence_phonemes() {
        let samples = tone(0.5, 0.5);
        assert!(piece_timeline(&samples, 24000, "", 0.).is_empty());
        assert!(!piece_timeline(&samples, 24000, "həlˈoʊ wˈɜːld mˈaɪ fɹˈɛnd", 0.).is_empty());
    }

    fn control(profiles: Profiles) -> Control {
        Control {
            state: Arc::new((Mutex::new(State::default()), Condvar::new())),
            player: Arc::new(Mutex::new(None)),
            profiles,
        }
    }

    #[test]
    fn a_new_answer_never_inherits_the_previous_playback_evidence() {
        let speaker = control(Profiles::default());
        lock(&speaker.state.0)
            .completed
            .push("Old completed reply.".into());
        let id = speaker.start(json!("new-conversation"), json!(2));
        speaker.feed(id, "New reply awaiting synthesis.", true);
        let evidence = speaker.spoken();
        assert!(!evidence.contains("Old"), "{evidence}");
        assert!(!evidence.contains("New reply"), "{evidence}");
        assert!(evidence.contains("none"), "{evidence}");
    }

    #[test]
    fn pause_and_resume_report_the_paused_answer() {
        capture();
        let speaker = control(Profiles::default());
        let id = speaker.start(json!("c1"), json!(41));
        assert_eq!(id, 1);
        lock(&speaker.state.0).speaking = true;
        captured();
        speaker.pause();
        speaker.pause();
        assert!(lock(&speaker.state.0).paused);
        assert_eq!(
            captured(),
            [
                json!({"event":"mouth","level":0,"viseme":0}),
                json!({"event":"speaking","on":false,"paused":true,"conversation":"c1","generation":41})
            ]
        );
        speaker.resume();
        assert!(!lock(&speaker.state.0).paused);
        assert_eq!(
            captured(),
            [json!({"event":"speaking","on":true,"conversation":"c1","generation":41})]
        );
    }

    type Reply = Result<Option<(Vec<f32>, u32)>, HighError>;

    struct Scripted(Mutex<Vec<Reply>>);

    impl HighVoice for Scripted {
        fn transcribes(&self) -> bool {
            false
        }

        fn engine(&self) -> &str {
            "qwen3-tts voice-tomoko (kuroki_tomoko)"
        }

        fn speak(
            &self,
            _: &str,
            stale: &dyn Fn() -> bool,
        ) -> Result<Option<(Vec<f32>, u32)>, HighError> {
            if stale() {
                return Ok(None);
            }
            lock(&self.0).remove(0)
        }

        fn transcribe(&self, _: &[f32], _: Instant) -> Result<Option<String>, HighError> {
            Ok(None)
        }
    }

    #[test]
    fn high_speech_falls_back_per_sentence_and_drops_a_broken_worker() {
        let high = Arc::new(Scripted(Mutex::new(vec![
            Ok(Some((vec![0.5; 2400], 24000))),
            Ok(Some((vec![0.5; 2400], 24000))),
            Err(HighError::Reply("odd text".into())),
            Err(HighError::Broken("exited".into())),
        ])));
        let speaker = control(Profiles::loaded(high.clone()));
        let id = speaker.start(json!("c1"), json!(1));
        let job = lock(&speaker.state.0).active.clone().unwrap();
        assert_eq!(job.id, id);
        let mut low = Engine::Espeak;
        let mut voices = Voices {
            low: &mut low,
            high: speaker.profiles.high(),
        };
        let (clip, streamed, engine) = speaker
            .high_clip(&job, &mut voices, "Hi.", None)
            .unwrap()
            .unwrap();
        assert_eq!((clip.rate, clip.samples.len(), streamed), (24000, 2400, false));
        assert_eq!(engine, "qwen3-tts voice-tomoko (kuroki_tomoko)");
        let (sender, receiver) = std::sync::mpsc::sync_channel(8);
        let (_, streamed, _) = speaker
            .high_clip(&job, &mut voices, "Hi.", Some(&sender))
            .unwrap()
            .unwrap();
        let piece = receiver.try_recv().unwrap().unwrap();
        assert!(streamed && piece.first && piece.last && piece.profile == "high");
        assert!(receiver.try_recv().is_err(), "a short sentence goes as one piece");
        assert!(
            speaker
                .high_clip(&job, &mut voices, "Odd.", None)
                .unwrap()
                .is_none(),
            "one sentence the worker cannot say goes to low"
        );
        assert!(voices.high.is_some());
        assert!(
            speaker
                .high_clip(&job, &mut voices, "One.", None)
                .unwrap()
                .is_none()
        );
        assert!(
            voices.high.is_none() && speaker.profiles.high().is_none(),
            "a broken worker is dropped for the answer and the service"
        );
        assert!(
            speaker
                .high_clip(&job, &mut voices, "Two.", None)
                .unwrap()
                .is_none()
        );

        let speaker = control(Profiles::loaded(high));
        let id = speaker.start(json!("c1"), json!(2));
        let job = lock(&speaker.state.0).active.clone().unwrap();
        speaker.hush();
        assert!(speaker.stale(id));
        let mut voices = Voices {
            low: &mut low,
            high: speaker.profiles.high(),
        };
        assert!(
            speaker.high_clip(&job, &mut voices, "Hush.", None).is_err(),
            "a hushed answer stops instead of falling back"
        );
    }

    #[test]
    fn kept_clips_name_the_answer_and_the_engine_that_spoke() {
        assert_eq!(
            clip_name(
                &json!(46),
                9,
                3,
                "high",
                "qwen3-tts voice-tomoko (kuroki_tomoko)"
            ),
            "g46-03-high-qwen3-tts-voice-tomoko-kuroki-tomoko.wav"
        );
        assert_eq!(
            clip_name(&Value::Null, 9, 1, "low", "kokoro"),
            "job9-01-low-kokoro.wav"
        );
    }
}
