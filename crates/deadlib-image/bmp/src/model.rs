use crate::{Error, reserve};

/// Resource bounds checked before allocating decoded or converted pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    /// Maximum supplied input length, including any trailing bytes.
    pub max_input_bytes: usize,
    /// Maximum number of pixels in the image.
    pub max_pixels: usize,
    /// Maximum decoded pixel bytes, including three bytes per palette entry.
    /// For RGBA conversion this bounds the new RGBA buffer instead; the caller
    /// already owns the source bitmap. It is not a process-wide memory budget.
    pub max_decoded_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_input_bytes: 256 << 20,
            max_pixels: 64 << 20,
            max_decoded_bytes: 256 << 20,
        }
    }
}

/// Unpacked pixels in top-to-bottom, left-to-right order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pixels {
    /// One index per pixel, with an opaque RGB palette. `bit_depth` retains the
    /// original packed depth (1, 4, or 8), although `indices` are always bytes.
    Indexed {
        bit_depth: u8,
        indices: Vec<u8>,
        palette: Vec<[u8; 3]>,
    },
    /// Three bytes per pixel in red, green, blue order.
    Rgb(Vec<u8>),
    /// Four bytes per pixel in red, green, blue, alpha order; unassociated alpha.
    Rgba(Vec<u8>),
}

/// An owned BMP image, independent of any external image representation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub pixels: Pixels,
}

pub(crate) fn pixel_count(width: u32, height: u32) -> Result<usize, Error> {
    if width == 0 || height == 0 {
        return Err(Error::Invalid("zero image dimension"));
    }
    usize::try_from(u64::from(width) * u64::from(height)).map_err(|_| Error::LimitExceeded)
}

impl Bitmap {
    /// Validate dimensions, sample counts, palette size, and every used index.
    pub fn validate(&self) -> Result<(), Error> {
        let count = pixel_count(self.width, self.height)?;
        match &self.pixels {
            Pixels::Indexed {
                bit_depth,
                indices,
                palette,
            } => {
                if !matches!(bit_depth, 1 | 4 | 8) {
                    return Err(Error::Invalid("indexed bit depth must be 1, 4, or 8"));
                }
                if palette.is_empty() || palette.len() > 1usize << bit_depth {
                    return Err(Error::Invalid("invalid palette size"));
                }
                if indices.len() != count {
                    return Err(Error::Invalid("incorrect index count"));
                }
                if indices
                    .iter()
                    .any(|&index| usize::from(index) >= palette.len())
                {
                    return Err(Error::Invalid("pixel index exceeds the palette"));
                }
            }
            Pixels::Rgb(samples) => check_samples(samples, count, 3)?,
            Pixels::Rgba(samples) => check_samples(samples, count, 4)?,
        }
        Ok(())
    }

    /// Expand into RGBA8 using the default pixel and output limits.
    pub fn to_rgba8(&self) -> Result<Vec<u8>, Error> {
        self.to_rgba8_with_limits(Limits::default())
    }

    /// Expand into RGBA8 with explicit pixel and output limits. This method does
    /// not read encoded data, so `max_input_bytes` does not apply.
    pub fn to_rgba8_with_limits(&self, limits: Limits) -> Result<Vec<u8>, Error> {
        let count = pixel_count(self.width, self.height)?;
        let bytes = count.checked_mul(4).ok_or(Error::LimitExceeded)?;
        if count > limits.max_pixels || bytes > limits.max_decoded_bytes {
            return Err(Error::LimitExceeded);
        }
        self.validate()?;
        let mut output = Vec::new();
        reserve(&mut output, bytes)?;
        match &self.pixels {
            Pixels::Indexed {
                indices, palette, ..
            } => {
                output.resize(bytes, 0);
                for (&index, rgba) in indices.iter().zip(output.as_chunks_mut::<4>().0) {
                    let [red, green, blue] = palette[usize::from(index)];
                    *rgba = [red, green, blue, 255];
                }
            }
            Pixels::Rgb(samples) => {
                output.resize(bytes, 0);
                for (rgb, rgba) in samples
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .zip(output.as_chunks_mut::<4>().0)
                {
                    *rgba = [rgb[0], rgb[1], rgb[2], 255];
                }
            }
            Pixels::Rgba(samples) => output.extend_from_slice(samples),
        }
        Ok(output)
    }
}

fn check_samples(samples: &[u8], count: usize, channels: usize) -> Result<(), Error> {
    let expected = count.checked_mul(channels).ok_or(Error::LimitExceeded)?;
    if samples.len() != expected {
        return Err(Error::Invalid("incorrect sample count"));
    }
    Ok(())
}
