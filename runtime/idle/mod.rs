//! Integrated idle policy. Protocol observers own their handles and feed this
//! process-owned machine; native Pleamar scenes own fades and authentication.
//! This module never connects to a display/bus or executes system actions.

use std::time::{Duration, Instant};

const LOCK_ACK_LIMIT: Duration = Duration::from_secs(5);
const MAX_FADE: Duration = Duration::from_secs(120);

/// The three retained automatic desktop actions, in execution order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Lock,
    ScreenOff,
    Suspend,
}

impl Action {
    const ALL: [Self; 3] = [Self::Lock, Self::ScreenOff, Self::Suspend];

    const fn index(self) -> usize {
        match self {
            Self::Lock => 0,
            Self::ScreenOff => 1,
            Self::Suspend => 2,
        }
    }
}

/// A missing unlocked timeout disables an action; a missing locked timeout
/// inherits its unlocked timeout. Durations are bounded to the standard
/// ext-idle-notify unsigned 32-bit millisecond range at construction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Timeouts {
    pub unlocked: Option<Duration>,
    pub locked: Option<Duration>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Config {
    pub lock: Timeouts,
    pub screen_off: Timeouts,
    pub suspend: Timeouts,
    pub fade: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            lock: Timeouts {
                unlocked: Some(Duration::from_secs(1800)),
                locked: None,
            },
            screen_off: Timeouts {
                unlocked: None,
                locked: Some(Duration::from_secs(60)),
            },
            suspend: Timeouts {
                unlocked: Some(Duration::from_secs(7200)),
                locked: Some(Duration::from_secs(5400)),
            },
            fade: Duration::from_secs(2),
        }
    }
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum Error {
    #[error(
        "idle timeout must be whole milliseconds between 1 and 4294967295; use None to disable"
    )]
    InvalidTimeout,
    #[error("idle fade must be whole milliseconds between 0 and 120000")]
    InvalidFade,
    #[error("idle deadline exceeds the monotonic clock range")]
    DeadlineOverflow,
}

/// A native callback must carry its subscription generation. Callbacks queued
/// before rearm, inhibition, resume or lock transitions cannot fire a new action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Subscription {
    action: Action,
    generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Effect {
    Arm {
        subscription: Subscription,
        after: Option<Duration>,
    },
    Fade {
        over: Duration,
    },
    ClearFade,
    RequestLock,
    /// Only emitted after the dedicated locker reports native lock.held.
    RequestSuspend,
    ScreensOff,
    ScreensOn,
    LockTimedOut,
}

/// Independent inhibitor observations; losing one must not clear another.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Inhibitors {
    pub screen_saver_cookies: u32,
    pub logind_idle: bool,
    pub caffeine: bool,
}

impl Inhibitors {
    const fn active(self) -> bool {
        self.screen_saver_cookies > 0 || self.logind_idle || self.caffeine
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Waiting,
    Fading,
    Fired,
}

#[derive(Clone, Copy)]
struct AwaitingLock {
    until: Instant,
    suspend: bool,
}

struct Fade {
    until: Instant,
    joined: [bool; 3],
}

/// One owner per ordinary shell process. A locker remains a separate process.
/// The caller executes returned effects in order, schedules only `deadline()`,
/// and disposes every replaced native notification on `Arm`.
pub struct Policy {
    config: Config,
    generation: u64,
    phases: [Phase; 3],
    locked: bool,
    inhibitors: Inhibitors,
    fading: Option<Fade>,
    veiled: bool,
    screens_off: bool,
    awaiting_lock: Option<AwaitingLock>,
}

impl Policy {
    /// # Errors
    /// Rejects invalid protocol timeout or fade durations.
    pub fn new(config: Config, locked: bool) -> Result<Self, Error> {
        for timeouts in [config.lock, config.screen_off, config.suspend] {
            for duration in [timeouts.unlocked, timeouts.locked].into_iter().flatten() {
                if duration.is_zero()
                    || duration > Duration::from_millis(u64::from(u32::MAX))
                    || duration.subsec_nanos() % 1_000_000 != 0
                {
                    return Err(Error::InvalidTimeout);
                }
            }
        }
        if config.fade > MAX_FADE || config.fade.subsec_nanos() % 1_000_000 != 0 {
            return Err(Error::InvalidFade);
        }
        Ok(Self {
            config,
            generation: 0,
            phases: [Phase::Waiting; 3],
            locked,
            inhibitors: Inhibitors::default(),
            fading: None,
            veiled: false,
            screens_off: false,
            awaiting_lock: None,
        })
    }

    fn timeout(&self, action: Action) -> Option<Duration> {
        let timeouts = match action {
            Action::Lock => self.config.lock,
            Action::ScreenOff => self.config.screen_off,
            Action::Suspend => self.config.suspend,
        };
        if self.locked {
            timeouts.locked.or(timeouts.unlocked)
        } else {
            timeouts.unlocked
        }
    }

    fn clear_fade(&mut self, effects: &mut Vec<Effect>) {
        self.fading = None;
        if self.veiled {
            self.veiled = false;
            effects.push(Effect::ClearFade);
        }
    }

    fn cancel_pending(&mut self, effects: &mut Vec<Effect>) {
        self.clear_fade(effects);
        self.awaiting_lock = None;
    }

