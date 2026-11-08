//! Video-only assignment and conservative playback policy, independent of graphics/codecs.
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

pub const MAX_OUTPUTS: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Protection {
    #[default]
    Unknown,
    Clear,
    Protected,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Occupancy {
    #[default]
    Unknown,
    Empty,
    Occupied,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub revision: u64,
    pub enabled: bool,
    pub assignments: BTreeMap<String, String>,
    pub logind: Protection,
    /// Actual dedicated lock-role protected status, never another role's local lock.held.
    pub locker: Protection,
    pub occupancy: BTreeMap<String, Occupancy>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("video snapshot exceeds32 assignments or observations")]
    Capacity,
    #[error("video output requires one literal native output name")]
    Output,
    #[error("video wallpaper requires an absolute local supported media path of at most4096 bytes")]
    Path,
    #[error("video snapshot revision must increase")]
    Revision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseReason {
    Protected,
    UnknownProtection,
    Occupied,
    UnknownOccupancy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Playback {
    Paused(PauseReason),
    Playing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action<'a> {
    Stop,
    Keep { path: &'a str, playback: Playback },
}

impl Snapshot {
    /// Validate the complete proposal before replacing any owner state.
    ///
    /// # Errors
    /// Rejects overcapacity, selector aliases, invalid paths and unsupported video types.
    pub fn validate(&self) -> Result<(), Error> {
        if self.assignments.len() > MAX_OUTPUTS || self.occupancy.len() > MAX_OUTPUTS {
            return Err(Error::Capacity);
        }
        for name in self.assignments.keys().chain(self.occupancy.keys()) {
            if name.is_empty()
                || name.len() > 255
                || name.trim() != name
                || name.contains([',', '\r', '\n', '\0'])
            {
                return Err(Error::Output);
            }
        }
        for path in self.assignments.values() {
            let supported = std::path::Path::new(path)
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| {
                    matches!(
                        s.to_ascii_lowercase().as_str(),
                        "mp4" | "webm" | "mkv" | "mov" | "gif"
                    )
                });
            if !path.starts_with('/') || path.len() > 4096 || path.contains('\0') || !supported {
                return Err(Error::Path);
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn action(&self, output: &str, connected: bool) -> Action<'_> {
        let Some(path) = self
            .assignments
            .get(output)
            .filter(|_| self.enabled && connected)
        else {
            return Action::Stop;
        };
        let pause = if self.logind == Protection::Protected || self.locker == Protection::Protected
        {
            Some(PauseReason::Protected)
        } else if self.logind != Protection::Clear || self.locker != Protection::Clear {
            Some(PauseReason::UnknownProtection)
        } else {
            match self.occupancy.get(output).copied().unwrap_or_default() {
                Occupancy::Unknown => Some(PauseReason::UnknownOccupancy),
                Occupancy::Occupied => Some(PauseReason::Occupied),
                Occupancy::Empty => None,
            }
        };
        Action::Keep {
            path,
            playback: pause.map_or(Playback::Playing, Playback::Paused),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Backoff {
    failures: u32,
    retry_at: Instant,
}

impl Backoff {
    #[must_use]
    pub fn failed(previous: Option<Self>, now: Instant) -> Self {
        let failures = previous.map_or(1, |old| old.failures.saturating_add(1));
        let millis = 100_u64
            .saturating_mul(1_u64 << failures.saturating_sub(1).min(7))
            .min(10_000);
        Self {
            failures,
            retry_at: now + Duration::from_millis(millis),
        }
    }

    #[must_use]
    pub fn retry_at(self) -> Instant {
        self.retry_at
    }
}

#[cfg(test)]
#[path = "../../tests/wallpaper/video.rs"]
mod tests;
