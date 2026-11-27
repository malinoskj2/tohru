use super::*;
use crate::protocol::{capture, captured};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Default)]
struct Heard {
    partials: VecDeque<String>,
    last: String,
    final_text: String,
    fed: usize,
    dropped: bool,
}

#[derive(Clone)]
struct FakeRecognizer(Rc<RefCell<Heard>>);

impl FakeRecognizer {
    fn new(partials: &[&str], final_text: &str) -> Self {
        Self(Rc::new(RefCell::new(Heard {
            partials: partials.iter().map(|p| (*p).to_owned()).collect(),
            final_text: final_text.to_owned(),
            ..Heard::default()
        })))
    }

    fn fed(&self) -> usize {
        self.0.borrow().fed
    }
}

impl Transcriber for FakeRecognizer {
    fn start(&mut self) {
        self.0.borrow_mut().dropped = false;
    }

    fn feed(&mut self, _: &[f32]) {
        self.0.borrow_mut().fed += 1;
    }

    fn partial(&self) -> String {
        let mut heard = self.0.borrow_mut();
        if heard.partials.len() > 1 {
            let next = heard.partials.pop_front().unwrap_or_default();
            heard.last = next.clone();
            return next;
        }
        heard.partials.front().cloned().unwrap_or_default()
    }

    fn finish(&mut self) -> String {
        let heard = self.0.borrow();
        if heard.dropped {
            String::new()
        } else {
            heard.final_text.clone()
        }
    }

    fn clear(&mut self) {
        self.0.borrow_mut().dropped = true;
    }
}

#[derive(Default)]
struct Spotted {
    hit_at: Option<usize>,
    fed: usize,
    flushed: usize,
}

#[derive(Clone)]
struct FakeSpotter(Rc<RefCell<Spotted>>);

impl WakeSpotter for FakeSpotter {
    fn start(&mut self) {}

    fn clear(&mut self) {}

    fn feed(&mut self, audio: &[f32]) -> bool {
        let mut spotted = self.0.borrow_mut();
        if audio.len() != WINDOW {
            spotted.flushed += 1;
            return false;
        }
        spotted.fed += 1;
        spotted.hit_at.is_some_and(|at| spotted.fed >= at)
    }
}

#[derive(Clone)]
struct FakeVad(Rc<Cell<f32>>);

impl Activity for FakeVad {
    fn probability(&mut self, _: &[f32]) -> Result<f32> {
        Ok(self.0.get())
    }

    fn reset(&mut self) {}
}

#[derive(Default)]
struct Spoken {
    speaking: bool,
    echo: f32,
    quiet: bool,
    said: Vec<(Value, String)>,
    hushed: usize,
    pauses: usize,
    resumes: usize,
    spoken: String,
    preloads: usize,
    unloads: usize,
}

#[derive(Clone, Default)]
struct FakeSpeaker(Rc<RefCell<Spoken>>);

impl Speech for FakeSpeaker {
    fn start(&self, conversation: Value, _: Value) -> u64 {
        let mut spoken = self.0.borrow_mut();
        spoken.said.push((conversation, String::new()));
        spoken.said.len() as u64 - 1
    }

    fn feed(&self, id: u64, text: &str, _: bool) {
        if let Some((_, said)) = self.0.borrow_mut().said.get_mut(id as usize) {
            said.push_str(text);
        }
    }

    fn hush(&self) {
        let mut spoken = self.0.borrow_mut();
        spoken.speaking = false;
        spoken.hushed += 1;
    }

    fn pause(&self) {
        self.0.borrow_mut().pauses += 1;
    }

    fn resume(&self) {
        self.0.borrow_mut().resumes += 1;
    }

    fn speaking(&self) -> bool {
        self.0.borrow().speaking
    }

    fn echo_level(&self) -> f32 {
        self.0.borrow().echo
    }

    fn deaf(&self) -> bool {
        self.0.borrow().quiet
    }

    fn spoken(&self) -> String {
        self.0.borrow().spoken.clone()
    }

    fn configure(&self, _: Settings) {}

    fn unload(&self) {
        self.0.borrow_mut().unloads += 1;
    }

    fn preload(&self) {
        self.0.borrow_mut().preloads += 1;
    }
}

struct Rig {
    voice: Voice,
    asr: FakeRecognizer,
    spotter: Option<FakeSpotter>,
    vad: Rc<Cell<f32>>,
    speaker: FakeSpeaker,
}

fn rig(partials: &[&str], final_text: &str) -> Rig {
    build(partials, final_text, None)
}

fn build(partials: &[&str], final_text: &str, hit_at: Option<Option<usize>>) -> Rig {
    capture();
    let asr = FakeRecognizer::new(partials, final_text);
    let spotter = hit_at.map(|hit_at| {
        FakeSpotter(Rc::new(RefCell::new(Spotted {
            hit_at,
            ..Spotted::default()
        })))
    });
    let vad = Rc::new(Cell::new(0.));
    let speaker = FakeSpeaker::default();
    let acoustics = Acoustics {
        vad: Box::new(FakeVad(vad.clone())),
        asr: Box::new(asr.clone()),
        turn: None,
        spotter: spotter.clone().map(|s| Box::new(s) as Box<dyn WakeSpotter>),
    };
    let mut voice = Voice::new(
        acoustics,
        Box::new(speaker.clone()),
        Profiles::default(),
        false,
    );
    voice.semantic = false;
    Rig {
        voice,
        asr,
        spotter,
        vad,
        speaker,
    }
}

const SILENCE: [f32; WINDOW] = [0.; WINDOW];

