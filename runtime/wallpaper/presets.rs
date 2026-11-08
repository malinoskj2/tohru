//! Linux fixed-preset encoder/cache only. No caller-controlled path or image content.
#![expect(
    unsafe_code,
    reason = "fd-relative fixed-name private-cache publication and nonblocking flock"
)]
use image::ImageEncoder;
use std::ffi::{CStr, CString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};
use tohru_shell::wallpaper::preset::{HEIGHT, Palette, Preset, WIDTH};

const MAX_PNG: usize = 2 * 1024 * 1024;
const DIRECTORY: &str = "tohru-wallpaper-presets-v1";
const PENDING: &CStr = c".pending-v1.png";

#[derive(Debug, thiserror::Error)]
pub(super) enum Error {
    #[error("fixed wallpaper preset cache is busy")]
    Busy,
    #[error("fixed wallpaper preset cache ownership/path/content is invalid")]
    Cache,
    #[error("accessing the owned fixed-preset cache")]
    Io(#[from] io::Error),
    #[error("encoding a fixed wallpaper preset")]
    Encode(#[from] image::ImageError),
}
pub(super) struct Generated {
    pub(super) path: PathBuf,
    pub(super) durable: bool,
    pub(super) cached: bool,
}
struct Bounded(Vec<u8>);
impl Write for Bounded {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > MAX_PNG {
            return Err(io::Error::other("fixed preset PNG limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn at(dir: &File, name: &CStr, flags: i32, mode: libc::mode_t) -> Result<File, Error> {
    // SAFETY: borrowed live dirfd and terminated fixed/validated filename; no pointer outlives call.
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            mode,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error().into());
    }
    // SAFETY: successful openat returned a new unique owned descriptor.
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn owned(file: &File, directory: bool) -> Result<(), Error> {
    let meta = file.metadata()?;
    // SAFETY: geteuid reads the current effective ID and has no pointer arguments.
    let uid = unsafe { libc::geteuid() };
    if meta.uid() != uid
        || meta.mode() & 0o077 != 0
        || (if directory {
            !meta.is_dir()
        } else {
            !meta.is_file()
        })
    {
        return Err(Error::Cache);
    }
    Ok(())
}
fn remove_pending(dir: &File) -> Result<(), Error> {
    match at(dir, PENDING, libc::O_RDONLY, 0) {
        Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Ok(file) => {
            owned(&file, false)?;
            if file.metadata()?.len() > MAX_PNG as u64 {
                return Err(Error::Cache);
            }
        }
        Err(error) => return Err(error),
    }
    // SAFETY: unlink only the single fixed temporary name in the owned locked directory.
    if unsafe { libc::unlinkat(dir.as_raw_fd(), PENDING.as_ptr(), 0) } != 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(())
}
struct Pending<'a>(&'a File);
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        // SAFETY: guard was created only after we exclusively created this fixed temporary; dirfd remains live.
        let _ = unsafe { libc::unlinkat(self.0.as_raw_fd(), PENDING.as_ptr(), 0) };
    }
}
fn parent() -> Result<PathBuf, Error> {
    let path = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .ok_or(Error::Cache)?;
    if !path.is_absolute() || path.components().any(|p| matches!(p, Component::ParentDir)) {
        return Err(Error::Cache);
    }
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&path)?;
    Ok(fs::canonicalize(path)?)
}
fn directory(base: &Path) -> Result<(File, PathBuf, bool), Error> {
    let parent = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(base)?;
    let meta = parent.metadata()?;
    // SAFETY: geteuid reads only scalar process credentials.
    let uid = unsafe { libc::geteuid() };
    if meta.uid() != uid || meta.mode() & 0o022 != 0 {
        return Err(Error::Cache);
    }
    let name = CString::new(DIRECTORY).map_err(|_| Error::Cache)?;
    // SAFETY: mkdirat creates only the fixed cache leaf under the held parent capability.
    let result = unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o700) };
    if result != 0 {
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::AlreadyExists {
            return Err(error.into());
        }
    }
    let dir = at(&parent, &name, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    owned(&dir, true)?;
    // SAFETY: nonblocking exclusive advisory lock on the held directory; closing fd releases it.
    if unsafe { libc::flock(dir.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::WouldBlock {
            return Err(Error::Busy);
        }
        return Err(error.into());
    }
    let root = base.join(DIRECTORY);
    tohru_shell::wallpaper::validate_folder(&root).map_err(|_| Error::Cache)?;
    Ok((dir, root, parent.sync_all().is_ok()))
}
fn verify_path(dir: &File, path: &Path) -> Result<(), Error> {
    let held = dir.metadata()?;
    let current = fs::symlink_metadata(path)?;
    if !current.is_dir() || held.dev() != current.dev() || held.ino() != current.ino() {
        return Err(Error::Cache);
    }
    Ok(())
}
fn existing(dir: &File, name: &CStr, expected: &[u8]) -> Result<Option<bool>, Error> {
    match at(dir, name, libc::O_RDONLY, 0) {
        Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Ok(file) => {
            owned(&file, false)?;
            if file.metadata()?.len() > MAX_PNG as u64 {
                return Err(Error::Cache);
            }
            let mut bytes = Vec::new();
            (&file).take(MAX_PNG as u64 + 1).read_to_end(&mut bytes)?;
            if bytes != expected {
                return Err(Error::Cache);
            }
            Ok(Some(file.sync_all().is_ok()))
        }
        Err(error) => Err(error),
    }
}
pub(super) fn generate(preset: Preset, palette: Palette) -> Result<Generated, Error> {
    generate_in(&parent()?, preset, palette)
}
fn generate_in(base: &Path, preset: Preset, palette: Palette) -> Result<Generated, Error> {
    let (dir, root, parent_durable) = directory(base)?;
    let filename = format!("{}-{}.png", preset.name(), palette.name());
    let path = root.join(&filename);
    tohru_shell::wallpaper::validate_folder(&path).map_err(|_| Error::Cache)?;
    let pixels = preset.rgba(palette);
    let mut png = Bounded(Vec::new());
    image::codecs::png::PngEncoder::new(&mut png).write_image(
        &pixels,
        WIDTH,
        HEIGHT,
        image::ExtendedColorType::Rgba8,
    )?;
    let name = CString::new(filename.as_bytes()).map_err(|_| Error::Cache)?;
    let previous = existing(&dir, &name, &png.0)?;
    let cached = previous.is_some();
    let data_durable = previous.unwrap_or(true);
    if !cached {
        // Reclaim only this new cache's one fixed stale temporary while exclusively locked.
        remove_pending(&dir)?;
        let mut temporary = at(
            &dir,
            PENDING,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )?;
        let pending = Pending(&dir);
        temporary.write_all(&png.0)?;
        temporary.sync_all()?;
        drop(temporary);
        // SAFETY: two fixed names in the same locked directory; linkat never overwrites a destination.
        if unsafe {
            libc::linkat(
                dir.as_raw_fd(),
                PENDING.as_ptr(),
                dir.as_raw_fd(),
                name.as_ptr(),
                0,
            )
        } != 0
        {
            return Err(io::Error::last_os_error().into());
        }
        drop(pending);
    }
    let durable = dir.sync_all().is_ok() && data_durable && parent_durable;
    verify_path(&dir, &root)?;
    Ok(Generated {
        path,
        durable,
        cached,
    })
}
#[cfg(test)]
#[path = "../../tests/wallpaper/presets.rs"]
mod tests;
