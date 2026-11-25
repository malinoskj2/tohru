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
use selector::{Core, Event, Machine, Published};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    thread::JoinHandle,
};
pub use worker::{HighError, HighVoice};

/// The handle the voice service and speaker share; cheap to clone.
#[derive(Clone, Default)]
pub struct Profiles {
    events: Option<mpsc::Sender<Event>>,
    published: Published,
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
        let notify = Box::new(|active: Active, reason: &str| {
            emit(json!({"event":"profile","active":active.name(),"reason":reason}));
        });
        let core = Core::new(
            config,
            high,
            Machine { script },
            sender.clone(),
            published.clone(),
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

    /// The loaded high profile and its identity, for reporting its failure.
    pub fn high(&self) -> Option<(u64, Arc<dyn HighVoice>)> {
        self.published
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
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
        }
    }
}
