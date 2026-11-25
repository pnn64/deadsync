use crate::{ColorType, Error, Image, Limits, Samples, decode::validate_text, model::*, reserve};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Background {
    /// Specification background index in the global (or external) palette.
    #[default]
    LogicalScreen,
    /// Common image-viewer policy: a transparent initial/disposed canvas.
    Transparent,
    Color([u8; 4]),
}
#[derive(Clone, Debug)]
pub struct CompositeOptions {
    pub background: Background,
    pub max_pixels: usize,
    pub external_palette: Option<Palette>,
}
impl Default for CompositeOptions {
    fn default() -> Self {
        Self {
            background: Background::LogicalScreen,
            max_pixels: Limits::default().max_frame_pixels,
            external_palette: None,
        }
    }
}

#[derive(Debug)]
pub struct RenderedFrame<'a> {
    pub width: u16,
    pub height: u16,
    pub rgba: &'a [u8],
    pub control: Option<GraphicControl>,
}

/// GIF leaves font choice to the decoder. Fill a row-major cell mask (zero =
/// background, nonzero = foreground). The codec handles layout, palette,
/// transparency, clipping, control scope, and disposal. The mask starts zeroed.
pub trait TextRasterizer {
    fn rasterize(
        &mut self,
        character: u8,
        width: u8,
        height: u8,
        mask: &mut [u8],
    ) -> Result<(), Error>;
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Rect {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}
impl Rect {
    fn spans(self, stride: usize) -> impl Iterator<Item = std::ops::Range<usize>> {
        let start = self.y * stride + self.x;
        // A full-width rectangle is one contiguous span of the canvas.
        let (width, rows) = if self.w == stride {
            (self.w * self.h, 1)
        } else {
            (self.w, self.h)
        };
        (0..rows).map(move |row| {
            let offset = start + row * stride;
            offset..offset + width
        })
    }
}

/// One reusable RGBA8 canvas plus an optional saved *rectangle*, not a second
/// whole-screen snapshot. Disposal is applied just before the next graphic.
/// Unspecified/reserved disposal keeps pixels. Zero delay is never clamped.
pub struct Compositor {
    width: u16,
    height: u16,
    max_pixels: usize,
    palette: [[u8; 4]; 256],
    palette_len: Option<usize>,
    background: [u8; 4],
    pixels: Vec<[u8; 4]>,
    saved: Vec<[u8; 4]>,
    pending: Option<(Rect, Disposal)>,
    text_coverage: Vec<u8>,
    mask: Vec<u8>,
}
impl Compositor {
    pub fn new(header: &Header) -> Result<Self, Error> {
        Self::with_options(header, CompositeOptions::default())
    }
    pub fn with_options(header: &Header, options: CompositeOptions) -> Result<Self, Error> {
        if options.background == Background::LogicalScreen {
            header.validate()?;
        } else {
            // Transparent and caller-supplied backgrounds never use the stored
            // background index, which compatibility decoding may retain intact.
            header.validate_structure()?;
        }
        if let Some(p) = &options.external_palette {
            validate_palette(p)?;
        }
        let count = header.width as usize * header.height as usize;
        if count > options.max_pixels {
            return Err(Error::LimitExceeded);
        }
        let palette = header
            .global_palette
            .as_deref()
            .or(options.external_palette.as_deref());
        let background = match options.background {
            Background::Transparent => [0; 4],
            Background::Color(c) => c,
            Background::LogicalScreen => {
                let p = palette.ok_or(Error::MissingPalette)?;
                let c = p
                    .get(header.background_index as usize)
                    .ok_or(Error::Invalid("background palette index"))?;
                [c[0], c[1], c[2], 255]
            }
        };
        count.checked_mul(4).ok_or(Error::LimitExceeded)?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(count)
            .map_err(|_| Error::Allocation)?;
        pixels.resize(count, background);
        Ok(Self {
            width: header.width,
            height: header.height,
            max_pixels: options.max_pixels,
            palette: expand_palette(palette.unwrap_or_default()),
            palette_len: palette.map(<[_]>::len),
            background,
            pixels,
            saved: Vec::new(),
            pending: None,
            text_coverage: Vec::new(),
            mask: Vec::new(),
        })
    }
    pub fn pixels(&self) -> &[u8] {
        self.pixels.as_flattened()
    }
    /// Move the current composited RGBA8 canvas into a shared image without copying.
    pub fn into_image(self) -> Image {
        Image {
            width: self.width.into(),
            height: self.height.into(),
            color_type: ColorType::Rgba,
            bit_depth: 8,
            samples: Samples::U8(self.pixels.into_flattened()),
        }
    }
    /// Start another playback with the configured background.
    pub fn reset(&mut self) {
        self.pixels.fill(self.background);
        self.pending = None;
        self.saved.clear();
    }
    fn clip(&self, left: u16, top: u16, width: u16, height: u16) -> Rect {
        let x = (left as usize).min(self.width as usize);
        let y = (top as usize).min(self.height as usize);
        Rect {
            x,
            y,
            w: (width as usize).min(self.width as usize - x),
            h: (height as usize).min(self.height as usize - y),
        }
    }
    fn prepare(&mut self, rect: Rect, control: Option<GraphicControl>) -> Result<bool, Error> {
        let disposal = control.unwrap_or_default().disposal;
        let visible = rect.w != 0 && rect.h != 0;
        let save = visible && disposal == Disposal::Previous;
        // Reserve before changing state; old saved pixels must remain intact
        // until the preceding Previous disposal has been applied.
        if save {
            let required = rect.w * rect.h;
            let extra = required.saturating_sub(self.saved.len());
            reserve(&mut self.saved, extra)?;
        }
        let stride = self.width as usize;
        let keep_saved = match self.pending {
            Some((r, Disposal::Background)) => {
                for span in r.spans(stride) {
                    self.pixels[span].fill(self.background);
                }
                false
            }
            Some((r, Disposal::Previous)) => {
                let mut saved = self.saved.as_slice();
                for span in r.spans(stride) {
                    let dest = &mut self.pixels[span];
                    let (source, remaining) = saved.split_at(dest.len());
                    dest.copy_from_slice(source);
                    saved = remaining;
                }
                // The restored pixels already match this snapshot.
                save && r == rect
            }
            _ => false,
        };
        if !keep_saved {
            self.saved.clear();
            if save {
                for span in rect.spans(stride) {
                    self.saved.extend_from_slice(&self.pixels[span]);
                }
            }
        }
        self.pending = (visible && matches!(disposal, Disposal::Background | Disposal::Previous))
            .then_some((rect, disposal));
        Ok(visible)
    }
    pub fn render(&mut self, frame: &Frame) -> Result<RenderedFrame<'_>, Error> {
        frame.validate::<false>((self.width, self.height), self.palette_len, false)?;
        let local = frame.local_palette.as_deref();
        if local.is_none() && self.palette_len.is_none() {
            return Err(Error::MissingPalette);
        }
        let r = self.clip(frame.left, frame.top, frame.width, frame.height);
        let transparent = frame.control.and_then(|c| c.transparent_index);
        let mut reuse_saved = false;
        if let Some((pending, pending_disposal)) = self.pending
            && transparent.is_none()
        {
            if frame
                .control
                .is_some_and(|control| control.disposal == Disposal::Previous)
            {
                // The existing snapshot already contains the next frame's background.
                reuse_saved = pending_disposal == Disposal::Previous && pending == r;
            } else if r.x <= pending.x
                && r.y <= pending.y
                && pending.x + pending.w <= r.x + r.w
                && pending.y + pending.h <= r.y + r.h
            {
                self.pending = None;
            }
        }
        if reuse_saved || self.prepare(r, frame.control)? {
            let stride = self.width as usize;
            let width = frame.width as usize;
            let visible = r.w * r.h;
            // Wide rows already use bulk fills. Probe them only to skip transparent spans.
            let solid = if (width < 32 || transparent.is_some())
                && r.w == stride
                && width == stride
                && visible >= 256
            {
                let indices = &frame.indices[..visible];
                let first = indices[0];
                if (width < 32 || transparent == Some(first))
                    && first == indices[visible - 1]
                    && first == indices[visible / 2]
                {
                    crate::lzw::repeated_index(indices)
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(index) = solid {
                if transparent != Some(index) {
                    let index = usize::from(index);
                    let color = if let Some(palette) = local {
                        let [red, green, blue] = palette[index];
                        [red, green, blue, 255]
                    } else {
                        self.palette[index]
                    };
                    let start = r.y * stride;
                    self.pixels[start..start + visible].fill(color);
                }
            } else {
                let binary_colors = match local {
                    Some(&[a, b]) => Some([a, b].map(|[r, g, b]| [r, g, b, 255])),
                    None if self.palette_len == Some(2) => Some([self.palette[0], self.palette[1]]),
                    _ => None,
                };
                let canvas = (self.pixels.as_mut_slice(), stride);
                if let Some(colors) = binary_colors {
                    draw_binary_indices::<false>(
                        canvas,
                        r,
                        &frame.indices,
                        width,
                        colors,
                        transparent,
                    );
                } else if visible <= 256
                    && r.w == stride
                    && width == stride
                    && let Some(palette) = local.and_then(|p| p.first_chunk::<256>())
                {
                    let start = r.y * stride;
                    let transparent = transparent.map_or(256, u16::from);
                    for (pixel, &index) in canvas.0[start..start + visible]
                        .iter_mut()
                        .zip(&frame.indices)
                    {
                        if u16::from(index) != transparent {
                            let [red, green, blue] = palette[usize::from(index)];
                            *pixel = [red, green, blue, 255];
                        }
                    }
                } else {
                    let local_table;
                    let table = if let Some(p) = local {
                        local_table = expand_palette(p);
                        &local_table
                    } else {
                        &self.palette
                    };
                    draw_indices(canvas, r, &frame.indices, width, table, transparent);
                }
            }
        }
        Ok(RenderedFrame {
            width: self.width,
            height: self.height,
            rgba: self.pixels.as_flattened(),
            control: frame.control,
        })
    }
    /// Non-graphic extensions leave the canvas and pending disposal untouched.
    pub fn render_block(&mut self, block: &Block) -> Result<Option<RenderedFrame<'_>>, Error> {
        match block {
            Block::Frame(frame) => self.render(frame).map(Some),
            Block::Extension(Extension::PlainText(_)) => Err(Error::TextRendererRequired),
            Block::Extension(Extension::Unknown { label, .. }) if is_graphic_label(*label) => {
                Err(Error::UnsupportedGraphicExtension(*label))
            }
            _ => Ok(None),
        }
    }
    pub fn render_text(
        &mut self,
        text: &PlainText,
        rasterizer: &mut impl TextRasterizer,
    ) -> Result<RenderedFrame<'_>, Error> {
        validate_text(text, (self.width, self.height), self.palette_len, false)?;
        if self.palette_len.is_none() {
            return Err(Error::MissingPalette);
        }
        let count = text.width as usize * text.height as usize;
        if count > self.max_pixels {
            return Err(Error::LimitExceeded);
        }
        let rect = self.clip(text.left, text.top, text.width, text.height);
        let cw = text.cell_width as usize;
        let ch = text.cell_height as usize;
        let columns = text.width as usize / cw;
        let rows = text.height as usize / ch;
        let glyph_count = columns * rows;
        let right = columns * cw;
        let has_glyphs = glyph_count != 0 && text.data.iter().any(|block| !block.is_empty());
        let background = text.background_index;
        let foreground = text.foreground_index;
        let single_color = !has_glyphs || background == foreground;
        let coverage_height = if single_color { 0 } else { rect.h };
        if !single_color {
            let visible = rect.w * rect.h;
            let mut remaining = glyph_count;
            if visible == 0
                || (self.text_coverage.len() >= visible
                    && right >= rect.w
                    && rows * ch >= rect.h
                    && text.data.iter().any(|block| {
                        remaining = remaining.saturating_sub(block.len());
                        remaining == 0
                    }))
            {
                self.text_coverage.truncate(visible);
            } else {
                self.text_coverage.clear();
                reserve(&mut self.text_coverage, visible)?;
                self.text_coverage.resize(visible, 0);
            }
        }
        if has_glyphs {
            let mask_size = cw * ch;
            let extra = mask_size.saturating_sub(self.mask.len());
            reserve(&mut self.mask, extra)?;
            self.mask.resize(mask_size, 0);
        }
        let (mut x, mut y) = (0, 0);
        for &character in text.data.iter().flatten().take(glyph_count) {
            self.mask.fill(0);
            rasterizer.rasterize(
                if !(0x20..=0xf7).contains(&character) {
                    b' '
                } else {
                    character
                },
                text.cell_width,
                text.cell_height,
                &mut self.mask,
            )?;
            if x < rect.w && y < coverage_height {
                let width = cw.min(rect.w - x);
                for (row, mask) in self
                    .mask
                    .chunks_exact(cw)
                    .take(coverage_height - y)
                    .enumerate()
                {
                    let start = (y + row) * rect.w + x;
                    let dest = &mut self.text_coverage[start..start + width];
                    dest.copy_from_slice(&mask[..width]);
                }
            }
            x += cw;
            if x == right {
                x = 0;
                y += ch;
            }
        }
        if self.prepare(rect, text.control)? {
            let transparent = text.control.and_then(|c| c.transparent_index);
            if single_color {
                if transparent != Some(background) {
                    let color = self.palette[usize::from(background)];
                    for span in rect.spans(self.width as usize) {
                        self.pixels[span].fill(color);
                    }
                }
            } else {
                let transparent = match transparent {
                    Some(i) if i == background => Some(0),
                    Some(i) if i == foreground => Some(1),
                    _ => None,
                };
                draw_binary_indices::<true>(
                    (&mut self.pixels, self.width as usize),
                    rect,
                    &self.text_coverage,
                    rect.w,
                    [
                        self.palette[usize::from(background)],
                        self.palette[usize::from(foreground)],
                    ],
                    transparent,
                );
            }
        }
        Ok(RenderedFrame {
            width: self.width,
            height: self.height,
            rgba: self.pixels.as_flattened(),
            control: text.control,
        })
    }
}

