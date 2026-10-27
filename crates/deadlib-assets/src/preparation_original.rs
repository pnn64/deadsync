use super::*;

pub(super) fn decode_png(path: &Path, color_key: bool) -> image::ImageResult<RgbaImage> {
    let mut decoder = png::Decoder::new(BufReader::new(std::fs::File::open(path)?));
    // Match the image loader's allocation limit instead of png's smaller default.
    decoder.set_limits(png::Limits {
        bytes: image::Limits::default()
            .max_alloc
            .unwrap_or(usize::MAX as u64)
            .min(usize::MAX as u64) as usize,
    });
    let header = decoder.read_header_info().map_err(png_error)?;
    check_image_alloc([header.width, header.height])?;
    let indexed = header.color_type == png::ColorType::Indexed;
    decoder.set_transformations(
        png::Transformations::STRIP_16
            | if indexed {
                png::Transformations::IDENTITY
            } else {
                png::Transformations::EXPAND
            },
    );
    let mut reader = decoder.read_info().map_err(png_error)?;
    let palette = reader.info().palette.clone();
    let alpha = reader.info().trns.clone();
    let mut bytes = vec![0; reader.output_buffer_size().ok_or_else(image_limit)?];
    let info = reader.next_frame(&mut bytes).map_err(png_error)?;
    if info.color_type == png::ColorType::Rgba {
        // STRIP_16 leaves tightly packed RGBA8, already in the upload layout.
        bytes.truncate(info.buffer_size());
        let mut image = RgbaImage::from_raw(info.width, info.height, bytes)
            .expect("decoded RGBA frame fits its output buffer");
        if color_key {
            key_rgba(&mut image);
        }
        return Ok(image);
    }
    let mut image = RgbaImage::new(info.width, info.height);
    if indexed {
        let mut colors = [[0, 0, 0, 255]; 256];
        for (i, rgb) in palette
            .as_deref()
            .unwrap_or_default()
            .chunks_exact(3)
            .enumerate()
        {
            colors[i] = [
                rgb[0],
                rgb[1],
                rgb[2],
                alpha
                    .as_deref()
                    .and_then(|a| a.get(i))
                    .copied()
                    .unwrap_or(255),
            ];
        }
        if color_key {
            key_palette(&mut colors);
        }
        let bits = info.bit_depth as usize;
        let pixels = image.as_mut().as_chunks_mut::<4>().0;
        if bits == 8 {
            for (pixel, &index) in pixels.iter_mut().zip(&bytes[..info.buffer_size()]) {
                *pixel = colors[index as usize];
            }
        } else {
            let mask = ((1u16 << bits) - 1) as u8;
            for (pixels, row) in pixels
                .chunks_exact_mut(info.width as usize)
                .zip(bytes.chunks_exact(info.line_size))
            {
                for (x, pixel) in pixels.iter_mut().enumerate() {
                    let bit = x * bits;
                    let index = (row[bit / 8] >> (8 - bits - bit % 8)) & mask;
                    *pixel = colors[index as usize];
                }
            }
        }
    } else {
        let channels = info.color_type.samples();
        for (pixel, source) in image
            .pixels_mut()
            .zip(bytes[..info.buffer_size()].chunks_exact(channels))
        {
            pixel.0 = match info.color_type {
                png::ColorType::Rgb => [source[0], source[1], source[2], 255],
                png::ColorType::Grayscale => [source[0], source[0], source[0], 255],
                png::ColorType::GrayscaleAlpha => [source[0], source[0], source[0], source[1]],
                png::ColorType::Rgba | png::ColorType::Indexed => {
                    unreachable!("RGBA and indexed branches handled above")
                }
            };
        }
        if color_key {
            key_rgba(&mut image);
        }
    }
    Ok(image)
}

pub(super) fn zoom_axis(source: u32, destination: u32) -> Vec<(u32, u32, u32)> {
    let ratio = source as f32 / destination as f32;
    (0..destination)
        .map(|x| {
            if source >= destination {
                let center = ratio * x as f32 + ratio / 2.0;
                let left = (center - ratio / 4.0) as u32;
                let right = (center + ratio / 4.0) as u32;
                let weight = if left == right {
                    1 << 24
                } else {
                    ((1.0 - (center - (left as f32 + 0.5)) / (right - left) as f32) * 16777216.0)
                        as u32
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
        .collect()
}

pub(super) fn zoom_image(mut image: RgbaImage, size: [u32; 2]) -> RgbaImage {
    while [image.width(), image.height()] != size {
        let next = std::array::from_fn::<_, 2, _>(|axis| {
            let current = [image.width(), image.height()][axis];
            let ratio = (size[axis] as f32 / current as f32).clamp(0.5, 2.0);
            (current as f32 * ratio).round_ties_even() as u32
        });
        let xs = zoom_axis(image.width(), next[0]);
        let ys = zoom_axis(image.height(), next[1]);
        let mut output = RgbaImage::new(next[0], next[1]);
        let width = image.width() as usize;
        let source = image.as_raw().as_chunks::<4>().0;
        for (row, &(top, bottom, wy)) in output
            .as_mut()
            .as_chunks_mut::<4>()
            .0
            .chunks_exact_mut(next[0] as usize)
            .zip(&ys)
        {
            let upper_row = &source[top as usize * width..(top as usize + 1) * width];
            let lower_row = &source[bottom as usize * width..(bottom as usize + 1) * width];
            for (pixel, &(left, right, wx)) in row.iter_mut().zip(&xs) {
                let c00 = upper_row[left as usize];
                let c01 = upper_row[right as usize];
                let c10 = lower_row[left as usize];
                let c11 = lower_row[right as usize];
                for c in 0..4 {
                    let upper =
                        (u32::from(c00[c]) * wx + u32::from(c01[c]) * (16777216 - wx)) >> 24;
                    let lower =
                        (u32::from(c10[c]) * wx + u32::from(c11[c]) * (16777216 - wx)) >> 24;
                    pixel[c] = ((upper * wy + lower * (16777216 - wy) + 8388608) >> 24) as u8;
                }
            }
        }
        image = output;
    }
    image
}

pub(super) fn apply_texture_hints(image: &mut RgbaImage, hints: &TextureHints) {
    if !(hints.grayscale || hints.alphamap) {
        return;
    }

    for pixel in image.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        let lum = ((u16::from(r) * 30 + u16::from(g) * 59 + u16::from(b) * 11) / 100) as u8;
        if hints.alphamap {
            pixel.0 = [255, 255, 255, lum];
        } else {
            pixel.0 = [lum, lum, lum, a];
        }
    }
}
