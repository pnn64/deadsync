use crate::{Bitmap, Error, Limits, Pixels, model::pixel_count, reserve};

#[cfg(target_arch = "x86_64")]
#[allow(unsafe_code)]
mod x86;

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
    Ok(parse_header(input, limits, Output::Native)?.info)
}

/// Decode a BMP with default limits, retaining indexed pixels and their palette.
pub fn decode(input: &[u8]) -> Result<Bitmap, Error> {
    decode_with_limits(input, Limits::default())
}

/// Decode a BMP with explicit resource limits. Missing row padding, invalid
/// indices, and malformed masks are errors. Advisory size/palette-count fields
/// and bytes after the complete pixel array do not alter pixel interpretation.
pub fn decode_with_limits(input: &[u8], limits: Limits) -> Result<Bitmap, Error> {
    let header = parse_header(input, limits, Output::Native)?;
    let source = pixel_source(input, &header)?;
    let pixels = decode_pixels(input, &header, source)?;
    Ok(bitmap(header.info, pixels))
}

/// Decode directly into packed, top-down RGBA8 with the default limits.
/// The returned metadata describes the source BMP.
pub fn decode_rgba8(input: &[u8]) -> Result<(Info, Vec<u8>), Error> {
    decode_rgba8_with_limits(input, Limits::default())
}

/// Decode into packed, top-down RGBA8 with explicit resource limits.
/// `max_decoded_bytes` bounds the four-byte-per-pixel raster.
pub fn decode_rgba8_with_limits(input: &[u8], limits: Limits) -> Result<(Info, Vec<u8>), Error> {
    let header = parse_header(input, limits, Output::Rgba)?;
    let source = pixel_source(input, &header)?;
    let samples = match header.format {
        Format::Indexed {
            palette_start,
            palette_len,
            entry_bytes,
        } if header.info.bit_depth == 8 => {
            let palette = &input[palette_start..palette_start + palette_len * entry_bytes];
            indexed8_rgba(source, &header, palette, palette_len, entry_bytes)?
        }
        Format::Rgb24 => rgb24_rgba(source, &header)?,
        Format::Masked {
            red,
            green,
            blue,
            alpha: None,
        } if header.info.bit_depth == 32 => rgbx32_rgba(source, &header, [red, green, blue])?,
        _ => bitmap(header.info, decode_pixels(input, &header, source)?)
            .into_rgba8_with_limits(limits)?,
    };
    Ok((header.info, samples))
}

fn bitmap(info: Info, pixels: Pixels) -> Bitmap {
    Bitmap {
        width: info.width,
        height: info.height,
        pixels,
    }
}

fn pixel_source<'a>(input: &'a [u8], header: &Header) -> Result<&'a [u8], Error> {
    // Validate the complete source extent before allocating any pixel buffers.
    let source_end = header
        .pixel_offset
        .checked_add(header.source_bytes)
        .ok_or(Error::LimitExceeded)?;
    input
        .get(header.pixel_offset..source_end)
        .ok_or(Error::Truncated)
}

