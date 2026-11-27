use crate::{
    acoustic::{
        self, Activity, LowPrecision, Recognizer, Spotter, Transcriber, Turn, TurnEnd, Vad, WINDOW,
        WakeSpotter,
    },
    policy,
    process::OwnedChild,
    profile::HighError,
    profile::Profiles,
    protocol::{diagnostic, emit, log},
    speaker::{Control, Speech},
    synthesis::Settings,
};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

// Orca admits a turn only once it is settled: the owner has not resumed within SETTLE_MS
// of quiet after it. A resumed utterance holds it until its words decide.
const SETTLE_MS: u32 = 352;
const SETTLE_CAP_MS: u32 = 2496;
const HIGH_ASR_TIMEOUT: Duration = Duration::from_secs(3);

// The acknowledgement drafted for a partial request; the bar keeps its line ready.
struct AckDraft {
    text: String,
    token: String,
}

impl AckDraft {
    fn cancel(self) {
        emit(json!({"event":"ack_cancel","token":self.token}));
    }
}

struct Settling {
    turn_id: String,
    quiet_ms: u32,
    total_ms: u32,
}

impl Settling {
    fn new(turn_id: String) -> Self {
        Self {
            turn_id,
            quiet_ms: 0,
            total_ms: 0,
        }
    }

    /// One 32 ms window; true once the turn has settled.
    fn tick(&mut self, in_speech: bool) -> bool {
        self.total_ms += 32;
        if !in_speech {
            self.quiet_ms += 32;
        }
        self.quiet_ms >= SETTLE_MS || self.total_ms >= SETTLE_CAP_MS
    }
}

/// The models one utterance passes through; real sessions, or fakes in tests.
pub struct Acoustics {
    pub vad: Box<dyn Activity>,
    pub asr: Box<dyn Transcriber>,
    pub turn: Option<Box<dyn TurnEnd>>,
    pub spotter: Option<Box<dyn WakeSpotter>>,
}

impl Acoustics {
    /// Loads the configured sessions. A Smart Turn model that fails to load is a warning:
    /// turns then end on silence alone.
    pub fn load(
        args: &crate::Args,
        precision: LowPrecision,
        check: &mut crate::config::ModelCheck,
    ) -> Result<Self> {
        let turn: Option<Box<dyn TurnEnd>> = if !check.turn {
            None
        } else {
            match Turn::new(&args.turn_model) {
                Ok(turn) => Some(Box::new(turn)),
                Err(error) => {
                    check.warnings.push(format!(
                        "turn detection unavailable: {error:#}; turns end on silence only"
                    ));
                    None
                }
            }
        };
        let spotter: Option<Box<dyn WakeSpotter>> = if args.wake == "kws" {
            Some(Box::new(Spotter::new(
                &args.kws_model,
                &args.assets.join("wake-keywords.txt"),
                precision,
            )?))
        } else {
            None
        };
        Ok(Self {
            vad: Box::new(Vad::new(&args.vad_model)?),
            asr: Box::new(Recognizer::new(&args.asr_model, precision)?),
            turn,
            spotter,
        })
    }
}

pub struct Voice {
    vad: Box<dyn Activity>,
    asr: Box<dyn Transcriber>,
    turn: Option<Box<dyn TurnEnd>>,
    spotter: Option<Box<dyn WakeSpotter>>,
    speaker: Box<dyn Speech>,
    profiles: Profiles,
    until: Option<Instant>,
    thinking: bool,
    cancelled: bool,
    last_turn: String,
    pending: String,
    settling: Option<Settling>,
    in_speech: bool,
    hearing: bool,
    hearing_eligible: bool,
    start_run: u32,
    quiet_ms: u32,
    speech_ms: u32,
    final_speech_ms: u32,
    last_speech_ns: Option<u64>,
    audio: Vec<f32>,
    preroll: VecDeque<Vec<f32>>,
    spotting: bool,
    turn_checked: bool,
    turn_complete: Option<bool>,
    draft: String,
    draft_count: u32,
    ack: Option<AckDraft>,
    ack_count: u32,
    delivering_ack: Option<AckDraft>,
    semantic: bool,
    semantic_decision: Option<String>,
    semantic_text: String,
    semantic_token: Option<String>,
    semantic_warned: bool,
    turn_id: String,
    display: String,
    barge_ms: u32,
    barge_pending: bool,
    barge_decide_ms: u32,
    echo_coupling: f32,
    aec: bool,
    generation: Value,
    speech_job: Option<u64>,
    first_text: bool,
    /// Her name was heard in this utterance and the high profile was asked to load.
    named: bool,
    paused: bool,
    load_on_conversation: bool,
    was_playing: bool,
    last_vad: (f32, f32),
    suppressed: HashMap<&'static str, (Instant, u32)>,
}

