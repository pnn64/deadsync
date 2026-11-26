use crate::bitstream::Bits;
use crate::headers::{Coding, Sequence, ALTERNATE, ZIGZAG};
use crate::{tables, transform, Error, Result};

pub(crate) fn decode<const MPEG2: bool>(
    bits: &mut Bits<'_>,
    coding: &Coding,
    sequence: &Sequence,
    dc: &mut [i32; 3],
    plane: usize,
    quantizer: i32,
    intra: bool,
) -> Result<[i16; 64]> {
    let matrix = &sequence.matrices[usize::from(!intra) + 2 * usize::from(plane != 0)];
    let scan = if coding.alternate_scan {
        &ALTERNATE
    } else {
        &ZIGZAG
    };
    let mut coefficients = [0; 64];
    let mut next = 0;
    if intra {
        let size = if plane == 0 {
            bits.vlc(&tables::DC_LUMA)?
        } else {
            bits.vlc(&tables::DC_CHROMA)?
        } as u8;
        if size != 0 {
            let value = bits.read(size)? as i32;
            let difference = if value < (1 << (size - 1)) {
                value + 1 - (1 << size)
            } else {
                value
            };
            dc[plane] += difference;
        }
        let dc = dc[plane] << (3 - coding.dc_precision);
        if !(-2048..=2047).contains(&dc) {
            return Err(Error::Invalid("intra DC coefficient"));
        }
        coefficients[0] = dc as i16;
        next = 1;
    } else if bits.peek(1) != 0 {
        bits.skip(1)?;
        let level = if bits.flag()? { -1 } else { 1 };
        coefficients[0] = dequantize::<MPEG2>(level, quantizer, matrix[0], false);
        next = 1;
    }

    let (short, long) = if MPEG2 && intra && coding.intra_vlc {
        (&tables::SHORT_ONE, &tables::COEFFICIENT_ONE)
    } else {
        (&tables::SHORT_ZERO, &tables::COEFFICIENT_ZERO)
    };
    let mut parity = coefficients[0];
    loop {
        let entry = short[bits.peek(9) as usize];
        let (run, level) = if entry == 0 {
            read_long(bits, long)?
        } else {
            bits.skip((entry & 15) as u8)?;
            let run = usize::from((entry >> 4) & 31);
            let level = i32::from((entry as i16) >> 9);
            if level == 0 {
                if run == 0 {
                    break;
                }
                if MPEG2 {
                    let escaped = bits.read(18)?;
                    let run = (escaped >> 12) as usize;
                    let encoded = (escaped & 0xfff) as i32;
                    let level = (encoded ^ 0x800) - 0x800;
                    if level == 0 || level == -2048 {
                        return Err(Error::Invalid("escaped coefficient level"));
                    }
                    (run, level)
                } else {
                    read_mpeg1_escape(bits)?
                }
            } else {
                (run, level)
            }
        };
        let position = *scan
            .get(next + run)
            .ok_or(Error::Invalid("coefficient run exceeds block"))?;
        let coefficient = dequantize::<MPEG2>(level, quantizer, matrix[position], intra);
        coefficients[position] = coefficient;
        parity ^= coefficient;
        next += run + 1;
    }

    if MPEG2 {
        // MPEG-2 mismatch control makes the sum of the saturated coefficients odd.
        coefficients[63] ^= (parity ^ 1) & 1;
    }
    Ok(transform::inverse(&coefficients))
}

#[cold]
fn read_long(bits: &mut Bits<'_>, table: &[u16; 65536]) -> Result<(usize, i32)> {
    let symbol = bits.vlc(table)?;
    let &(run, level) = tables::RUN_LEVEL
        .get(symbol)
        .ok_or(Error::Invalid("long coefficient code"))?;
    Ok((run, if bits.flag()? { -level } else { level }))
}

fn read_mpeg1_escape(bits: &mut Bits<'_>) -> Result<(usize, i32)> {
    let escaped = bits.read(14)?;
    let run = (escaped >> 8) as usize;
    let level = match escaped as u8 {
        0 => bits.read(8)? as i32,
        128 => bits.read(8)? as i32 - 256,
        byte => i32::from(byte as i8),
    };
    if level == 0 || level == -256 {
        return Err(Error::Invalid("escaped coefficient level"));
    }
    Ok((run, level))
}

fn dequantize<const MPEG2: bool>(level: i32, quantizer: i32, weight: u8, intra: bool) -> i16 {
    let magnitude = level.abs();
    let weight = i32::from(weight) * quantizer;
    let magnitude = if intra {
        (magnitude * weight) >> 4
    } else {
        ((2 * magnitude + 1) * weight) >> 5
    };
    // MPEG-1 oddifies each inverse-quantized AC coefficient before saturation.
    let magnitude = if MPEG2 {
        magnitude
    } else {
        (magnitude - 1) | 1
    };
    (level.signum() * magnitude).clamp(-2048, 2047) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mpeg1_escape_reads_both_lengths_and_signs() {
        let run = 23;
        for (code, width, level) in [
            (0x7f, 8, 127),
            (0x81, 8, -127),
            (0x0080, 16, 128),
            (0x8080, 16, -128),
            (0x00ff, 16, 255),
            (0x8001, 16, -255),
        ] {
            let encoded = (((run as u32) << width | code) << (26 - width)).to_be_bytes();
            assert_eq!(
                read_mpeg1_escape(&mut Bits::new(&encoded)),
                Ok((run, level))
            );
        }
        let extended = [0; 2];
        assert_eq!(
            read_mpeg1_escape(&mut Bits::new(&extended)),
            Err(Error::Truncated)
        );
    }
}
