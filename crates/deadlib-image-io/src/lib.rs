//! File and memory adapters for the native image codecs.

mod error;
mod gif;
mod png;

pub use error::{ImageError, ImageResult};
pub use gif::GifFrames;

use deadlib_image::{ColorType, Image, RgbaImage, Samples};
use deadlib_image_bmp as bmp;
use deadlib_image_jpeg as jpeg;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Cursor, Read, Seek, Write};
use std::path::Path;

const MAX_RGBA_BYTES: usize = 512 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Png,
    Gif,
    Bmp,
    Jpeg,
}

impl Format {
    fn from_signature(bytes: &[u8]) -> Option<Self> {
        match bytes {
            [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, ..] => Some(Self::Png),
            [b'G', b'I', b'F', b'8', b'7' | b'9', b'a', ..] => Some(Self::Gif),
            [b'B', b'M', ..] => Some(Self::Bmp),
            [0xff, 0xd8, 0xff, ..] => Some(Self::Jpeg),
            _ => None,
        }
    }

    fn from_extension(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?;
        if ext.eq_ignore_ascii_case("png") {
            Some(Self::Png)
        } else if ext.eq_ignore_ascii_case("gif") {
            Some(Self::Gif)
        } else if ext.eq_ignore_ascii_case("bmp") {
            Some(Self::Bmp)
        } else if ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg") {
            Some(Self::Jpeg)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_width: u32,
    pub max_height: u32,
    pub max_bytes: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Use {
    Texture { color_key: bool },
    Artwork,
}

/// Decode a PNG, GIF, BMP, or JPEG file to RGBA8. The signature takes priority
/// over the extension, so mislabeled image assets remain loadable.
pub fn open(path: &Path) -> ImageResult<RgbaImage> {
    open_reporting_format(path, |_, _| {})
}

pub fn open_reporting_format(
    path: &Path,
    mismatch: impl FnOnce(Format, Format),
) -> ImageResult<RgbaImage> {
    let mut input = BufReader::new(File::open(path)?);
    let extension = Format::from_extension(path);
    let format = file_format(&mut input, extension)?;
    if let Some(declared) = extension
        && declared != format
    {
        mismatch(declared, format);
    }
    decode_file(&mut input, format, Use::Artwork)
}

pub fn open_with_limits(path: &Path, limits: Limits) -> ImageResult<RgbaImage> {
    let mut input = BufReader::new(File::open(path)?);
    let format = file_format(&mut input, Format::from_extension(path))?;
    decode_limited(&mut input, format, limits)
}

/// Decode a file for texture preparation, before resizing or changing alpha.
pub fn open_texture(path: &Path, color_key: bool) -> ImageResult<RgbaImage> {
    let mut input = BufReader::new(File::open(path)?);
    let format = file_format(&mut input, Format::from_extension(path))?;
    decode_file(&mut input, format, Use::Texture { color_key })
}

fn decode_file(input: &mut BufReader<File>, format: Format, usage: Use) -> ImageResult<RgbaImage> {
    let input_size = if format == Format::Jpeg
        || (format == Format::Bmp && input.buffer().len() == input.capacity())
    {
        Some(input.get_ref().metadata()?.len())
    } else {
        None
    };
    decode_reader(input, format, usage, input_size)
}

fn file_format(input: &mut impl BufRead, extension: Option<Format>) -> ImageResult<Format> {
    Format::from_signature(input.fill_buf()?)
        .or(extension)
        .ok_or(ImageError::UnsupportedFormat)
}

/// Read image dimensions without decoding pixels. GIF dimensions describe
/// the logical screen, which can be larger than the first frame.
pub fn dimensions(path: &Path) -> ImageResult<(u32, u32)> {
    let mut input = BufReader::new(File::open(path)?);
    let format = file_format(&mut input, Format::from_extension(path))?;
    dimensions_reader(&mut input, format)
}

fn dimensions_reader(input: &mut impl BufRead, format: Format) -> ImageResult<(u32, u32)> {
    match format {
        Format::Png => {
            let info = deadlib_image_png::read_header(input)
                .map_err(|error| decode_error(format, error))?;
            Ok((info.width, info.height))
        }
        Format::Gif => gif::dimensions(input),
        Format::Bmp => match bmp::probe_with_limits(
            input.fill_buf()?,
            bmp::Limits {
                max_pixels: usize::MAX,
                max_decoded_bytes: usize::MAX,
                ..Default::default()
            },
        ) {
            Ok(info) => Ok((info.width, info.height)),
            Err(error) => Err(decode_error(format, error)),
        },
        Format::Jpeg => {
            let header = jpeg::read_frame_header(input, jpeg::Limits::default())
                .map_err(|error| decode_error(format, error))?;
            if header.height == 0 {
                return Err(decode_error(format, "JPEG dimensions are deferred to DNL"));
            }
            Ok((u32::from(header.width), u32::from(header.height)))
        }
    }
}

/// Decode image bytes, inferring their format from the signature.
pub fn load(bytes: &[u8]) -> ImageResult<RgbaImage> {
    let format = Format::from_signature(bytes).ok_or(ImageError::UnsupportedFormat)?;
    match format {
        Format::Bmp => decode_bmp(bytes),
        Format::Jpeg => decode_jpeg(bytes),
        _ => decode_reader(&mut Cursor::new(bytes), format, Use::Artwork, None),
    }
}

pub fn load_with_limits(bytes: &[u8], limits: Limits) -> ImageResult<RgbaImage> {
    let format = Format::from_signature(bytes).ok_or(ImageError::UnsupportedFormat)?;
    decode_limited(&mut Cursor::new(bytes), format, limits)
}

fn decode_limited(
    input: &mut (impl BufRead + Seek),
    format: Format,
    limits: Limits,
) -> ImageResult<RgbaImage> {
    let (width, height) = dimensions_reader(input, format)?;
    let bytes = output_len(width, height)?;
    if width > limits.max_width || height > limits.max_height || bytes > limits.max_bytes {
        return Err(ImageError::LimitExceeded);
    }
    input.rewind()?;
    let budget = limits.max_bytes.min(MAX_RGBA_BYTES);
    match format {
        Format::Png => png::decode(input, Use::Artwork, budget),
        Format::Jpeg => {
            let bytes = read_bounded(input, jpeg::Limits::default().max_memory_bytes, None)?;
            let defaults = jpeg::Limits::default();
            decode_jpeg_with_limits(
                &bytes,
                jpeg::Limits {
                    max_pixels: defaults.max_pixels.min(budget / 4),
                    max_memory_bytes: budget,
                    ..defaults
                },
            )
        }
        Format::Gif | Format::Bmp => decode_reader(input, format, Use::Artwork, None),
    }
}

/// Decode a known PNG payload directly.
pub fn load_png(bytes: &[u8]) -> ImageResult<RgbaImage> {
    png::decode(bytes, Use::Artwork, MAX_RGBA_BYTES)
}

/// Write an RGBA8 PNG directly from the existing raster.
pub fn save_png(path: &Path, image: &RgbaImage) -> ImageResult<()> {
    let length = output_len(image.width(), image.height())?;
    let mut writer = BufWriter::new(File::create(path)?);
    deadlib_image_png::encode_rgba8_writer(
        image.width(),
        image.height(),
        &image.as_raw()[..length],
        &deadlib_image_png::EncodeOptions::default(),
        &mut writer,
    )
    .map_err(|error| ImageError::Encode {
        format: Format::Png,
        source: Box::new(error),
    })?;
    writer.flush()?;
    Ok(())
}

fn decode_reader(
    input: &mut impl BufRead,
    format: Format,
    usage: Use,
    input_size: Option<u64>,
) -> ImageResult<RgbaImage> {
    let color_key = matches!(usage, Use::Texture { color_key: true });
    let mut image = match format {
        Format::Png => return png::decode(input, usage, MAX_RGBA_BYTES),
        Format::Gif => gif::decode_first(input)?,
        Format::Bmp => {
            // The common small bitmap fits in the file reader's buffer.
            // Decode larger inputs from one owned buffer only if needed.
            match bmp::decode_rgba8(input.fill_buf()?) {
                Ok((info, samples)) => rgba(info.width, info.height, samples)?,
                Err(bmp::Error::Truncated) => {
                    let bytes =
                        read_bounded(input, bmp::Limits::default().max_input_bytes, input_size)?;
                    decode_bmp(&bytes)?
                }
                Err(error) => return Err(decode_error(format, error)),
            }
        }
        Format::Jpeg => {
            let bytes = read_bounded(input, jpeg::Limits::default().max_memory_bytes, input_size)?;
            decode_jpeg(&bytes)?
        }
    };
    if color_key {
        key_rgba(&mut image);
    }
    Ok(image)
}

fn decode_bmp(bytes: &[u8]) -> ImageResult<RgbaImage> {
    let (info, samples) =
        bmp::decode_rgba8(bytes).map_err(|error| decode_error(Format::Bmp, error))?;
    rgba(info.width, info.height, samples)
}

fn decode_jpeg(bytes: &[u8]) -> ImageResult<RgbaImage> {
    decode_jpeg_with_limits(bytes, jpeg::Limits::default())
}

fn decode_jpeg_with_limits(bytes: &[u8], limits: jpeg::Limits) -> ImageResult<RgbaImage> {
    let decoded = jpeg::decode_with_options(
        bytes,
        jpeg::DecodeOptions {
            strict: false,
            limits,
        },
        jpeg::Upsampling::Linear,
    )
    .map_err(|error| decode_error(Format::Jpeg, error))?;
    let Image {
        width,
        height,
        color_type,
        bit_depth,
        samples,
    } = decoded;
    let mut output = output_buffer(width, height)?;
    match samples {
        Samples::U8(samples) => match color_type {
            ColorType::Grayscale => output.extend(samples.into_iter().flat_map(|v| [v, v, v, 255])),
            ColorType::Rgb => output.extend(
                samples
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .flat_map(|&[r, g, b]| [r, g, b, 255]),
            ),
            _ => return Err(decode_error(Format::Jpeg, "unsupported JPEG channels")),
        },
        Samples::U16(samples) => {
            let max = (1u32 << bit_depth) - 1;
            let scale = |v| ((u32::from(v) * 255 + max / 2) / max) as u8;
            match color_type {
                ColorType::Grayscale => output.extend(samples.into_iter().flat_map(|v| {
                    let v = scale(v);
                    [v, v, v, 255]
                })),
                ColorType::Rgb => output.extend(
                    samples
                        .as_chunks::<3>()
                        .0
                        .iter()
                        .flat_map(|&[r, g, b]| [scale(r), scale(g), scale(b), 255]),
                ),
                _ => return Err(decode_error(Format::Jpeg, "unsupported JPEG channels")),
            }
        }
    }
    rgba(width, height, output)
}

fn read_bounded(reader: impl Read, limit: usize, input_size: Option<u64>) -> ImageResult<Vec<u8>> {
    let mut bytes = Vec::new();
    if let Some(input_size) = input_size {
        if input_size > limit as u64 {
            return Err(ImageError::LimitExceeded);
        }
        bytes
            .try_reserve_exact(input_size as usize)
            .map_err(|_| ImageError::LimitExceeded)?;
    }
    reader.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(ImageError::LimitExceeded);
    }
    Ok(bytes)
}

fn output_len(width: u32, height: u32) -> ImageResult<usize> {
    u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .filter(|&bytes| width != 0 && height != 0 && bytes <= MAX_RGBA_BYTES as u64)
        .map(|bytes| bytes as usize)
        .ok_or(ImageError::LimitExceeded)
}

fn output_buffer(width: u32, height: u32) -> ImageResult<Vec<u8>> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(output_len(width, height)?)
        .map_err(|_| ImageError::LimitExceeded)?;
    Ok(output)
}

fn rgba(width: u32, height: u32, bytes: Vec<u8>) -> ImageResult<RgbaImage> {
    RgbaImage::from_raw(width, height, bytes).ok_or(ImageError::LimitExceeded)
}

fn decode_error(
    format: Format,
    error: impl Into<Box<dyn std::error::Error + Send + Sync>>,
) -> ImageError {
    ImageError::Decode {
        format,
        source: error.into(),
    }
}

fn key_rgba(image: &mut RgbaImage) {
    let off_pink = [248, 0, 248, 255];
    let (width, height) = image.dimensions();
    let edge_has_key = height != 0
        && (0..width).any(|x| {
            image.get_pixel(x, 0).0 == off_pink || image.get_pixel(x, height - 1).0 == off_pink
        });
    let key = u32::from_ne_bytes(if edge_has_key {
        off_pink
    } else {
        [255, 0, 255, 255]
    });
    let bytes = width as usize * height as usize * 4;
    for pixel in image.as_mut()[..bytes].as_chunks_mut::<4>().0 {
        let value = u32::from_ne_bytes(*pixel);
        let keep = u32::from(value != key).wrapping_neg();
        *pixel = (value & keep).to_ne_bytes();
    }
}

#[cfg(test)]
mod tests;
