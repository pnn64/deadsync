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

#[derive(Clone, Copy)]
struct Rect {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
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
    text_indices: Vec<u8>,
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
            text_indices: Vec::new(),
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
        // A full-width rectangle is one contiguous span of the canvas.
        let spans = |rect: Rect| {
            let start = rect.y * stride + rect.x;
            let (width, rows) = if rect.w == stride {
                (rect.w * rect.h, 1)
            } else {
                (rect.w, rect.h)
            };
            (0..rows).map(move |row| {
                let offset = start + row * stride;
                offset..offset + width
            })
        };
        if let Some((r, method)) = self.pending {
            let mut saved = self.saved.as_slice();
            for span in spans(r) {
                let dest = &mut self.pixels[span];
                match method {
                    Disposal::Background => {
                        dest.fill(self.background);
                    }
                    Disposal::Previous => {
                        let (source, remaining) = saved.split_at(dest.len());
                        dest.copy_from_slice(source);
                        saved = remaining;
                    }
                    _ => {}
                }
            }
        }
        self.saved.clear();
        if save {
            for span in spans(rect) {
                self.saved.extend_from_slice(&self.pixels[span]);
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
        if self.prepare(r, frame.control)? {
            let transparent = frame.control.and_then(|c| c.transparent_index);
            let local_table;
            let table = if let Some(p) = local {
                local_table = expand_palette(p);
                &local_table
            } else {
                &self.palette
            };
            let two_colors = local.map(<[_]>::len).or(self.palette_len) == Some(2);
            let canvas = (self.pixels.as_mut_slice(), self.width as usize);
            if two_colors && transparent.is_none() {
                draw_binary_indices(canvas, r, &frame.indices, frame.width as usize, table);
            } else {
                let opaque = if two_colors {
                    transparent.map(|index| index ^ 1)
                } else {
                    None
                };
                draw_indices(
                    canvas,
                    r,
                    &frame.indices,
                    frame.width as usize,
                    table,
                    transparent,
                    opaque,
                );
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
        self.text_indices.clear();
        let visible = rect.w * rect.h;
        reserve(&mut self.text_indices, visible)?;
        self.text_indices.resize(visible, text.background_index);
        let cw = text.cell_width as usize;
        let ch = text.cell_height as usize;
        self.mask.clear();
        reserve(&mut self.mask, cw * ch)?;
        self.mask.resize(cw * ch, 0);
        let columns = text.width as usize / cw;
        let rows = text.height as usize / ch;
        let right = columns * cw;
        let (mut x, mut y) = (0, 0);
        for &character in text.data.iter().flatten().take(columns * rows) {
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
            if x < rect.w && y < rect.h {
                let width = cw.min(rect.w - x);
                for (row, mask) in self.mask.chunks_exact(cw).take(rect.h - y).enumerate() {
                    let start = (y + row) * rect.w + x;
                    let dest = &mut self.text_indices[start..start + width];
                    for (index, &covered) in dest.iter_mut().zip(mask) {
                        *index = if covered == 0 {
                            text.background_index
                        } else {
                            text.foreground_index
                        };
                    }
                }
            }
            x += cw;
            if x == right {
                x = 0;
                y += ch;
            }
        }
        if self.prepare(rect, text.control)? {
            // The validated text colors are the only two indices rasterization writes.
            let transparent = text
                .control
                .and_then(|c| c.transparent_index)
                .filter(|&index| index == text.foreground_index || index == text.background_index);
            let opaque = transparent.map(|index| {
                if index == text.foreground_index {
                    text.background_index
                } else {
                    text.foreground_index
                }
            });
            draw_indices(
                (&mut self.pixels, self.width as usize),
                rect,
                &self.text_indices,
                rect.w,
                &self.palette,
                transparent,
                opaque,
            );
        }
        Ok(RenderedFrame {
            width: self.width,
            height: self.height,
            rgba: self.pixels.as_flattened(),
            control: text.control,
        })
    }
}

#[inline(always)]
fn draw_indices(
    (canvas, stride): (&mut [[u8; 4]], usize),
    r: Rect,
    indices: &[u8],
    width: usize,
    table: &[[u8; 4]; 256],
    transparent: Option<u8>,
    opaque: Option<u8>,
) {
    for row in 0..r.h {
        let source = row * width;
        let target = (r.y + row) * stride + r.x;
        let indices = &indices[source..source + r.w];
        let dest = &mut canvas[target..target + r.w];
        if let Some(index) = uniform_index(indices) {
            if transparent != Some(index) {
                dest.fill(table[index as usize]);
            }
            continue;
        }
        if let Some(t) = transparent {
            if let Some(index) = opaque {
                // With two colors there is only one opaque value. Blend
                // whole RGBA words so the loop can vectorize.
                let paint = u32::from_ne_bytes(table[usize::from(index)]);
                for (pixel, &index) in dest.iter_mut().zip(indices) {
                    let keep = u32::from(index == t).wrapping_neg();
                    let old = u32::from_ne_bytes(*pixel);
                    *pixel = ((old & keep) | (paint & !keep)).to_ne_bytes();
                }
            } else {
                for (pixel, &index) in dest.iter_mut().zip(indices) {
                    if index != t {
                        *pixel = table[index as usize];
                    }
                }
            }
        } else {
            for (pixel, &index) in dest.iter_mut().zip(indices) {
                *pixel = table[index as usize];
            }
        }
    }
}

#[inline(never)]
fn draw_binary_indices(
    (canvas, stride): (&mut [[u8; 4]], usize),
    r: Rect,
    indices: &[u8],
    width: usize,
    table: &[[u8; 4]; 256],
) {
    // Validated indices are zero or one. Expand them to full-word masks.
    let first = u32::from_ne_bytes(table[0]);
    let difference = first ^ u32::from_ne_bytes(table[1]);
    let src = indices.chunks_exact(width);
    let dst = canvas.chunks_exact_mut(stride).skip(r.y);
    for (src, dst) in src.zip(dst).take(r.h) {
        let src = &src[..r.w];
        let dst = &mut dst[r.x..r.x + r.w];
        if let Some(index) = uniform_index(src) {
            dst.fill(table[usize::from(index)]);
            continue;
        }
        for (pixel, &index) in dst.iter_mut().zip(src) {
            let select = u32::from(index).wrapping_neg();
            *pixel = (first ^ (difference & select)).to_ne_bytes();
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
