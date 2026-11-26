use anyhow::{Context, Result, ensure};
use ort::{
    session::{RunOptions, Session},
    value::Tensor,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub engine: String,
    pub model: PathBuf,
    pub voicepack: String,
    pub voice: String,
    pub speed: i32,
    pub pitch: i32,
    pub load_on_conversation: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            engine: "kokoro".into(),
            model: PathBuf::new(),
            voicepack: "tide".into(),
            voice: "en-us+f3".into(),
            speed: 100,
            pitch: 3,
            load_on_conversation: false,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Clip {
    pub samples: Vec<f32>,
    pub rate: u32,
    pub phonemes: String,
    pub timeline: Vec<(f64, f64, u8)>,
}

impl Clip {
    pub fn trim(&mut self) {
        let first = self.samples.iter().position(|s| s.abs() >= 0.0001);
        let last = self.samples.iter().rposition(|s| s.abs() >= 0.0001);
        if let (Some(first), Some(last)) = (first, last) {
            let guard = (self.rate as f64 * 0.08).round() as usize;
            let start = first.saturating_sub(guard);
            let end = (last + guard + 1).min(self.samples.len());
            let offset = start as f64 / self.rate as f64;
            let length = (end - start) as f64 / self.rate as f64;
            self.timeline = self
                .timeline
                .iter()
                .filter(|(a, b, _)| *b > offset && *a < end as f64 / self.rate as f64)
                .map(|(a, b, m)| ((a - offset).max(0.), (b - offset).min(length), *m))
                .collect();
            self.samples = self.samples[start..end].to_vec();
        }
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: self.rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(path, spec)?;
        for sample in &self.samples {
            writer.write_sample(*sample)?;
        }
        writer.finalize()?;
        Ok(())
    }
}

pub struct Kokoro {
    session: Session,
    vocab: HashMap<char, i64>,
    pack: Vec<f32>,
    frontend: crate::frontend::English,
}

impl Kokoro {
    pub fn new(settings: &Settings, assets: &Path) -> Result<Self> {
        ensure!(
            !settings.model.as_os_str().is_empty(),
            "assistant.tts_model is not set (the Kokoro folder holding model.onnx, tokenizer.json and voices/)"
        );
        let model = ["model.onnx", "kokoro-v1.0.onnx", "model_fp32.onnx"]
            .into_iter()
            .map(|name| settings.model.join(name))
            .find(|p| p.is_file() && settings.model.join("tokenizer.json").is_file())
            .with_context(|| {
                format!(
                    "assistant.tts_model: {} has no model.onnx and tokenizer.json",
                    settings.model.display()
                )
            })?;
        let tokenizer: serde_json::Value =
            serde_json::from_slice(&std::fs::read(settings.model.join("tokenizer.json"))?)?;
        let vocab = tokenizer["model"]["vocab"]
            .as_object()
            .context("missing Kokoro vocabulary")?
            .iter()
            .filter_map(|(s, n)| {
                let mut chars = s.chars();
                let c = chars.next()?;
                if chars.next().is_some() {
                    return None;
                }
                Some((c, n.as_i64()?))
            })
            .collect();
        let pack_path = if settings.voicepack == "tide" {
            assets.join("tohru-tide.bin")
        } else if Path::new(&settings.voicepack).is_absolute() {
            PathBuf::from(&settings.voicepack)
        } else {
            ensure!(
                !settings.voicepack.contains("..")
                    && settings
                        .voicepack
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c)),
                "invalid voicepack name"
            );
            settings
                .model
                .join("voices")
                .join(format!("{}.bin", settings.voicepack))
        };
        let bytes = std::fs::read(&pack_path)
            .with_context(|| format!("read voicepack {}", pack_path.display()))?;
        ensure!(
            bytes.len() % 1024 == 0 && bytes.len() >= 2048,
            "voicepack needs rows of 256 float32 values"
        );
        let pack = bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        Ok(Self {
            // Match Python Kokoro's thread and default spinning policy. The
            // VAD and turn sessions separately disable worker spinning.
            session: Session::builder()?
                .with_intra_threads(4)?
                .with_inter_threads(1)?
                .commit_from_file(&model)?,
            vocab,
            pack,
            frontend: crate::frontend::English::default(),
        })
    }

    pub fn synthesize(&mut self, text: &str, speed: i32, options: &RunOptions) -> Result<Clip> {
        let phonemes = self.frontend.phonemes(text)?;
        self.from_phonemes(&phonemes, speed, options)
    }

    pub fn from_phonemes(
        &mut self,
        phonemes: &str,
        speed: i32,
        options: &RunOptions,
    ) -> Result<Clip> {
        let kept: String = phonemes
            .chars()
            .filter(|c| self.vocab.contains_key(c))
            .take(510.min(self.pack.len() / 256 - 1))
            .collect();
        let mut ids = vec![0_i64];
        ids.extend(kept.chars().map(|c| self.vocab[&c]));
        let length = ids.len() - 1;
        ids.push(0);
        let samples = if length == 0 {
            Vec::new()
        } else {
            let style = self.pack[length * 256..(length + 1) * 256].to_vec();
            let outputs=self.session.run_with_options(ort::inputs!["input_ids"=>Tensor::from_array(([1,ids.len()],ids))?,"style"=>Tensor::from_array(([1,256],style))?,"speed"=>Tensor::from_array(([1],vec![(speed as f32/100.).clamp(0.5,2.)]))?],options)?;
            let data = outputs[0].try_extract_tensor::<f32>()?.1;
            ensure!(
                data.len() <= 24000 * 60 && data.iter().all(|s| s.is_finite()),
                "invalid Kokoro waveform"
            );
            data.to_vec()
        };
        let timeline = timeline(&samples, 24000, &kept);
        Ok(Clip {
            samples,
            rate: 24000,
            phonemes: kept,
            timeline,
        })
    }
}

