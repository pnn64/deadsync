use crate::transform::idct;
use crate::{CoefficientImage, ComponentSpec, Error, FrameHeader, Limits, MetadataSegment, Result};
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CmykPolarity {
    #[default]
    Conventional,
    AdobeInverted,
}

/// Full-resolution encoded CMYK values. Precision stays native (including
/// 12-bit samples); polarity describes whether components encode ink or its
/// complement. This is suitable for a caller's ICC color-management system.
#[derive(Clone, Debug)]
pub struct CmykImage {
    pub width: u16,
    pub height: u16,
    pub precision: u8,
    pub polarity: CmykPolarity,
    pub samples: Vec<u16>,
    limits: Limits,
    memory_bytes: usize,
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
    pub cmyk_polarity: CmykPolarity,
    pub components: Vec<ComponentPlane>,
    pub(crate) limits: Limits,
    pub(crate) memory_bytes: usize,
}

/// How component samples are expanded to the full image grid. Both policies use
/// JFIF's centered sample positions, including fractional sampling ratios.
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
        infer_color(&self.frame, &self.metadata)
    }

    pub fn reconstruct(&self) -> Result<PlanarImage> {
        self.validate_grid()?;
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
            cmyk_polarity: self.cmyk_polarity(),
            components: planes,
            limits: self.limits,
            memory_bytes,
        })
    }

    pub fn cmyk_polarity(&self) -> CmykPolarity {
        if self.components.len() == 4 {
            infer_cmyk_polarity(&self.metadata)
        } else {
            CmykPolarity::Conventional
        }
    }

    fn validate_grid(&self) -> Result<()> {
        let f = &self.frame;
        if f.width == 0
            || f.height == 0
            || !matches!(f.precision, 8 | 12)
            || self.components.is_empty()
            || self.components.len() != f.components.len()
        {
            return Err(Error::Invalid("reconstruction frame"));
        }
        if self.components.iter().any(|c| {
            !(1..=4).contains(&c.spec.horizontal_sampling)
                || !(1..=4).contains(&c.spec.vertical_sampling)
        }) {
            return Err(Error::Invalid("reconstruction sampling factors"));
        }
        let max_h = self
            .components
            .iter()
            .map(|c| c.spec.horizontal_sampling as usize)
            .max()
            .unwrap();
        let max_v = self
            .components
            .iter()
            .map(|c| c.spec.vertical_sampling as usize)
            .max()
            .unwrap();
        for (i, c) in self.components.iter().enumerate() {
            if c.spec != f.components[i]
                || c.width
                    != (f.width as usize * c.spec.horizontal_sampling as usize).div_ceil(max_h)
                || c.height
                    != (f.height as usize * c.spec.vertical_sampling as usize).div_ceil(max_v)
                || c.blocks_wide < c.width.div_ceil(8)
                || c.blocks_high < c.height.div_ceil(8)
                || c.blocks_wide.checked_mul(c.blocks_high) != Some(c.blocks.len())
                || c.quantization.contains(&0)
            {
                return Err(Error::Invalid("reconstruction coefficient grid"));
            }
        }
        Ok(())
    }
}

