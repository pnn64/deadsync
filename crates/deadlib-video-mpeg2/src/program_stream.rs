use alloc::vec::Vec;

use crate::bitstream::find_prefix;
use crate::{Error, Result};

/// Returns the concatenated video PES payloads of the first video stream.
///
/// Accepts MPEG-1 and MPEG-2 pack and PES headers. The returned payload is
/// an elementary stream and can be passed to [`crate::decode_elementary_stream`].
pub fn video_elementary_stream(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut video = Vec::new();
    let mut video_id = None;
    let mut cursor = 0;
    while let Some(start) = find_prefix(bytes, cursor) {
        let packet = &bytes[start..];
        let id = *packet.get(3).ok_or(Error::Truncated)?;
        cursor = match id {
            0xb9 => start + 4,
            0xba => {
                let size = match packet.get(4).ok_or(Error::Truncated)? {
                    first if first & 0xc0 == 0x40 => {
                        14 + usize::from(packet.get(13).ok_or(Error::Truncated)? & 7)
                    }
                    first if first & 0xf0 == 0x20 => 12,
                    _ => return Err(Error::Invalid("program pack header")),
                };
                packet.get(..size).ok_or(Error::Truncated)?;
                start + size
            }
            0xbb..=0xff => {
                let encoded_length = packet
                    .get(4..)
                    .and_then(|bytes| bytes.first_chunk())
                    .ok_or(Error::Truncated)?;
                let length = usize::from(u16::from_be_bytes(*encoded_length));
                let is_video = (0xe0..=0xef).contains(&id);
                let end = if length == 0 && is_video {
                    end_of_unbounded_packet(bytes, start + 6)
                } else {
                    start + 6 + length
                };
                let payload = bytes.get(start + 6..end).ok_or(Error::Truncated)?;
                if is_video && id == *video_id.get_or_insert(id) {
                    video.extend_from_slice(pes_payload(payload)?);
                }
                end
            }
            _ => return Err(Error::Invalid("program stream packet start code")),
        };
    }
    if video.is_empty() {
        return Err(Error::NoVideo);
    }
    Ok(video)
}

fn end_of_unbounded_packet(bytes: &[u8], mut cursor: usize) -> usize {
    while let Some(start) = find_prefix(bytes, cursor) {
        if bytes.get(start + 3).is_some_and(|&id| id >= 0xb9) {
            return start;
        }
        cursor = start + 3;
    }
    bytes.len()
}

fn pes_payload(mut bytes: &[u8]) -> Result<&[u8]> {
    let first = *bytes.first().ok_or(Error::Truncated)?;
    if first & 0xc0 == 0x80 {
        let header_length = usize::from(*bytes.get(2).ok_or(Error::Truncated)?);
        return bytes.get(3 + header_length..).ok_or(Error::Truncated);
    }

    while let Some((&0xff, tail)) = bytes.split_first() {
        bytes = tail;
    }
    if bytes.first().is_some_and(|first| first & 0xc0 == 0x40) {
        bytes = bytes.get(2..).ok_or(Error::Truncated)?;
    }
    let first = *bytes.first().ok_or(Error::Truncated)?;
    let skip = match first {
        0x20..=0x2f => 5,  // PTS.
        0x30..=0x3f => 10, // PTS and DTS.
        0x0f => 1,
        _ => return Err(Error::Invalid("PES header")),
    };
    bytes.get(skip..).ok_or(Error::Truncated)
}
