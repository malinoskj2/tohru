use super::*;

impl Voice {
    pub fn step(&mut self, frame: &[f32]) -> Result<()> {
        self.playback_changed();
        let probability = self.vad.probability(frame)?;
        let rms = (frame.iter().map(|s| s * s).sum::<f32>() / WINDOW as f32).sqrt();
        self.last_vad = (probability, rms);
        if self.speaker.speaking() && !self.barge_pending {
            if probability >= 0.5 {
                self.suppression("playback", probability, rms);
            }
            let playback = self.speaker.echo_level();
            let floor = if self.aec {
                0.065
            } else {
                0.025_f32.max(1.6 * self.echo_coupling * playback)
            };
            if probability >= 0.5 && rms > floor {
                self.barge_ms += 32;
            } else {
                if probability < 0.35 || rms < floor / 2. {
                    self.barge_ms = 0;
                }
                if !self.aec && self.barge_ms == 0 && playback >= 0.02 && rms <= floor {
                    self.echo_coupling +=
                        0.12 * ((rms / playback).clamp(0.05, 2.) - self.echo_coupling);
                }
            }
            self.remember_frame(frame, 31);
            if self.barge_ms >= 200 {
                diagnostic("interruption_detected", json!({}));
                self.barge_ms = 0;
                self.barge_pending = true;
                self.barge_decide_ms = 0;
                self.speaker.pause();
                self.extend();
                self.begin();
            }
            return Ok(());
        }
        if self.speaker.deaf() && !self.in_speech {
            if probability >= 0.5 {
                self.suppression("echo_cooldown", probability, rms);
            }
            self.start_run = 0;
            return Ok(());
        }
        if !self.in_speech {
            self.remember_frame(frame, 4);
            let above = probability >= 0.5 && (!self.thinking || rms > 0.025);
            if !above && rms >= 0.025 {
                self.suppression(
                    if probability < 0.5 {
                        "vad_below_start"
                    } else {
                        "thinking_floor"
                    },
                    probability,
                    rms,
                );
            }
            self.start_run = if above { self.start_run + 1 } else { 0 };
            if self.start_run >= 2 {
                self.begin();
            }
            if self.until.is_some() && !self.active() && !self.thinking {
                self.until = None;
                self.conversation(false);
            }
            return Ok(());
        }
        self.hear(frame, probability >= 0.35)
    }