impl Voice {
    pub fn new(
        acoustics: Acoustics,
        speaker: Box<dyn Speech>,
        profiles: Profiles,
        load_on_conversation: bool,
    ) -> Self {
        let Acoustics {
            vad,
            asr,
            turn,
            spotter,
        } = acoustics;
        Self {
            vad,
            asr,
            turn,
            spotter,
            speaker,
            profiles,
            until: None,
            thinking: false,
            cancelled: false,
            last_turn: String::new(),
            pending: String::new(),
            settling: None,
            in_speech: false,
            hearing: false,
            hearing_eligible: false,
            start_run: 0,
            quiet_ms: 0,
            speech_ms: 0,
            final_speech_ms: 0,
            last_speech_ns: None,
            audio: Vec::new(),
            preroll: VecDeque::new(),
            spotting: false,
            turn_checked: false,
            turn_complete: None,
            draft: String::new(),
            draft_count: 0,
            ack: None,
            ack_count: 0,
            delivering_ack: None,
            semantic: std::env::var("TOHRU_VOICE_SEMANTIC").as_deref() == Ok("on"),
            semantic_decision: None,
            semantic_text: String::new(),
            semantic_token: None,
            semantic_warned: false,
            turn_id: String::new(),
            display: String::new(),
            barge_ms: 0,
            barge_pending: false,
            barge_decide_ms: 0,
            echo_coupling: 1.,
            aec: false,
            generation: Value::Null,
            speech_job: None,
            first_text: false,
            named: false,
            paused: false,
            load_on_conversation,
            was_playing: false,
            last_vad: (0., 0.),
            suppressed: HashMap::new(),
        }
    }

    fn active(&self) -> bool {
        self.until.is_some_and(|until| Instant::now() < until)
    }

    fn extend(&mut self) {
        self.until = Some(Instant::now() + Duration::from_secs(60));
    }

    fn conversation(&self, on: bool) {
        emit(json!({"event":"conversation","on":on,"turn_id":self.turn_id}));
        if on {
            self.profiles.wake();
        }
        if !on && self.load_on_conversation {
            self.speaker.unload();
        } else if on && self.load_on_conversation {
            self.speaker.preload();
        }
    }

    fn reset(&mut self) {
        if self.hearing {
            emit(json!({"event":"hearing","on":false}));
        }
        self.hearing = false;
        self.in_speech = false;
        self.start_run = 0;
        self.quiet_ms = 0;
        self.speech_ms = 0;
        self.audio.clear();
        self.named = false;
        self.turn_checked = false;
        self.turn_complete = None;
        self.barge_ms = 0;
        self.barge_pending = false;
        self.barge_decide_ms = 0;
        self.draft.clear();
        self.draft_count = 0;
        if let Some(ack) = self.ack.take() {
            ack.cancel();
        }
        self.ack_count = 0;
        self.preroll.clear();
        self.spotting = false;
        self.asr.clear();
        if let Some(spotter) = &mut self.spotter {
            spotter.clear();
        }
        self.semantic_decision = None;
        self.semantic_text.clear();
        self.semantic_token = None;
    }

    fn discard(&mut self) {
        self.semantic_decision = None;
        self.semantic_text.clear();
        self.semantic_token = None;
        if !self.draft.is_empty() {
            self.draft.clear();
            emit(json!({"event":"draft_cancel"}));
        }
    }

    fn candidate(&self, text: &str) -> String {
        if text.is_empty() {
            return String::new();
        }
        if self.active() {
            let request = policy::wake_split(text).unwrap_or_else(|| policy::sentence(text));
            if self.pending.is_empty() {
                request
            } else {
                policy::join(&self.pending, &request)
            }
        } else {
            policy::wake_split(text).unwrap_or_default()
        }
    }

    fn maybe_draft(&mut self, partial: &str) {
        if self.semantic
            && self
                .semantic_decision
                .as_deref()
                .is_some_and(|s| ["error", "unsupported"].contains(&s))
        {
            return;
        }
        if self.spotting
            || self.barge_pending
            || self.thinking
            || self.draft_count >= 2
            || !self.draft.is_empty()
            || (!self.semantic && self.turn_complete == Some(false))
        {
            return;
        }
        let request = self.candidate(partial);
        if request.is_empty() || (!self.semantic && policy::unfinished(&request)) {
            return;
        }
        let norm = policy::normalize(&request);
        if !self.semantic && norm.split_whitespace().count() < 4 {
            return;
        }
        self.draft_count += 1;
        self.draft = norm;
        let token = crate::protocol::elapsed_ns().to_string();
        self.semantic_token = Some(token.clone());
        emit(json!({"event":"draft","text":request,"semantic":self.semantic,"token":token}));
    }

    /// Asks for an acknowledgement of the request so far once the owner pauses, so the line is
    /// ready when the endpoint fires; a changed partial cancels it.
    fn maybe_ack(&mut self, partial: &str) {
        if self.spotting || self.barge_pending || self.thinking || self.ack.is_some() {
            return;
        }
        if self.ack_count >= 2 || self.turn_complete == Some(false) {
            return;
        }
        let request = self.candidate(partial);
        if request.is_empty() || policy::unfinished(&request) || policy::backchannel(&request) {
            return;
        }
        let text = policy::normalize(&request);
        if text.split_whitespace().count() < 2 {
            return;
        }
        self.ack_count += 1;
        let token = crate::protocol::elapsed_ns().to_string();
        emit(json!({"event":"ack_draft","text":request,"token":token}));
        self.ack = Some(AckDraft { text, token });
    }

