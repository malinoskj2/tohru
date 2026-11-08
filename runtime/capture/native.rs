//! Child of pinned `platform::capture`: reuses its sole protocol link and frozen pixels.
use std::collections::HashSet;
use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;

use image::{ImageEncoder, RgbaImage};
use tohru_shell::capture::{Layout, MAX_PIXELS, Rect, SaveOutcome};

use super::{Asked, FROZEN, Frozen, Link, SysValue, with_link};

#[path = "clipboard_owner.rs"]
mod clipboard;

pub(super) fn shutdown() {
    clipboard::shutdown();
}

fn map(fields: Vec<(&str, SysValue)>) -> SysValue {
    SysValue::Map(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

pub(super) fn outputs(link: &mut Link) -> Result<SysValue, Asked> {
    link.settle()?;
    if link.state.outputs.len() > 32 {
        return Err(Asked::No("capture supports at most32 outputs".into()));
    }
    let mut records = Vec::new();
    for output in &link.state.outputs {
        let rect = Rect::new(
            output.at.0,
            output.at.1,
            u32::try_from(output.size.0).unwrap_or(0),
            u32::try_from(output.size.1).unwrap_or(0),
        )
        .map_err(|_| {
            Asked::No(format!(
                "capture output '{}' has no valid logical layout yet",
                output.name
            ))
        })?;
        if output.name.is_empty() || output.transform > 7 {
            return Err(Asked::No("capture output metadata is incomplete".into()));
        }
        records.push(map(vec![
            ("name", SysValue::Text(output.name.clone())),
            ("x", SysValue::Num(f64::from(rect.x))),
            ("y", SysValue::Num(f64::from(rect.y))),
            ("width", SysValue::Num(f64::from(rect.width))),
            ("height", SysValue::Num(f64::from(rect.height))),
            ("scale", SysValue::Num(output.scale())),
            ("transform", SysValue::Num(f64::from(output.transform))),
        ]));
    }
    Ok(SysValue::List(records))
}

fn token(token: &str) -> Result<(&str, u64), String> {
    let (name, version) = token
        .strip_prefix("capture:")
        .and_then(|value| value.rsplit_once('?'))
        .ok_or("capture operation requires a versioned freeze token")?;
    if name.is_empty() || name.len() > 255 {
        return Err("invalid frozen output name".into());
    }
    let version = version
        .parse()
        .map_err(|_| "invalid capture token version")?;
    Ok((name, version))
}

fn snapshots(tokens: &[SysValue]) -> Result<Vec<(String, Frozen)>, String> {
    if tokens.is_empty() || tokens.len() > 32 {
        return Err("capture export requires one to32 freeze tokens".into());
    }
    let frozen = FROZEN
        .lock()
        .map_err(|_| "frozen capture cache unavailable")?;
    let frozen = frozen.as_ref().ok_or("no frozen frames")?;
    let mut seen = HashSet::new();
    let mut records = Vec::new();
    for token_value in tokens {
        let SysValue::Text(value) = token_value else {
            return Err("capture tokens must be text".into());
        };
        let (name, version) = token(value)?;
        if !seen.insert(name) {
            return Err("duplicate frozen output token".into());
        }
        let record = frozen
            .get(name)
            .filter(|record| record.frame.version == version)
            .ok_or("frozen frame was released or superseded")?;
        records.push((name.to_owned(), record.clone()));
    }
    Ok(records)
}

#[expect(
    clippy::cast_precision_loss,
    reason = "cache is at most128MiB and process Arc counts are below f64 exact integer range"
)]
pub(super) fn release(value: &str) -> Result<SysValue, String> {
    let (name, version) = token(value)?;
    let mut frozen = FROZEN
        .lock()
        .map_err(|_| "frozen capture cache unavailable")?;
    let Some(frozen) = frozen.as_mut() else {
        return Ok(map(vec![("released", SysValue::Bool(false))]));
    };
    if frozen
        .get(name)
        .is_some_and(|record| record.frame.version != version)
    {
        return Ok(map(vec![
            ("released", SysValue::Bool(false)),
            ("stale", SysValue::Bool(true)),
        ]));
    }
    let removed = frozen.remove(name);
    let references = removed.as_ref().map_or(0, |record| {
        Arc::strong_count(&record.frame).saturating_sub(1)
    });
    let bytes = frozen
        .values()
        .map(|record| record.frame.pixels.len() as u64)
        .sum::<u64>();
    Ok(map(vec![
        ("released", SysValue::Bool(removed.is_some())),
        ("cached_bytes", SysValue::Num(bytes as f64)),
        (
            "external_frame_references",
            SysValue::Num(references as f64),
        ),
    ]))
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "whole finite value is range-checked against i32 before conversion"
)]
fn coordinate(value: f64) -> Result<i32, String> {
    if !value.is_finite()
        || value.fract() != 0.0
        || value < f64::from(i32::MIN)
        || value > f64::from(i32::MAX)
    {
        return Err("capture coordinates must be finite whole logical pixels".into());
    }
    Ok(value as i32)
}

