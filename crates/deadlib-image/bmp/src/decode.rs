use crate::{Bitmap, Error, Limits, Pixels, model::pixel_count, reserve};

/// Validated dimensions and storage information from a BMP header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Info {
    pub width: u32,
    pub height: u32,
    /// Bits per pixel in the encoded file, before unpacking or conversion.
    pub bit_depth: u16,
    /// Whether encoded rows start at the top. Decoded rows always do.
    pub top_down: bool,
}

/// Inspect a header using default limits, without allocating pixel storage.
/// The complete DIB and external masks are required; palette and pixel bytes
/// need not be present. Successful probing does not validate those bytes.
pub fn probe(input: &[u8]) -> Result<Info, Error> {
    probe_with_limits(input, Limits::default())
}

/// Inspect a header and check input, pixel count, and decoded storage limits.
pub fn probe_with_limits(input: &[u8], limits: Limits) -> Result<Info, Error> {
    Ok(parse_header(input, limits)?.info)
}

/// Decode a BMP with default limits, retaining indexed pixels and their palette.
pub fn decode(input: &[u8]) -> Result<Bitmap, Error> {
    decode_with_limits(input, Limits::default())
}

/// Decode a BMP with explicit resource limits. Missing row padding, invalid
/// indices, and malformed masks are errors. Advisory size/palette-count fields
/// and bytes after the complete pixel array do not alter pixel interpretation.
pub fn decode_with_limits(input: &[u8], limits: Limits) -> Result<Bitmap, Error> {
    let header = parse_header(input, limits)?;
    // Validate the complete source extent before allocating any pixel buffers.
    let source_end = header
        .pixel_offset
        .checked_add(header.source_bytes)
        .ok_or(Error::LimitExceeded)?;
    let source = input
        .get(header.pixel_offset..source_end)
        .ok_or(Error::Truncated)?;
    let width = header.info.width as usize;
    let height = header.info.height as usize;

    let pixels = match header.format {
        Format::Indexed {
            palette_start,
            palette_len,
            entry_bytes,
        } => {
            let palette_end = palette_start + palette_len * entry_bytes;
            let encoded_palette = input
                .get(palette_start..palette_end)
                .ok_or(Error::Truncated)?;
            let mut palette = Vec::new();
            reserve(&mut palette, palette_len)?;
            for entry in encoded_palette.chunks_exact(entry_bytes) {
                palette.push([entry[2], entry[1], entry[0]]);
            }
            let mut indices = Vec::new();
            reserve(&mut indices, header.pixel_count)?;
            indices.resize(header.pixel_count, 0);
            let check_indices = palette_len < 1usize << header.info.bit_depth;
            for (encoded_y, row) in source.chunks_exact(header.row_pitch).enumerate() {
                let y = if header.info.top_down {
                    encoded_y
                } else {
                    height - 1 - encoded_y
                };
                let output = &mut indices[y * width..(y + 1) * width];
                match header.info.bit_depth {
                    1 => unpack_indices::<8>(row, output),
                    4 => unpack_indices::<2>(row, output),
                    8 => output.copy_from_slice(&row[..width]),
                    _ => unreachable!("validated indexed depth"),
                }
                // Unpacking bounds every index to the encoded domain. A full
                // palette needs no scan; short palettes use one byte reduction.
                if check_indices
                    && output
                        .iter()
                        .copied()
                        .max()
                        .is_some_and(|index| usize::from(index) >= palette_len)
                {
                    return Err(Error::Invalid("pixel index exceeds the palette"));
                }
            }
            Pixels::Indexed {
                bit_depth: header.info.bit_depth as u8,
                indices,
                palette,
            }
        }
        Format::Rgb24 | Format::Masked { .. } => {
            let channels = header.format.channels();
            let mut samples = Vec::new();
            reserve(&mut samples, header.decoded_bytes)?;
            samples.resize(header.decoded_bytes, 0);
            for (encoded_y, row) in source.chunks_exact(header.row_pitch).enumerate() {
                let y = if header.info.top_down {
                    encoded_y
                } else {
                    height - 1 - encoded_y
                };
                let output = &mut samples[y * width * channels..(y + 1) * width * channels];
                match header.format {
                    Format::Rgb24 => {
                        map_row::<3, 3>(row, output, |bgr| [bgr[2], bgr[1], bgr[0]]);
                    }
                    Format::Masked {
                        red,
                        green,
                        blue,
                        alpha,
                        ..
                    } => {
                        masked_row(
                            row,
                            output,
                            header.info.bit_depth,
                            [red, green, blue],
                            alpha,
                        );
                    }
                    Format::Indexed { .. } => unreachable!("truecolor format"),
                }
            }
            if matches!(
                header.format,
                Format::Masked {
                    opaque_if_zero: true,
                    ..
                }
            ) && samples.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 0)
            {
                for pixel in samples.as_chunks_mut::<4>().0 {
                    pixel[3] = 255;
                }
            }
            if channels == 4 {
                Pixels::Rgba(samples)
            } else {
                Pixels::Rgb(samples)
            }
        }
    };
    Ok(Bitmap {
        width: header.info.width,
        height: header.info.height,
        pixels,
    })
}

