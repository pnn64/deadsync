use crate::decode::allocate_components;
use crate::transform::fdct;
use crate::{
    CmykPolarity, CodingOptions, CoefficientImage, ColorSpace, ComponentSpec, Error, FrameHeader,
    Limits, MetadataOptions, MetadataSegment, Process, Result, encode_coefficients_with_metadata,
};
use deadlib_image::{ColorType, Image, Samples};

/// Interleaved source samples at the declared original precision.
#[derive(Clone, Copy, Debug)]
pub enum SampleData<'a> {
    U8(&'a [u8]),
    U16(&'a [u16]),
}

#[derive(Clone, Copy, Debug)]
pub struct EncodeInput<'a> {
    pub width: u16,
    pub height: u16,
    /// Source precision, 1..16. DCT output scales to its selected precision;
    /// lossless output requires 2..16 and preserves native integers.
    pub precision: u8,
    pub color_space: ColorSpace,
    pub samples: SampleData<'a>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Subsampling {
    S444,
    S422,
    #[default]
    S420,
    S440,
    S411,
    S410,
    /// Per-component (H,V) factors in frame order. Each factor is 1..4;
    /// Each interleaved scan must contain at most ten blocks per MCU.
    Custom([(u8, u8); 4]),
}

impl Subsampling {
    pub(crate) fn factors(self) -> [(u8, u8); 4] {
        let first = match self {
            Self::S444 => (1, 1),
            Self::S422 => (2, 1),
            Self::S420 => (2, 2),
            Self::S440 => (1, 2),
            Self::S411 => (4, 1),
            Self::S410 => (4, 2),
            Self::Custom(v) => return v,
        };
        [first, (1, 1), (1, 1), (1, 1)]
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EncodeOptions<'a> {
    pub quality: u8,
    /// JPEG DCT output precision: 8 or 12 bits.
    pub precision: u8,
    pub subsampling: Subsampling,
    /// RGB input uses the JFIF YCbCr transform by default; false retains RGB.
    pub rgb_to_ycbcr: bool,
    /// Meaning of source CMYK samples. Adobe-complemented values preserve the
    /// original raw-input convention; conventional ink values are normalized
    /// during sampling, without allocating a converted source image.
    pub cmyk_polarity: CmykPolarity,
    /// Transform source CMYK to YCCK; false writes complemented CMYK directly.
    pub cmyk_to_ycck: bool,
    /// Clamp scaled Annex K quantizers to 255; false permits extended tables.
    pub baseline_quantization: bool,
    pub coding: CodingOptions<'a>,
    pub limits: Limits,
}

impl Default for EncodeOptions<'_> {
    fn default() -> Self {
        Self {
            quality: 75,
            precision: 8,
            subsampling: Subsampling::S420,
            rgb_to_ycbcr: true,
            cmyk_polarity: CmykPolarity::AdobeInverted,
            cmyk_to_ycck: false,
            baseline_quantization: true,
            coding: CodingOptions::default(),
            limits: Limits::default(),
        }
    }
}

// T.81 Tables K.1/K.2 in natural frequency order. Quality scaling follows the
// conventional 50-based scale, with an explicit baseline quantizer cap.
const LUMA: [u16; 64] = [
    16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56,
    14, 17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113,
    92, 49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103, 99,
];
const CHROMA: [u16; 64] = [
    17, 18, 24, 47, 99, 99, 99, 99, 18, 21, 26, 66, 99, 99, 99, 99, 24, 26, 56, 99, 99, 99, 99, 99,
    47, 66, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
];
const JFIF: &[u8] = b"JFIF\0\x01\x02\0\0\x01\0\x01\0\0";
const ADOBE0: &[u8] = b"Adobe\0\x64\0\0\0\0\0";
const ADOBE2: &[u8] = b"Adobe\0\x64\0\0\0\0\x02";

/// Encode the shared grayscale or RGB image model. Alpha and indexed input
/// require an explicit conversion by the caller. 16-bit sources are scaled to
/// the selected 8/12-bit JPEG precision; no temporary widened source is made.
pub fn encode(image: &Image, options: EncodeOptions) -> Result<Vec<u8>> {
    encode_with_metadata(image, options, MetadataOptions::default())
}

pub fn encode_with_metadata(
    image: &Image,
    options: EncodeOptions,
    metadata: MetadataOptions<'_>,
) -> Result<Vec<u8>> {
    image
        .validate()
        .map_err(|_| Error::Invalid("encoder shared image"))?;
    let color_space = match image.color_type {
        ColorType::Grayscale => ColorSpace::Grayscale,
        ColorType::Rgb => ColorSpace::Rgb,
        _ => return Err(Error::Unsupported("encoder shared image color type")),
    };
    let input = EncodeInput {
        width: u16::try_from(image.width)
            .map_err(|_| Error::Invalid("encoder width exceeds 65535"))?,
        height: u16::try_from(image.height)
            .map_err(|_| Error::Invalid("encoder height exceeds 65535"))?,
        precision: image.bit_depth,
        color_space,
        samples: match &image.samples {
            Samples::U8(v) => SampleData::U8(v),
            Samples::U16(v) => SampleData::U16(v),
        },
    };
    encode_samples_with_metadata(&input, options, metadata)
}

