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
fn expanded_palette<const ENTRIES: usize>(png: &Png) -> [[u16; 4]; ENTRIES] {
    let mut palette = [[0, 0, 0, 65535]; ENTRIES];
    for (out, &[r, g, b]) in palette.iter_mut().zip(&png.palette) {
        *out = [
            u16::from(r) * 257,
            u16::from(g) * 257,
            u16::from(b) * 257,
            65535,
        ];
    }
    if let Some(Transparency::Palette(alpha)) = &png.transparency {
        for (out, &alpha) in palette.iter_mut().zip(alpha) {
            out[3] = u16::from(alpha) * 257;
        }
    }
    palette
}

#[inline(never)]
fn render_color_rows<T: Copy + Into<u16>>(
    png: &Png,
    image: &Image,
    canvas: &mut [u16],
    stride: usize,
    control: FrameControl,
    samples: &[T],
) {
    let mask = ((1u32 << image.bit_depth) - 1) as u16;
    let factor = u16::MAX / mask;
    match image.color_type {
        ColorType::Grayscale => {
            let key = match png.transparency {
                Some(Transparency::Grayscale(value)) => Some([value & mask]),
                _ => None,
            };
            render_keyed_rows(canvas, stride, control, samples, key, factor);
        }
        ColorType::Rgb => {
            let key = match png.transparency {
                Some(Transparency::Rgb(rgb)) => Some(rgb.map(|value| value & mask)),
                _ => None,
            };
            render_keyed_rows(canvas, stride, control, samples, key, factor);
        }
        _ => unreachable!("other formats have their own frame loops"),
    }
}

fn render_keyed_rows<T: Copy + Into<u16>, const N: usize>(
    canvas: &mut [u16],
    stride: usize,
    control: FrameControl,
    samples: &[T],
    key: Option<[u16; N]>,
    factor: u16,
) {
    let (width, stride) = if control.width as usize * 4 == stride {
        let pixels = samples.len() / N;
        (pixels, pixels * 4)
    } else {
        (control.width as usize, stride)
    };
    let rows = samples.as_chunks::<N>().0.chunks_exact(width);
    for (dst, src) in canvas.chunks_mut(stride).zip(rows) {
        for (dst, pixel) in dst[..width * 4].as_chunks_mut::<4>().0.iter_mut().zip(src) {
            let values = pixel.map(Into::into);
            let transparent = key == Some(values);
            if transparent && control.blend == BlendOp::Over {
                continue;
            }
            let alpha = if transparent { 0 } else { u16::MAX };
            let gray_or_red = values[0] * factor;
            *dst = if N == 1 {
                [gray_or_red, gray_or_red, gray_or_red, alpha]
            } else {
                [gray_or_red, values[1] * factor, values[2] * factor, alpha]
            };
        }
    }
}

#[inline]
fn over<const GRAY: bool>(dst: &mut [u16; 4], src: [u16; 4]) {
    if src[3] == 0 {
        return;
    }
    if src[3] == 65535 || dst[3] == 0 {
        *dst = src;
    } else if dst[3] == 65535 {
        // An opaque destination stays opaque. The exact rounded
        // numerator is at most 65535^2 + 32767, which fits in u32.
        let sa = u32::from(src[3]);
        let inv = 65535 - sa;
        for n in 0..if GRAY { 1 } else { 3 } {
            dst[n] = ((u32::from(src[n]) * sa + u32::from(dst[n]) * inv + 32767) / 65535) as u16;
        }
    } else {
        let sa = u32::from(src[3]);
        let source_weight = sa * 65535;
        let destination_weight = u32::from(dst[3]) * (65535 - sa);
        // The weights sum to at most 65535^2, leaving room to round alpha.
        let alpha = source_weight + destination_weight;
        let inverse = 1.0 / f64::from(alpha);
        let round = u64::from(alpha / 2);
        for n in 0..if GRAY { 1 } else { 3 } {
            let numerator = u64::from(src[n]) * u64::from(source_weight)
                + u64::from(dst[n]) * u64::from(destination_weight)
                + round;
            dst[n] = divide_color(numerator, alpha, inverse);
        }
        dst[3] = ((alpha + 32767) / 65535) as u16;
    }
    if GRAY {
        dst[1] = dst[0];
        dst[2] = dst[0];
    }
}

