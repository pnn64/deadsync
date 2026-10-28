use crate::transform::idct;
use crate::{CoefficientImage, ComponentSpec, Error, Limits, Result};
use deadlib_image::{ColorType, Image, Samples};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorSpace {
    Grayscale,
    YCbCr,
    Rgb,
    Cmyk,
    Ycck,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct ComponentPlane {
    pub spec: ComponentSpec,
    pub width: usize,
    pub height: usize,
    /// Native precision samples, with no padding at row boundaries.
    pub samples: Vec<u16>,
}

#[derive(Clone, Debug)]
pub struct PlanarImage {
    pub width: u16,
    pub height: u16,
    pub precision: u8,
    pub color_space: ColorSpace,
    pub components: Vec<ComponentPlane>,
    limits: Limits,
    memory_bytes: usize,
}

/// How component samples are expanded to the full image grid. Linear uses the
/// centered chroma sample positions from JFIF 1.02, including fractional ratios.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Upsampling {
    Nearest,
    #[default]
    Linear,
}

impl CoefficientImage<'_> {
    /// Interpret only well-known JFIF, Adobe and RGB component conventions.
    /// An untagged three-component JPEG conventionally uses YCbCr.
    pub fn color_space(&self) -> ColorSpace {
        let adobe = self.metadata.iter().rev().find_map(|s| {
            if s.marker == 0xee && s.data.starts_with(b"Adobe") && s.data.len() == 12 {
                Some(s.data[11])
            } else {
                None
            }
        });
        let jfif = self
            .metadata
            .iter()
            .any(|s| s.marker == 0xe0 && s.data.starts_with(b"JFIF\0"));
        match (self.components.len(), adobe) {
            (1, _) => ColorSpace::Grayscale,
            (3, Some(0)) => ColorSpace::Rgb,
            (3, Some(1)) => ColorSpace::YCbCr,
            (3, None) if !jfif && self.components.iter().map(|c| c.spec.id).eq(*b"RGB") => {
                ColorSpace::Rgb
            }
            (3, None) => ColorSpace::YCbCr,
            (4, Some(2)) => ColorSpace::Ycck,
            (4, None | Some(0)) => ColorSpace::Cmyk,
            _ => ColorSpace::Unknown,
        }
    }

    pub fn reconstruct(&self) -> Result<PlanarImage> {
        let sample_bytes = self
            .components
            .iter()
            .try_fold(0usize, |total, c| {
                c.width
                    .checked_mul(c.height)
                    .and_then(|n| n.checked_mul(2))
                    .and_then(|n| total.checked_add(n))
            })
            .ok_or(Error::LimitExceeded("sample storage"))?;
        let memory_bytes = self
            .memory_bytes
            .checked_add(sample_bytes)
            .ok_or(Error::LimitExceeded("sample storage"))?;
        if memory_bytes > self.limits.max_memory_bytes {
            return Err(Error::LimitExceeded("sample storage"));
        }
        let mut planes = Vec::new();
        planes
            .try_reserve_exact(self.components.len())
            .map_err(|_| Error::Allocation)?;
        for c in &self.components {
            let mut samples = Vec::new();
            samples
                .try_reserve_exact(c.width * c.height)
                .map_err(|_| Error::Allocation)?;
            samples.resize(c.width * c.height, 0);
            for by in 0..c.height.div_ceil(8) {
                for bx in 0..c.width.div_ceil(8) {
                    let pixels = idct(
                        &c.blocks[by * c.blocks_wide + bx],
                        &c.quantization,
                        self.frame.precision,
                    );
                    let cols = (c.width - bx * 8).min(8);
                    for y in 0..(c.height - by * 8).min(8) {
                        let dst = (by * 8 + y) * c.width + bx * 8;
                        samples[dst..dst + cols].copy_from_slice(&pixels[y * 8..y * 8 + cols]);
                    }
                }
            }
            planes.push(ComponentPlane {
                spec: c.spec,
                width: c.width,
                height: c.height,
                samples,
            });
        }
        Ok(PlanarImage {
            width: self.frame.width,
            height: self.frame.height,
            precision: self.frame.precision,
            color_space: self.color_space(),
            components: planes,
            limits: self.limits,
            memory_bytes,
        })
    }
}

