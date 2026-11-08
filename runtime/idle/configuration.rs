//! Cache only configurations accepted by the existing process-owned observer.
use super::Config;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Change {
    Unchanged,
    Accepted,
}

pub struct Configuration {
    accepted: Config,
}

impl Configuration {
    #[must_use]
    pub const fn new(accepted: Config) -> Self {
        Self { accepted }
    }

    /// # Errors
    /// A failed submission leaves the previous accepted value unchanged.
    pub fn update<E>(
        &mut self,
        config: Config,
        submit: impl FnOnce(Config) -> Result<(), E>,
    ) -> Result<Change, E> {
        if config == self.accepted {
            return Ok(Change::Unchanged);
        }
        submit(config)?;
        self.accepted = config;
        Ok(Change::Accepted)
    }
}

#[cfg(test)]
#[path = "../../tests/session/idle_configuration.rs"]
mod tests;