    pub(super) fn hear(&mut self, frame: &[f32], speech: bool) -> Result<()> {
        self.audio.extend_from_slice(frame);
        self.speech_ms += 32;
        if speech {
            self.last_speech_ns = Some(crate::protocol::elapsed_ns());
            self.discard();
            self.quiet_ms = 0;
            self.turn_checked = false;
            self.turn_complete = None;
        } else {
            self.quiet_ms += 32;
        }
        if self.spotting {
            if (speech || self.quiet_ms <= 160)
                && self.spotter.as_mut().is_some_and(|s| s.feed(frame))
            {
                self.spotted();
            }
        } else if speech || self.quiet_ms <= 160 {
            self.asr.feed(frame);
        }
        let partial = if self.spotting {
            String::new()
        } else {
            self.asr.partial()
        };
        // VAD onset alone is not the owner taking the floor: a blip that never
        // yields words must not silence speech. Barge-in waits for the same
        // non-backchannel words before it confirms, so the speaker's own pause
        // covers the gap and a backchannel can still resume.
        if self.hearing_eligible
            && !self.hearing
            && !partial.trim().is_empty()
            && !(self.barge_pending && policy::backchannel(&partial))
        {
            self.hearing = true;
            emit(json!({"event":"hearing","on":true}));
        }
        let visible = if self.spotting {
            String::new()
        } else {
            self.candidate(&partial)
        };
        if visible != self.display {
            self.display = visible.clone();
            emit(
                json!({"event":"transcript","text":visible.chars().take(240).collect::<String>(),"final":false}),
            );
        }
        if self.barge_pending {
            self.barge_decide_ms += 32;
            if !policy::backchannel(&partial) {
                self.confirm_barge();
            } else if self.barge_decide_ms >= 1000 {
                self.end_turn();
                return Ok(());
            }
        } else if self.thinking && self.active() && !self.spotting && !policy::backchannel(&partial)
        {
            self.cancel_thinking();
        }
        if !self.draft.is_empty() && policy::normalize(&self.candidate(&partial)) != self.draft {
            self.discard();
        }
        if self.quiet_ms >= 220 && !self.turn_checked && !self.spotting {
            self.turn_checked = true;
            if let Some(turn) = &mut self.turn {
                match turn.probability(&self.audio) {
                    Ok(p) => self.turn_complete = Some(p > 0.5),
                    Err(error) => {
                        emit(json!({"event":"error","message":format!("turn: {error:#}")}));
                        self.turn = None;
                    }
                }
            }
        }
        if !speech && self.quiet_ms >= 220 {
            self.maybe_draft(&partial);
        }
        let unfinished = !self.spotting && policy::unfinished(&partial);
        let hold = if unfinished || self.turn_complete == Some(false) {
            1050
        } else if !self.draft.is_empty() || self.turn_complete == Some(true) {
            480
        } else {
            650
        };
        if self.quiet_ms >= hold || self.speech_ms >= 30000 {
            if self.spotting && !self.spot_end() {
                return Ok(());
            }
            if self.semantic
                && !self.spotting
                && !self.barge_pending
                && !self.candidate(&partial).is_empty()
            {
                let request = self.candidate(&partial);
                if self.semantic_decision.as_deref() != Some("wait")
                    && policy::explicit_handoff(&request)
                {
                    self.semantic_decision = Some("respond".into());
                    self.semantic_text = policy::normalize(&request);
                }
                if self.semantic_decision.as_deref() != Some("respond") {
                    let unavailable = self
                        .semantic_decision
                        .as_deref()
                        .is_some_and(|d| ["error", "unsupported"].contains(&d));
                    let exhausted = self.draft.is_empty() && self.draft_count >= 2;
                    if unavailable || exhausted {
                        diagnostic(
                            "semantic_fallback",
                            json!({"reason":self.semantic_decision.as_deref().unwrap_or("budget"),"policy":"ordinary"}),
                        );
                        self.discard();
                        self.end_turn();
                        return Ok(());
                    }
                    if self.quiet_ms < 6000 && self.speech_ms < 30000 {
                        return Ok(());
                    }
                    if self.semantic_decision.as_deref() != Some("wait") {
                        diagnostic(
                            "semantic_fallback",
                            json!({"reason":"deadline","policy":"ordinary"}),
                        );
                        self.discard();
                    }
                }
            }
            self.end_turn();
        }
        Ok(())
    }

    pub(super) fn confirm_barge(&mut self) {
        self.barge_pending = false;
        self.barge_decide_ms = 0;
        let spoken = self.speaker.spoken();
        self.generation = Value::Null;
        self.speech_job = None;
        self.speaker.hush();
        self.last_turn.clear();
        self.pending.clear();
        emit(json!({"event":"barge","spoken":spoken}));
    }

    pub(super) fn cancel_thinking(&mut self) {
        self.generation = Value::Null;
        self.speech_job = None;
        self.speaker.hush();
        self.thinking = false;
        self.cancelled = true;
        if !self.last_turn.is_empty() {
            self.pending = std::mem::take(&mut self.last_turn);
        }
        let mut event = json!({"event":"barge"});
        if !self.pending.is_empty() {
            event["continuation"] = true.into();
        }
        emit(event);
    }

    fn spot_end(&mut self) -> bool {
        if self
            .spotter
            .as_mut()
            .is_some_and(|s| s.feed(&vec![0.; 4800]))
        {
            self.spotted();
            return true;
        }
        self.wake_decision(false, "kws_no_match", "");
        self.reset();
        false
    }

