use crate::{TextureHints, apply_texture_hints, fix_hidden_alpha, open_image_fallback};
use deadlib_render_core::SamplerDesc;
use image::RgbaImage;
use std::{
    io::{BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{Condvar, Mutex},
};

/// Number of decode jobs claimed under one queue lock.
///
/// Eight amortizes synchronization while leaving enough batches to balance
/// differently sized texture files across the available workers.
const DECODE_JOB_BATCH_SIZE: usize = 8;

/// A fully resolved image load. Asset catalogs, path selection, and sampler policy
/// belong to the caller; workers apply only the supplied image options.
pub struct TextureDecodeJob {
    pub key: String,
    pub path: PathBuf,
    pub sampler: SamplerDesc,
    pub hints: TextureHints,
}

pub(crate) struct TextureDecodeResult {
    pub key: String,
    pub sampler: SamplerDesc,
    pub decoded: image::ImageResult<DecodedTexture>,
}

/// A prepared upload and its original bitmap size, before resize or source hints.
pub struct DecodedTexture {
    pub image: RgbaImage,
    pub source_size: [u32; 2],
}

struct DecodeSlot {
    state: Mutex<DecodeSlotState>,
    ready: Condvar,
    empty: Condvar,
}

struct DecodeSlotState {
    result: Option<TextureDecodeResult>,
    active_workers: usize,
    cancelled: bool,
}

impl DecodeSlot {
    fn new(active_workers: usize) -> Self {
        Self {
            state: Mutex::new(DecodeSlotState {
                result: None,
                active_workers,
                cancelled: false,
            }),
            ready: Condvar::new(),
            empty: Condvar::new(),
        }
    }

    fn send(&self, result: TextureDecodeResult) -> bool {
        let mut state = self.state.lock().expect("texture decode slot poisoned");
        while state.result.is_some() && !state.cancelled {
            state = self
                .empty
                .wait(state)
                .expect("texture decode slot poisoned");
        }
        if state.cancelled {
            return false;
        }
        state.result = Some(result);
        self.ready.notify_one();
        true
    }

    fn receive(&self) -> Option<TextureDecodeResult> {
        let mut state = self.state.lock().expect("texture decode slot poisoned");
        loop {
            if let Some(result) = state.result.take() {
                self.empty.notify_one();
                return Some(result);
            }
            if state.active_workers == 0 {
                return None;
            }
            state = self
                .ready
                .wait(state)
                .expect("texture decode slot poisoned");
        }
    }

    fn finish_worker(&self) {
        let mut state = self.state.lock().expect("texture decode slot poisoned");
        state.active_workers -= 1;
        self.ready.notify_one();
    }

    fn cancel(&self) {
        let mut state = self.state.lock().expect("texture decode slot poisoned");
        state.cancelled = true;
        self.empty.notify_all();
    }
}

struct DecodeWorker<'a>(&'a DecodeSlot);

impl Drop for DecodeWorker<'_> {
    fn drop(&mut self) {
        self.0.finish_worker();
    }
}

#[derive(Clone, Copy)]
pub struct TextureAssetSpec {
    pub key: &'static str,
    pub path: &'static str,
}

#[must_use]
pub const fn texture_asset(path: &'static str) -> TextureAssetSpec {
    TextureAssetSpec { key: path, path }
}

fn decode_rgba(job: TextureDecodeJob) -> TextureDecodeResult {
    TextureDecodeResult {
        decoded: decode_texture(&job.path, &job.hints),
        key: job.key,
        sampler: job.sampler,
    }
}

pub fn decode_texture_image(path: &Path, hints: &TextureHints) -> image::ImageResult<RgbaImage> {
    decode_texture(path, hints).map(|decoded| decoded.image)
}

pub fn decode_texture(path: &Path, hints: &TextureHints) -> image::ImageResult<DecodedTexture> {
    let mut image = decode_image(path, hints.hot_pink_color_key)?;
    let source_size = [image.width(), image.height()];
    let size = texture_image_size(source_size, hints)?;
    if size != [image.width(), image.height()] {
        image = zoom_image(image, size);
    }
    if !hints.is_default() {
        apply_texture_hints(&mut image, hints);
    }
    // Native cleanup follows color keying and the final resize, including
    // sources that gain an alpha channel during preparation.
    fix_hidden_alpha(&mut image);
    Ok(DecodedTexture { image, source_size })
}

