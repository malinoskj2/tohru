//! Nonexported native lock-cycle authority; no scene data can construct a permit.
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
};

const REQUESTED: u8 = 0;
const HELD: u8 = 1;
const FINISHED: u8 = 2;
const RELEASING: u8 = 3;

#[derive(Clone)]
pub(crate) struct Cycle {
    phase: Arc<AtomicU8>,
    owner: usize,
}

/// Per-output liveness is independent of the session cycle: an output may
/// retire while the same lock stays held on the remaining monitors.
#[derive(Clone)]
pub(crate) struct Face {
    alive: Arc<AtomicBool>,
    cycle: Cycle,
}

impl Face {
    pub(crate) fn new(cycle: Cycle) -> Self {
        Self {
            alive: Arc::new(AtomicBool::new(true)),
            cycle,
        }
    }

    pub(crate) fn retire(&self) {
        self.alive.store(false, Ordering::Release);
    }

    pub(crate) fn current(&self) -> bool {
        self.alive.load(Ordering::Acquire) && self.cycle.publishable()
    }
}

pub(crate) struct Snapshot {
    phase: Arc<AtomicU8>,
    owner: usize,
}

/// Opaque and intentionally nonclone. Sent internally from native PAM to the
/// renderer; it is never a scene fact/text/event, IPC value or Lua object.
pub struct Permit {
    phase: Arc<AtomicU8>,
    owner: usize,
}

pub(crate) struct Release {
    phase: Arc<AtomicU8>,
    owner: usize,
}

impl Cycle {
    pub(crate) fn requested(owner: usize) -> Self {
        Self {
            phase: Arc::new(AtomicU8::new(REQUESTED)),
            owner,
        }
    }

    pub(crate) fn confirm(&self) {
        let _ = self
            .phase
            .compare_exchange(REQUESTED, HELD, Ordering::AcqRel, Ordering::Acquire);
    }

    pub(crate) fn finish(&self) {
        self.phase.store(FINISHED, Ordering::Release);
    }

    pub(crate) fn active(&self) -> bool {
        self.phase.load(Ordering::Acquire) != FINISHED
    }

    pub(crate) fn held(&self) -> bool {
        self.phase.load(Ordering::Acquire) == HELD
    }

    pub(crate) fn publishable(&self) -> bool {
        matches!(self.phase.load(Ordering::Acquire), REQUESTED | HELD)
    }

    /// Capture before blocking PAM. A requested but unconfirmed lock is not
    /// sufficient; success begun before acquisition cannot release that cycle.
    pub(crate) fn snapshot(&self) -> Option<Snapshot> {
        self.held().then(|| Snapshot {
            phase: self.phase.clone(),
            owner: self.owner,
        })
    }

    /// Reserve once before dropping GPU resources. New authentication cannot
    /// mint while releasing; callbacks may finish but cannot rebind this cycle.
    pub(crate) fn reserve(&self, permit: Permit) -> Option<Release> {
        if self.owner != permit.owner || !Arc::ptr_eq(&self.phase, &permit.phase) {
            return None;
        }
        self.phase
            .compare_exchange(HELD, RELEASING, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        Some(Release {
            phase: permit.phase,
            owner: permit.owner,
        })
    }

    pub(crate) fn accepts_release(&self, release: &Release) -> bool {
        self.phase.load(Ordering::Acquire) == RELEASING
            && self.owner == release.owner
            && Arc::ptr_eq(&self.phase, &release.phase)
    }
}

impl Snapshot {
    /// The native adapter calls this only with the result of PAM authentication
    /// AND account validation. Ordinary auth.check has no path to this method.
    pub(crate) fn verified(self, accepted: bool) -> Option<Permit> {
        (accepted && self.phase.load(Ordering::Acquire) == HELD).then_some(Permit {
            phase: self.phase,
            owner: self.owner,
        })
    }
}

#[cfg(test)]
#[path = "../../tests/session/lock_authority.rs"]
mod tests;