    fn drop_stale_ack(&mut self, partial: &str) {
        let request = policy::normalize(&self.candidate(partial));
        if self.ack.as_ref().is_some_and(|ack| ack.text != request)
            && let Some(ack) = self.ack.take()
        {
            ack.cancel();
        }
    }

    /// The drafted acknowledgement's token when it was drafted for exactly this request.
    fn claim_ack(&mut self, request: &str) -> Option<String> {
        let ack = self.delivering_ack.take()?;
        if ack.text == policy::normalize(request) {
            Some(ack.token)
        } else {
            ack.cancel();
            None
        }
    }

    fn remember_frame(&mut self, frame: &[f32], limit: usize) {
        self.preroll.push_back(frame.to_vec());
        while self.preroll.len() > limit {
            self.preroll.pop_front();
        }
    }

    fn begin(&mut self) {
        self.turn_id = crate::protocol::elapsed_ns().to_string();
        diagnostic(
            "utterance_start",
            json!({"turn_id":self.turn_id,"vad_probability":self.last_vad.0,"rms":self.last_vad.1,"conversation":self.active()}),
        );
        self.in_speech = true;
        self.quiet_ms = 0;
        self.speech_ms = 0;
        self.named = false;
        self.turn_checked = false;
        self.turn_complete = None;
        self.audio = self.preroll.iter().flatten().copied().collect();
        self.preroll.clear();
        self.spotting = self.spotter.is_some() && !self.active();
        self.hearing_eligible = self.active();
        if self.spotting {
            if let Some(spotter) = &mut self.spotter {
                spotter.start();
                for frame in self.audio.chunks(WINDOW) {
                    spotter.feed(frame);
                }
            }
        } else {
            self.asr.start();
            for frame in self.audio.chunks(WINDOW) {
                self.asr.feed(frame);
            }
        }
    }

    fn suppression(&mut self, reason: &'static str, probability: f32, rms: f32) {
        let entry = self
            .suppressed
            .entry(reason)
            .or_insert((Instant::now() - Duration::from_secs(5), 0));
        entry.1 += 1;
        if entry.0.elapsed() >= Duration::from_secs(5) {
            diagnostic(
                "capture_suppressed",
                json!({"reason":reason,"windows":entry.1,"vad_probability":probability,"rms":rms}),
            );
            *entry = (Instant::now(), 0);
        }
    }

    fn wake_decision(&self, accepted: bool, reason: &str, candidate: &str) {
        let words = policy::words(candidate);
        let (at, count) =
            policy::wake_match(&words).unwrap_or((0, policy::wake_split_width(&words)));
        let candidate: Vec<String> = words
            .iter()
            .skip(at)
            .take(count)
            .map(|w| {
                w.chars()
                    .filter(char::is_ascii_alphabetic)
                    .take(16)
                    .collect()
            })
            .collect();
        diagnostic(
            "wake_decision",
            json!({"turn_id":self.turn_id,"accepted":accepted,"reason":reason,"candidate":candidate,"speech_ms":if accepted {self.final_speech_ms}else{self.speech_ms},"mode":if self.spotter.is_some(){"kws"}else{"asr"}}),
        );
    }
}

impl Voice {
    fn settle_window(&mut self) {
        let in_speech = self.in_speech;
        if self.settling.as_mut().is_some_and(|s| s.tick(in_speech))
            && let Some(settling) = self.settling.take()
        {
            emit(json!({"event":"settled","turn_id":settling.turn_id}));
        }
    }

    fn playback_changed(&mut self) {
        let playing = self.speaker.speaking();
        if playing && !self.was_playing {
            self.thinking = false;
        }
        if !playing && self.was_playing && self.until.is_some() {
            self.extend();
        }
        self.was_playing = playing;
    }
}

mod capture;
mod commands;
#[cfg(test)]
mod tests;
mod turn;

pub use capture::{device_report, devices, run};

#[cfg(test)]
mod settle_tests {
    use super::*;

    #[test]
    fn quiet_settles_after_the_window_and_speech_holds_it() {
        let mut quiet = Settling::new("t".into());
        let windows = SETTLE_MS / 32;
        assert!((1..windows).all(|_| !quiet.tick(false)));
        assert!(quiet.tick(false));

        let mut resumed = Settling::new("t".into());
        assert!((0..windows / 2).all(|_| !resumed.tick(false)));
        assert!((0..windows * 2).all(|_| !resumed.tick(true)));
        assert!((windows / 2..windows - 1).all(|_| !resumed.tick(false)));
        assert!(resumed.tick(false));
    }

    #[test]
    fn speech_without_words_cannot_hold_a_turn_forever() {
        let mut noisy = Settling::new("t".into());
        assert!((1..SETTLE_CAP_MS / 32).all(|_| !noisy.tick(true)));
        assert!(noisy.tick(true));
    }
}
