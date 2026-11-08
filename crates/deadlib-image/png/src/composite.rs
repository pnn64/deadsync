use crate::codec::Header;
use crate::{
    BlendOp, ColorType, DisposeOp, Error, FrameControl, Image, Limits, Png, Samples, Transparency,
    reserve,
};

fn validate(png: &Png) -> Result<(), Error> {
    png.image
        .validate()
        .map_err(|_| Error::Invalid("image samples"))?;
    crate::codec::validate_pixels(png, crate::PalettePolicy::RecoverBlack)?;
    let color = png.image.color_type;
    if let Some(t) = &png.transparency {
        match t {
            Transparency::Grayscale(_) if color == ColorType::Grayscale => (),
            Transparency::Rgb(_) if color == ColorType::Rgb => (),
            Transparency::Palette(v)
                if color == ColorType::Indexed && v.len() <= png.palette.len() => {}
            _ => return Err(Error::Invalid("transparency for color type")),
        }
    }
    crate::apng::validate(
        png,
        Header {
            width: png.image.width,
            height: png.image.height,
            color,
            depth: png.image.bit_depth,
            interlaced: png.interlaced,
        },
        crate::PalettePolicy::RecoverBlack,
    )
}

#[inline]
fn palette_rgba(png: &Png, index: usize) -> [u16; 4] {
    let Some(&[r, g, b]) = png.palette.get(index) else {
        return [0, 0, 0, 65535];
    };
    let a = match &png.transparency {
        Some(Transparency::Palette(v)) => v.get(index).copied().unwrap_or(255),
        _ => 255,
    };
    [
        u16::from(r) * 257,
        u16::from(g) * 257,
        u16::from(b) * 257,
        u16::from(a) * 257,
    ]
}

#[inline(always)]
fn rgba(png: &Png, image: &Image, index: usize, factor: u16) -> [u16; 4] {
    let raw = |n| match &image.samples {
        Samples::U8(v) => u16::from(v[n]),
        Samples::U16(v) => v[n],
    };
    let mask = ((1u32 << image.bit_depth) - 1) as u16;
    let scale = |v: u16| v * factor;
    let base = index * image.color_type.channels();
    match image.color_type {
        ColorType::Grayscale => {
            let value = raw(base);
            let g = scale(value);
            let a = if matches!(png.transparency,Some(Transparency::Grayscale(v)) if value==(v&mask))
            {
                0
            } else {
                65535
            };
            [g, g, g, a]
        }
        ColorType::Rgb => {
            let values = [raw(base), raw(base + 1), raw(base + 2)];
            let a = if matches!(png.transparency,Some(Transparency::Rgb(v)) if values==v.map(|v| v&mask))
            {
                0
            } else {
                65535
            };
            [scale(values[0]), scale(values[1]), scale(values[2]), a]
        }
        ColorType::Indexed => {
            let index = raw(base) as usize;
            let Some(&[r, g, b]) = png.palette.get(index) else {
                return [0, 0, 0, 65535];
            };
            let a = match &png.transparency {
                Some(Transparency::Palette(v)) => v.get(index).copied().unwrap_or(255),
                _ => 255,
            };
            [
                u16::from(r) * 257,
                u16::from(g) * 257,
                u16::from(b) * 257,
                u16::from(a) * 257,
            ]
        }
        ColorType::GrayscaleAlpha => {
            let g = scale(raw(base));
            [g, g, g, scale(raw(base + 1))]
        }
        ColorType::Rgba => [
            scale(raw(base)),
            scale(raw(base + 1)),
            scale(raw(base + 2)),
            scale(raw(base + 3)),
        ],
    }
}

