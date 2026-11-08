//! Authenticated logind owner and query provenance, independent of bus transport.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Query {
    pub owner: String,
    revision: u64,
    serial: u64,
}

#[derive(Default)]
pub struct Owner {
    name: Option<String>,
    revision: u64,
    serial: u64,
}

impl Owner {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// A delayed initial `GetNameOwner` result cannot overwrite an observed loss
    /// or replacement. Until a successful current-owner property query, inhibit.
    pub fn initialize(&mut self, revision: u64, owner: Option<&str>) {
        if self.revision == revision {
            self.changed(owner.unwrap_or(""));
        }
    }

    pub fn changed(&mut self, owner: &str) {
        self.revision = self.revision.wrapping_add(1);
        self.name = if owner.is_empty() {
            None
        } else {
            Some(owner.to_owned())
        };
    }

    pub fn query(&mut self) -> Option<Query> {
        self.serial = self.serial.wrapping_add(1);
        Some(Query {
            owner: self.name.clone()?,
            revision: self.revision,
            serial: self.serial,
        })
    }

    /// None means stale: caller leaves the newer observation untouched. A failed
    /// current query returns inhibited=true, never an invented available state.
    #[must_use]
    pub fn complete(&self, query: &Query, blocked: Option<bool>) -> Option<bool> {
        if self.revision != query.revision
            || self.serial != query.serial
            || self.name.as_ref() != Some(&query.owner)
        {
            return None;
        }
        Some(blocked.unwrap_or(true))
    }
}

#[must_use]
pub fn owner_signal(sender: &str, interface: &str, member: &str, path: &str, name: &str) -> bool {
    sender == "org.freedesktop.DBus"
        && interface == "org.freedesktop.DBus"
        && member == "NameOwnerChanged"
        && path == "/org/freedesktop/DBus"
        && name == "org.freedesktop.login1"
}

#[cfg(test)]
#[path = "../../tests/session/idle_logind.rs"]
mod tests;