// A byte contains N indices, most-significant first. Expand whole groups at a
// time, and handle the partial final byte without decoding its unused low bits.
fn unpack_indices<const N: usize>(row: &[u8], output: &mut [u8]) {
    let index = |byte: u8, i| (byte >> (8 - (i + 1) * (8 / N))) & (u8::MAX >> (8 - 8 / N));
    let (groups, tail) = output.as_chunks_mut::<N>();
    for (indices, &byte) in groups.iter_mut().zip(row) {
        if N == 8 {
            // Copy the byte at nine-bit intervals. The high bit of each
            // output byte then holds the next source bit, from MSB to LSB.
            let word = u64::from(byte).wrapping_mul(0x8040_2010_0804_0201);
            let bits = (word >> 7) & 0x0101_0101_0101_0101;
            indices.copy_from_slice(&bits.to_le_bytes());
        } else {
            *indices = std::array::from_fn(|i| index(byte, i));
        }
    }
    if !tail.is_empty() {
        let byte = row[groups.len()];
        for (i, output) in tail.iter_mut().enumerate() {
            *output = index(byte, i);
        }
    }
}

// Fixed pixel sizes let the compiler remove stride and channel decisions from
// the loop. The output bounds iteration, so source row padding stays ignored.
fn map_row<const INPUT: usize, const OUTPUT: usize>(
    row: &[u8],
    output: &mut [u8],
    mut pixel: impl FnMut([u8; INPUT]) -> [u8; OUTPUT],
) {
    for (&packed, output) in row
        .as_chunks::<INPUT>()
        .0
        .iter()
        .zip(output.as_chunks_mut::<OUTPUT>().0)
    {
        *output = pixel(packed);
    }
}

// RGB555 and RGB565 differ only in the green field's width. Repeat the high
// bits instead of rounding, exactly as the general Channel sampler does.
fn rgb16<const GREEN_BITS: u32>(packed: [u8; 2]) -> [u8; 3] {
    let word = u16::from_le_bytes(packed);
    let red = ((word >> (5 + GREEN_BITS)) & 31) as u8;
    let green = ((word >> 5) & ((1 << GREEN_BITS) - 1)) as u8;
    let blue = (word & 31) as u8;
    [
        (red << 3) | (red >> 2),
        (green << (8 - GREEN_BITS)) | (green >> (2 * GREEN_BITS - 8)),
        (blue << 3) | (blue >> 2),
    ]
}

fn masked_row(
    row: &[u8],
    output: &mut [u8],
    bit_depth: u16,
    rgb: [Channel; 3],
    alpha: Option<Channel>,
) {
    let [red, green, blue] = rgb;
    match (bit_depth, alpha) {
        (16, None) => match rgb.map(|channel| channel.mask) {
            [0x7c00, 0x03e0, 0x001f] => map_row(row, output, rgb16::<5>),
            [0xf800, 0x07e0, 0x001f] => map_row(row, output, rgb16::<6>),
            _ => map_row::<2, 3>(row, output, |packed| {
                let word = u32::from(u16::from_le_bytes(packed));
                [red.sample(word), green.sample(word), blue.sample(word)]
            }),
        },
        (16, Some(alpha)) => map_row::<2, 4>(row, output, |packed| {
            let word = u32::from(u16::from_le_bytes(packed));
            [
                red.sample(word),
                green.sample(word),
                blue.sample(word),
                alpha.sample(word),
            ]
        }),
        (32, None) => match rgb.map(|channel| channel.mask) {
            [0xff0000, 0xff00, 0xff] => {
                map_row::<4, 3>(row, output, |bgrx| [bgrx[2], bgrx[1], bgrx[0]]);
            }
            [0xff, 0xff00, 0xff0000] => {
                map_row::<4, 3>(row, output, |rgbx| [rgbx[0], rgbx[1], rgbx[2]]);
            }
            _ => map_row::<4, 3>(row, output, |packed| {
                let word = u32::from_le_bytes(packed);
                [red.sample(word), green.sample(word), blue.sample(word)]
            }),
        },
        (32, Some(alpha)) => match [red.mask, green.mask, blue.mask, alpha.mask] {
            [0xff0000, 0xff00, 0xff, 0xff000000] => {
                map_row::<4, 4>(row, output, |bgra| [bgra[2], bgra[1], bgra[0], bgra[3]]);
            }
            [0xff, 0xff00, 0xff0000, 0xff000000] => {
                output.copy_from_slice(&row[..output.len()]);
            }
            _ => map_row::<4, 4>(row, output, |packed| {
                let word = u32::from_le_bytes(packed);
                [
                    red.sample(word),
                    green.sample(word),
                    blue.sample(word),
                    alpha.sample(word),
                ]
            }),
        },
        _ => unreachable!("validated masked bit depth"),
    }
}

