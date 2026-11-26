use std::io::Cursor;

// Consume the native sample representation directly, as an RGBA8 asset consumer
// would. In particular, RGBA8 reuses its allocation and 16-bit samples never pass
// through an intermediate RGBA16 image.
pub fn native_rgba8(bytes: &[u8]) -> image::RgbaImage {
    use deadlib_image_png::{ColorType, Samples, Transparency};
    let png = deadlib_image_png::decode(bytes).unwrap();
    let image = png.image;
    let (width, height, color, depth) =
        (image.width, image.height, image.color_type, image.bit_depth);
    let mask = ((1u32 << depth) - 1) as u16;
    let transparent_gray = match &png.transparency {
        Some(Transparency::Grayscale(value)) => Some(*value & mask),
        _ => None,
    };
    let transparent_rgb = match &png.transparency {
        Some(Transparency::Rgb(value)) => Some(value.map(|v| v & mask)),
        _ => None,
    };
    let pixel_count = usize::try_from(u64::from(width) * u64::from(height)).unwrap();
    let pixels = match image.samples {
        Samples::U8(samples) if color == ColorType::Rgba => samples,
        Samples::U8(samples) => {
            let mut output: Vec<[u8; 4]> = Vec::with_capacity(pixel_count);
            match color {
                ColorType::Indexed => {
                    let mut palette = [[0, 0, 0, 255]; 256];
                    for (out, &[r, g, b]) in palette.iter_mut().zip(&png.palette) {
                        *out = [r, g, b, 255];
                    }
                    if let Some(Transparency::Palette(alpha)) = &png.transparency {
                        for (out, &alpha) in palette.iter_mut().take(png.palette.len()).zip(alpha) {
                            out[3] = alpha;
                        }
                    }
                    output.extend(samples.into_iter().map(|index| palette[usize::from(index)]));
                }
                ColorType::Grayscale => {
                    let factor = (255 / mask) as u8;
                    output.extend(samples.into_iter().map(|value| {
                        let gray = value * factor;
                        let alpha = if transparent_gray == Some(u16::from(value)) {
                            0
                        } else {
                            255
                        };
                        [gray, gray, gray, alpha]
                    }));
                }
                ColorType::Rgb => {
                    output.extend(samples.as_chunks::<3>().0.iter().map(|rgb| {
                        let alpha = if transparent_rgb == Some(rgb.map(u16::from)) {
                            0
                        } else {
                            255
                        };
                        [rgb[0], rgb[1], rgb[2], alpha]
                    }));
                }
                ColorType::GrayscaleAlpha => {
                    output.extend(
                        samples
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|&[gray, alpha]| [gray, gray, gray, alpha]),
                    );
                }
                ColorType::Rgba => unreachable!(),
            }
            output.into_flattened()
        }
        Samples::U16(samples) => {
            let mut output: Vec<[u8; 4]> = Vec::with_capacity(pixel_count);
            match color {
                ColorType::Grayscale => {
                    output.extend(samples.into_iter().map(|value| {
                        let gray = (value >> 8) as u8;
                        let alpha = if transparent_gray == Some(value) {
                            0
                        } else {
                            255
                        };
                        [gray, gray, gray, alpha]
                    }));
                }
                ColorType::Rgb => {
                    output.extend(samples.as_chunks::<3>().0.iter().map(|rgb| {
                        let alpha = if transparent_rgb == Some(*rgb) {
                            0
                        } else {
                            255
                        };
                        [
                            (rgb[0] >> 8) as u8,
                            (rgb[1] >> 8) as u8,
                            (rgb[2] >> 8) as u8,
                            alpha,
                        ]
                    }));
                }
                ColorType::GrayscaleAlpha => {
                    output.extend(samples.as_chunks::<2>().0.iter().map(|&[gray, alpha]| {
                        let gray = (gray >> 8) as u8;
                        [gray, gray, gray, (alpha >> 8) as u8]
                    }));
                }
                ColorType::Rgba => {
                    output.extend(
                        samples
                            .as_chunks::<4>()
                            .0
                            .iter()
                            .map(|pixel| pixel.map(|value| (value >> 8) as u8)),
                    );
                }
                ColorType::Indexed => unreachable!("PNG has no 16-bit palette indices"),
            }
            output.into_flattened()
        }
    };
    image::RgbaImage::from_raw(width, height, pixels).unwrap()
}

fn unpack_palette_rgba<const N: usize>(
    row: &[u8],
    pixels: &mut [[u8; 4]],
    palette: &[[u8; 4]; 256],
) {
    let index = |byte: u8, i| (byte >> (8 - (i + 1) * (8 / N))) & (u8::MAX >> (8 - 8 / N));
    let (groups, tail) = pixels.as_chunks_mut::<N>();
    for (pixels, &byte) in groups.iter_mut().zip(row) {
        *pixels = std::array::from_fn(|i| palette[index(byte, i) as usize]);
    }
    if !tail.is_empty() {
        let byte = row[groups.len()];
        for (i, pixel) in tail.iter_mut().enumerate() {
            *pixel = palette[index(byte, i) as usize];
        }
    }
}

