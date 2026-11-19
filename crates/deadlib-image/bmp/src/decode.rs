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
            for (encoded_y, row) in source.chunks_exact(header.row_pitch).enumerate() {
                let y = if header.info.top_down {
                    encoded_y
                } else {
                    height - 1 - encoded_y
                };
                for (x, index) in indices[y * width..(y + 1) * width].iter_mut().enumerate() {
                    *index = match header.info.bit_depth {
                        1 => (row[x / 8] >> (7 - x % 8)) & 1,
                        4 => (row[x / 2] >> (4 * (1 - x % 2))) & 15,
                        8 => row[x],
                        _ => unreachable!("validated indexed depth"),
                    };
                    if usize::from(*index) >= palette_len {
                        return Err(Error::Invalid("pixel index exceeds the palette"));
                    }
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
            let mut any_alpha = false;
            for (encoded_y, row) in source.chunks_exact(header.row_pitch).enumerate() {
                let y = if header.info.top_down {
                    encoded_y
                } else {
                    height - 1 - encoded_y
                };
                let output = &mut samples[y * width * channels..(y + 1) * width * channels];
                match header.format {
                    Format::Rgb24 => {
                        for (bgr, rgb) in row[..width * 3]
                            .as_chunks::<3>()
                            .0
                            .iter()
                            .zip(output.as_chunks_mut::<3>().0)
                        {
                            rgb.copy_from_slice(&[bgr[2], bgr[1], bgr[0]]);
                        }
                    }
                    Format::Masked {
                        red,
                        green,
                        blue,
                        alpha,
                        ..
                    } => {
                        let step = usize::from(header.info.bit_depth / 8);
                        for (packed, pixel) in row[..width * step]
                            .chunks_exact(step)
                            .zip(output.chunks_exact_mut(channels))
                        {
                            let word = if step == 2 {
                                u32::from(u16::from_le_bytes([packed[0], packed[1]]))
                            } else {
                                u32::from_le_bytes([packed[0], packed[1], packed[2], packed[3]])
                            };
                            pixel[0] = red.sample(word);
                            pixel[1] = green.sample(word);
                            pixel[2] = blue.sample(word);
                            if let Some(alpha) = alpha {
                                pixel[3] = alpha.sample(word);
                                any_alpha |= pixel[3] != 0;
                            }
                        }
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
            ) && !any_alpha
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
