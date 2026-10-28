use crate::{
    ChannelMode, Error, FrameHeader, GranuleChannel, MpegVersion, bands::Bands, bits::BitReader,
    huffman, scalefactors::ScaleFactors,
};
const PRETAB: [u8; 22] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 3, 3, 3, 2, 0,
];
static POWERS: std::sync::LazyLock<[f64; 16]> = std::sync::LazyLock::new(|| {
    // Keep the same runtime powf path as uncached coefficients; constant-folded
    // powers can round differently from the platform math implementation.
    std::array::from_fn(|n| std::hint::black_box(n as f64).powf(4.0 / 3.0))
});

static MPEG1_INTENSITY: std::sync::LazyLock<[(f64, f64); 5]> = std::sync::LazyLock::new(|| {
    std::array::from_fn(|n| {
        // Use runtime tan so cached gains retain the uncached math rounding.
        let position = std::hint::black_box((n + 1) as f64);
        let ratio = (std::f64::consts::PI * position / 12.0).tan();
        (ratio / (1.0 + ratio), 1.0 / (1.0 + ratio))
    })
});

/// Decodes big-values pairs and count1 quads. Incomplete final count1 words are
/// stuffing; incomplete big-values words are errors. On error, output is invalid
/// and the bit cursor remains at its original position.
pub fn quantized(
    bits: &mut BitReader<'_>,
    info: &GranuleChannel,
    bands: Bands,
    output: &mut [i16; 576],
) -> Result<usize, Error> {
    if info.big_values > 288 || info.block_type > 3 {
        return Err(Error::Invalid("spectral granule fields"));
    }
    let (first, second) = if info.block_type == 2 {
        let first = if info.mixed_block {
            // Mixed region zero has eight band entries. LSF uses six long
            // bands, followed by short band 3 in the first two windows.
            bands.mixed_lines()
                + (8 - bands.mixed_long_bands()) * (bands.short()[4] - bands.short()[3]) as usize
        } else {
            3 * bands.short()[3] as usize
        };
        (first, 576)
    } else if info.block_type != 0 {
        (
            bands
                .long()
                .get(info.region0_count as usize + 1)
                .copied()
                .unwrap_or(576) as usize,
            576,
        )
    } else {
        (
            bands
                .long()
                .get(info.region0_count as usize + 1)
                .copied()
                .unwrap_or(576) as usize,
            bands
                .long()
                .get(info.region0_count as usize + info.region1_count as usize + 2)
                .copied()
                .unwrap_or(576) as usize,
        )
    };
    let mut cursor = *bits;
    output.fill(0);
    let mut line = 0;
    let limit = (info.big_values as usize) * 2;
    for (region, end) in [first.min(limit), second.min(limit), limit]
        .into_iter()
        .enumerate()
    {
        let book = info.table_select[region];
        if book == 0 {
            // The output is already zeroed; book zero consumes no bits.
            line = end;
            continue;
        }
        while line < end {
            let pair = huffman::pair(&mut cursor, book)?;
            output[line..line + 2].copy_from_slice(&pair);
            line += 2;
        }
    }
    while line + 4 <= 576 && cursor.remaining() != 0 {
        match huffman::quad(&mut cursor, info.count1_table) {
            Ok(quad) => {
                output[line..line + 4].copy_from_slice(&quad);
                line += 4;
            }
            Err(Error::Truncated) => break,
            Err(error) => return Err(error),
        }
    }
    *bits = cursor;
    Ok(line)
}
fn power(value: i16, scale: f64, powers: &[f64; 16]) -> f64 {
    match value {
        0 => 0.0,
        1 => scale,
        -1 => -scale,
        _ => {
            let magnitude = value.unsigned_abs() as usize;
            let power = if magnitude < powers.len() {
                powers[magnitude]
            } else {
                (magnitude as f64).powf(4.0 / 3.0)
            };
            power.copysign(value as f64) * scale
        }
    }
}
/// Requantization and short-block reordering into frequency order.
pub fn requantize(
    input: &[i16; 576],
    info: &GranuleChannel,
    factors: &ScaleFactors,
    bands: Bands,
    output: &mut [f64; 576],
) -> Result<(), Error> {
    if info.block_type > 3 {
        return Err(Error::Invalid("requantization block shape"));
    }
    let powers = &*POWERS;
    output.fill(0.0);
    if input.iter().fold(0, |combined, &value| combined | value) == 0 {
        return Ok(());
    }
    let gain = ((info.global_gain as f64 - 210.0) * 0.25).exp2();
    let scale = 1 + i32::from(info.scalefac_scale);
    let short = info.block_type == 2;
    let long_bands = if !short {
        22
    } else if info.mixed_block {
        bands.mixed_long_bands()
    } else {
        0
    };
    for (band, &pre) in PRETAB.iter().enumerate().take(long_bands) {
        let start = bands.long()[band] as usize;
        let end = bands.long()[band + 1] as usize;
        if input[start..end].iter().all(|&value| value == 0) {
            continue;
        }
        let exponent =
            scale * (factors.long[band] as i32 + if factors.preflag { pre as i32 } else { 0 });
        let amplitude = gain * (-0.5 * exponent as f64).exp2();
        for line in start..end {
            output[line] = power(input[line], amplitude, powers);
        }
    }
    if short {
        let mut source = if info.mixed_block {
            bands.mixed_lines()
        } else {
            0
        };
        for band in if info.mixed_block { 3 } else { 0 }..13 {
            let start = bands.short()[band] as usize;
            let end = bands.short()[band + 1] as usize;
            for window in 0..3 {
                let values = &input[source..source + end - start];
                source += values.len();
                if values.iter().all(|&value| value == 0) {
                    continue;
                }
                let exponent = scale * factors.short[band][window] as i32
                    + 4 * info.subblock_gain[window] as i32;
                let amplitude = gain * (-0.5 * exponent as f64).exp2();
                for (line, &value) in (start..end).zip(values) {
                    output[3 * line + window] = power(value, amplitude, powers);
                }
            }
        }
        debug_assert_eq!(source, 576);
    }
    Ok(())
}
fn intensity(version: MpegVersion, position: u8, width: u8, scale: bool) -> Option<(f64, f64)> {
    if version == MpegVersion::Mpeg1 {
        match position {
            0 => Some((0.0, 1.0)),
            6 => Some((1.0, 0.0)),
            1..=5 => Some(MPEG1_INTENSITY[(position - 1) as usize]),
            _ => None,
        }
    } else if width > 7 || (width != 0 && position as u16 == ((1u16 << width) - 1)) {
        None
    } else {
        let step = if scale { 0.5 } else { 0.25 };
        if position == 0 {
            Some((1.0, 1.0))
        } else if position & 1 != 0 {
            Some(((-step * f64::from(position.div_ceil(2))).exp2(), 1.0))
        } else {
            Some((1.0, (-step * f64::from(position / 2)).exp2()))
        }
    }
}
/// Applies intensity stereo above independently determined right-channel zero
/// bounds and mid/side only on lines not handled by intensity stereo.
pub fn stereo(
    header: FrameHeader,
    info: &[GranuleChannel; 2],
    factors: &[ScaleFactors; 2],
    bands: Bands,
    spectra: &mut [[f64; 576]; 2],
) -> Result<(), Error> {
    if header.channel_mode() != ChannelMode::JointStereo {
        return Ok(());
    }
    let mode = header.mode_extension();
    if mode == 0 {
        return Ok(());
    }
    if info[0].block_type != info[1].block_type || info[0].mixed_block != info[1].mixed_block {
        return Err(Error::Invalid("joint-stereo block mismatch"));
    }
    if mode == 2 {
        let [left, right] = spectra;
        for (left, right) in left.iter_mut().zip(right) {
            let mid = *left;
            let side = *right;
            *left = (mid + side) * std::f64::consts::FRAC_1_SQRT_2;
            *right = (mid - side) * std::f64::consts::FRAC_1_SQRT_2;
        }
        return Ok(());
    }
    let [left, right] = spectra;
    let short = info[1].block_type == 2;
    let mixed = info[1].mixed_block;
    let mid_side = mode & 2 != 0;
    let mut short_bounds = [if mixed { 3 } else { 0 }; 3];
    let mut short_nonzero = false;
    if short {
        for (window, bound) in short_bounds.iter_mut().enumerate() {
            for band in (if mixed { 3 } else { 0 }..13).rev() {
                if (bands.short()[band] as usize..bands.short()[band + 1] as usize)
                    .any(|n| right[3 * n + window] != 0.0)
                {
                    *bound = band + 1;
                    short_nonzero = true;
                    break;
                }
            }
        }
    }
    let long_bands = if short {
        if mixed { bands.mixed_long_bands() } else { 0 }
    } else {
        22
    };
    let long_count = if short_nonzero { 0 } else { long_bands };
    let mut long_bound = 0;
    for band in (0..long_count).rev() {
        if right[bands.long()[band] as usize..bands.long()[band + 1] as usize]
            .iter()
            .any(|&v| v != 0.0)
        {
            long_bound = band + 1;
            break;
        }
    }
    // Every line belongs to one long band or one short-window band. Select
    // its transform once, without flags or a second pass over the spectra.
    let first_long = if mid_side { 0 } else { long_bound };
    let last_long = if mid_side { long_bands } else { long_count };
    for band in first_long..last_long {
        let gains = if band >= long_bound && band < long_count {
            let sf = band.min(20);
            // The uncoded top band inherits pan only if its predecessor is
            // intensity coded; otherwise retain the ordinary top-band rule.
            let top_only = long_bound == 21;
            intensity(
                header.version(),
                if top_only {
                    if header.version() == MpegVersion::Mpeg1 {
                        3
                    } else {
                        0
                    }
                } else {
                    factors[1].long[sf]
                },
                if top_only {
                    0
                } else {
                    factors[1].long_width[sf]
                },
                factors[1].intensity_scale,
            )
        } else {
            None
        };
        let start = bands.long()[band] as usize;
        let end = bands.long()[band + 1] as usize;
        let left = &mut left[start..end];
        let right = &mut right[start..end];
        if let Some((left_gain, right_gain)) = gains {
            for (left, right) in left.iter_mut().zip(right) {
                let value = *left;
                *left = value * left_gain;
                *right = value * right_gain;
            }
        } else if mid_side {
            for (left, right) in left.iter_mut().zip(right) {
                let mid = *left;
                let side = *right;
                *left = (mid + side) * std::f64::consts::FRAC_1_SQRT_2;
                *right = (mid - side) * std::f64::consts::FRAC_1_SQRT_2;
            }
        }
    }
    if short {
        for (window, &bound) in short_bounds.iter().enumerate() {
            let first = if mid_side {
                if mixed { 3 } else { 0 }
            } else {
                bound
            };
            for band in first..13 {
                let gains = if band >= bound {
                    let sf = band.min(11);
                    let top_only = bound == 12;
                    intensity(
                        header.version(),
                        if top_only {
                            if header.version() == MpegVersion::Mpeg1 {
                                3
                            } else {
                                0
                            }
                        } else {
                            factors[1].short[sf][window]
                        },
                        if top_only {
                            0
                        } else {
                            factors[1].short_width[sf][window]
                        },
                        factors[1].intensity_scale,
                    )
                } else {
                    None
                };
                if let Some((left_gain, right_gain)) = gains {
                    for frequency in bands.short()[band] as usize..bands.short()[band + 1] as usize
                    {
                        let line = 3 * frequency + window;
                        let value = left[line];
                        left[line] = value * left_gain;
                        right[line] = value * right_gain;
                    }
                } else if mid_side {
                    for frequency in bands.short()[band] as usize..bands.short()[band + 1] as usize
                    {
                        let line = 3 * frequency + window;
                        let mid = left[line];
                        let side = right[line];
                        left[line] = (mid + side) * std::f64::consts::FRAC_1_SQRT_2;
                        right[line] = (mid - side) * std::f64::consts::FRAC_1_SQRT_2;
                    }
                }
            }
        }
    }
    Ok(())
}
