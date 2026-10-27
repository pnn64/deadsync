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
        1 => {
            for n in bpp..row.len() {
                row[n] = row[n].wrapping_add(row[n - bpp]);
            }
        }
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
        let mut zeros = Vec::new();
        reserve(&mut zeros, stride)?;
        zeros.resize(stride, 0);
        for row_index in 0..h {
            let (before, after) = raw.split_at_mut(position);
            let filter = *after.first().ok_or(Error::Truncated)?;
            let row = after.get_mut(1..1 + stride).ok_or(Error::Truncated)?;
            let previous = if row_index == 0 {
                &zeros
            } else if reuse {
                &before[(row_index - 1) * stride..row_index * stride]
            } else {
                &before[position - stride..position]
            };
            unfilter(row, previous, bpp, filter)?;
            if reuse {
                // Read the next row's predictor from the compacted output.
                // Moving this row left cannot overwrite any unread row.
                raw.copy_within(position + 1..position + 1 + stride, row_index * stride);
                position += stride + 1;
                continue;
            }
            let dst_y = y as usize + row_index * dy as usize;
            if dx == 1 && depth >= 8 {
                let dst = (dst_y * width as usize + x as usize) * channels;
                match &mut samples {
                    Samples::U8(v) => v[dst..dst + row.len()].copy_from_slice(row),
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

pub(crate) fn filter_into(row: &[u8], prev: &[u8], bpp: usize, filter: u8, out: &mut [u8]) -> u64 {
    let mut score = 0u64;
    for n in 0..row.len() {
        let left = if n >= bpp { row[n - bpp] } else { 0 };
        let up_left = if n >= bpp { prev[n - bpp] } else { 0 };
        let predictor = match filter {
            0 => 0,
            1 => left,
            2 => prev[n],
            3 => ((u16::from(left) + u16::from(prev[n])) / 2) as u8,
            _ => paeth(left, prev[n], up_left),
        };
        out[n] = row[n].wrapping_sub(predictor);
        score += u64::from((out[n] as i8).unsigned_abs());
    }
    score
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
                return;
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
                return;
            }
            _ => (),
        }
    }
    row.fill(0);
    for pixel in 0..width {
        let src = start + pixel * step * channels;
        for channel in 0..channels {
            let dst = pixel * channels + channel;
            match &image.samples {
                Samples::U16(v) => {
                    row[dst * 2..dst * 2 + 2].copy_from_slice(&v[src + channel].to_be_bytes())
                }
                Samples::U8(v) if depth == 8 => row[dst] = v[src + channel],
                Samples::U8(v) => {
                    row[dst * depth as usize / 8] |=
                        v[src + channel] << (8 - depth - (dst * depth as usize % 8) as u8)
                }
            }
        }
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
    for &(x, y, dx, dy) in passes(interlaced) {
        let w = extent(image.width, x, dx);
        let h = extent(image.height, y, dy);
        if w == 0 || h == 0 {
            continue;
        }
        let stride = row_bytes(w, channels, depth)?;
        let mut row = vec![0; stride];
        let mut previous = vec![0; stride];
        let mut candidate = vec![0; stride];
        let mut best = vec![0; stride];
        for row_index in 0..h {
            let src_y = y as usize + row_index * dy as usize;
            pack_row(image, src_y, x as usize, dx as usize, w, &mut row);
            let mut selected = 0;
            if filter == Filter::Adaptive {
                let mut score = u64::MAX;
                for kind in 0..5 {
                    let value = filter_into(&row, &previous, bpp, kind, &mut candidate);
                    if value < score {
                        score = value;
                        selected = kind;
                        core::mem::swap(&mut candidate, &mut best);
                    }
                }
            } else {
                selected = filter as u8;
                filter_into(&row, &previous, bpp, selected, &mut best);
            }
            out.push(selected);
            out.extend_from_slice(&best);
            core::mem::swap(&mut row, &mut previous);
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
                        assert_eq!(filter_into(&row, &prev, bpp, filter, &mut actual), score);
                        assert_eq!(actual, expected);
                        unfilter(&mut actual, &prev, bpp, filter).unwrap();
                        assert_eq!(actual, row);
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
        }
    }
}
