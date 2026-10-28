use crate::codec::Filter;
use crate::{ColorType, Error, Image, Samples, reserve};

const ADAM7: [(u32, u32, u32, u32); 7] = [
    (0, 0, 8, 8),
    (4, 0, 8, 8),
    (0, 4, 4, 8),
    (2, 0, 4, 4),
    (0, 2, 2, 4),
    (1, 0, 2, 2),
    (0, 1, 1, 2),
];
const PLAIN: [(u32, u32, u32, u32); 1] = [(0, 0, 1, 1)];
pub(crate) fn passes(interlaced: bool) -> &'static [(u32, u32, u32, u32)] {
    if interlaced { &ADAM7 } else { &PLAIN }
}
pub(crate) fn extent(size: u32, start: u32, step: u32) -> usize {
    size.saturating_sub(start).div_ceil(step) as usize
}
pub(crate) fn row_bytes(width: usize, channels: usize, depth: u8) -> Result<usize, Error> {
    width
        .checked_mul(channels)
        .and_then(|v| v.checked_mul(depth as usize))
        .and_then(|v| v.checked_add(7))
        .map(|v| v / 8)
        .ok_or(Error::LimitExceeded)
}

pub(crate) fn filtered_size(
    width: u32,
    height: u32,
    color: ColorType,
    depth: u8,
    interlaced: bool,
) -> Result<usize, Error> {
    let mut size = 0usize;
    for &(x, y, dx, dy) in passes(interlaced) {
        let w = extent(width, x, dx);
        let h = extent(height, y, dy);
        if w == 0 || h == 0 {
            continue;
        }
        size = row_bytes(w, color.channels(), depth)?
            .checked_add(1)
            .and_then(|n| n.checked_mul(h))
            .and_then(|n| n.checked_add(size))
            .ok_or(Error::LimitExceeded)?;
    }
    Ok(size)
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    // Equivalent nearest-neighbor regions, including the prescribed ties.
    // The threshold is in -510..=765, so all arithmetic fits in i16.
    let low = i16::from(a.min(b));
    let high = i16::from(a.max(b));
    let threshold = 3 * i16::from(c) - i16::from(a) - i16::from(b);
    if threshold <= low {
        high as u8
    } else if threshold >= high {
        low as u8
    } else {
        c
    }
}

fn unfilter_pixels<const N: usize, const PAETH: bool>(row: &mut [u8], prev: &[u8]) {
    let prev = &prev[..row.len()];
    let (pixels, tail) = row.as_chunks_mut::<N>();
    let (above, above_tail) = prev.as_chunks::<N>();
    let mut left = [0; N];
    let mut upper_left = [0; N];
    for (pixel, up) in pixels.iter_mut().zip(above) {
        for n in 0..N {
            let predictor = if PAETH {
                paeth(left[n], up[n], upper_left[n])
            } else {
                ((u16::from(left[n]) + u16::from(up[n])) / 2) as u8
            };
            pixel[n] = pixel[n].wrapping_add(predictor);
        }
        left = *pixel;
        upper_left = *up;
    }
    for (n, (value, &up)) in tail.iter_mut().zip(above_tail).enumerate() {
        let predictor = if PAETH {
            paeth(left[n], up, upper_left[n])
        } else {
            ((u16::from(left[n]) + u16::from(up)) / 2) as u8
        };
        *value = value.wrapping_add(predictor);
    }
}

fn unfilter_neighbors<const PAETH: bool>(row: &mut [u8], prev: &[u8], bpp: usize) {
    // A fixed pixel width makes its byte lanes independent and keeps the
    // preceding pixel in registers instead of indexing reconstructed bytes.
    match bpp {
        1 => unfilter_pixels::<1, PAETH>(row, prev),
        2 => {
            let prefix = 2.min(row.len());
            for (value, &up) in row[..prefix].iter_mut().zip(prev) {
                *value = value.wrapping_add(if PAETH { up } else { up / 2 });
            }
            for n in 2..row.len() {
                let predictor = if PAETH {
                    paeth(row[n - 2], prev[n], prev[n - 2])
                } else {
                    ((u16::from(row[n - 2]) + u16::from(prev[n])) / 2) as u8
                };
                row[n] = row[n].wrapping_add(predictor);
            }
        }
        3 => unfilter_pixels::<3, PAETH>(row, prev),
        4 => unfilter_pixels::<4, PAETH>(row, prev),
        6 => unfilter_pixels::<6, PAETH>(row, prev),
        8 => unfilter_pixels::<8, PAETH>(row, prev),
        _ => unreachable!("validated PNG byte stride"),
    }
}