/// Native interleaved sample encoding. CMYK polarity and CMYK-to-YCCK are
/// explicit options; YCCK input already contains encoded component values.
pub fn encode_samples(input: &EncodeInput<'_>, options: EncodeOptions) -> Result<Vec<u8>> {
    encode_samples_with_metadata(input, options, MetadataOptions::default())
}

pub fn encode_samples_with_metadata(
    input: &EncodeInput<'_>,
    options: EncodeOptions,
    metadata: MetadataOptions<'_>,
) -> Result<Vec<u8>> {
    let coefficients = quantize(input, options)?;
    encode_coefficients_with_metadata(&coefficients, options.coding, metadata)
}

/// Produce native quantized coefficients without emitting a JPEG. This permits
/// coefficient inspection and metadata attachment before entropy coding.
pub fn quantize(
    input: &EncodeInput<'_>,
    options: EncodeOptions,
) -> Result<CoefficientImage<'static>> {
    if !matches!(options.precision, 8 | 12) || !(1..=100).contains(&options.quality) {
        return Err(Error::Invalid("encoder dimensions, quality or precision"));
    }
    let channels = validate_raster(input, options.precision, options.limits)?;
    let output_space = if input.color_space == ColorSpace::Rgb && options.rgb_to_ycbcr {
        ColorSpace::YCbCr
    } else if input.color_space == ColorSpace::Cmyk && options.cmyk_to_ycck {
        ColorSpace::Ycck
    } else {
        input.color_space
    };
    let mut factors = if channels == 1 {
        [(1, 1); 4]
    } else {
        options.subsampling.factors()
    };
    if output_space == ColorSpace::Ycck && !matches!(options.subsampling, Subsampling::Custom(_)) {
        factors[3] = factors[0]; // Y and K retain matching spatial resolution.
    }
    let mut blocks_per_mcu = 0;
    for &(h, v) in &factors[..channels] {
        if !(1..=4).contains(&h) || !(1..=4).contains(&v) {
            return Err(Error::Invalid("encoder sampling factors"));
        }
        blocks_per_mcu += h as usize * v as usize;
    }
    if channels > 1 && blocks_per_mcu > 10 && options.coding.scan_script.is_empty() {
        return Err(Error::Invalid("encoder MCU exceeds ten blocks"));
    }
    let ids = match output_space {
        ColorSpace::Rgb => *b"RGB\0",
        ColorSpace::Cmyk => *b"CMYK",
        _ => [1, 2, 3, 4],
    };
    let mut specs = Vec::new();
    specs
        .try_reserve_exact(channels)
        .map_err(|_| Error::Allocation)?;
    for i in 0..channels {
        specs.push(ComponentSpec {
            id: ids[i],
            horizontal_sampling: factors[i].0,
            vertical_sampling: factors[i].1,
            quantization_table: u8::from(
                i > 0 && i < 3 && matches!(output_space, ColorSpace::YCbCr | ColorSpace::Ycck),
            ),
        });
    }
    let mut frame = FrameHeader {
        process: if options.precision == 8 {
            Process::Baseline
        } else {
            Process::ExtendedSequential
        },
        precision: options.precision,
        width: input.width,
        height: input.height,
        components: specs,
    };
    let (marker, data) = match output_space {
        ColorSpace::Grayscale | ColorSpace::YCbCr => (0xe0, JFIF),
        ColorSpace::Ycck => (0xee, ADOBE2),
        _ => (0xee, ADOBE0),
    };
    let metadata_bytes = data.len() + core::mem::size_of::<MetadataSegment<'_>>();
    if metadata_bytes > options.limits.max_metadata_bytes {
        return Err(Error::LimitExceeded("encoder metadata"));
    }
    let remaining = options
        .limits
        .max_memory_bytes
        .checked_sub(metadata_bytes)
        .ok_or(Error::LimitExceeded("encoder metadata and coefficients"))?;
    let (mut components, memory_bytes) = allocate_components(
        &frame,
        Limits {
            max_memory_bytes: remaining,
            ..options.limits
        },
    )?;
    let tables = quantization_tables(options.quality, options.baseline_quantization)?;
    if options.precision == 8 && tables.iter().any(|q| q.iter().any(|&v| v > 255)) {
        frame.process = Process::ExtendedSequential;
    }
    let max_h = factors[..channels]
        .iter()
        .map(|f| f.0 as usize)
        .max()
        .unwrap();
    let max_v = factors[..channels]
        .iter()
        .map(|f| f.1 as usize)
        .max()
        .unwrap();
    for (i, c) in components.iter_mut().enumerate() {
        c.quantization = tables[c.spec.quantization_table as usize];
        c.successive_approximation.fill(0);
        for by in 0..c.blocks_high {
            for bx in 0..c.blocks_wide {
                let mut block = [0u16; 64];
                for y in 0..8 {
                    for x in 0..8 {
                        let sx = (bx * 8 + x).min(c.width - 1);
                        let sy = (by * 8 + y).min(c.height - 1);
                        block[y * 8 + x] = resample(
                            input,
                            ColorTransform {
                                precision: options.precision,
                                space: output_space,
                                cmyk_polarity: options.cmyk_polarity,
                            },
                            i,
                            sx,
                            sy,
                            channels,
                            c.spec.horizontal_sampling as usize,
                            c.spec.vertical_sampling as usize,
                            max_h,
                            max_v,
                        );
                    }
                }
                c.blocks[by * c.blocks_wide + bx] =
                    fdct(&block, &c.quantization, options.precision);
            }
        }
    }
    let mut metadata = Vec::new();
    metadata
        .try_reserve_exact(1)
        .map_err(|_| Error::Allocation)?;
    metadata.push(MetadataSegment {
        marker,
        data,
        offset: 0,
        preceding_scans: 0,
    });
    Ok(CoefficientImage {
        frame,
        components,
        metadata,
        scans: 0,
        consumed: 0,
        memory_bytes: memory_bytes + metadata_bytes,
        limits: options.limits,
    })
}