impl Rig {
    fn hear(&mut self, speech: bool) {
        self.voice.hear(&SILENCE, speech).unwrap();
    }

    fn speak(&mut self, windows: usize, quiet: usize) {
        self.voice.begin();
        for _ in 0..windows {
            self.hear(true);
        }
        for _ in 0..quiet {
            if !self.voice.in_speech {
                break;
            }
            self.hear(false);
        }
    }

    fn step(&mut self, level: f32) {
        self.voice.step(&[level; WINDOW]).unwrap();
    }

    fn swap(&mut self, partials: &[&str], final_text: &str) {
        self.asr = FakeRecognizer::new(partials, final_text);
        self.voice.asr = Box::new(self.asr.clone());
    }

    fn spoken(&self) -> std::cell::Ref<'_, Spoken> {
        self.speaker.0.borrow()
    }
}

/// Emitted events without transcripts, diagnostics or capture identifiers.
fn events(all: &[Value]) -> Vec<Value> {
    all.iter()
        .filter(|e| {
            e.get("stage").is_none()
                && !["transcript", "ack_draft", "ack_cancel"]
                    .contains(&e["event"].as_str().unwrap_or(""))
        })
        .map(|e| {
            let mut e = e.clone();
            e.as_object_mut().map(|o| {
                o.remove("turn_id");
                o.remove("ack")
            });
            e
        })
        .collect()
}

fn names(all: &[Value]) -> Vec<String> {
    events(all)
        .iter()
        .map(|e| e["event"].as_str().unwrap_or("").to_owned())
        .collect()
}

fn diagnostics<'a>(all: &'a [Value], stage: &str) -> Vec<&'a Value> {
    all.iter().filter(|e| e["stage"] == stage).collect()
}

#[test]
fn wake_rejection_logs_a_bounded_candidate_and_reason_without_the_request() {
    let mut rig = rig(&["TOTALLY PRIVATE"], "TOTALLY PRIVATE REQUEST CONTENT");
    rig.speak(20, 40);
    let all = captured();
    let decisions = diagnostics(&all, "wake_decision");
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0]["reason"], "final_name_mismatch");
    assert_eq!(decisions[0]["candidate"], json!(["totally"]));
    assert!(!format!("{all:?}").contains("PRIVATE"));
}

#[test]
fn suppression_logging_is_rate_limited_and_records_vad() {
    let mut rig = rig(&[""], "");
    for _ in 0..1000 {
        rig.voice.suppression("echo_cooldown", 0.8, 0.1);
    }
    let all = captured();
    let logged = diagnostics(&all, "capture_suppressed");
    assert_eq!(logged.len(), 1);
    assert_eq!(
        (
            logged[0]["reason"].as_str(),
            logged[0]["vad_probability"].as_f64()
        ),
        (Some("echo_cooldown"), Some(0.800000011920929))
    );
}

#[test]
fn ui_diagnostics_filter_text_and_unknown_stages() {
    let mut rig = rig(&[""], "");
    rig.voice.command(json!({"cmd":"diagnostic","stage":"ui_state_observed","visible":true,"text":"private","turn_id":"capture"})).unwrap();
    rig.voice
        .command(json!({"cmd":"diagnostic","stage":"arbitrary","text":"private"}))
        .unwrap();
    let all = captured();
    assert_eq!(all.len(), 1);
    assert_eq!(
        (
            all[0]["stage"].as_str(),
            all[0]["visible"].as_bool(),
            all[0]["turn_id"].as_str()
        ),
        (Some("ui_state_observed"), Some(true), Some("capture"))
    );
    assert!(!all[0].to_string().contains("private"));
}

#[test]
fn background_synthesis_releases_foreground_capture_before_audio_is_ready() {
    let mut rig = rig(&["CREATE A WORKTREE"], "CREATE A WORKTREE");
    rig.voice.extend();
    rig.speak(10, 25);
    assert!(rig.voice.thinking);
    assert_eq!(rig.voice.last_turn, "Create a worktree");
    rig.voice
        .command(json!({"cmd":"speech_start","generation":2,"background":true}))
        .unwrap();
    rig.voice
        .command(json!({"cmd":"speech_end","generation":2,"text":"I will start that work."}))
        .unwrap();
    assert!(!rig.voice.thinking);
    assert!(rig.voice.last_turn.is_empty());
    rig.swap(&["ARE YOU STILL THERE"], "ARE YOU STILL THERE");
    captured();
    rig.speak(10, 25);
    let all = events(&captured());
    assert_eq!(
        all.last().unwrap(),
        &json!({"event":"turn","text":"Are you still there"})
    );
    assert!(!all.iter().any(|e| e["continuation"] == true));
    rig.voice
        .command(json!({"cmd":"speech_start","generation":3}))
        .unwrap();
    assert!(rig.voice.thinking);
}

#[test]
fn hidden_task_admission_releases_thinking_without_resetting_active_capture() {
    let mut rig = rig(&["CREATE A WORKTREE"], "CREATE A WORKTREE");
    rig.voice.extend();
    rig.speak(10, 25);
    rig.voice.pending = "Create a worktree".into();
    rig.swap(&["ARE YOU STILL THERE"], "ARE YOU STILL THERE");
    rig.voice.begin();
    let capture = rig.voice.turn_id.clone();
    rig.voice
        .command(json!({"cmd":"task_accepted","request":"orca:hidden","generation":2}))
        .unwrap();
    assert!(!rig.voice.thinking && rig.voice.last_turn.is_empty() && rig.voice.pending.is_empty());
    assert!(rig.voice.in_speech);
    assert_eq!(rig.voice.turn_id, capture);
    captured();
    for _ in 0..10 {
        rig.hear(true);
    }
    for _ in 0..25 {
        if !rig.voice.in_speech {
            break;
        }
        rig.hear(false);
    }
    assert_eq!(
        events(&captured()).last().unwrap(),
        &json!({"event":"turn","text":"Are you still there"})
    );
}

