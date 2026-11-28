use std::mem::MaybeUninit;

use super::{Error, Header, display_rows, reserve};

fn rgba_palette<const N: usize>(palette: &[u8], entry_bytes: usize) -> [[u8; 4]; N] {
    if entry_bytes == 3 {
        palette_colors(palette.as_chunks::<3>().0, |[b, g, r]| [r, g, b, 255])
    } else {
        palette_colors(palette.as_chunks::<4>().0, |[b, g, r, _]| [r, g, b, 255])
    }
}

fn palette_colors<const ENTRY: usize, const N: usize>(
    entries: &[[u8; ENTRY]],
    rgba: impl Fn([u8; ENTRY]) -> [u8; 4],
) -> [[u8; 4]; N] {
    if let Ok(full) = <&[[u8; ENTRY]; N]>::try_from(entries) {
        full.map(rgba)
    } else {
        let mut colors = [[0; 4]; N];
        for (color, &entry) in colors.iter_mut().zip(entries) {
            *color = rgba(entry);
        }
        colors
    }
}

pub(super) fn rgba_packed<const BITS: usize, const PER_BYTE: usize>(
    source: &[u8],
    header: &Header,
    palette: &[u8],
    entry_bytes: usize,
) -> Result<Vec<u8>, Error> {
    let colors = rgba_palette::<16>(palette, entry_bytes);
    let mask = (1 << BITS) - 1;
    let table: [[[u8; 4]; PER_BYTE]; 256] = std::array::from_fn(|byte| {
        std::array::from_fn(|pixel| colors[(byte >> (8 - BITS * (pixel + 1))) & mask])
    });
    let mut samples = Vec::new();
    reserve(&mut samples, header.pixel_count)?;
    let mut append = |row: &[u8], pixels: usize| {
        let bytes = pixels / PER_BYTE;
        samples.extend(
            row[..bytes]
                .iter()
                .flat_map(|&byte| table[usize::from(byte)]),
        );
        let tail = pixels % PER_BYTE;
        if tail != 0 {
            samples.extend_from_slice(&table[usize::from(row[bytes])][..tail]);
        }
    };
    let width = header.info.width as usize;
    if header.info.top_down && width == header.row_pitch * PER_BYTE {
        append(source, header.pixel_count);
    } else {
        for row in display_rows(source, header.row_pitch, header.info.top_down) {
            append(row, width);
        }
    }
    Ok(samples.into_flattened())
}

pub(super) fn rgba_packed_small<const BITS: usize, const PER_BYTE: usize>(
    source: &[u8],
    header: &Header,
    palette: &[u8],
    palette_len: usize,
    entry_bytes: usize,
) -> Result<Vec<u8>, Error> {
    let colors = rgba_palette::<16>(palette, entry_bytes);
    let mask = (1 << BITS) - 1;
    let width = header.info.width as usize;
    let mut pixels = Vec::new();
    reserve(&mut pixels, header.pixel_count)?;
    for row in display_rows(source, header.row_pitch, header.info.top_down) {
        for x in 0..width {
            let shift = (PER_BYTE - 1 - x % PER_BYTE) * BITS;
            let index = (usize::from(row[x / PER_BYTE]) >> shift) & mask;
            if index >= palette_len {
                return Err(Error::Invalid("pixel index exceeds the palette"));
            }
            pixels.push(colors[index]);
        }
    }
    Ok(pixels.into_flattened())
}

pub(super) fn rgba8(
    source: &[u8],
    header: &Header,
    palette: &[u8],
    palette_len: usize,
    entry_bytes: usize,
) -> Result<Vec<u8>, Error> {
    let colors = rgba_palette::<256>(palette, entry_bytes);
    let mut samples = Vec::<[u8; 4]>::new();
    reserve(&mut samples, header.pixel_count)?;
    let output = &mut samples.spare_capacity_mut()[..header.pixel_count];
    expand_rgba8(source, header, &colors, palette_len, output)?;
    // SAFETY: A successful expansion initialized every pixel in the output.
    unsafe { samples.set_len(header.pixel_count) };
    Ok(samples.into_flattened())
}

fn expand_rgba8(
    source: &[u8],
    header: &Header,
    colors: &[[u8; 4]; 256],
    palette_len: usize,
    output: &mut [MaybeUninit<[u8; 4]>],
) -> Result<(), Error> {
    assert_eq!(source.len(), header.source_bytes);
    #[cfg(target_arch = "x86_64")]
    if let Some(result) =
        super::x86::expand_small_palette(source, header, colors, palette_len, output)
    {
        return result;
    }
    let width = header.info.width as usize;
    #[cfg(target_arch = "x86_64")]
    if palette_len == 256
        && header.info.top_down
        && header.row_pitch == width
        && super::x86::expand_large_palette(source, colors, output)
    {
        return Ok(());
    }
    // Pixel order does not affect validity. A full palette covers every byte.
    if palette_len < colors.len() {
        let invalid = |indices: &[u8]| {
            indices
                .iter()
                .copied()
                .max()
                .is_some_and(|i| usize::from(i) >= palette_len)
        };
        let bad_index = if header.row_pitch == width {
            invalid(source)
        } else {
            source
                .chunks_exact(header.row_pitch)
                .any(|row| invalid(&row[..width]))
        };
        if bad_index {
            return Err(Error::Invalid("pixel index exceeds the palette"));
        }
    }
    if header.info.top_down && header.row_pitch == width {
        expand_index_row(source, colors, output);
    } else {
        let source_rows = source.chunks_exact(header.row_pitch);
        let dest_rows = output.chunks_exact_mut(width);
        if header.info.top_down {
            for (row, out) in source_rows.zip(dest_rows) {
                expand_index_row(&row[..width], colors, out);
            }
        } else {
            for (row, out) in source_rows.rev().zip(dest_rows) {
                expand_index_row(&row[..width], colors, out);
            }
        }
    }
    Ok(())
}

#[inline(always)]
fn expand_index_row(indices: &[u8], colors: &[[u8; 4]; 256], output: &mut [MaybeUninit<[u8; 4]>]) {
    let (quads, tail) = indices.as_chunks::<4>();
    let (blocks, remaining) = output.as_chunks_mut::<4>();
    for (&indices, out) in quads.iter().zip(blocks) {
        *out = indices.map(|i| MaybeUninit::new(colors[usize::from(i)]));
    }
    for (&index, out) in tail.iter().zip(remaining) {
        out.write(colors[usize::from(index)]);
    }
}