pub(crate) fn unfilter(row: &mut [u8], prev: &[u8], bpp: usize, filter: u8) -> Result<(), Error> {
    match filter {
        0 => (),
        1 => match bpp {
            1 | 3 | 4 | 6 => unfilter_first_neighbors::<false>(row, bpp),
            _ => {
                for n in bpp..row.len() {
                    row[n] = row[n].wrapping_add(row[n - bpp]);
                }
            }
        },
        2 => {
            for (v, &up) in row.iter_mut().zip(prev) {
                *v = v.wrapping_add(up);
            }
        }
        3 => unfilter_neighbors::<false>(row, prev, bpp),
        4 => unfilter_neighbors::<true>(row, prev, bpp),
        _ => return Err(Error::Invalid("scanline filter")),
    }
    Ok(())
}

pub(crate) fn unpack<const DEPTH: u8>(row: &[u8], out: &mut [u8]) {
    for (samples, &byte) in out.chunks_mut(8 / DEPTH as usize).zip(row) {
        for (n, sample) in samples.iter_mut().enumerate() {
            *sample = (byte >> (8 - DEPTH - n as u8 * DEPTH)) & ((1 << DEPTH) - 1);
        }
    }
}

fn scatter8<const CHANNELS: usize>(row: &[u8], out: &mut [u8], step: usize) {
    for (pixel, dst) in row
        .as_chunks::<CHANNELS>()
        .0
        .iter()
        .zip(out.chunks_mut(step * CHANNELS))
    {
        dst[..CHANNELS].copy_from_slice(pixel);
    }
}

fn unfilter_first_pixels<const N: usize, const HALF: bool>(row: &mut [u8]) {
    let (pixels, tail) = row.as_chunks_mut::<N>();
    let mut left = [0; N];
    for pixel in pixels {
        for n in 0..N {
            pixel[n] = pixel[n].wrapping_add(left[n] >> u8::from(HALF));
        }
        left = *pixel;
    }
    for (n, value) in tail.iter_mut().enumerate() {
        *value = value.wrapping_add(left[n] >> u8::from(HALF));
    }
}

fn unfilter_first_neighbors<const HALF: bool>(row: &mut [u8], bpp: usize) {
    match bpp {
        1 => unfilter_first_pixels::<1, HALF>(row),
        2 => {
            for n in 2..row.len() {
                row[n] = row[n].wrapping_add(row[n - 2] >> u8::from(HALF));
            }
        }
        3 => unfilter_first_pixels::<3, HALF>(row),
        4 => unfilter_first_pixels::<4, HALF>(row),
        6 => unfilter_first_pixels::<6, HALF>(row),
        8 => unfilter_first_pixels::<8, HALF>(row),
        _ => unreachable!("validated PNG byte stride"),
    }
}

pub(crate) fn unfilter_first(row: &mut [u8], bpp: usize, filter: u8) -> Result<(), Error> {
    // With a zero row above, Paeth is Sub and Average uses half of the left byte.
    match filter {
        0 | 2 => (),
        1 => return unfilter(row, &[], bpp, 1),
        4 => unfilter_first_neighbors::<false>(row, bpp),
        3 => unfilter_first_neighbors::<true>(row, bpp),
        _ => return Err(Error::Invalid("scanline filter")),
    }
    Ok(())
}

