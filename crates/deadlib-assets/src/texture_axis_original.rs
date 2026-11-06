//! Frozen texture functions from a2d215ee50f2a90cf7424e9c314b6fd8380704a1.
use super::*;
pub fn apply_texture_hints(image: &mut RgbaImage, hints: &TextureHints) {
    if !(hints.grayscale || hints.alphamap) {
        return;
    }

    for pixel in image.as_mut().as_chunks_mut::<4>().0 {
        let word = u32::from_le_bytes(*pixel);
        let lum = ((word & 255) * 30 + ((word >> 8) & 255) * 59 + ((word >> 16) & 255) * 11) / 100;
        let output = if hints.alphamap {
            0x00ff_ffff | (lum << 24)
        } else {
            (word & 0xff00_0000) | (lum * 0x0001_0101)
        };
        *pixel = output.to_le_bytes();
    }
}
fn zoom_axis(source: u32, destination: u32) -> impl Iterator<Item = (u32, u32, u32)> {
    let ratio = source as f32 / destination as f32;
    (0..destination).map(move |x| {
        if source >= destination {
            let center = ratio * x as f32 + ratio / 2.0;
            let left = (center - ratio / 4.0) as u32;
            let right = (center + ratio / 4.0) as u32;
            let weight = if left == right {
                1 << 24
            } else {
                ((1.0 - (center - (left as f32 + 0.5)) / (right - left) as f32) * 16777216.0) as u32
            };
            (left, right, weight)
        } else {
            let center = (source - 1) as f32 / (destination - 1) as f32 * x as f32;
            (
                (center as u32).min(source - 1),
                ((center + 1.0) as u32).min(source - 1),
                ((1.0 - (center - center.floor())) * 16777216.0) as u32,
            )
        }
    })
}
#[inline(never)]
fn zoom_half(image: &RgbaImage, size: [u32; 2]) -> RgbaImage {
    let mut output = RgbaImage::new(size[0], size[1]);
    let source = image.as_raw().as_chunks::<4>().0;
    for (rows, row) in source.chunks_exact(image.width() as usize * 2).zip(
        output
            .as_mut()
            .as_chunks_mut::<4>()
            .0
            .chunks_exact_mut(size[0] as usize),
    ) {
        let (top, bottom) = rows.split_at(image.width() as usize);
        for ((pixel, top), bottom) in row
            .iter_mut()
            .zip(top.as_chunks::<2>().0)
            .zip(bottom.as_chunks::<2>().0)
        {
            let [a, b] = top.map(u32::from_le_bytes);
            let [c, d] = bottom.map(u32::from_le_bytes);
            let upper = (a & b) + (((a ^ b) & 0xfefe_fefe) >> 1);
            let lower = (c & d) + (((c ^ d) & 0xfefe_fefe) >> 1);
            *pixel = ((upper | lower) - (((upper ^ lower) & 0xfefe_fefe) >> 1)).to_le_bytes();
        }
    }
    output
}
pub(super) fn zoom_image(mut image: RgbaImage, size: [u32; 2]) -> RgbaImage {
    while [image.width(), image.height()] != size {
        let next = std::array::from_fn::<_, 2, _>(|axis| {
            let current = [image.width(), image.height()][axis];
            let ratio = (size[axis] as f32 / current as f32).clamp(0.5, 2.0);
            (current as f32 * ratio).round_ties_even() as u32
        });
        // Beyond 2^23, f32 coordinates no longer preserve half-pixel steps.
        if image.width().max(image.height()) <= 1 << 23
            && [next[0] * 2, next[1] * 2] == [image.width(), image.height()]
        {
            image = zoom_half(&image, next);
            continue;
        }
        let xs: Vec<_> = zoom_axis(image.width(), next[0]).collect();
        let ys = zoom_axis(image.height(), next[1]);
        let mut output = RgbaImage::new(next[0], next[1]);
        let width = image.width() as usize;
        let source = image.as_raw().as_chunks::<4>().0;
        for (row, (top, bottom, wy)) in output
            .as_mut()
            .as_chunks_mut::<4>()
            .0
            .chunks_exact_mut(next[0] as usize)
            .zip(ys)
        {
            let upper_row = &source[top as usize * width..(top as usize + 1) * width];
            let lower_row = &source[bottom as usize * width..(bottom as usize + 1) * width];
            for (pixel, &(left, right, wx)) in row.iter_mut().zip(&xs) {
                // Two 32-bit lanes hold independent 8-bit channels and their
                // 24-bit products. Complementary weights cannot carry between lanes.
                let spread = |p: [u8; 4]| {
                    [
                        u64::from(p[0]) | (u64::from(p[2]) << 32),
                        u64::from(p[1]) | (u64::from(p[3]) << 32),
                    ]
                };
                let c00 = spread(upper_row[left as usize]);
                let c01 = spread(upper_row[right as usize]);
                let c10 = spread(lower_row[left as usize]);
                let c11 = spread(lower_row[right as usize]);
                for c in 0..2 {
                    let upper = ((c00[c] * u64::from(wx) + c01[c] * u64::from(16777216 - wx))
                        >> 24)
                        & 0x0000_00ff_0000_00ff;
                    let lower = ((c10[c] * u64::from(wx) + c11[c] * u64::from(16777216 - wx))
                        >> 24)
                        & 0x0000_00ff_0000_00ff;
                    let pair = (upper * u64::from(wy)
                        + lower * u64::from(16777216 - wy)
                        + 0x0080_0000_0080_0000)
                        >> 24;
                    pixel[c] = pair as u8;
                    pixel[c + 2] = (pair >> 32) as u8;
                }
            }
        }
        image = output;
    }
    image
}
