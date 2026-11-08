use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::{Error, MAX_PIXELS};

static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub enum SaveOutcome {
    Saved(PathBuf),
    DurabilityUnconfirmed {
        path: PathBuf,
        source: std::io::Error,
    },
}

struct Temporary(PathBuf);

impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Publish complete PNG bytes without replacing any file or symlink.
///
/// # Errors
/// Rejects invalid PNG paths/size and any pre-publication I/O failure. A directory
/// sync failure after publication returns `DurabilityUnconfirmed`, never a false rollback.
pub fn save_png(path: &Path, png: &[u8]) -> Result<SaveOutcome, Error> {
    if !path.is_absolute()
        || path
            .to_str()
            .is_none_or(|p| p.len() > 4096 || p.contains('\0'))
        || path.components().any(|p| matches!(p, Component::ParentDir))
        || path.extension().and_then(|s| s.to_str()) != Some("png")
    {
        return Err(Error::Path);
    }
    if !png.starts_with(b"\x89PNG\r\n\x1a\n") || png.len() as u64 > MAX_PIXELS * 4 + 1024 * 1024 {
        return Err(Error::Bytes);
    }
    let parent = path.parent().ok_or(Error::Path)?;
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(parent)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut created = None;
    for _ in 0..32 {
        let temporary = parent.join(format!(
            ".tohru-capture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match options.open(&temporary) {
            Ok(file) => {
                created = Some((Temporary(temporary), file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    let (temporary, mut file) = created.ok_or_else(|| {
        Error::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "capture temporary names are exhausted",
        ))
    })?;
    file.write_all(png)?;
    file.sync_all()?;
    drop(file);
    // link creates the destination atomically and fails if it already exists,
    // including a symlink. No check-then-rename overwrite race.
    fs::hard_link(&temporary.0, path)?;
    drop(temporary);
    match File::open(parent).and_then(|directory| directory.sync_all()) {
        Ok(()) => Ok(SaveOutcome::Saved(path.to_owned())),
        Err(source) => Ok(SaveOutcome::DurabilityUnconfirmed {
            path: path.to_owned(),
            source,
        }),
    }
}

#[cfg(test)]
#[path = "../../tests/capture/files.rs"]
mod tests;
