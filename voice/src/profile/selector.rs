use super::{
    catalog::{self, Models, ModelsNotReady},
    config::{HighProfile, ProfileConfig},
    gpu::{TelemetryError, Usage},
    worker::{HighVoice, Pending, Plan, Running},
};
use crate::protocol::log;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, RecvTimeoutError, Sender},
    },
    thread,
    time::Instant,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Active {
    Low,
    High,
}

impl Active {
    pub fn name(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::High => "high",
        }
    }
}

/// A high profile that has reported ready; dropping it releases its reservation.
pub(super) trait Loaded: Send {
    fn voice(&self) -> Arc<dyn HighVoice>;
}

/// A reservation that was requested and is queued or loading.
pub(super) trait Launch: Send {
    fn budget_mib(&self) -> u64;
    fn wait(self: Box<Self>, cancelled: &AtomicBool) -> Result<Box<dyn Loaded>, String>;
}

/// What the selector needs from the machine: GPU telemetry, the model catalog and `gpu-run`.
pub(super) trait Backend: Send {
    fn usage(&mut self) -> Result<Usage, TelemetryError>;
    fn resolve(
        &mut self,
        high: &HighProfile,
        root: Option<&Path>,
    ) -> Result<Models, ModelsNotReady>;
    fn scheduler(&self) -> bool;
    fn launch(
        &mut self,
        config: &ProfileConfig,
        high: &HighProfile,
        models: &Models,
        budget_mib: u64,
    ) -> std::io::Result<Box<dyn Launch>>;
}

impl Loaded for Running {
    fn voice(&self) -> Arc<dyn HighVoice> {
        self.client.clone()
    }
}

impl Launch for Pending {
    fn budget_mib(&self) -> u64 {
        Pending::budget_mib(self)
    }

    fn wait(self: Box<Self>, cancelled: &AtomicBool) -> Result<Box<dyn Loaded>, String> {
        Pending::wait(*self, cancelled).map(|running| Box::new(running) as Box<dyn Loaded>)
    }
}

pub(super) struct Machine {
    pub script: PathBuf,
}

impl Backend for Machine {
    fn usage(&mut self) -> Result<Usage, TelemetryError> {
        super::gpu::status()
    }

    fn resolve(
        &mut self,
        high: &HighProfile,
        root: Option<&Path>,
    ) -> Result<Models, ModelsNotReady> {
        catalog::resolve(high, root)
    }

    fn scheduler(&self) -> bool {
        crate::process::installed("gpu-run")
    }

    fn launch(
        &mut self,
        config: &ProfileConfig,
        high: &HighProfile,
        models: &Models,
        budget_mib: u64,
    ) -> std::io::Result<Box<dyn Launch>> {
        let plan = Plan {
            config,
            high,
            models,
            budget_mib,
            script: &self.script,
            launcher: Path::new("gpu-run"),
        };
        super::worker::start(&plan).map(|pending| Box::new(pending) as Box<dyn Launch>)
    }
}

pub(super) enum Event {
    Presence(bool),
    Ready(u64, Box<dyn Loaded>),
    Failed(Option<u64>, String),
    Shutdown,
}

/// Tells the desktop which profile now answers, and why.
pub(super) type Notify = Box<dyn FnMut(Active, &str) + Send>;

/// The loaded high profile that answers use, published by the selector thread.
pub(super) type Published = Arc<Mutex<Option<(u64, Arc<dyn HighVoice>)>>>;

enum Worker {
    Idle,
    Starting { id: u64, cancel: Arc<AtomicBool> },
    Ready { id: u64, _loaded: Box<dyn Loaded> },
}

pub(super) struct Core<B: Backend> {
    config: ProfileConfig,
    high: HighProfile,
    backend: B,
    events: Sender<Event>,
    published: Published,
    notify: Notify,
    worker: Worker,
    next_id: u64,
    visible: bool,
    unload_at: Option<Instant>,
}

impl<B: Backend> Core<B> {
    pub(super) fn new(
        config: ProfileConfig,
        high: HighProfile,
        backend: B,
        events: Sender<Event>,
        published: Published,
        notify: Notify,
    ) -> Self {
        Self {
            config,
            high,
            backend,
            events,
            published,
            notify,
            worker: Worker::Idle,
            next_id: 1,
            visible: false,
            unload_at: None,
        }
    }

