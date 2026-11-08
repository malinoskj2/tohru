//! Locker-local logind delay ownership; ordinary shell hints cannot release it.
//! The native adapter calls `acknowledge` only inside confirmed held authority.
pub struct Delay<T> {
    inhibitor: Option<T>,
    sleeping: bool,
}

impl<T> Delay<T> {
    #[must_use]
    pub const fn new(inhibitor: T) -> Self {
        Self {
            inhibitor: Some(inhibitor),
            sleeping: false,
        }
    }

    /// PrepareForSleep(true) alone never drops the owned descriptor.
    pub fn prepare(&mut self) {
        self.sleeping = true;
    }

    /// The caller must hold the native confirmed-lock transaction while this
    /// method closes the FD. Writable facts and remote hints are not authority.
    pub fn acknowledge(&mut self) {
        if self.sleeping {
            self.inhibitor.take();
        }
    }

    /// Install the freshly acquired delay before advertising resume readiness.
    pub fn resumed(&mut self, inhibitor: T) {
        self.sleeping = false;
        self.inhibitor = Some(inhibitor);
    }

    #[must_use]
    pub const fn protected(&self) -> bool {
        self.inhibitor.is_some()
    }
}

#[cfg(test)]
#[path = "../../tests/session/idle_delay.rs"]
mod tests;
