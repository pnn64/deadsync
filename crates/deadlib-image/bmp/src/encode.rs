use crate::{Bitmap, Error, Pixels, reserve};

/// Storage orientation and output size limit for BMP encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EncodeOptions {
    /// Store the top row first and write a negative Windows DIB height.
    /// Bitmap pixels always remain in top-to-bottom order.
    pub top_down: bool,
    /// Maximum size of the complete encoded file, including headers and padding.
    pub max_output_bytes: usize,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            top_down: false,
            max_output_bytes: 256 << 20,
        }
    }
}

/// Encode a BMP with bottom-up rows and the default output limit.
///
/// RGB and indexed input produce a 24-bit BITMAPINFOHEADER image. Indexed pixels
/// expand directly into the output rows. RGBA input produces a 32-bit
/// BITMAPV4HEADER image with explicit alpha masks, preserving zero alpha.
pub fn encode(bitmap: &Bitmap) -> Result<Vec<u8>, Error> {
    encode_with_options(bitmap, EncodeOptions::default())
}

/// Encode a BMP with an explicit row orientation and output limit.
///
/// The source bitmap is validated before encoding. Dimensions must fit positive
/// signed 32-bit Windows DIB fields, and the complete file must fit its unsigned
/// 32-bit size field as well as the configured output limit.
pub fn encode_with_options(bitmap: &Bitmap, options: EncodeOptions) -> Result<Vec<u8>, Error> {
    bitmap.validate()?;
    let width = i32::try_from(bitmap.width).map_err(|_| Error::LimitExceeded)?;
    let height = i32::try_from(bitmap.height).map_err(|_| Error::LimitExceeded)?;
    let (header_size, channels): (usize, usize) = match &bitmap.pixels {
        Pixels::Indexed { .. } | Pixels::Rgb(_) => (40, 3),
        Pixels::Rgba(_) => (108, 4),
    };
    let row_bytes = (width as usize)
        .checked_mul(channels)
        .ok_or(Error::LimitExceeded)?;
    let pitch = row_bytes.checked_add(3).ok_or(Error::LimitExceeded)? & !3;
    let pixel_bytes = pitch
        .checked_mul(height as usize)
        .ok_or(Error::LimitExceeded)?;
    let offset = 14 + header_size;
    let file_size = offset
        .checked_add(pixel_bytes)
        .ok_or(Error::LimitExceeded)?;
    let file_size_u32 = u32::try_from(file_size).map_err(|_| Error::LimitExceeded)?;
    if file_size > options.max_output_bytes {
        return Err(Error::LimitExceeded);
    }

    let mut output = Vec::new();
    reserve(&mut output, file_size)?;
    output.resize(file_size, 0);
    output[..2].copy_from_slice(b"BM");
    put_u32(&mut output, 2, file_size_u32);
    put_u32(&mut output, 10, offset as u32);
    put_u32(&mut output, 14, header_size as u32);
    put_u32(&mut output, 18, width as u32);
    let stored_height = if options.top_down { -height } else { height };
    put_u32(&mut output, 22, stored_height as u32);
    output[26..28].copy_from_slice(&1u16.to_le_bytes());
    output[28..30].copy_from_slice(&((channels * 8) as u16).to_le_bytes());
    put_u32(&mut output, 34, pixel_bytes as u32);
    if channels == 4 {
        put_u32(&mut output, 30, 3); // BI_BITFIELDS
        put_u32(&mut output, 54, 0x00ff_0000);
        put_u32(&mut output, 58, 0x0000_ff00);
        put_u32(&mut output, 62, 0x0000_00ff);
        put_u32(&mut output, 66, 0xff00_0000);
        put_u32(&mut output, 70, 0x7352_4742); // LCS_sRGB
    }

    let width = width as usize;
    let height = height as usize;
    for file_row in 0..height {
        let source_row = if options.top_down {
            file_row
        } else {
            height - 1 - file_row
        };
        let first_pixel = source_row * width;
        let row = &mut output[offset + file_row * pitch..][..row_bytes];
        match &bitmap.pixels {
            Pixels::Indexed {
                indices, palette, ..
            } => {
                for (&index, bgr) in indices[first_pixel..first_pixel + width]
                    .iter()
                    .zip(row.as_chunks_mut::<3>().0)
                {
                    let rgb = palette[usize::from(index)];
                    bgr.copy_from_slice(&[rgb[2], rgb[1], rgb[0]]);
                }
            }
            Pixels::Rgb(samples) => {
                let source = &samples[first_pixel * 3..(first_pixel + width) * 3];
                for (rgb, bgr) in source
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .zip(row.as_chunks_mut::<3>().0)
                {
                    bgr.copy_from_slice(&[rgb[2], rgb[1], rgb[0]]);
                }
            }
            Pixels::Rgba(samples) => {
                let source = &samples[first_pixel * 4..(first_pixel + width) * 4];
                for (rgba, bgra) in source
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(row.as_chunks_mut::<4>().0)
                {
                    bgra.copy_from_slice(&[rgba[2], rgba[1], rgba[0], rgba[3]]);
                }
            }
        }
    }
    Ok(output)
}

fn put_u32(output: &mut [u8], offset: usize, value: u32) {
    output[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
