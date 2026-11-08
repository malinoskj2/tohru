use super::Error;

pub const MAX_PIXELS: u64 = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    /// # Errors
    /// Rejects empty rectangles and edges outside signed logical coordinates.
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Result<Self, Error> {
        let width_i = i32::try_from(width).map_err(|_| Error::Geometry)?;
        let height_i = i32::try_from(height).map_err(|_| Error::Geometry)?;
        if width == 0
            || height == 0
            || x.checked_add(width_i).is_none()
            || y.checked_add(height_i).is_none()
        {
            return Err(Error::Geometry);
        }
        Ok(Self {
            x,
            y,
            width,
            height,
        })
    }

    fn intersection(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (i64::from(self.x) + i64::from(self.width))
            .min(i64::from(other.x) + i64::from(other.width));
        let bottom = (i64::from(self.y) + i64::from(self.height))
            .min(i64::from(other.y) + i64::from(other.height));
        Self::new(
            x,
            y,
            u32::try_from(right - i64::from(x)).ok()?,
            u32::try_from(bottom - i64::from(y)).ok()?,
        )
        .ok()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub logical: Rect,
    /// Upright captured dimensions, after the native capture backend undoes rotation.
    pub pixels: (u32, u32),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Piece {
    pub output: usize,
    pub source: Rect,
    pub destination: Rect,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    pub pixels: (u32, u32),
    pub pieces: Vec<Piece>,
}

fn budget(width: u32, height: u32) -> Result<(), Error> {
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(Error::Geometry);
    }
    Ok(())
}

fn edge(value: u32, numerator: u32, denominator: u32, ceil: bool) -> Result<u32, Error> {
    if denominator == 0 {
        return Err(Error::Geometry);
    }
    let scaled = u64::from(value) * u64::from(numerator);
    let value = if ceil {
        scaled.div_ceil(u64::from(denominator))
    } else {
        scaled / u64::from(denominator)
    };
    u32::try_from(value).map_err(|_| Error::Geometry)
}

/// Plan crops and placement at the densest output's actual upright pixel density.
/// No pixels are read or allocated here; unoccupied desktop gaps remain transparent.
///
/// # Errors
/// Rejects invalid layout/geometry, no intersecting output, more than32 outputs,
/// or retained/output pixels beyond32Mi pixels. No partial plan is returned.
pub fn plan(region: Rect, outputs: &[Layout]) -> Result<Plan, Error> {
    Rect::new(region.x, region.y, region.width, region.height)?;
    budget(region.width, region.height)?;
    if outputs.is_empty() || outputs.len() > 32 {
        return Err(Error::Outputs);
    }
    let mut total = 0_u64;
    let mut scale = (0_u32, 1_u32);
    for output in outputs {
        Rect::new(
            output.logical.x,
            output.logical.y,
            output.logical.width,
            output.logical.height,
        )?;
        budget(output.pixels.0, output.pixels.1)?;
        total += u64::from(output.pixels.0) * u64::from(output.pixels.1);
        if total > MAX_PIXELS {
            return Err(Error::Geometry);
        }
        if region.intersection(output.logical).is_some() {
            for candidate in [
                (output.pixels.0, output.logical.width),
                (output.pixels.1, output.logical.height),
            ] {
                if u64::from(candidate.0) * u64::from(scale.1)
                    > u64::from(scale.0) * u64::from(candidate.1)
                {
                    scale = candidate;
                }
            }
        }
    }
    if scale.0 == 0 {
        return Err(Error::Outside);
    }
    let pixels = (
        edge(region.width, scale.0, scale.1, true)?,
        edge(region.height, scale.0, scale.1, true)?,
    );
    budget(pixels.0, pixels.1)?;
    let mut pieces = Vec::new();
    for (index, output) in outputs.iter().enumerate() {
        let Some(area) = region.intersection(output.logical) else {
            continue;
        };
        let x = u32::try_from(i64::from(area.x) - i64::from(output.logical.x))
            .map_err(|_| Error::Geometry)?;
        let y = u32::try_from(i64::from(area.y) - i64::from(output.logical.y))
            .map_err(|_| Error::Geometry)?;
        let sx = edge(x, output.pixels.0, output.logical.width, false)?;
        let sy = edge(y, output.pixels.1, output.logical.height, false)?;
        let ex = edge(x + area.width, output.pixels.0, output.logical.width, true)?;
        let ey = edge(
            y + area.height,
            output.pixels.1,
            output.logical.height,
            true,
        )?;
        let dx =
            u32::try_from(i64::from(area.x) - i64::from(region.x)).map_err(|_| Error::Geometry)?;
        let dy =
            u32::try_from(i64::from(area.y) - i64::from(region.y)).map_err(|_| Error::Geometry)?;
        let left = edge(dx, scale.0, scale.1, false)?;
        let top = edge(dy, scale.0, scale.1, false)?;
        let right = edge(dx + area.width, scale.0, scale.1, true)?;
        let bottom = edge(dy + area.height, scale.0, scale.1, true)?;
        pieces.push(Piece {
            output: index,
            source: Rect::new(
                i32::try_from(sx).map_err(|_| Error::Geometry)?,
                i32::try_from(sy).map_err(|_| Error::Geometry)?,
                ex - sx,
                ey - sy,
            )?,
            destination: Rect::new(
                i32::try_from(left).map_err(|_| Error::Geometry)?,
                i32::try_from(top).map_err(|_| Error::Geometry)?,
                right - left,
                bottom - top,
            )?,
        });
    }
    if pieces.is_empty() {
        return Err(Error::Outside);
    }
    Ok(Plan { pixels, pieces })
}

#[cfg(test)]
#[path = "../../tests/capture/geometry.rs"]
mod tests;