#[test]
fn a_name_late_in_the_utterance_wakes_without_earlier_partials_showing() {
    let mut rig = rig(
        &[
            "CAN",
            "CAN YOU CHECK",
            "CAN YOU CHECK THIS",
            "CAN YOU CHECK THIS TOHRU",
        ],
        "CAN YOU CHECK THIS TOHRU",
    );
    rig.speak(80, 40);
    let all = captured();
    let shown: std::collections::BTreeSet<_> = all
        .iter()
        .filter(|e| e["event"] == "transcript")
        .map(|e| e["text"].as_str().unwrap_or("").to_owned())
        .collect();
    assert_eq!(shown, ["Can you check this".to_owned()].into());
    assert_eq!(names(&all), ["conversation", "turn"]);
    assert_eq!(events(&all)[1]["text"], "Can you check this");
    let decision = diagnostics(&all, "wake_decision")[0];
    assert_eq!(
        (decision["accepted"].as_bool(), &decision["candidate"]),
        (Some(true), &json!(["tohru"]))
    );
}

#[test]
fn the_name_wakes_after_hey_mid_sentence_but_a_common_word_form_does_not() {
    let mut hey = rig(&["HEY", "HEY TOHRU WHAT"], "HEY TOHRU WHAT'S THE WEATHER");
    hey.speak(20, 40);
    let all = events(&captured());
    assert_eq!(all.last().unwrap()["text"], "What's the weather");

    let mut middle = rig(&["SO"], "SO TOHRU CAN YOU CHECK THE BUILD");
    middle.speak(20, 40);
    assert_eq!(
        events(&captured()).last().unwrap(),
        &json!({"event":"turn","text":"So can you check the build"})
    );

    let mut lunch = rig(&["I HAD"], "I HAD TARO FOR LUNCH");
    lunch.speak(20, 40);
    assert!(events(&captured()).is_empty());
}

#[test]
fn the_spotter_hears_a_late_name_and_replays_the_whole_utterance() {
    let mut rig = build(
        &["CAN YOU CHECK THIS TOHRU"],
        "CAN YOU CHECK THIS TOHRU",
        Some(Some(70)),
    );
    rig.speak(80, 40);
    assert!(rig.asr.fed() >= 70);
    assert_eq!(rig.spotter.as_ref().unwrap().0.borrow().flushed, 0);
    assert_eq!(
        events(&captured()).last().unwrap(),
        &json!({"event":"turn","text":"Can you check this"})
    );
}

#[test]
fn the_spotter_keeps_listening_through_a_short_pause() {
    let mut rig = build(&["CHECK THIS TOHRU"], "CHECK THIS TOHRU", Some(Some(26)));
    rig.voice.begin();
    for _ in 0..20 {
        rig.hear(true);
    }
    for _ in 0..(220 / 32 + 2) {
        rig.hear(false);
    }
    assert!(rig.voice.spotting && rig.voice.in_speech);
    rig.speak_more(10, 40);
    assert_eq!(
        events(&captured()).last().unwrap(),
        &json!({"event":"turn","text":"Check this"})
    );
}

impl Rig {
    fn speak_more(&mut self, windows: usize, quiet: usize) {
        for _ in 0..windows {
            self.hear(true);
        }
        for _ in 0..quiet {
            if !self.voice.in_speech {
                break;
            }
            self.hear(false);
        }
    }
}

#[test]
fn speech_without_the_wake_word_is_discarded_unheard() {
    let mut rig = rig(&["WHAT", "WHAT TIME IS IT"], "WHAT TIME IS IT");
    rig.speak(20, 40);
    assert!(rig.asr.0.borrow().dropped);
    assert!(!rig.voice.in_speech);
    assert!(events(&captured()).is_empty());
}

#[test]
fn the_wake_word_opens_a_conversation_and_follow_ups_need_none() {
    let mut rig = rig(&["TO", "TOHRU WHAT"], "TOHRU WHAT TIME IS IT");
    rig.speak(20, 40);
    let all = captured();
    assert_eq!(names(&all), ["conversation", "turn"]);
    assert_eq!(events(&all)[1]["text"], "What time is it");
    assert!(rig.voice.active() && rig.voice.thinking);

    let mut follow = self::rig(&["AND TOMORROW"], "AND TOMORROW");
    follow.voice.extend();
    follow.speak(20, 40);
    assert_eq!(
        events(&captured()),
        [
            json!({"event":"hearing","on":true}),
            json!({"event":"hearing","on":false}),
            json!({"event":"turn","text":"And tomorrow"})
        ]
    );
}

#[test]
fn a_wordless_onset_in_a_conversation_never_takes_the_floor() {
    let mut rig = rig(&[""], "");
    rig.voice.extend();
    rig.speak(5, 40);
    assert!(!rig.voice.in_speech);
    assert!(events(&captured()).is_empty());
}