pub(crate) fn validate_raster(
    input: &EncodeInput<'_>,
    precision: u8,
    limits: Limits,
) -> Result<usize> {
    let channels = match input.color_space {
        ColorSpace::Grayscale => 1,
        ColorSpace::Rgb | ColorSpace::YCbCr => 3,
        ColorSpace::Cmyk | ColorSpace::Ycck => 4,
        _ => return Err(Error::Unsupported("encoder input color space")),
    };
    if input.width == 0
        || input.height == 0
        || !(1..=16).contains(&input.precision)
        || !(1..=16).contains(&precision)
    {
        return Err(Error::Invalid("encoder dimensions or precision"));
    }
    let pixels = input.width as usize * input.height as usize;
    if pixels > limits.max_pixels {
        return Err(Error::LimitExceeded("encoder pixels"));
    }
    let count = pixels
        .checked_mul(channels)
        .ok_or(Error::LimitExceeded("encoder source samples"))?;
    let maximum = (1u32 << input.precision) - 1;
    let valid = match input.samples {
        SampleData::U8(v) => {
            input.precision <= 8 && v.len() == count && v.iter().all(|&s| u32::from(s) <= maximum)
        }
        SampleData::U16(v) => v.len() == count && v.iter().all(|&s| u32::from(s) <= maximum),
    };
    if !valid {
        return Err(Error::Invalid("encoder source sample count or range"));
    }
    Ok(channels)
}

pub(crate) fn quantization_tables(quality: u8, baseline: bool) -> Result<[[u16; 64]; 2]> {
    if !(1..=100).contains(&quality) {
        return Err(Error::Invalid("encoder quality"));
    }
    let scale = if quality < 50 {
        5000 / u32::from(quality)
    } else {
        200 - 2 * u32::from(quality)
    };
    let cap = if baseline { 255 } else { 65535 };
    Ok([
        LUMA.map(|v| ((u32::from(v) * scale + 50) / 100).clamp(1, cap) as u16),
        CHROMA.map(|v| ((u32::from(v) * scale + 50) / 100).clamp(1, cap) as u16),
    ])
}

#[derive(Clone, Copy)]
pub(crate) struct ColorTransform {
    pub precision: u8,
    pub space: ColorSpace,
    pub cmyk_polarity: CmykPolarity,
}

