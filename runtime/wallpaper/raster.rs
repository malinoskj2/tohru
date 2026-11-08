//! Native image-worker input bounds. No renderer, decoder implementation or service.
use std::fs::{File, OpenOptions};
use std::io::{Cursor, Read};
use std::path::Path;

pub(super) const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PIXELS: u64 = 32 * 1024 * 1024;
const MAX_SIDE: u32 = 16_384;
const MAX_ALLOC: u64 = 256 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub(super) enum Error {
    #[error("image input must be a regular file")]
    NotRegular,
    #[error("image input exceeds64MiB or raster exceeds32Mi pixels/16384 per side")]
    Bounds,
    #[error("reading image input")]
    Io(#[from] std::io::Error),
    #[error("decoding bounded image input")]
    Decode(#[from] image::ImageError),
}

fn open(path: &Path) -> Result<File, Error> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Do not block opening a swapped-in FIFO. Following ordinary symlink
        // aliases is native behavior; fstat validates the actual opened inode.
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(Error::NotRegular);
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(Error::Bounds);
    }
    Ok(file)
}

/// One bounded fd read. Growth/replacement cannot cause an unbounded allocation;
/// dimensions/decode subsequently use only these captured immutable bytes.
pub(super) fn read(path: &Path) -> Result<Vec<u8>, Error> {
    read_limited(open(path)?, MAX_FILE_BYTES)
}

fn read_limited(reader: impl Read, limit: u64) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(Error::Bounds);
    }
    Ok(bytes)
}

pub(super) fn limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    limits
}

fn checked(size: (u32, u32)) -> Result<(u32, u32), Error> {
    if size.0 == 0
        || size.1 == 0
        || size.0 > MAX_SIDE
        || size.1 > MAX_SIDE
        || u64::from(size.0) * u64::from(size.1) > MAX_PIXELS
    {
        return Err(Error::Bounds);
    }
    Ok(size)
}

/// Preliminary worker sizing only; actual decode validates its own immutable data.
pub(super) fn dimensions(path: &Path) -> Result<(u32, u32), Error> {
    let bytes = read(path)?;
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    reader.limits(limits());
    checked(reader.into_dimensions()?)
}

pub(super) fn dimensions_bytes(bytes: &[u8]) -> Result<(u32, u32), Error> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(Error::Bounds);
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    reader.limits(limits());
    checked(reader.into_dimensions()?)
}

/// Bound the existing native animation decoder's retained source frames before
/// its existing output decimation. GIF wallpapers still use the video backend.
pub(super) fn frames(frames: image::Frames<'_>) -> Result<Vec<image::Frame>, Error> {
    let mut output = Vec::new();
    let mut bytes = 0u64;
    for frame in frames {
        if output.len() >= 4096 {
            return Err(Error::Bounds);
        }
        let frame = frame?;
        checked(frame.buffer().dimensions())?;
        bytes = bytes
            .checked_add(frame.buffer().len() as u64)
            .filter(|total| *total <= MAX_ALLOC)
            .ok_or(Error::Bounds)?;
        output.push(frame);
    }
    Ok(output)
}

pub(super) fn decode(bytes: &[u8]) -> Result<(image::RgbaImage, (u32, u32)), Error> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(Error::Bounds);
    }
    let size = dimensions_bytes(bytes)?;
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    reader.limits(limits());
    let decoded = reader.decode()?;
    // These must describe the bytes actually decoded, never a previous path lookup.
    if (decoded.width(), decoded.height()) != size {
        return Err(Error::Bounds);
    }
    Ok((decoded.to_rgba8(), size))
}

#[cfg(test)]
#[path = "../../tests/wallpaper/raster.rs"]
mod tests;