#[inline]
fn divide_color(numerator: u64, alpha: u32, inverse: f64) -> u16 {
    // The numerator is below 2^48 and converts exactly. For quotients
    // below 2^16 the reciprocal and multiply err by less than 2^-35,
    // smaller than the 2^-32 gap of a noninteger ratio to any integer.
    // Only an exact integer can truncate one too low. Check it in integers.
    let estimate = (numerator as i64 as f64 * inverse) as u32;
    let next = u64::from(estimate + 1) * u64::from(alpha);
    (estimate + u32::from(next <= numerator)) as u16
}

#[inline(never)]
fn source_rgba_rows(canvas: &mut [u16], stride: usize, width: usize, samples: &Samples) {
    let (width, stride) = if width == stride {
        let len = samples.len();
        (len, len)
    } else {
        (width, stride)
    };
    let rows = canvas.chunks_mut(stride);
    match samples {
        Samples::U8(samples) => {
            for (dst, src) in rows.zip(samples.chunks_exact(width)) {
                for (out, &value) in dst[..width].iter_mut().zip(src) {
                    *out = u16::from(value) * 257;
                }
            }
        }
        Samples::U16(samples) => {
            for (dst, src) in rows.zip(samples.chunks_exact(width)) {
                dst[..width].copy_from_slice(src);
            }
        }
    }
}

#[inline(never)]
fn render_alpha_rows<T: Copy, const N: usize>(
    canvas: &mut [u16],
    stride: usize,
    width: usize,
    samples: &[T],
    blend: BlendOp,
    convert: impl Fn([T; N]) -> [u16; 4],
) {
    let rows = samples.as_chunks::<N>().0.chunks_exact(width);
    for (dst, src) in canvas.chunks_mut(stride).zip(rows) {
        let dst = dst[..width * 4].as_chunks_mut::<4>().0;
        let pixels = dst.iter_mut().zip(src);
        if N == 2 {
            if blend == BlendOp::Source {
                for (dst, &src) in pixels {
                    *dst = convert(src);
                }
            } else {
                // All frames share the grayscale format, and disposal and
                // reset preserve equal RGB channels on the canvas.
                for (dst, &src) in pixels {
                    over::<true>(dst, convert(src));
                }
            }
        } else {
            for (dst, &src) in pixels {
                over::<false>(dst, convert(src));
            }
        }
    }
}

#[inline(never)]
fn render_indexed_rows(
    png: &Png,
    canvas: &mut [u16],
    stride: usize,
    control: FrameControl,
    samples: &[u8],
) {
    let offset = control.y_offset as usize * stride + control.x_offset as usize * 4;
    let canvas = canvas[offset..].as_chunks_mut::<4>().0;
    let stride = stride / 4;
    if samples.len() >= 256 {
        let palette = expanded_palette::<256>(png);
        render_indices(canvas, stride, control, samples, |index| {
            palette[usize::from(index)]
        });
    } else if png.image.bit_depth < 8 && samples.len() >= 32 {
        let palette = expanded_palette::<16>(png);
        render_indices(canvas, stride, control, samples, |index| {
            // Validated sub-byte frame samples fit in four bits.
            palette[usize::from(index & 15)]
        });
    } else {
        render_indices(canvas, stride, control, samples, |index| {
            palette_rgba(png, usize::from(index))
        });
    }
}

#[inline]
fn render_indices(
    canvas: &mut [[u16; 4]],
    stride: usize,
    control: FrameControl,
    samples: &[u8],
    lookup: impl Fn(u8) -> [u16; 4],
) {
    let width = control.width as usize;
    for (dst, indices) in canvas.chunks_mut(stride).zip(samples.chunks_exact(width)) {
        let pixels = dst[..width].iter_mut().zip(indices);
        if control.blend == BlendOp::Source {
            for (dst, &index) in pixels {
                *dst = lookup(index);
            }
        } else {
            for (dst, &index) in pixels {
                over::<false>(dst, lookup(index));
            }
        }
    }
}