struct Header {
    info: Info,
    format: Format,
    pixel_offset: usize,
    row_pitch: usize,
    source_bytes: usize,
    pixel_count: usize,
    decoded_bytes: usize,
}

#[derive(Clone, Copy)]
enum Format {
    Indexed {
        palette_start: usize,
        palette_len: usize,
        entry_bytes: usize,
    },
    Rgb24,
    Masked {
        red: Channel,
        green: Channel,
        blue: Channel,
        alpha: Option<Channel>,
        opaque_if_zero: bool,
    },
}

impl Format {
    fn channels(self) -> usize {
        match self {
            Self::Indexed { .. } => 1,
            Self::Masked { alpha: Some(_), .. } => 4,
            _ => 3,
        }
    }
}

#[derive(Clone, Copy)]
struct Channel {
    mask: u32,
    shift: u32,
    multiplier: u32,
    downshift: u32,
}

impl Channel {
    fn new(mask: u32, bit_depth: u16) -> Result<Self, Error> {
        if mask == 0 {
            return Err(Error::Invalid("zero RGB mask"));
        }
        if bit_depth < 32 && mask >> bit_depth != 0 {
            return Err(Error::Invalid("mask exceeds the pixel bit depth"));
        }
        let shift = mask.trailing_zeros();
        let bits = mask.count_ones();
        let value = mask >> shift;
        if value.checked_add(1).is_none_or(|next| value & next != 0) {
            return Err(Error::Invalid("noncontiguous bitfield mask"));
        }
        if bits > 8 {
            return Err(Error::Unsupported("bitfield channel wider than 8 bits"));
        }
        // Repeat the channel's bit pattern to fill eight bits. This agrees with
        // ITGmania/STB, including 5-bit value 3 -> 24 rather than rounded 25.
        let (multiplier, downshift) = match bits {
            1 => (255, 0),
            2 => (85, 0),
            3 => (73, 1),
            4 => (17, 0),
            5 => (33, 2),
            6 => (65, 4),
            7 => (129, 6),
            8 => (1, 0),
            _ => unreachable!("nonzero mask with at most eight bits"),
        };
        Ok(Self {
            mask,
            shift,
            multiplier,
            downshift,
        })
    }

    fn sample(self, word: u32) -> u8 {
        ((((word & self.mask) >> self.shift) * self.multiplier) >> self.downshift) as u8
    }
}

fn masks_format(masks: [u32; 4], bit_depth: u16, opaque_if_zero: bool) -> Result<Format, Error> {
    let mut used = 0;
    for &mask in &masks {
        if used & mask != 0 {
            return Err(Error::Invalid("overlapping bitfield masks"));
        }
        used |= mask;
    }
    Ok(Format::Masked {
        red: Channel::new(masks[0], bit_depth)?,
        green: Channel::new(masks[1], bit_depth)?,
        blue: Channel::new(masks[2], bit_depth)?,
        alpha: if masks[3] == 0 {
            None
        } else {
            Some(Channel::new(masks[3], bit_depth)?)
        },
        opaque_if_zero,
    })
}

