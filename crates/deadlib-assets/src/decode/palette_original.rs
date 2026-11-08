//! Frozen decoder bodies from f2dd892e7; shared pixel helpers are unchanged.
use super::*;

pub(super) fn decode_gif_before(path: &Path, color_key: bool) -> image::ImageResult<RgbaImage> {
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
    let stride = size[0] as usize * 4;
    for row_index in 0..size[1] {
        let y = if top_down {
            row_index
        } else {
            size[1] - 1 - row_index
        };
        input.read_exact(&mut row)?;
        let pixels = image.as_mut()[y as usize * stride..][..stride]
            .as_chunks_mut::<4>()
            .0;
        match bits {
            1 => unpack_palette::<8>(&row, pixels, &colors),
            4 => unpack_palette::<2>(&row, pixels, &colors),
            _ => unpack_palette::<1>(&row, pixels, &colors),
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
