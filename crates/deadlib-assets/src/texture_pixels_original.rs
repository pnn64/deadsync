// Frozen from 458c71c88 for byte-exact comparisons and paired benchmarks.

fn decode_png_original(path: &Path, color_key: bool) -> image::ImageResult<RgbaImage> {
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
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            let bit = x as usize * bits;
            let byte = bytes[y as usize * info.line_size + bit / 8];
            let index = (byte >> (8 - bits - bit % 8)) & ((1u16 << bits) - 1) as u8;
            pixel.0 = colors[index as usize];
        }
    } else {
        let channels = info.color_type.samples();
        for (pixel, source) in image
            .pixels_mut()
            .zip(bytes[..info.buffer_size()].chunks_exact(channels))
        {
            pixel.0 = match info.color_type {
                png::ColorType::Rgb => [source[0], source[1], source[2], 255],
                png::ColorType::Rgba => [source[0], source[1], source[2], source[3]],
                png::ColorType::Grayscale => [source[0], source[0], source[0], 255],
                png::ColorType::GrayscaleAlpha => [source[0], source[0], source[0], source[1]],
                png::ColorType::Indexed => unreachable!("indexed branch handled above"),
            };
        }
        if color_key {
            key_rgba(&mut image);
        }
    }
    Ok(image)
}

fn decode_gif_original(path: &Path, color_key: bool) -> image::ImageResult<RgbaImage> {
    let mut reader = gif_reader(path)?;
    let canvas = [u32::from(reader.width()), u32::from(reader.height())];
    check_image_alloc(canvas)?;
    let global = reader.global_palette().map(<[u8]>::to_vec);
    let frame = reader
        .next_frame_info()
        .map_err(gif_error)?
        .ok_or_else(|| decode_error(image::ImageFormat::Gif, "GIF has no frame"))?;
    let size = [u32::from(frame.width), u32::from(frame.height)];
    let origin = [u32::from(frame.left), u32::from(frame.top)];
    if size.contains(&0)
        || canvas.contains(&0)
        || origin[0] + size[0] > canvas[0]
        || origin[1] + size[1] > canvas[1]
    {
        return Err(image_limit());
    }
    check_image_alloc(size)?;
    // The primary stb loader starts with a transparent logical canvas and
    // leaves transparent first-frame pixels unwritten.
    let mut colors = [[0; 4]; 256];
    for (color, rgb) in colors.iter_mut().zip(
        frame
            .palette
            .as_deref()
            .or(global.as_deref())
            .unwrap_or_default()
            .chunks_exact(3),
    ) {
        *color = [rgb[0], rgb[1], rgb[2], 255];
    }
    if let Some(index) = frame.transparent {
        colors[index as usize] = [0; 4];
    }
    let mut indices = vec![0; size[0] as usize * size[1] as usize];
    reader.read_into_buffer(&mut indices).map_err(gif_error)?;
    let mut image = RgbaImage::new(canvas[0], canvas[1]);
    for (ordinal, index) in indices.into_iter().enumerate() {
        let x = origin[0] + ordinal as u32 % size[0];
        let y = origin[1] + ordinal as u32 / size[0];
        image.put_pixel(x, y, image::Rgba(colors[index as usize]));
    }
    if color_key {
        key_rgba(&mut image);
    }
    Ok(image)
}

fn zoom_image_original(mut image: RgbaImage, size: [u32; 2]) -> RgbaImage {
    while [image.width(), image.height()] != size {
        let next = std::array::from_fn::<_, 2, _>(|axis| {
            let current = [image.width(), image.height()][axis];
            let ratio = (size[axis] as f32 / current as f32).clamp(0.5, 2.0);
            (current as f32 * ratio).round_ties_even() as u32
        });
        let xs: Vec<_> = zoom_axis(image.width(), next[0]).collect();
        let ys: Vec<_> = zoom_axis(image.height(), next[1]).collect();
        let mut output = RgbaImage::new(next[0], next[1]);
        for (y, &(top, bottom, wy)) in ys.iter().enumerate() {
            for (x, &(left, right, wx)) in xs.iter().enumerate() {
                let c00 = image.get_pixel(left, top).0;
                let c01 = image.get_pixel(right, top).0;
                let c10 = image.get_pixel(left, bottom).0;
                let c11 = image.get_pixel(right, bottom).0;
                let pixel = output.get_pixel_mut(x as u32, y as u32);
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
