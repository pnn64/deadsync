use crate::codec::Filter;
use crate::{ColorType, Error, Image, Samples, reserve};

#[cfg(target_arch = "x86_64")]
#[allow(unsafe_code)]
#[path = "scanline/x86.rs"]
mod x86;

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
    let low = a.min(b);
    let high = a.max(b);
    let to_low = c.saturating_sub(low);
    let to_high = high.saturating_sub(c);
    // Inside [low, high], 2*to_low <= to_high is exactly 3*c-a-b <= low;
    // the other inequality selects low. Saturation also covers outside values.
    let candidate = if to_high <= to_low / 2 { low } else { c };
    if to_low <= to_high / 2 {
        high
    } else {
        candidate
    }
}

fn paeth_wide(a: i16, b: i16, c: i16) -> u8 {
    // Equivalent nearest-neighbor regions, including the prescribed ties.
    // The threshold is in -510..=765, so all arithmetic fits in i16.
    let low = a.min(b);
    let high = a.max(b);
    let threshold = 3 * c - a - b;
    // Compute both selections independently. An early-return chain makes
    // unpredictable pixel values branch on the preceding pixel's result.
    let candidate = if threshold >= high { low } else { c };
    (if threshold <= low { high } else { candidate }) as u8
}

fn unfilter_pixels<const N: usize, const PAETH: bool>(row: &mut [u8], prev: &[u8]) {
    let prev = &prev[..row.len()];
    let (pixels, tail) = row.as_chunks_mut::<N>();
    let (above, above_tail) = prev.as_chunks::<N>();
    let mut left = [0i16; N];
    let mut upper_left = [0i16; N];
    for (pixel, up) in pixels.iter_mut().zip(above) {
        for n in 0..N {
            let predictor = if PAETH {
                paeth_wide(left[n], i16::from(up[n]), upper_left[n])
            } else {
                ((left[n] + i16::from(up[n])) / 2) as u8
            };
            pixel[n] = pixel[n].wrapping_add(predictor);
            left[n] = i16::from(pixel[n]);
            upper_left[n] = i16::from(up[n]);
        }
    }
    for (n, (value, &up)) in tail.iter_mut().zip(above_tail).enumerate() {
        let predictor = if PAETH {
            paeth_wide(left[n], i16::from(up), upper_left[n])
        } else {
            ((left[n] + i16::from(up)) / 2) as u8
        };
        *value = value.wrapping_add(predictor);
    }
}