fn decode_pixels(input: &[u8], header: &Header, source: &[u8]) -> Result<Pixels, Error> {
    let width = header.info.width as usize;

    Ok(match header.format {
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
            // A 32-bit row boundary leaves no padding or partial byte.
            let tight_packed = header.info.top_down
                && palette_len > 1
                && match header.info.bit_depth {
                    1 => width.is_multiple_of(32),
                    4 => width.is_multiple_of(8),
                    _ => false,
                };
            let byte_aligned_mono = header.info.bit_depth == 1
                && palette_len == 2
                && (512..1024).contains(&width)
                && width.is_multiple_of(8);
            let mut indices = Vec::new();
            if !tight_packed && !byte_aligned_mono {
                reserve(&mut indices, header.pixel_count)?;
            }
            // Full palettes cover the whole index domain.
            let check_indices = palette_len < 1usize << header.info.bit_depth;
            let validate_indices = |indices: &[u8]| {
                if check_indices
                    && indices
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
            if palette_len == 1 {
                if !indices_are_zero(source, header.row_pitch, width, header.info.bit_depth) {
                    return Err(Error::Invalid("pixel index exceeds the palette"));
                }
                indices.resize(header.pixel_count, 0);
            } else if header.info.bit_depth == 8 {
                // Keep block validation for the wider short-palette rows on x64.
                let fixed_width_limit = if check_indices && !cfg!(target_arch = "x86") {
                    12
                } else {
                    24
                };
                if width < fixed_width_limit && !width.is_multiple_of(4) && header.info.height > 1 {
                    let append = match width {
                        1 => append_narrow_indexed::<1, 4>,
                        2 => append_narrow_indexed::<2, 4>,
                        3 => append_narrow_indexed::<3, 4>,
                        5 => append_narrow_indexed::<5, 8>,
                        6 => append_narrow_indexed::<6, 8>,
                        7 => append_narrow_indexed::<7, 8>,
                        9 => append_narrow_indexed::<9, 12>,
                        10 => append_narrow_indexed::<10, 12>,
                        11 => append_narrow_indexed::<11, 12>,
                        13 => append_narrow_indexed::<13, 16>,
                        14 => append_narrow_indexed::<14, 16>,
                        15 => append_narrow_indexed::<15, 16>,
                        17 => append_narrow_indexed::<17, 20>,
                        18 => append_narrow_indexed::<18, 20>,
                        19 => append_narrow_indexed::<19, 20>,
                        21 => append_narrow_indexed::<21, 24>,
                        22 => append_narrow_indexed::<22, 24>,
                        _ => append_narrow_indexed::<23, 24>,
                    };
                    append(source, &mut indices, header.info.top_down, palette_len)?;
                } else if header.info.top_down && width == header.row_pitch {
                    validate_indices(source)?;
                    indices.extend_from_slice(source);
                } else if width == header.row_pitch && width <= 512 {
                    // Validate groups of complete rows, then copy them while hot.
                    let block_bytes = if check_indices {
                        (8192 / width) * width
                    } else {
                        source.len()
                    };
                    let append = match width {
                        4 => append_reversed_index_blocks::<4>,
                        8 => append_reversed_index_blocks::<8>,
                        12 => append_reversed_index_blocks::<12>,
                        16 => append_reversed_index_blocks::<16>,
                        24 => append_reversed_index_blocks::<24>,
                        32 => append_reversed_index_blocks::<32>,
                        48 => append_reversed_index_blocks::<48>,
                        64 => append_reversed_index_blocks::<64>,
                        96 => append_reversed_index_blocks::<96>,
                        128 => append_reversed_index_blocks::<128>,
                        256 => append_reversed_index_blocks::<256>,
                        _ => append_reversed_index_blocks::<0>,
                    };
                    append(source, &mut indices, width, block_bytes, validate_indices)?;
                } else if width < 512 && header.info.height > 1 {
                    if check_indices {
                        append_padded_index_blocks(source, header, &mut indices, validate_indices)?;
                    } else {
                        append_full_palette_rows(source, header, &mut indices);
                    }
                } else {
                    for row in rows {
                        let row = &row[..width];
                        validate_indices(row)?;
                        indices.extend_from_slice(row);
                    }
                }
            } else if header.info.bit_depth == 4 {
                if tight_packed {
                    indices = unpack_contiguous(source, |byte| [byte >> 4, byte & 15])?;
                } else {
                    append_four_bit_rows(source, header, &mut indices);
                }
                validate_indices(&indices)?;
            } else if tight_packed {
                indices = unpack_contiguous(source, expand_one_bit_byte)?;
            } else if byte_aligned_mono {
                indices = unpack_byte_aligned_mono(source, header)?;
            } else {
                let top_down = header.info.top_down;
                match width {
                    8 => append_mono_word_rows::<8>(source, &mut indices, top_down),
                    16 => append_mono_word_rows::<16>(source, &mut indices, top_down),
                    24 => append_mono_word_rows::<24>(source, &mut indices, top_down),
                    32 => append_mono_word_rows::<32>(source, &mut indices, top_down),
                    _ => append_one_bit_rows(source, header, &mut indices),
                }
            }
            Pixels::Indexed {
                bit_depth: header.info.bit_depth as u8,
                indices,
                palette,
            }
        }
        Format::Rgb24
            if (cfg!(target_arch = "x86") && header.info.width.is_multiple_of(4))
                || (header.info.width == 4 && !header.info.top_down) =>
        {
            Pixels::Rgb(decode_rgb24_quads(source, header)?)
        }
        Format::Rgb24 => Pixels::Rgb(map_rows(source, header, |[blue, green, red]| {
            [red, green, blue]
        })?),
        Format::Rgb555 => Pixels::Rgb(map_rows(source, header, rgb16::<5>)?),
        Format::Bgra32 => {
            let all_zero = if header.info.top_down {
                bgra_alpha_is_zero(source, false)
            } else {
                let (rest, first_row) = source.split_at(source.len() - header.row_pitch);
                bgra_alpha_is_zero(first_row, false) && bgra_alpha_is_zero(rest, true)
            };
            let samples = if all_zero {
                map_rows(source, header, |[blue, green, red, _]| {
                    [red, green, blue, 255]
                })?
            } else {
                map_rows(source, header, |[blue, green, red, alpha]| {
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
            let samples = masked_samples(source, header, [red, green, blue], alpha)?;
            if alpha.is_some() {
                Pixels::Rgba(samples)
            } else {
                Pixels::Rgb(samples)
            }
        }
    })
}

#[inline(never)]
fn append_reversed_index_blocks<const WIDTH: usize>(
    source: &[u8],
    indices: &mut Vec<u8>,
    width: usize,
    block_bytes: usize,
    validate: impl Fn(&[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
    for block in source.rchunks(block_bytes) {
        validate(block)?;
        if WIDTH == 0 {
            for row in block.rchunks_exact(width) {
                indices.extend_from_slice(row);
            }
        } else {
            indices.extend(block.as_chunks::<WIDTH>().0.iter().rev().copied().flatten());
        }
    }
    Ok(())
}

#[inline(never)]
fn append_full_palette_rows(source: &[u8], header: &Header, indices: &mut Vec<u8>) {
    let width = header.info.width as usize;
    let rows = source.chunks_exact(header.row_pitch);
    let copy = |row: &[u8]| indices.extend_from_slice(&row[..width]);
    if header.info.top_down {
        rows.for_each(copy);
    } else {
        rows.rev().for_each(copy);
    }
}

#[inline(never)]
fn append_padded_index_blocks(
    source: &[u8],
    header: &Header,
    indices: &mut Vec<u8>,
    validate: impl Fn(&[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
    let width = header.info.width as usize;
    let pitch = header.row_pitch;
    let top_down = header.info.top_down;
    let block_bytes = (8192 / width) * pitch;
    let append = |block| {
        let start = indices.len();
        for row in display_rows(block, pitch, top_down) {
            indices.extend_from_slice(&row[..width]);
        }
        validate(&indices[start..])
    };
    if top_down {
        source.chunks(block_bytes).try_for_each(append)
    } else {
        source.rchunks(block_bytes).try_for_each(append)
    }
}

#[inline(never)]
fn append_narrow_indexed<const WIDTH: usize, const PITCH: usize>(
    source: &[u8],
    indices: &mut Vec<u8>,
    top_down: bool,
    palette_len: usize,
) -> Result<(), Error> {
    let rows = source.as_chunks::<PITCH>().0;
    let pixels = |row: &[u8; PITCH]| *row.first_chunk::<WIDTH>().unwrap();
    if palette_len < 256
        && rows
            .iter()
            .flat_map(pixels)
            .any(|i| usize::from(i) >= palette_len)
    {
        return Err(Error::Invalid("pixel index exceeds the palette"));
    }
    if top_down {
        indices.extend(rows.iter().flat_map(pixels));
    } else {
        indices.extend(rows.iter().rev().flat_map(pixels));
    }
    Ok(())
}

#[inline(never)]
fn append_four_bit_rows(source: &[u8], header: &Header, indices: &mut Vec<u8>) {
    let width = header.info.width as usize;
    let top_down = header.info.top_down;
    if width <= 7 {
        append_narrow_packed::<4>(source, indices, width, top_down);
        return;
    }
    if width == 8 {
        // The top-down case is already handled by contiguous unpacking.
        indices.extend(
            source
                .as_chunks::<4>()
                .0
                .iter()
                .rev()
                .copied()
                .flat_map(unpack_eight_nibbles),
        );
        return;
    }
    let max_padded_width = if cfg!(target_arch = "x86") { 15 } else { 12 };
    if (9..=max_padded_width).contains(&width) {
        let append = match width {
            9 => append_packed_rows::<4, 9, 8>,
            10 => append_packed_rows::<4, 10, 8>,
            11 => append_packed_rows::<4, 11, 8>,
            12 => append_packed_rows::<4, 12, 8>,
            13 => append_packed_rows::<4, 13, 8>,
            14 => append_packed_rows::<4, 14, 8>,
            _ => append_packed_rows::<4, 15, 8>,
        };
        append(source, indices, top_down);
        return;
    }
    match width {
        16 => append_nibble_word_rows::<8, 16>(source, indices),
        24 => append_nibble_word_rows::<12, 24>(source, indices),
        32 => append_nibble_word_rows::<16, 32>(source, indices),
        _ => {
            let pairs = width / 2;
            let has_tail = !width.is_multiple_of(2);
            for row in display_rows(source, header.row_pitch, top_down) {
                indices.extend(row[..pairs].iter().flat_map(|&byte| [byte >> 4, byte & 15]));
                if has_tail {
                    indices.push(row[pairs] >> 4);
                }
            }
        }
    }
}

#[inline(never)]
fn append_nibble_word_rows<const PACKED: usize, const WIDTH: usize>(
    source: &[u8],
    indices: &mut Vec<u8>,
) {
    indices.extend(source.as_chunks::<PACKED>().0.iter().rev().flat_map(|row| {
        let mut unpacked = [0; WIDTH];
        let words = row.as_chunks::<4>().0;
        for (out, &word) in unpacked.as_chunks_mut::<8>().0.iter_mut().zip(words) {
            *out = unpack_eight_nibbles(word);
        }
        unpacked
    }));
}

fn unpack_eight_nibbles(packed: [u8; 4]) -> [u8; 8] {
    // Widen each byte to a 16-bit lane so its two nibbles can split into bytes.
    let word = u64::from(u32::from_le_bytes(packed));
    let halves = (word | (word << 16)) & 0x0000_ffff_0000_ffff;
    let bytes = (halves | (halves << 8)) & 0x00ff_00ff_00ff_00ff;
    let mask = 0x000f_000f_000f_000f;
    (((bytes >> 4) & mask) | ((bytes & mask) << 8)).to_le_bytes()
}

#[inline(never)]
fn append_narrow_packed<const BITS: usize>(
    source: &[u8],
    indices: &mut Vec<u8>,
    width: usize,
    top_down: bool,
) {
    let append = match width {
        1 => append_packed_rows::<BITS, 1, 4>,
        2 => append_packed_rows::<BITS, 2, 4>,
        3 => append_packed_rows::<BITS, 3, 4>,
        4 => append_packed_rows::<BITS, 4, 4>,
        5 => append_packed_rows::<BITS, 5, 4>,
        6 => append_packed_rows::<BITS, 6, 4>,
        _ => append_packed_rows::<BITS, 7, 4>,
    };
    append(source, indices, top_down);
}

fn append_packed_rows<const BITS: usize, const WIDTH: usize, const PITCH: usize>(
    source: &[u8],
    indices: &mut Vec<u8>,
    top_down: bool,
) {
    let rows = source.as_chunks::<PITCH>().0.iter();
    let unpack = |row: &[u8; PITCH]| {
        std::array::from_fn::<_, WIDTH, _>(|i| {
            let bit = i * BITS;
            (row[bit / 8] >> (8 - BITS - bit % 8)) & ((1 << BITS) - 1)
        })
    };
    if top_down {
        indices.extend(rows.flat_map(unpack));
    } else {
        indices.extend(rows.rev().flat_map(unpack));
    }
}

#[inline(never)]
fn indices_are_zero(source: &[u8], pitch: usize, width: usize, bit_depth: u16) -> bool {
    let packing_shift = 3 - bit_depth.trailing_zeros();
    let complete_bytes = width >> packing_shift;
    let tail_pixels = width & ((1 << packing_shift) - 1);
    let tail_bits = tail_pixels * usize::from(bit_depth);
    let tail_mask = (0xff_u16 << (8 - tail_bits)) as u8;
    let bytes_are_zero = |bytes: &[u8]| bytes.iter().copied().max().unwrap_or(0) == 0;
    if complete_bytes == pitch {
        bytes_are_zero(source)
    } else if pitch == 4 {
        // Repeat the used-bit mask across four adjacent rows.
        let shift = complete_bytes * 8;
        let mask = ((1u32 << shift) - 1) | (u32::from(tail_mask) << shift);
        let (groups, tail) = source.as_chunks::<16>();
        let group_mask = u128::from(mask) * 0x0000_0001_0000_0001_0000_0001_0000_0001;
        groups
            .iter()
            .all(|&rows| u128::from_le_bytes(rows) & group_mask == 0)
            && tail
                .as_chunks::<4>()
                .0
                .iter()
                .all(|&row| u32::from_le_bytes(row) & mask == 0)
    } else {
        // Check only used bits and bytes. Every valid pixel has index zero.
        source.chunks_exact(pitch).all(|row| {
            bytes_are_zero(&row[..complete_bytes])
                && (tail_mask == 0 || row[complete_bytes] & tail_mask == 0)
        })
    }
}

fn unpack_contiguous<const N: usize>(
    source: &[u8],
    unpack: impl FnMut(u8) -> [u8; N],
) -> Result<Vec<u8>, Error> {
    let mut indices = Vec::new();
    reserve(&mut indices, source.len())?;
    indices.extend(source.iter().copied().map(unpack));
    Ok(indices.into_flattened())
}

#[inline(never)]
fn unpack_byte_aligned_mono(source: &[u8], header: &Header) -> Result<Vec<u8>, Error> {
    let mut indices = Vec::new();
    reserve(&mut indices, header.pixel_count / 8)?;
    let packed_width = header.info.width as usize / 8;
    for row in display_rows(source, header.row_pitch, header.info.top_down) {
        indices.extend(row[..packed_width].iter().copied().map(expand_one_bit_byte));
    }
    Ok(indices.into_flattened())
}

#[inline(never)]
fn append_one_bit_rows(source: &[u8], header: &Header, indices: &mut Vec<u8>) {
    let width = header.info.width as usize;
    let top_down = header.info.top_down;
    if width <= 7 {
        append_narrow_packed::<1>(source, indices, width, top_down);
        return;
    }
    indices.resize(header.pixel_count, 0);
    let rows = display_rows(source, header.row_pitch, top_down);
    for (packed, output) in rows.zip(indices.chunks_exact_mut(width)) {
        unpack_one_bit_indices(packed, output);
    }
}

#[inline(never)]
fn append_mono_word_rows<const WIDTH: usize>(source: &[u8], indices: &mut Vec<u8>, top_down: bool) {
    let rows = source.as_chunks::<4>().0.iter();
    let unpack = |row: &[u8; 4]| {
        let mut output = [0; WIDTH];
        for (pixels, &byte) in output.as_chunks_mut::<8>().0.iter_mut().zip(row) {
            *pixels = expand_one_bit_byte(byte);
        }
        output
    };
    if top_down {
        indices.extend(rows.flat_map(unpack));
    } else {
        indices.extend(rows.rev().flat_map(unpack));
    }
}

fn unpack_one_bit_indices(row: &[u8], output: &mut [u8]) {
    let (groups, tail) = output.as_chunks_mut::<8>();
    for (indices, &byte) in groups.iter_mut().zip(row) {
        *indices = expand_one_bit_byte(byte);
    }
    if !tail.is_empty() {
        let byte = row[groups.len()];
        for (i, output) in tail.iter_mut().enumerate() {
            *output = (byte >> (7 - i)) & 1;
        }
    }
}

fn expand_one_bit_byte(byte: u8) -> [u8; 8] {
    // Copy the byte at nine-bit intervals. The high bit of each output
    // byte then holds the next source bit, from MSB to LSB.
    let word = u64::from(byte).wrapping_mul(0x8040_2010_0804_0201);
    ((word >> 7) & 0x0101_0101_0101_0101).to_le_bytes()
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

#[inline]
fn map_rows<const INPUT: usize, const OUTPUT: usize>(
    source: &[u8],
    header: &Header,
    pixel: impl FnMut([u8; INPUT]) -> [u8; OUTPUT],
) -> Result<Vec<u8>, Error> {
    map_rows_inner::<INPUT, OUTPUT, true>(source, header, pixel)
}

fn map_rows_inner<const INPUT: usize, const OUTPUT: usize, const WIDE_QUADS: bool>(
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
    } else if INPUT == 2 && width == 2 {
        // A bottom-up row contains exactly two packed 16-bit pixels.
        samples.extend(source.as_chunks::<4>().0.iter().rev().flat_map(|row| {
            let &[first, second] = row.as_chunks::<INPUT>().0.first_chunk().unwrap();
            [pixel(first), pixel(second)]
        }));
    } else if INPUT != 2 && width == 2 {
        append_wide_pixel_pairs(source, &mut samples, header.info.top_down, pixel);
    } else if width == 3 {
        if INPUT == 2 {
            append_pixel_triples::<INPUT, OUTPUT, 8>(
                source,
                &mut samples,
                header.info.top_down,
                pixel,
            );
        } else {
            append_pixel_triples::<INPUT, OUTPUT, 12>(
                source,
                &mut samples,
                header.info.top_down,
                pixel,
            );
        }
    } else if WIDE_QUADS && INPUT == 4 && width == 4 {
        append_wide_pixel_quads(source, &mut samples, pixel);
    } else if cfg!(target_pointer_width = "64") && width == 1 {
        // A one-pixel truecolor row always occupies four bytes with padding.
        // Restrict this specialization to 64-bit builds to keep x86 decodes small.
        let rows = source.as_chunks::<4>().0.iter();
        let sample = |row: &[u8; 4]| pixel(*row.first_chunk::<INPUT>().unwrap());
        if header.info.top_down {
            samples.extend(rows.map(sample));
        } else {
            samples.extend(rows.rev().map(sample));
        }
    } else {
        for row in display_rows(source, header.row_pitch, header.info.top_down) {
            let (pixels, _) = row.as_chunks::<INPUT>();
            samples.extend(pixels[..width].iter().copied().map(&mut pixel));
        }
    }
    Ok(samples.into_flattened())
}

#[inline(never)]
fn append_wide_pixel_pairs<const INPUT: usize, const OUTPUT: usize>(
    source: &[u8],
    samples: &mut Vec<[u8; OUTPUT]>,
    top_down: bool,
    mut pixel: impl FnMut([u8; INPUT]) -> [u8; OUTPUT],
) {
    let rows = source.as_chunks::<8>().0.iter();
    let unpack = |row: &[u8; 8]| {
        let &[first, second] = row.as_chunks::<INPUT>().0.first_chunk().unwrap();
        [pixel(first), pixel(second)]
    };
    if INPUT == 3 && top_down {
        samples.extend(rows.flat_map(unpack));
    } else {
        samples.extend(rows.rev().flat_map(unpack));
    }
}

#[inline(never)]
fn append_pixel_triples<const INPUT: usize, const OUTPUT: usize, const ROW: usize>(
    source: &[u8],
    samples: &mut Vec<[u8; OUTPUT]>,
    top_down: bool,
    mut pixel: impl FnMut([u8; INPUT]) -> [u8; OUTPUT],
) {
    let rows = source.as_chunks::<ROW>().0.iter();
    let unpack = |row: &[u8; ROW]| {
        let &[first, second, third] = row.as_chunks::<INPUT>().0.first_chunk().unwrap();
        [pixel(first), pixel(second), pixel(third)]
    };
    if INPUT != 4 && top_down {
        samples.extend(rows.flat_map(unpack));
    } else {
        samples.extend(rows.rev().flat_map(unpack));
    }
}

#[inline(never)]
fn append_wide_pixel_quads<const INPUT: usize, const OUTPUT: usize>(
    source: &[u8],
    samples: &mut Vec<[u8; OUTPUT]>,
    mut pixel: impl FnMut([u8; INPUT]) -> [u8; OUTPUT],
) {
    // Top-down four-pixel rows already use contiguous decoding.
    samples.extend(source.as_chunks::<16>().0.iter().rev().flat_map(|row| {
        let &[first, second, third, fourth] = row.as_chunks::<INPUT>().0.first_chunk().unwrap();
        [pixel(first), pixel(second), pixel(third), pixel(fourth)]
    }));
}

#[inline(never)]
fn decode_rgb24_quads(source: &[u8], header: &Header) -> Result<Vec<u8>, Error> {
    let swap = |&[b0, g0, r0, b1, g1, r1, b2, g2, r2, b3, g3, r3]: &[u8; 12]| {
        [r0, g0, b0, r1, g1, b1, r2, g2, b2, r3, g3, b3]
    };
    let mut samples = Vec::new();
    reserve(&mut samples, header.pixel_count / 4)?;
    if cfg!(target_arch = "x86") && header.info.top_down {
        samples.extend(source.as_chunks::<12>().0.iter().map(swap));
    } else if !cfg!(target_arch = "x86") || header.row_pitch == 12 {
        samples.extend(source.as_chunks::<12>().0.iter().rev().map(swap));
    } else {
        for row in source.rchunks_exact(header.row_pitch) {
            samples.extend(row.as_chunks::<12>().0.iter().map(swap));
        }
    }
    Ok(samples.into_flattened())
}

fn indexed8_rgba(
    source: &[u8],
    header: &Header,
    palette: &[u8],
    palette_len: usize,
    entry_bytes: usize,
) -> Result<Vec<u8>, Error> {
    let mut colors = [[0; 4]; 256];
    if entry_bytes == 3 {
        for (color, &[b, g, r]) in colors.iter_mut().zip(palette.as_chunks::<3>().0) {
            *color = [r, g, b, 255];
        }
    } else {
        for (color, &[b, g, r, _]) in colors.iter_mut().zip(palette.as_chunks::<4>().0) {
            *color = [r, g, b, 255];
        }
    }
    let mut samples = Vec::new();
    reserve(&mut samples, header.pixel_count)?;
    let mut append = |indices: &[u8]| -> Result<(), Error> {
        // Check only used bytes before expanding, and skip the scan for full palettes.
        if palette_len < colors.len()
            && indices
                .iter()
                .copied()
                .max()
                .is_some_and(|i| usize::from(i) >= palette_len)
        {
            return Err(Error::Invalid("pixel index exceeds the palette"));
        }
        let (quads, tail) = indices.as_chunks::<4>();
        samples.extend(
            quads
                .iter()
                .flat_map(|&quad| quad.map(|i| colors[usize::from(i)])),
        );
        samples.extend(tail.iter().map(|&i| colors[usize::from(i)]));
        Ok(())
    };
    let width = header.info.width as usize;
    if header.info.top_down && header.row_pitch == width {
        append(source)?;
    } else {
        for row in display_rows(source, header.row_pitch, header.info.top_down) {
            append(&row[..width])?;
        }
    }
    Ok(samples.into_flattened())
}

fn rgb24_rgba(source: &[u8], header: &Header) -> Result<Vec<u8>, Error> {
    #[cfg(target_arch = "x86_64")]
    if let Some(samples) = x86::bgr_rgba::<3>(source, header) {
        return samples;
    }
    let width = header.info.width as usize;
    let mut samples = Vec::new();
    reserve(&mut samples, header.pixel_count)?;
    let mut append = |row: &[u8]| {
        let (quads, tail) = row.as_chunks::<12>();
        samples.extend(quads.iter().flat_map(|quad| {
            // Four BGR triples span three words. Align each triple in the low
            // bytes, then reverse the channels and fill the alpha byte.
            let [a, b, c] = quad
                .as_chunks::<4>()
                .0
                .first_chunk::<3>()
                .unwrap()
                .map(u32::from_le_bytes);
            [a, (a >> 24) | (b << 8), (b >> 16) | (c << 16), c >> 8]
                .map(|bgr| ((bgr.swap_bytes() >> 8) | 0xff00_0000).to_le_bytes())
        }));
        samples.extend(
            tail.as_chunks::<3>()
                .0
                .iter()
                .map(|&[b, g, r]| [r, g, b, 255]),
        );
    };
    if header.info.top_down && header.row_pitch == width * 3 {
        append(source);
    } else {
        for row in display_rows(source, header.row_pitch, header.info.top_down) {
            append(&row[..width * 3]);
        }
    }
    Ok(samples.into_flattened())
}

fn rgbx32_rgba(source: &[u8], header: &Header, rgb: [Channel; 3]) -> Result<Vec<u8>, Error> {
    match rgb.map(|channel| channel.mask) {
        [0xff0000, 0xff00, 0xff] => map_rows(source, header, |[b, g, r, _]| [r, g, b, 255]),
        [0xff, 0xff00, 0xff0000] => map_rows(source, header, |[r, g, b, _]| [r, g, b, 255]),
        _ => map_rows(source, header, |packed| {
            let word = u32::from_le_bytes(packed);
            let [r, g, b] = rgb.map(|channel| channel.sample_wide(word));
            [r, g, b, 255]
        }),
    }
}

fn bgra32_rgba(source: &[u8], header: &Header) -> Result<Vec<u8>, Error> {
    #[cfg(target_arch = "x86_64")]
    if let Some(samples) = x86::bgr_rgba::<4>(source, header) {
        return samples;
    }
    map_rows(source, header, |[b, g, r, a]| [r, g, b, a])
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
            _ => {
                let [red, green, blue, alpha] =
                    [red, green, blue, alpha].map(Channel::with_high_byte_16);
                map_rows::<2, 4>(source, header, |packed| {
                    let word = u16::from_le_bytes(packed);
                    [
                        red.sample16(word),
                        green.sample16(word),
                        blue.sample16(word),
                        alpha.sample16(word),
                    ]
                })
            }
        },
        (32, None) => match rgb.map(|channel| channel.mask) {
            [0xff0000, 0xff00, 0xff] => {
                map_rows::<4, 3>(source, header, |bgrx| [bgrx[2], bgrx[1], bgrx[0]])
            }
            [0xff, 0xff00, 0xff0000] => {
                map_rows::<4, 3>(source, header, |rgbx| [rgbx[0], rgbx[1], rgbx[2]])
            }
            #[cfg(target_pointer_width = "64")]
            _ => masked_rgb32_samples(source, header, rgb),
            // Wide products and four-pixel batches cost more for these masks on x86.
            #[cfg(not(target_pointer_width = "64"))]
            _ => map_rows_inner::<4, 3, false>(source, header, |packed| {
                let word = u32::from_le_bytes(packed);
                [red.sample(word), green.sample(word), blue.sample(word)]
            }),
        },
        (32, Some(alpha)) => match [red.mask, green.mask, blue.mask, alpha.mask] {
            [0xff0000, 0xff00, 0xff, 0xff000000] => bgra32_rgba(source, header),
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

#[cfg(target_pointer_width = "64")]
#[inline(never)]
fn masked_rgb32_samples(
    source: &[u8],
    header: &Header,
    rgb: [Channel; 3],
) -> Result<Vec<u8>, Error> {
    let scaled = rgb.map(|channel| {
        // Keep the repeated bits in the top byte of the product.
        let shift = 56 - channel.shift - channel.downshift;
        (channel.mask, u64::from(channel.multiplier) << shift)
    });
    map_rows::<4, 3>(source, header, |packed| {
        let word = u32::from_le_bytes(packed);
        scaled.map(|(mask, multiplier)| ((u64::from(word & mask) * multiplier) >> 56) as u8)
    })
}

struct Header {
    info: Info,
    format: Format,
    pixel_offset: usize,
    row_pitch: usize,
    source_bytes: usize,
    pixel_count: usize,
}

enum Output {
    Native,
    Rgba,
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

    #[cfg(not(target_pointer_width = "64"))]
    fn sample(self, word: u32) -> u8 {
        ((((word & self.mask) >> self.shift) * self.multiplier) >> self.downshift) as u8
    }

    fn sample_wide(self, word: u32) -> u8 {
        // Widen before multiplying so the field can stay at its original offset.
        let field = u64::from(word & self.mask);
        ((field * u64::from(self.multiplier)) >> (self.shift + self.downshift)) as u8
    }

    fn with_high_byte_16(self) -> Self {
        // Place the repeated bits in the high byte of the 32-bit product.
        Self {
            multiplier: self.multiplier << (24 - self.shift - self.downshift),
            shift: 24,
            downshift: 0,
            ..self
        }
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
fn parse_header(input: &[u8], limits: Limits, output: Output) -> Result<Header, Error> {
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
    // Dimensions are at most i32::MAX and each raster uses at most four
    // bytes per pixel, so even adding the palette stays within u64.
    let pixels = count as u64;
    let decoded_bytes = match (output, format) {
        (Output::Rgba, _) | (_, Format::Bgra32 | Format::Masked { alpha: Some(_), .. }) => {
            pixels * 4
        }
        (_, Format::Indexed { palette_len, .. }) => pixels + palette_len as u64 * 3,
        _ => pixels * 3,
    };
    if decoded_bytes > limits.max_decoded_bytes as u64 {
        return Err(Error::LimitExceeded);
    }
    // The padded source row is also at most four bytes per pixel.
    let row_pitch = (u64::from(width) * u64::from(bit_depth)).div_ceil(32) * 4;
    let source_bytes = row_pitch * u64::from(height);
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