pub(crate) fn decode(
    mut raw: Vec<u8>,
    width: u32,
    height: u32,
    color: ColorType,
    depth: u8,
    interlaced: bool,
    limit: usize,
) -> Result<Image, Error> {
    let count = (width as usize)
        .checked_mul(height as usize)
        .and_then(|v| v.checked_mul(color.channels()))
        .ok_or(Error::LimitExceeded)?;
    let bytes = count
        .checked_mul(if depth == 16 { 2 } else { 1 })
        .ok_or(Error::LimitExceeded)?;
    if bytes > limit {
        return Err(Error::LimitExceeded);
    }
    let reuse = depth == 8 && !interlaced;
    let mut samples = if reuse {
        Samples::U8(Vec::new())
    } else if depth == 16 {
        let mut v = Vec::new();
        reserve(&mut v, count)?;
        v.resize(count, 0u16);
        Samples::U16(v)
    } else {
        let mut v = Vec::new();
        reserve(&mut v, count)?;
        v.resize(count, 0u8);
        Samples::U8(v)
    };
    let channels = color.channels();
    let bpp = (channels * depth as usize).div_ceil(8);
    let mut position = 0;
    for &(x, y, dx, dy) in passes(interlaced) {
        let w = extent(width, x, dx);
        let h = extent(height, y, dy);
        if w == 0 || h == 0 {
            continue;
        }
        let stride = row_bytes(w, channels, depth)?;
        for row_index in 0..h {
            let (before, after) = raw.split_at_mut(position);
            let filter = *after.first().ok_or(Error::Truncated)?;
            let row = after.get_mut(1..1 + stride).ok_or(Error::Truncated)?;
            if row_index == 0 {
                unfilter_first(row, bpp, filter)?;
            } else {
                let previous = if reuse {
                    &before[(row_index - 1) * stride..row_index * stride]
                } else {
                    &before[position - stride..position]
                };
                unfilter(row, previous, bpp, filter)?;
            }
            if reuse {
                // Read the next row's predictor from the compacted output.
                // Moving this row left cannot overwrite any unread row.
                raw.copy_within(position + 1..position + 1 + stride, row_index * stride);
                position += stride + 1;
                continue;
            }
            let dst_y = y as usize + row_index * dy as usize;
            if dx == 1 {
                let dst = (dst_y * width as usize + x as usize) * channels;
                match &mut samples {
                    Samples::U8(v) => {
                        let out = &mut v[dst..dst + w * channels];
                        match depth {
                            1 => unpack::<1>(row, out),
                            2 => unpack::<2>(row, out),
                            4 => unpack::<4>(row, out),
                            _ => out.copy_from_slice(row),
                        }
                    }
                    Samples::U16(v) => {
                        for (out, &bytes) in v[dst..dst + w * channels]
                            .iter_mut()
                            .zip(row.as_chunks::<2>().0)
                        {
                            *out = u16::from_be_bytes(bytes);
                        }
                    }
                }
                position += stride + 1;
                continue;
            }
            if depth == 8 {
                if let Samples::U8(v) = &mut samples {
                    let dst = (dst_y * width as usize + x as usize) * channels;
                    let out = &mut v[dst..];
                    match channels {
                        1 => scatter8::<1>(row, out, dx as usize),
                        2 => scatter8::<2>(row, out, dx as usize),
                        3 => scatter8::<3>(row, out, dx as usize),
                        _ => scatter8::<4>(row, out, dx as usize),
                    }
                }
                position += stride + 1;
                continue;
            }
            for pixel in 0..w {
                let dst = (dst_y * width as usize + x as usize + pixel * dx as usize) * channels;
                for channel in 0..channels {
                    let src = pixel * channels + channel;
                    match &mut samples {
                        Samples::U16(v) => {
                            v[dst + channel] = u16::from_be_bytes([row[src * 2], row[src * 2 + 1]])
                        }
                        Samples::U8(v) if depth == 8 => v[dst + channel] = row[src],
                        Samples::U8(v) => {
                            v[dst + channel] = (row[src * depth as usize / 8]
                                >> (8 - depth - (src * depth as usize % 8) as u8))
                                & ((1 << depth) - 1)
                        }
                    }
                }
            }
            position += stride + 1;
        }
    }
    if position != raw.len() {
        return Err(Error::Invalid("extra image data"));
    }
    Ok(Image {
        width,
        height,
        color_type: color,
        bit_depth: depth,
        samples: if reuse {
            raw.truncate(count);
            Samples::U8(raw)
        } else {
            samples
        },
    })
}

#[inline]
pub(crate) fn filter_into<const SCORE: bool>(
    row: &[u8],
    prev: &[u8],
    bpp: usize,
    filter: u8,
    out: &mut [u8],
) -> u64 {
    match filter {
        0 => filter_bytes::<0, SCORE>(row, prev, bpp, out),
        1 => filter_bytes::<1, SCORE>(row, prev, bpp, out),
        2 => filter_bytes::<2, SCORE>(row, prev, bpp, out),
        3 => filter_bytes::<3, SCORE>(row, prev, bpp, out),
        _ => filter_bytes::<4, SCORE>(row, prev, bpp, out),
    }
}

