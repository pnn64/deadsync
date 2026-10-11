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

fn image_signature(path: &Path) -> image::ImageResult<[u8; 8]> {
    let mut signature = [0; 8];
    std::fs::File::open(path)?.read_exact(&mut signature)?;
    Ok(signature)
}

/// Raw native texture dimensions before resolution hints or image preparation.
/// GIF textures use the first frame's dimensions, ignoring canvas and offsets.
pub fn texture_source_size(path: &Path) -> image::ImageResult<(u32, u32)> {
    let signature = image_signature(path)?;
    if &signature[..6] == b"GIF87a" || &signature[..6] == b"GIF89a" {
        let mut reader = gif_reader(path)?;
        let frame = reader
            .next_frame_info()
            .map_err(gif_error)?
            .ok_or_else(|| decode_error(image::ImageFormat::Gif, "GIF has no frame"))?;
        if frame.width == 0 || frame.height == 0 {
            return Err(image_limit());
        }
        return Ok((u32::from(frame.width), u32::from(frame.height)));
    }
    if &signature[..2] == b"BM" {
        check_bmp_reader_order(path)?;
    }
    image::ImageReader::open(path)?
        .with_guessed_format()?
        .into_dimensions()
}

fn decode_image(path: &Path, color_key: bool) -> image::ImageResult<RgbaImage> {
    let signature = image_signature(path)?;
    if signature == *b"\x89PNG\r\n\x1a\n" {
        return decode_png(path, color_key);
    }
    if &signature[..6] == b"GIF87a" || &signature[..6] == b"GIF89a" {
        return decode_gif(path, color_key);
    }
    if &signature[..2] == b"BM" {
        check_bmp_reader_order(path)?;
        if let Some(image) = decode_indexed_bmp(path, color_key)? {
            return Ok(image);
        }
    }
    let mut image = open_image_fallback(path)?.into_rgba8();
    if color_key {
        key_rgba(&mut image);
    }
    Ok(image)
}

// RageSurface_Load tries a registered extension first. Load_BMP retains
// the previous reader's wrong-format error, so BMP cannot be a fallback after
// another registered bitmap reader. An unknown extension tries BMP first.
fn check_bmp_reader_order(path: &Path) -> image::ImageResult<()> {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["png", "gif", "jpg", "jpeg"]
                .iter()
                .any(|format| extension.eq_ignore_ascii_case(format))
        })
    {
        return Err(decode_error(image::ImageFormat::Bmp, "Unknown file format"));
    }
    Ok(())
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

fn gif_reader(path: &Path) -> image::ImageResult<gif::Decoder<BufReader<std::fs::File>>> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::Indexed);
    let limit = std::num::NonZeroU64::new(image::Limits::default().max_alloc.unwrap_or(u64::MAX))
        .ok_or_else(image_limit)?;
    options.set_memory_limit(gif::MemoryLimit::Bytes(limit));
    options
        .read_info(BufReader::new(std::fs::File::open(path)?))
        .map_err(gif_error)
}

fn decode_gif(path: &Path, color_key: bool) -> image::ImageResult<RgbaImage> {
    let mut reader = gif_reader(path)?;
    let global = reader.global_palette().map(<[u8]>::to_vec);
    let frame = reader
        .next_frame_info()
        .map_err(gif_error)?
        .ok_or_else(|| decode_error(image::ImageFormat::Gif, "GIF has no frame"))?;
    let size = [u32::from(frame.width), u32::from(frame.height)];
    if size.contains(&0) {
        return Err(image_limit());
    }
    check_image_alloc(size)?;
    // RageSurface_Load_GIF leaves unused palette entries transparent black.
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
        colors[index as usize][3] = 0;
    }
    if color_key {
        key_palette(&mut colors);
    }
    let mut indices = vec![0; size[0] as usize * size[1] as usize];
    reader.read_into_buffer(&mut indices).map_err(gif_error)?;
    let mut image = RgbaImage::new(size[0], size[1]);
    for (pixel, index) in image.pixels_mut().zip(indices) {
        pixel.0 = colors[index as usize];
    }
    Ok(image)
}

// Native indexed BMP parsing stays together: DIB12/DIB40 headers, palette,
// padded bottom-up rows and bit unpacking share the file offset contract.
// Other BMP formats retain the existing decoder pending native mask validation.
fn decode_indexed_bmp(path: &Path, color_key: bool) -> image::ImageResult<Option<RgbaImage>> {
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
    let (size, planes, bits, compression, declared_colors) = if header_size == 12 {
        (
            [u32::from(le16(18)), u32::from(le16(20))],
            le16(22),
            le16(24),
            0,
            0,
        )
    } else {
        ([le32(18), le32(22)], le16(26), le16(28), le32(30), le32(46))
    };
    if !matches!(bits, 1 | 4 | 8) || compression != 0 {
        return Ok(None);
    }
    if size.contains(&0) {
        return Err(image_limit());
    }
    check_image_alloc(size)?;
    let color_count = if declared_colors == 0 {
        1 << bits
    } else {
        declared_colors
    };
    if planes != 1 || color_count > 256 {
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
    if color_key {
        key_palette(&mut colors);
    }
    let pitch = usize::try_from((u64::from(size[0]) * u64::from(bits)).div_ceil(32) * 4)
        .map_err(|_| image_limit())?;
    let mut row = vec![0; pitch];
    input.seek(SeekFrom::Start(u64::from(data_offset)))?;
    let mut image = RgbaImage::new(size[0], size[1]);
    for y in (0..size[1]).rev() {
        input.read_exact(&mut row)?;
        for x in 0..size[0] {
            let index = match bits {
                1 => (row[x as usize / 8] >> (7 - x % 8)) & 1,
                // Pinned RageSurface_Load_BMP reads the low nibble first,
                // including the padding nibble at the end of an odd row.
                4 => (row[x as usize / 2] >> (4 * (x % 2))) & 15,
                _ => row[x as usize],
            };
            image.put_pixel(x, y, image::Rgba(colors[index as usize]));
        }
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

// PNG indexed entries must remain distinct: native palette color keying
// changes the first matching entry, even when another entry has identical RGB.
// Native PNG loading strips 16-bit channels rather than rescaling their values.
fn decode_png(path: &Path, color_key: bool) -> image::ImageResult<RgbaImage> {
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
    for pixel in image.pixels_mut() {
        if pixel.0 == key {
            pixel.0 = [0; 4];
        }
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
fn zoom_axis(source: u32, destination: u32) -> Vec<(u32, u32, u32)> {
    let ratio = source as f32 / destination as f32;
    (0..destination)
        .map(|x| {
            if source >= destination {
                let center = ratio * x as f32 + ratio / 2.0;
                let left = (center - ratio / 4.0) as u32;
                let right = (center + ratio / 4.0) as u32;
                let weight = if left == right {
                    1 << 24
                } else {
                    ((1.0 - (center - (left as f32 + 0.5)) / (right - left) as f32) * 16777216.0)
                        as u32
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
        .collect()
}

fn zoom_image(mut image: RgbaImage, size: [u32; 2]) -> RgbaImage {
    while [image.width(), image.height()] != size {
        let next = std::array::from_fn::<_, 2, _>(|axis| {
            let current = [image.width(), image.height()][axis];
            let ratio = (size[axis] as f32 / current as f32).clamp(0.5, 2.0);
            (current as f32 * ratio).round_ties_even() as u32
        });
        let xs = zoom_axis(image.width(), next[0]);
        let ys = zoom_axis(image.height(), next[1]);
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