    pub(super) fn unload_at(&self) -> Option<Instant> {
        self.unload_at
    }

    fn current(&self) -> Option<u64> {
        match self.worker {
            Worker::Idle => None,
            Worker::Starting { id, .. } | Worker::Ready { id, .. } => Some(id),
        }
    }

    fn publish(&self, voice: Option<(u64, Arc<dyn HighVoice>)>) {
        *self
            .published
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = voice;
    }

    /// Returns false once the selector has shut down.
    pub(super) fn handle(&mut self, event: Event) -> bool {
        match event {
            Event::Presence(visible) => self.presence(visible),
            Event::Ready(id, loaded) => {
                if self.current() != Some(id) {
                    return true;
                }
                self.publish(Some((id, loaded.voice())));
                self.worker = Worker::Ready {
                    id,
                    _loaded: loaded,
                };
                log("voice profile: high loaded; new answers use it");
                (self.notify)(Active::High, "loaded");
            }
            Event::Failed(id, reason) => {
                if id.is_some() && id != self.current() {
                    return true;
                }
                self.stop();
                log(&format!("voice profile: falling back to low; {reason}"));
                (self.notify)(Active::Low, &reason);
            }
            Event::Shutdown => {
                self.visible = false;
                self.unload_at = None;
                self.unload("the voice service stopped");
                return false;
            }
        }
        true
    }

    /// The hidden grace period ended at `now`.
    pub(super) fn expire(&mut self, now: Instant) {
        if !self.visible && self.unload_at.is_some_and(|at| now >= at) {
            self.unload_at = None;
            let seconds = self.config.unload_after.as_secs();
            self.unload(&format!("Tohru has been hidden for {seconds} s"));
        }
    }

    fn presence(&mut self, visible: bool) {
        self.visible = visible;
        self.unload_at = None;
        if visible {
            if self.current().is_none() {
                self.consider();
            }
            return;
        }
        if self.current().is_none() {
            return;
        }
        if self.config.unload_after.is_zero() {
            self.unload("Tohru is hidden");
        } else {
            self.unload_at = Some(Instant::now() + self.config.unload_after);
        }
    }

    fn consider(&mut self) {
        let threshold = self.config.threshold_percent;
        let usage = match self.backend.usage() {
            Ok(usage) => usage,
            Err(error) => {
                log(&format!(
                    "voice profile: staying on low; cannot read GPU use from the scheduler ({error})"
                ));
                return;
            }
        };
        let (percent, committed, total) = (usage.percent, usage.committed_mib, usage.total_mib);
        if percent >= threshold as f64 {
            log(&format!(
                "voice profile: staying on low; GPU VRAM {percent:.0}% used ({committed}/{total} MiB) is at or above {threshold}%"
            ));
            return;
        }
        let models = match self
            .backend
            .resolve(&self.high, self.config.model_root.as_deref())
        {
            Ok(models) => models,
            Err(error) => {
                let aliases = error.aliases();
                log(&format!(
                    "voice profile: staying on low; high-VRAM models not ready ({aliases}): {error}"
                ));
                (self.notify)(Active::Low, &format!("models not ready: {aliases}"));
                return;
            }
        };
        if !self.backend.scheduler() {
            log(
                "voice profile: staying on low; gpu-run is not installed, so no GPU reservation can be held",
            );
            return;
        }
        let budget = self
            .high
            .vram_mib
            .unwrap_or_else(|| catalog::estimate_vram_mib(&models, self.high.precision));
        let launch = match self
            .backend
            .launch(&self.config, &self.high, &models, budget)
        {
            Ok(launch) => launch,
            Err(error) => {
                log(&format!(
                    "voice profile: falling back to low; cannot start gpu-run: {error}"
                ));
                (self.notify)(Active::Low, &error.to_string());
                return;
            }
        };
        let id = self.next_id;
        self.next_id += 1;
        let cancel = Arc::new(AtomicBool::new(false));
        log(&format!(
            "voice profile: GPU VRAM {percent:.0}% used ({committed}/{total} MiB) is under {threshold}%; requesting a {} MiB reservation for {}; answering with low until it is ready",
            launch.budget_mib(),
            models.aliases()
        ));
        self.worker = Worker::Starting {
            id,
            cancel: cancel.clone(),
        };
        let events = self.events.clone();
        thread::spawn(move || {
            let event = match launch.wait(&cancel) {
                Ok(loaded) => Event::Ready(id, loaded),
                Err(reason) => Event::Failed(Some(id), reason),
            };
            let _ = events.send(event);
        });
    }