impl ColorTransform {
    #[cfg(test)]
    fn new(precision: u8, space: ColorSpace) -> Self {
        Self {
            precision,
            space,
            cmyk_polarity: CmykPolarity::AdobeInverted,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn resample(
    input: &EncodeInput<'_>,
    transform: ColorTransform,
    component: usize,
    x: usize,
    y: usize,
    channels: usize,
    h: usize,
    v: usize,
    max_h: usize,
    max_v: usize,
) -> u16 {
    let ColorTransform {
        precision,
        space,
        cmyk_polarity,
    } = transform;
    let maximum = (1i64 << precision) - 1;
    // Integrate the rectangular source footprint using integer overlap weights.
    // This also handles legal fractional sampling ratios such as H=3 and H=2.
    let (left, right, top, bottom) = (x * max_h, (x + 1) * max_h, y * max_v, (y + 1) * max_v);
    let mut total = 0u32;
    for sy in top / v..bottom.div_ceil(v) {
        let wy = bottom.min((sy + 1) * v) - top.max(sy * v);
        for sx in left / h..right.div_ceil(h) {
            let wx = right.min((sx + 1) * h) - left.max(sx * h);
            let index = (sy.min(input.height as usize - 1) * input.width as usize
                + sx.min(input.width as usize - 1))
                * channels;
            let read = |channel: usize| {
                let value = match input.samples {
                    SampleData::U8(v) => u32::from(v[index + channel]),
                    SampleData::U16(v) => u32::from(v[index + channel]),
                };
                let source_max = (1u32 << input.precision) - 1;
                ((value * ((1u32 << precision) - 1) + source_max / 2) / source_max) as i64
            };
            let to_ycc = (input.color_space == ColorSpace::Rgb && space == ColorSpace::YCbCr)
                || (input.color_space == ColorSpace::Cmyk
                    && space == ColorSpace::Ycck
                    && component < 3);
            let value = if to_ycc {
                let mut rgb = [read(0), read(1), read(2)];
                // YCCK applies the same matrix to conventional CMY values;
                // the complemented K component passes through separately.
                if input.color_space == ColorSpace::Cmyk
                    && cmyk_polarity == CmykPolarity::AdobeInverted
                {
                    rgb = rgb.map(|v| maximum - v);
                }
                let [r, g, b] = rgb;
                let center = 1i64 << (precision - 1);
                let value = match component {
                    0 => (19595 * r + 38470 * g + 7471 * b + 32768) >> 16,
                    1 => ((-11059 * r - 21709 * g + 32768 * b + 32768) >> 16) + center,
                    _ => ((32768 * r - 27439 * g - 5329 * b + 32768) >> 16) + center,
                };
                value.clamp(0, (1 << precision) - 1)
            } else if input.color_space == ColorSpace::Cmyk
                && cmyk_polarity == CmykPolarity::Conventional
            {
                maximum - read(component)
            } else {
                read(component)
            };
            total += value as u32 * (wx * wy) as u32;
        }
    }
    let area = (max_h * max_v) as u32;
    ((total + area / 2) / area) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fractional_area_footprints_and_edge_replication() {
        let input = EncodeInput {
            width: 3,
            height: 1,
            precision: 8,
            color_space: ColorSpace::YCbCr,
            samples: SampleData::U8(&[128, 0, 128, 128, 30, 128, 128, 90, 128]),
        };
        assert_eq!(
            resample(
                &input,
                ColorTransform::new(8, ColorSpace::YCbCr),
                1,
                0,
                0,
                3,
                2,
                1,
                3,
                1
            ),
            10
        );
        assert_eq!(
            resample(
                &input,
                ColorTransform::new(8, ColorSpace::YCbCr),
                1,
                1,
                0,
                3,
                2,
                1,
                3,
                1
            ),
            70
        );
        assert_eq!(
            resample(
                &input,
                ColorTransform::new(8, ColorSpace::YCbCr),
                1,
                0,
                0,
                3,
                1,
                1,
                2,
                1
            ),
            15
        );
        assert_eq!(
            resample(
                &input,
                ColorTransform::new(8, ColorSpace::YCbCr),
                1,
                1,
                0,
                3,
                1,
                1,
                2,
                1
            ),
            90
        );
        let input = EncodeInput {
            width: 4,
            height: 2,
            precision: 8,
            color_space: ColorSpace::Grayscale,
            samples: SampleData::U8(&[0, 10, 20, 30, 40, 50, 60, 70]),
        };
        assert_eq!(
            resample(
                &input,
                ColorTransform::new(8, ColorSpace::Grayscale),
                0,
                0,
                0,
                1,
                1,
                1,
                4,
                2
            ),
            35
        );
    }

    #[test]
    fn jfif_primary_colors_and_neutral_chroma() {
        for (rgb, want) in [
            ([0, 0, 0], [0, 128, 128]),
            ([255, 255, 255], [255, 128, 128]),
            ([255, 0, 0], [76, 85, 255]),
            ([0, 255, 0], [150, 44, 21]),
            ([0, 0, 255], [29, 255, 107]),
        ] {
            let input = EncodeInput {
                width: 1,
                height: 1,
                precision: 8,
                color_space: ColorSpace::Rgb,
                samples: SampleData::U8(&rgb),
            };
            for (c, expected) in want.into_iter().enumerate() {
                assert_eq!(
                    resample(
                        &input,
                        ColorTransform::new(8, ColorSpace::YCbCr),
                        c,
                        0,
                        0,
                        3,
                        1,
                        1,
                        1,
                        1
                    ),
                    expected
                );
            }
        }
    }
}
