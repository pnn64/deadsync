//! VP9 "superframe" splitting (spec "VP9 Bitstream - superframe and uncompressed header" §3).
//!
//! A single container chunk (one IVF frame, one WebM block, etc.) may pack more than one
//! VP9 frame -- commonly one or more hidden frames (`show_frame == 0`, e.g. an altref, or a
//! sequence of intra-only frames priming several DPB slots) followed by one visible frame.
//! This is signaled by a trailing "superframe index": if the chunk's last byte has its top 3
//! bits set to `0b110`, the next 2 bits give `frame_size_length_minus_one` and the low 3 bits
//! give `num_frames_minus_one`; the index (of size `2 + (frame_size_length_minus_one + 1) *
//! (num_frames_minus_one + 1)` bytes, repeating the marker byte at both ends) then lists each
//! frame's byte size (least-significant byte first), and the chunk's payload up to the index
//! is exactly the concatenation of that many consecutive VP9 frames, in decode order.
//!
//! A container reader (e.g. [`crate::ivf::IvfReader`]) hands back one chunk at a time;
//! [`crate::Decoder::push_packet`] takes such a chunk directly and runs it through
//! [`split_superframe`] internally, decoding each contained VP9 frame in turn.

/// Splits one container chunk into the VP9 frame(s) it contains, in decode order.
///
/// If `data` doesn't end with a valid superframe index, returns `data` itself as the sole
/// element (the common case: one chunk == one VP9 frame).
pub fn split_superframe(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    let (frames, count) = parse_superframe_index(data).unwrap_or_else(|| {
        let mut frames = [&[][..]; 8];
        frames[0] = data;
        (frames, 1)
    });
    frames.into_iter().take(count)
}

fn parse_superframe_index(data: &[u8]) -> Option<([&[u8]; 8], usize)> {
    let marker = *data.last()?;
    if marker & 0xe0 != 0xc0 {
        return None;
    }
    let bytes_per_framesize = (((marker >> 3) & 0x3) + 1) as usize;
    let num_frames = ((marker & 0x7) + 1) as usize;
    let index_size = 2 + bytes_per_framesize * num_frames;
    if data.len() < index_size {
        return None;
    }
    let index = &data[data.len() - index_size..];
    // The marker byte is duplicated at the start of the index; a mismatch means this isn't
    // actually a superframe index (spec: "go to the first byte of the superframe index, and
    // check that it matches the last byte of the superframe index").
    if index[0] != marker {
        return None;
    }

    let payload = &data[..data.len() - index_size];
    let mut frames = [&[][..]; 8];
    let mut offset = 0;
    for (slot, len) in frames
        .iter_mut()
        .zip(index[1..].chunks_exact(bytes_per_framesize))
        .take(num_frames)
    {
        let mut size = 0usize;
        for (b, &byte) in len.iter().enumerate() {
            size |= (byte as usize) << (8 * b);
        }
        let end = usize::checked_add(offset, size)?;
        *slot = payload.get(offset..end)?;
        offset = end;
    }
    Some((frames, num_frames))
}