#[test]
fn the_keyword_spotter_gates_the_recogniser_and_the_transcript_confirms_it() {
    let mut woken = build(&["TOHRU WHAT"], "TOHRU WHAT TIME IS IT", Some(Some(12)));
    woken.speak(20, 40);
    assert!(woken.asr.fed() >= 12);
    let all = captured();
    assert_eq!(names(&all), ["conversation", "turn"]);
    assert_eq!(events(&all)[1]["text"], "What time is it");

    let mut unflagged = build(&["TOHRU"], "TOHRU WHAT TIME IS IT", Some(None));
    unflagged.speak(60, 40);
    assert_eq!(unflagged.asr.fed(), 0);
    assert_eq!(unflagged.spotter.as_ref().unwrap().0.borrow().flushed, 1);
    assert!(events(&captured()).is_empty());

    let mut misheard = build(&["TOTALLY"], "TOTALLY FINE", Some(Some(3)));
    misheard.speak(20, 40);
    assert!(events(&captured()).is_empty());

    let mut follow = build(&["AND TOMORROW"], "AND TOMORROW", Some(None));
    follow.voice.extend();
    follow.speak(20, 40);
    assert_eq!(follow.spotter.as_ref().unwrap().0.borrow().fed, 0);
    assert_eq!(names(&captured()), ["hearing", "hearing", "turn"]);
}

#[test]
fn her_name_in_a_partial_starts_the_high_profile_loading_before_the_turn_ends() {
    let mut named = rig(&["", "TOHRU WHAT", "TOHRU WHAT TIME"], "WHAT TIME IS IT");
    let (profiles, watch) = Profiles::watched();
    named.voice.profiles = profiles;
    named.speak(12, 40);
    assert_eq!(
        watch.wakes(),
        1,
        "once per utterance, though the final lost her name"
    );

    let mut other = rig(&["WHAT", "WHAT TIME"], "WHAT TIME IS IT");
    let (profiles, watch) = Profiles::watched();
    other.voice.profiles = profiles;
    other.speak(12, 40);
    assert_eq!(watch.wakes(), 0);

    let mut summoned = rig(&["TOHRU"], "TOHRU");
    let (profiles, watch) = Profiles::watched();
    summoned.voice.profiles = profiles;
    summoned.speak(12, 40);
    assert_eq!(
        watch.wakes(),
        2,
        "the partial, then the conversation it opens"
    );
}

#[test]
fn the_name_alone_waits_and_summons_her_again_in_a_conversation() {
    let mut alone = rig(&["TOHRU"], "TOHRU");
    alone.speak(12, 40);
    assert_eq!(names(&captured()), ["conversation", "wake"]);

    let mut again = rig(&["TOHRU"], "TOHRU.");
    again.voice.extend();
    again.speak(12, 40);
    let all = captured();
    assert_eq!(names(&all), ["hearing", "hearing", "conversation", "wake"]);
    assert_eq!(events(&all)[2]["on"], true);
    assert!(again.voice.active());
}

#[test]
fn the_name_alone_while_thinking_shows_her_without_dropping_the_answer() {
    let mut rig = rig(&[""], "");
    rig.voice.extend();
    rig.voice.thinking = true;
    rig.voice.last_turn = "What time is it".into();
    rig.voice.deliver("TOHRU", "");
    assert_eq!(
        events(&captured()),
        [json!({"event":"conversation","on":true})]
    );
    assert!(rig.voice.thinking);
    assert_eq!(rig.voice.last_turn, "What time is it");
}

#[test]
fn answers_are_spoken_plainly_and_not_while_paused() {
    let mut rig = rig(&[""], "");
    rig.voice.command(json!({"cmd":"say","text":"**Eight** ounces. See [the chart](https://example.com).\n```\ncode\n```","conversation":"c1"})).unwrap();
    assert_eq!(
        rig.spoken().said,
        [(
            json!("c1"),
            "Eight ounces. See the chart. (code in the popup)".to_owned()
        )]
    );
    rig.voice.paused = true;
    rig.voice
        .command(json!({"cmd":"say","text":"Hello","conversation":"c1"}))
        .unwrap();
    assert_eq!(rig.spoken().said.len(), 1);
    let long = "A sentence here. ".repeat(80);
    assert_eq!(policy::speakable(&long), long.trim());
}

#[test]
fn tts_stays_loaded_by_default_or_follows_the_conversation() {
    let mut rig = rig(&["TOHRU"], "TOHRU");
    rig.voice.conversation(true);
    rig.voice.conversation(false);
    assert_eq!((rig.spoken().preloads, rig.spoken().unloads), (0, 0));
    rig.voice.load_on_conversation = true;
    rig.voice.conversation(true);
    rig.voice.conversation(false);
    assert_eq!((rig.spoken().preloads, rig.spoken().unloads), (1, 1));
}

#[test]
fn the_dragon_does_not_answer_its_own_echo() {
    let mut rig = rig(&["WAIT STOP"], "WAIT STOP");
    rig.vad.set(0.9);
    {
        let mut spoken = rig.speaker.0.borrow_mut();
        spoken.speaking = true;
        spoken.echo = 0.2;
    }
    for _ in 0..40 {
        rig.step(0.25);
    }
    assert_eq!((rig.spoken().pauses, rig.spoken().hushed), (0, 0));
    for _ in 0..10 {
        rig.step(0.5);
    }
    assert_eq!((rig.spoken().pauses, rig.spoken().hushed), (1, 1));
    assert_eq!(names(&captured()), ["hearing", "barge"]);
    rig.speaker.0.borrow_mut().speaking = false;
    rig.speaker.0.borrow_mut().quiet = true;
    rig.voice.reset();
    for _ in 0..10 {
        rig.step(0.5);
    }
    assert!(!rig.voice.in_speech);
}