#[inline(never)]
fn draw_indices(
    (canvas, stride): (&mut [[u8; 4]], usize),
    r: Rect,
    indices: &[u8],
    width: usize,
    table: &[[u8; 4]; 256],
    transparent: Option<u8>,
) {
    let rows = &mut canvas[r.y * stride..(r.y + r.h) * stride];
    let paint_opaque = |dest: &mut [[u8; 4]], indices: &[u8]| {
        for (pixel, &index) in dest.iter_mut().zip(indices) {
            *pixel = table[usize::from(index)];
        }
    };
    if width < 32 && r.w == stride && width == stride {
        if let Some(t) = transparent {
            draw_narrow_transparent(rows, indices, table, t);
        } else {
            paint_opaque(rows, indices);
        }
        return;
    }
    for (source, target) in indices
        .chunks_exact(width)
        .zip(rows.chunks_exact_mut(stride))
    {
        let indices = &source[..r.w];
        let dest = &mut target[r.x..r.x + r.w];
        if let Some(index) = uniform_index(indices) {
            if transparent != Some(index) {
                dest.fill(table[index as usize]);
            }
            continue;
        }
        if let Some(t) = transparent {
            if indices.len() >= 64
                && frequent_transparency_changes(*indices.first_chunk::<8>().unwrap(), t)
            {
                for (pixel, &index) in dest.iter_mut().zip(indices) {
                    let source = if index == t {
                        &*pixel
                    } else {
                        &table[usize::from(index)]
                    };
                    *pixel = *source;
                }
            } else {
                for (pixel, &index) in dest.iter_mut().zip(indices) {
                    if index != t {
                        *pixel = table[usize::from(index)];
                    }
                }
            }
        } else {
            paint_opaque(dest, indices);
        }
    }
}