// Match deadlib-assets::decode_png_from_reader with color_key=false. Memory input
// removes filesystem I/O equally from all adapters. The loader deliberately
// stops at next_frame rather than finishing the PNG stream.
pub fn png_assets_rgba8(bytes: &[u8]) -> image::RgbaImage {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    let max_alloc = image::Limits::default().max_alloc.unwrap_or(u64::MAX);
    decoder.set_limits(png::Limits {
        bytes: max_alloc.min(usize::MAX as u64) as usize,
    });
    let header = decoder.read_header_info().unwrap();
    assert!(u64::from(header.width) * u64::from(header.height) <= max_alloc / 4);
    let indexed = header.color_type == png::ColorType::Indexed;
    decoder.set_transformations(
        png::Transformations::STRIP_16
            | if indexed {
                png::Transformations::IDENTITY
            } else {
                png::Transformations::EXPAND
            },
    );
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    if info.color_type == png::ColorType::Rgba {
        bytes.truncate(info.buffer_size());
        return image::RgbaImage::from_raw(info.width, info.height, bytes).unwrap();
    }
    let mut output = image::RgbaImage::new(info.width, info.height);
    let pixels = output.as_mut().as_chunks_mut::<4>().0;
    if indexed {
        let palette = reader.info().palette.as_deref();
        let alpha = reader.info().trns.as_deref();
        let mut colors = [[0, 0, 0, 255]; 256];
        for (i, rgb) in palette
            .unwrap_or_default()
            .as_chunks::<3>()
            .0
            .iter()
            .enumerate()
        {
            colors[i] = [
                rgb[0],
                rgb[1],
                rgb[2],
                alpha.and_then(|a| a.get(i)).copied().unwrap_or(255),
            ];
        }
        if info.bit_depth == png::BitDepth::Eight {
            for (pixel, &index) in pixels.iter_mut().zip(&bytes[..info.buffer_size()]) {
                *pixel = colors[usize::from(index)];
            }
        } else {
            for (pixels, row) in pixels
                .chunks_exact_mut(info.width as usize)
                .zip(bytes.chunks_exact(info.line_size))
            {
                match info.bit_depth {
                    png::BitDepth::One => unpack_palette_rgba::<8>(row, pixels, &colors),
                    png::BitDepth::Two => unpack_palette_rgba::<4>(row, pixels, &colors),
                    png::BitDepth::Four => unpack_palette_rgba::<2>(row, pixels, &colors),
                    _ => unreachable!(),
                }
            }
        }
    } else {
        let bytes = &bytes[..info.buffer_size()];
        match info.color_type {
            png::ColorType::Rgb => {
                for (pixel, rgb) in pixels.iter_mut().zip(bytes.as_chunks::<3>().0) {
                    *pixel = [rgb[0], rgb[1], rgb[2], 255];
                }
            }
            png::ColorType::Grayscale => {
                for (pixel, &gray) in pixels.iter_mut().zip(bytes) {
                    *pixel = [gray, gray, gray, 255];
                }
            }
            png::ColorType::GrayscaleAlpha => {
                for (pixel, &[gray, alpha]) in pixels.iter_mut().zip(bytes.as_chunks::<2>().0) {
                    *pixel = [gray, gray, gray, alpha];
                }
            }
            png::ColorType::Rgba | png::ColorType::Indexed => unreachable!(),
        }
    }
    output
}

pub fn image_rgba8(bytes: &[u8]) -> image::RgbaImage {
    use image::DynamicImage;
    let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).unwrap();
    let (width, height) = (decoded.width(), decoded.height());
    if decoded.color().bits_per_pixel() / u16::from(decoded.color().channel_count()) != 16 {
        return decoded.into_rgba8();
    }
    // image's normal u16->u8 conversion rounds. The asset loader instead keeps
    // the high byte, after png has matched any full-precision tRNS value.
    let mut pixels: Vec<[u8; 4]> =
        Vec::with_capacity(usize::try_from(u64::from(width) * u64::from(height)).unwrap());
    match decoded {
        DynamicImage::ImageLuma16(image) => {
            pixels.extend(image.into_raw().into_iter().map(|value| {
                let gray = (value >> 8) as u8;
                [gray, gray, gray, 255]
            }));
        }
        DynamicImage::ImageLumaA16(image) => {
            pixels.extend(
                image
                    .as_raw()
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&[gray, alpha]| {
                        let gray = (gray >> 8) as u8;
                        [gray, gray, gray, (alpha >> 8) as u8]
                    }),
            );
        }
        DynamicImage::ImageRgb16(image) => {
            pixels.extend(image.as_raw().as_chunks::<3>().0.iter().map(|rgb| {
                [
                    (rgb[0] >> 8) as u8,
                    (rgb[1] >> 8) as u8,
                    (rgb[2] >> 8) as u8,
                    255,
                ]
            }));
        }
        DynamicImage::ImageRgba16(image) => {
            pixels.extend(
                image
                    .as_raw()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|pixel| pixel.map(|value| (value >> 8) as u8)),
            );
        }
        _ => unreachable!("PNG decoding returns integer samples"),
    }
    image::RgbaImage::from_raw(width, height, pixels.into_flattened()).unwrap()
}
