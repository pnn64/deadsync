use std::mem::MaybeUninit;

use super::{Error, Header, display_rows, reserve};

fn rgba_palette<const N: usize>(palette: &[u8], entry_bytes: usize) -> [[u8; 4]; N] {
    let mut colors = [[0; 4]; N];
    if entry_bytes == 3 {
        for (color, &[b, g, r]) in colors.iter_mut().zip(palette.as_chunks::<3>().0) {
            *color = [r, g, b, 255];
        }
    } else {
        for (color, &[b, g, r, _]) in colors.iter_mut().zip(palette.as_chunks::<4>().0) {
            *color = [r, g, b, 255];
        }
    }
    colors
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
    let expand = |indices: &[u8], output: &mut [MaybeUninit<[u8; 4]>]| {
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
        let (blocks, remaining) = output.as_chunks_mut::<4>();
        for (&indices, out) in quads.iter().zip(blocks) {
            *out = indices.map(|i| MaybeUninit::new(colors[usize::from(i)]));
        }
        for (&index, out) in tail.iter().zip(remaining) {
            out.write(colors[usize::from(index)]);
        }
        Ok(())
    };
    if header.info.top_down && header.row_pitch == width {
        expand(source, output)?;
    } else {
        let rows = display_rows(source, header.row_pitch, header.info.top_down);
        for (row, out) in rows.zip(output.chunks_exact_mut(width)) {
            expand(&row[..width], out)?;
        }
    }
    Ok(())
}