#[inline(never)]
fn draw_narrow_transparent(
    dest: &mut [[u8; 4]],
    indices: &[u8],
    table: &[[u8; 4]; 256],
    transparent: u8,
) {
    for (pixel, &index) in dest.iter_mut().zip(indices) {
        if index != transparent {
            *pixel = table[usize::from(index)];
        }
    }
}

fn frequent_transparency_changes(sample: [u8; 8], transparent: u8) -> bool {
    const HIGH_BITS: u64 = 0x8080_8080_8080_8080;
    let different = u64::from_le_bytes(sample) ^ u64::from_le_bytes([transparent; 8]);
    // A byte's high bit stays clear only when all of its bits were zero.
    let matches = !(((different & !HIGH_BITS).wrapping_add(!HIGH_BITS)) | different) & HIGH_BITS;
    (matches ^ (matches << 8)).count_ones() >= 3
}

#[inline(never)]
fn draw_binary_indices<const NORMALIZE: bool>(
    (canvas, stride): (&mut [[u8; 4]], usize),
    r: Rect,
    indices: &[u8],
    width: usize,
    colors: [[u8; 4]; 2],
    transparent: Option<u8>,
) {
    // Frame indices are already binary. Glyphs can supply any nonzero byte.
    let binary = |index| {
        if NORMALIZE {
            u8::from(index != 0)
        } else {
            index
        }
    };
    let [background, foreground] = colors;
    let first = u32::from_ne_bytes(background);
    let difference = first ^ u32::from_ne_bytes(foreground);
    let blend = transparent.map(|t| {
        (
            t,
            u32::from_ne_bytes(if t == 0 { foreground } else { background }),
        )
    });
    let draw_span = |src: &[u8], dst: &mut [[u8; 4]]| {
        if let Some(index) = uniform_index(src) {
            let index = binary(index);
            if transparent != Some(index) {
                dst.fill(if index == 0 { background } else { foreground });
            }
            return;
        }
        if let Some((t, paint)) = blend {
            for (pixel, &index) in dst.iter_mut().zip(src) {
                let keep = u32::from(binary(index) == t).wrapping_neg();
                let old = u32::from_ne_bytes(*pixel);
                *pixel = ((old & keep) | (paint & !keep)).to_ne_bytes();
            }
        } else {
            for (pixel, &index) in dst.iter_mut().zip(src) {
                let select = u32::from(binary(index)).wrapping_neg();
                *pixel = (first ^ (difference & select)).to_ne_bytes();
            }
        }
    };
    let rows = &mut canvas[r.y * stride..(r.y + r.h) * stride];
    // Short, full-width rows can share one vectorized span and its tail.
    if r.w == stride && r.w == width && width < 32 {
        draw_span(&indices[..rows.len()], rows);
    } else {
        for (src, dst) in indices
            .chunks_exact(width)
            .zip(rows.chunks_exact_mut(stride))
        {
            draw_span(&src[..r.w], &mut dst[r.x..r.x + r.w]);
        }
    }
}

#[inline]
fn uniform_index(indices: &[u8]) -> Option<u8> {
    // Short rows are cheaper to expand directly.
    if indices.len() < 32 {
        return None;
    }
    let index = indices[0];
    // Fixed-size equality lets the compiler compare whole vectors.
    let (chunks, tail) = indices.as_chunks::<16>();
    (chunks.iter().all(|chunk| *chunk == [index; 16]) && tail.iter().all(|&value| value == index))
        .then_some(index)
}

fn expand_palette(colors: &[[u8; 3]]) -> [[u8; 4]; 256] {
    let mut table = [[0; 4]; 256];
    for (dest, &[r, g, b]) in table.iter_mut().zip(colors) {
        *dest = [r, g, b, 255];
    }
    table
}
