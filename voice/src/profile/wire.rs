//! Frames between the voice service and the high-profile worker on a private Unix socket:
//! a 4-byte big-endian header length, a JSON header, then `header.bytes` bytes of body.
use serde_json::Value;
use std::io::{self, Read, Write};

pub(super) const PROTOCOL: u64 = 1;
const HEADER_LIMIT: usize = 64 * 1024;
/// One minute of 48 kHz float32 audio.
pub(super) const BODY_LIMIT: usize = 48_000 * 60 * 4;

#[derive(Debug, thiserror::Error)]
pub(super) enum WireError {
    #[error("worker connection failed")]
    Io(#[from] io::Error),
    #[error("worker frame header of {0} bytes exceeds 64 KiB")]
    HeaderTooLarge(usize),
    #[error("worker frame body of {0} bytes exceeds the limit")]
    BodyTooLarge(u64),
    #[error("worker sent invalid JSON")]
    Json(#[from] serde_json::Error),
}

pub(super) fn write_frame(
    out: &mut impl Write,
    mut header: Value,
    body: &[u8],
) -> Result<(), WireError> {
    if !body.is_empty() {
        header["bytes"] = body.len().into();
    }
    let header = serde_json::to_vec(&header)?;
    if header.len() > HEADER_LIMIT {
        return Err(WireError::HeaderTooLarge(header.len()));
    }
    let length =
        u32::try_from(header.len()).map_err(|_| WireError::HeaderTooLarge(header.len()))?;
    let mut frame = Vec::with_capacity(4 + header.len() + body.len());
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&header);
    frame.extend_from_slice(body);
    out.write_all(&frame)?;
    out.flush()?;
    Ok(())
}

pub(super) fn read_frame(input: &mut impl Read) -> Result<(Value, Vec<u8>), WireError> {
    let mut length = [0; 4];
    input.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > HEADER_LIMIT {
        return Err(WireError::HeaderTooLarge(length));
    }
    let mut header = vec![0; length];
    input.read_exact(&mut header)?;
    let header: Value = serde_json::from_slice(&header)?;
    let size = header["bytes"].as_u64().unwrap_or(0);
    if size > BODY_LIMIT as u64 {
        return Err(WireError::BodyTooLarge(size));
    }
    let mut body = vec![0; size as usize];
    input.read_exact(&mut body)?;
    Ok((header, body))
}

pub(super) fn samples(body: &[u8]) -> Vec<f32> {
    body.chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

pub(super) fn bytes(samples: &[f32]) -> Vec<u8> {
    samples.iter().flat_map(|s| s.to_le_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frames_round_trip_and_bound_their_parts() {
        let mut buffer = Vec::new();
        let audio = bytes(&[0.5, -0.25]);
        write_frame(&mut buffer, json!({"type":"audio","rate":24000}), &audio).unwrap();
        write_frame(&mut buffer, json!({"type":"done"}), &[]).unwrap();
        let mut input = io::Cursor::new(buffer);
        let (header, body) = read_frame(&mut input).unwrap();
        assert_eq!(
            (header["type"].as_str(), header["bytes"].as_u64()),
            (Some("audio"), Some(8))
        );
        assert_eq!(samples(&body), vec![0.5, -0.25]);
        assert_eq!(read_frame(&mut input).unwrap().0["type"], "done");
        assert!(matches!(read_frame(&mut input), Err(WireError::Io(_))));

        let mut huge = io::Cursor::new(u32::MAX.to_be_bytes().to_vec());
        assert!(matches!(
            read_frame(&mut huge),
            Err(WireError::HeaderTooLarge(_))
        ));
        let mut oversized = Vec::new();
        write_frame(&mut oversized, json!({"bytes":BODY_LIMIT + 1}), &[]).unwrap();
        assert!(matches!(
            read_frame(&mut io::Cursor::new(oversized)),
            Err(WireError::BodyTooLarge(_))
        ));
    }
}