fn parse_header(input: &[u8], limits: Limits) -> Result<Header, Error> {
    if input.len() > limits.max_input_bytes {
        return Err(Error::LimitExceeded);
    }
    if input.get(..2).ok_or(Error::Truncated)? != b"BM" {
        return Err(Error::Invalid("missing BM signature"));
    }
    let dib_size = read_u32(input, 14)?;
    if !matches!(dib_size, 12 | 40 | 52 | 56 | 108 | 124) {
        return Err(Error::Unsupported("DIB header size"));
    }
    let header_end = 14 + dib_size as usize;
    input.get(..header_end).ok_or(Error::Truncated)?;
    let pixel_offset = usize::try_from(read_u32(input, 10)?).map_err(|_| Error::LimitExceeded)?;
    let core = dib_size == 12;
    let (width, height, top_down, planes, bit_depth, compression) = if core {
        (
            u32::from(read_u16(input, 18)?),
            u32::from(read_u16(input, 20)?),
            false,
            read_u16(input, 22)?,
            read_u16(input, 24)?,
            0,
        )
    } else {
        let width = read_u32(input, 18)? as i32;
        let height = read_u32(input, 22)? as i32;
        if width <= 0 || height == 0 || height == i32::MIN {
            return Err(Error::Invalid("invalid signed image dimension"));
        }
        (
            width as u32,
            height.unsigned_abs(),
            height < 0,
            read_u16(input, 26)?,
            read_u16(input, 28)?,
            read_u32(input, 30)?,
        )
    };
    if planes != 1 {
        return Err(Error::Invalid("plane count must be one"));
    }
    if !matches!(bit_depth, 1 | 4 | 8 | 16 | 24 | 32) {
        return Err(Error::Unsupported("pixel bit depth"));
    }
    if core && matches!(bit_depth, 16 | 32) {
        return Err(Error::Unsupported(
            "truecolor bit depth in OS/2 core header",
        ));
    }
    if !matches!(compression, 0 | 3) {
        return Err(Error::Unsupported(
            "compression (only BI_RGB and BI_BITFIELDS are supported)",
        ));
    }
    if compression == 3 && !matches!(bit_depth, 16 | 32) {
        return Err(Error::Invalid("bitfields require 16-bit or 32-bit pixels"));
    }
    let mask_end = if compression == 3 && dib_size == 40 {
        header_end + 12
    } else {
        header_end
    };
    if pixel_offset < mask_end {
        return Err(Error::Invalid("pixel offset overlaps the header or masks"));
    }
    let count = pixel_count(width, height)?;
    if count > limits.max_pixels {
        return Err(Error::LimitExceeded);
    }
    let format = match bit_depth {
        1 | 4 | 8 => {
            let entry_bytes = if core { 3 } else { 4 };
            // Legacy assets often have unreliable biClrUsed values. Infer the
            // available palette from bfOffBits, bounded by the indexed depth.
            // Entries start at byte 26 for a core header, not STB's byte 38.
            let palette_len = ((pixel_offset - header_end) / entry_bytes).min(1usize << bit_depth);
            if palette_len == 0 {
                return Err(Error::Invalid("missing indexed palette"));
            }
            Format::Indexed {
                palette_start: header_end,
                palette_len,
                entry_bytes,
            }
        }
        24 => Format::Rgb24,
        16 | 32 => {
            let masks = if compression == 3 {
                // A 40-byte DIB has three external masks. Larger DIBs store
                // them inside the header; 56/108/124-byte headers include alpha.
                [
                    read_u32(input, 54)?,
                    read_u32(input, 58)?,
                    read_u32(input, 62)?,
                    if dib_size >= 56 {
                        read_u32(input, 66)?
                    } else {
                        0
                    },
                ]
            } else if bit_depth == 16 {
                // Match ITGmania's legacy V4/V5 BI_RGB16 alpha interpretation.
                [
                    0x7c00,
                    0x03e0,
                    0x001f,
                    if dib_size >= 108 {
                        read_u32(input, 66)?
                    } else {
                        0
                    },
                ]
            } else {
                [0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0xff00_0000]
            };
            masks_format(masks, bit_depth, compression == 0 && bit_depth == 32)?
        }
        _ => unreachable!("validated BMP bit depth"),
    };
    let palette_bytes = match format {
        Format::Indexed { palette_len, .. } => palette_len * 3,
        _ => 0,
    };
    let decoded_bytes = count
        .checked_mul(format.channels())
        .and_then(|bytes| bytes.checked_add(palette_bytes))
        .ok_or(Error::LimitExceeded)?;
    if decoded_bytes > limits.max_decoded_bytes {
        return Err(Error::LimitExceeded);
    }
    let row_pitch = (u64::from(width) * u64::from(bit_depth)).div_ceil(32) * 4;
    let source_bytes = row_pitch
        .checked_mul(u64::from(height))
        .ok_or(Error::LimitExceeded)?;
    Ok(Header {
        info: Info {
            width,
            height,
            bit_depth,
            top_down,
        },
        format,
        pixel_offset,
        row_pitch: usize::try_from(row_pitch).map_err(|_| Error::LimitExceeded)?,
        source_bytes: usize::try_from(source_bytes).map_err(|_| Error::LimitExceeded)?,
        pixel_count: count,
        decoded_bytes,
    })
}

fn read_u16(input: &[u8], offset: usize) -> Result<u16, Error> {
    let bytes = input.get(offset..offset + 2).ok_or(Error::Truncated)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(input: &[u8], offset: usize) -> Result<u32, Error> {
    let bytes = input.get(offset..offset + 4).ok_or(Error::Truncated)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}
