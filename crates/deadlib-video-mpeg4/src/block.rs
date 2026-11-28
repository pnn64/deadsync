use crate::{bitstream::Bits, tables, transform, Error, Result};

const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];
const VERTICAL: [usize; 64] = [
    0, 8, 16, 24, 1, 9, 2, 10, 17, 25, 32, 40, 48, 56, 57, 49, 41, 33, 26, 18, 3, 11, 4, 12, 19,
    27, 34, 42, 50, 58, 35, 43, 51, 59, 20, 28, 5, 13, 6, 14, 21, 29, 36, 44, 52, 60, 37, 45, 53,
    61, 22, 30, 7, 15, 23, 31, 38, 46, 54, 62, 39, 47, 55, 63,
];
const HORIZONTAL: [usize; 64] = [
    0, 1, 2, 3, 8, 9, 16, 17, 10, 11, 4, 5, 6, 7, 15, 14, 13, 12, 19, 18, 24, 25, 32, 33, 26, 27,
    20, 21, 22, 23, 28, 29, 30, 31, 34, 35, 40, 41, 48, 49, 42, 43, 36, 37, 38, 39, 44, 45, 46, 47,
    50, 51, 56, 57, 58, 59, 52, 53, 54, 55, 60, 61, 62, 63,
];

#[derive(Clone, Copy)]
pub(crate) struct Predictor {
    dc: i32,
    vertical: [i32; 7],
    horizontal: [i32; 7],
    quantizer: i32,
}

impl Default for Predictor {
    fn default() -> Self {
        Self {
            dc: 1024,
            vertical: [0; 7],
            horizontal: [0; 7],
            quantizer: 1,
        }
    }
}

pub(crate) fn intra<const MP42: bool>(
    bits: &mut Bits<'_>,
    coded: bool,
    separate_dc: bool,
    predict_ac: bool,
    quantizer: i32,
    plane: usize,
    [left, diagonal, above]: [Predictor; 3],
) -> Result<([i16; 64], Predictor)> {
    let left_gradient = (left.dc - diagonal.dc).abs();
    let top_gradient = (diagonal.dc - above.dc).abs();
    // MP42 chooses the top block when the two DC gradients are equal.
    let top = left_gradient < top_gradient || (MP42 && left_gradient == top_gradient);
    let neighbor = if top { above } else { left };
    let scale = if MP42 { 8 } else { dc_scale(quantizer, plane) };
    let predictor = if MP42 {
        neighbor.dc / 8
    } else {
        visual_dc_prediction(neighbor.dc, scale)
    };
    let difference = if separate_dc {
        dc_difference::<MP42>(bits, plane)?
    } else {
        0
    };
    if !coded && !predict_ac {
        let dc = (predictor + difference) * scale;
        let coefficient = if MP42 {
            dc as i16
        } else {
            dc.clamp(-2048, 2047) as i16
        };
        return Ok((
            [transform::inverse_dc(coefficient); 64],
            Predictor {
                dc: i32::from(if MP42 {
                    coefficient
                } else {
                    coefficient.max(0)
                }),
                quantizer,
                ..Predictor::default()
            },
        ));
    }
    let scan = match (predict_ac, top) {
        (false, _) => &ZIGZAG,
        (true, true) => &HORIZONTAL,
        (true, false) => &VERTICAL,
    };
    let mut coeff = [0; 64];
    coeff[0] = difference;
    if coded {
        texture::<MP42, _>(
            bits,
            !MP42 || plane == 0,
            scan,
            usize::from(separate_dc),
            |position, level| {
                coeff[position] = level;
            },
        )?;
    }
    let dc = (predictor + coeff[0]) * scale;
    if predict_ac {
        for i in 1..8 {
            let (index, value) = if top {
                (i, neighbor.horizontal[i - 1])
            } else {
                (8 * i, neighbor.vertical[i - 1])
            };
            let prediction = if neighbor.quantizer == quantizer {
                value
            } else {
                let scaled = value.abs() * neighbor.quantizer;
                value.signum() * ((scaled + quantizer / 2) / quantizer)
            };
            let predicted = coeff[index] + prediction;
            coeff[index] = if MP42 {
                i32::from(predicted as i16)
            } else {
                predicted.clamp(-2048, 2047)
            };
        }
    }
    let state = Predictor {
        dc: if MP42 {
            i32::from(dc as i16)
        } else {
            dc.clamp(0, 2047)
        },
        vertical: core::array::from_fn(|i| coeff[8 * (i + 1)]),
        horizontal: core::array::from_fn(|i| coeff[i + 1]),
        quantizer,
    };
    let mut scaled = coeff.map(|level| inverse_quantize::<MP42>(level, quantizer));
    scaled[0] = if MP42 {
        state.dc
    } else {
        dc.clamp(-2048, 2047)
    } as i16;
    Ok((transform::inverse(&scaled), state))
}