fn filter_bytes<const FILTER: u8, const SCORE: bool>(
    row: &[u8],
    prev: &[u8],
    bpp: usize,
    out: &mut [u8],
) -> u64 {
    let prev = if FILTER >= 2 {
        &prev[..row.len()]
    } else {
        prev
    };
    let out = &mut out[..row.len()];
    let mut score = 0u64;
    for (n, (&value, out)) in row.iter().zip(out).enumerate() {
        let left = if n >= bpp { row[n - bpp] } else { 0 };
        let predictor = match FILTER {
            0 => 0,
            1 => left,
            2 => prev[n],
            3 => ((u16::from(left) + u16::from(prev[n])) / 2) as u8,
            _ => paeth(left, prev[n], if n >= bpp { prev[n - bpp] } else { 0 }),
        };
        *out = value.wrapping_sub(predictor);
        if SCORE {
            score += u64::from((*out as i8).unsigned_abs());
        }
    }
    score
}

fn pack_bits<const DEPTH: u8>(samples: &[u8], row: &mut [u8]) {
    if DEPTH == 4 {
        for (out, values) in row.iter_mut().zip(samples.chunks(8 / DEPTH as usize)) {
            let mut byte = 0;
            for (n, &value) in values.iter().enumerate() {
                byte |= value << (8 - DEPTH - n as u8 * DEPTH);
            }
            *out = byte;
        }
        return;
    }
    let group = 8 / DEPTH as usize;
    let full = samples.len() / group;
    let (words, tail) = row.split_at_mut(full);
    // Validated low sample bits are gathered into the high word byte in PNG
    // order. Each product bit has one contributor, so there are no carries.
    match DEPTH {
        1 => {
            for (out, &bytes) in words.iter_mut().zip(samples.as_chunks::<8>().0) {
                *out = (u64::from_le_bytes(bytes).wrapping_mul(0x8040_2010_0804_0201) >> 56) as u8;
            }
        }
        2 => {
            for (out, &bytes) in words.iter_mut().zip(samples.as_chunks::<4>().0) {
                *out = (u32::from_le_bytes(bytes).wrapping_mul(0x4010_0401) >> 24) as u8;
            }
        }
        _ => unreachable!("validated packed PNG depth"),
    }
    if let Some(out) = tail.first_mut() {
        let mut byte = 0;
        for (n, &value) in samples[full * group..].iter().enumerate() {
            byte |= value << (8 - DEPTH - n as u8 * DEPTH);
        }
        *out = byte;
    }
}

fn pack_step<const DEPTH: u8>(samples: &[u8], step: usize, width: usize, row: &mut [u8]) {
    let group = 8 / DEPTH as usize;
    let full = width / group;
    let (words, tail) = row.split_at_mut(full);
    for (index, out) in words.iter_mut().enumerate() {
        let start = index * group * step;
        let mut byte = 0;
        for n in 0..group {
            byte |= samples[start + n * step] << (8 - DEPTH - n as u8 * DEPTH);
        }
        *out = byte;
    }
    if let Some(out) = tail.first_mut() {
        let start = full * group;
        let mut byte = 0;
        for n in 0..width - start {
            byte |= samples[(start + n) * step] << (8 - DEPTH - n as u8 * DEPTH);
        }
        *out = byte;
    }
}

fn gather8<const CHANNELS: usize>(samples: &[u8], step: usize, row: &mut [u8]) {
    for (out, pixel) in row
        .as_chunks_mut::<CHANNELS>()
        .0
        .iter_mut()
        .zip(samples.chunks(step * CHANNELS))
    {
        out.copy_from_slice(&pixel[..CHANNELS]);
    }
}

