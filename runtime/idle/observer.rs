//! One process-owned standard Wayland idle/output-power connection.
//! No polling at rest: commands use a coalescing calloop ping, fades alone tick.
use anyhow::{Context, Result};
use smithay_client_toolkit::{
    delegate_output, delegate_registry, delegate_seat,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop::{
            EventLoop,
            ping::{Ping, make_ping},
        },
        calloop_wayland_source::WaylandSource,
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{Capability, SeatHandler, SeatState},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tohru_shell::idle::{Config, Effect, Policy, Subscription};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_output, wl_seat},
};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1::{self, ExtIdleNotificationV1},
    ext_idle_notifier_v1::ExtIdleNotifierV1,
};
use wayland_protocols_wlr::output_power_management::v1::client::{
    zwlr_output_power_manager_v1::ZwlrOutputPowerManagerV1,
    zwlr_output_power_v1::{self, ZwlrOutputPowerV1},
};

use tohru_shell::idle::configuration::Configuration;
use tohru_shell::idle::input::{Cancellation, Input, Observation};

pub(super) struct Observer {
    commands: SyncSender<Config>,
    accepted: Mutex<Configuration>,
    latest: Arc<Mutex<Input>>,
    ping: Ping,
    cancellation: Cancellation,
    stop: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

/// Effects leave the protocol thread through bounded/nonblocking native channels.
/// The receiver cancels queued action requests whenever `rearmed` is called.
pub(super) trait Sink: Send + Sync {
    fn opacity(&self, alpha: f32);
    fn status(&self, status: &str);
    fn rearmed(&self);
    fn lock(&self, stamp: u64);
    fn suspend(&self, stamp: u64);
    fn state(&self, stamp: u64);
}

impl Observer {
    pub(super) fn spawn(
        config: Config,
        sink: Arc<dyn Sink>,
        cancellation: Cancellation,
    ) -> Result<Self> {
        let policy = Policy::new(config, false).context("validate idle configuration")?;
        let (commands, receiver) = mpsc::sync_channel(8);
        let (ping, source) = make_ping().context("create idle observer wakeup")?;
        let latest = Arc::new(Mutex::new(Input::new(cancellation.clone())));
        let stop = Arc::new(AtomicBool::new(false));
        let (thread_latest, thread_stop) = (latest.clone(), stop.clone());
        let thread_cancellation = cancellation.clone();
        let thread = thread::Builder::new()
            .name("tohru-idle".into())
            .spawn(move || {
                let result = run(
                    policy,
                    receiver,
                    thread_latest,
                    thread_stop,
                    source,
                    sink.clone(),
                    thread_cancellation,
                );
                if let Err(error) = result {
                    sink.status(&format!("idle unavailable: {error:#}"));
                }
                sink.opacity(0.0);
                sink.rearmed();
            })
            .context("start idle observer")?;
        Ok(Self {
            commands,
            accepted: Mutex::new(Configuration::new(config)),
            latest,
            ping,
            cancellation,
            stop,
            thread: Mutex::new(Some(thread)),
        })
    }

    pub(super) fn configure(&self, config: Config) -> Result<()> {
        Policy::new(config, false).context("validate idle configuration")?;
        let alive = self
            .thread
            .lock()
            .map_err(|_| anyhow::anyhow!("idle observer thread owner poisoned"))?
            .as_ref()
            .is_some_and(|thread| !thread.is_finished());
        anyhow::ensure!(
            !self.stop.load(Ordering::Acquire) && alive,
            "idle observer is no longer running"
        );
        let mut accepted = self
            .accepted
            .lock()
            .map_err(|_| anyhow::anyhow!("idle configuration cache poisoned"))?;
        accepted.update(config, |config| -> Result<()> {
            self.commands
                .try_send(config)
                .context("idle configuration queue unavailable")?;
            self.observe(|state| state.resume = state.resume.wrapping_add(1))?;
            Ok(())
        })?;
        Ok(())
    }

    pub(super) fn observe(&self, change: impl FnOnce(&mut Observation)) -> Result<()> {
        let mut latest = self
            .latest
            .lock()
            .map_err(|_| anyhow::anyhow!("idle observation mutex poisoned"))?;
        latest.update(change);
        drop(latest);
        self.ping.ping();
        Ok(())
    }
    pub(super) fn request_stop(&self) {
        tohru_shell::idle::teardown::request_stop(&self.stop, &self.cancellation, || {
            self.ping.ping()
        });
    }