fn unfilter_neighbors<const PAETH: bool>(row: &mut [u8], prev: &[u8], bpp: usize) {
    #[cfg(target_arch = "x86_64")]
    {
        let handled = match bpp {
            3 => x86::unfilter::<3, PAETH>(row, prev),
            4 => x86::unfilter::<4, PAETH>(row, prev),
            6 => x86::unfilter::<6, PAETH>(row, prev),
            8 => x86::unfilter::<8, PAETH>(row, prev),
            _ => false,
        };
        if handled {
            return;
        }
    }
    // A fixed pixel width makes its byte lanes independent and keeps the
    // preceding pixel in registers instead of indexing reconstructed bytes.
    match bpp {
        1 => unfilter_pixels::<1, PAETH>(row, prev),
        2 => unfilter_pixels::<2, PAETH>(row, prev),
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

static ONE_BIT_SAMPLES: [[u8; 8]; 256] = {
    let mut table = [[0; 8]; 256];
    let mut byte = 0;
    while byte < 256 {
        let mut lane = 0;
        while lane < 8 {
            table[byte][lane] = ((byte >> (7 - lane)) & 1) as u8;
            lane += 1;
        }
        byte += 1;
    }
    table
};

pub(crate) fn unpack<const DEPTH: u8>(row: &[u8], out: &mut [u8]) {
    match DEPTH {
        1 => unpack_groups::<8>(row, out, |byte| ONE_BIT_SAMPLES[usize::from(byte)]),
        2 => unpack_groups::<4>(row, out, |byte| {
            [byte >> 6, (byte >> 4) & 3, (byte >> 2) & 3, byte & 3]
        }),
        4 => unpack_groups::<2>(row, out, |byte| [byte >> 4, byte & 15]),
        _ => unreachable!("validated packed PNG depth"),
    }
}

fn unpack_groups<const N: usize>(row: &[u8], out: &mut [u8], expand: impl Fn(u8) -> [u8; N]) {
    // Complete bytes have a fixed number of independent output lanes. Keep the
    // short final group out of this loop so its length cannot limit unrolling.
    let (groups, tail) = out.as_chunks_mut::<N>();
    for (samples, &byte) in groups.iter_mut().zip(row) {
        *samples = expand(byte);
    }
    if !tail.is_empty()
        && let Some(&byte) = row.get(groups.len())
    {
        let values = expand(byte);
        tail.copy_from_slice(&values[..tail.len()]);
    }
}

pub(crate) fn scatter<T: Copy, const CHANNELS: usize>(row: &[T], out: &mut [T], step: usize) {
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
                        1 => scatter::<_, 1>(row, out, dx as usize),
                        2 => scatter::<_, 2>(row, out, dx as usize),
                        3 => scatter::<_, 3>(row, out, dx as usize),
                        _ => scatter::<_, 4>(row, out, dx as usize),
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

#[inline]
fn filter_chunk<const FILTER: u8>(
    row: &[u8; 32],
    left: &[u8; 32],
    upper: &[u8; 32],
    upper_left: &[u8; 32],
    out: &mut [u8; 32],
) {
    for n in 0..32 {
        let predictor = match FILTER {
            0 => 0,
            1 => left[n],
            2 => upper[n],
            3 => ((u16::from(left[n]) + u16::from(upper[n])) / 2) as u8,
            4 => paeth(left[n], upper[n], upper_left[n]),
            _ => left[n] / 2,
        };
        out[n] = row[n].wrapping_sub(predictor);
    }
}

fn score_bytes(bytes: &[u8]) -> u64 {
    let (chunks, tail) = bytes.as_chunks::<32>();
    let mut score = 0u64;
    for chunk in chunks {
        // Signed-byte magnitudes are at most 128: a chunk sums to at most 4096.
        let mut sum = 0u16;
        for &value in chunk {
            sum += u16::from((value as i8).unsigned_abs());
        }
        score += u64::from(sum);
    }
    for &value in tail {
        score += u64::from((value as i8).unsigned_abs());
    }
    score
}

fn filter_bytes<const FILTER: u8, const SCORE: bool>(
    row: &[u8],
    prev: &[u8],
    bpp: usize,
    out: &mut [u8],
) -> u64 {
    let prev = if FILTER >= 2 && FILTER <= 4 {
        &prev[..row.len()]
    } else {
        prev
    };
    let out = &mut out[..row.len()];
    let split = bpp.min(row.len());
    for (n, (&value, out)) in row[..split].iter().zip(&mut out[..split]).enumerate() {
        let predictor = match FILTER {
            2 | 4 => prev[n],
            3 => prev[n] / 2,
            _ => 0,
        };
        *out = value.wrapping_sub(predictor);
    }
    // All remaining bytes have a left neighbor. Keeping the boundary out of
    // this loop makes its byte lanes independent for compiler vectorization.
    let (upper, upper_left) = if FILTER >= 2 && FILTER <= 4 {
        (&prev[split..], &prev[..row.len() - split])
    } else {
        // These predictors are unused in this specialization. Equal-length
        // slices avoid accessing the empty previous row of None/Sub.
        (&row[split..], &row[..row.len() - split])
    };
    let (values, values_tail) = row[split..].as_chunks::<32>();
    let (left, left_tail) = row[..row.len() - split].as_chunks::<32>();
    let (upper, upper_tail) = upper.as_chunks::<32>();
    let (upper_left, corner_tail) = upper_left.as_chunks::<32>();
    let (output, output_tail) = out[split..].as_chunks_mut::<32>();
    for ((((values, left), upper), upper_left), output) in values
        .iter()
        .zip(left)
        .zip(upper)
        .zip(upper_left)
        .zip(output)
    {
        filter_chunk::<FILTER>(values, left, upper, upper_left, output);
    }
    for ((((&value, &left), &upper), &corner), out) in values_tail
        .iter()
        .zip(left_tail)
        .zip(upper_tail)
        .zip(corner_tail)
        .zip(output_tail)
    {
        let predictor = match FILTER {
            0 => 0,
            1 => left,
            2 => upper,
            3 => ((u16::from(left) + u16::from(upper)) / 2) as u8,
            4 => paeth(left, upper, corner),
            _ => left / 2,
        };
        *out = value.wrapping_sub(predictor);
    }
    if SCORE { score_bytes(out) } else { 0 }
}

pub(crate) fn filter_first<const SCORE: bool>(
    row: &[u8],
    bpp: usize,
    filter: u8,
    out: &mut [u8],
) -> u64 {
    // A pass starts with zero upper predictors: Up equals None, Paeth equals
    // Sub, and Average uses only half the left byte.
    match filter {
        0 | 2 => filter_bytes::<0, SCORE>(row, &[], bpp, out),
        1 | 4 => filter_bytes::<1, SCORE>(row, &[], bpp, out),
        3 => filter_bytes::<5, SCORE>(row, &[], bpp, out),
        _ => unreachable!("validated scanline filter"),
    }
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
    let pixels = match &image.samples {
        Samples::U8(pixels) if depth == 8 && !interlaced => Some(pixels.as_slice()),
        _ => None,
    };
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
    let mut row = if pixels.is_some() {
        Vec::new()
    } else {
        vec![0; capacity]
    };
    let uses_previous =
        pixels.is_none() && !matches!(filter, Filter::None | Filter::Sub) && image.height > 1;
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
        if pixels.is_none() {
            row.resize(stride, 0);
        }
        if uses_previous {
            previous.resize(stride, 0);
        }
        if filter == Filter::Adaptive {
            candidate.resize(stride, 0);
        }
        best.resize(stride, 0);
        for row_index in 0..h {
            let src_y = y as usize + row_index * dy as usize;
            let (current, prior) = if let Some(pixels) = pixels {
                let start = src_y * stride;
                (
                    &pixels[start..start + stride],
                    &pixels[start.saturating_sub(stride)..start],
                )
            } else {
                pack_row(image, src_y, x as usize, dx as usize, w, &mut row);
                (row.as_slice(), previous.as_slice())
            };
            let mut selected = 0;
            if filter == Filter::Adaptive && row_index != 0 && current == prior {
                // Up is zero; None/Sub can tie it only for an all-zero row.
                selected = if current.iter().all(|&byte| byte == 0) {
                    0
                } else {
                    2
                };
                best.fill(0);
            } else if filter == Filter::Adaptive {
                let mut score = u64::MAX;
                for kind in 0..5 {
                    // Up/Paeth duplicate the earlier None/Sub candidates on
                    // the first row, so neither can win a strict-score tie.
                    if row_index == 0 && matches!(kind, 2 | 4) {
                        continue;
                    }
                    let value = if row_index == 0 {
                        filter_first::<true>(current, bpp, kind, &mut candidate)
                    } else {
                        filter_into::<true>(current, prior, bpp, kind, &mut candidate)
                    };
                    if value < score {
                        score = value;
                        selected = kind;
                        core::mem::swap(&mut candidate, &mut best);
                        if score == 0 {
                            break;
                        }
                    }
                }
            } else {
                selected = filter as u8;
                if row_index == 0 {
                    filter_first::<false>(current, bpp, selected, &mut best);
                } else {
                    filter_into::<false>(current, prior, bpp, selected, &mut best);
                }
            }
            out.push(selected);
            out.extend_from_slice(&best);
            if uses_previous && row_index + 1 < h {
                core::mem::swap(&mut row, &mut previous);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Read the PNG bit stream one bit at a time, independently of byte-group
    // expansion. Leave unmatched output unchanged, as the unpacker's zip does.
    fn reference_unpack_bits(depth: u8, row: &[u8], out: &mut [u8]) {
        let mut bits = row
            .iter()
            .flat_map(|&byte| (0..8).rev().map(move |bit| (byte >> bit) & 1));
        for sample in out {
            let mut value = 0;
            for _ in 0..depth {
                let Some(bit) = bits.next() else { return };
                value = value * 2 + bit;
            }
            *sample = value;
        }
    }

    fn unpack_depth(depth: u8, row: &[u8], out: &mut [u8]) {
        match depth {
            1 => unpack::<1>(row, out),
            2 => unpack::<2>(row, out),
            4 => unpack::<4>(row, out),
            _ => unreachable!(),
        }
    }

    #[test]
    fn unpack_every_byte_and_partial_group() {
        for depth in [1, 2, 4] {
            for byte in 0..=u8::MAX {
                for width in 0..=8 / usize::from(depth) {
                    for offset in 0..8 {
                        let mut actual = [0xa5; 32];
                        let mut expected = actual;
                        reference_unpack_bits(
                            depth,
                            &[byte],
                            &mut expected[offset..offset + width],
                        );
                        unpack_depth(depth, &[byte], &mut actual[offset..offset + width]);
                        assert_eq!(
                            actual, expected,
                            "depth={depth} byte={byte} width={width} offset={offset}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn unpack_rows_keep_padding_and_slice_boundaries() {
        for depth in [1, 2, 4] {
            for width in [
                0, 1, 2, 3, 7, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129, 255, 256,
                257, 2047, 2048, 2049,
            ] {
                let row_bytes = (width * usize::from(depth)).div_ceil(8);
                for offset in [0, 1, 7, 15, 16, 31] {
                    for pattern in 0..4 {
                        let mut input = vec![0xa5; offset + 3 * row_bytes + 7];
                        for y in 0..3 {
                            for n in 0..row_bytes {
                                input[offset + y * row_bytes + n] = match pattern {
                                    0 => 0,
                                    1 => u8::MAX,
                                    2 => (n + y * 37) as u8,
                                    _ => (n * 173 + n * n * 47 + y * 71) as u8,
                                };
                            }
                        }
                        let stride = width + 3;
                        let mut actual = vec![0xa5; offset + 3 * stride + 7];
                        let mut expected = actual.clone();
                        for y in 0..3 {
                            let row = &input[offset + y * row_bytes..offset + (y + 1) * row_bytes];
                            let start = offset + y * stride;
                            reference_unpack_bits(depth, row, &mut expected[start..start + width]);
                            unpack_depth(depth, row, &mut actual[start..start + width]);
                        }
                        assert_eq!(
                            actual, expected,
                            "depth={depth} width={width} offset={offset} pattern={pattern}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn unpack_preserves_short_and_extra_input_behavior() {
        let bytes = [0x96, 0x69, 0x37, 0xc8, 0xff];
        for depth in [1, 2, 4] {
            for input_len in 0..=bytes.len() {
                for width in 0..=4 * 8 / usize::from(depth) + 1 {
                    let mut actual = vec![0xa5; width + 3];
                    let mut expected = actual.clone();
                    reference_unpack_bits(depth, &bytes[..input_len], &mut expected[1..1 + width]);
                    unpack_depth(depth, &bytes[..input_len], &mut actual[1..1 + width]);
                    assert_eq!(
                        actual, expected,
                        "depth={depth} input_len={input_len} width={width}"
                    );
                }
            }
        }
    }

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
            for len in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 15, 16, 17, 31, 32, 33, 127]
                .into_iter()
                .chain([bpp + 31, bpp + 32, bpp + 33, bpp + 63, bpp + 64, bpp + 65])
            {
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
                        actual.fill(0xa5);
                        assert_eq!(
                            filter_into::<false>(&row, predictors, bpp, filter, &mut actual),
                            0
                        );
                        assert_eq!(actual, expected);
                        unfilter(&mut actual, &prev, bpp, filter).unwrap();
                        assert_eq!(actual, row);
                        if prev.iter().all(|&v| v == 0) {
                            let mut first_filtered = vec![0xa5; len];
                            assert_eq!(
                                filter_first::<true>(&row, bpp, filter, &mut first_filtered),
                                score
                            );
                            assert_eq!(first_filtered, expected);
                            first_filtered.fill(0xa5);
                            assert_eq!(
                                filter_first::<false>(&row, bpp, filter, &mut first_filtered),
                                0
                            );
                            assert_eq!(first_filtered, expected);
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
