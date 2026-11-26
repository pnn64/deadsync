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

pub(crate) fn intra(
    bits: &mut Bits<'_>,
    coded: bool,
    separate_dc: bool,
    predict_ac: bool,
    quantizer: i32,
    plane: usize,
    [left, diagonal, above]: [Predictor; 3],
) -> Result<([i16; 64], Predictor)> {
    let top = (left.dc - diagonal.dc).abs() < (diagonal.dc - above.dc).abs();
    let neighbor = if top { above } else { left };
    let scan = match (predict_ac, top) {
        (false, _) => &ZIGZAG,
        (true, true) => &HORIZONTAL,
        (true, false) => &VERTICAL,
    };
    let scale = dc_scale(quantizer, plane);
    let mut coeff = [0; 64];
    if separate_dc {
        coeff[0] = dc_difference(bits, plane)?;
    }
    if coded {
        texture(bits, true, &mut coeff, scan, usize::from(separate_dc))?;
    }
    let dc = ((neighbor.dc + scale / 2) / scale + coeff[0]) * scale;
    if predict_ac {
        for i in 1..8 {
            let (index, value) = if top {
                (i, neighbor.horizontal[i - 1])
            } else {
                (8 * i, neighbor.vertical[i - 1])
            };
            let scaled = value.abs() * neighbor.quantizer;
            coeff[index] = (coeff[index] + value.signum() * ((scaled + quantizer / 2) / quantizer))
                .clamp(-2048, 2047);
        }
    }
    let state = Predictor {
        dc: dc.clamp(0, 2047),
        vertical: core::array::from_fn(|i| coeff[8 * (i + 1)]),
        horizontal: core::array::from_fn(|i| coeff[i + 1]),
        quantizer,
    };
    let mut scaled = coeff.map(|level| inverse_quantize(level, quantizer));
    scaled[0] = dc.clamp(-2048, 2047) as i16;
    Ok((transform::inverse(&scaled), state))
}

pub(crate) fn inter(bits: &mut Bits<'_>, quantizer: i32) -> Result<[i16; 64]> {
    let mut coeff = [0; 64];
    texture(bits, false, &mut coeff, &ZIGZAG, 0)?;
    Ok(transform::inverse(
        &coeff.map(|level| inverse_quantize(level, quantizer)),
    ))
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

fn dc_difference(bits: &mut Bits<'_>, plane: usize) -> Result<i32> {
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
    Ok(if value < (1 << (size - 1)) {
        value + 1 - (1 << size)
    } else {
        value
    })
}

fn inverse_quantize(level: i32, q: i32) -> i16 {
    let magnitude = 2 * level.abs() * q + ((q - 1) | 1);
    (level.signum() * magnitude).clamp(-2048, 2047) as i16
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

fn texture(
    bits: &mut Bits<'_>,
    intra: bool,
    coeff: &mut [i32; 64],
    scan: &[usize; 64],
    mut next: usize,
) -> Result<()> {
    let (codes, values, limits) = if intra {
        (&tables::INTRA, &tables::INTRA_VALUES, &INTRA_LIMITS)
    } else {
        (&tables::INTER, &tables::INTER_VALUES, &INTER_LIMITS)
    };
    loop {
        let symbol = bits.vlc(codes)?;
        let (run, level, last) = if symbol == ESCAPE {
            read_escape(bits, codes, values, limits)?
        } else {
            let (run, level, last) = values[symbol];
            (run, if bits.flag()? { -level } else { level }, last)
        };
        let position = *scan
            .get(next + run)
            .ok_or(Error::Invalid("coefficient run exceeds block"))?;
        coeff[position] = level;
        if last {
            return Ok(());
        }
        next += run + 1;
    }
}

fn read_escape(
    bits: &mut Bits<'_>,
    codes: &[u16; 4096],
    values: &[Event; ESCAPE],
    limits: &EscapeLimits,
) -> Result<Event> {
    let extended_run = bits.flag()?;
    if extended_run && bits.flag()? {
        let last = bits.flag()?;
        let run = bits.read(6)? as usize;
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
    if extended_run {
        run += 1 + limits.run[usize::from(last)][level as usize];
    } else {
        level += limits.level[usize::from(last)][run];
    }
    Ok((run, if bits.flag()? { -level } else { level }, last))
}