fn canvas_len(png: &Png, limits: Limits) -> Result<usize, Error> {
    let count = (png.image.width as usize)
        .checked_mul(png.image.height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or(Error::LimitExceeded)?;
    if count > limits.max_output_bytes / 2 {
        return Err(Error::LimitExceeded);
    }
    Ok(count)
}
impl Png {
    /// Explicitly expand to RGBA16 without applying gamma, ICC or HDR transforms.
    /// Sub-byte samples and palette channels scale to the complete u16 range.
    /// Out-of-range palette indices expand to opaque black (PNG 3, 13.1).
    pub fn to_rgba16(&self, limits: Limits) -> Result<Image, Error> {
        validate(self)?;
        let pixels = canvas_len(self, limits)? / 4;
        let mut out: Vec<[u16; 4]> = Vec::new();
        out.try_reserve_exact(pixels)
            .map_err(|_| Error::Allocation)?;
        // Every valid sample mask (1, 3, 15, 255, 65535) divides 65535 exactly.
        let factor = (65535 / ((1u32 << self.image.bit_depth) - 1)) as u16;
        match (&self.image.samples, self.image.color_type) {
            (Samples::U8(samples), ColorType::Rgba) => {
                out.extend(
                    samples
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|pixel| pixel.map(|v| u16::from(v) * 257)),
                );
            }
            (Samples::U16(samples), ColorType::Rgba) => {
                out.extend_from_slice(samples.as_chunks::<4>().0)
            }
            (Samples::U8(samples), ColorType::Indexed) if samples.len() >= 256 => {
                let mut palette = [[0, 0, 0, 65535]; 256];
                for (out, &[r, g, b]) in palette.iter_mut().zip(&self.palette) {
                    *out = [
                        u16::from(r) * 257,
                        u16::from(g) * 257,
                        u16::from(b) * 257,
                        65535,
                    ];
                }
                if let Some(Transparency::Palette(alpha)) = &self.transparency {
                    for (out, &alpha) in palette.iter_mut().zip(alpha) {
                        out[3] = u16::from(alpha) * 257;
                    }
                }
                out.extend(samples.iter().map(|&index| palette[usize::from(index)]));
            }
            (Samples::U8(samples), ColorType::Indexed) => {
                out.extend(
                    samples
                        .iter()
                        .map(|&index| palette_rgba(self, usize::from(index))),
                );
            }
            _ => {
                out.extend((0..pixels).map(|n| rgba(self, &self.image, n, factor)));
            }
        }
        Ok(Image {
            width: self.image.width,
            height: self.image.height,
            color_type: ColorType::Rgba,
            bit_depth: 16,
            samples: Samples::U16(out.into_flattened()),
        })
    }
    pub fn compositor(&self, limits: Limits) -> Result<Compositor<'_>, Error> {
        validate(self)?;
        if self.animation.is_none() {
            return Err(Error::Invalid("no APNG animation"));
        }
        let count = canvas_len(self, limits)?;
        let mut canvas = Vec::new();
        reserve(&mut canvas, count)?;
        canvas.resize(count, 0);
        Ok(Compositor {
            png: self,
            canvas,
            previous: Vec::new(),
            index: 0,
            limits,
        })
    }
}

/// Borrowed view of the entire rendered canvas. Samples are straight RGBA16.
pub struct RenderedFrame<'a> {
    pub width: u32,
    pub height: u32,
    pub control: FrameControl,
    pub samples: &'a [u16],
}

