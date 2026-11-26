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
    let scan = match (predict_ac, top) {
        (false, _) => &ZIGZAG,
        (true, true) => &HORIZONTAL,
        (true, false) => &VERTICAL,
    };
    let scale = if MP42 { 8 } else { dc_scale(quantizer, plane) };
    let mut coeff = [0; 64];
    if separate_dc {
        coeff[0] = dc_difference::<MP42>(bits, plane)?;
    }
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
    let predictor = if MP42 {
        neighbor.dc / 8
    } else {
        (neighbor.dc + scale / 2) / scale
    };
    let dc = (predictor + coeff[0]) * scale;
    if predict_ac {
        for i in 1..8 {
            let (index, value) = if top {
                (i, neighbor.horizontal[i - 1])
            } else {
                (8 * i, neighbor.vertical[i - 1])
            };
            let scaled = value.abs() * neighbor.quantizer;
            let predicted = coeff[index] + value.signum() * ((scaled + quantizer / 2) / quantizer);
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
    let difference = if value < (1 << (size - 1)) {
        value + 1 - (1 << size)
    } else {
        value
    };
    if MP42 && !(-256..=255).contains(&difference) {
        return Err(Error::Invalid("MP42 DC difference"));
    }
    Ok(difference)
}

fn inverse_quantize<const MP42: bool>(level: i32, q: i32) -> i16 {
    let magnitude = 2 * level.abs() * q + ((q - 1) | 1);
    let scaled = level.signum() * magnitude;
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