#[test]
fn aec_uses_a_flat_barge_threshold() {
    let mut rig = rig(&["WAIT"], "WAIT");
    rig.vad.set(0.9);
    rig.voice.aec = true;
    rig.speaker.0.borrow_mut().speaking = true;
    rig.speaker.0.borrow_mut().echo = 0.35;
    for _ in 0..20 {
        rig.step(0.05);
    }
    assert_eq!(rig.spoken().pauses, 0);
    for _ in 0..7 {
        rig.step(0.08);
    }
    assert_eq!(rig.spoken().pauses, 1);
    assert!(rig.voice.barge_pending);
}

#[test]
fn echo_coupling_adapts_without_aec() {
    let mut rig = rig(&["WAIT"], "WAIT");
    rig.vad.set(0.9);
    rig.speaker.0.borrow_mut().speaking = true;
    rig.speaker.0.borrow_mut().echo = 0.25;
    for _ in 0..40 {
        rig.step(0.04);
    }
    assert_eq!(rig.spoken().pauses, 0);
    assert!((0.05..0.25).contains(&rig.voice.echo_coupling));
    for _ in 0..7 {
        rig.step(0.12);
    }
    assert_eq!(rig.spoken().pauses, 1);
}

/// Fills about a second of pre-roll, then crosses the barge window.
fn barge(rig: &mut Rig) {
    rig.voice.aec = true;
    rig.speaker.0.borrow_mut().speaking = true;
    rig.vad.set(0.);
    for _ in 0..30 {
        rig.step(0.1);
    }
    rig.vad.set(0.9);
    for _ in 0..7 {
        rig.step(0.1);
    }
}

#[test]
fn pause_then_resume_on_noise_and_backchannels() {
    for word in ["", "MM", "MHM", "UH HUH", "YEAH", "RIGHT", "OKAY"] {
        let mut rig = rig(&[word], word);
        barge(&mut rig);
        assert_eq!((rig.spoken().pauses, rig.spoken().hushed), (1, 0), "{word}");
        assert!(rig.asr.fed() >= 30);
        for _ in 0..35 {
            if !rig.voice.barge_pending {
                break;
            }
            rig.step(0.1);
        }
        assert!(!rig.voice.barge_pending, "{word}");
        assert_eq!(
            (rig.spoken().resumes, rig.spoken().hushed),
            (1, 0),
            "{word}"
        );
        let names = names(&captured());
        for absent in ["barge", "turn", "hearing"] {
            assert!(!names.iter().any(|n| n == absent), "{word}: {names:?}");
        }
    }
}

#[test]
fn pause_then_barge_on_real_words() {
    let mut rig = rig(
        &["", "WAIT WHAT", "WAIT WHAT ABOUT TOMORROW"],
        "WAIT WHAT ABOUT TOMORROW",
    );
    barge(&mut rig);
    assert_eq!((rig.spoken().pauses, rig.spoken().hushed), (1, 0));
    assert!(rig.asr.fed() >= 30);
    rig.step(0.1);
    assert_eq!(rig.spoken().hushed, 0);
    rig.step(0.1);
    assert_eq!(rig.spoken().hushed, 1);
    assert!(!rig.voice.barge_pending);
    rig.vad.set(0.);
    for _ in 0..35 {
        if !rig.voice.in_speech {
            break;
        }
        rig.step(0.);
    }
    let all = captured();
    assert_eq!(
        names(&all),
        ["hearing", "barge", "draft", "hearing", "turn"]
    );
    assert_eq!(
        events(&all).last().unwrap()["text"],
        "Wait what about tomorrow"
    );
}

#[test]
fn an_interruption_while_thinking_cancels_the_stale_answer() {
    let mut rig = rig(&["", "ACTUALLY NEVER MIND"], "ACTUALLY NEVER MIND");
    rig.voice.extend();
    rig.voice.thinking = true;
    rig.vad.set(0.9);
    for _ in 0..5 {
        rig.step(0.01);
    }
    assert!(
        !rig.voice.in_speech,
        "below-floor noise while thinking does not start a turn"
    );
    for _ in 0..4 {
        rig.step(0.05);
    }
    assert!(names(&captured()).contains(&"barge".to_owned()));
    rig.voice
        .command(json!({"cmd":"say","text":"Old answer","conversation":"c1"}))
        .unwrap();
    assert!(rig.spoken().said.is_empty());
    rig.vad.set(0.);
    for _ in 0..35 {
        if !rig.voice.in_speech {
            break;
        }
        rig.step(0.);
    }
    let all = captured();
    assert_eq!(names(&all), ["hearing", "turn"]);
    assert_eq!(events(&all).last().unwrap()["text"], "Actually never mind");
    rig.voice
        .command(json!({"cmd":"say","text":"New answer","conversation":"c1"}))
        .unwrap();
    assert_eq!(rig.spoken().said, [(json!("c1"), "New answer".to_owned())]);
}

#[test]
fn a_continuation_merges_fragments_into_one_turn() {
    let mut rig = rig(&["TELL ME ABOUT THE HISTORY"], "TELL ME ABOUT THE HISTORY");
    rig.voice.extend();
    rig.speak(10, 25);
    assert_eq!(
        events(&captured()).last().unwrap(),
        &json!({"event":"turn","text":"Tell me about the history"})
    );
    assert!(rig.voice.thinking);
    rig.swap(&["OF THE ROMAN EMPIRE"], "OF THE ROMAN EMPIRE");
    rig.speak(10, 25);
    let all = events(&captured());
    assert!(
        all.iter()
            .any(|e| e["event"] == "barge" && e["continuation"] == true)
    );
    assert_eq!(
        all.last().unwrap(),
        &json!({"event":"turn","text":"Tell me about the history of the roman empire"})
    );
}