pub enum Engine {
    Kokoro(Box<Kokoro>),
    Espeak,
}

impl Engine {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Kokoro(_) => "kokoro",
            Self::Espeak => "espeak-ng",
        }
    }

    pub fn load(settings: &Settings, assets: &Path) -> Result<Self> {
        match settings.engine.as_str() {
            "kokoro" => Ok(Self::Kokoro(Box::new(Kokoro::new(settings, assets)?))),
            "espeak-ng" => Ok(Self::Espeak),
            other => anyhow::bail!("unknown TTS engine {other}"),
        }
    }

    pub fn synthesize(
        &mut self,
        text: &str,
        settings: &Settings,
        options: &RunOptions,
    ) -> Result<Clip> {
        if let Self::Kokoro(kokoro) = self {
            return kokoro.synthesize(text, settings.speed, options);
        }
        let wpm = (175. * settings.speed as f64 / 100.).round_ties_even() as i32;
        let pitch = settings
            .pitch
            .saturating_mul(4)
            .saturating_add(50)
            .clamp(0, 99);
        let bytes = crate::process::output(
            Command::new("espeak-ng").args([
                "--stdout",
                "-v",
                &settings.voice,
                "-s",
                &wpm.to_string(),
                "-p",
                &pitch.to_string(),
                "--stdin",
            ]),
            text.as_bytes(),
            Duration::from_secs(30),
            24000 * 60 * 2 + 65536,
        )?;
        let (rate, samples) = espeak_pcm(bytes)?;
        let phonemes = espeak_phonemes(text, &settings.voice, wpm, pitch);
        let timeline = timeline(&samples, rate, &phonemes);
        Ok(Clip {
            samples,
            rate,
            phonemes,
            timeline,
        })
    }
}

/// eSpeak's IPA for `text`, for mouth timing; empty when eSpeak cannot run.
pub fn espeak_phonemes(text: &str, voice: &str, wpm: i32, pitch: i32) -> String {
    let ipa = crate::process::output(
        Command::new("espeak-ng").args([
            "-q",
            "--ipa",
            "-v",
            voice,
            "-s",
            &wpm.to_string(),
            "-p",
            &pitch.to_string(),
            "--stdin",
        ]),
        text.as_bytes(),
        Duration::from_secs(10),
        65536,
    )
    .unwrap_or_default();
    String::from_utf8_lossy(&ipa).into_owned()
}

