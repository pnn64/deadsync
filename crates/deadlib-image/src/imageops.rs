use crate::{Rgba, RgbaImage};

/// Resize an RGBA raster with a separable triangle filter.
pub fn resize_triangle(image: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    let (source_width, source_height) = image.dimensions();
    if width == source_width && height == source_height {
        return image.clone();
    }
    if width == 0 || height == 0 || source_width == 0 || source_height == 0 {
        return RgbaImage::new(width, height);
    }
    let mut vertical = vec![[0.0; 4]; source_width as usize * height as usize];
    let mut weights = Vec::new();
    for (y, row) in vertical.chunks_exact_mut(source_width as usize).enumerate() {
        let start = triangle_weights(source_height, height, y as u32, &mut weights);
        for (x, pixel) in row.iter_mut().enumerate() {
            for (offset, &weight) in weights.iter().enumerate() {
                let source = image.get_pixel(x as u32, start + offset as u32);
                for (dest, value) in pixel.iter_mut().zip(source.0) {
                    *dest += f32::from(value) * weight;
                }
            }
        }
    }
    let mut output = RgbaImage::new(width, height);
    for x in 0..width {
        let start = triangle_weights(source_width, width, x, &mut weights) as usize;
        for (y, row) in vertical.chunks_exact(source_width as usize).enumerate() {
            let mut pixel = [0.0f32; 4];
            for (&source, &weight) in row[start..].iter().zip(&weights) {
                for (dest, value) in pixel.iter_mut().zip(source) {
                    *dest += value * weight;
                }
            }
            output.put_pixel(x, y as u32, Rgba(pixel.map(|value| value.round() as u8)));
        }
    }
    output
}

fn triangle_weights(source: u32, target: u32, index: u32, weights: &mut Vec<f32>) -> u32 {
    let ratio = source as f32 / target as f32;
    let support = ratio.max(1.0);
    let center = (index as f32 + 0.5) * ratio;
    let start = ((center - support).floor() as u32).min(source - 1);
    let end = ((center + support).ceil() as u32).clamp(start + 1, source);
    let comparison = center - 0.5;
    weights.clear();
    let mut total = 0.0;
    for input in start..end {
        let distance = ((input as f32 - comparison) / support).abs();
        let weight = (1.0 - distance).max(0.0);
        weights.push(weight);
        total += weight;
    }
    for weight in weights {
        *weight /= total;
    }
    start
}

/// Fit a raster inside the supplied bounds and average source pixels.
pub fn thumbnail_fit(image: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    let (source_width, source_height) = image.dimensions();
    let ratio = (f64::from(width) / f64::from(source_width))
        .min(f64::from(height) / f64::from(source_height));
    let target_width = ((f64::from(source_width) * ratio).round() as u32).max(1);
    let target_height = ((f64::from(source_height) * ratio).round() as u32).max(1);
    thumbnail_region(
        image,
        source_width,
        source_height,
        target_width,
        target_height,
    )
}

/// Resize the top-left frame of a raster, without copying that frame first.
pub fn thumbnail_region(
    image: &RgbaImage,
    source_width: u32,
    source_height: u32,
    width: u32,
    height: u32,
) -> RgbaImage {
    assert!(source_width <= image.width() && source_height <= image.height());
    if width == 0 || height == 0 || source_width == 0 || source_height == 0 {
        return RgbaImage::new(width, height);
    }
    let x_ratio = source_width as f32 / width as f32;
    let y_ratio = source_height as f32 / height as f32;
    RgbaImage::from_fn(width, height, |x, y| {
        let (left, right, xf) = thumbnail_span(x, x_ratio, source_width);
        let (top, bottom, yf) = thumbnail_span(y, y_ratio, source_height);
        match (left == right, top == bottom) {
            (false, false) => {
                let sum = sum_region(image, left..right, top..bottom);
                let count = u64::from(right - left) * u64::from(bottom - top);
                Rgba(sum.map(|value| ((value + count / 2) / count) as u8))
            }
            (true, false) => {
                let low = left.saturating_sub(1);
                let a = sum_region(image, low..low + 1, top..bottom);
                let b = sum_region(image, left..left + 1, top..bottom);
                mix_thumbnail(a, b, xf, bottom - top)
            }
            (false, true) => {
                let low = top.saturating_sub(1);
                let a = sum_region(image, left..right, low..low + 1);
                let b = sum_region(image, left..right, top..top + 1);
                mix_thumbnail(a, b, yf, right - left)
            }
            (true, true) => {
                let (x0, y0) = (left.saturating_sub(1), top.saturating_sub(1));
                let pixels = [
                    image.get_pixel(left, y0),
                    image.get_pixel(left, top),
                    image.get_pixel(x0, y0),
                    image.get_pixel(x0, top),
                ];
                let weights = [
                    xf * (1.0 - yf),
                    xf * yf,
                    (1.0 - xf) * (1.0 - yf),
                    (1.0 - xf) * yf,
                ];
                Rgba(std::array::from_fn(|channel| {
                    pixels
                        .iter()
                        .zip(weights)
                        .map(|(p, weight)| f32::from(p[channel]) * weight)
                        .sum::<f32>() as u8
                }))
            }
        }
    })
}

fn thumbnail_span(index: u32, ratio: f32, source: u32) -> (u32, u32, f32) {
    let start = index as f32 * ratio;
    let end = start + ratio;
    let low = (start.ceil() as u32).min(source - 1);
    let high = (end.ceil() as u32).clamp(low, source);
    (low, high, (start.fract() + end.fract()) / 2.0)
}

fn sum_region(image: &RgbaImage, x: std::ops::Range<u32>, y: std::ops::Range<u32>) -> [u64; 4] {
    let mut sum = [0; 4];
    for row in y {
        for column in x.clone() {
            for (total, value) in sum.iter_mut().zip(image.get_pixel(column, row).0) {
                *total += u64::from(value);
            }
        }
    }
    sum
}

fn mix_thumbnail(a: [u64; 4], b: [u64; 4], fraction: f32, count: u32) -> Rgba {
    let a_weight = (1.0 - fraction) / count as f32;
    let b_weight = fraction / count as f32;
    Rgba(std::array::from_fn(|i| {
        (a[i] as f32 * a_weight + b[i] as f32 * b_weight) as u8
    }))
}

/// Composite a straight-alpha raster at a nonnegative destination offset.
pub fn overlay(target: &mut RgbaImage, source: &RgbaImage, x: u32, y: u32) {
    let width = source.width().min(target.width().saturating_sub(x));
    let height = source.height().min(target.height().saturating_sub(y));
    for row in 0..height {
        for column in 0..width {
            let upper = source.get_pixel(column, row);
            match upper[3] {
                0 => {}
                255 => target.put_pixel(x + column, y + row, upper),
                alpha => {
                    let lower = target.get_pixel(x + column, y + row);
                    let upper_alpha = f32::from(alpha) / 255.0;
                    let lower_alpha = f32::from(lower[3]) / 255.0;
                    let alpha = lower_alpha + upper_alpha - lower_alpha * upper_alpha;
                    let mut pixel = [0; 4];
                    for i in 0..3 {
                        let over = upper_alpha * (f32::from(upper[i]) / 255.0);
                        let under = lower_alpha * (f32::from(lower[i]) / 255.0);
                        let output = (over + under * (1.0 - upper_alpha)) / alpha;
                        pixel[i] = (255.0 * output) as u8;
                    }
                    pixel[3] = (255.0 * alpha) as u8;
                    target.put_pixel(x + column, y + row, Rgba(pixel));
                }
            }
        }
    }
}
