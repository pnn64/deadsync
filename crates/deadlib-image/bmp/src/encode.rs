use crate::{Bitmap, Error, Pixels, reserve};

const RGB_HEADER: [u8; 54] = header_template(24);
const RGBA_HEADER: [u8; 122] = header_template(32);

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
    let (header, channels): (&[u8], usize) = match &bitmap.pixels {
        Pixels::Indexed { .. } | Pixels::Rgb(_) => (&RGB_HEADER, 3),
        Pixels::Rgba(_) => (&RGBA_HEADER, 4),
    };
    let row_bytes = (width as usize)
        .checked_mul(channels)
        .ok_or(Error::LimitExceeded)?;
    let pitch = row_bytes.checked_add(3).ok_or(Error::LimitExceeded)? & !3;
    let pixel_bytes = pitch
        .checked_mul(height as usize)
        .ok_or(Error::LimitExceeded)?;
    let offset = header.len();
    let file_size = offset
        .checked_add(pixel_bytes)
        .ok_or(Error::LimitExceeded)?;
    let file_size_u32 = u32::try_from(file_size).map_err(|_| Error::LimitExceeded)?;
    if file_size > options.max_output_bytes {
        return Err(Error::LimitExceeded);
    }

    let mut output = Vec::new();
    reserve(&mut output, file_size)?;
    output.extend_from_slice(header);
    let fields = &mut output[..RGB_HEADER.len()];
    put_u32(fields, 2, file_size_u32);
    put_u32(fields, 18, width as u32);
    let stored_height = if options.top_down { -height } else { height };
    put_u32(fields, 22, stored_height as u32);
    put_u32(fields, 34, pixel_bytes as u32);

    let width = width as usize;
    match &bitmap.pixels {
        Pixels::Indexed {
            indices, palette, ..
        } => {
            if let [[red, green, blue]] = *palette.as_slice() {
                output.extend_from_slice(&[blue, green, red]);
                if pitch != row_bytes {
                    repeat_bytes(&mut output, offset, offset + row_bytes);
                    output.extend_from_slice(&[0; 3][..pitch - row_bytes]);
                }
                repeat_bytes(&mut output, offset, file_size);
            } else if indices.len() >= 1024 {
                let padding = &[0; 3][..pitch - row_bytes];
                let chunk_pixels = if options.top_down && padding.is_empty() {
                    indices.len()
                } else {
                    width
                };
                append_indexed_rows(
                    &mut output,
                    indices,
                    palette,
                    chunk_pixels,
                    options.top_down,
                    padding,
                );
            } else {
                output.resize(file_size, 0);
                let mut sources = indices.chunks_exact(width);
                let rows = std::iter::from_fn(|| {
                    if options.top_down {
                        sources.next()
                    } else {
                        sources.next_back()
                    }
                });
                for (source, row) in rows.zip(output[offset..].chunks_exact_mut(pitch)) {
                    for (&index, bgr) in source.iter().zip(row[..row_bytes].as_chunks_mut::<3>().0)
                    {
                        let [red, green, blue] = palette[usize::from(index)];
                        *bgr = [blue, green, red];
                    }
                }
            }
        }
        Pixels::Rgb(samples) => append_truecolor_rows(
            &mut output,
            samples,
            row_bytes,
            &[0; 3][..pitch - row_bytes],
            options.top_down,
            |&[red, green, blue]| [blue, green, red],
        ),
        Pixels::Rgba(samples) => append_truecolor_rows(
            &mut output,
            samples,
            row_bytes,
            &[],
            options.top_down,
            |&[red, green, blue, alpha]| [blue, green, red, alpha],
        ),
    }
    Ok(output)
}

fn repeat_bytes(output: &mut Vec<u8>, start: usize, end: usize) {
    while output.len() < end {
        let count = (end - output.len()).min(output.len() - start);
        output.extend_from_within(start..start + count);
    }
}

fn append_truecolor_rows<const N: usize>(
    output: &mut Vec<u8>,
    samples: &[u8],
    row_bytes: usize,
    padding: &[u8],
    top_down: bool,
    mut reorder: impl FnMut(&[u8; N]) -> [u8; N],
) {
    if top_down && padding.is_empty() {
        output.extend(samples.as_chunks::<N>().0.iter().flat_map(reorder));
    } else {
        let mut rows = samples.chunks_exact(row_bytes);
        while let Some(row) = if top_down {
            rows.next()
        } else {
            rows.next_back()
        } {
            output.extend(row.as_chunks::<N>().0.iter().flat_map(&mut reorder));
            output.extend_from_slice(padding);
        }
    }
}

// Keep the palette table off the stack of small encodes.
#[inline(never)]
fn append_indexed_rows(
    output: &mut Vec<u8>,
    indices: &[u8],
    palette: &[[u8; 3]],
    chunk_pixels: usize,
    top_down: bool,
    padding: &[u8],
) {
    let mut bgr = [[0; 3]; 256];
    for (&[red, green, blue], color) in palette.iter().zip(&mut bgr) {
        *color = [blue, green, red];
    }
    let mut rows = indices.chunks_exact(chunk_pixels);
    while let Some(row) = if top_down {
        rows.next()
    } else {
        rows.next_back()
    } {
        output.extend(row.iter().flat_map(|&index| bgr[usize::from(index)]));
        output.extend_from_slice(padding);
    }
}

const fn header_template<const SIZE: usize>(bit_depth: u8) -> [u8; SIZE] {
    let mut header = [0; SIZE];
    header[0] = b'B';
    header[1] = b'M';
    put_u32(&mut header, 10, SIZE as u32);
    put_u32(&mut header, 14, (SIZE - 14) as u32);
    header[26] = 1;
    header[28] = bit_depth;
    if bit_depth == 32 {
        put_u32(&mut header, 30, 3); // BI_BITFIELDS
        put_u32(&mut header, 54, 0x00ff_0000);
        put_u32(&mut header, 58, 0x0000_ff00);
        put_u32(&mut header, 62, 0x0000_00ff);
        put_u32(&mut header, 66, 0xff00_0000);
        put_u32(&mut header, 70, 0x7352_4742); // LCS_sRGB
    }
    header
}

const fn put_u32(output: &mut [u8], offset: usize, value: u32) {
    let (_, field) = output.split_at_mut(offset);
    field
        .split_at_mut(4)
        .0
        .copy_from_slice(&value.to_le_bytes());
}