pub fn texture_image_size(source: [u32; 2], hints: &TextureHints) -> image::ImageResult<[u32; 2]> {
    let max_size = hints.max_size.unwrap_or(u32::MAX);
    let mut size = source.map(|dimension| dimension.min(max_size));
    if size.contains(&0) {
        return Err(image_limit());
    }
    let min_size = hints.min_size.unwrap_or(0);
    if hints.min_size.is_some() && (!min_size.is_power_of_two() || min_size > max_size) {
        return Err(image_limit());
    }
    let allocation = if hints.stretch || min_size != 0 {
        [size[0].checked_next_power_of_two().ok_or_else(image_limit)?,
         size[1].checked_next_power_of_two().ok_or_else(image_limit)?]
    } else { size };
    if hints.stretch || allocation.iter().any(|dimension| *dimension < min_size) {
        for dimension in &mut size {
            *dimension = dimension
                .checked_next_power_of_two()
                .ok_or_else(image_limit)?
                .max(min_size.max(8));
        }
    }
    if size.iter().any(|dimension| *dimension > max_size) {
        return Err(image_limit());
    }
    check_image_alloc(size)?;
    Ok(size)
}

fn check_image_alloc(size: [u32; 2]) -> image::ImageResult<()> {
    let limit = image::Limits::default().max_alloc.unwrap_or(u64::MAX);
    if u64::from(size[0]) * u64::from(size[1]) > limit / 4 {
        return Err(image::ImageError::Limits(
            image::error::LimitError::from_kind(image::error::LimitErrorKind::InsufficientMemory),
        ));
    }
    Ok(())
}

fn image_limit() -> image::ImageError {
    image::ImageError::Limits(image::error::LimitError::from_kind(
        image::error::LimitErrorKind::DimensionError,
    ))
}

fn png_error(error: png::DecodingError) -> image::ImageError {
    image::ImageError::Decoding(image::error::DecodingError::new(
        image::ImageFormat::Png.into(),
        error,
    ))
}

fn image_source(path: &Path) -> image::ImageResult<([u8; 8], std::fs::File)> {
    let mut signature = [0; 8];
    let mut input = std::fs::File::open(path)?;
    input.read_exact(&mut signature)?;
    Ok((signature, input))
}

fn image_reader(
    mut input: std::fs::File,
    path: &Path,
) -> image::ImageResult<BufReader<std::fs::File>> {
    // Reproduce the original reopen if this source cannot rewind.
    if input.rewind().is_err() {
        drop(input);
        input = std::fs::File::open(path)?;
    }
    Ok(BufReader::new(input))
}

/// Raw native texture dimensions before resolution hints or image preparation.
/// GIF textures use the logical canvas, including the first frame's offsets.
pub fn texture_source_size(path: &Path) -> image::ImageResult<(u32, u32)> {
    let (signature, input) = image_source(path)?;
    if &signature[..6] == b"GIF87a" || &signature[..6] == b"GIF89a" {
        let reader = gif_reader_from_reader(image_reader(input, path)?)?;
        if reader.width() == 0 || reader.height() == 0 {
            return Err(image_limit());
        }
        return Ok((u32::from(reader.width()), u32::from(reader.height())));
    }
    drop(input);
    image::ImageReader::open(path)?
        .with_guessed_format()?
        .into_dimensions()
}

fn decode_image(path: &Path, color_key: bool) -> image::ImageResult<RgbaImage> {
    let (signature, input) = image_source(path)?;
    if signature == *b"\x89PNG\r\n\x1a\n" {
        return decode_png_from_reader(image_reader(input, path)?, color_key);
    }
    if &signature[..6] == b"GIF87a" || &signature[..6] == b"GIF89a" {
        return decode_gif_from_reader(image_reader(input, path)?, color_key);
    }
    if &signature[..2] == b"BM" {
        if let Some(image) = decode_indexed_bmp_from_reader(image_reader(input, path)?, color_key)?
        {
            return Ok(image);
        }
    } else {
        drop(input);
    }
    let mut image = open_image_fallback(path)?.into_rgba8();
    if color_key {
        key_rgba(&mut image);
    }
    Ok(image)
}

