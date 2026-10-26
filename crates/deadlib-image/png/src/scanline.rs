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
fn passes(interlaced: bool) -> &'static [(u32, u32, u32, u32)] {
    if interlaced { &ADAM7 } else { &PLAIN }
}
fn extent(size: u32, start: u32, step: u32) -> usize {
    size.saturating_sub(start).div_ceil(step) as usize
}
fn row_bytes(width: usize, channels: usize, depth: u8) -> Result<usize, Error> {
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
    let a = i32::from(a);
    let b = i32::from(b);
    let c = i32::from(c);
    let p = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();
    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}

fn unfilter(row: &mut [u8], prev: &[u8], bpp: usize, filter: u8) -> Result<(), Error> {
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
        3 | 4 => {
            for n in 0..row.len() {
                let left = if n >= bpp { row[n - bpp] } else { 0 };
                let ul = if n >= bpp { prev[n - bpp] } else { 0 };
                let predictor = if filter == 3 {
                    ((u16::from(left) + u16::from(prev[n])) / 2) as u8
                } else {
                    paeth(left, prev[n], ul)
                };
                row[n] = row[n].wrapping_add(predictor);
            }
        }
        _ => return Err(Error::Invalid("scanline filter")),
    }
    Ok(())
}

pub(crate) fn decode(
    raw: &mut [u8],
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
    let mut samples = if depth == 16 {
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
            } else {
                &before[position - stride..position]
            };
            unfilter(row, previous, bpp, filter)?;
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
        samples,
    })
}

fn filter_into(row: &[u8], prev: &[u8], bpp: usize, filter: u8, out: &mut [u8]) -> u64 {
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
            row.fill(0);
            let src_y = y as usize + row_index * dy as usize;
            for pixel in 0..w {
                let src =
                    (src_y * image.width as usize + x as usize + pixel * dx as usize) * channels;
                for channel in 0..channels {
                    let dst = pixel * channels + channel;
                    match &image.samples {
                        Samples::U16(v) => row[dst * 2..dst * 2 + 2]
                            .copy_from_slice(&v[src + channel].to_be_bytes()),
                        Samples::U8(v) if depth == 8 => row[dst] = v[src + channel],
                        Samples::U8(v) => {
                            row[dst * depth as usize / 8] |=
                                v[src + channel] << (8 - depth - (dst * depth as usize % 8) as u8)
                        }
                    }
                }
            }
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
