//! Bounded, read-only public Linux information for About and information pages.
//! Called only on page open/explicit refresh. No commands, caller paths or polling.

mod read;
mod storage;
mod system;

use std::path::Path;

/// Missing facts remain unknown; available portions survive unrelated failures.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub system: System,
    pub accounts: Vec<Account>,
    pub disks: Vec<Disk>,
    pub mounts: Vec<Mount>,
    pub generations: Vec<Generation>,
    pub issues: Vec<Issue>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct System {
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    pub hostname: Option<String>,
    pub kernel: Option<String>,
    pub processor: Option<String>,
    pub graphics: Vec<String>,
    pub memory_total_bytes: Option<u64>,
    pub memory_available_bytes: Option<u64>,
    pub uptime_seconds: Option<u64>,
    pub current_uid: Option<u32>,
    pub current_generation: Option<u64>,
    pub configuration_revision: Option<String>,
    pub system_target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub name: String,
    pub uid: u32,
    pub description: String,
    pub home: String,
    pub shell: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disk {
    pub name: String,
    pub model: Option<String>,
    pub capacity_bytes: Option<u64>,
    pub removable: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub path: String,
    pub filesystem: String,
    pub source: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generation {
    pub number: u64,
    pub target: String,
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub source: String,
    pub kind: IssueKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IssueKind {
    #[error("reading public system information failed ({0:?})")]
    Io(std::io::ErrorKind),
    #[error("public information exceeded its byte or entry limit")]
    TooLarge,
    #[error("public information has an invalid format or identity")]
    Invalid,
    #[error("the information snapshot budget was exhausted")]
    Budget,
}

/// Read the fixed public Linux sources. Byte/entry/read counts are hard capped;
/// the250ms deadline is cooperative between filesystem operations, not hard RT.
#[must_use]
pub fn snapshot() -> Snapshot {
    collect(Path::new("/"))
}

fn collect(root: &Path) -> Snapshot {
    let mut reader = read::Reader::new(root);
    let mut system = system::read(&mut reader);
    let accounts = system::accounts(&mut reader);
    let disks = storage::disks(&mut reader);
    let mounts = storage::mounts(&mut reader);
    let generations = storage::generations(&mut reader, system.system_target.as_deref());
    system.current_generation = generations
        .iter()
        .find(|generation| generation.current)
        .map(|generation| generation.number);
    Snapshot {
        system,
        accounts,
        disks,
        mounts,
        generations,
        issues: reader.issues,
    }
}

#[cfg(test)]
mod tests;