#[test]
fn a_turn_settles_after_quiet_but_a_continued_fragment_never_does() {
    let windows = (SETTLE_MS / 32) as usize;
    let settled = |all: &[Value]| all.iter().filter(|e| e["event"] == "settled").count();
    let mut rig = rig(&["WHAT TIME IS IT"], "WHAT TIME IS IT");
    rig.voice.extend();
    rig.speak(10, 25);
    captured();
    for _ in 0..windows - 1 {
        rig.voice.settle_window();
    }
    assert_eq!(settled(&captured()), 0);
    rig.voice.settle_window();
    for _ in 0..windows * 3 {
        rig.voice.settle_window();
    }
    assert_eq!(settled(&captured()), 1);

    let mut rig = self::rig(&["TELL ME ABOUT THE HISTORY"], "TELL ME ABOUT THE HISTORY");
    rig.voice.extend();
    rig.speak(10, 25);
    for _ in 0..windows / 2 {
        rig.voice.settle_window();
    }
    rig.swap(&["OF THE ROMAN EMPIRE"], "OF THE ROMAN EMPIRE");
    rig.voice.begin();
    for _ in 0..windows * 2 {
        rig.voice.settle_window();
        rig.hear(true);
    }
    let mut all = captured();
    assert_eq!(settled(&all), 0);
    for _ in 0..25 {
        if !rig.voice.in_speech {
            break;
        }
        rig.hear(false);
    }
    all.extend(captured());
    let all = events(&all);
    assert!(all.iter().any(|e| e["continuation"] == true));
    assert_eq!(
        all.last().unwrap(),
        &json!({"event":"turn","text":"Tell me about the history of the roman empire"})
    );
    for _ in 0..windows {
        rig.voice.settle_window();
    }
    assert_eq!(settled(&captured()), 1, "only the joined turn settles");
}

#[test]
fn the_settle_window_is_bounded_while_noise_never_yields_words() {
    let mut rig = rig(&["CHECK THE BUILD"], "CHECK THE BUILD");
    rig.voice.extend();
    rig.speak(10, 25);
    captured();
    rig.voice.in_speech = true;
    for _ in 0..(SETTLE_CAP_MS / 32) as usize - 1 {
        rig.voice.settle_window();
    }
    assert!(captured().iter().all(|e| e["event"] != "settled"));
    rig.voice.settle_window();
    assert_eq!(events(&captured()), [json!({"event":"settled"})]);
}

fn open(phrase: &str, final_text: &str) -> Rig {
    let mut rig = rig(&[phrase], final_text);
    rig.voice.extend();
    rig.voice.begin();
    rig.hear(true);
    rig
}

#[test]
fn endpointing_holds_unfinished_phrases_and_shortens_with_a_draft() {
    let mut unfinished = open("WHAT TIME IS", "WHAT TIME IS");
    for _ in 0..25 {
        unfinished.hear(false);
    }
    assert!(
        unfinished.voice.in_speech,
        "an unfinished phrase is held past 800 ms"
    );
    for _ in 0..10 {
        if !unfinished.voice.in_speech {
            break;
        }
        unfinished.hear(false);
    }
    assert!(!unfinished.voice.in_speech);
    assert_eq!(events(&captured()).last().unwrap()["text"], "What time is");
    for phrase in ["SO", "SO UM", "SO UM WHAT I WANT IS", "TELL ME ABOUT,"] {
        assert!(policy::unfinished(phrase), "{phrase}");
    }
    assert!(!policy::unfinished("WHAT TIME IS IT"));

    let mut short = open("WHAT TIME NOW", "WHAT TIME NOW");
    for _ in 0..15 {
        short.hear(false);
    }
    assert!(short.voice.in_speech);
    for _ in 0..7 {
        if !short.voice.in_speech {
            break;
        }
        short.hear(false);
    }
    assert!(!short.voice.in_speech);
    assert_eq!(events(&captured()).last().unwrap()["text"], "What time now");

    let mut drafted = open("WHAT IS THE WEATHER TODAY", "WHAT IS THE WEATHER TODAY");
    for _ in 0..7 {
        drafted.hear(false);
    }
    let all = captured();
    assert_eq!(names(&all), ["hearing", "draft"]);
    assert_eq!(events(&all)[1]["text"], "What is the weather today");
    for _ in 0..8 {
        if !drafted.voice.in_speech {
            break;
        }
        drafted.hear(false);
    }
    assert!(!drafted.voice.in_speech);
    assert_eq!(names(&captured()), ["hearing", "turn"]);
}

#[test]
fn drafts_discard_on_resumed_speech_and_changed_text_and_cap_at_two() {
    let mut rig = open("WHAT IS THE WEATHER TODAY", "WHAT IS THE WEATHER TODAY");
    for attempt in 0..3 {
        if attempt > 0 {
            rig.hear(true);
        }
        for _ in 0..8 {
            rig.hear(false);
        }
        if attempt < 2 {
            rig.hear(true);
        }
    }
    let all = names(&captured());
    assert_eq!(all.iter().filter(|n| *n == "draft").count(), 2);
    assert_eq!(all.iter().filter(|n| *n == "draft_cancel").count(), 2);

    let mut changed = open("WHAT IS THE WEATHER TODAY", "WHAT IS THE WEATHER TOMORROW");
    for _ in 0..16 {
        if !changed.voice.in_speech {
            break;
        }
        changed.hear(false);
    }
    assert_eq!(
        names(&captured()),
        ["hearing", "draft", "hearing", "draft_cancel", "turn"]
    );

    for phrase in ["HI THERE NOW", "WHAT IS THE WEATHER IN"] {
        let mut rig = open(phrase, phrase);
        for _ in 0..10 {
            rig.hear(false);
        }
        assert!(
            !names(&captured()).contains(&"draft".to_owned()),
            "{phrase}"
        );
    }
}