pub(crate) fn inter<const MP42: bool>(bits: &mut Bits<'_>, quantizer: i32) -> Result<[i16; 64]> {
    let mut coeff = [0; 64];
    texture::<MP42, _>(bits, false, &ZIGZAG, 0, |position, level| {
        coeff[position] = inverse_quantize::<MP42>(level, quantizer);
    })?;
    Ok(transform::inverse(&coeff))
}

// Visual DC predictors are in 0..=2047 and scales are in 8..=46.
// With a rounded-up 20-bit reciprocal, numerator * scale < 2^20,
// so the multiplication cannot cross the next integer quotient.
static DC_RECIPROCAL: [i32; 47] = {
    let mut table = [0; 47];
    let mut scale = 8;
    while scale < table.len() {
        table[scale] = ((1 << 20) + scale as i32 - 1) / scale as i32;
        scale += 1;
    }
    table
};

#[inline]
fn visual_dc_prediction(dc: i32, scale: i32) -> i32 {
    ((dc + scale / 2) * DC_RECIPROCAL[scale as usize]) >> 20
}

fn dc_scale(q: i32, plane: usize) -> i32 {
    if q < 5 {
        8
    } else if plane == 0 {
        match q {
            5..=8 => 2 * q,
            9..=24 => q + 8,
            _ => 2 * q - 16,
        }
    } else if q < 25 {
        (q + 13) / 2
    } else {
        q - 6
    }
}

fn dc_difference<const MP42: bool>(bits: &mut Bits<'_>, plane: usize) -> Result<i32> {
    let short = match (MP42, plane == 0) {
        (false, true) => &VISUAL_DC_LUMA,
        (false, false) => &VISUAL_DC_CHROMA,
        (true, true) => &MP42_DC_LUMA,
        (true, false) => &MP42_DC_CHROMA,
    };
    let entry = short[bits.peek(DC_BITS) as usize];
    if entry != 0 {
        bits.skip((entry & 31) as u8)?;
        return Ok(i32::from((entry as i16) >> 5));
    }
    let size = match (MP42, plane == 0) {
        (false, true) => bits.vlc(&tables::DC_LUMA)?,
        (false, false) => bits.vlc(&tables::DC_CHROMA)?,
        (true, true) => bits.inverted_vlc(&tables::DC_LUMA)?,
        (true, false) => bits.inverted_vlc(&tables::DC_CHROMA)?,
    } as u8;
    if size == 0 {
        return Ok(0);
    }
    let value = bits.read(size)? as i32;
    if size > 8 {
        bits.marker()?;
    }
    let difference = signed_dc(value, size);
    if MP42 && !(-256..=255).contains(&difference) {
        return Err(Error::Invalid("MP42 DC difference"));
    }
    Ok(difference)
}

const fn signed_dc(value: i32, size: u8) -> i32 {
    if size != 0 && value < (1 << (size - 1)) {
        value + 1 - (1 << size)
    } else {
        value
    }
}

const DC_BITS: u8 = 10;
const DC_ENTRIES: usize = 1 << DC_BITS;
static VISUAL_DC_LUMA: [u16; DC_ENTRIES] = short_dc(&tables::DC_LUMA, false);
static VISUAL_DC_CHROMA: [u16; DC_ENTRIES] = short_dc(&tables::DC_CHROMA, false);
static MP42_DC_LUMA: [u16; DC_ENTRIES] = short_dc(&tables::DC_LUMA, true);
static MP42_DC_CHROMA: [u16; DC_ENTRIES] = short_dc(&tables::DC_CHROMA, true);

// Decode the size prefix together with its signed DC difference.
// Microsoft MPEG-4 uses an inverted size prefix.
const fn short_dc<const N: usize>(codes: &[u16; N], invert: bool) -> [u16; DC_ENTRIES] {
    let mut short = [0; DC_ENTRIES];
    let mut prefix = 0;
    while prefix < DC_ENTRIES {
        let code = if invert {
            (DC_ENTRIES - 1) ^ prefix
        } else {
            prefix
        };
        let entry = codes[code << (N.trailing_zeros() - DC_BITS as u32)];
        let size = (entry >> 5) as u8;
        let bits = (entry & 31) as u8 + size;
        if entry != 0 && bits <= DC_BITS {
            let value = (prefix >> (DC_BITS - bits)) & ((1 << size) - 1);
            short[prefix] = (signed_dc(value as i32, size) as u16) << 5 | bits as u16;
        }
        prefix += 1;
    }
    short
}