    fn spotted(&mut self) {
        self.spotting = false;
        if let Some(spotter) = &mut self.spotter {
            spotter.clear();
        }
        self.asr.start();
        for frame in self.audio.chunks(WINDOW) {
            self.asr.feed(frame);
        }
    }

    pub(super) fn end_turn(&mut self) {
        let pending = self.barge_pending;
        let mut draft = self.draft.clone();
        self.final_speech_ms = self.speech_ms;
        diagnostic(
            "asr_finalize_start",
            json!({"turn_id":self.turn_id,"last_speech_ns":self.last_speech_ns}),
        );
        let transcript = self.asr.finish();
        if self.semantic && !pending {
            let request = self.candidate(&transcript);
            let same = policy::normalize(&request) == self.semantic_text;
            if !request.is_empty()
                && ((same && self.semantic_decision.as_deref() == Some("wait"))
                    || policy::explicit_hold(&request))
            {
                self.pending = request.chars().take(4096).collect();
                self.extend();
                self.conversation(true);
                diagnostic("semantic_deferred", json!({"decision":"wait"}));
                self.discard();
                self.reset();
                return;
            }
            if !request.is_empty() && !same {
                diagnostic(
                    "semantic_fallback",
                    json!({"reason":"final_unclassified","policy":"ordinary"}),
                );
                self.discard();
                draft.clear();
            }
        }
        diagnostic(
            "asr_finalize_end",
            json!({"turn_id":self.turn_id,"result_chars":transcript.chars().count()}),
        );
        if pending {
            if policy::backchannel(&transcript) {
                self.discard();
                self.reset();
                self.speaker.resume();
                return;
            }
            self.confirm_barge();
        }
        self.reset();
        self.deliver(&transcript, &draft);
    }

    pub fn deliver(&mut self, transcript: &str, draft: &str) {
        if transcript.trim().is_empty() {
            if !self.active() {
                self.wake_decision(false, "empty_final", "");
            }
            self.pending.clear();
            if !draft.is_empty() {
                emit(json!({"event":"draft_cancel"}));
            }
            return;
        }
        let request = if self.active() {
            let request =
                policy::wake_split(transcript).unwrap_or_else(|| policy::sentence(transcript));
            if self.thinking {
                if policy::backchannel(&request) {
                    if !draft.is_empty() {
                        emit(json!({"event":"draft_cancel"}));
                    }
                    return;
                }
                self.cancel_thinking();
            }
            let request = if self.pending.is_empty() {
                request
            } else {
                let first = std::mem::take(&mut self.pending);
                policy::join(&first, &request)
            };
            self.extend();
            request
        } else {
            let Some(request) = policy::wake_split(transcript) else {
                self.wake_decision(false, "final_name_mismatch", transcript);
                if !draft.is_empty() {
                    emit(json!({"event":"draft_cancel"}));
                }
                return;
            };
            self.wake_decision(
                true,
                if request.is_empty() {
                    "name_only"
                } else {
                    "name_and_request"
                },
                transcript,
            );
            self.extend();
            self.conversation(true);
            if request.is_empty() {
                if !draft.is_empty() {
                    emit(json!({"event":"draft_cancel"}));
                }
                emit(json!({"event":"wake","turn_id":self.turn_id}));
                return;
            }
            request
        };
        if !request.is_empty() {
            if !draft.is_empty() && policy::normalize(&request) != draft {
                emit(json!({"event":"draft_cancel"}));
            }
            self.cancelled = false;
            self.last_turn = request.clone();
            self.thinking = true;
            emit(
                json!({"event":"transcript","text":request.chars().take(240).collect::<String>(),"final":true}),
            );
            emit(json!({"event":"turn","text":request,"turn_id":self.turn_id}));
        } else if !draft.is_empty() {
            emit(json!({"event":"draft_cancel"}));
        }
    }
}
