use anyhow::{Context, Result, ensure};
use ort::{session::Session, value::Tensor};
use rustfft::{Fft, FftPlanner, num_complex::Complex};
use sherpa_onnx::{
    KeywordSpotter, KeywordSpotterConfig, OnlineModelConfig, OnlineRecognizer,
    OnlineRecognizerConfig, OnlineStream,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub const RATE: usize = 16000;
pub const WINDOW: usize = 512;

pub fn initialize() -> Result<()> {
    let path = std::env::var("ORT_DYLIB_PATH")
        .context("set ORT_DYLIB_PATH to the packaged native libonnxruntime.so")?;
    // ort rc.10 panics on dlopen failures. Validate the native ABI first so an
    // unavailable runtime is an ordinary configuration error at our boundary.
    #[expect(
        unsafe_code,
        reason = "validate explicitly configured ONNX Runtime with dlopen"
    )]
    // SAFETY: loading the configured inference runtime is the requested FFI
    // boundary; the owning Library remains alive until ort acquires its handle.
    let library = unsafe { libloading::Library::new(&path) }
        .context("load configured ONNX Runtime shared library")?;
    ort::init_from(path).commit()?;
    drop(library);
    Ok(())
}

pub fn session(path: &Path, threads: usize) -> Result<Session> {
    Session::builder()?
        .with_intra_threads(threads)?
        .with_inter_threads(1)?
        .with_config_entry("session.intra_op.allow_spinning", "0")?
        .commit_from_file(path)
        .with_context(|| format!("load {}", path.display()))
}

pub struct Vad {
    session: Session,
    state: Vec<f32>,
    context: [f32; 64],
}

impl Vad {
    pub fn new(path: &Path) -> Result<Self> {
        Ok(Self {
            session: session(path, 1)?,
            state: vec![0.; 256],
            context: [0.; 64],
        })
    }

    pub fn reset(&mut self) {
        self.state.fill(0.);
        self.context.fill(0.);
    }

    pub fn probability(&mut self, window: &[f32]) -> Result<f32> {
        ensure!(window.len() == WINDOW, "VAD requires 512 samples");
        let mut audio = self.context.to_vec();
        audio.extend_from_slice(window);
        let outputs = self.session.run(ort::inputs!["input" => Tensor::from_array(([1, 576], audio))?, "state" => Tensor::from_array(([2,1,128], self.state.clone()))?, "sr" => Tensor::from_array((Vec::<usize>::new(), vec![16000_i64]))?])?;
        let probability = *outputs[0]
            .try_extract_tensor::<f32>()?
            .1
            .first()
            .context("empty VAD result")?;
        self.state = outputs[1].try_extract_tensor::<f32>()?.1.to_vec();
        self.context.copy_from_slice(&window[WINDOW - 64..]);
        ensure!(probability.is_finite(), "nonfinite VAD result");
        Ok(probability)
    }
}

pub struct TurnFeatures {
    fft: Arc<dyn Fft<f64>>,
    filters: Vec<f64>,
    window: Vec<f64>,
}

impl Default for TurnFeatures {
    fn default() -> Self {
        fn mel(hz: f64) -> f64 {
            if hz >= 1000. {
                15. + (hz / 1000.).ln() * 27. / 6.4_f64.ln()
            } else {
                3. * hz / 200.
            }
        }
        fn hz(mel: f64) -> f64 {
            if mel >= 15. {
                1000. * (6.4_f64.ln() / 27. * (mel - 15.)).exp()
            } else {
                200. * mel / 3.
            }
        }
        let points: Vec<f64> = (0..82).map(|i| hz(i as f64 * mel(8000.) / 81.)).collect();
        let filters = (0..80)
            .flat_map(|m| {
                let points = &points;
                (0..201).map(move |bin| {
                    let f = bin as f64 * 40.;
                    ((f - points[m]) / (points[m + 1] - points[m]))
                        .min((points[m + 2] - f) / (points[m + 2] - points[m + 1]))
                        .max(0.)
                        * 2.
                        / (points[m + 2] - points[m])
                })
            })
            .collect();
        Self {
            fft: FftPlanner::new().plan_fft_forward(400),
            filters,
            window: (0..400)
                .map(|i| 0.5 - 0.5 * (2. * std::f64::consts::PI * i as f64 / 400.).cos())
                .collect(),
        }
    }
}

impl TurnFeatures {
    pub fn extract(&self, audio: &[f32]) -> Vec<f32> {
        let length = RATE * 8;
        let kept = audio.len().min(length);
        let mut padded = vec![0_f64; length];
        for (out, sample) in padded[length - kept..]
            .iter_mut()
            .zip(&audio[audio.len() - kept..])
        {
            *out = f64::from(*sample);
        }
        let mean = padded.iter().sum::<f64>() / length as f64;
        let variance = padded.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / length as f64;
        let scale = (variance + 1e-7).sqrt();
        for value in &mut padded {
            *value = (*value - mean) / scale;
        }
        let mut features = vec![0_f64; 80 * 800];
        let mut frame = vec![Complex::default(); 400];
        for t in 0..800 {
            for (k, out) in frame.iter_mut().enumerate() {
                let index = t as isize * 160 + k as isize - 200;
                let reflected = if index < 0 {
                    -index as usize
                } else if index >= length as isize {
                    2 * length - index as usize - 2
                } else {
                    index as usize
                };
                *out = Complex::new(padded[reflected] * self.window[k], 0.);
            }
            self.fft.process(&mut frame);
            for m in 0..80 {
                features[m * 800 + t] = frame[..201]
                    .iter()
                    .zip(&self.filters[m * 201..(m + 1) * 201])
                    .map(|(x, w)| x.norm_sqr() * w)
                    .sum::<f64>()
                    .max(1e-10)
                    .log10();
            }
        }
        let floor = features.iter().copied().fold(f64::NEG_INFINITY, f64::max) - 8.;
        features
            .into_iter()
            .map(|x| ((x.max(floor) + 4.) / 4.) as f32)
            .collect()
    }
}

