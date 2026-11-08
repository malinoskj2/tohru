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
use tohru_shell::idle::{Config, Effect, Inhibitors, Policy, Subscription};
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

#[derive(Clone, Copy, Default)]
pub(super) struct Observations {
    pub held: Option<bool>,
    pub inhibitors: Inhibitors,
    pub resume: u64,
}

pub(super) struct Observer {
    commands: SyncSender<Config>,
    latest: Arc<Mutex<Observations>>,
    ping: Ping,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

/// Effects leave the protocol thread through bounded/nonblocking native channels.
/// The receiver cancels queued action requests whenever `rearmed` is called.
pub(super) trait Sink: Send + Sync {
    fn opacity(&self, alpha: f32);
    fn status(&self, status: &str);
    fn rearmed(&self);
    fn lock(&self);
    fn suspend(&self);
    fn state(&self);
}

impl Observer {
    pub(super) fn spawn(config: Config, sink: Arc<dyn Sink>) -> Result<Self> {
        let policy = Policy::new(config, false).context("validate idle configuration")?;
        let (commands, receiver) = mpsc::sync_channel(8);
        let (ping, source) = make_ping().context("create idle observer wakeup")?;
        let latest = Arc::new(Mutex::new(Observations::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (thread_latest, thread_stop) = (latest.clone(), stop.clone());
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
            latest,
            ping,
            stop,
            thread: Some(thread),
        })
    }

    pub(super) fn configure(&self, config: Config) -> Result<()> {
        // Validate before enqueue; process policy remains the sole generation owner.
        Policy::new(config, false).context("validate idle configuration")?;
        self.commands
            .try_send(config)
            .context("idle configuration queue unavailable")?;
        self.ping.ping();
        Ok(())
    }

    pub(super) fn observe(&self, change: impl FnOnce(&mut Observations)) -> Result<()> {
        let mut latest = self
            .latest
            .lock()
            .map_err(|_| anyhow::anyhow!("idle observation mutex poisoned"))?;
        change(&mut latest);
        drop(latest);
        self.ping.ping();
        Ok(())
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.ping.ping();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
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
    applied: Observations,
    requested_off: bool,
    alpha: f32,
}

fn run(
    policy: Policy,
    commands: Receiver<Config>,
    latest: Arc<Mutex<Observations>>,
    stop: Arc<AtomicBool>,
    source: smithay_client_toolkit::reexports::calloop::ping::PingSource,
    sink: Arc<dyn Sink>,
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
        applied: Observations::default(),
        requested_off: false,
        alpha: -1.0,
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
    let effects = state.policy.inhibited(Inhibitors {
        logind_idle: true,
        ..Inhibitors::default()
    });
    state.effects(effects, &qh);
    state.sink.status(if state.power_manager.is_some() {
        "idle observer ready"
    } else {
        "idle ready; output power unavailable"
    });
    state.sink.state();
    while !stop.load(Ordering::Acquire) {
        for config in commands.try_iter() {
            let effects = state.policy.configure(config)?;
            state.effects(effects, &qh);
        }
        let observed = *latest
            .lock()
            .map_err(|_| anyhow::anyhow!("idle observation mutex poisoned"))?;
        if observed.resume != state.applied.resume {
            let effects = state.policy.resumed();
            state.effects(effects, &qh);
        }
        let mut inhibitors = observed.inhibitors;
        inhibitors.logind_idle |= observed.held.is_none();
        let effects = state.policy.inhibited(inhibitors);
        state.effects(effects, &qh);
        let effects = state
            .policy
            .session(observed.held.unwrap_or(false), Instant::now());
        state.effects(effects, &qh);
        state.applied = observed;
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
        if effects.iter().any(|e| matches!(e, Effect::Arm { .. })) {
            self.sink.rearmed();
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
                Effect::RequestLock => self.sink.lock(),
                Effect::RequestSuspend => self.sink.suspend(),
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
