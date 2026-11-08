//! Read-only catalog of regular media files directly in an explicitly chosen folder.
pub mod preset;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub const MAX_ENTRIES: usize = 4096;
pub const MAX_PATH_BYTES: usize = 4096;
pub const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("choose an absolute UTF-8 wallpaper folder path of at most 4096 bytes")]
    InvalidFolder,
    #[error("wallpaper folder exceeds its 4096-entry or 1 MiB catalog limit")]
    TooLarge,
    #[error("wallpaper folder contains an unsupported path encoding or length")]
    InvalidEntry,
    #[error("wallpaper file changed outside the selected folder during listing")]
    ChangedOutsideFolder,
    #[error("wallpaper identity requires an existing regular supported media file")]
    InvalidFile,
    #[error("wallpaper identity request exceeds32 files")]
    TooManyFiles,
    #[error("reading wallpaper folder")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Image,
    Video,
}

#[must_use]
pub fn kind(path: &Path) -> Option<Kind> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "png" | "jpg" | "jpeg" | "webp" => Some(Kind::Image),
        "mp4" | "webm" | "mkv" | "mov" | "gif" => Some(Kind::Video),
        _ => None,
    }
}

/// Validate the caller-selected folder representation without filesystem I/O.
///
/// # Errors
/// Returns `InvalidFolder` for relative, non-UTF-8, NUL or overlong paths.
pub fn validate_folder(path: &Path) -> Result<(), Error> {
    let value = path.to_str().ok_or(Error::InvalidFolder)?;
    if !path.is_absolute() || value.len() > MAX_PATH_BYTES || value.contains('\0') {
        return Err(Error::InvalidFolder);
    }
    Ok(())
}

/// Canonical identities for at most32 explicitly assigned/chosen media paths.
/// This checks file metadata only; it never opens or decodes media contents.
///
/// # Errors
/// Rejects invalid/unsupported/nonregular/missing files and request bounds.
pub fn identities(paths: &[PathBuf]) -> Result<Vec<String>, Error> {
    if paths.len() > 32 {
        return Err(Error::TooManyFiles);
    }
    paths
        .iter()
        .map(|path| {
            validate_folder(path)?;
            if kind(path).is_none() {
                return Err(Error::InvalidFile);
            }
            let canonical = fs::canonicalize(path)?;
            validate_folder(&canonical)?;
            if !fs::metadata(&canonical)?.is_file() {
                return Err(Error::InvalidFile);
            }
            canonical
                .into_os_string()
                .into_string()
                .map_err(|_| Error::InvalidEntry)
        })
        .collect()
}

/// Invoke in the narrow probe process: individual filesystem calls may block.
/// Entry/count/serialization bounds do not promise a filesystem I/O deadline.
///
/// # Errors
/// Rejects invalid paths, I/O failures, catalog limits and files that changed
/// outside the selected folder; no partial catalog is returned.
pub fn catalog(folder: &Path) -> Result<Vec<String>, Error> {
    validate_folder(folder)?;
    let root = fs::canonicalize(folder)?;
    validate_folder(&root)?;
    let mut files = BTreeSet::new();
    let mut escaped_bytes = 2usize;
    for (index, entry) in fs::read_dir(&root)?.enumerate() {
        if index >= MAX_ENTRIES {
            return Err(Error::TooLarge);
        }
        let entry = entry?;
        // Do not follow symlinks or descend into subdirectories. In particular,
        // FIFOs/devices with media extensions must never be opened or decoded.
        if !entry.file_type()?.is_file() || kind(&entry.path()).is_none() {
            continue;
        }
        let path: PathBuf = fs::canonicalize(entry.path())?;
        if path.parent() != Some(root.as_path()) {
            return Err(Error::ChangedOutsideFolder);
        }
        let value = path.to_str().ok_or(Error::InvalidEntry)?;
        if value.len() > MAX_PATH_BYTES || value.contains('\0') {
            return Err(Error::InvalidEntry);
        }
        // Every byte might need JSON's six-byte control escape. Bound before
        // serialization, including separators/quotes, rather than allocating first.
        escaped_bytes = value
            .len()
            .checked_mul(6)
            .and_then(|bytes| bytes.checked_add(4))
            .and_then(|bytes| escaped_bytes.checked_add(bytes))
            .filter(|bytes| *bytes <= MAX_RESPONSE_BYTES)
            .ok_or(Error::TooLarge)?;
        files.insert(value.to_owned());
    }
    Ok(files.into_iter().collect())
}

#[cfg(test)]
#[path = "../../tests/wallpaper/catalog.rs"]
mod tests;
