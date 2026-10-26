use crate::{Error, Limits, decode::validate_text, model::*, reserve};

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
    header: Header,
    options: CompositeOptions,
    background: [u8; 4],
    pixels: Vec<u8>,
    saved: Vec<u8>,
    pending: Option<(Rect, Disposal)>,
    text_indices: Vec<u8>,
    mask: Vec<u8>,
}
impl Compositor {
    pub fn new(header: &Header) -> Result<Self, Error> {
        Self::with_options(header, CompositeOptions::default())
    }
    pub fn with_options(header: &Header, options: CompositeOptions) -> Result<Self, Error> {
        header.validate()?;
        if let Some(p) = &options.external_palette {
            validate_palette(p)?;
        }
        let count = header.width as usize * header.height as usize;
        if count > options.max_pixels {
            return Err(Error::LimitExceeded);
        }
        let background = match options.background {
            Background::Transparent => [0; 4],
            Background::Color(c) => c,
            Background::LogicalScreen => {
                let p = header
                    .global_palette
                    .as_deref()
                    .or(options.external_palette.as_deref())
                    .ok_or(Error::MissingPalette)?;
                let c = p
                    .get(header.background_index as usize)
                    .ok_or(Error::Invalid("background palette index"))?;
                [c[0], c[1], c[2], 255]
            }
        };
        let mut pixels = Vec::new();
        reserve(
            &mut pixels,
            count.checked_mul(4).ok_or(Error::LimitExceeded)?,
        )?;
        for _ in 0..count {
            pixels.extend_from_slice(&background);
        }
        Ok(Self {
            header: header.clone(),
            options,
            background,
            pixels,
            saved: Vec::new(),
            pending: None,
            text_indices: Vec::new(),
            mask: Vec::new(),
        })
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    /// Start another playback with the configured background.
    pub fn reset(&mut self) {
        for pixel in self.pixels.as_chunks_mut::<4>().0 {
            pixel.copy_from_slice(&self.background);
        }
        self.pending = None;
        self.saved.clear();
    }
    fn clip(&self, left: u16, top: u16, width: u16, height: u16) -> Rect {
        let x = (left as usize).min(self.header.width as usize);
        let y = (top as usize).min(self.header.height as usize);
        Rect {
            x,
            y,
            w: (width as usize).min(self.header.width as usize - x),
            h: (height as usize).min(self.header.height as usize - y),
        }
    }
    fn prepare(&mut self, rect: Rect, control: Option<GraphicControl>) -> Result<(), Error> {
        let disposal = control.unwrap_or_default().disposal;
        // Reserve before changing state; old saved pixels must remain intact
        // until the preceding Previous disposal has been applied.
        if disposal == Disposal::Previous {
            let required = rect.w * rect.h * 4;
            let extra = required.saturating_sub(self.saved.len());
            reserve(&mut self.saved, extra)?;
        }
        let stride = self.header.width as usize * 4;
        if let Some((r, method)) = self.pending {
            for row in 0..r.h {
                let dest = &mut self.pixels
                    [(r.y + row) * stride + r.x * 4..(r.y + row) * stride + (r.x + r.w) * 4];
                match method {
                    Disposal::Background => {
                        for p in dest.as_chunks_mut::<4>().0 {
                            p.copy_from_slice(&self.background);
                        }
                    }
                    Disposal::Previous => {
                        dest.copy_from_slice(&self.saved[row * r.w * 4..(row + 1) * r.w * 4])
                    }
                    _ => {}
                }
            }
        }
        self.saved.clear();
        if disposal == Disposal::Previous {
            for y in rect.y..rect.y + rect.h {
                self.saved.extend_from_slice(
                    &self.pixels[y * stride + rect.x * 4..y * stride + (rect.x + rect.w) * 4],
                );
            }
        }
        self.pending = Some((rect, disposal));
        Ok(())
    }
    pub fn render(&mut self, frame: &Frame) -> Result<RenderedFrame<'_>, Error> {
        frame.validate(&self.header, false)?;
        let p = frame
            .palette(&self.header)
            .or(self.options.external_palette.as_deref())
            .ok_or(Error::MissingPalette)?;
        validate_indices(&frame.indices, p.len())?;
        validate_control(frame.control, p.len())?;
        let mut table = [[0u8; 4]; 256];
        for (dest, src) in table.iter_mut().zip(p) {
            *dest = [src[0], src[1], src[2], 255];
        }
        let transparent = frame.control.and_then(|c| c.transparent_index);
        let r = self.clip(frame.left, frame.top, frame.width, frame.height);
        self.prepare(r, frame.control)?;
        let stride = self.header.width as usize * 4;
        for row in 0..r.h {
            let indices =
                &frame.indices[row * frame.width as usize..row * frame.width as usize + r.w];
            let dest = &mut self.pixels
                [(r.y + row) * stride + r.x * 4..(r.y + row) * stride + (r.x + r.w) * 4];
            if let Some(t) = transparent {
                for (pixel, &index) in dest.as_chunks_mut::<4>().0.iter_mut().zip(indices) {
                    if index != t {
                        pixel.copy_from_slice(&table[index as usize]);
                    }
                }
            } else {
                for (pixel, &index) in dest.as_chunks_mut::<4>().0.iter_mut().zip(indices) {
                    pixel.copy_from_slice(&table[index as usize]);
                }
            }
        }
        Ok(RenderedFrame {
            width: self.header.width,
            height: self.header.height,
            rgba: &self.pixels,
            control: frame.control,
        })
    }
    /// Non-graphic extensions leave the canvas and pending disposal untouched.
    pub fn render_block(&mut self, block: &Block) -> Result<Option<RenderedFrame<'_>>, Error> {
        match block {
            Block::Frame(frame) => self.render(frame).map(Some),
            Block::Extension(Extension::PlainText(_)) => Err(Error::TextRendererRequired),
            _ => Ok(None),
        }
    }
    pub fn render_text(
        &mut self,
        text: &PlainText,
        rasterizer: &mut impl TextRasterizer,
    ) -> Result<RenderedFrame<'_>, Error> {
        validate_text(
            text,
            &self.header,
            self.options.external_palette.as_deref(),
            false,
        )?;
        let p = self
            .header
            .global_palette
            .as_deref()
            .or(self.options.external_palette.as_deref())
            .ok_or(Error::MissingPalette)?;
        let mut palette = Vec::new();
        reserve(&mut palette, p.len())?;
        palette.extend_from_slice(p);
        let count = text.width as usize * text.height as usize;
        if count > self.options.max_pixels {
            return Err(Error::LimitExceeded);
        }
        self.text_indices.clear();
        reserve(&mut self.text_indices, count)?;
        self.text_indices.resize(count, text.background_index);
        let cw = text.cell_width as usize;
        let ch = text.cell_height as usize;
        self.mask.clear();
        reserve(&mut self.mask, cw * ch)?;
        self.mask.resize(cw * ch, 0);
        let columns = text.width as usize / cw;
        let rows = text.height as usize / ch;
        for (n, &character) in text.data.iter().flatten().take(columns * rows).enumerate() {
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
            let x = n % columns * cw;
            let y = n / columns * ch;
            for row in 0..ch {
                for col in 0..cw {
                    if self.mask[row * cw + col] != 0 {
                        self.text_indices[(y + row) * text.width as usize + x + col] =
                            text.foreground_index;
                    }
                }
            }
        }
        let frame = Frame {
            left: text.left,
            top: text.top,
            width: text.width,
            height: text.height,
            indices: std::mem::take(&mut self.text_indices),
            local_palette: Some(palette),
            control: text.control,
            ..Default::default()
        };
        // Restore the reusable text allocation even if rendering fails.
        let result = self.render(&frame).map(|_| ());
        self.text_indices = frame.indices;
        result?;
        Ok(RenderedFrame {
            width: self.header.width,
            height: self.header.height,
            rgba: &self.pixels,
            control: text.control,
        })
    }
}
