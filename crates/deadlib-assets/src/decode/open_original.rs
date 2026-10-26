//! Frozen file dispatch and native decoders from 0e6c38a56.
use super::*;

pub(super) fn image_signature_before(path: &Path) -> image::ImageResult<[u8; 8]> {
    let mut signature = [0; 8];
    std::fs::File::open(path)?.read_exact(&mut signature)?;
    Ok(signature)
}

pub(super) fn texture_source_size_before(path: &Path) -> image::ImageResult<(u32, u32)> {
    let signature = image_signature_before(path)?;
    if &signature[..6] == b"GIF87a" || &signature[..6] == b"GIF89a" {
        let reader = gif_reader_before(path)?;
        if reader.width() == 0 || reader.height() == 0 {
            return Err(image_limit());
        }
        return Ok((u32::from(reader.width()), u32::from(reader.height())));
    }
    image::ImageReader::open(path)?
        .with_guessed_format()?
        .into_dimensions()
}

pub(super) fn decode_image_before(path: &Path, color_key: bool) -> image::ImageResult<RgbaImage> {
    let signature = image_signature_before(path)?;
    if signature == *b"\x89PNG\r\n\x1a\n" {
        return decode_png_before(path, color_key);
    }
    if &signature[..6] == b"GIF87a" || &signature[..6] == b"GIF89a" {
        return decode_gif_before(path, color_key);
    }
    if &signature[..2] == b"BM" {
        if let Some(image) = decode_indexed_bmp_before(path, color_key)? {
            return Ok(image);
        }
    }
    let mut image = open_image_fallback(path)?.into_rgba8();
    if color_key {
        key_rgba(&mut image);
    }
    Ok(image)
}

pub(super) fn decode_png_before(path: &Path, color_key: bool) -> image::ImageResult<RgbaImage> {
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
        // Indexed palettes are fixed before IDAT; borrow the decoder's storage.
        let palette = reader.info().palette.as_deref();
        let alpha = reader.info().trns.as_deref();
        let mut colors = [[0, 0, 0, 255]; 256];
        for (i, rgb) in palette.unwrap_or_default().chunks_exact(3).enumerate() {
            colors[i] = [
                rgb[0],
                rgb[1],
                rgb[2],
                alpha.and_then(|a| a.get(i)).copied().unwrap_or(255),
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
            for (pixels, row) in pixels
                .chunks_exact_mut(info.width as usize)
                .zip(bytes.chunks_exact(info.line_size))
            {
                match bits {
                    1 => unpack_palette::<8>(row, pixels, &colors),
                    2 => unpack_palette::<4>(row, pixels, &colors),
                    _ => unpack_palette::<2>(row, pixels, &colors),
                }
            }
        }
    } else {
        let pixels = image.as_mut().as_chunks_mut::<4>().0;
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
                for (pixel, ga) in pixels.iter_mut().zip(bytes.as_chunks::<2>().0) {
                    *pixel = [ga[0], ga[0], ga[0], ga[1]];
                }
            }
            png::ColorType::Rgba | png::ColorType::Indexed => {
                unreachable!("RGBA and indexed branches handled above")
            }
        }
        if color_key {
            key_rgba(&mut image);
        }
    }
    Ok(image)
}

pub(super) fn gif_reader_before(
    path: &Path,
) -> image::ImageResult<gif::Decoder<BufReader<std::fs::File>>> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::Indexed);
    let limit = std::num::NonZeroU64::new(image::Limits::default().max_alloc.unwrap_or(u64::MAX))
        .ok_or_else(image_limit)?;
    options.set_memory_limit(gif::MemoryLimit::Bytes(limit));
    options
        .read_info(BufReader::new(std::fs::File::open(path)?))
        .map_err(gif_error)
}

pub(super) fn decode_gif_before(path: &Path, color_key: bool) -> image::ImageResult<RgbaImage> {
    let mut reader = gif_reader_before(path)?;
    let canvas = [u32::from(reader.width()), u32::from(reader.height())];
    check_image_alloc(canvas)?;
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
    let transparent = frame.transparent;
    let palette = match frame.palette.as_deref() {
        Some(palette) => palette,
        None => reader.global_palette().unwrap_or_default(),
    };
    for (color, rgb) in colors.iter_mut().zip(palette.chunks_exact(3)) {
        *color = [rgb[0], rgb[1], rgb[2], 255];
    }
    if let Some(index) = transparent {
        colors[index as usize] = [0; 4];
    }
    let mut indices = vec![0; size[0] as usize * size[1] as usize];
    reader.read_into_buffer(&mut indices).map_err(gif_error)?;
    let mut image = RgbaImage::new(canvas[0], canvas[1]);
    for (source, row) in indices.chunks_exact(size[0] as usize).zip(
        image
            .as_mut()
            .chunks_exact_mut(canvas[0] as usize * 4)
            .skip(origin[1] as usize),
    ) {
        for (&index, pixel) in source
            .iter()
            .zip(row[origin[0] as usize * 4..].as_chunks_mut::<4>().0)
        {
            *pixel = colors[index as usize];
        }
    }
    if color_key {
        key_rgba(&mut image);
    }
    Ok(image)
}

pub(super) fn decode_indexed_bmp_before(
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
    // A palette has at most 256 four-byte entries. Reuse those bytes for rows.
    let mut scratch = [0; 1024];
    let palette = &mut scratch[..color_count as usize * entry_size as usize];
    input.read_exact(palette)?;
    for (color, entry) in colors
        .iter_mut()
        .zip(palette.chunks_exact(entry_size as usize))
    {
        *color = [entry[2], entry[1], entry[0], 255];
    }
    let pitch = usize::try_from((u64::from(size[0]) * u64::from(bits)).div_ceil(32) * 4)
        .map_err(|_| image_limit())?;
    let mut heap;
    let row = if pitch <= scratch.len() {
        &mut scratch[..pitch]
    } else {
        heap = vec![0; pitch];
        heap.as_mut_slice()
    };
    input.seek(SeekFrom::Start(u64::from(data_offset)))?;
    let mut image = RgbaImage::new(size[0], size[1]);
    let stride = size[0] as usize * 4;
    for row_index in 0..size[1] {
        let y = if top_down {
            row_index
        } else {
            size[1] - 1 - row_index
        };
        input.read_exact(row)?;
        let pixels = image.as_mut()[y as usize * stride..][..stride]
            .as_chunks_mut::<4>()
            .0;
        match bits {
            1 => unpack_palette::<8>(row, pixels, &colors),
            4 => unpack_palette::<2>(row, pixels, &colors),
            _ => unpack_palette::<1>(row, pixels, &colors),
        }
        // Valid BMP palette entries have alpha 255; missing entries retain 0.
        // A complete palette needs no validation, and padding is never decoded.
        if color_count < (1 << bits) && pixels.iter().any(|pixel| pixel[3] == 0) {
            return Err(decode_error(
                image::ImageFormat::Bmp,
                "bitmap palette index out of range",
            ));
        }
    }
    if color_key {
        key_rgba(&mut image);
    }
    Ok(Some(image))
}
