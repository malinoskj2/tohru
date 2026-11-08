//! Bounded geometry and no-overwrite publication for the native frozen-capture adapter.
mod files;
mod geometry;

pub use files::{SaveOutcome, save_png};
pub use geometry::{Layout, MAX_PIXELS, Piece, Plan, Rect, plan};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("capture rectangle is empty, overflowing or outside the pixel budget")]
    Geometry,
    #[error("capture requires between one and 32 frozen outputs")]
    Outputs,
    #[error("capture region intersects no frozen output")]
    Outside,
    #[error("choose an absolute PNG destination without parent traversal, at most 4096 bytes")]
    Path,
    #[error("PNG data is empty or exceeds the capture byte budget")]
    Bytes,
    #[error("publishing capture PNG")]
    Io(#[from] std::io::Error),
}
