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

    let table = if intra && coding.intra_vlc {
        &tables::COEFFICIENT_ONE
    } else {
        &tables::COEFFICIENT_ZERO
    };
    loop {
        let symbol = bits.vlc(table)?;
        let (run, level) = match symbol {
            tables::END_OF_BLOCK => break,
            tables::ESCAPE => {
                let run = bits.read(6)? as usize;
                let encoded = bits.read(12)? as i32;
                let level = (encoded ^ 0x800) - 0x800;
                if level == 0 || level == -2048 {
                    return Err(Error::Invalid("escaped coefficient level"));
                }
                (run, level)
            }
            _ => {
                let (run, level) = tables::RUN_LEVEL[symbol];
                (run, if bits.flag()? { -level } else { level })
            }
        };
        let position = *scan
            .get(next + run)
            .ok_or(Error::Invalid("coefficient run exceeds block"))?;
        coefficients[position] = dequantize(level, quantizer, matrix[position], intra);
        next += run + 1;
    }

    // Mismatch control makes the sum of the saturated coefficients odd.
    let parity = coefficients.iter().fold(0, |parity, coeff| parity ^ coeff);
    coefficients[63] ^= (parity ^ 1) & 1;
    Ok(transform::inverse(&coefficients))
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