    /// Begin/restart protocol waits, retaining whether screens need restoring.
    pub fn arm(&mut self) -> Vec<Effect> {
        let mut effects = Vec::with_capacity(5);
        self.cancel_pending(&mut effects);
        // Wrapping cannot collide with a queued subscription within any
        // realistic process lifetime; no unbounded subscription state is kept.
        self.generation = self.generation.wrapping_add(1);
        self.phases = [Phase::Waiting; 3];
        for action in Action::ALL {
            effects.push(Effect::Arm {
                subscription: Subscription {
                    action,
                    generation: self.generation,
                },
                after: if self.inhibitors.active() {
                    None
                } else {
                    self.timeout(action)
                },
            });
        }
        effects
    }

    /// Native idle notification, not a wall-clock polling timer.
    ///
    /// # Errors
    /// Returns an error if a monotonic fade/lock deadline cannot be represented.
    pub fn idled(
        &mut self,
        subscription: Subscription,
        now: Instant,
    ) -> Result<Vec<Effect>, Error> {
        let index = subscription.action.index();
        if subscription.generation != self.generation
            || self.phases[index] != Phase::Waiting
            || self.inhibitors.active()
            || self.timeout(subscription.action).is_none()
        {
            return Ok(Vec::new());
        }
        if self.config.fade.is_zero() {
            let mut effects = Vec::with_capacity(3);
            self.fire(
                [
                    subscription.action == Action::Lock,
                    subscription.action == Action::ScreenOff,
                    subscription.action == Action::Suspend,
                ],
                now,
                &mut effects,
            )?;
            return Ok(effects);
        }
        if let Some(fade) = &mut self.fading {
            fade.joined[index] = true;
            self.phases[index] = Phase::Fading;
            return Ok(Vec::new());
        }
        let until = now
            .checked_add(self.config.fade)
            .ok_or(Error::DeadlineOverflow)?;
        let mut joined = [false; 3];
        joined[index] = true;
        self.fading = Some(Fade { until, joined });
        self.phases[index] = Phase::Fading;
        if self.locked {
            Ok(Vec::new())
        } else {
            self.veiled = true;
            Ok(vec![Effect::Fade {
                over: self.config.fade,
            }])
        }
    }

    fn fire(
        &mut self,
        joined: [bool; 3],
        now: Instant,
        effects: &mut Vec<Effect>,
    ) -> Result<(), Error> {
        let needs_lock = !self.locked && (joined[0] || joined[2]);
        let lock_deadline = if needs_lock {
            Some(
                now.checked_add(LOCK_ACK_LIMIT)
                    .ok_or(Error::DeadlineOverflow)?,
            )
        } else {
            None
        };
        self.fading = None;
        if !needs_lock {
            self.clear_fade(effects);
        }
        for action in Action::ALL {
            if joined[action.index()] {
                self.phases[action.index()] = Phase::Fired;
            }
        }
        if needs_lock {
            if let Some(waiting) = &mut self.awaiting_lock {
                waiting.suspend |= joined[2];
            } else if let Some(until) = lock_deadline {
                self.awaiting_lock = Some(AwaitingLock {
                    until,
                    suspend: joined[2],
                });
                effects.push(Effect::RequestLock);
            }
        }
        if joined[1] && !self.screens_off {
            self.screens_off = true;
            effects.push(Effect::ScreensOff);
        }
        if joined[2] && self.locked {
            effects.push(Effect::RequestSuspend);
        }
        Ok(())
    }

    /// One-shot deadline for fade completion or missing locker acknowledgment.
    #[must_use]
    pub fn deadline(&self) -> Option<Instant> {
        [
            self.fading.as_ref().map(|fade| fade.until),
            self.awaiting_lock.map(|waiting| waiting.until),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// # Errors
    /// Returns an error if a monotonic lock deadline cannot be represented.
    pub fn step(&mut self, now: Instant) -> Result<Vec<Effect>, Error> {
        let mut effects = Vec::with_capacity(5);
        if let Some(fade) = &self.fading {
            if now >= fade.until {
                let joined = fade.joined;
                self.fire(joined, now, &mut effects)?;
            }
        }
        if self
            .awaiting_lock
            .is_some_and(|waiting| now >= waiting.until)
        {
            effects.extend(self.arm());
            effects.push(Effect::LockTimedOut);
        }
        Ok(effects)
    }

    /// Feed actual compositor acknowledgment, never native IPC acceptance.
    pub fn session(&mut self, locked: bool) -> Vec<Effect> {
        if locked == self.locked {
            return Vec::new();
        }
        let suspend = locked && self.awaiting_lock.is_some_and(|waiting| waiting.suspend);
        self.locked = locked;
        let mut effects = self.arm();
        if suspend {
            effects.push(Effect::RequestSuspend);
        }
        effects
    }

    pub fn lock_failed(&mut self) -> Vec<Effect> {
        if self.awaiting_lock.is_none() {
            return Vec::new();
        }
        self.arm()
    }

    /// Input or system resume restores screens, clears fades/intent and starts
    /// locked/unlocked timeouts from now without ever changing lock state.
    pub fn resumed(&mut self) -> Vec<Effect> {
        let mut effects = self.arm();
        if self.screens_off {
            self.screens_off = false;
            effects.insert(0, Effect::ScreensOn);
        }
        effects
    }

    /// Any transition into inhibition cancels an in-flight fade or sleep intent.
    /// On final release timers start over, rather than immediately firing.
    pub fn inhibited(&mut self, inhibitors: Inhibitors) -> Vec<Effect> {
        let was_active = self.inhibitors.active();
        self.inhibitors = inhibitors;
        if was_active == inhibitors.active() {
            return Vec::new();
        }
        self.resumed()
    }
}

#[cfg(test)]
#[path = "../../tests/session/idle.rs"]
mod tests;
