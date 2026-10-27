use crate::{
    ChannelMode, Error, FrameHeader, GranuleChannel, MpegVersion, bands::Bands, bits::BitReader,
    huffman, scalefactors::ScaleFactors,
};
const PRETAB: [u8; 22] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 3, 3, 3, 2, 0,
];

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
        (3 * bands.short()[3] as usize, 576)
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
    while line < (info.big_values as usize) * 2 {
        let region = usize::from(line >= first) + usize::from(line >= second);
        let pair = huffman::pair(&mut cursor, info.table_select[region])?;
        output[line..line + 2].copy_from_slice(&pair);
        line += 2;
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
fn power(value: i16, scale: f64) -> f64 {
    if value == 0 {
        0.0
    } else {
        (value.unsigned_abs() as f64)
            .powf(4.0 / 3.0)
            .copysign(value as f64)
            * scale
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
    output.fill(0.0);
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
        let exponent =
            scale * (factors.long[band] as i32 + if factors.preflag { pre as i32 } else { 0 });
        let amplitude = gain * (-0.5 * exponent as f64).exp2();
        for line in bands.long()[band] as usize..bands.long()[band + 1] as usize {
            output[line] = power(input[line], amplitude);
        }
    }
    if short {
        let mut source = if info.mixed_block {
            bands.mixed_lines()
        } else {
            0
        };
        for band in if info.mixed_block { 3 } else { 0 }..13 {
            for window in 0..3 {
                let exponent = scale * factors.short[band][window] as i32
                    + 4 * info.subblock_gain[window] as i32;
                let amplitude = gain * (-0.5 * exponent as f64).exp2();
                for line in bands.short()[band] as usize..bands.short()[band + 1] as usize {
                    output[3 * line + window] = power(input[source], amplitude);
                    source += 1;
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
            1..=5 => {
                let ratio = (std::f64::consts::PI * position as f64 / 12.0).tan();
                Some((ratio / (1.0 + ratio), 1.0 / (1.0 + ratio)))
            }
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
/// bounds, then mid/side only on lines not handled by intensity stereo.
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
    let mut coupled = [false; 576];
    if mode & 1 != 0 {
        let right = &spectra[1];
        let short = info[1].block_type == 2;
        let mixed = info[1].mixed_block;
        let mut short_bounds = [if mixed { 3 } else { 0 }; 3];
        let mut short_nonzero = false;
        if short {
            for band in if mixed { 3 } else { 0 }..13 {
                for (window, bound) in short_bounds.iter_mut().enumerate() {
                    if (bands.short()[band] as usize..bands.short()[band + 1] as usize)
                        .any(|n| right[3 * n + window] != 0.0)
                    {
                        *bound = band + 1;
                        short_nonzero = true;
                    }
                }
            }
        }
        let long_count = if short {
            if mixed && !short_nonzero {
                bands.mixed_long_bands()
            } else {
                0
            }
        } else {
            22
        };
        let mut long_bound = 0;
        for band in 0..long_count {
            if right[bands.long()[band] as usize..bands.long()[band + 1] as usize]
                .iter()
                .any(|&v| v != 0.0)
            {
                long_bound = band + 1;
            }
        }
        for band in long_bound..long_count {
            let sf = band.min(20);
            // If only the uncoded top band remains, the preceding factor is
            // an ordinary right-channel scale, not a transmitted pan.
            let top_only = long_bound == 21;
            if let Some((left_gain, right_gain)) = intensity(
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
            ) {
                for (line, flag) in coupled
                    .iter_mut()
                    .enumerate()
                    .take(bands.long()[band + 1] as usize)
                    .skip(bands.long()[band] as usize)
                {
                    let value = spectra[0][line];
                    spectra[0][line] = value * left_gain;
                    spectra[1][line] = value * right_gain;
                    *flag = true;
                }
            }
        }
        if short {
            for (window, &bound) in short_bounds.iter().enumerate() {
                for band in bound..13 {
                    let sf = band.min(11);
                    let top_only = bound == 12;
                    if let Some((left_gain, right_gain)) = intensity(
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
                    ) {
                        for frequency in
                            bands.short()[band] as usize..bands.short()[band + 1] as usize
                        {
                            let line = 3 * frequency + window;
                            let value = spectra[0][line];
                            spectra[0][line] = value * left_gain;
                            spectra[1][line] = value * right_gain;
                            coupled[line] = true;
                        }
                    }
                }
            }
        }
    }
    if mode & 2 != 0 {
        for (line, &is_intensity) in coupled.iter().enumerate() {
            if !is_intensity {
                let mid = spectra[0][line];
                let side = spectra[1][line];
                spectra[0][line] = (mid + side) * std::f64::consts::FRAC_1_SQRT_2;
                spectra[1][line] = (mid - side) * std::f64::consts::FRAC_1_SQRT_2;
            }
        }
    }
    Ok(())
}
