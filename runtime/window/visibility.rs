//! Ordinary-window presentation intent, independent of settings and lock roles.
#[derive(Default)]
pub(crate) struct Intent {
    sent: Option<bool>,
    dismissed: bool,
}

impl Intent {
    pub(crate) fn update(&mut self, requested: bool) -> Option<bool> {
        if !requested {
            self.dismissed = false;
        }
        let wanted = requested && !self.dismissed;
        if self.sent == Some(wanted) {
            return None;
        }
        self.sent = Some(wanted);
        Some(wanted)
    }

    pub(crate) fn closed(&mut self) {
        self.dismissed = true;
        self.sent = Some(false);
    }
}

/// Serializes ordinary native commits with retirement and waits for frame workers.
pub(crate) struct Retirement(std::sync::Mutex<RetirementState>);

struct RetirementState {
    current: bool,
    gpu_released: bool,
    workers: usize,
}

impl Default for Retirement {
    fn default() -> Self {
        Self(std::sync::Mutex::new(RetirementState {
            current: true,
            gpu_released: false,
            workers: 0,
        }))
    }
}

impl Retirement {
    pub(crate) fn current(&self) -> bool {
        self.0.lock().is_ok_and(|state| state.current)
    }

    pub(crate) fn if_current(&self, action: impl FnOnce()) {
        if let Ok(state) = self.0.lock() {
            if state.current {
                // Retirement cannot pass this commit; GPU waits happen outside.
                action();
            }
        }
    }

    pub(crate) fn retire(&self) {
        if let Ok(mut state) = self.0.lock() {
            state.current = false;
        }
    }

    pub(crate) fn start_worker(&self) -> bool {
        let Ok(mut state) = self.0.lock() else {
            return false;
        };
        let Some(count) = state.workers.checked_add(1) else {
            return false;
        };
        state.workers = count;
        true
    }

    pub(crate) fn finish_worker(&self) -> bool {
        let Ok(mut state) = self.0.lock() else {
            return false;
        };
        let Some(count) = state.workers.checked_sub(1) else {
            return false;
        };
        state.workers = count;
        !state.current && count == 0
    }

    pub(crate) fn gpu_released(&self) {
        if let Ok(mut state) = self.0.lock() {
            state.gpu_released = true;
        }
    }

    pub(crate) fn ready(&self) -> bool {
        self.0
            .lock()
            .is_ok_and(|state| !state.current && state.gpu_released && state.workers == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::Intent;

    #[test]
    fn initially_closed_stays_unmapped_until_explicit_open() {
        let mut intent = Intent::default();
        assert_eq!(intent.update(false), Some(false));
        assert_eq!(intent.update(false), None);
        assert_eq!(intent.update(true), Some(true));
        assert_eq!(intent.update(true), None);
    }

    #[test]
    fn hide_and_reopen_each_change_native_presentation_once() {
        let mut intent = Intent::default();
        assert_eq!(intent.update(true), Some(true));
        assert_eq!(intent.update(false), Some(false));
        assert_eq!(intent.update(false), None);
        assert_eq!(intent.update(true), Some(true));
    }

    #[test]
    fn wm_close_does_not_immediately_reopen_a_true_compound_condition() {
        let mut intent = Intent::default();
        assert_eq!(intent.update(true), Some(true));
        intent.closed();
        assert_eq!(intent.update(true), None);
        assert_eq!(intent.update(true), None);
        assert_eq!(intent.update(false), None);
        assert_eq!(intent.update(true), Some(true));
    }

    #[test]
    fn gpu_ack_does_not_allow_reopen_until_delayed_worker_drains() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
            mpsc,
        };
        let life = Arc::new(super::Retirement::default());
        let commits = Arc::new(AtomicUsize::new(0));
        assert!(life.current() && life.start_worker());
        let (resume, delayed) = mpsc::channel();
        let worker_life = life.clone();
        let worker_commits = commits.clone();
        let worker = std::thread::spawn(move || {
            delayed.recv().unwrap(); // queued frame blocked outside native gate
            worker_life.if_current(|| {
                worker_commits.fetch_add(1, Ordering::SeqCst);
            });
            worker_life.finish_worker()
        });
        life.retire();
        life.gpu_released();
        assert!(!life.current() && !life.ready());
        resume.send(()).unwrap();
        assert!(worker.join().unwrap());
        assert_eq!(commits.load(Ordering::SeqCst), 0);
        assert!(life.ready());
    }

    #[test]
    fn worker_drain_before_gpu_ack_still_waits_and_retirement_is_idempotent() {
        let life = super::Retirement::default();
        assert!(life.start_worker());
        let mut commits = 0;
        life.if_current(|| commits += 1);
        life.retire();
        life.retire();
        life.if_current(|| commits += 1);
        assert_eq!(commits, 1);
        assert!(life.finish_worker());
        assert!(!life.ready());
        life.gpu_released();
        assert!(life.ready());
        assert!(!life.finish_worker());
    }
}