pub(crate) fn pack_row(
    image: &Image,
    y: usize,
    x: usize,
    step: usize,
    width: usize,
    row: &mut [u8],
) {
    let channels = image.color_type.channels();
    let depth = image.bit_depth;
    let start = (y * image.width as usize + x) * channels;
    if step == 1 {
        match &image.samples {
            Samples::U8(v) if depth == 8 => {
                row.copy_from_slice(&v[start..start + width * channels]);
            }
            Samples::U16(v) => {
                for (out, value) in row
                    .as_chunks_mut::<2>()
                    .0
                    .iter_mut()
                    .zip(&v[start..start + width * channels])
                {
                    *out = value.to_be_bytes();
                }
            }
            Samples::U8(v) => {
                let samples = &v[start..start + width * channels];
                match depth {
                    1 => pack_bits::<1>(samples, row),
                    2 => pack_bits::<2>(samples, row),
                    4 => pack_bits::<4>(samples, row),
                    _ => unreachable!("validated packed PNG depth"),
                }
            }
        }
        return;
    }
    let samples = match &image.samples {
        Samples::U8(v) if depth == 8 => {
            let samples = &v[start..];
            match channels {
                1 => gather8::<1>(samples, step, row),
                2 => gather8::<2>(samples, step, row),
                3 => gather8::<3>(samples, step, row),
                _ => gather8::<4>(samples, step, row),
            }
            return;
        }
        Samples::U16(v) => {
            for pixel in 0..width {
                let src = start + pixel * step * channels;
                for channel in 0..channels {
                    let dst = (pixel * channels + channel) * 2;
                    row[dst..dst + 2].copy_from_slice(&v[src + channel].to_be_bytes());
                }
            }
            return;
        }
        Samples::U8(v) => v,
    };
    let samples = &samples[start..];
    match depth {
        1 => pack_step::<1>(samples, step, width, row),
        2 => pack_step::<2>(samples, step, width, row),
        4 => pack_step::<4>(samples, step, width, row),
        _ => unreachable!("validated packed PNG depth"),
    }
}