#[test]
fn a_confirmed_barge_carries_what_was_spoken_so_far() {
    let mut rig = rig(&["WAIT WHAT"], "WAIT WHAT");
    rig.voice.aec = true;
    rig.speaker.0.borrow_mut().speaking = true;
    rig.speaker.0.borrow_mut().spoken = "Paris is the capital. It is known for…".into();
    rig.vad.set(0.9);
    for _ in 0..8 {
        rig.step(0.1);
    }
    let all = events(&captured());
    let barge = all.iter().find(|e| e["event"] == "barge").unwrap();
    assert_eq!(barge["spoken"], "Paris is the capital. It is known for…");
}

#[test]
fn a_backchannel_while_thinking_does_not_cancel() {
    let mut rig = rig(&["OKAY"], "OKAY");
    rig.voice.extend();
    rig.voice.thinking = true;
    rig.speak(10, 35);
    assert!(rig.voice.thinking);
    let names = names(&captured());
    assert!(!names.contains(&"barge".to_owned()) && !names.contains(&"turn".to_owned()));
}

#[test]
fn speech_outside_a_conversation_is_not_shown_as_heard() {
    let mut rig = rig(&["WHAT"], "WHAT");
    rig.speak(20, 40);
    assert!(!names(&captured()).contains(&"hearing".to_owned()));
}

#[test]
fn a_transcript_shows_only_after_the_wake_word_or_in_a_conversation() {
    let shown = |all: &[Value]| {
        all.iter()
            .any(|e| e["event"] == "transcript" && e["text"] != "")
    };
    let mut private = rig(&["PRIVATE WORDS"], "PRIVATE WORDS");
    private.voice.begin();
    private.hear(true);
    assert!(!shown(&captured()));
    let mut named = rig(&["TOHRU WHAT TIME"], "TOHRU WHAT TIME");
    named.voice.begin();
    named.hear(true);
    assert!(
        captured()
            .iter()
            .any(|e| e["event"] == "transcript" && e["text"] == "What time")
    );
}

#[test]
fn stream_commands_reject_a_stale_generation() {
    let mut rig = rig(&[""], "");
    rig.voice
        .command(json!({"cmd":"speech_start","generation":1,"conversation":"a"}))
        .unwrap();
    rig.voice.command(json!({"cmd":"hush"})).unwrap();
    rig.voice
        .command(json!({"cmd":"speech_delta","generation":1,"text":"Stale."}))
        .unwrap();
    rig.voice
        .command(json!({"cmd":"speech_start","generation":2,"conversation":"b"}))
        .unwrap();
    rig.voice
        .command(json!({"cmd":"speech_end","generation":1,"text":"Wrong conversation."}))
        .unwrap();
    rig.voice
        .command(json!({"cmd":"speech_end","generation":2,"text":"Current."}))
        .unwrap();
    assert_eq!(
        rig.spoken().said,
        [
            (json!("a"), String::new()),
            (json!("b"), "Current.".to_owned())
        ]
    );
}

struct Listening {
    transcribes: bool,
    reply: fn() -> Result<Option<String>, HighError>,
    heard: std::sync::Mutex<Vec<usize>>,
}

impl crate::profile::HighVoice for Listening {
    fn transcribes(&self) -> bool {
        self.transcribes
    }

    fn engine(&self) -> &str {
        "listening"
    }

    fn speak(&self, _: &str, _: &dyn Fn() -> bool) -> Result<Option<(Vec<f32>, u32)>, HighError> {
        Ok(None)
    }

    fn transcribe(&self, audio: &[f32], _: Instant) -> Result<Option<String>, HighError> {
        self.heard.lock().unwrap().push(audio.len());
        (self.reply)()
    }
}

#[test]
fn the_final_pass_replaces_the_streaming_transcript_only_when_it_works() {
    let heard = |transcribes, reply: fn() -> Result<Option<String>, HighError>| {
        let mut rig = rig(&[""], "");
        let high = Arc::new(Listening {
            transcribes,
            reply,
            heard: std::sync::Mutex::new(Vec::new()),
        });
        rig.voice.profiles = Profiles::loaded(high.clone());
        rig.voice.audio = vec![0.; WINDOW];
        let text = rig.voice.high_transcript("turn on the lice".into());
        let calls = high.heard.lock().unwrap().clone();
        (text, calls, rig.voice.profiles.high().is_some())
    };
    assert_eq!(
        heard(true, || Ok(Some("turn on the lights".into()))),
        ("turn on the lights".into(), vec![WINDOW], true)
    );
    assert_eq!(
        heard(true, || Ok(Some(String::new()))).0,
        "turn on the lice"
    );
    assert_eq!(
        heard(true, || Ok(None)).0,
        "turn on the lice",
        "a busy worker keeps the streaming text"
    );
    assert_eq!(
        heard(true, || Err(HighError::Reply("odd".into()))),
        ("turn on the lice".into(), vec![WINDOW], true)
    );
    assert_eq!(
        heard(true, || Err(HighError::Broken("exited".into()))),
        ("turn on the lice".into(), vec![WINDOW], false),
        "a broken worker is dropped at once"
    );
    assert_eq!(
        heard(false, || Ok(Some("unused".into()))),
        ("turn on the lice".into(), vec![], true)
    );
    assert_eq!(
        rig(&[""], "").voice.high_transcript("streaming".into()),
        "streaming"
    );
}

fn semantic(text: &str) -> Rig {
    let mut rig = rig(&[text], text);
    rig.voice.semantic = true;
    rig.voice.extend();
    rig.voice.begin();
    rig.voice.maybe_draft(text);
    rig
}

