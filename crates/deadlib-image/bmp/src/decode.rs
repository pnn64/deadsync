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
            if entry_bytes == 3 {
                let (entries, _) = encoded_palette.as_chunks::<3>();
                palette.extend(entries.iter().map(|&[blue, green, red]| [red, green, blue]));
            } else {
                let (entries, _) = encoded_palette.as_chunks::<4>();
                palette.extend(
                    entries
                        .iter()
                        .map(|&[blue, green, red, _]| [red, green, blue]),
                );
            }
            let mut indices = Vec::new();
            reserve(&mut indices, header.pixel_count)?;
            // Full palettes cover the entire encoded index domain.
            let check_indices = palette_len < 1usize << header.info.bit_depth;
            let validate_row = |row: &[u8]| {
                if check_indices
                    && row
                        .iter()
                        .copied()
                        .max()
                        .is_some_and(|index| usize::from(index) >= palette_len)
                {
                    return Err(Error::Invalid("pixel index exceeds the palette"));
                }
                Ok(())
            };
            let rows = display_rows(source, header.row_pitch, header.info.top_down);
            if header.info.bit_depth == 8 {
                if header.info.top_down && width == header.row_pitch {
                    validate_row(source)?;
                    indices.extend_from_slice(source);
                } else {
                    for row in rows {
                        let row = &row[..width];
                        validate_row(row)?;
                        indices.extend_from_slice(row);
                    }
                }
            } else if header.info.bit_depth == 4 {
                let (packed_chunk_bytes, unpacked_chunk_pixels) =
                    if header.info.top_down && width.is_multiple_of(8) {
                        (source.len(), header.pixel_count)
                    } else {
                        (header.row_pitch, width)
                    };
                let pairs = unpacked_chunk_pixels / 2;
                let has_tail = !unpacked_chunk_pixels.is_multiple_of(2);
                let rows = display_rows(source, packed_chunk_bytes, header.info.top_down);
                for row in rows {
                    let start = indices.len();
                    indices.extend(row[..pairs].iter().flat_map(|&byte| [byte >> 4, byte & 15]));
                    if has_tail {
                        indices.push(row[pairs] >> 4);
                    }
                    validate_row(&indices[start..])?;
                }
            } else {
                indices.resize(header.pixel_count, 0);
                // A 32-bit row boundary leaves no padding or partial byte.
                let (packed_chunk_bytes, unpacked_chunk_pixels) =
                    if header.info.top_down && width.is_multiple_of(32) {
                        (source.len(), header.pixel_count)
                    } else {
                        (header.row_pitch, width)
                    };
                let chunks = display_rows(source, packed_chunk_bytes, header.info.top_down);
                for (packed, output) in chunks.zip(indices.chunks_exact_mut(unpacked_chunk_pixels))
                {
                    unpack_one_bit_indices(packed, output);
                    validate_row(output)?;
                }
            }
            Pixels::Indexed {
                bit_depth: header.info.bit_depth as u8,
                indices,
                palette,
            }
        }
        Format::Rgb24 => Pixels::Rgb(map_rows(source, &header, |[blue, green, red]| {
            [red, green, blue]
        })?),
        Format::Rgb555 => Pixels::Rgb(map_rows(source, &header, rgb16::<5>)?),
        Format::Bgra32 => {
            let all_zero = if header.info.top_down {
                bgra_alpha_is_zero(source, false)
            } else {
                let (rest, first_row) = source.split_at(source.len() - header.row_pitch);
                bgra_alpha_is_zero(first_row, false) && bgra_alpha_is_zero(rest, true)
            };
            let samples = if all_zero {
                map_rows(source, &header, |[blue, green, red, _]| {
                    [red, green, blue, 255]
                })?
            } else {
                map_rows(source, &header, |[blue, green, red, alpha]| {
                    [red, green, blue, alpha]
                })?
            };
            Pixels::Rgba(samples)
        }
        Format::Masked {
            red,
            green,
            blue,
            alpha,
        } => {
            let samples = masked_samples(source, &header, [red, green, blue], alpha)?;
            if alpha.is_some() {
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

fn unpack_one_bit_indices(row: &[u8], output: &mut [u8]) {
    let (groups, tail) = output.as_chunks_mut::<8>();
    for (indices, &byte) in groups.iter_mut().zip(row) {
        // Copy the byte at nine-bit intervals. The high bit of each output
        // byte then holds the next source bit, from MSB to LSB.
        let word = u64::from(byte).wrapping_mul(0x8040_2010_0804_0201);
        let bits = (word >> 7) & 0x0101_0101_0101_0101;
        *indices = bits.to_le_bytes();
    }
    if !tail.is_empty() {
        let byte = row[groups.len()];
        for (i, output) in tail.iter_mut().enumerate() {
            *output = (byte >> (7 - i)) & 1;
        }
    }
}

fn display_rows(source: &[u8], pitch: usize, top_down: bool) -> impl Iterator<Item = &[u8]> {
    let mut rows = source.chunks_exact(pitch);
    std::iter::from_fn(move || {
        if top_down {
            rows.next()
        } else {
            rows.next_back()
        }
    })
}

fn map_rows<const INPUT: usize, const OUTPUT: usize>(
    source: &[u8],
    header: &Header,
    mut pixel: impl FnMut([u8; INPUT]) -> [u8; OUTPUT],
) -> Result<Vec<u8>, Error> {
    let mut samples = Vec::new();
    reserve(&mut samples, header.pixel_count)?;
    let width = header.info.width as usize;
    if header.info.top_down && header.row_pitch == width * INPUT {
        let (pixels, _) = source.as_chunks::<INPUT>();
        samples.extend(pixels.iter().copied().map(pixel));
    } else {
        for row in display_rows(source, header.row_pitch, header.info.top_down) {
            let (pixels, _) = row.as_chunks::<INPUT>();
            samples.extend(pixels[..width].iter().copied().map(&mut pixel));
        }
    }
    Ok(samples.into_flattened())
}

fn bgra_alpha_is_zero(source: &[u8], reverse: bool) -> bool {
    const ALPHA_MASK: u128 = 0xff00_0000_ff00_0000_ff00_0000_ff00_0000;
    let (blocks, tail) = source.as_chunks::<16>();
    let block_is_zero = |&block: &[u8; 16]| u128::from_le_bytes(block) & ALPHA_MASK == 0;
    let tail_is_zero = || tail.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 0);
    let remaining_is_zero = |blocks: &[[u8; 16]], reverse: bool| {
        let (groups, remaining_blocks) = blocks.as_chunks::<4>();
        let group_is_zero = |group: &[[u8; 16]; 4]| {
            let [first, second, third, fourth] = group.map(u128::from_le_bytes);
            (first | second | third | fourth) & ALPHA_MASK == 0
        };
        if reverse {
            remaining_blocks.iter().rev().all(block_is_zero)
                && groups.iter().rev().all(group_is_zero)
        } else {
            groups.iter().all(group_is_zero) && remaining_blocks.iter().all(block_is_zero)
        }
    };
    if reverse {
        tail_is_zero()
            && blocks
                .split_last()
                .is_none_or(|(last, blocks)| block_is_zero(last) && remaining_is_zero(blocks, true))
    } else {
        blocks
            .split_first()
            .is_none_or(|(first, blocks)| block_is_zero(first) && remaining_is_zero(blocks, false))
            && tail_is_zero()
    }
}

fn rgb16<const GREEN_BITS: u32>(packed: [u8; 2]) -> [u8; 3] {
    let word = u16::from_le_bytes(packed);
    let [red, green, blue, _] = expand_rgb16::<GREEN_BITS>(word).to_le_bytes();
    [red, green, blue]
}

// RGB555 and RGB565 differ only in the green field's width. Repeat the high
// bits instead of rounding, exactly as the general Channel sampler does.
fn expand_rgb16<const GREEN_BITS: u32>(word: u16) -> u32 {
    let word = u32::from(word);
    let green_mask = ((1 << GREEN_BITS) - 1) << (16 - GREEN_BITS);
    // Place each channel in the high bits of its output byte.
    let channels = ((word >> (2 + GREEN_BITS)) & 0xf8)
        | ((word << (11 - GREEN_BITS)) & green_mask)
        | ((word << 19) & 0xf8_0000);
    let green_tail = ((1 << (8 - GREEN_BITS)) - 1) << 8;
    channels | ((channels >> 5) & 0x07_0007) | ((channels >> GREEN_BITS) & green_tail)
}

fn masked_samples(
    source: &[u8],
    header: &Header,
    rgb: [Channel; 3],
    alpha: Option<Channel>,
) -> Result<Vec<u8>, Error> {
    let [red, green, blue] = rgb;
    match (header.info.bit_depth, alpha) {
        (16, None) => match rgb.map(|channel| channel.mask) {
            [0x7c00, 0x03e0, 0x001f] => map_rows(source, header, rgb16::<5>),
            [0xf800, 0x07e0, 0x001f] => map_rows(source, header, rgb16::<6>),
            _ => map_rows::<2, 3>(source, header, |packed| {
                let word = u16::from_le_bytes(packed);
                [
                    red.sample16(word),
                    green.sample16(word),
                    blue.sample16(word),
                ]
            }),
        },
        (16, Some(alpha)) => match [red.mask, green.mask, blue.mask, alpha.mask] {
            [0x7c00, 0x03e0, 0x001f, 0x8000] => map_rows(source, header, |packed| {
                let word = u16::from_le_bytes(packed);
                let alpha = u32::from(word >> 15) * 0xff00_0000;
                (expand_rgb16::<5>(word) | alpha).to_le_bytes()
            }),
            [0x0f00, 0x00f0, 0x000f, 0xf000] => map_rows(source, header, |[low, high]| {
                let nibbles = u32::from_le_bytes([high & 15, low >> 4, low & 15, high >> 4]);
                // Repeat each nibble within its own byte.
                (nibbles * 17).to_le_bytes()
            }),
            _ => map_rows::<2, 4>(source, header, |packed| {
                let word = u16::from_le_bytes(packed);
                [
                    red.sample16(word),
                    green.sample16(word),
                    blue.sample16(word),
                    alpha.sample16(word),
                ]
            }),
        },
        (32, None) => match rgb.map(|channel| channel.mask) {
            [0xff0000, 0xff00, 0xff] => {
                map_rows::<4, 3>(source, header, |bgrx| [bgrx[2], bgrx[1], bgrx[0]])
            }
            [0xff, 0xff00, 0xff0000] => {
                map_rows::<4, 3>(source, header, |rgbx| [rgbx[0], rgbx[1], rgbx[2]])
            }
            _ => map_rows::<4, 3>(source, header, |packed| {
                let word = u32::from_le_bytes(packed);
                [red.sample(word), green.sample(word), blue.sample(word)]
            }),
        },
        (32, Some(alpha)) => match [red.mask, green.mask, blue.mask, alpha.mask] {
            [0xff0000, 0xff00, 0xff, 0xff000000] => {
                map_rows::<4, 4>(source, header, |bgra| [bgra[2], bgra[1], bgra[0], bgra[3]])
            }
            [0xff, 0xff00, 0xff0000, 0xff000000] => {
                map_rows::<4, 4>(source, header, std::convert::identity)
            }
            _ => map_rows::<4, 4>(source, header, |packed| {
                let word = u32::from_le_bytes(packed);
                [
                    red.sample_wide(word),
                    green.sample_wide(word),
                    blue.sample_wide(word),
                    alpha.sample_wide(word),
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
}

#[derive(Clone, Copy)]
enum Format {
    Indexed {
        palette_start: usize,
        palette_len: usize,
        entry_bytes: usize,
    },
    Rgb24,
    Rgb555,
    Bgra32,
    Masked {
        red: Channel,
        green: Channel,
        blue: Channel,
        alpha: Option<Channel>,
    },
}

impl Format {
    fn channels(self) -> usize {
        match self {
            Self::Indexed { .. } => 1,
            Self::Bgra32 | Self::Masked { alpha: Some(_), .. } => 4,
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
        let value = mask >> shift;
        let next = value.wrapping_add(1);
        if next == 0 || value & next != 0 {
            return Err(Error::Invalid("noncontiguous bitfield mask"));
        }
        // A contiguous field is one less than a power of two.
        let bits = next.trailing_zeros();
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

    fn sample_wide(self, word: u32) -> u8 {
        // Widen before multiplying so the field can stay at its original offset.
        let field = u64::from(word & self.mask);
        ((field * u64::from(self.multiplier)) >> (self.shift + self.downshift)) as u8
    }

    fn sample16(self, word: u16) -> u8 {
        // Multiplying the unshifted 16-bit field still fits in a u32.
        (((u32::from(word) & self.mask) * self.multiplier) >> (self.shift + self.downshift)) as u8
    }
}

fn masks_format(masks: [u32; 4], bit_depth: u16) -> Result<Format, Error> {
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
    })
}

#[inline(always)]
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
            // Legacy assets often have unreliable biClrUsed values. Infer the
            // available palette from bfOffBits, bounded by the indexed depth.
            // Entries start at byte 26 for a core header, not STB's byte 38.
            let palette_bytes = pixel_offset - header_end;
            let (entry_bytes, entries) = if core {
                (3, palette_bytes / 3)
            } else {
                (4, palette_bytes / 4)
            };
            let palette_len = entries.min(1usize << bit_depth);
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
        16 | 32 if compression == 3 => {
            // A 40-byte DIB has three external masks. Larger DIBs store
            // them inside the header; 56/108/124-byte headers include alpha.
            masks_format(
                [
                    read_u32(input, 54)?,
                    read_u32(input, 58)?,
                    read_u32(input, 62)?,
                    if dib_size >= 56 {
                        read_u32(input, 66)?
                    } else {
                        0
                    },
                ],
                bit_depth,
            )?
        }
        16 => {
            // Match ITGmania's legacy V4/V5 BI_RGB16 alpha interpretation.
            let alpha = if dib_size >= 108 {
                read_u32(input, 66)?
            } else {
                0
            };
            if alpha == 0 {
                Format::Rgb555
            } else {
                masks_format([0x7c00, 0x03e0, 0x001f, alpha], bit_depth)?
            }
        }
        32 => Format::Bgra32,
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