pub(crate) fn encode(image: &Image, interlaced: bool, filter: Filter) -> Result<Vec<u8>, Error> {
    let size = filtered_size(
        image.width,
        image.height,
        image.color_type,
        image.bit_depth,
        interlaced,
    )?;
    let mut out = Vec::new();
    reserve(&mut out, size)?;
    let channels = image.color_type.channels();
    let depth = image.bit_depth;
    let bpp = (channels * depth as usize).div_ceil(8);
    let capacity = if filter == Filter::None {
        0
    } else {
        let width = passes(interlaced)
            .iter()
            .filter(|&&(_, y, _, _)| y < image.height)
            .map(|&(x, _, dx, _)| extent(image.width, x, dx))
            .max()
            .unwrap_or(0);
        row_bytes(width, channels, depth)?
    };
    let mut row = vec![0; capacity];
    let uses_previous = !matches!(filter, Filter::None | Filter::Sub);
    let mut previous = if uses_previous {
        vec![0; capacity]
    } else {
        Vec::new()
    };
    let mut candidate = if filter == Filter::Adaptive {
        vec![0; capacity]
    } else {
        Vec::new()
    };
    let mut best = vec![0; capacity];
    for &(x, y, dx, dy) in passes(interlaced) {
        let w = extent(image.width, x, dx);
        let h = extent(image.height, y, dy);
        if w == 0 || h == 0 {
            continue;
        }
        let stride = row_bytes(w, channels, depth)?;
        if filter == Filter::None {
            for row_index in 0..h {
                let src_y = y as usize + row_index * dy as usize;
                out.push(0);
                match &image.samples {
                    Samples::U8(samples) if depth == 8 && dx == 1 => {
                        let start = (src_y * image.width as usize + x as usize) * channels;
                        out.extend_from_slice(&samples[start..start + w * channels]);
                    }
                    _ => {
                        let start = out.len();
                        out.resize(start + stride, 0);
                        pack_row(image, src_y, x as usize, dx as usize, w, &mut out[start..]);
                    }
                }
            }
            continue;
        }
        row.resize(stride, 0);
        if uses_previous {
            previous.resize(stride, 0);
            previous.fill(0);
        }
        if filter == Filter::Adaptive {
            candidate.resize(stride, 0);
        }
        best.resize(stride, 0);
        for row_index in 0..h {
            let src_y = y as usize + row_index * dy as usize;
            pack_row(image, src_y, x as usize, dx as usize, w, &mut row);
            let mut selected = 0;
            if filter == Filter::Adaptive {
                let mut score = u64::MAX;
                for kind in 0..5 {
                    let value = filter_into::<true>(&row, &previous, bpp, kind, &mut candidate);
                    if value < score {
                        score = value;
                        selected = kind;
                        core::mem::swap(&mut candidate, &mut best);
                    }
                }
            } else {
                selected = filter as u8;
                filter_into::<false>(&row, &previous, bpp, selected, &mut best);
            }
            out.push(selected);
            out.extend_from_slice(&best);
            if uses_previous {
                core::mem::swap(&mut row, &mut previous);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // PNG Third Edition 9.4, expressed independently of the optimized regions.
    fn reference_paeth(a: u8, b: u8, c: u8) -> u8 {
        let p = i32::from(a) + i32::from(b) - i32::from(c);
        let distances = [
            (p - i32::from(a)).abs(),
            (p - i32::from(b)).abs(),
            (p - i32::from(c)).abs(),
        ];
        if distances[0] <= distances[1] && distances[0] <= distances[2] {
            a
        } else if distances[1] <= distances[2] {
            b
        } else {
            c
        }
    }

    #[test]
    fn every_packed_byte_and_tail_survives_strided_gather() {
        for depth in [1, 2, 4] {
            for byte in 0..=255u8 {
                for width in 1..=8 / depth as usize {
                    for step in [1, 2, 4, 8] {
                        for offset in 0..3 {
                            let size = offset + (width - 1) * step + 3;
                            let mut samples = vec![(1 << depth) - 1; size];
                            for n in 0..width {
                                samples[offset + n * step] =
                                    (byte >> (8 - depth as usize * (n + 1))) & ((1 << depth) - 1);
                            }
                            let image = Image {
                                width: size as u32,
                                height: 1,
                                color_type: ColorType::Grayscale,
                                bit_depth: depth,
                                samples: Samples::U8(samples),
                            };
                            let mut output = [0xa5];
                            pack_row(&image, 0, offset, step, width, &mut output);
                            assert_eq!(
                                output,
                                [byte & (u8::MAX << (8 - depth as usize * width))],
                                "depth={depth} byte={byte} width={width} step={step} offset={offset}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn every_paeth_triplet_matches_the_specification() {
        for a in 0..=255 {
            for b in 0..=255 {
                for c in 0..=255 {
                    assert_eq!(paeth(a, b, c), reference_paeth(a, b, c), "{a} {b} {c}");
                }
            }
        }
    }

    #[test]
    fn independent_filter_bytes_scores_and_short_row_boundaries() {
        for bpp in [1, 2, 3, 4, 6, 8] {
            for len in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 15, 16, 17, 31, 32, 33, 127] {
                for pattern in 0..5 {
                    let row: Vec<u8> = (0..len)
                        .map(|n| match pattern {
                            0 => 0,
                            1 => 255,
                            2 => n as u8,
                            _ => (n * 173 + n * n * 47 + 71) as u8,
                        })
                        .collect();
                    let prev: Vec<u8> = (0..len)
                        .map(|n| match pattern {
                            0 | 4 => 0,
                            1 => 255,
                            _ => (n * 59 + n * n * 113 + 219) as u8,
                        })
                        .collect();
                    for filter in 0..5 {
                        let expected: Vec<u8> = (0..len)
                            .map(|n| {
                                let a = if n < bpp { 0 } else { row[n - bpp] };
                                let c = if n < bpp { 0 } else { prev[n - bpp] };
                                let predictor = match filter {
                                    0 => 0,
                                    1 => a,
                                    2 => prev[n],
                                    3 => ((u16::from(a) + u16::from(prev[n])) / 2) as u8,
                                    _ => reference_paeth(a, prev[n], c),
                                };
                                row[n].wrapping_sub(predictor)
                            })
                            .collect();
                        let score: u64 = expected
                            .iter()
                            .map(|&v| u64::from((v as i8).unsigned_abs()))
                            .sum();
                        let mut actual = vec![0xa5; len];
                        let predictors: &[u8] = if filter < 2 { &[] } else { &prev };
                        assert_eq!(
                            filter_into::<true>(&row, predictors, bpp, filter, &mut actual),
                            score
                        );
                        assert_eq!(actual, expected);
                        unfilter(&mut actual, &prev, bpp, filter).unwrap();
                        assert_eq!(actual, row);
                        if prev.iter().all(|&v| v == 0) {
                            let mut first = expected.clone();
                            unfilter_first(&mut first, bpp, filter).unwrap();
                            assert_eq!(first, row);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn invalid_filter_bytes_do_not_change_the_row() {
        for filter in 5..=255 {
            let mut row = [0xa5; 17];
            assert_eq!(
                unfilter(&mut row, &[0; 17], 3, filter),
                Err(Error::Invalid("scanline filter"))
            );
            assert_eq!(row, [0xa5; 17]);
            assert_eq!(
                unfilter_first(&mut row, 3, filter),
                Err(Error::Invalid("scanline filter"))
            );
            assert_eq!(row, [0xa5; 17]);
        }
    }
}