pub struct Turn {
    session: Session,
    features: TurnFeatures,
}

impl Turn {
    pub fn new(path: &Path) -> Result<Self> {
        Ok(Self {
            session: session(path, 2)?,
            features: TurnFeatures::default(),
        })
    }

    pub fn probability(&mut self, audio: &[f32]) -> Result<f32> {
        let output=self.session.run(ort::inputs!["input_features"=>Tensor::from_array(([1,80,800],self.features.extract(audio)))?])?;
        let probability = *output[0]
            .try_extract_tensor::<f32>()?
            .1
            .first()
            .context("empty turn result")?;
        ensure!(probability.is_finite(), "nonfinite turn result");
        Ok(probability.clamp(0., 1.))
    }
}

fn transducer(folder: &Path, threads: i32) -> Result<OnlineModelConfig> {
    fn pick(folder: &Path, prefix: &str) -> Result<String> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(folder)?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| {
                path.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with(prefix))
                    && path.extension().is_some_and(|ext| ext == "onnx")
            })
            .collect();
        files.sort_by_key(|p| (!p.to_string_lossy().contains(".int8."), p.clone()));
        Ok(files
            .first()
            .with_context(|| format!("no {prefix} ONNX in {}", folder.display()))?
            .to_string_lossy()
            .into_owned())
    }
    let tokens = folder.join("tokens.txt");
    ensure!(tokens.is_file(), "missing {}", tokens.display());
    let mut config = OnlineModelConfig {
        num_threads: threads,
        tokens: Some(tokens.to_string_lossy().into_owned()),
        ..Default::default()
    };
    config.transducer.encoder = Some(pick(folder, "encoder")?);
    config.transducer.decoder = Some(pick(folder, "decoder")?);
    config.transducer.joiner = Some(pick(folder, "joiner")?);
    Ok(config)
}

pub struct Recognizer {
    recognizer: OnlineRecognizer,
    stream: Option<OnlineStream>,
}

impl Recognizer {
    pub fn new(folder: &Path) -> Result<Self> {
        let config = OnlineRecognizerConfig {
            model_config: transducer(folder, 2)?,
            decoding_method: Some("greedy_search".into()),
            enable_endpoint: false,
            ..Default::default()
        };
        Ok(Self {
            recognizer: OnlineRecognizer::create(&config).context("create streaming ASR")?,
            stream: None,
        })
    }

    pub fn start(&mut self) {
        let stream = self.recognizer.create_stream();
        stream.accept_waveform(16000, &vec![0.; 4800]);
        self.stream = Some(stream);
    }

    pub fn feed(&mut self, audio: &[f32]) {
        if self.stream.is_none() {
            self.start();
        }
        if let Some(stream) = &self.stream {
            stream.accept_waveform(16000, audio);
            while self.recognizer.is_ready(stream) {
                self.recognizer.decode(stream);
            }
        }
    }

    pub fn partial(&self) -> String {
        self.stream
            .as_ref()
            .and_then(|s| self.recognizer.get_result(s))
            .map(|r| r.text)
            .unwrap_or_default()
    }

    pub fn finish(&mut self) -> String {
        if self.stream.is_none() {
            return String::new();
        }
        self.feed(&vec![0.; 4800]);
        if let Some(stream) = &self.stream {
            stream.input_finished();
            while self.recognizer.is_ready(stream) {
                self.recognizer.decode(stream);
            }
        }
        let result = self.partial();
        self.clear();
        result
    }

    pub fn clear(&mut self) {
        self.stream = None;
    }
}

pub struct Spotter {
    spotter: KeywordSpotter,
    stream: Option<OnlineStream>,
}

impl Spotter {
    pub fn new(folder: &Path, keywords: &Path) -> Result<Self> {
        ensure!(keywords.is_file(), "missing keyword list");
        let config = KeywordSpotterConfig {
            model_config: transducer(folder, 1)?,
            keywords_file: Some(keywords.to_string_lossy().into_owned()),
            keywords_score: 2.,
            keywords_threshold: 0.05,
            max_active_paths: 4,
            num_trailing_blanks: 1,
            ..Default::default()
        };
        Ok(Self {
            spotter: KeywordSpotter::create(&config).context("create keyword spotter")?,
            stream: None,
        })
    }

    pub fn start(&mut self) {
        self.stream = Some(self.spotter.create_stream());
    }

    pub fn clear(&mut self) {
        self.stream = None;
    }

    pub fn feed(&mut self, audio: &[f32]) -> bool {
        if let Some(stream) = &self.stream {
            stream.accept_waveform(16000, audio);
            while self.spotter.is_ready(stream) {
                self.spotter.decode(stream);
                if self
                    .spotter
                    .get_result(stream)
                    .is_some_and(|r| !r.keyword.is_empty())
                {
                    return true;
                }
            }
        }
        false
    }
}
