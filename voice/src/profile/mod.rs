//! Voice profiles: the always-loaded CPU profile, and the high-VRAM profile that loads under
//! `gpu-run` while Tohru is on screen and GPU memory allows.
mod catalog;
pub mod config;
mod gpu;
mod selector;
mod spawn;
mod wire;
mod worker;

use crate::protocol::{emit, log};
pub use config::{LowPrecision, ProfileConfig};
pub use selector::Active;
use selector::{Core, Event, Loading, Machine, Published};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    thread::JoinHandle,
    time::Duration,
};
pub use worker::{HighError, HighVoice};

/// How long an answer holds its first words for a reload in progress before speaking with low.
const RELOAD_WAIT: Duration = Duration::from_secs(3);
/// A reload older than this is stuck in the scheduler's queue, not loading, so nothing waits.
const RELOAD_FRESH: Duration = Duration::from_secs(20);

/// The handle the voice service and speaker share; cheap to clone.
#[derive(Clone, Default)]
pub struct Profiles {
    events: Option<mpsc::Sender<Event>>,
    published: Published,
    loading: Loading,
}

pub struct Selector {
    pub profiles: Profiles,
    thread: Option<JoinHandle<()>>,
}

impl Selector {
    /// Low only: nothing touches the GPU.
    pub fn low() -> Self {
        Self {
            profiles: Profiles::default(),
            thread: None,
        }
    }

    pub fn start(config: ProfileConfig, script: PathBuf) -> Self {
        let Some(high) = config.high.clone() else {
            return Self::low();
        };
        log(&format!(
            "voice profile: low (cpu, {}) loaded; high (cuda, {}) loads when Tohru appears with GPU VRAM under {}%",
            match config.low_precision {
                LowPrecision::Int8 => "int8",
                LowPrecision::Float32 => "float32",
            },
            high.precision.name(),
            config.threshold_percent
        ));
        let (sender, events) = mpsc::channel();
        let published = Published::default();
        let loading = Loading::default();
        let notify = Box::new(|active: Active, reason: &str| {
            emit(json!({"event":"profile","active":active.name(),"reason":reason}));
        });
        let core = Core::new(
            config,
            high,
            Machine { script },
            sender.clone(),
            published.clone(),
            loading.clone(),
            notify,
        );
        let thread = std::thread::Builder::new()
            .name("voice-profiles".into())
            .spawn(move || selector::run(core, events))
            .ok();
        Self {
            profiles: Profiles {
                events: thread.is_some().then_some(sender),
                published,
                loading,
            },
            thread,
        }
    }

    /// Releases the reservation before the service exits.
    pub fn shutdown(mut self) {
        self.profiles.send(Event::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Profiles {
    fn send(&self, event: Event) {
        if let Some(events) = &self.events {
            let _ = events.send(event);
        }
    }

    pub fn presence(&self, visible: bool) {
        self.send(Event::Presence(visible));
    }

    /// Starts loading as a voice turn begins, ahead of the show it causes.
    pub fn wake(&self) {
        self.send(Event::Wake);
    }

    /// The loaded high profile and its identity, for reporting its failure.
    pub fn high(&self) -> Option<(u64, Arc<dyn HighVoice>)> {
        self.published
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// As `high`, but first waits up to `RELOAD_WAIT` for a fresh reload, until `stale`.
    pub fn high_soon(&self, stale: &dyn Fn() -> bool) -> Option<(u64, Arc<dyn HighVoice>)> {
        let deadline = std::time::Instant::now() + RELOAD_WAIT;
        loop {
            if let Some(high) = self.high() {
                return Some(high);
            }
            let since = *self
                .loading
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if stale()
                || std::time::Instant::now() >= deadline
                || since.is_none_or(|at| at.elapsed() >= RELOAD_FRESH)
            {
                return None;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// The worker broke; answers return to the low profile at once.
    pub fn failed(&self, id: u64, reason: String) {
        let mut published = self
            .published
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if published
            .as_ref()
            .is_some_and(|(current, _)| *current == id)
        {
            *published = None;
        }
        drop(published);
        self.send(Event::Failed(Some(id), reason));
    }

    #[cfg(test)]
    pub fn loaded(voice: Arc<dyn HighVoice>) -> Self {
        Self {
            events: None,
            published: Arc::new(std::sync::Mutex::new(Some((1, voice)))),
            loading: Loading::default(),
        }
    }

    /// Profiles whose requests are counted instead of handled.
    #[cfg(test)]
    pub fn watched() -> (Self, Watch) {
        let (sender, events) = mpsc::channel();
        let profiles = Self {
            events: Some(sender),
            ..Self::default()
        };
        (profiles, Watch(events))
    }

    /// A reload that the selector publishes `after` from now.
    #[cfg(test)]
    pub fn reloading(voice: Arc<dyn HighVoice>, after: Duration) -> Self {
        let profiles = Self::default();
        *profiles.loading.lock().unwrap() = Some(std::time::Instant::now());
        let (published, loading) = (profiles.published.clone(), profiles.loading.clone());
        std::thread::spawn(move || {
            std::thread::sleep(after);
            *published.lock().unwrap() = Some((1, voice));
            *loading.lock().unwrap() = None;
        });
        profiles
    }
}

#[cfg(test)]
pub struct Watch(mpsc::Receiver<Event>);

#[cfg(test)]
impl Watch {
    pub fn wakes(&self) -> usize {
        self.0
            .try_iter()
            .filter(|event| matches!(event, Event::Wake))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    struct Mute;

    impl HighVoice for Mute {
        fn transcribes(&self) -> bool {
            false
        }

        fn engine(&self) -> &str {
            "mute"
        }

        fn speak(
            &self,
            _: &str,
            _: &dyn Fn() -> bool,
        ) -> Result<Option<(Vec<f32>, u32)>, HighError> {
            Ok(None)
        }

        fn transcribe(&self, _: &[f32], _: Instant) -> Result<Option<String>, HighError> {
            Ok(None)
        }
    }

    #[test]
    fn an_answer_waits_for_a_fresh_reload_but_not_for_a_stale_or_absent_one() {
        let started = Instant::now();
        let profiles = Profiles::reloading(Arc::new(Mute), Duration::from_millis(200));
        assert!(profiles.high_soon(&|| false).is_some());
        assert!(started.elapsed() >= Duration::from_millis(200));

        let started = Instant::now();
        assert!(Profiles::default().high_soon(&|| false).is_none());
        let profiles = Profiles::reloading(Arc::new(Mute), Duration::from_secs(30));
        assert!(
            profiles.high_soon(&|| true).is_none(),
            "a hushed answer stops waiting"
        );
        assert!(started.elapsed() < Duration::from_millis(100));

        *profiles.loading.lock().unwrap() = Instant::now().checked_sub(RELOAD_FRESH);
        assert!(
            profiles.high_soon(&|| false).is_none(),
            "a load still queued long after it began answers with low"
        );
        assert!(started.elapsed() < Duration::from_millis(100));

        let started = Instant::now();
        *profiles.loading.lock().unwrap() = Instant::now().checked_sub(Duration::from_secs(10));
        assert!(profiles.high_soon(&|| false).is_none());
        let waited = started.elapsed();
        assert!(
            (RELOAD_WAIT..RELOAD_WAIT + Duration::from_secs(1)).contains(&waited),
            "a reload begun at her name is waited on from the answer, {waited:?}"
        );
    }
}
