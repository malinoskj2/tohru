//! Bounded observation mailbox: latest values plus a monotonic cancellation edge.
//! Even an inhibitor pulse completed before the next drain invalidates old work.
use super::{Effect, Inhibitors, Policy};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Instant;

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicU64>);

impl Cancellation {
    #[must_use]
    pub fn stamp(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }

    pub fn invalidate(&self) {
        self.0.fetch_add(1, Ordering::AcqRel);
    }

    /// Reserve the next decision epoch only if no producer has invalidated it.
    #[must_use]
    pub fn advance(&self, stamp: u64) -> Option<u64> {
        let next = stamp.wrapping_add(1);
        self.0
            .compare_exchange(stamp, next, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| next)
    }

    #[must_use]
    pub fn current(&self, stamp: u64) -> bool {
        self.stamp() == stamp
    }
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub struct Observation {
    pub held: Option<bool>,
    pub inhibitors: Inhibitors,
    pub resume: u64,
}

pub struct Input {
    latest: Observation,
    epoch: u64,
    applied: u64,
    cancellation: Cancellation,
    initialized: bool,
}

impl Input {
    #[must_use]
    pub fn new(cancellation: Cancellation) -> Self {
        Self {
            latest: Observation::default(),
            epoch: 0,
            applied: 0,
            cancellation,
            initialized: false,
        }
    }

    /// Called under the adapter's mailbox mutex. Cancellation is visible to
    /// the action dispatcher immediately, before the observer drains values.
    pub fn update(&mut self, change: impl FnOnce(&mut Observation)) {
        let before = self.latest;
        change(&mut self.latest);
        if self.latest != before {
            self.cancellation.invalidate();
        }
        if self.latest.inhibitors != before.inhibitors || self.latest.resume != before.resume {
            self.epoch = self.epoch.wrapping_add(1);
        }
    }

    pub fn drain(&mut self, policy: &mut Policy, now: Instant) -> Vec<Effect> {
        let mut effects = Vec::new();
        let mut inhibitors = self.latest.inhibitors;
        inhibitors.logind_idle |= self.latest.held.is_none();
        append(&mut effects, policy.inhibited(inhibitors));
        if self.epoch != self.applied {
            append(&mut effects, policy.resumed());
        }
        append(
            &mut effects,
            policy.session(self.latest.held.unwrap_or(false), now),
        );
        if !self.initialized
            && !effects
                .iter()
                .any(|effect| matches!(effect, Effect::Arm { .. }))
        {
            append(&mut effects, policy.arm());
        }
        self.initialized = true;
        self.applied = self.epoch;
        effects
    }
}

// Every policy rearm replaces all three subscriptions. Keep only the final
// batch when one mailbox drain combines several state changes.
fn append(effects: &mut Vec<Effect>, next: Vec<Effect>) {
    if next
        .iter()
        .any(|effect| matches!(effect, Effect::Arm { .. }))
    {
        effects.retain(|effect| !matches!(effect, Effect::Arm { .. }));
    }
    effects.extend(next);
}

#[cfg(test)]
#[path = "../../tests/session/idle_input.rs"]
mod tests;
