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
}

impl<'a> Reader<'a> {
    pub(super) fn new(root: &'a Path) -> Self {
        Self {
            root,
            remaining: TOTAL_BYTES,
            reads: MAX_READS,
            deadline: Instant::now() + Duration::from_millis(250),
            issues: Vec::new(),
        }
    }

    pub(super) fn issue(&mut self, source: &str, kind: IssueKind) {
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
            Ok(path) => Some(bounded(&path.to_string_lossy())),
            Err(error) => {
                self.issue(source, IssueKind::Io(error.kind()));
                None
            }
        }
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
