use super::*;

impl Voice {
    pub fn command(&mut self, message: Value) -> Result<()> {
        let text = message["text"].as_str().unwrap_or("");
        match message["cmd"].as_str().unwrap_or("") {
            "speech_start" if !self.paused => {
                self.generation = message["generation"].clone();
                self.speech_job = Some(self.speaker.start(
                    message["conversation"].clone(),
                    message["generation"].clone(),
                ));
                self.thinking = true;
                self.cancelled = false;
                self.first_text = true;
                diagnostic(
                    "speech_stream_start",
                    json!({"generation":self.generation,"turn_id":message["turn_id"]}),
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
                let stage = message["stage"].as_str().unwrap_or("");
                if [
                    "ui_visibility_request",
                    "ui_state_observed",
                    "assistant_request",
                    "continuation_speech",
                    "continuation_dropped",
                ]
                .contains(&stage)
                {
                    let mut fields = serde_json::Map::new();
                    for key in [
                        "turn_id",
                        "generation",
                        "visible",
                        "ui_sequence",
                        "flight_alpha",
                        "request",
                        "completion",
                        "reason",
                    ] {
                        if !message[key].is_null() {
                            fields.insert(key.into(), message[key].clone());
                        }
                    }
                    diagnostic(stage, Value::Object(fields));
                }
            }
            _ => {}
        }
        Ok(())
    }
}