fn image(region: Rect, records: &[(String, Frozen)]) -> Result<RgbaImage, String> {
    let layouts: Vec<Layout> = records.iter().map(|(_, record)| record.layout).collect();
    let plan = tohru_shell::capture::plan(region, &layouts).map_err(|e| e.to_string())?;
    let mut canvas = RgbaImage::new(plan.pixels.0, plan.pixels.1);
    for piece in plan.pieces {
        let record = &records[piece.output].1;
        let frame = &record.frame;
        let expected = u64::from(frame.width) * u64::from(frame.height) * 4;
        if frame.pixels.len() as u64 != expected
            || (frame.width, frame.height) != record.layout.pixels
        {
            return Err("frozen capture pixel layout is inconsistent".into());
        }
        let mut rgba = Vec::with_capacity(
            usize::try_from(u64::from(piece.source.width) * u64::from(piece.source.height) * 4)
                .map_err(|_| "capture crop is too large")?,
        );
        let x = u32::try_from(piece.source.x).map_err(|_| "negative pixel crop")?;
        let y = u32::try_from(piece.source.y).map_err(|_| "negative pixel crop")?;
        for row in y..y + piece.source.height {
            let start =
                usize::try_from((u64::from(row) * u64::from(frame.width) + u64::from(x)) * 4)
                    .map_err(|_| "capture crop offset overflow")?;
            let end = start
                + usize::try_from(piece.source.width).map_err(|_| "capture crop width overflow")?
                    * 4;
            let row = frame
                .pixels
                .get(start..end)
                .ok_or("capture crop outside frozen frame")?;
            for pixel in row.chunks_exact(4) {
                rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
            }
        }
        let crop = RgbaImage::from_raw(piece.source.width, piece.source.height, rgba)
            .ok_or("capture crop allocation mismatch")?;
        let size = (piece.destination.width, piece.destination.height);
        let crop = if crop.dimensions() == size {
            crop
        } else {
            image::imageops::resize(&crop, size.0, size.1, image::imageops::FilterType::Triangle)
        };
        image::imageops::replace(
            &mut canvas,
            &crop,
            i64::from(piece.destination.x),
            i64::from(piece.destination.y),
        );
    }
    Ok(canvas)
}