impl PlanarImage {
    /// Convert grayscale or RGB-family planes to the shared image model.
    /// 12-bit samples are explicitly scaled to 16 bits. CMYK/YCCK stay native
    /// until a color-management policy is supplied; ICC profiles are not applied.
    pub fn to_image(&self, upsampling: Upsampling) -> Result<Image> {
        let color_type = match self.color_space {
            ColorSpace::Grayscale => ColorType::Grayscale,
            ColorSpace::Rgb | ColorSpace::YCbCr => ColorType::Rgb,
            _ => {
                return Err(Error::Unsupported(
                    "shared conversion of CMYK/YCCK or unknown color space",
                ));
            }
        };
        let channels = color_type.channels();
        let count = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|n| n.checked_mul(channels))
            .ok_or(Error::LimitExceeded("output storage"))?;
        let bytes = count
            .checked_mul(if self.precision == 8 { 1 } else { 2 })
            .ok_or(Error::LimitExceeded("output storage"))?;
        if self
            .memory_bytes
            .checked_add(bytes)
            .is_none_or(|n| n > self.limits.max_memory_bytes)
        {
            return Err(Error::LimitExceeded("output storage"));
        }
        let mut u8_samples = Vec::new();
        let mut u16_samples = Vec::new();
        if self.precision == 8 {
            u8_samples
                .try_reserve_exact(count)
                .map_err(|_| Error::Allocation)?;
        } else {
            u16_samples
                .try_reserve_exact(count)
                .map_err(|_| Error::Allocation)?;
        }
        let max = (1i32 << self.precision) - 1;
        let center = 1i32 << (self.precision - 1);
        let max_h = self
            .components
            .iter()
            .map(|c| c.spec.horizontal_sampling as usize)
            .max()
            .ok_or(Error::Invalid("empty planes"))?;
        let max_v = self
            .components
            .iter()
            .map(|c| c.spec.vertical_sampling as usize)
            .max()
            .unwrap();
        for y in 0..self.height as usize {
            for x in 0..self.width as usize {
                let mut values = [0i32; 3];
                for (c, v) in self.components.iter().zip(&mut values) {
                    *v = sample(c, x, y, max_h, max_v, upsampling);
                }
                if self.color_space == ColorSpace::YCbCr {
                    let [l, cb, cr] = values;
                    let cb = cb - center;
                    let cr = cr - center;
                    values = [
                        l + ((91881 * cr + 32768) >> 16),
                        l - ((22554 * cb + 46802 * cr + 32768) >> 16),
                        l + ((116130 * cb + 32768) >> 16),
                    ];
                }
                for &v in &values[..channels] {
                    let v = v.clamp(0, max) as u32;
                    if self.precision == 8 {
                        u8_samples.push(v as u8);
                    } else {
                        u16_samples.push(((v * 65535 + max as u32 / 2) / max as u32) as u16);
                    }
                }
            }
        }
        Ok(Image {
            width: self.width as u32,
            height: self.height as u32,
            color_type,
            bit_depth: if self.precision == 8 { 8 } else { 16 },
            samples: if self.precision == 8 {
                Samples::U8(u8_samples)
            } else {
                Samples::U16(u16_samples)
            },
        })
    }
}

fn sample(
    c: &ComponentPlane,
    x: usize,
    y: usize,
    max_h: usize,
    max_v: usize,
    method: Upsampling,
) -> i32 {
    let h = c.spec.horizontal_sampling as usize;
    let v = c.spec.vertical_sampling as usize;
    if h == max_h && v == max_v {
        return c.samples[y * c.width + x] as i32;
    }
    let get = |sx: i64, sy: i64| {
        c.samples[sy.clamp(0, c.height as i64 - 1) as usize * c.width
            + sx.clamp(0, c.width as i64 - 1) as usize] as i64
    };
    match method {
        Upsampling::Nearest => get((x * h / max_h) as i64, (y * v / max_v) as i64) as i32,
        Upsampling::Linear => {
            let sx = (((2 * x + 1) * h * 256) / (2 * max_h)) as i64 - 128;
            let sy = (((2 * y + 1) * v * 256) / (2 * max_v)) as i64 - 128;
            let ix = sx.div_euclid(256);
            let fx = sx.rem_euclid(256);
            let iy = sy.div_euclid(256);
            let fy = sy.rem_euclid(256);
            let top = get(ix, iy) * (256 - fx) + get(ix + 1, iy) * fx;
            let bottom = get(ix, iy + 1) * (256 - fx) + get(ix + 1, iy + 1) * fx;
            ((top * (256 - fy) + bottom * fy + 32768) >> 16) as i32
        }
    }
}