fn inverse_quantize<const MP42: bool>(level: i32, q: i32) -> i16 {
    let scaled = 2 * level * q + level.signum() * ((q - 1) | 1);
    if MP42 {
        scaled as i16
    } else {
        scaled.clamp(-2048, 2047) as i16
    }
}

type Event = (usize, i32, bool);
const ESCAPE: usize = 102;

struct EscapeLimits {
    run: [[usize; 32]; 2],
    level: [[i32; 64]; 2],
}

const fn escape_limits(events: &[Event; ESCAPE]) -> EscapeLimits {
    let mut limits = EscapeLimits {
        run: [[0; 32]; 2],
        level: [[0; 64]; 2],
    };
    let mut index = 0;
    while index < events.len() {
        let (run, level, last) = events[index];
        let last = last as usize;
        if limits.run[last][level as usize] < run {
            limits.run[last][level as usize] = run;
        }
        if limits.level[last][run] < level {
            limits.level[last][run] = level;
        }
        index += 1;
    }
    limits
}

static INTRA_LIMITS: EscapeLimits = escape_limits(&tables::INTRA_VALUES);
static INTER_LIMITS: EscapeLimits = escape_limits(&tables::INTER_VALUES);

const SHORT_BITS: u8 = 9;
const SHORT_SIZE: usize = 1 << SHORT_BITS;

static SHORT_INTRA: [u32; SHORT_SIZE] = short_events(&tables::INTRA, &tables::INTRA_VALUES);
static SHORT_INTER: [u32; SHORT_SIZE] = short_events(&tables::INTER, &tables::INTER_VALUES);

// The short table includes the sign bit along with the length, run and last flag.
const fn short_events(codes: &[u16; 4096], events: &[Event; ESCAPE]) -> [u32; SHORT_SIZE] {
    let mut short = [0; SHORT_SIZE];
    let mut prefix = 0;
    while prefix < SHORT_SIZE {
        let entry = codes[prefix << (12 - SHORT_BITS)];
        let length = entry & 31;
        let symbol = (entry >> 5) as usize;
        if length > 0 && length < SHORT_BITS as u16 && symbol < ESCAPE {
            let (run, level, last) = events[symbol];
            let sign = (prefix >> (SHORT_BITS as u16 - 1 - length)) & 1;
            let level = if sign == 0 { level } else { -level };
            short[prefix] = (length as u32 + 1)
                | ((run as u32) << 4)
                | ((last as u32) << 10)
                | ((level as u32) << 11);
        }
        prefix += 1;
    }
    short
}

#[inline]
fn texture<const MP42: bool, F: FnMut(usize, i32)>(
    bits: &mut Bits<'_>,
    intra: bool,
    scan: &[usize; 64],
    mut next: usize,
    mut write: F,
) -> Result<()> {
    let (short, codes, values, limits) = if intra {
        (
            &SHORT_INTRA,
            &tables::INTRA,
            &tables::INTRA_VALUES,
            &INTRA_LIMITS,
        )
    } else {
        (
            &SHORT_INTER,
            &tables::INTER,
            &tables::INTER_VALUES,
            &INTER_LIMITS,
        )
    };
    loop {
        let entry = short[bits.peek(SHORT_BITS) as usize];
        let (run, level, last) = if entry != 0 {
            bits.skip((entry & 15) as u8)?;
            (
                ((entry >> 4) & 63) as usize,
                (entry as i32) >> 11,
                entry & 1024 != 0,
            )
        } else {
            read_long::<MP42>(bits, codes, values, limits)?
        };
        let position = *scan
            .get(next + run)
            .ok_or(Error::Invalid("coefficient run exceeds block"))?;
        write(position, level);
        if last {
            return Ok(());
        }
        next += run + 1;
    }
}

#[cold]
fn read_long<const MP42: bool>(
    bits: &mut Bits<'_>,
    codes: &[u16; 4096],
    values: &[Event; ESCAPE],
    limits: &EscapeLimits,
) -> Result<Event> {
    let symbol = bits.vlc(codes)?;
    if symbol == ESCAPE {
        read_escape::<MP42>(bits, codes, values, limits)
    } else {
        let (run, level, last) = values[symbol];
        Ok((run, if bits.flag()? { -level } else { level }, last))
    }
}

