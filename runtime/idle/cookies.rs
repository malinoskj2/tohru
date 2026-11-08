//! Bounded standard `ScreenSaver` inhibition, owned by each unique bus client.
const PER_CLIENT: usize = 32;
const TOTAL: usize = 256;

struct Cookie {
    id: u32,
    owner: String,
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum Refused {
    #[error("this client already holds {PER_CLIENT} idle inhibitors")]
    ClientLimit,
    #[error("the shell already holds {TOTAL} idle inhibitors")]
    TotalLimit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Release {
    Released,
    Unknown,
    DifferentOwner,
}

/// No application data or reason text is retained; the bus authenticates owner.
pub struct Cookies {
    next: u32,
    held: Vec<Cookie>,
}

impl Default for Cookies {
    fn default() -> Self {
        Self {
            next: 1337,
            held: Vec::new(),
        }
    }
}

impl Cookies {
    /// # Errors
    /// Rejects either client or global capacity before allocating a cookie.
    pub fn inhibit(&mut self, owner: &str) -> Result<u32, Refused> {
        if self.held.len() >= TOTAL {
            return Err(Refused::TotalLimit);
        }
        if self
            .held
            .iter()
            .filter(|cookie| cookie.owner == owner)
            .count()
            >= PER_CLIENT
        {
            return Err(Refused::ClientLimit);
        }
        // At most TOTAL occupied IDs and the reserved zero can be skipped.
        for _ in 0..=TOTAL + 1 {
            let id = self.next;
            self.next = self.next.wrapping_add(1);
            if id != 0 && !self.held.iter().any(|cookie| cookie.id == id) {
                self.held.push(Cookie {
                    id,
                    owner: owner.to_owned(),
                });
                return Ok(id);
            }
        }
        Err(Refused::TotalLimit)
    }

    pub fn release(&mut self, owner: &str, id: u32) -> Release {
        let Some(index) = self.held.iter().position(|cookie| cookie.id == id) else {
            return Release::Unknown;
        };
        if self.held[index].owner != owner {
            return Release::DifferentOwner;
        }
        self.held.swap_remove(index);
        Release::Released
    }

    pub fn client_gone(&mut self, owner: &str) {
        self.held.retain(|cookie| cookie.owner != owner);
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.held.len()
    }
}

#[cfg(test)]
#[path = "../../tests/session/idle_cookies.rs"]
mod tests;