    /// True when a reservation was held or requested.
    fn stop(&mut self) -> bool {
        self.publish(None);
        match std::mem::replace(&mut self.worker, Worker::Idle) {
            Worker::Idle => false,
            Worker::Starting { cancel, .. } => {
                cancel.store(true, Ordering::Relaxed);
                true
            }
            Worker::Ready { .. } => true,
        }
    }

    fn unload(&mut self, reason: &str) {
        if self.stop() {
            log(&format!(
                "voice profile: high unloaded and its GPU reservation released ({reason}); using low"
            ));
            (self.notify)(Active::Low, reason);
        }
    }
}

/// Every decision runs on this one long-lived thread, which is also the thread that starts
/// `gpu-run`, so the parent-death signal only fires when the voice service itself exits.
pub(super) fn run<B: Backend>(mut core: Core<B>, events: Receiver<Event>) {
    loop {
        let next = match core.unload_at() {
            Some(at) => events.recv_timeout(at.saturating_duration_since(Instant::now())),
            None => events.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match next {
            Ok(event) => {
                if !core.handle(event) {
                    return;
                }
            }
            Err(RecvTimeoutError::Timeout) => core.expire(Instant::now()),
            Err(RecvTimeoutError::Disconnected) => {
                core.handle(Event::Shutdown);
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{
        catalog::{Model, tests::high},
        gpu::tests::status,
        worker::HighError,
    };
    use serde_json::json;
    use std::{sync::mpsc, time::Duration};

    #[derive(Default)]
    struct Record {
        launched: Vec<u64>,
        stopped: usize,
        notices: Vec<(Active, String)>,
    }

    struct Silent;

    impl HighVoice for Silent {
        fn transcribes(&self) -> bool {
            false
        }

        fn engine(&self) -> &str {
            "silent"
        }

        fn speak(
            &self,
            _: &str,
            _: &dyn Fn() -> bool,
        ) -> Result<Option<(Vec<f32>, u32)>, HighError> {
            Ok(Some((vec![0.], 24000)))
        }

        fn transcribe(&self, _: &[f32], _: Instant) -> Result<Option<String>, HighError> {
            Ok(None)
        }
    }

    struct FakeLoaded(Arc<Mutex<Record>>);

    impl Loaded for FakeLoaded {
        fn voice(&self) -> Arc<dyn HighVoice> {
            Arc::new(Silent)
        }
    }

    impl Drop for FakeLoaded {
        fn drop(&mut self) {
            self.0.lock().unwrap().stopped += 1;
        }
    }

    struct FakeLaunch(u64, Result<(), String>, Arc<Mutex<Record>>);

    impl Launch for FakeLaunch {
        fn budget_mib(&self) -> u64 {
            self.0
        }

        fn wait(self: Box<Self>, _: &AtomicBool) -> Result<Box<dyn Loaded>, String> {
            let Self(_, outcome, record) = *self;
            outcome.map(|()| Box::new(FakeLoaded(record)) as Box<dyn Loaded>)
        }
    }

    struct Fake {
        status: serde_json::Value,
        models: Result<(), ModelsNotReady>,
        scheduler: bool,
        outcome: Result<(), String>,
        record: Arc<Mutex<Record>>,
    }

    impl Backend for Fake {
        fn usage(&mut self) -> Result<Usage, TelemetryError> {
            super::super::gpu::usage(&self.status)
        }

        fn resolve(
            &mut self,
            high: &HighProfile,
            _: Option<&Path>,
        ) -> Result<Models, ModelsNotReady> {
            match &self.models {
                Err(ModelsNotReady(problems)) => Err(ModelsNotReady(problems.clone())),
                Ok(()) => Ok(Models {
                    tts: Model {
                        alias: high.tts_model.clone(),
                        path: "/m/voice-tts".into(),
                        bytes: 4_520_159_586,
                    },
                    asr: None,
                }),
            }
        }

        fn scheduler(&self) -> bool {
            self.scheduler
        }

        fn launch(
            &mut self,
            _: &ProfileConfig,
            _: &HighProfile,
            _: &Models,
            budget: u64,
        ) -> std::io::Result<Box<dyn Launch>> {
            self.record.lock().unwrap().launched.push(budget);
            Ok(Box::new(FakeLaunch(
                budget,
                self.outcome.clone(),
                self.record.clone(),
            )))
        }
    }

    struct Harness {
        core: Core<Fake>,
        events: Receiver<Event>,
        published: Published,
        record: Arc<Mutex<Record>>,
    }

    impl Harness {
        fn new(
            unload_after: u64,
            extra: serde_json::Value,
            backend: impl FnOnce(&mut Fake),
        ) -> Self {
            let record = Arc::new(Mutex::new(Record::default()));
            let mut fake = Fake {
                status: status(2000, json!([]), 0),
                models: Ok(()),
                scheduler: true,
                outcome: Ok(()),
                record: record.clone(),
            };
            backend(&mut fake);
            let config = ProfileConfig {
                unload_after: Duration::from_secs(unload_after),
                ..ProfileConfig::default()
            };
            let (sender, events) = mpsc::channel();
            let published = Published::default();
            let notices = record.clone();
            let notify = Box::new(move |active, reason: &str| {
                notices
                    .lock()
                    .unwrap()
                    .notices
                    .push((active, reason.to_owned()))
            });
            let core = Core::new(config, high(extra), fake, sender, published.clone(), notify);
            Self {
                core,
                events,
                published,
                record,
            }
        }

        /// Delivers what the waiter thread reported back.
        fn settle(&mut self) {
            let event = self.events.recv_timeout(Duration::from_secs(5)).unwrap();
            self.core.handle(event);
        }

        fn active(&self) -> Active {
            if self.published.lock().unwrap().is_some() {
                Active::High
            } else {
                Active::Low
            }
        }

        fn launched(&self) -> Vec<u64> {
            self.record.lock().unwrap().launched.clone()
        }

        fn stopped(&self) -> usize {
            self.record.lock().unwrap().stopped
        }

        fn last_notice(&self) -> Option<(Active, String)> {
            self.record.lock().unwrap().notices.last().cloned()
        }
    }

    #[test]
    fn under_threshold_requests_a_reservation_and_switches_when_loaded() {
        let mut harness = Harness::new(30, json!({}), |_| {});
        harness.core.handle(Event::Presence(true));
        assert_eq!(harness.launched(), vec![7168]);
        assert_eq!(
            harness.active(),
            Active::Low,
            "low answers while the high profile queues or loads"
        );
        harness.core.handle(Event::Presence(true));
        assert_eq!(
            harness.launched().len(),
            1,
            "showing again never queues a second reservation"
        );
        harness.settle();
        assert_eq!(harness.active(), Active::High);
        assert_eq!(harness.last_notice(), Some((Active::High, "loaded".into())));
    }

    #[test]
    fn at_or_over_threshold_unready_models_and_missing_scheduler_stay_low() {
        let mut busy = Harness::new(30, json!({}), |f| f.status = status(16000, json!([]), 0));
        busy.core.handle(Event::Presence(true));
        assert!(busy.launched().is_empty());

        let mut unready = Harness::new(30, json!({}), |f| {
            f.models = Err(ModelsNotReady(vec![(
                "voice-tts".into(),
                "readiness is missing".into(),
            )]))
        });
        unready.core.handle(Event::Presence(true));
        assert_eq!(
            unready.last_notice(),
            Some((Active::Low, "models not ready: voice-tts".into()))
        );

        let mut blind = Harness::new(30, json!({}), |f| {
            f.status["telemetry_error"] = "socket missing".into()
        });
        blind.core.handle(Event::Presence(true));
        let mut unscheduled = Harness::new(30, json!({}), |f| f.scheduler = false);
        unscheduled.core.handle(Event::Presence(true));
        assert!(
            unready.launched().is_empty()
                && blind.launched().is_empty()
                && unscheduled.launched().is_empty()
        );
    }

    #[test]
    fn explicit_budget_overrides_the_estimate() {
        let mut harness = Harness::new(30, json!({"vram_mib":9000}), |_| {});
        harness.core.handle(Event::Presence(true));
        assert_eq!(harness.launched(), vec![9000]);
    }

    #[test]
    fn hidden_unloads_after_the_grace_and_releases_the_reservation() {
        let mut harness = Harness::new(30, json!({}), |_| {});
        harness.core.handle(Event::Presence(true));
        harness.settle();
        harness.core.handle(Event::Presence(false));
        let at = harness.core.unload_at().unwrap();
        assert!(at > Instant::now() + Duration::from_secs(29));
        harness.core.expire(at - Duration::from_secs(1));
        assert_eq!(harness.stopped(), 0);
        harness.core.handle(Event::Presence(true));
        assert!(
            harness.core.unload_at().is_none(),
            "coming back cancels the unload"
        );
        harness.core.expire(at + Duration::from_secs(1));
        assert_eq!(
            harness.stopped(),
            0,
            "an expired deadline that was cancelled is ignored"
        );
        harness.core.handle(Event::Presence(false));
        harness
            .core
            .expire(Instant::now() + Duration::from_secs(31));
        assert_eq!((harness.stopped(), harness.active()), (1, Active::Low));
        assert_eq!(
            harness.last_notice(),
            Some((Active::Low, "Tohru has been hidden for 30 s".into()))
        );
        harness.core.handle(Event::Presence(true));
        assert_eq!(
            harness.launched().len(),
            2,
            "the next appearance checks VRAM and loads again"
        );
    }

    #[test]
    fn zero_grace_unloads_at_once() {
        let mut harness = Harness::new(0, json!({}), |_| {});
        harness.core.handle(Event::Presence(true));
        harness.settle();
        harness.core.handle(Event::Presence(false));
        assert_eq!(harness.stopped(), 1);
        assert!(harness.core.unload_at().is_none());
    }

    #[test]
    fn failed_reservation_or_load_falls_back_and_stale_failures_are_ignored() {
        let mut failing = Harness::new(30, json!({}), |f| {
            f.outcome = Err("gpu-run or the worker exited with exit status: 1".into())
        });
        failing.core.handle(Event::Presence(true));
        failing.settle();
        assert_eq!(failing.active(), Active::Low);
        assert!(failing.last_notice().unwrap().1.contains("exited"));

        let mut harness = Harness::new(30, json!({}), |_| {});
        harness.core.handle(Event::Presence(true));
        harness.settle();
        harness.core.handle(Event::Failed(
            Some(99),
            "late failure from an older worker".into(),
        ));
        assert_eq!((harness.active(), harness.stopped()), (Active::High, 0));
        harness.core.handle(Event::Failed(
            Some(1),
            "the high-VRAM voice worker failed: exited".into(),
        ));
        assert_eq!((harness.active(), harness.stopped()), (Active::Low, 1));
    }

    #[test]
    fn a_late_ready_after_unload_releases_at_once() {
        let mut harness = Harness::new(0, json!({}), |_| {});
        harness.core.handle(Event::Presence(true));
        harness.core.handle(Event::Presence(false));
        harness.settle();
        assert_eq!((harness.active(), harness.stopped()), (Active::Low, 1));
    }

    #[test]
    fn shutdown_releases_the_reservation_and_the_thread_never_blocks_callers() {
        let mut harness = Harness::new(30, json!({}), |_| {});
        harness.core.handle(Event::Presence(true));
        harness.settle();
        assert!(!harness.core.handle(Event::Shutdown));
        assert_eq!(harness.stopped(), 1);
        assert_eq!(
            harness.last_notice(),
            Some((Active::Low, "the voice service stopped".into()))
        );

        let harness = Harness::new(30, json!({}), |_| {});
        let (record, published) = (harness.record.clone(), harness.published.clone());
        let (sender, events) = mpsc::channel();
        let core = Core::new(
            ProfileConfig::default(),
            high(json!({})),
            harness.core.backend,
            sender.clone(),
            published,
            Box::new(|_, _| {}),
        );
        let thread = thread::spawn(move || run(core, events));
        sender.send(Event::Presence(true)).unwrap();
        sender.send(Event::Shutdown).unwrap();
        thread.join().unwrap();
        assert_eq!(record.lock().unwrap().launched.len(), 1);
    }
}
