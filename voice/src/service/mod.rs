use crate::{
    acoustic::{self, Recognizer, Spotter, Turn, Vad, WINDOW},
    policy,
    process::OwnedChild,
    protocol::{diagnostic, emit},
    speaker::Control,
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

pub struct Voice {
    vad: Vad,
    asr: Recognizer,
    turn: Option<Turn>,
    spotter: Option<Spotter>,
    pub speaker: Control,
    until: Option<Instant>,
    thinking: bool,
    cancelled: bool,
    last_turn: String,
    pending: String,
    in_speech: bool,
    hearing: bool,
    start_run: u32,
    quiet_ms: u32,
    speech_ms: u32,
    final_speech_ms: u32,
    last_speech_ns: Option<u64>,
    audio: Vec<f32>,
    preroll: VecDeque<Vec<f32>>,
    rejected: bool,
    spotting: bool,
    turn_checked: bool,
    turn_complete: Option<bool>,
    draft: String,
    draft_count: u32,
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
    paused: bool,
    load_on_conversation: bool,
    was_playing: bool,
    last_vad: (f32, f32),
    suppressed: HashMap<&'static str, (Instant, u32)>,
}

impl Voice {
    pub fn new(args: &crate::Args, speaker: Control) -> Result<Self> {
        let vad = Vad::new(&args.vad_model)?;
        let asr = Recognizer::new(&args.asr_model)?;
        let turn = if args.turn_model.as_os_str().is_empty() {
            None
        } else {
            match Turn::new(&args.turn_model) {
                Ok(turn) => Some(turn),
                Err(error) => {
                    emit(
                        json!({"event":"error","message":format!("turn detection unavailable: {error:#}")}),
                    );
                    None
                }
            }
        };
        let spotter = if args.wake == "kws" {
            Some(Spotter::new(
                &args.kws_model,
                &args.assets.join("wake-keywords.txt"),
            )?)
        } else {
            None
        };
        Ok(Self {
            vad,
            asr,
            turn,
            spotter,
            speaker,
            until: None,
            thinking: false,
            cancelled: false,
            last_turn: String::new(),
            pending: String::new(),
            in_speech: false,
            hearing: false,
            start_run: 0,
            quiet_ms: 0,
            speech_ms: 0,
            final_speech_ms: 0,
            last_speech_ns: None,
            audio: Vec::new(),
            preroll: VecDeque::new(),
            rejected: false,
            spotting: false,
            turn_checked: false,
            turn_complete: None,
            draft: String::new(),
            draft_count: 0,
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
            paused: false,
            load_on_conversation: args.tts_load_on_conversation,
            was_playing: false,
            last_vad: (0., 0.),
            suppressed: HashMap::new(),
        })
    }

    fn active(&self) -> bool {
        self.until.is_some_and(|until| Instant::now() < until)
    }

    fn extend(&mut self) {
        self.until = Some(Instant::now() + Duration::from_secs(60));
    }

    fn conversation(&self, on: bool) {
        emit(json!({"event":"conversation","on":on,"turn_id":self.turn_id}));
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
        self.rejected = false;
        self.turn_checked = false;
        self.turn_complete = None;
        self.barge_ms = 0;
        self.barge_pending = false;
        self.barge_decide_ms = 0;
        self.draft.clear();
        self.draft_count = 0;
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
        if self.rejected
            || self.spotting
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
        self.turn_checked = false;
        self.turn_complete = None;
        self.rejected = false;
        self.audio = self.preroll.iter().flatten().copied().collect();
        self.preroll.clear();
        self.spotting = self.spotter.is_some() && !self.active();
        if self.active() {
            self.hearing = true;
            emit(json!({"event":"hearing","on":true}));
        }
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
        let count = if words
            .first()
            .is_some_and(|w| ["to", "too", "two", "toe", "tow", "toh"].contains(&w.as_str()))
        {
            2
        } else {
            1
        };
        let candidate: Vec<String> = words
            .iter()
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
mod turn;
pub use capture::{device_report, devices, run};
