use crate::bitstream::Bits;
use crate::headers::{Coding, Sequence, ALTERNATE, ZIGZAG};
use crate::{tables, Error, Result};

// Cosine weights for the 8-bit Simple IDCT, scaled by sqrt(2) * 2^14
// with quantized DC and pi/4 weights. Rows are indexed by output sample.
const BASIS: [[i32; 8]; 8] = [
    [16383, 22725, 21407, 19266, 16383, 12873, 8867, 4520],
    [16383, 19266, 8867, -4520, -16383, -22725, -21407, -12873],
    [16383, 12873, -8867, -22725, -16383, 4520, 21407, 19266],
    [16383, 4520, -21407, -12873, 16383, 19266, -8867, -22725],
    [16383, -4520, -21407, 12873, 16383, -19266, -8867, 22725],
    [16383, -12873, -8867, 22725, -16383, -4520, 21407, -19266],
    [16383, -19266, 8867, 4520, -16383, 22725, -21407, 12873],
    [16383, -22725, 21407, -19266, 16383, -12873, 8867, -4520],
];
const ROW_SHIFT: u32 = 11;
const COLUMN_SHIFT: u32 = 20;
const COLUMN_BIAS: i16 = 32;

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
        coefficients[0] = dc[plane] << (3 - coding.dc_precision);
        if !(-2048..=2047).contains(&coefficients[0]) {
            return Err(Error::Invalid("intra DC coefficient"));
        }
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
    Ok(inverse_dct(&coefficients))
}

fn dequantize(level: i32, quantizer: i32, weight: u8, intra: bool) -> i32 {
    let magnitude = level.abs();
    let weight = i32::from(weight) * quantizer;
    let magnitude = if intra {
        (magnitude * weight) >> 4
    } else {
        ((2 * magnitude + 1) * weight) >> 5
    };
    (level.signum() * magnitude).clamp(-2048, 2047)
}

fn inverse_dct(coefficients: &[i32; 64]) -> [i16; 64] {
    if coefficients[1..].iter().all(|&coeff| coeff == 0) {
        let dc = coefficients[0] * 8 + i32::from(COLUMN_BIAS);
        return [((dc * BASIS[0][0]) >> COLUMN_SHIFT) as i16; 64];
    }

    // The horizontal pass rounds and clips to signed 16-bit intermediates.
    let mut horizontal = [[0i16; 8]; 8];
    for (row, transformed) in coefficients.as_chunks::<8>().0.iter().zip(&mut horizontal) {
        if row[1..].iter().all(|&coeff| coeff == 0) {
            *transformed = [(row[0] * 8) as i16; 8];
            continue;
        }
        for (output, basis) in transformed.iter_mut().zip(BASIS) {
            let product = row
                .iter()
                .zip(basis)
                .map(|(&coeff, weight)| coeff * weight)
                .sum::<i32>();
            let value = (product + (1 << (ROW_SHIFT - 1))) >> ROW_SHIFT;
            *output = value.clamp(i16::MIN.into(), i16::MAX.into()) as i16;
        }
    }
    // Apply the vertical rounding bias to the DC input before multiplying.
    for dc in &mut horizontal[0] {
        *dc = dc.wrapping_add(COLUMN_BIAS);
    }

    let mut samples = [0; 64];
    for (row, basis) in samples.as_chunks_mut::<8>().0.iter_mut().zip(BASIS) {
        for (x, output) in row.iter_mut().enumerate() {
            let product = horizontal
                .iter()
                .zip(basis)
                .map(|(row, weight)| i32::from(row[x]) * weight)
                .fold(0i32, i32::wrapping_add);
            *output = (product >> COLUMN_SHIFT) as i16;
        }
    }
    samples
}