fn expand_direct<T: Copy + Into<u16>>(
    out: &mut Vec<[u16; 4]>,
    samples: &[T],
    color: ColorType,
    transparency: Option<&Transparency>,
    mask: u16,
    factor: u16,
) {
    match color {
        ColorType::Grayscale => {
            let key = match transparency {
                Some(Transparency::Grayscale(value)) => Some(value & mask),
                _ => None,
            };
            out.extend(samples.iter().map(|&sample| {
                let value = sample.into();
                let gray = value * factor;
                let alpha = if key == Some(value) { 0 } else { u16::MAX };
                [gray, gray, gray, alpha]
            }));
        }
        ColorType::Rgb => {
            let key = match transparency {
                Some(Transparency::Rgb(rgb)) => Some(rgb.map(|value| value & mask)),
                _ => None,
            };
            out.extend(samples.as_chunks::<3>().0.iter().map(|&pixel| {
                let rgb = pixel.map(Into::into);
                let [r, g, b] = rgb.map(|value| value * factor);
                let alpha = if key == Some(rgb) { 0 } else { u16::MAX };
                [r, g, b, alpha]
            }));
        }
        ColorType::GrayscaleAlpha => {
            out.extend(samples.as_chunks::<2>().0.iter().map(|&[gray, alpha]| {
                let gray = gray.into() * factor;
                [gray, gray, gray, alpha.into() * factor]
            }));
        }
        _ => unreachable!("other colors are expanded separately"),
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
        let mask = ((1u32 << self.image.bit_depth) - 1) as u16;
        let factor = u16::MAX / mask;
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
                let palette = expanded_palette::<256>(self);
                out.extend(samples.iter().map(|&index| palette[usize::from(index)]));
            }
            (Samples::U8(samples), ColorType::Indexed) => {
                out.extend(
                    samples
                        .iter()
                        .map(|&index| palette_rgba(self, usize::from(index))),
                );
            }
            (Samples::U8(samples), color) => expand_direct(
                &mut out,
                samples,
                color,
                self.transparency.as_ref(),
                mask,
                factor,
            ),
            (Samples::U16(samples), color) => {
                expand_direct(
                    &mut out,
                    samples,
                    color,
                    self.transparency.as_ref(),
                    mask,
                    1,
                );
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

#[inline(never)]
fn clear_region(canvas: &mut [u16], control: FrameControl, stride: usize) {
    let width = control.width as usize * 4;
    let height = control.height as usize;
    let start = control.y_offset as usize * stride + control.x_offset as usize * 4;
    if width == stride {
        canvas[start..start + height * width].fill(0);
    } else {
        let end = start + (height - 1) * stride + width;
        for row in canvas[start..end].chunks_mut(stride) {
            row[..width].fill(0);
        }
    }
}

#[inline(never)]
fn save_region(canvas: &[u16], previous: &mut Vec<u16>, control: FrameControl, stride: usize) {
    let width = control.width as usize * 4;
    let start = control.y_offset as usize * stride + control.x_offset as usize * 4;
    let end = start + (control.height as usize - 1) * stride + width;
    let region = &canvas[start..end];
    previous.clear();
    if width == stride {
        previous.extend_from_slice(region);
    } else {
        for row in region.chunks(stride) {
            previous.extend_from_slice(&row[..width]);
        }
    }
}

#[inline(never)]
fn restore_region(canvas: &mut [u16], previous: &[u16], control: FrameControl, stride: usize) {
    let width = control.width as usize * 4;
    let start = control.y_offset as usize * stride + control.x_offset as usize * 4;
    let end = start + (control.height as usize - 1) * stride + width;
    let region = &mut canvas[start..end];
    if width == stride {
        region.copy_from_slice(previous);
    } else {
        for (dst, src) in region.chunks_mut(stride).zip(previous.chunks_exact(width)) {
            dst[..width].copy_from_slice(src);
        }
    }
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
        let save_previous = c.dispose == DisposeOp::Previous && self.index != 0;
        if save_previous {
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
            match control.dispose {
                DisposeOp::None => (),
                DisposeOp::Previous if self.index > 1 => {
                    if self.previous.len() == self.canvas.len() {
                        core::mem::swap(&mut self.canvas, &mut self.previous);
                    } else {
                        restore_region(&mut self.canvas, &self.previous, control, stride);
                    }
                }
                _ => clear_region(&mut self.canvas, control, stride),
            }
        }
        if save_previous {
            let width = c.width as usize * 4;
            let height = c.height as usize;
            if c.blend == BlendOp::Source && width * height == self.canvas.len() {
                // SOURCE overwrites the whole canvas, so reuse the scratch buffer for it.
                self.previous.resize(self.canvas.len(), 0);
                core::mem::swap(&mut self.canvas, &mut self.previous);
            } else {
                save_region(&self.canvas, &mut self.previous, c, stride);
            }
        } else {
            self.previous.clear();
        }
        if c.blend == BlendOp::Source && image.color_type == ColorType::Rgba {
            let offset = c.y_offset as usize * stride + c.x_offset as usize * 4;
            source_rgba_rows(
                &mut self.canvas[offset..],
                stride,
                c.width as usize * 4,
                &image.samples,
            );
        } else if image.color_type == ColorType::Rgba {
            let offset = c.y_offset as usize * stride + c.x_offset as usize * 4;
            let canvas = &mut self.canvas[offset..];
            let width = c.width as usize;
            match &image.samples {
                Samples::U8(samples) => render_alpha_rows(
                    canvas,
                    stride,
                    width,
                    samples,
                    BlendOp::Over,
                    |pixel: [u8; 4]| pixel.map(|channel| u16::from(channel) * 257),
                ),
                Samples::U16(samples) => {
                    render_alpha_rows(canvas, stride, width, samples, BlendOp::Over, |pixel| pixel);
                }
            }
        } else if image.color_type == ColorType::GrayscaleAlpha {
            let offset = c.y_offset as usize * stride + c.x_offset as usize * 4;
            let canvas = &mut self.canvas[offset..];
            let width = c.width as usize;
            match &image.samples {
                Samples::U8(samples) => {
                    render_alpha_rows(canvas, stride, width, samples, c.blend, |[g, a]| {
                        let g = u16::from(g) * 257;
                        [g, g, g, u16::from(a) * 257]
                    });
                }
                Samples::U16(samples) => {
                    render_alpha_rows(canvas, stride, width, samples, c.blend, |[g, a]| {
                        [g, g, g, a]
                    });
                }
            }
        } else if let (ColorType::Indexed, Samples::U8(samples)) =
            (image.color_type, &image.samples)
        {
            render_indexed_rows(self.png, &mut self.canvas, stride, c, samples);
        } else {
            let offset = c.y_offset as usize * stride + c.x_offset as usize * 4;
            let canvas = &mut self.canvas[offset..];
            match &image.samples {
                Samples::U8(samples) => {
                    render_color_rows(self.png, image, canvas, stride, c, samples)
                }
                Samples::U16(samples) => {
                    render_color_rows(self.png, image, canvas, stride, c, samples)
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

#[cfg(test)]
mod tests {
    use super::divide_color;

    #[test]
    fn reciprocal_keeps_exact_color_division_at_integer_boundaries() {
        let mut state = 1u32;
        for alpha in [1, 65535, 65536, 65535 * 65535]
            .into_iter()
            .chain((1..=65535).map(|sa| {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                sa * 65535 + (state >> 16) * (65535 - sa)
            }))
        {
            let inverse = 1.0 / f64::from(alpha);
            for quotient in [0u64, 1, 127, 255, 32768, 65534, 65535] {
                let exact = quotient * u64::from(alpha);
                for numerator in [exact.saturating_sub(1), exact, exact + 1] {
                    let expected = numerator / u64::from(alpha);
                    if expected <= u64::from(u16::MAX) {
                        assert_eq!(
                            u64::from(divide_color(numerator, alpha, inverse)),
                            expected,
                            "{numerator} / {alpha}"
                        );
                    }
                }
            }
        }
    }
}