fn decode_error(format: image::ImageFormat, message: &str) -> image::ImageError {
    image::ImageError::Decoding(image::error::DecodingError::new(format.into(), message))
}

fn gif_error(error: gif::DecodingError) -> image::ImageError {
    image::ImageError::Decoding(image::error::DecodingError::new(
        image::ImageFormat::Gif.into(),
        error,
    ))
}

#[cfg(test)]
fn gif_reader(path: &Path) -> image::ImageResult<gif::Decoder<BufReader<std::fs::File>>> {
    gif_reader_from_reader(BufReader::new(std::fs::File::open(path)?))
}

fn gif_reader_from_reader(
    input: BufReader<std::fs::File>,
) -> image::ImageResult<gif::Decoder<BufReader<std::fs::File>>> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::Indexed);
    let limit = std::num::NonZeroU64::new(image::Limits::default().max_alloc.unwrap_or(u64::MAX))
        .ok_or_else(image_limit)?;
    options.set_memory_limit(gif::MemoryLimit::Bytes(limit));
    options.read_info(input).map_err(gif_error)
}

#[cfg(test)]
fn decode_gif(path: &Path, color_key: bool) -> image::ImageResult<RgbaImage> {
    decode_gif_from_reader(BufReader::new(std::fs::File::open(path)?), color_key)
}

