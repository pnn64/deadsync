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
        if widths == [0; 2]
            && (factors.short_block || granule == 0 || side.scfsi[channel] == [false; 4])
        {
            return Ok(factors);
        }
        if factors.short_block {
            if factors.mixed_block && widths[0] != 0 {
                read_values(&mut cursor, widths[0], &mut factors.long[..8])?;
                factors.long_width[..8].fill(widths[0]);
            }
            let first = if factors.mixed_block { 3 } else { 0 };
            for (first, last, width) in [(first, 6, widths[0]), (6, 12, widths[1])] {
                if width == 0 {
                    continue;
                }
                let shift = u32::from(width);
                let mask = (1 << shift) - 1;
                for band in first..last {
                    let packed = cursor.read(3 * shift)?;
                    factors.short[band] = [
                        ((packed >> (2 * shift)) & mask) as u8,
                        ((packed >> shift) & mask) as u8,
                        (packed & mask) as u8,
                    ];
                    factors.short_width[band] = [width; 3];
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
                    if width == 0 {
                        continue;
                    }
                    read_values(&mut cursor, width, &mut factors.long[first..last])?;
                    factors.long_width[first..last].fill(width);
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
        if plan.widths == [0; 4] {
            // No bits are transmitted; the default values and widths already
            // describe every band, and the supplied cursor stays untouched.
            return Ok(factors);
        }
        let mut counts = plan.counts;
        if factors.mixed_block {
            // The first six transmitted factors are long bands. They occupy
            // one complete chunk of the first partition in every LSF class.
            let width = plan.widths[0];
            if width != 0 {
                read_values(&mut cursor, width, &mut factors.long[..6])?;
                factors.long_width[..6].fill(width);
            }
            counts[0] -= 6;
        }
        let (values, widths): (&mut [u8], &mut [u8]) = if factors.short_block {
            let first = if factors.mixed_block { 9 } else { 0 };
            (
                &mut factors.short.as_flattened_mut()[first..36],
                &mut factors.short_width.as_flattened_mut()[first..36],
            )
        } else {
            (&mut factors.long[..21], &mut factors.long_width[..21])
        };
        let mut n = 0;
        for (&count, &width) in counts.iter().zip(&plan.widths) {
            if width == 0 {
                n += count as usize;
                continue;
            }
            let end = n + count as usize;
            // Six maximum-width (five-bit) factors fit in one field.
            for chunk in values[n..end].chunks_mut(6) {
                read_values(&mut cursor, width, chunk)?;
            }
            widths[n..end].fill(width);
            n = end;
        }
    }
    factors.encoded_bits = cursor.position() - start;
    if factors.encoded_bits > info.part2_3_bits as usize {
        return Err(Error::Invalid("scalefactors exceed part2_3 length"));
    }
    *bits = cursor;
    Ok(factors)
}

/// Reads an equal-width span whose combined field fits in a u32.
#[inline]
fn read_values(bits: &mut BitReader<'_>, width: u8, output: &mut [u8]) -> Result<(), Error> {
    let width = u32::from(width);
    let mut packed = bits.read(output.len() as u32 * width)?;
    let mask = (1 << width) - 1;
    for value in output.iter_mut().rev() {
        *value = (packed & mask) as u8;
        packed >>= width;
    }
    Ok(())
}