impl Rig {
    fn decide(&mut self, decision: &str, text: &str, token: Option<String>) {
        let token = token.or_else(|| self.voice.semantic_token.clone());
        self.voice
            .command(json!({"cmd":"semantic","decision":decision,"text":text,"token":token}))
            .unwrap();
    }

    fn quiet(&mut self, windows: usize) {
        for _ in 0..windows {
            if self.voice.in_speech {
                self.hear(false);
            }
        }
    }

    fn revise(&self, final_text: &str) {
        self.asr.0.borrow_mut().final_text = final_text.to_owned();
    }
}

fn count(all: &[Value], name: &str) -> usize {
    names(all).iter().filter(|n| *n == name).count()
}

#[test]
fn resumed_speech_invalidates_a_semantic_decision() {
    let mut rig = semantic("Maybe a map would work");
    let token = rig.voice.semantic_token.clone();
    rig.hear(true);
    rig.decide("respond", "Maybe a map would work", token);
    assert_eq!(rig.voice.semantic_decision, None);
}

#[test]
fn a_semantic_decision_needs_the_drafted_text_and_its_token() {
    let mut rig = semantic("Maybe a map would work");
    rig.decide("respond", "Something else", None);
    assert_eq!(rig.voice.semantic_decision, None);
    rig.decide("respond", "Maybe a map would work", Some("old".into()));
    assert_eq!(rig.voice.semantic_decision, None);
}

#[test]
fn a_revised_final_falls_back_to_the_ordinary_turn() {
    let mut rig = semantic("Maybe a map would work");
    rig.decide("respond", "Maybe a map would work", None);
    rig.revise("Explain the second index instead");
    rig.voice.end_turn();
    let all = captured();
    assert_eq!((count(&all, "turn"), count(&all, "draft_cancel")), (1, 1));
}

#[test]
fn an_explicit_handoff_after_a_wait_takes_the_floor() {
    let mut rig = semantic("Hold on let me think");
    rig.decide("wait", "Hold on let me think", None);
    rig.quiet(190);
    assert!(!rig.voice.in_speech);
    rig.swap(&["Go ahead"], "Go ahead");
    rig.voice.begin();
    rig.quiet(24);
    assert_eq!(count(&captured(), "turn"), 1);
}

#[test]
fn ending_the_conversation_invalidates_the_capture() {
    let mut rig = semantic("Maybe a map would work");
    let token = rig.voice.semantic_token.clone();
    rig.voice.command(json!({"cmd":"end"})).unwrap();
    rig.decide("respond", "Maybe a map would work", token);
    assert_eq!(rig.voice.semantic_decision, None);
}

#[test]
fn a_confirmed_wait_survives_a_handoff_phrase_and_the_deadline() {
    let mut rig = semantic("Go ahead");
    rig.decide("wait", "Go ahead", None);
    rig.quiet(200);
    assert_eq!(count(&captured(), "turn"), 0);
}

#[test]
fn a_failed_semantic_policy_falls_back_once_without_retrying() {
    let mut rig = semantic("Explain this index");
    rig.decide("error", "Explain this index", None);
    rig.quiet(24);
    let all = captured();
    assert_eq!((count(&all, "turn"), count(&all, "draft")), (1, 1));
}

#[test]
fn an_explicit_hold_in_a_revised_final_is_kept() {
    let mut rig = semantic("Explain this index");
    rig.decide("respond", "Explain this index", None);
    rig.revise("Actually hold on");
    rig.voice.end_turn();
    assert_eq!(count(&captured(), "turn"), 0);
}

fn acks(all: &[Value]) -> Vec<Value> {
    all.iter()
        .filter(|e| e["event"] == "ack_draft" || e["event"] == "ack_cancel" || e["event"] == "turn")
        .cloned()
        .collect()
}

#[test]
fn an_acknowledgement_is_drafted_at_the_pause_and_claimed_by_the_matching_turn() {
    let mut rig = open("CHECK THE BUILD", "CHECK THE BUILD");
    for _ in 0..30 {
        if !rig.voice.in_speech {
            break;
        }
        rig.hear(false);
    }
    let all = acks(&captured());
    assert_eq!(all.len(), 2);
    assert_eq!(
        (&all[0]["event"], &all[0]["text"]),
        (&json!("ack_draft"), &json!("Check the build"))
    );
    assert_eq!(all[1]["event"], "turn");
    assert_eq!(all[1]["ack"], all[0]["token"]);
}

#[test]
fn a_changed_request_cancels_its_acknowledgement() {
    let mut changed = open("CHECK THE BUILD", "CHECK THE BUILD AND THE TESTS");
    for _ in 0..30 {
        if !changed.voice.in_speech {
            break;
        }
        changed.hear(false);
    }
    let all = acks(&captured());
    let names: Vec<_> = all.iter().map(|e| e["event"].as_str().unwrap()).collect();
    assert_eq!(names, ["ack_draft", "ack_cancel", "turn"]);
    assert_eq!(all[1]["token"], all[0]["token"]);
    assert!(all[2].get("ack").is_none());
}

#[test]
fn no_acknowledgement_is_drafted_for_backchannels_or_unfinished_phrases() {
    for phrase in ["MM OKAY", "CAN YOU PUT IT ON THE"] {
        let mut rig = open(phrase, phrase);
        for _ in 0..12 {
            rig.hear(false);
        }
        assert!(
            acks(&captured()).iter().all(|e| e["event"] != "ack_draft"),
            "{phrase}"
        );
    }
}