fn decode_gif_from_reader(
    input: BufReader<std::fs::File>,
    color_key: bool,
) -> image::ImageResult<RgbaImage> {
    let mut reader = gif_reader_from_reader(input)?;
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

#[cfg(test)]
fn decode_indexed_bmp(path: &Path, color_key: bool) -> image::ImageResult<Option<RgbaImage>> {
    decode_indexed_bmp_from_reader(BufReader::new(std::fs::File::open(path)?), color_key)
}

// Native indexed BMP parsing stays together: DIB12/DIB40 headers, palette,
// padded bottom-up rows and bit unpacking share the file offset contract.
// Other BMP formats retain the existing decoder pending native mask validation.
fn decode_indexed_bmp_from_reader(
    mut input: BufReader<std::fs::File>,
    color_key: bool,
) -> image::ImageResult<Option<RgbaImage>> {
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

fn key_palette(colors: &mut [[u8; 4]; 256]) {
    for pink in [[248, 0, 248, 255], [255, 0, 255, 255]] {
        if let Some(color) = colors.iter_mut().find(|color| **color == pink) {
            color[3] = 0;
        }
    }
}

// A byte holds N palette indices, most-significant first. The final chunk
// can be short: unused low bits in a padded scanline must not produce pixels.
fn unpack_palette<const N: usize>(row: &[u8], pixels: &mut [[u8; 4]], colors: &[[u8; 4]; 256]) {
    let index = |byte: u8, i| (byte >> (8 - (i + 1) * (8 / N))) & (u8::MAX >> (8 - 8 / N));
    let (groups, tail) = pixels.as_chunks_mut::<N>();
    for (pixels, &byte) in groups.iter_mut().zip(row) {
        *pixels = std::array::from_fn(|i| colors[index(byte, i) as usize]);
    }
    if !tail.is_empty() {
        let byte = row[groups.len()];
        for (i, pixel) in tail.iter_mut().enumerate() {
            *pixel = colors[index(byte, i) as usize];
        }
    }
}

// PNG indexed entries must remain distinct: native palette color keying
// changes the first matching entry, even when another entry has identical RGB.
// Native PNG loading strips 16-bit channels rather than rescaling their values.
#[cfg(test)]
fn decode_png(path: &Path, color_key: bool) -> image::ImageResult<RgbaImage> {
    decode_png_from_reader(BufReader::new(std::fs::File::open(path)?), color_key)
}

fn decode_png_from_reader(
    input: BufReader<std::fs::File>,
    color_key: bool,
) -> image::ImageResult<RgbaImage> {
    let mut decoder = png::Decoder::new(input);
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

fn key_rgba(image: &mut RgbaImage) {
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
    let key = u32::from_ne_bytes(key);
    let length = image.width() as usize * image.height() as usize * 4;
    for pixel in image.as_mut()[..length].as_chunks_mut::<4>().0 {
        let value = u32::from_ne_bytes(*pixel);
        let keep = u32::from(value != key).wrapping_neg();
        *pixel = (value & keep).to_ne_bytes();
    }
}

/*
 * Port of RageSurfaceUtils_Zoom, copyright (c) A. Schiffler, Glenn Maynard.
 * All rights reserved.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a
 * copy of this software and associated documentation files (the
 * "Software"), to deal in the Software without restriction, including
 * without limitation the rights to use, copy, modify, merge, publish,
 * distribute, and/or sell copies of the Software, and to permit persons to
 * whom the Software is furnished to do so, provided that the above
 * copyright notice(s) and this permission notice appear in all copies of
 * the Software and that both the above copyright notice(s) and this
 * permission notice appear in supporting documentation.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
 * OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
 * MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
 * THIRD PARTY RIGHTS. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS
 * INCLUDED IN THIS NOTICE BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT
 * OR CONSEQUENTIAL DAMAGES, OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS
 * OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR
 * OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
 * PERFORMANCE OF THIS SOFTWARE.
 */
// RageSurfaceUtils_Zoom uses float coordinates and 24-bit fixed-point weights;
// it floors horizontal interpolation and rounds the final vertical blend.
fn zoom_axis(source: u32, destination: u32) -> impl Iterator<Item = (u32, u32, u32)> {
    let ratio = source as f32 / destination as f32;
    (0..destination).map(move |x| {
        if source >= destination {
            let center = ratio * x as f32 + ratio / 2.0;
            let left = (center - ratio / 4.0) as u32;
            let right = (center + ratio / 4.0) as u32;
            let weight = if left == right {
                1 << 24
            } else {
                ((1.0 - (center - (left as f32 + 0.5)) / (right - left) as f32) * 16777216.0) as u32
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
}

// At exactly 2:1 both fixed-point weights are 1/2. Average all four
// byte lanes together: horizontal blends floor, then the vertical blend rounds.
#[inline(never)]
fn zoom_half(image: &RgbaImage, size: [u32; 2]) -> RgbaImage {
    let mut output = RgbaImage::new(size[0], size[1]);
    let source = image.as_raw().as_chunks::<4>().0;
    for (rows, row) in source.chunks_exact(image.width() as usize * 2).zip(
        output
            .as_mut()
            .as_chunks_mut::<4>()
            .0
            .chunks_exact_mut(size[0] as usize),
    ) {
        let (top, bottom) = rows.split_at(image.width() as usize);
        for ((pixel, top), bottom) in row
            .iter_mut()
            .zip(top.as_chunks::<2>().0)
            .zip(bottom.as_chunks::<2>().0)
        {
            let [a, b] = top.map(u32::from_le_bytes);
            let [c, d] = bottom.map(u32::from_le_bytes);
            let upper = (a & b) + (((a ^ b) & 0xfefe_fefe) >> 1);
            let lower = (c & d) + (((c ^ d) & 0xfefe_fefe) >> 1);
            *pixel = ((upper | lower) - (((upper ^ lower) & 0xfefe_fefe) >> 1)).to_le_bytes();
        }
    }
    output
}

fn zoom_image(mut image: RgbaImage, size: [u32; 2]) -> RgbaImage {
    while [image.width(), image.height()] != size {
        let next = std::array::from_fn::<_, 2, _>(|axis| {
            let current = [image.width(), image.height()][axis];
            let ratio = (size[axis] as f32 / current as f32).clamp(0.5, 2.0);
            (current as f32 * ratio).round_ties_even() as u32
        });
        // Beyond 2^23, f32 coordinates no longer preserve half-pixel steps.
        if image.width().max(image.height()) <= 1 << 23
            && [next[0] * 2, next[1] * 2] == [image.width(), image.height()]
        {
            image = zoom_half(&image, next);
            continue;
        }
        let xs: Vec<_> = zoom_axis(image.width(), next[0]).collect();
        let ys = zoom_axis(image.height(), next[1]);
        let mut output = RgbaImage::new(next[0], next[1]);
        let width = image.width() as usize;
        let source = image.as_raw().as_chunks::<4>().0;
        for (row, (top, bottom, wy)) in output
            .as_mut()
            .as_chunks_mut::<4>()
            .0
            .chunks_exact_mut(next[0] as usize)
            .zip(ys)
        {
            let upper_row = &source[top as usize * width..(top as usize + 1) * width];
            let lower_row = &source[bottom as usize * width..(bottom as usize + 1) * width];
            for (pixel, &(left, right, wx)) in row.iter_mut().zip(&xs) {
                // Two 32-bit lanes hold independent 8-bit channels and their
                // 24-bit products. Complementary weights cannot carry between lanes.
                let spread = |p: [u8; 4]| [
                    u64::from(p[0]) | (u64::from(p[2]) << 32),
                    u64::from(p[1]) | (u64::from(p[3]) << 32),
                ];
                let c00 = spread(upper_row[left as usize]);
                let c01 = spread(upper_row[right as usize]);
                let c10 = spread(lower_row[left as usize]);
                let c11 = spread(lower_row[right as usize]);
                for c in 0..2 {
                    let upper = ((c00[c] * u64::from(wx) + c01[c] * u64::from(16777216 - wx)) >> 24)
                        & 0x0000_00ff_0000_00ff;
                    let lower = ((c10[c] * u64::from(wx) + c11[c] * u64::from(16777216 - wx)) >> 24)
                        & 0x0000_00ff_0000_00ff;
                    let pair = (upper * u64::from(wy) + lower * u64::from(16777216 - wy)
                        + 0x0080_0000_0080_0000) >> 24;
                    pixel[c] = pair as u8;
                    pixel[c + 2] = (pair >> 32) as u8;
                }
            }
        }
        image = output;
    }
    image
}

/// Decodes on workers while the caller consumes completed images immediately.
///
/// The bounded handoff limits retained decoded pixels to roughly one image per
/// worker instead of the full startup corpus.
///
/// # Panics
///
/// Panics if an internal worker fails.
pub(crate) fn decode_texture_jobs_with<E>(
    jobs: Vec<TextureDecodeJob>,
    mut consume: impl FnMut(TextureDecodeResult) -> Result<(), E>,
) -> Result<(), E> {
    let job_count = jobs.len();
    if job_count == 0 {
        return Ok(());
    }

    let worker_count = std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1)
        .min(job_count);
    if worker_count == 1 {
        for job in jobs {
            consume(decode_rgba(job))?;
        }
        return Ok(());
    }

    // A worker claims a whole batch, so additional workers cannot do any work.
    // Keep one worker even for a single batch to overlap decoding with uploads.
    let worker_count = worker_count.min(job_count.div_ceil(DECODE_JOB_BATCH_SIZE));

    let jobs = Mutex::new(jobs.into_iter());
    let slot = DecodeSlot::new(worker_count);
    std::thread::scope(|scope| {
        let mut workers = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let jobs = &jobs;
            let slot = &slot;
            workers.push(scope.spawn(move || {
                let _worker = DecodeWorker(slot);
                let mut batch = Vec::with_capacity(DECODE_JOB_BATCH_SIZE);
                loop {
                    {
                        let mut jobs = jobs.lock().expect("texture decode job queue poisoned");
                        batch.extend(jobs.by_ref().take(DECODE_JOB_BATCH_SIZE));
                    }
                    if batch.is_empty() {
                        return;
                    }
                    for job in batch.drain(..) {
                        if !slot.send(decode_rgba(job)) {
                            return;
                        }
                    }
                }
            }));
        }

        let mut result = Ok(());
        while let Some(decoded) = slot.receive() {
            if let Err(error) = consume(decoded) {
                slot.cancel();
                result = Err(error);
                break;
            }
        }
        for worker in workers {
            worker.join().expect("texture decode worker panicked");
        }
        result
    })
}

#[cfg(test)]
#[path = "decode_perf.rs"]
mod perf_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_render_core::{SamplerFilter, SamplerWrap};

    #[test]
    fn primary_bitmap_observations_match_native() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let reference: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join("tests/fixtures/itgmania-primary-bitmaps/capture.json"))
                .expect("committed primary codec observations"),
        ).expect("primary bitmap JSON");
        let mut failures = Vec::new();
        let mut sources = 0;
        let mut uploads = 0;
        let rgba = |value: &serde_json::Value| -> Vec<u8> {
            value.as_array().expect("native pixels").iter().flat_map(|pixel|
                pixel.as_array().expect("RGBA").iter().map(|value|
                    u8::try_from(value.as_u64().unwrap()).unwrap())).collect()
        };
        for case in reference["cases"].as_array().unwrap() {
            let name = case["request"]["name"].as_str().unwrap();
            let path = root.join(case["source"].as_str().unwrap());
            let native = &case["native"];
            let expected_source = [native["loaded"]["width"].as_u64().unwrap() as u32,
                native["loaded"]["height"].as_u64().unwrap() as u32];
            match texture_source_size(&path) {
                Ok((width, height)) if [width, height] == expected_source => {},
                result => failures.push(format!("{name}: source dimensions {result:?}, expected {expected_source:?}")),
            }
            match decode_image(&path, false) {
                Ok(image) => {
                    if image.as_raw() != &rgba(&native["loaded"]["pixels"]) {
                        failures.push(format!("{name}: decoded source pixels differ"));
                    }
                },
                Err(error) => failures.push(format!("{name}: source decode failed: {error}")),
            }
            sources += 1;
            let model = case["request"]["kind"] == "model";
            let hints = TextureHints {
                non_default: true, hot_pink_color_key: model, stretch: model,
                min_size: (!model).then_some(8), max_size: Some(2048),
                color_depth: Some(32), ..Default::default()
            };
            match decode_texture(&path, &hints) {
                Ok(decoded) => {
                    if serde_json::json!([decoded.image.width(), decoded.image.height()])
                        != native["dimensions"]["image"] {
                        failures.push(format!("{name}: prepared image dimensions differ"));
                    }
                    if native["upload"]["pixels_captured"] == true {
                        if decoded.image.as_raw() != &rgba(&native["upload"]["pixels"]) {
                            failures.push(format!("{name}: prepared upload pixels differ"));
                        }
                        uploads += 1;
                    } else {
                        assert_ne!(native["dimensions"]["image"], native["dimensions"]["texture"]);
                    }
                },
                Err(error) => failures.push(format!("{name}: preparation failed: {error}")),
            }
        }
        assert_eq!(sources, 35, "retain every defined primary decode");
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        assert_eq!(uploads, 33, "retain every complete native upload");
    }

    #[test]
    fn primary_bitmap_undefined_native_pixels_have_defined_rust_behavior() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let reference: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join("tests/fixtures/itgmania-primary-bitmaps/capture.json"))
                .expect("committed primary bitmap capture")).unwrap();
        let cases = reference["header_only"].as_array().unwrap();
        assert_eq!(cases.len(), 2);
        for case in cases {
            let path = root.join(case["source"].as_str().unwrap());
            let (width, height) = texture_source_size(&path).expect("defined native header dimensions");
            assert_eq!(serde_json::json!([width, height]), case["native"]["dimensions"]);
        }
        let minimal = root.join("tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files/os2-1.bmp");
        let decoded = decode_image(&minimal, false).expect("valid minimal OS/2 bitmap");
        assert!(decoded.pixels().all(|pixel| pixel.0 == [255, 0, 255, 255]));
        let invalid = root.join("tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files/win8-unused-entry.bmp");
        assert!(decode_image(&invalid, false).is_err(), "reject an index outside the stored palette");
    }

    fn missing_job(index: usize) -> TextureDecodeJob {
        TextureDecodeJob {
            key: format!("missing-{index}"),
            path: PathBuf::from(format!("__missing_texture_{index}__.png")),
            sampler: SamplerDesc {
                filter: SamplerFilter::Nearest,
                wrap: SamplerWrap::Repeat,
                mipmaps: true,
            },
            hints: TextureHints::default(),
        }
    }

    #[test]
    fn indexed_bmp_rejects_truncation_and_excessive_allocation() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files/win8.bmp");
        let original = std::fs::read(source).expect("indexed BMP input");
        let path =
            std::env::temp_dir().join(format!("invalid-indexed-bmp-{}.bmp", std::process::id()));
        let mut truncated = original.clone();
        truncated.pop();
        let mut oversized = original;
        oversized[18..22].copy_from_slice(&u32::MAX.to_le_bytes());
        for bytes in [truncated, oversized] {
            std::fs::write(&path, bytes).expect("invalid BMP source");
            assert!(decode_texture_image(&path, &TextureHints::default()).is_err());
        }
        std::fs::remove_file(path).expect("remove invalid BMP inputs");
    }

    #[test]
    fn empty_jobs_do_not_call_consumer() {
        decode_texture_jobs_with(Vec::new(), |_| -> Result<(), ()> {
            panic!("empty jobs must not call the consumer")
        })
        .expect("empty decode");
    }

    #[test]
    fn decode_failure_preserves_key_and_sampler() {
        let job = missing_job(0);
        let sampler = job.sampler;
        decode_texture_jobs_with(vec![job], |result| {
            assert_eq!(result.key, "missing-0");
            assert_eq!(result.sampler, sampler);
            assert!(result.decoded.is_err());
            Ok::<_, ()>(())
        })
        .expect("consumer accepts failures");
    }

    #[test]
    fn consumer_error_cancels_and_joins_workers() {
        let jobs = (0..32).map(missing_job).collect();
        let mut consumed = 0;
        let result = decode_texture_jobs_with(jobs, |_| {
            consumed += 1;
            Err("upload failed")
        });
        assert_eq!(result, Err("upload failed"));
        assert_eq!(consumed, 1);
    }

    #[test]
    fn workers_apply_resolved_options_without_interpreting_names() {
        let path = std::env::temp_dir().join(format!("resolved-decode-{}.png", std::process::id()));
        let source = RgbaImage::from_pixel(2, 2, image::Rgba([17, 83, 149, 255]));
        source.save(&path).expect("write decode fixture");
        let sampler = missing_job(0).sampler;
        let jobs = [false, true]
            .into_iter()
            .map(|grayscale| TextureDecodeJob {
                key: if grayscale {
                    "plain.png"
                } else {
                    "named (grayscale nearest).png"
                }
                .into(),
                path: path.clone(),
                sampler,
                hints: TextureHints {
                    non_default: grayscale,
                    grayscale,
                    ..Default::default()
                },
            })
            .collect();
        let mut consumed = 0;
        decode_texture_jobs_with(jobs, |result| {
            assert_eq!(result.sampler, sampler);
            let image = result.decoded.expect("decode fixture").image;
            if result.key == "plain.png" {
                let pixel = image.get_pixel(0, 0).0;
                assert_eq!(pixel[0], pixel[1]);
                assert_eq!(pixel[1], pixel[2]);
                assert_eq!(pixel[3], 255);
            } else {
                assert_eq!(image, source);
            }
            consumed += 1;
            Ok::<_, ()>(())
        })
        .expect("consume decoded images");
        assert_eq!(consumed, 2);
        std::fs::remove_file(path).expect("remove decode fixture");
    }
}

#[cfg(test)]
#[path = "texture_pixels_perf.rs"]
mod texture_pixels_perf;

#[cfg(test)]
#[path = "indexed_pixels_perf.rs"]
mod indexed_pixels_perf;

#[cfg(test)]
mod preparation_perf;

#[cfg(test)]
mod direct_media_perf;

#[cfg(test)]
mod resize_kernel;

#[cfg(test)]
mod key_kernel;

#[cfg(test)]
mod palette_perf;

#[cfg(test)]
mod open_perf;

#[cfg(test)]
mod open_seek;

#[cfg(test)]
mod metadata_perf;
