use crate::bitstream::Bits;
use crate::headers::{Coding, Sequence, ALTERNATE, ZIGZAG};
use crate::{tables, transform, Error, Result};

pub(crate) fn decode(
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
        coefficients[0] = dequantize(level, quantizer, matrix[0], false);
        next = 1;
    }

    let (short, long) = if intra && coding.intra_vlc {
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
                let escaped = bits.read(18)?;
                let run = (escaped >> 12) as usize;
                let encoded = (escaped & 0xfff) as i32;
                let level = (encoded ^ 0x800) - 0x800;
                if level == 0 || level == -2048 {
                    return Err(Error::Invalid("escaped coefficient level"));
                }
                (run, level)
            } else {
                (run, level)
            }
        };
        let position = *scan
            .get(next + run)
            .ok_or(Error::Invalid("coefficient run exceeds block"))?;
        let coefficient = dequantize(level, quantizer, matrix[position], intra);
        coefficients[position] = coefficient;
        parity ^= coefficient;
        next += run + 1;
    }

    // Mismatch control makes the sum of the saturated coefficients odd.
    coefficients[63] ^= (parity ^ 1) & 1;
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

fn dequantize(level: i32, quantizer: i32, weight: u8, intra: bool) -> i16 {
    let magnitude = level.abs();
    let weight = i32::from(weight) * quantizer;
    let magnitude = if intra {
        (magnitude * weight) >> 4
    } else {
        ((2 * magnitude + 1) * weight) >> 5
    };
    (level.signum() * magnitude).clamp(-2048, 2047) as i16
}
