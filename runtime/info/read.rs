use super::{Issue, IssueKind};
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const TOTAL_BYTES: usize = 1 << 20;
const MAX_READS: usize = 384;
const MAX_ISSUES: usize = 32;
pub(super) const MAX_TEXT: usize = 256;

/// A single fixed-source snapshot owns its cumulative I/O budget.
pub(super) struct Reader<'a> {
    root: &'a Path,
    remaining: usize,
    reads: usize,
    deadline: Instant,
    pub(super) issues: Vec<Issue>,
    failures: usize,
}

impl<'a> Reader<'a> {
    pub(super) fn new(root: &'a Path) -> Self {
        Self {
            root,
            remaining: TOTAL_BYTES,
            reads: MAX_READS,
            deadline: Instant::now() + Duration::from_millis(250),
            issues: Vec::new(),
            failures: 0,
        }
    }

    pub(super) fn failure_count(&self) -> usize {
        self.failures
    }

    pub(super) fn issue(&mut self, source: &str, kind: IssueKind) {
        self.failures = self.failures.saturating_add(1);
        if self.issues.len() < MAX_ISSUES {
            self.issues.push(Issue {
                source: bounded(source),
                kind,
            });
        }
    }

    fn permit(&mut self, source: &str) -> bool {
        if self.reads == 0 || self.remaining == 0 || Instant::now() >= self.deadline {
            self.issue(source, IssueKind::Budget);
            return false;
        }
        self.reads -= 1;
        true
    }

    fn path(&self, source: &str) -> PathBuf {
        self.root.join(source)
    }

    pub(super) fn text(&mut self, source: &str, max: usize) -> Option<String> {
        if !self.permit(source) {
            return None;
        }
        let result = (|| {
            let mut options = OpenOptions::new();
            options.read(true);
            // Linux O_NONBLOCK: never wait opening an accidentally substituted FIFO.
            // No FFI/unsafe; a non-regular opened file is rejected before reading.
            #[cfg(target_os = "linux")]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.custom_flags(0o4000);
            }
            let file = options
                .open(self.path(source))
                .map_err(|e| IssueKind::Io(e.kind()))?;
            if !file
                .metadata()
                .map_err(|e| IssueKind::Io(e.kind()))?
                .is_file()
            {
                return Err(IssueKind::Invalid);
            }
            let allowance = max.saturating_add(1).min(self.remaining);
            let mut bytes = Vec::new();
            let result = file
                .take(u64::try_from(allowance).unwrap_or(u64::MAX))
                .read_to_end(&mut bytes);
            self.remaining = self.remaining.saturating_sub(bytes.len());
            result.map_err(|error| IssueKind::Io(error.kind()))?;
            if bytes.len() > max {
                return Err(IssueKind::TooLarge);
            }
            // Exhausting the cumulative budget cannot prove a complete final file.
            if bytes.len() == allowance && allowance <= max {
                return Err(IssueKind::Budget);
            }
            String::from_utf8(bytes).map_err(|_| IssueKind::Invalid)
        })();
        match result {
            Ok(value) => Some(value),
            Err(kind) => {
                self.issue(source, kind);
                None
            }
        }
    }

    pub(super) fn line(&mut self, source: &str) -> Option<String> {
        self.text(source, 4096)
            .map(|text| bounded(text.trim()))
            .filter(|text| !text.is_empty())
    }

    pub(super) fn link(&mut self, source: &str) -> Option<String> {
        if !self.permit(source) {
            return None;
        }
        match fs::read_link(self.path(source)) {
            Ok(path) => match path.to_str() {
                Some(path) if path.chars().count() <= MAX_TEXT => Some(path.to_owned()),
                Some(_) => {
                    self.issue(source, IssueKind::TooLarge);
                    None
                }
                None => {
                    self.issue(source, IssueKind::Invalid);
                    None
                }
            },
            Err(error) => {
                self.issue(source, IssueKind::Io(error.kind()));
                None
            }
        }
    }

    /// Resolve only the active system/profile symlink identity, never arbitrary
    /// content. Eight hops and the common snapshot request budget bound cycles.
    pub(super) fn system_identity(&mut self, source: &str) -> Option<String> {
        use std::path::Component;
        let mut path = PathBuf::from(source);
        for _ in 0..8 {
            let source = path.to_str()?;
            if !self.permit(source) {
                return None;
            }
            let metadata = match fs::symlink_metadata(self.root.join(&path)) {
                Ok(metadata) => metadata,
                Err(error) => {
                    self.issue(source, IssueKind::Io(error.kind()));
                    return None;
                }
            };
            if metadata.is_dir() && path.starts_with("nix/store") && path.components().count() == 3
            {
                let identity = format!("/{}", path.to_str()?);
                if identity.chars().count() > MAX_TEXT {
                    self.issue(source, IssueKind::TooLarge);
                    return None;
                }
                return Some(identity);
            }
            if !metadata.is_symlink() {
                self.issue(source, IssueKind::Invalid);
                return None;
            }
            let target = self.link(source)?;
            let joined = if target.starts_with('/') {
                PathBuf::from(target)
            } else {
                path.parent()?.join(target)
            };
            let mut normalized = PathBuf::new();
            for component in joined.components() {
                match component {
                    Component::Normal(part) => normalized.push(part),
                    Component::ParentDir => {
                        if !normalized.pop() {
                            return None;
                        }
                    }
                    Component::RootDir | Component::CurDir => {}
                    Component::Prefix(_) => return None,
                }
            }
            if !normalized.starts_with("nix/store")
                && !normalized.starts_with("nix/var/nix/profiles")
            {
                self.issue(source, IssueKind::Invalid);
                return None;
            }
            path = normalized;
        }
        self.issue(source, IssueKind::Budget);
        None
    }

    pub(super) fn names(&mut self, source: &str, max: usize) -> Vec<String> {
        if !self.permit(source) {
            return Vec::new();
        }
        let entries = match fs::read_dir(self.path(source)) {
            Ok(entries) => entries,
            Err(error) => {
                self.issue(source, IssueKind::Io(error.kind()));
                return Vec::new();
            }
        };
        let mut names = Vec::new();
        for (index, entry) in entries.take(max + 1).enumerate() {
            if index == max {
                self.issue(source, IssueKind::TooLarge);
                break;
            }
            match entry {
                Ok(entry) => {
                    if let Some(name) = entry.file_name().to_str() {
                        names.push(name.to_owned());
                    } else {
                        self.issue(source, IssueKind::Invalid);
                    }
                }
                Err(error) => self.issue(source, IssueKind::Io(error.kind())),
            }
        }
        names.sort();
        names
    }
}

pub(super) fn bounded(text: &str) -> String {
    text.chars().take(MAX_TEXT).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_generation_scan_cannot_establish_uniqueness() {
        let mut reader = Reader::new(Path::new("/"));
        reader.deadline = Instant::now();
        let generations = crate::storage::generations(&mut reader, Some("/nix/store/active"));
        assert!(generations.is_empty());
        assert!(
            reader
                .issues
                .iter()
                .any(|issue| issue.kind == IssueKind::Budget)
        );
    }
}