pub fn timeline(samples: &[f32], rate: u32, phonemes: &str) -> Vec<(f64, f64, u8)> {
    let step = (rate as usize / 100).max(1);
    let loud: Vec<usize> = samples
        .chunks(step)
        .enumerate()
        .filter_map(|(i, frame)| {
            let rms = (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt();
            (rms > 0.01).then_some(i * step)
        })
        .collect();
    let start = loud.first().copied().unwrap_or(0) as f64 / rate as f64;
    let end = loud
        .last()
        .map_or(samples.len(), |i| (i + step).min(samples.len())) as f64
        / rate as f64;
    let mut sounds: Vec<(u8, f64)> = Vec::new();
    for c in phonemes.chars() {
        let mouth = match c {
            'm' | 'b' | 'p' | 'ɱ' => Some(1),
            'f' | 'v' => Some(7),
            c if "aɑæʌɐaIW".contains(c) => Some(2),
            c if "eɛəɜɚᵻᵊɘAkgŋhxɡ".contains(c) => Some(3),
            c if "iɪjytdnlszθðɾɫʔ".contains(c) => Some(4),
            c if "oɔɒOY".contains(c) => Some(5),
            c if "uʊwɹrʃʒʤʧɥ".contains(c) => Some(6),
            _ => None,
        };
        if "ːˑ".contains(c) {
            if let Some((_, w)) = sounds.last_mut() {
                *w += 0.5;
            }
        } else if let Some(mouth) = mouth {
            sounds.push((
                mouth,
                if "aɑæʌɐeɛəɜɚᵻᵊɘiɪyoɔɒuʊAIOWY".contains(c) {
                    1.
                } else if mouth == 1 {
                    0.8
                } else {
                    0.6
                },
            ));
        } else if !sounds.is_empty() && ",.;:!?—…".contains(c) {
            sounds.push((0, 1.2));
        } else if c.is_whitespace() && sounds.last().is_some_and(|(m, _)| *m != 0) {
            sounds.push((0, 0.15));
        }
    }
    while sounds.last().is_some_and(|(m, _)| *m == 0) {
        sounds.pop();
    }
    let total = sounds.iter().map(|(_, w)| w).sum::<f64>();
    if total <= 0. || end <= start {
        return Vec::new();
    }
    let mut spans: Vec<(f64, f64, u8)> = Vec::new();
    let mut at = 0.;
    for (mouth, weight) in sounds {
        let next = at + weight * (end - start) / total;
        if let Some(last) = spans.last_mut().filter(|s| s.2 == mouth) {
            last.1 = next;
        } else {
            spans.push((at, next, mouth));
        }
        at = next;
    }
    let mut merged: Vec<(f64, f64, u8)> = Vec::new();
    for span in spans {
        if let Some(last) = merged.last_mut().filter(|last| {
            (span.1 - span.0 < 2. / 30. && span.2 != 1)
                || (last.1 - last.0 < 2. / 30. && last.2 != 1)
        }) {
            let mouth = if last.1 - last.0 >= span.1 - span.0 {
                last.2
            } else {
                span.2
            };
            last.1 = span.1;
            last.2 = mouth;
        } else {
            merged.push(span);
        }
        let n = merged.len();
        if n > 1 && merged[n - 2].2 == merged[n - 1].2 {
            let end = merged[n - 1].1;
            merged.pop();
            merged[n - 2].1 = end;
        }
    }
    merged
        .into_iter()
        .map(|(a, b, m)| (a + start, b + start, m))
        .collect()
}

fn espeak_pcm(bytes: Vec<u8>) -> Result<(u32, Vec<f32>)> {
    let reader = hound::WavReader::new(Cursor::new(bytes))?;
    let rate = reader.spec().sample_rate;
    ensure!(
        reader.spec().channels == 1 && reader.spec().bits_per_sample == 16,
        "eSpeak WAV must be mono s16"
    );
    // eSpeak cannot seek stdout to finalize RIFF/data sizes. Hound locates
    // and validates the PCM chunk; the captured buffer bounds its real size.
    let declared_samples = reader.len() as usize;
    let cursor = reader.into_inner();
    let data = cursor
        .get_ref()
        .get(cursor.position() as usize..)
        .context("eSpeak PCM data offset")?;
    ensure!(data.len() % 2 == 0, "partial eSpeak PCM sample");
    let samples: Vec<f32> = data
        .chunks_exact(2)
        .take(declared_samples)
        .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.)
        .collect();
    Ok((rate, samples))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouth_timing_uses_voiced_windows_instead_of_isolated_peaks() {
        let mut audio = vec![0.; 720];
        audio[0] = 0.02;
        audio[240..480].fill(0.02);
        assert_eq!(timeline(&audio, 24000, "k"), vec![(0.01, 0.02, 3)]);
    }

    #[test]
    fn streaming_wav_placeholder_length_uses_only_captured_pcm() {
        let mut cursor = Cursor::new(Vec::new());
        {
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 22050,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut writer = hound::WavWriter::new(&mut cursor, spec).unwrap();
            writer.write_sample(1234_i16).unwrap();
            writer.write_sample(-4321_i16).unwrap();
            writer.finalize().unwrap();
        }
        let mut bytes = cursor.into_inner();
        bytes[4..8].copy_from_slice(&0x7ffff024_u32.to_le_bytes());
        let data = bytes.windows(4).position(|w| w == b"data").unwrap() + 4;
        bytes[data..data + 4].copy_from_slice(&0x7ffff000_u32.to_le_bytes());
        let (rate, samples) = espeak_pcm(bytes).unwrap();
        assert_eq!(rate, 22050);
        assert_eq!(samples, vec![1234. / 32768., -4321. / 32768.]);
    }

    #[test]
    fn kokoro_needs_an_explicit_folder() {
        let unset = Kokoro::new(&Settings::default(), Path::new("/"))
            .err()
            .unwrap()
            .to_string();
        assert!(unset.contains("assistant.tts_model is not set"), "{unset}");
        let settings = Settings {
            model: std::env::temp_dir(),
            ..Settings::default()
        };
        let empty = Kokoro::new(&settings, Path::new("/"))
            .err()
            .unwrap()
            .to_string();
        assert!(
            empty.contains("has no model.onnx and tokenizer.json"),
            "{empty}"
        );
    }
}
