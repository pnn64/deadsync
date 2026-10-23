// Frozen from f5a7daa09; test-only exact-behavior and performance reference.

fn decode_indexed_bmp_original(
    path: &Path,
    color_key: bool,
) -> image::ImageResult<Option<RgbaImage>> {
    let mut input = BufReader::new(std::fs::File::open(path)?);
    let mut header = [0; 54];
    input.read_exact(&mut header[..18])?;
    let le32 = |offset| {
        u32::from_le_bytes([
            header[offset],
            header[offset + 1],
            header[offset + 2],
            header[offset + 3],
        ])
    };
    let header_size = le32(14);
    if !matches!(header_size, 12 | 40) {
        return Ok(None);
    }
    let data_offset = le32(10);
    input.read_exact(&mut header[18..14 + header_size as usize])?;
    let le16 = |offset| u16::from_le_bytes([header[offset], header[offset + 1]]);
    let le32 = |offset| {
        u32::from_le_bytes([
            header[offset],
            header[offset + 1],
            header[offset + 2],
            header[offset + 3],
        ])
    };
    let (size, top_down, planes, bits, compression) = if header_size == 12 {
        (
            [u32::from(le16(18)), u32::from(le16(20))],
            false,
            le16(22),
            le16(24),
            0,
        )
    } else {
        let height = le32(22) as i32;
        (
            [le32(18), height.unsigned_abs()],
            height < 0,
            le16(26),
            le16(28),
            le32(30),
        )
    };
    if !matches!(bits, 1 | 4 | 8) || compression != 0 {
        return Ok(None);
    }
    if size.contains(&0) {
        return Err(image_limit());
    }
    check_image_alloc(size)?;
    // stb derives the available table from the pixel offset, rather than
    // biClrUsed. Preserve safe palette handling for minimal OS/2 files too;
    // the primary decoder's negative-size palette path has undefined pixels.
    let entry_size = if header_size == 12 { 3 } else { 4 };
    let color_count = data_offset
        .checked_sub(14 + header_size)
        .ok_or_else(|| decode_error(image::ImageFormat::Bmp, "invalid bitmap pixel offset"))?
        / entry_size;
    if planes != 1 || color_count == 0 || color_count > 256 {
        return Err(decode_error(
            image::ImageFormat::Bmp,
            "invalid indexed BMP planes or palette size",
        ));
    }
    let mut colors = [[0; 4]; 256];
    for color in &mut colors[..color_count as usize] {
        let mut entry = [0; 4];
        input.read_exact(&mut entry[..if header_size == 12 { 3 } else { 4 }])?;
        *color = [entry[2], entry[1], entry[0], 255];
    }
    let pitch = usize::try_from((u64::from(size[0]) * u64::from(bits)).div_ceil(32) * 4)
        .map_err(|_| image_limit())?;
    let mut row = vec![0; pitch];
    input.seek(SeekFrom::Start(u64::from(data_offset)))?;
    let mut image = RgbaImage::new(size[0], size[1]);
    for row_index in 0..size[1] {
        let y = if top_down {
            row_index
        } else {
            size[1] - 1 - row_index
        };
        input.read_exact(&mut row)?;
        for x in 0..size[0] {
            let index = match bits {
                1 => (row[x as usize / 8] >> (7 - x % 8)) & 1,
                4 => (row[x as usize / 2] >> (4 * (1 - x % 2))) & 15,
                _ => row[x as usize],
            };
            if u32::from(index) >= color_count {
                return Err(decode_error(
                    image::ImageFormat::Bmp,
                    "bitmap palette index out of range",
                ));
            }
            image.put_pixel(x, y, image::Rgba(colors[index as usize]));
        }
    }
    if color_key {
        key_rgba_original(&mut image);
    }
    Ok(Some(image))
}

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
    if info.color_type == png::ColorType::Rgba {
        // STRIP_16 leaves tightly packed RGBA8, already in the upload layout.
        bytes.truncate(info.buffer_size());
        let mut image = RgbaImage::from_raw(info.width, info.height, bytes)
            .expect("decoded RGBA frame fits its output buffer");
        if color_key {
            key_rgba_original(&mut image);
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
                png::ColorType::Grayscale => [source[0], source[0], source[0], 255],
                png::ColorType::GrayscaleAlpha => [source[0], source[0], source[0], source[1]],
                png::ColorType::Rgba | png::ColorType::Indexed => {
                    unreachable!("RGBA and indexed branches handled above")
                }
            };
        }
        if color_key {
            key_rgba_original(&mut image);
        }
    }
    Ok(image)
}

fn key_rgba_original(image: &mut RgbaImage) {
    let off_pink = [248, 0, 248, 255];
    let edge_has_key = image.width() > 0
        && image.height() > 0
        && (0..image.width()).any(|x| {
            image.get_pixel(x, 0).0 == off_pink
                || image.get_pixel(x, image.height() - 1).0 == off_pink
        });
    let key = if edge_has_key {
        off_pink
    } else {
        [255, 0, 255, 255]
    };
    for pixel in image.pixels_mut() {
        if pixel.0 == key {
            pixel.0 = [0; 4];
        }
    }
}