impl PlanarImage {
    /// Convert to shared gray/RGB storage. CMYK uses an uncalibrated subtractive
    /// approximation with explicit ink polarity; use `to_cmyk` and the original
    /// ICC profile for color-managed conversion. Precision other than 8 bits
    /// scales to shared 16-bit storage; native planes retain original integers.
    pub fn to_image(&self, upsampling: Upsampling) -> Result<Image> {
        let color_type = match self.color_space {
            ColorSpace::Grayscale => ColorType::Grayscale,
            ColorSpace::Rgb | ColorSpace::YCbCr | ColorSpace::Cmyk | ColorSpace::Ycck => {
                ColorType::Rgb
            }
            _ => {
                return Err(Error::Unsupported(
                    "shared conversion of unknown color space",
                ));
            }
        };
        let (max_h, max_v) = self.validate_layout()?;
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
        for y in 0..self.height as usize {
            for x in 0..self.width as usize {
                let mut values = [0i32; 4];
                for (c, v) in self.components.iter().zip(&mut values) {
                    *v = sample(c, x, y, max_h, max_v, upsampling);
                }
                if self.color_space == ColorSpace::YCbCr {
                    let rgb = ycbcr_to_rgb(values[..3].try_into().unwrap(), self.precision);
                    values[..3].copy_from_slice(&rgb);
                } else if matches!(self.color_space, ColorSpace::Cmyk | ColorSpace::Ycck) {
                    let cmyk = if self.color_space == ColorSpace::Ycck {
                        ycck_to_cmyk(values, self.precision)
                    } else {
                        values
                    };
                    values[..3].copy_from_slice(&cmyk_to_rgb(
                        cmyk,
                        self.cmyk_polarity,
                        self.precision,
                    ));
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

    pub fn to_cmyk(&self, upsampling: Upsampling) -> Result<CmykImage> {
        if !matches!(self.color_space, ColorSpace::Cmyk | ColorSpace::Ycck) {
            return Err(Error::Unsupported("CMYK output of non-CMYK image"));
        }
        let (max_h, max_v) = self.validate_layout()?;
        let count = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or(Error::LimitExceeded("CMYK sample storage"))?;
        let memory_bytes = count
            .checked_mul(2)
            .and_then(|n| self.memory_bytes.checked_add(n))
            .ok_or(Error::LimitExceeded("CMYK sample storage"))?;
        if memory_bytes > self.limits.max_memory_bytes {
            return Err(Error::LimitExceeded("CMYK sample storage"));
        }
        let mut samples = Vec::new();
        samples
            .try_reserve_exact(count)
            .map_err(|_| Error::Allocation)?;
        for y in 0..self.height as usize {
            for x in 0..self.width as usize {
                let mut values = [0i32; 4];
                for (c, v) in self.components.iter().zip(&mut values) {
                    *v = sample(c, x, y, max_h, max_v, upsampling);
                }
                if self.color_space == ColorSpace::Ycck {
                    values = ycck_to_cmyk(values, self.precision);
                }
                samples.extend(values.map(|v| v as u16));
            }
        }
        Ok(CmykImage {
            width: self.width,
            height: self.height,
            precision: self.precision,
            polarity: self.cmyk_polarity,
            samples,
            limits: self.limits,
            memory_bytes,
        })
    }

    fn validate_layout(&self) -> Result<(usize, usize)> {
        let count = match self.color_space {
            ColorSpace::Grayscale => 1,
            ColorSpace::Rgb | ColorSpace::YCbCr => 3,
            ColorSpace::Cmyk | ColorSpace::Ycck => 4,
            _ => return Err(Error::Unsupported("unknown planar color space")),
        };
        if self.width == 0
            || self.height == 0
            || !(2..=16).contains(&self.precision)
            || self.components.len() != count
            || (self.color_space == ColorSpace::Ycck
                && self.cmyk_polarity != CmykPolarity::AdobeInverted)
        {
            return Err(Error::Invalid(
                "planar output dimensions, precision or components",
            ));
        }
        if self.components.iter().any(|c| {
            !(1..=4).contains(&c.spec.horizontal_sampling)
                || !(1..=4).contains(&c.spec.vertical_sampling)
        }) {
            return Err(Error::Invalid("planar output sampling factors"));
        }
        let max_h = self
            .components
            .iter()
            .map(|c| c.spec.horizontal_sampling as usize)
            .max()
            .unwrap();
        let max_v = self
            .components
            .iter()
            .map(|c| c.spec.vertical_sampling as usize)
            .max()
            .unwrap();
        let max = ((1u32 << self.precision) - 1) as u16;
        for c in &self.components {
            if c.width
                != (self.width as usize * c.spec.horizontal_sampling as usize).div_ceil(max_h)
                || c.height
                    != (self.height as usize * c.spec.vertical_sampling as usize).div_ceil(max_v)
                || c.width.checked_mul(c.height) != Some(c.samples.len())
                || c.samples.iter().any(|&v| v > max)
            {
                return Err(Error::Invalid("planar output sample layout or range"));
            }
        }
        Ok((max_h, max_v))
    }
}

impl CmykImage {
    /// Encode using this raster's explicit source polarity. Select YCCK with
    /// `EncodeOptions::cmyk_to_ycck`; no converted source buffer is allocated.
    pub fn encode(&self, options: crate::EncodeOptions) -> Result<Vec<u8>> {
        self.encode_with_metadata(options, crate::MetadataOptions::default())
    }
    pub fn encode_with_metadata(
        &self,
        mut options: crate::EncodeOptions,
        metadata: crate::MetadataOptions<'_>,
    ) -> Result<Vec<u8>> {
        self.validate()?;
        options.cmyk_polarity = self.polarity;
        crate::encode_samples_with_metadata(
            &crate::EncodeInput {
                width: self.width,
                height: self.height,
                precision: self.precision,
                color_space: ColorSpace::Cmyk,
                samples: crate::SampleData::U16(&self.samples),
            },
            options,
            metadata,
        )
    }
    pub fn convert_polarity(&mut self, polarity: CmykPolarity) -> Result<()> {
        let (_, max) = self.validate()?;
        if polarity != self.polarity {
            for v in &mut self.samples {
                *v = max - *v;
            }
            self.polarity = polarity;
        }
        Ok(())
    }
    /// Uncalibrated subtractive approximation. CMYK/ICC users can consume the
    /// original samples and profile instead of this RGB convenience output.
    pub fn to_image(&self) -> Result<Image> {
        let (pixels, max) = self.validate()?;
        let count = pixels
            .checked_mul(3)
            .ok_or(Error::LimitExceeded("CMYK RGB output"))?;
        let bytes = count
            .checked_mul(if self.precision == 8 { 1 } else { 2 })
            .ok_or(Error::LimitExceeded("CMYK RGB output"))?;
        if self
            .memory_bytes
            .checked_add(bytes)
            .is_none_or(|n| n > self.limits.max_memory_bytes)
        {
            return Err(Error::LimitExceeded("CMYK RGB output"));
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
        for values in self.samples.as_chunks::<4>().0 {
            for value in cmyk_to_rgb(values.map(i32::from), self.polarity, self.precision) {
                if self.precision == 8 {
                    u8_samples.push(value as u8);
                } else {
                    u16_samples.push(((value as u32 * 65535 + max as u32 / 2) / max as u32) as u16);
                }
            }
        }
        Ok(Image {
            width: self.width as u32,
            height: self.height as u32,
            color_type: ColorType::Rgb,
            bit_depth: if self.precision == 8 { 8 } else { 16 },
            samples: if self.precision == 8 {
                Samples::U8(u8_samples)
            } else {
                Samples::U16(u16_samples)
            },
        })
    }

    fn validate(&self) -> Result<(usize, u16)> {
        let pixels = self.width as usize * self.height as usize;
        if self.width == 0
            || self.height == 0
            || !(2..=16).contains(&self.precision)
            || pixels.checked_mul(4) != Some(self.samples.len())
        {
            return Err(Error::Invalid("CMYK image layout"));
        }
        let max = ((1u32 << self.precision) - 1) as u16;
        if self.samples.iter().any(|&v| v > max) {
            return Err(Error::Invalid("CMYK sample range"));
        }
        Ok((pixels, max))
    }
}

fn ycbcr_to_rgb(values: [i32; 3], precision: u8) -> [i32; 3] {
    let [y, cb, cr] = values.map(i64::from);
    let center = 1i64 << (precision - 1);
    let max = (1i64 << precision) - 1;
    let (cb, cr) = (cb - center, cr - center);
    [
        y + ((91881 * cr + 32768) >> 16),
        y + ((-22554 * cb - 46802 * cr + 32768) >> 16),
        y + ((116130 * cb + 32768) >> 16),
    ]
    .map(|v| v.clamp(0, max) as i32)
}

fn ycck_to_cmyk(values: [i32; 4], precision: u8) -> [i32; 4] {
    let max = (1i32 << precision) - 1;
    let rgb = ycbcr_to_rgb(values[..3].try_into().unwrap(), precision);
    [max - rgb[0], max - rgb[1], max - rgb[2], values[3]]
}
fn cmyk_to_rgb(values: [i32; 4], polarity: CmykPolarity, precision: u8) -> [i32; 3] {
    let max = (1i32 << precision) - 1;
    let inverse = values.map(|v| {
        if polarity == CmykPolarity::AdobeInverted {
            v
        } else {
            max - v
        }
    });
    [0, 1, 2].map(|i| {
        ((i64::from(inverse[i]) * i64::from(inverse[3]) + i64::from(max / 2)) / i64::from(max))
            as i32
    })
}

pub(crate) fn infer_color(frame: &FrameHeader, metadata: &[MetadataSegment<'_>]) -> ColorSpace {
    infer_components(&frame.components, metadata)
}
pub(crate) fn infer_components(
    components: &[ComponentSpec],
    metadata: &[MetadataSegment<'_>],
) -> ColorSpace {
    let adobe = metadata
        .iter()
        .rev()
        .filter(|s| s.preceding_scans == 0)
        .find_map(|s| s.adobe().ok().flatten().map(|a| a.transform));
    let jfif = metadata.iter().any(|s| {
        s.preceding_scans == 0
            && s.marker == 0xe0
            && s.data.len() >= 14
            && s.data.starts_with(b"JFIF\0")
    });
    match (components.len(), adobe) {
        (1, _) => ColorSpace::Grayscale,
        (3, _) if jfif => ColorSpace::YCbCr,
        (3, Some(0)) => ColorSpace::Rgb,
        (3, Some(1)) => ColorSpace::YCbCr,
        (3, None) if components.iter().map(|c| c.id).eq(*b"RGB") => ColorSpace::Rgb,
        (3, None) => ColorSpace::YCbCr,
        (4, Some(2)) => ColorSpace::Ycck,
        (4, None | Some(0)) => ColorSpace::Cmyk,
        _ => ColorSpace::Unknown,
    }
}

pub(crate) fn infer_cmyk_polarity(metadata: &[MetadataSegment<'_>]) -> CmykPolarity {
    if metadata.iter().filter(|s| s.preceding_scans == 0).any(|s| {
        s.adobe()
            .ok()
            .flatten()
            .is_some_and(|a| a.transform == 0 || a.transform == 2)
    }) {
        CmykPolarity::AdobeInverted
    } else {
        CmykPolarity::Conventional
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
        Upsampling::Nearest => get(
            ((2 * x + 1) * h / (2 * max_h)) as i64,
            ((2 * y + 1) * v / (2 * max_v)) as i64,
        ) as i32,
        Upsampling::Linear => {
            let sx = ((2 * x + 1) * h) as i64 - max_h as i64;
            let sy = ((2 * y + 1) * v) as i64 - max_v as i64;
            let dx = (2 * max_h) as i64;
            let dy = (2 * max_v) as i64;
            let ix = sx.div_euclid(dx);
            let fx = sx.rem_euclid(dx);
            let iy = sy.div_euclid(dy);
            let fy = sy.rem_euclid(dy);
            let top = get(ix, iy) * (dx - fx) + get(ix + 1, iy) * fx;
            let bottom = get(ix, iy + 1) * (dx - fx) + get(ix + 1, iy + 1) * fx;
            ((top * (dy - fy) + bottom * fy + dx * dy / 2) / (dx * dy)) as i32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn subtractive_extrema_and_polarity_conversion() {
        for precision in 2..=16 {
            let max = (1i32 << precision) - 1;
            assert_eq!(
                cmyk_to_rgb([0, 0, 0, 0], CmykPolarity::Conventional, precision),
                [max; 3]
            );
            assert_eq!(
                cmyk_to_rgb([0, 0, 0, max], CmykPolarity::Conventional, precision),
                [0; 3]
            );
            assert_eq!(
                cmyk_to_rgb([max, 0, 0, 0], CmykPolarity::Conventional, precision),
                [0, max, max]
            );
            assert_eq!(
                ycck_to_cmyk(
                    [0, 1 << (precision - 1), 1 << (precision - 1), max],
                    precision
                ),
                [max; 4]
            );
            let center = 1 << (precision - 1);
            assert_eq!(ycbcr_to_rgb([max, center, center], precision), [max; 3]);
            assert_eq!(ycbcr_to_rgb([0, center, center], precision), [0; 3]);
            assert_eq!(
                cmyk_to_rgb([max; 4], CmykPolarity::AdobeInverted, precision),
                [max; 3]
            );
        }
        let mut image = CmykImage {
            width: 1,
            height: 1,
            precision: 8,
            polarity: CmykPolarity::Conventional,
            samples: vec![0, 128, 255, 64],
            limits: Limits::default(),
            memory_bytes: 8,
        };
        assert_eq!(
            image.to_image().unwrap().samples,
            Samples::U8(vec![191, 95, 0])
        );
        image.convert_polarity(CmykPolarity::AdobeInverted).unwrap();
        assert_eq!(image.samples, [255, 127, 0, 191]);
        assert_eq!(
            image.to_image().unwrap().samples,
            Samples::U8(vec![191, 95, 0])
        );
        image.convert_polarity(CmykPolarity::Conventional).unwrap();
        assert_eq!(image.samples, [0, 128, 255, 64]);
    }

    #[test]
    fn fractional_siting_retains_twelve_bit_interpolation_precision() {
        let c = ComponentPlane {
            spec: ComponentSpec {
                id: 1,
                horizontal_sampling: 2,
                vertical_sampling: 1,
                quantization_table: 0,
            },
            width: 5,
            height: 1,
            samples: vec![0, 4095, 0, 4095, 0],
        };
        assert_eq!(sample(&c, 2, 0, 3, 1, Upsampling::Linear), 3413); // 4095 * 5/6.
        assert_eq!(sample(&c, 1, 0, 3, 1, Upsampling::Nearest), 4095);
        for x in 0..7 {
            let pos = (x as f64 + 0.5) * 2.0 / 3.0 - 0.5;
            let left = pos.floor() as i32;
            let weight = pos - pos.floor();
            let get = |i: i32| c.samples[i.clamp(0, 4) as usize] as f64;
            let expected =
                (get(left) * (1.0 - weight) + get(left + 1) * weight + 1e-9).round() as i32;
            assert_eq!(sample(&c, x, 0, 3, 1, Upsampling::Linear), expected);
        }
    }
}