/// Reuses a single canvas and (only for PREVIOUS disposal) a saved frame region.
/// OVER uses integer arithmetic, rounded to nearest at 16-bit output precision.
/// Compositing uses stored sample space; color management is caller-controlled.
pub struct Compositor<'a> {
    png: &'a Png,
    canvas: Vec<u16>,
    previous: Vec<u16>,
    index: usize,
    limits: Limits,
}
impl Compositor<'_> {
    pub fn reset(&mut self) {
        self.index = 0;
        self.canvas.fill(0);
        self.previous.clear();
    }
    pub fn next_frame(&mut self) -> Result<Option<RenderedFrame<'_>>, Error> {
        let animation = self
            .png
            .animation
            .as_ref()
            .ok_or(Error::Invalid("no APNG animation"))?;
        if self.index == animation.frames.len() {
            return Ok(None);
        }
        let stride = self.png.image.width as usize * 4;
        let frame = &animation.frames[self.index];
        let c = frame.control;
        let image = frame.image.as_ref().unwrap_or(&self.png.image);
        let factor = (65535 / ((1u32 << image.bit_depth) - 1)) as u16;
        if c.dispose == DisposeOp::Previous && self.index != 0 {
            let count = c.width as usize * c.height as usize * 4;
            if self
                .canvas
                .len()
                .checked_add(count)
                .ok_or(Error::LimitExceeded)?
                > self.limits.max_output_bytes / 2
            {
                return Err(Error::LimitExceeded);
            }
            let additional = count.saturating_sub(self.previous.len());
            reserve(&mut self.previous, additional)?;
        }
        if self.index != 0 {
            let control = animation.frames[self.index - 1].control;
            if control.dispose != DisposeOp::None {
                let width = control.width as usize * 4;
                for y in 0..control.height as usize {
                    let start =
                        (control.y_offset as usize + y) * stride + control.x_offset as usize * 4;
                    if control.dispose == DisposeOp::Previous && self.index > 1 {
                        self.canvas[start..start + width]
                            .copy_from_slice(&self.previous[y * width..(y + 1) * width]);
                    } else {
                        self.canvas[start..start + width].fill(0);
                    }
                }
            }
        }
        self.previous.clear();
        if c.dispose == DisposeOp::Previous && self.index != 0 {
            for y in 0..c.height as usize {
                let start = (c.y_offset as usize + y) * stride + c.x_offset as usize * 4;
                self.previous
                    .extend_from_slice(&self.canvas[start..start + c.width as usize * 4]);
            }
        }
        if c.blend == BlendOp::Source && image.color_type == ColorType::Rgba {
            let width = c.width as usize * 4;
            for y in 0..c.height as usize {
                let offset = (c.y_offset as usize + y) * stride + c.x_offset as usize * 4;
                let row = &mut self.canvas[offset..offset + width];
                let start = y * width;
                match &image.samples {
                    Samples::U8(samples) => {
                        for (out, &value) in row.iter_mut().zip(&samples[start..start + width]) {
                            *out = u16::from(value) * 257;
                        }
                    }
                    Samples::U16(samples) => row.copy_from_slice(&samples[start..start + width]),
                }
            }
        } else {
            for y in 0..c.height as usize {
                for x in 0..c.width as usize {
                    let src = rgba(self.png, image, y * c.width as usize + x, factor);
                    let offset = (c.y_offset as usize + y) * stride + (c.x_offset as usize + x) * 4;
                    let dst: &mut [u16; 4] =
                        (&mut self.canvas[offset..offset + 4]).try_into().unwrap();
                    if c.blend == BlendOp::Source || src[3] == 65535 {
                        *dst = src;
                    } else if src[3] == 0 {
                        continue;
                    } else if dst[3] == 65535 {
                        // An opaque destination stays opaque. The exact rounded
                        // numerator is at most 65535^2 + 32767, which fits in u32.
                        let sa = u32::from(src[3]);
                        let inv = 65535 - sa;
                        for n in 0..3 {
                            dst[n] = ((u32::from(src[n]) * sa + u32::from(dst[n]) * inv + 32767)
                                / 65535) as u16;
                        }
                    } else {
                        let sa = u64::from(src[3]);
                        let da = u64::from(dst[3]);
                        let inv = 65535 - sa;
                        let alpha = sa * 65535 + da * inv;
                        for n in 0..3 {
                            dst[n] = ((u64::from(src[n]) * sa * 65535
                                + u64::from(dst[n]) * da * inv
                                + alpha / 2)
                                / alpha) as u16;
                        }
                        dst[3] = ((alpha + 32767) / 65535) as u16;
                    }
                }
            }
        }
        self.index += 1;
        Ok(Some(RenderedFrame {
            width: self.png.image.width,
            height: self.png.image.height,
            control: c,
            samples: &self.canvas,
        }))
    }
}