    pub(super) fn stop(&self) -> Result<()> {
        self.request_stop();
        let mut failures = Vec::new();
        let thread = tohru_shell::idle::teardown::take_owned(
            &self.thread,
            tohru_shell::idle::teardown::Stage::Observer,
            &mut failures,
        );
        if let Some(thread) = thread {
            if thread.join().is_err() {
                failures.push(tohru_shell::idle::teardown::Failure {
                    stage: tohru_shell::idle::teardown::Stage::Observer,
                    detail: "observer thread panicked".into(),
                });
            }
        }
        anyhow::ensure!(
            failures.is_empty(),
            "{}",
            failures
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        );
        Ok(())
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

struct Wait {
    proxy: ExtIdleNotificationV1,
    subscription: Subscription,
}

struct Power {
    output: wl_output::WlOutput,
    proxy: ZwlrOutputPowerV1,
}

struct State {
    connection: Connection,
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    manager: ExtIdleNotifierV1,
    power_manager: Option<ZwlrOutputPowerManagerV1>,
    waits: Vec<Wait>,
    powers: Vec<Power>,
    policy: Policy,
    sink: Arc<dyn Sink>,
    requested_off: bool,
    alpha: f32,
    cancellation: Cancellation,
    applied_stamp: u64,
}

fn run(
    policy: Policy,
    commands: Receiver<Config>,
    latest: Arc<Mutex<Input>>,
    stop: Arc<AtomicBool>,
    source: smithay_client_toolkit::reexports::calloop::ping::PingSource,
    sink: Arc<dyn Sink>,
    cancellation: Cancellation,
) -> Result<()> {
    let connection = Connection::connect_to_env().context("connect idle observer to Wayland")?;
    let (globals, queue) =
        registry_queue_init::<State>(&connection).context("enumerate idle protocols")?;
    let qh = queue.handle();
    let manager = globals
        .bind(&qh, 1..=1, ())
        .context("ext_idle_notifier_v1 unavailable")?;
    let power_manager = globals.bind(&qh, 1..=1, ()).ok();
    let mut state = State {
        connection: connection.clone(),
        registry: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        outputs: OutputState::new(&globals, &qh),
        manager,
        power_manager,
        waits: Vec::new(),
        powers: Vec::new(),
        policy,
        sink,
        requested_off: false,
        alpha: -1.0,
        applied_stamp: cancellation.stamp(),
        cancellation,
    };
    let mut event_loop: EventLoop<State> =
        EventLoop::try_new().context("create idle event loop")?;
    WaylandSource::new(connection.clone(), queue)
        .insert(event_loop.handle())
        .map_err(|e| anyhow::anyhow!("install idle Wayland source: {e}"))?;
    event_loop
        .handle()
        .insert_source(source, |_, _, _| {})
        .map_err(|e| anyhow::anyhow!("install idle wakeup: {e}"))?;
    let (effects, stamp) = {
        let mut input = latest
            .lock()
            .map_err(|_| anyhow::anyhow!("idle observation mutex poisoned"))?;
        let effects = input.drain(&mut state.policy, Instant::now());
        (effects, state.cancellation.stamp())
    };
    state.applied_stamp = stamp;
    state.effects(effects, &qh);
    state.sink.status(if state.power_manager.is_some() {
        "idle observer ready"
    } else {
        "idle ready; output power unavailable"
    });
    state.sink.state(state.applied_stamp);
    while !stop.load(Ordering::Acquire) {
        for config in commands.try_iter() {
            let effects = state.policy.configure(config)?;
            state.effects(effects, &qh);
        }
        let (effects, stamp) = {
            let mut input = latest
                .lock()
                .map_err(|_| anyhow::anyhow!("idle observation mutex poisoned"))?;
            let effects = input.drain(&mut state.policy, Instant::now());
            (effects, state.cancellation.stamp())
        };
        state.applied_stamp = stamp;
        state.effects(effects, &qh);
        let now = Instant::now();
        let effects = state.policy.step(now)?;
        state.effects(effects, &qh);
        let alpha = state.policy.opacity(now).unwrap_or(0.0);
        if alpha != state.alpha {
            state.alpha = alpha;
            state.sink.opacity(alpha);
        }
        let deadline = state
            .policy
            .deadline()
            .map(|d| d.saturating_duration_since(now));
        let timeout = if state.policy.fading() {
            Some(
                deadline
                    .unwrap_or(Duration::from_millis(16))
                    .min(Duration::from_millis(16)),
            )
        } else {
            deadline
        };
        event_loop
            .dispatch(timeout, &mut state)
            .context("dispatch idle protocols")?;
    }
    // Restore requested power before the observer disconnects; no GPU handles here.
    state.power(false);
    for wait in state.waits.drain(..) {
        wait.proxy.destroy();
    }
    for power in state.powers.drain(..) {
        power.proxy.destroy();
    }
    state.manager.destroy();
    if let Some(manager) = state.power_manager.take() {
        manager.destroy();
    }
    let _ = connection.flush();
    Ok(())
}

impl State {
    fn effects(&mut self, effects: Vec<Effect>, qh: &QueueHandle<Self>) {
        let arms = effects
            .iter()
            .any(|effect| matches!(effect, Effect::Arm { .. }));
        let valid = if arms {
            self.cancellation
                .advance(self.applied_stamp)
                .map(|stamp| self.applied_stamp = stamp)
                .is_some()
        } else {
            self.cancellation.current(self.applied_stamp)
        };
        if !valid {
            // Input/resume may race the prepared decision. Restoration remains
            // safe; stale requests and subscriptions cannot become new work.
            for effect in effects {
                match effect {
                    Effect::ScreensOn => self.power(false),
                    Effect::ClearFade => {
                        self.alpha = 0.0;
                        self.sink.opacity(0.0);
                    }
                    _ => {}
                }
            }
            return;
        }
        if arms {
            for wait in self.waits.drain(..) {
                wait.proxy.destroy();
            }
        }
        for effect in effects {
            match effect {
                Effect::Arm {
                    subscription,
                    after,
                } => {
                    // An arm batch always replaces the complete bounded wait set.
                    if self.waits.iter().any(|w| w.subscription == subscription) {
                        continue;
                    }
                    if let Some(after) = after {
                        if let Some(seat) = self.seats.seats().next() {
                            let proxy = self.manager.get_idle_notification(
                                after.as_millis() as u32,
                                &seat,
                                qh,
                                subscription,
                            );
                            self.waits.push(Wait {
                                proxy,
                                subscription,
                            });
                        }
                    }
                }
                Effect::Fade { .. } => {}
                Effect::ClearFade => {
                    self.alpha = 0.0;
                    self.sink.opacity(0.0);
                }
                Effect::RequestLock => self.sink.lock(self.applied_stamp),
                Effect::RequestSuspend => self.sink.suspend(self.applied_stamp),
                Effect::ScreensOff => self.power(true),
                Effect::ScreensOn => self.power(false),
                Effect::LockTimedOut => self
                    .sink
                    .status("idle lock acknowledgement timed out; sleep cancelled"),
            }
        }
    }

    fn rearm(&mut self, qh: &QueueHandle<Self>) {
        let effects = self.policy.resumed();
        self.effects(effects, qh);
    }

    fn power(&mut self, off: bool) {
        self.requested_off = off;
        if self.power_manager.is_none() && off {
            self.sink
                .status("screen off unavailable: compositor has no output-power protocol");
        }
        for power in &self.powers {
            power.proxy.set_mode(if off {
                zwlr_output_power_v1::Mode::Off
            } else {
                zwlr_output_power_v1::Mode::On
            });
        }
    }
}

impl Dispatch<ExtIdleNotifierV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ExtIdleNotifierV1,
        _: <ExtIdleNotifierV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtIdleNotificationV1, Subscription> for State {
    fn event(
        state: &mut Self,
        proxy: &ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event,
        subscription: &Subscription,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if !state.cancellation.current(state.applied_stamp) {
            return;
        }
        if !state
            .waits
            .iter()
            .any(|wait| wait.proxy == *proxy && wait.subscription == *subscription)
        {
            return;
        }
        match event {
            ext_idle_notification_v1::Event::Idled => {
                match state.policy.idled(*subscription, Instant::now()) {
                    Ok(effects) => state.effects(effects, qh),
                    Err(error) => state.sink.status(&error.to_string()),
                }
            }
            ext_idle_notification_v1::Event::Resumed => {
                let effects = state.policy.resumed();
                state.effects(effects, qh);
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrOutputPowerManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwlrOutputPowerManagerV1,
        _: <ZwlrOutputPowerManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrOutputPowerV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ZwlrOutputPowerV1,
        event: zwlr_output_power_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, zwlr_output_power_v1::Event::Failed) {
            state.sink.status("compositor refused output power request");
        }
    }
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        if let Some(manager) = &self.power_manager {
            let proxy = manager.get_output_power(&output, qh, ());
            proxy.set_mode(if self.requested_off {
                zwlr_output_power_v1::Mode::Off
            } else {
                zwlr_output_power_v1::Mode::On
            });
            self.powers.push(Power { output, proxy });
        }
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.powers.retain(|power| {
            if power.output == output {
                power.proxy.destroy();
                false
            } else {
                true
            }
        });
    }
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, qh: &QueueHandle<Self>, _: wl_seat::WlSeat) {
        self.rearm(qh);
    }
    fn remove_seat(&mut self, _: &Connection, qh: &QueueHandle<Self>, _: wl_seat::WlSeat) {
        self.rearm(qh);
    }
    fn new_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers!(OutputState, SeatState);
}
delegate_output!(State);
delegate_seat!(State);
delegate_registry!(State);

impl Drop for State {
    fn drop(&mut self) {
        self.power(false);
        for wait in self.waits.drain(..) {
            wait.proxy.destroy();
        }
        for power in self.powers.drain(..) {
            power.proxy.destroy();
        }
        let _ = self.connection.flush();
    }
}