fn read_escape<const MP42: bool>(
    bits: &mut Bits<'_>,
    codes: &[u16; 4096],
    values: &[Event; ESCAPE],
    limits: &EscapeLimits,
) -> Result<Event> {
    // MP42 reverses both escape selector bits and uses an 8-bit literal.
    let extended = bits.flag()? ^ MP42;
    if extended && (bits.flag()? ^ MP42) {
        let last = bits.flag()?;
        let run = bits.read(6)? as usize;
        if MP42 {
            let level = i32::from(bits.read(8)? as i8);
            if level == 0 {
                return Err(Error::Invalid("escaped MP42 coefficient level"));
            }
            return Ok((run, level, last));
        }
        bits.marker()?;
        let raw = bits.read(12)? as i32;
        bits.marker()?;
        let level = (raw ^ 0x800) - 0x800;
        if level == 0 || level == -2048 {
            return Err(Error::Invalid("escaped coefficient level"));
        }
        return Ok((run, level, last));
    }
    let (mut run, mut level, last) = *values
        .get(bits.vlc(codes)?)
        .ok_or(Error::Invalid("nested coefficient escape"))?;
    if extended {
        run += usize::from(!MP42) + limits.run[usize::from(last)][level as usize];
    } else {
        level += limits.level[usize::from(last)][run];
    }
    Ok((run, if bits.flag()? { -level } else { level }, last))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_dc_reciprocal_matches_all_legal_divisions() {
        for scale in 8..=46 {
            for dc in 0..=2047 {
                assert_eq!(visual_dc_prediction(dc, scale), (dc + scale / 2) / scale);
            }
        }
    }

    #[test]
    fn ac_prediction_preserves_equal_quantizer_and_rescales_others() {
        let left = Predictor {
            vertical: [-2048, -3, -1, 0, 1, 3, 2047],
            quantizer: 3,
            ..Predictor::default()
        };
        let boundary = Predictor {
            dc: 0,
            ..Predictor::default()
        };
        for (q, expected) in [
            (3, [-2048, -3, -1, 0, 1, 3, 2047]),
            (2, [-2048, -5, -2, 0, 2, 5, 2047]),
        ] {
            let (_, state) = intra::<false>(
                &mut Bits::new(&[]),
                false,
                false,
                true,
                q,
                0,
                [left, boundary, boundary],
            )
            .unwrap();
            assert_eq!(state.vertical, expected);
            assert_eq!(state.horizontal, [0; 7]);
        }
    }

    #[test]
    fn signed_inverse_quantization_matches_magnitude_formula() {
        for q in 1..=31 {
            for level in i32::from(i16::MIN)..=i32::from(i16::MAX) {
                let expected = level.signum() * (2 * level.abs() * q + ((q - 1) | 1));
                assert_eq!(inverse_quantize::<true>(level, q), expected as i16);
                assert_eq!(
                    inverse_quantize::<false>(level, q),
                    expected.clamp(-2048, 2047) as i16
                );
            }
        }
    }

    #[test]
    fn uncoded_visual_dc_clips_prediction_and_samples_separately() {
        for (dc, encoded, coefficient, expected_dc) in [
            (0, 0b1100_0000, -8, 0),
            (2047, 0b1110_0000, 2047, 2047),
            (1024, 0b0110_0000, 1024, 1024),
        ] {
            let neighbors = [Predictor {
                dc,
                ..Predictor::default()
            }; 3];
            let mut bits = Bits::new(core::slice::from_ref(&encoded));
            let (samples, state) =
                intra::<false>(&mut bits, false, true, false, 1, 0, neighbors).unwrap();
            let mut coeff = [0; 64];
            coeff[0] = coefficient;
            assert_eq!(samples, transform::inverse(&coeff));
            assert_eq!(state.dc, expected_dc);
            assert_eq!(state.vertical, [0; 7]);
            assert_eq!(state.horizontal, [0; 7]);
            assert_eq!(state.quantizer, 1);
        }
    }

    #[test]
    fn visual_dc_lookup_matches_size_then_magnitude() {
        fn reference(bits: &mut Bits<'_>, plane: usize) -> Result<i32> {
            let size = if plane == 0 {
                bits.vlc(&tables::DC_LUMA)?
            } else {
                bits.vlc(&tables::DC_CHROMA)?
            } as u8;
            if size == 0 {
                return Ok(0);
            }
            let value = bits.read(size)? as i32;
            if size > 8 {
                bits.marker()?;
            }
            Ok(signed_dc(value, size))
        }
        for prefix in 0..=u16::MAX {
            let bytes = prefix.to_be_bytes();
            for input in [&bytes[..1], &bytes[..]] {
                for plane in 0..=1 {
                    let mut a = Bits::new(input);
                    let mut b = a;
                    let expected = reference(&mut a, plane);
                    assert_eq!(dc_difference::<false>(&mut b, plane), expected);
                    if expected.is_ok() {
                        assert_eq!(a.peek(16), b.peek(16));
                        assert_eq!(a.alignment_stuffing(), b.alignment_stuffing());
                    }
                }
            }
        }
    }
}
