use crate::nal::{annex_b_units, find_start_code, parse_nal};
use crate::{DecodeError, Decoder, Frame};

/// Decodes a complete Annex B stream and emits its frames in display order.
pub fn decode_annex_b(data: &[u8], mut output: impl FnMut(Frame)) -> Result<(), DecodeError> {
    let (start, prefix) =
        find_start_code(data, 0).ok_or(DecodeError::InvalidSyntax("missing Annex B start code"))?;
    if data[..prefix].iter().any(|&byte| byte != 0) {
        return Err(DecodeError::InvalidSyntax(
            "bytes before Annex B start code",
        ));
    }

    let mut decoder = Decoder::new().with_display_order();
    for payload in annex_b_units(data, start) {
        feed(&mut decoder, payload, &mut output)?;
    }
    while let Some(frame) = decoder.flush()? {
        output(frame);
    }
    Ok(())
}

fn feed(
    decoder: &mut Decoder,
    data: &[u8],
    output: &mut impl FnMut(Frame),
) -> Result<(), DecodeError> {
    let nal = parse_nal(data).ok_or(DecodeError::InvalidSyntax("NAL unit header"))?;
    if let Some(frame) = decoder.decode_nal(&nal)? {
        output(frame);
    }
    while let Some(frame) = decoder.next_output() {
        output(frame);
    }
    Ok(())
}

impl Decoder {
    /// Feeds the parameter sets from an HEVC decoder configuration record (`hvcC`).
    ///
    /// Pass the returned NAL length-field size to [`Self::push_packet`].
    /// Feed this record before the packets that use it.
    pub fn push_decoder_config(&mut self, mut data: &[u8]) -> Result<u8, DecodeError> {
        let header = take(&mut data, 23)?;
        if header[0] != 1 {
            return Err(DecodeError::Unsupported(
                "HEVC configuration record version",
            ));
        }
        for _ in 0..header[22] {
            let nal_type = take(&mut data, 1)?[0] & 0x3f;
            if nal_type < 32 {
                return Err(DecodeError::InvalidSyntax(
                    "slice in HEVC configuration record",
                ));
            }
            let count = read_length(&mut data, 2)?;
            for _ in 0..count {
                let length = read_length(&mut data, 2)?;
                let bytes = take(&mut data, length)?;
                let nal =
                    parse_nal(bytes).ok_or(DecodeError::InvalidSyntax("configuration NAL unit"))?;
                if ((bytes[0] >> 1) & 0x3f) != nal_type {
                    return Err(DecodeError::InvalidSyntax("configuration NAL unit type"));
                }
                self.decode_config_nal(&nal)?;
            }
        }
        if !data.is_empty() {
            return Err(DecodeError::InvalidSyntax(
                "trailing bytes in HEVC configuration record",
            ));
        }
        Ok((header[21] & 3) + 1)
    }

    /// Feeds one container packet containing length-prefixed NAL units.
    ///
    /// Use [`Self::with_display_order`] to emit the reordered frames. Call
    /// [`Self::flush`] until it returns `Ok(None)` after the last packet.
    pub fn push_packet(
        &mut self,
        mut packet: &[u8],
        length_size: u8,
        mut output: impl FnMut(Frame),
    ) -> Result<(), DecodeError> {
        if !(1..=4).contains(&length_size) {
            return Err(DecodeError::InvalidSyntax("NAL length-field size"));
        }
        if packet.is_empty() {
            return Err(DecodeError::InvalidSyntax("empty HEVC packet"));
        }
        while !packet.is_empty() {
            let length = read_length(&mut packet, length_size)?;
            feed(self, take(&mut packet, length)?, &mut output)?;
        }
        Ok(())
    }
}

fn take<'a>(data: &mut &'a [u8], n: usize) -> Result<&'a [u8], DecodeError> {
    let (front, rest) = data.split_at_checked(n).ok_or(DecodeError::UnexpectedEof)?;
    *data = rest;
    Ok(front)
}

fn read_length(data: &mut &[u8], size: u8) -> Result<usize, DecodeError> {
    let bytes = take(data, usize::from(size))?;
    Ok(bytes
        .iter()
        .fold(0, |value, &byte| (value << 8) | usize::from(byte)))
}
