use super::*;

impl Voice {
    pub fn command(&mut self, message: Value) -> Result<()> {
        let text = message["text"].as_str().unwrap_or("");
        match message["cmd"].as_str().unwrap_or("") {
            "task_accepted" => {
                self.thinking = false;
                self.last_turn.clear();
                self.pending.clear();
                self.cancelled = false;
                diagnostic(
                    "foreground_released",
                    json!({"generation":message["generation"],"turn_id":message["turn_id"],"request":message["request"].as_str().unwrap_or("").chars().take(512).collect::<String>()}),
                );
            }
            "speech_start" if !self.paused => {
                self.generation = message["generation"].clone();
                self.speech_job = Some(self.speaker.start(
                    message["conversation"].clone(),
                    message["generation"].clone(),
                ));
                let background = message["background"] == true;
                self.thinking = !background;
                if background {
                    self.last_turn.clear();
                    self.pending.clear();
                }
                self.cancelled = false;
                self.first_text = true;
                diagnostic(
                    "speech_stream_start",
                    json!({"generation":self.generation,"turn_id":message["turn_id"],"background":background,"request":message["request"].as_str().unwrap_or("").chars().take(512).collect::<String>()}),
                );
            }
            "speech_delta" | "speech_end" => {
                if !self.paused
                    && message["generation"] == self.generation
                    && let Some(job) = self.speech_job
                {
                    let final_part = message["cmd"] == "speech_end";
                    self.speaker.feed(job, text, final_part);
                    if final_part || self.first_text {
                        diagnostic(
                            if final_part {
                                "provider_done"
                            } else {
                                "first_provider_text"
                            },
                            json!({"generation":self.generation}),
                        );
                        self.first_text = false;
                    }
                }
            }
            "say" => {
                if self.cancelled {
                    self.cancelled = false;
                    return Ok(());
                }
                self.generation = message["generation"].clone();
                diagnostic(
                    "committed_answer_ready",
                    json!({"generation":self.generation,"turn_id":message["turn_id"]}),
                );
                let text = if text.chars().count() <= policy::SPOKEN_CHARS {
                    policy::speakable(text)
                } else {
                    "That answer is long. Please read the details in the chat.".into()
                };
                self.thinking = false;
                self.last_turn.clear();
                self.pending.clear();
                if !text.is_empty() && !self.paused {
                    if self.in_speech && !self.barge_pending {
                        self.discard();
                        self.reset();
                    }
                    let id = self.speaker.start(
                        message["conversation"].clone(),
                        message["generation"].clone(),
                    );
                    self.speaker.feed(id, &text, true);
                    self.speech_job = Some(id);
                } else if self.until.is_some() {
                    self.extend();
                }
            }
            "hush" => {
                self.generation = Value::Null;
                self.speech_job = None;
                self.barge_pending = false;
                self.barge_decide_ms = 0;
                self.speaker.hush();
            }
            "thinking" => {
                if self.cancelled && message["on"] != true {
                    self.cancelled = false;
                    return Ok(());
                }
                self.thinking = message["on"] == true;
                if !self.thinking {
                    self.last_turn.clear();
                    if self.until.is_some() {
                        self.extend();
                    }
                }
            }
            "draft" if message["ok"] != true => {
                self.draft.clear();
            }
            "semantic"
                if self.semantic
                    && self.in_speech
                    && self.semantic_token.as_deref() == message["token"].as_str()
                    && self.semantic_token.is_some()
                    && policy::normalize(text) == self.draft =>
            {
                let decision = message["decision"].as_str().unwrap_or("error");
                let decision = if ["wait", "respond", "unsupported"].contains(&decision) {
                    decision
                } else {
                    "error"
                };
                self.semantic_decision = Some(decision.into());
                self.semantic_text = policy::normalize(text);
                diagnostic("semantic_decision", json!({"decision":decision}));
                if decision == "unsupported" && !self.semantic_warned {
                    self.semantic_warned = true;
                    emit(
                        json!({"event":"error","message":"Semantic turn detection is unavailable on this backend; using ordinary turn detection."}),
                    );
                }
            }
            "presence" => self.profiles.presence(message["visible"] == true),
            "tts" => {
                let settings: Settings = serde_json::from_value(message)?;
                self.load_on_conversation = settings.load_on_conversation;
                self.speaker.configure(settings);
            }
            "end" => {
                self.semantic_warned = false;
                self.generation = Value::Null;
                self.speech_job = None;
                self.speaker.hush();
                self.discard();
                self.reset();
                self.cancelled = false;
                self.last_turn.clear();
                self.pending.clear();
                self.until = None;
                self.conversation(false);
            }
            "diagnostic" => {
                if let Some((stage, fields)) = crate::protocol::diagnostic_message(&message) {
                    diagnostic(stage, fields);
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    #[ignore = "requires the pinned shared voice artifacts and an explicit scheduler reservation"]
    fn background_admission_releases_the_foreground_before_synthesis() {
        assert!(
            std::env::var_os("GPU_LEASE_ID").is_some(),
            "run this model-backed test inside gpu-run"
        );
        crate::acoustic::initialize().unwrap();
        let root =
            std::path::PathBuf::from(std::env::var_os("MODEL_ROOT").expect("MODEL_ROOT required"))
                .join("by-name");
        let args = crate::Args::parse_from([
            "native-voice",
            "--vad-model",
            root.join("silero-vad/silero-vad-v6.2.3.onnx")
                .to_str()
                .unwrap(),
            "--asr-model",
            root.join("voice-asr").to_str().unwrap(),
            "--tts-engine",
            "espeak-ng",
            "--tts-load-on-conversation",
        ]);
        let mut check = args.check_models();
        let acoustics = Acoustics::load(&args, LowPrecision::Int8, &mut check).unwrap();
        let (speaker, worker) =
            Control::spawn(args.settings(), args.assets.clone(), Profiles::default());
        let mut voice = Voice::new(
            acoustics,
            Box::new(speaker.clone()),
            Profiles::default(),
            true,
        );
        voice.extend();
        voice.thinking = true;
        voice.last_turn = "Create a worktree".into();
        voice.pending = "old cumulative text".into();
        voice.command(json!({"cmd":"speech_start","generation":2,"background":true,"request":"orca:canonical-id"})).unwrap();
        assert!(!voice.thinking);
        assert!(voice.last_turn.is_empty());
        assert!(voice.pending.is_empty());
        voice.deliver("ARE YOU STILL THERE", "");
        assert_eq!(voice.last_turn, "Are you still there");
        assert!(!voice.cancelled);
        assert!(voice.pending.is_empty());
        voice.thinking = true;
        voice.pending = "admitted work continuation".into();
        voice.begin();
        voice.audio.extend_from_slice(&[0.1, 0.2]);
        let capture = voice.turn_id.clone();
        voice
            .command(json!({"cmd":"task_accepted","request":"orca:hidden","generation":2}))
            .unwrap();
        assert!(!voice.thinking && voice.pending.is_empty() && voice.last_turn.is_empty());
        assert!(voice.in_speech);
        assert_eq!(voice.audio, vec![0.1, 0.2]);
        assert_eq!(voice.turn_id, capture);
        voice.pending = "genuine continuation".into();
        voice
            .command(json!({"cmd":"speech_start","generation":3}))
            .unwrap();
        assert!(voice.thinking);
        assert_eq!(voice.pending, "genuine continuation");
        speaker.shutdown();
        worker.join().unwrap();
    }
}
