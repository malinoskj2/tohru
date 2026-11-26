use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap};

pub const PATHS: &[&str] = &[
    "draft", "normal", "quick", "home", "ack", "notice", "greeting", "task", "alert", "result",
    "stub", "error",
];

/// The controller's label for the reply a speech command starts; unknown labels are dropped.
#[must_use]
pub fn path(message: &Value) -> Value {
    message["path"]
        .as_str()
        .filter(|path| PATHS.contains(path))
        .map_or(Value::Null, Into::into)
}

#[derive(Default)]
struct Turn {
    eou_ns: Option<u64>,
    finalize_ns: Option<u64>,
}

#[derive(Default, Clone)]
struct Generation {
    turn_id: String,
    path: String,
    start_ns: u64,
    text_ns: Option<u64>,
    audio: Option<(u64, String)>,
}

#[derive(Default)]
struct Session {
    turns: HashMap<String, Turn>,
    latest: HashMap<String, usize>,
    starts: Vec<Generation>,
}

fn key(value: &Value) -> Option<String> {
    match value {
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn record(line: &str) -> Option<Map<String, Value>> {
    let at = line.find("tohru-voice: {")?;
    match serde_json::from_str(&line[at + "tohru-voice: ".len()..]) {
        Ok(Value::Object(record)) if record.contains_key("stage") => Some(record),
        _ => None,
    }
}

/// One spoken turn: its end of speech to the first audible sample of the first reply, and of
/// the first reply that is not an acknowledgement.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub path: String,
    pub profile: String,
    pub first_ms: f64,
    pub answer_ms: Option<f64>,
    pub endpoint_ms: Option<f64>,
    pub provider_ms: Option<f64>,
    pub speech_ms: Option<f64>,
}

fn acknowledgement(path: &str) -> bool {
    path == "ack" || path == "notice"
}

fn ms(from: u64, to: u64) -> f64 {
    (to as f64 - from as f64) / 1e6
}

/// Joins the service's diagnostics into per-turn first-audio samples.
pub fn samples<'a>(lines: impl IntoIterator<Item = &'a str>) -> Vec<Sample> {
    let mut sessions: BTreeMap<String, Session> = BTreeMap::new();
    for line in lines {
        let Some(record) = record(line) else { continue };
        let stage = record["stage"].as_str().unwrap_or("");
        let session = sessions
            .entry(record["session"].as_str().unwrap_or("").to_owned())
            .or_default();
        let now = record["mono_ns"].as_u64().unwrap_or(0);
        let generation = key(&record.get("generation").cloned().unwrap_or(Value::Null));
        match stage {
            "asr_finalize_start" => {
                if let Some(turn) = key(&record["turn_id"]) {
                    let entry = session.turns.entry(turn).or_default();
                    entry.eou_ns = record["last_speech_ns"].as_u64();
                    entry.finalize_ns = Some(now);
                }
            }
            "speech_stream_start" | "committed_answer_ready" => {
                let (Some(generation), Some(turn_id)) = (generation, key(&record["turn_id"]))
                else {
                    continue;
                };
                session.latest.insert(generation, session.starts.len());
                session.starts.push(Generation {
                    turn_id,
                    path: record["path"].as_str().unwrap_or("unlabelled").to_owned(),
                    start_ns: now,
                    text_ns: (stage == "committed_answer_ready").then_some(now),
                    audio: None,
                });
            }
            "first_provider_text" => {
                if let Some(entry) = generation
                    .and_then(|g| session.latest.get(&g).copied())
                    .map(|i| &mut session.starts[i])
                    && entry.text_ns.is_none()
                {
                    entry.text_ns = Some(now);
                }
            }
            "first_audio" => {
                if let Some(entry) = generation
                    .and_then(|g| session.latest.get(&g).copied())
                    .map(|i| &mut session.starts[i])
                    && entry.audio.is_none()
                {
                    let onset = record["onset_ms"].as_f64().unwrap_or(0.);
                    entry.audio = Some((
                        now + (onset * 1e6) as u64,
                        record["profile"].as_str().unwrap_or("unknown").to_owned(),
                    ));
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for session in sessions.values() {
        let mut seen: HashMap<&str, usize> = HashMap::new();
        for entry in &session.starts {
            let Some(turn) = session.turns.get(&entry.turn_id) else {
                continue;
            };
            let Some(eou) = turn.eou_ns else { continue };
            let Some((audio, profile)) = &entry.audio else {
                continue;
            };
            if let Some(&index) = seen.get(entry.turn_id.as_str()) {
                let sample: &mut Sample = &mut out[index];
                if sample.answer_ms.is_none() && !acknowledgement(&entry.path) {
                    sample.answer_ms = Some(ms(eou, *audio));
                    sample.path = format!("{}+{}", sample.path, entry.path);
                }
                continue;
            }
            seen.insert(&entry.turn_id, out.len());
            out.push(Sample {
                path: entry.path.clone(),
                profile: profile.clone(),
                first_ms: ms(eou, *audio),
                answer_ms: (!acknowledgement(&entry.path)).then(|| ms(eou, *audio)),
                endpoint_ms: turn.finalize_ns.map(|at| ms(eou, at)),
                provider_ms: entry.text_ns.map(|at| ms(entry.start_ns, at)),
                speech_ms: entry.text_ns.map(|at| ms(at, *audio)),
            });
        }
    }
    out
}

/// Nearest-rank percentile of unsorted values.
#[must_use]
pub fn percentile(values: &[f64], p: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = ((p / 100. * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
    Some(sorted[rank - 1])
}

fn stats(values: &[f64]) -> Value {
    let round = |v: Option<f64>| v.map(|v| (v * 10.).round() / 10.);
    json!({"n":values.len(),"p50":round(percentile(values,50.)),"p95":round(percentile(values,95.))})
}

/// p50/p95 per path and profile.
#[must_use]
pub fn report(samples: &[Sample]) -> Value {
    let mut groups: BTreeMap<(String, String), Vec<&Sample>> = BTreeMap::new();
    for sample in samples {
        groups
            .entry((sample.path.clone(), sample.profile.clone()))
            .or_default()
            .push(sample);
    }
    let rows: Vec<Value> = groups
        .into_iter()
        .map(|((path, profile), group)| {
            let pick = |f: fn(&Sample) -> Option<f64>| -> Vec<f64> {
                group.iter().filter_map(|s| f(s)).collect()
            };
            json!({
                "path": path,
                "profile": profile,
                "first_audio_ms": stats(&pick(|s| Some(s.first_ms))),
                "answer_audio_ms": stats(&pick(|s| s.answer_ms)),
                "endpoint_ms": stats(&pick(|s| s.endpoint_ms)),
                "provider_first_text_ms": stats(&pick(|s| s.provider_ms)),
                "text_to_audio_ms": stats(&pick(|s| s.speech_ms)),
            })
        })
        .collect();
    json!({"turns": samples.len(), "groups": rows})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(fields: Value) -> String {
        format!("tohru-voice: {fields}")
    }

    #[test]
    fn a_turn_joins_end_of_speech_to_its_first_audible_sample() {
        let lines = [
            line(
                json!({"stage":"asr_finalize_start","session":"s","mono_ns":1_650_000_000u64,"turn_id":"7","last_speech_ns":1_000_000_000u64}),
            ),
            line(
                json!({"stage":"speech_stream_start","session":"s","mono_ns":1_700_000_000u64,"turn_id":"7","generation":3,"path":"quick"}),
            ),
            line(
                json!({"stage":"first_provider_text","session":"s","mono_ns":4_700_000_000u64,"generation":3}),
            ),
            line(
                json!({"stage":"first_audio","session":"s","mono_ns":5_400_000_000u64,"generation":3,"profile":"low","onset_ms":80.0}),
            ),
            "unrelated journal noise".to_owned(),
        ];
        let samples = samples(lines.iter().map(String::as_str));
        assert_eq!(samples.len(), 1);
        let sample = &samples[0];
        assert_eq!(
            (sample.path.as_str(), sample.profile.as_str()),
            ("quick", "low")
        );
        assert!((sample.first_ms - 4480.).abs() < 1e-6);
        assert_eq!(sample.answer_ms, Some(sample.first_ms));
        assert!((sample.endpoint_ms.unwrap() - 650.).abs() < 1e-6);
        assert!((sample.provider_ms.unwrap() - 3000.).abs() < 1e-6);
    }

    #[test]
    fn an_acknowledgement_is_first_audio_and_the_answer_is_timed_separately() {
        let lines = [
            line(
                json!({"stage":"asr_finalize_start","session":"s","mono_ns":1_500u64,"turn_id":"1","last_speech_ns":1_000_000u64}),
            ),
            line(
                json!({"stage":"committed_answer_ready","session":"s","mono_ns":2_000_000u64,"turn_id":"1","generation":1,"path":"ack"}),
            ),
            line(
                json!({"stage":"first_audio","session":"s","mono_ns":301_000_000u64,"generation":1,"profile":"low","onset_ms":0}),
            ),
            line(
                json!({"stage":"speech_stream_start","session":"s","mono_ns":3_000_000u64,"turn_id":"1","generation":2,"path":"home"}),
            ),
            line(
                json!({"stage":"first_audio","session":"s","mono_ns":5_001_000_000u64,"generation":2,"profile":"high","onset_ms":0}),
            ),
        ];
        let samples = samples(lines.iter().map(String::as_str));
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].path, "ack+home");
        assert!((samples[0].first_ms - 300.).abs() < 1e-6);
        assert!((samples[0].answer_ms.unwrap() - 5000.).abs() < 1e-6);
    }

    #[test]
    fn nearest_rank_percentiles() {
        let values: Vec<f64> = (1..=20).map(f64::from).collect();
        assert_eq!(percentile(&values, 50.), Some(10.));
        assert_eq!(percentile(&values, 95.), Some(19.));
        assert_eq!(percentile(&[], 50.), None);
    }

    #[test]
    fn unknown_paths_are_not_logged() {
        assert_eq!(path(&json!({"path":"quick"})), json!("quick"));
        assert_eq!(path(&json!({"path":"secret text"})), Value::Null);
    }
}