struct Png(Vec<u8>);
impl Write for Png {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) as u64 > MAX_PIXELS * 4 + 1024 * 1024 {
            return Err(io::Error::other(
                "encoded capture PNG exceeds its byte budget",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn region(args: &[SysValue]) -> Result<SysValue, String> {
    let [
        SysValue::List(tokens),
        SysValue::Num(x),
        SysValue::Num(y),
        SysValue::Num(w),
        SysValue::Num(h),
        SysValue::Text(path),
        SysValue::Bool(copy),
    ] = args
    else {
        return Err("capture.region takes freeze tokens,x,y,width,height,absolute PNG path (or empty),copy boolean".into());
    };
    if path.is_empty() && !copy {
        return Err("enable PNG saving or image clipboard before capture".into());
    }
    let width = u32::try_from(coordinate(*w)?).map_err(|_| "capture width must be positive")?;
    let height = u32::try_from(coordinate(*h)?).map_err(|_| "capture height must be positive")?;
    let rectangle =
        Rect::new(coordinate(*x)?, coordinate(*y)?, width, height).map_err(|e| e.to_string())?;
    let records = snapshots(tokens)?;
    with_link(|link| {
        link.settle()?;
        for (name, record) in &records {
            let now = link.output(name)?;
            let old = record.layout.logical;
            let old_size = (
                i32::try_from(old.width).map_err(|_| Asked::No("invalid frozen width".into()))?,
                i32::try_from(old.height).map_err(|_| Asked::No("invalid frozen height".into()))?,
            );
            if now.at != (old.x, old.y)
                || now.size != old_size
                || now.transform != record.transform
                || now.mode != record.mode
            {
                return Err(Asked::No(
                    "output layout changed after freeze; start a new region capture".into(),
                ));
            }
        }
        Ok(())
    })?;
    let image = image(rectangle, &records)?;
    let mut png = Png(Vec::new());
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("encoding capture PNG: {e}"))?;
    let (mut saved, mut durable, mut save_error) = (false, false, String::new());
    if !path.is_empty() {
        match tohru_shell::capture::save_png(Path::new(path), &png.0) {
            Ok(SaveOutcome::Saved(_)) => {
                saved = true;
                durable = true;
            }
            Ok(SaveOutcome::DurabilityUnconfirmed { source, .. }) => {
                saved = true;
                save_error = format!("PNG saved, crash durability unconfirmed: {source}");
            }
            Err(error) => {
                save_error = anyhow::Error::from(error)
                    .chain()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(": ");
            }
        }
    }
    let (copied, copy_error) = if *copy {
        match clipboard::copy(png.0) {
            Ok(()) => (true, String::new()),
            Err(error) => (false, error),
        }
    } else {
        (false, String::new())
    };
    Ok(map(vec![
        ("saved", SysValue::Bool(saved)),
        (
            "path",
            SysValue::Text(if saved { path.clone() } else { String::new() }),
        ),
        ("durable", SysValue::Bool(durable)),
        ("copied", SysValue::Bool(copied)),
        ("save_error", SysValue::Text(save_error)),
        ("clipboard_error", SysValue::Text(copy_error)),
        ("width", SysValue::Num(f64::from(image.width()))),
        ("height", SysValue::Num(f64::from(image.height()))),
    ]))
}

#[cfg(test)]
mod tests {
    use super::super::Frame;
    use super::*;
    fn frozen(version: u64) -> Frozen {
        Frozen {
            frame: Arc::new(Frame {
                version,
                width: 2,
                height: 2,
                pixels: vec![0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0],
                opaque: true,
                card: None,
            }),
            layout: Layout {
                logical: Rect::new(-2, 0, 2, 2).unwrap(),
                pixels: (2, 2),
            },
            transform: 0,
            mode: Some((2, 2)),
        }
    }
    #[test]
    fn native_bgra_crop_and_png_round_trip_preserve_upright_colors() {
        let records = vec![("PIXELS".into(), frozen(1))];
        let canvas = image(Rect::new(-2, 0, 2, 2).unwrap(), &records).unwrap();
        assert_eq!(canvas.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(canvas.get_pixel(0, 1).0, [0, 0, 255, 255]);
        let mut png = Png(Vec::new());
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(canvas.as_raw(), 2, 2, image::ExtendedColorType::Rgba8)
            .unwrap();
        assert_eq!(
            image::load_from_memory(&png.0).unwrap().into_rgba8(),
            canvas
        );
        let crop = image(Rect::new(-1, 0, 1, 2).unwrap(), &records).unwrap();
        assert_eq!(crop.get_pixel(0, 0).0, [0, 255, 0, 255]);
        assert_eq!(crop.get_pixel(0, 1).0, [255, 255, 255, 255]);
    }
    #[test]
    fn freeze_tokens_bind_version_and_release_only_cache_ownership() {
        let name = "EXPORT-TOKEN-TEST";
        let frame = frozen(17);
        let retained = Arc::clone(&frame.frame);
        FROZEN
            .lock()
            .unwrap()
            .get_or_insert_with(Default::default)
            .insert(name.into(), frame);
        assert!(snapshots(&[SysValue::Text(format!("capture:{name}?16"))]).is_err());
        assert!(release(&format!("capture:{name}?16")).is_ok());
        assert!(snapshots(&[SysValue::Text(format!("capture:{name}?17"))]).is_ok());
        assert!(
            snapshots(&[
                SysValue::Text(format!("capture:{name}?17")),
                SysValue::Text(format!("capture:{name}?17"))
            ])
            .is_err()
        );
        assert!(snapshots(&[SysValue::Text(format!("capture:{name}?17"))]).is_ok());
        release(&format!("capture:{name}?17")).unwrap();
        assert!(snapshots(&[SysValue::Text(format!("capture:{name}?17"))]).is_err());
        assert_eq!(retained.version, 17);
        assert_eq!(Arc::strong_count(&retained), 1);
        assert!(release(&format!("capture:{name}?17")).is_ok());
        assert!(token("capture:bad?not-version").is_err());
    }
}
