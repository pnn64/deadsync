//! Layer III part2 fields. MPEG-2 LSF formulas and partition counts follow
//! ISO/IEC 13818-3 section 2.4.3.2 (printed pages 23-24).
use crate::{ChannelMode, Error, FrameHeader, MpegVersion, SideInfo, bits::BitReader};

const SLEN: [[u8; 2]; 16] = [
    [0, 0],
    [0, 1],
    [0, 2],
    [0, 3],
    [3, 0],
    [1, 1],
    [1, 2],
    [1, 3],
    [2, 1],
    [2, 2],
    [2, 3],
    [3, 1],
    [3, 2],
    [3, 3],
    [4, 2],
    [4, 3],
];
// Six compression classes; long, short, mixed shapes; four partitions.
const LSF_COUNTS: [[[u8; 4]; 3]; 6] = [
    [[6, 5, 5, 5], [9, 9, 9, 9], [6, 9, 9, 9]],
    [[6, 5, 7, 3], [9, 9, 12, 6], [6, 9, 12, 6]],
    [[11, 10, 0, 0], [18, 18, 0, 0], [15, 18, 0, 0]],
    [[7, 7, 7, 0], [12, 12, 12, 0], [6, 15, 12, 0]],
    [[6, 6, 6, 3], [12, 9, 9, 6], [6, 12, 9, 6]],
    [[8, 8, 5, 0], [15, 12, 9, 0], [6, 18, 9, 0]],
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScaleFactors {
    /// The final long band is fixed zero rather than transmitted.
    pub long: [u8; 22],
    /// Band-major, then window; the final short band is fixed zero.
    pub short: [[u8; 3]; 13],
    /// Preserve transmitted widths for MPEG-2 intensity-position interpretation.
    pub long_width: [u8; 22],
    pub short_width: [[u8; 3]; 13],
    pub preflag: bool,
    pub intensity_scale: bool,
    pub short_block: bool,
    pub mixed_block: bool,
    pub encoded_bits: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LsfParameters {
    pub widths: [u8; 4],
    pub counts: [u8; 4],
    pub preflag: bool,
    pub intensity_scale: bool,
}
impl LsfParameters {
    pub fn bits(self) -> usize {
        self.widths
            .into_iter()
            .zip(self.counts)
            .map(|(w, n)| w as usize * n as usize)
            .sum()
    }
}

pub fn lsf_parameters(
    compress: u16,
    short: bool,
    mixed: bool,
    intensity: bool,
) -> Result<LsfParameters, Error> {
    if compress >= 512 || (mixed && !short) {
        return Err(Error::Invalid("LSF scalefactor parameters"));
    }
    let (class, widths) = if intensity {
        let n = compress >> 1;
        if n < 180 {
            (3, [n / 36, (n % 36) / 6, n % 6, 0])
        } else if n < 244 {
            let x = n - 180;
            (4, [x / 16, (x % 16) / 4, x % 4, 0])
        } else {
            let x = n - 244;
            (5, [x / 3, x % 3, 0, 0])
        }
    } else if compress < 400 {
        (
            0,
            [
                (compress >> 4) / 5,
                (compress >> 4) % 5,
                (compress % 16) >> 2,
                compress % 4,
            ],
        )
    } else if compress < 500 {
        let x = compress - 400;
        (1, [(x >> 2) / 5, (x >> 2) % 5, x % 4, 0])
    } else {
        let x = compress - 500;
        (2, [x / 3, x % 3, 0, 0])
    };
    let shape = if mixed { 2 } else { usize::from(short) };
    Ok(LsfParameters {
        widths: widths.map(|w| w as u8),
        counts: LSF_COUNTS[class][shape],
        preflag: class == 2,
        intensity_scale: intensity && compress & 1 != 0,
    })
}

/// Reads just the part2 scalefactors, leaving the following Huffman data intact.
/// `previous` is granule zero's scalefactors when SCFSI reuses long bands.
/// Failure never advances the supplied bit cursor.
pub fn read(
    bits: &mut BitReader<'_>,
    header: FrameHeader,
    side: &SideInfo,
    granule: usize,
    channel: usize,
    previous: Option<&ScaleFactors>,
) -> Result<ScaleFactors, Error> {
    if granule >= header.granules() || channel >= header.channels() {
        return Err(Error::Invalid("scalefactor granule/channel"));
    }
    let info = &side.granules[granule][channel];
    let mut cursor = *bits;
    let start = cursor.position();
    let mut factors = ScaleFactors {
        preflag: info.preflag,
        short_block: info.block_type == 2,
        mixed_block: info.mixed_block && info.block_type == 2,
        ..Default::default()
    };
    if header.version() == MpegVersion::Mpeg1 {
        let widths = *SLEN
            .get(info.scalefac_compress as usize)
            .ok_or(Error::Invalid("MPEG-1 scalefactor compression"))?;
        if factors.short_block {
            if factors.mixed_block {
                for band in 0..8 {
                    factors.long[band] = cursor.read(widths[0] as u32)? as u8;
                    factors.long_width[band] = widths[0];
                }
            }
            for band in if factors.mixed_block { 3 } else { 0 }..12 {
                let width = widths[usize::from(band >= 6)];
                for window in 0..3 {
                    factors.short[band][window] = cursor.read(width as u32)? as u8;
                    factors.short_width[band][window] = width;
                }
            }
        } else {
            for (group, (first, last)) in [(0, 6), (6, 11), (11, 16), (16, 21)]
                .into_iter()
                .enumerate()
            {
                if granule == 1 && side.scfsi[channel][group] {
                    let prev = previous.ok_or(Error::Invalid("SCFSI needs preceding granule"))?;
                    if prev.short_block {
                        return Err(Error::Invalid("SCFSI reuse of short-block factors"));
                    }
                    factors.long[first..last].copy_from_slice(&prev.long[first..last]);
                    factors.long_width[first..last].copy_from_slice(&prev.long_width[first..last]);
                } else {
                    let width = widths[usize::from(group >= 2)];
                    for band in first..last {
                        factors.long[band] = cursor.read(width as u32)? as u8;
                        factors.long_width[band] = width;
                    }
                }
            }
        }
    } else {
        let intensity = channel == 1
            && header.channel_mode() == ChannelMode::JointStereo
            && header.mode_extension() & 1 != 0;
        let plan = lsf_parameters(
            info.scalefac_compress,
            factors.short_block,
            factors.mixed_block,
            intensity,
        )?;
        factors.preflag = plan.preflag;
        factors.intensity_scale = plan.intensity_scale;
        let mut flat = [0u8; 36];
        let mut widths = [0u8; 36];
        let mut n = 0;
        for (&count, &width) in plan.counts.iter().zip(&plan.widths) {
            for _ in 0..count {
                flat[n] = cursor.read(width as u32)? as u8;
                widths[n] = width;
                n += 1;
            }
        }
        if factors.short_block {
            let long = if factors.mixed_block { 6 } else { 0 };
            factors.long[..long].copy_from_slice(&flat[..long]);
            factors.long_width[..long].copy_from_slice(&widths[..long]);
            let mut n = long;
            for band in if factors.mixed_block { 3 } else { 0 }..12 {
                for window in 0..3 {
                    factors.short[band][window] = flat[n];
                    factors.short_width[band][window] = widths[n];
                    n += 1;
                }
            }
        } else {
            factors.long[..21].copy_from_slice(&flat[..21]);
            factors.long_width[..21].copy_from_slice(&widths[..21]);
        }
    }
    factors.encoded_bits = cursor.position() - start;
    if factors.encoded_bits > info.part2_3_bits as usize {
        return Err(Error::Invalid("scalefactors exceed part2_3 length"));
    }
    *bits = cursor;
    Ok(factors)
}
