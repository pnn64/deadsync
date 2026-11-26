use super::{Bridges, Bytes, Pixels, Row, Shape, decode, difference, foreign, header, measure};
use deadlib_image_jpeg as native;
use native_samples::{ColorType, Image, Samples};
use std::{hint::black_box, io::Write, time::Duration};

const PROGRESSIVE: u32 = 1;
const OPTIMIZE: u32 = 2;
const ARITHMETIC: u32 = 4;
const LOSSLESS: u32 = 8;
const NATIVE_PROFILE: u32 = 16;
const CODECS: [&str; 11] = [
    "deadlib-opt",
    "deadlib-fixed",
    "turbo-opt",
    "turbo-fixed",
    "mozjpeg",
    "jpegli",
    "sjpeg-opt",
    "sjpeg-fixed",
    "zenjpeg",
    "jpeg-encoder",
    "image",
];

#[derive(Clone, Copy)]
struct Settings {
    quality: u8,
    h: u32,
    v: u32,
    flags: u32,
}

impl Settings {
    fn new(config: &str) -> Self {
        let (quality, h, v, flags) = match config {
            "q75-420" => (75, 2, 2, 0),
            "q90-420" => (90, 2, 2, 0),
            "q85-444" | "q85-gray" => (85, 1, 1, 0),
            "q85-422" => (85, 2, 1, 0),
            "q85-440" => (85, 1, 2, 0),
            "q85-411" => (85, 4, 1, 0),
            "q85-410" => (85, 4, 2, 0),
            "q85-progressive" => (85, 2, 2, PROGRESSIVE),
            "q85-arithmetic" => (85, 2, 2, ARITHMETIC),
            "lossless" => (0, 1, 1, LOSSLESS),
            _ => panic!("unknown encoder configuration {config}"),
        };
        Self {
            quality,
            h,
            v,
            flags,
        }
    }

    fn flag(self, bit: u32) -> bool {
        self.flags & bit != 0
    }
}

fn native_codec(input: &Image, s: Settings, optimize: bool) -> Result<Vec<u8>, native::Error> {
    if s.flag(LOSSLESS) {
        return native::encode_lossless(
            input,
            native::LosslessOptions {
                optimize_huffman: optimize,
                ..Default::default()
            },
        );
    }
    let options = native::EncodeOptions {
        quality: s.quality,
        precision: input.bit_depth,
        subsampling: match (s.h, s.v) {
            (1, 1) => native::Subsampling::S444,
            (2, 1) => native::Subsampling::S422,
            (1, 2) => native::Subsampling::S440,
            (4, 1) => native::Subsampling::S411,
            (4, 2) => native::Subsampling::S410,
            _ => native::Subsampling::S420,
        },
        coding: native::CodingOptions {
            progressive: s.flag(PROGRESSIVE),
            successive_approximation: u8::from(s.flag(PROGRESSIVE)),
            entropy_coding: if s.flag(ARITHMETIC) {
                native::EntropyCoding::Arithmetic
            } else {
                native::EntropyCoding::Huffman
            },
            optimize_huffman: optimize,
            ..Default::default()
        },
        ..Default::default()
    };
    if let Samples::U16(samples) = &input.samples
        && input.bit_depth == 12
    {
        return native::encode_samples(
            &native::EncodeInput {
                width: u16::try_from(input.width).unwrap(),
                height: u16::try_from(input.height).unwrap(),
                precision: input.bit_depth,
                color_space: native::ColorSpace::Grayscale,
                samples: native::SampleData::U16(samples),
            },
            options,
        );
    }
    native::encode(input, options)
}

fn run<'a>(id: usize, input: &Image, s: Settings, libs: &'a Bridges) -> Result<Bytes<'a>, String> {
    if id <= 1 {
        return native_codec(input, s, id == 0)
            .map(Bytes::U8)
            .map_err(|e| e.to_string());
    }
    let channels = input.color_type.channels() as u32;
    if id <= 7 {
        let (lib, flags) = match id {
            2 => (&libs.turbo, OPTIMIZE),
            3 => (&libs.turbo, 0),
            4 => (&libs.moz, OPTIMIZE | NATIVE_PROFILE),
            5 => (&libs.jpegli, OPTIMIZE),
            6 => (&libs.extra, OPTIMIZE),
            _ => (&libs.extra, 0),
        };
        let ptr = match &input.samples {
            Samples::U8(v) => v.as_ptr(),
            Samples::U16(v) => v.as_ptr().cast(),
        };
        return lib
            .encode(
                &foreign::Raster {
                    data: ptr,
                    width: input.width,
                    height: input.height,
                    channels,
                    precision: input.bit_depth.into(),
                    quality: s.quality.into(),
                    horizontal: s.h,
                    vertical: s.v,
                    flags: s.flags | flags,
                },
                id >= 6,
            )
            .map(Bytes::Foreign);
    }
    let Samples::U8(samples) = &input.samples else {
        return Err("8-bit encoder".into());
    };
    if s.flag(LOSSLESS | ARITHMETIC) {
        return Err("encoder process unavailable".into());
    }
    let gray = channels == 1;
    let width = input.width as u16;
    let height = input.height as u16;
    let mut data = Vec::new();
    match id {
        8 => {
            use zenjpeg::encoder::{ChromaSubsampling, EncoderConfig, PixelLayout, Unstoppable};
            let config = if gray {
                EncoderConfig::grayscale(f32::from(s.quality))
            } else {
                EncoderConfig::ycbcr(
                    f32::from(s.quality),
                    match (s.h, s.v) {
                        (1, 1) => ChromaSubsampling::None,
                        (2, 1) => ChromaSubsampling::HalfHorizontal,
                        (1, 2) => ChromaSubsampling::HalfVertical,
                        (2, 2) => ChromaSubsampling::Quarter,
                        _ => return Err("zenjpeg encoder sampling unavailable".into()),
                    },
                )
            }
            .progressive(s.flag(PROGRESSIVE));
            let layout = if gray {
                PixelLayout::Gray8Srgb
            } else {
                PixelLayout::Rgb8Srgb
            };
            let mut encoder = config
                .encode_from_bytes(input.width, input.height, layout)
                .map_err(|e| e.to_string())?;
            encoder
                .push_packed(samples, Unstoppable)
                .map_err(|e| e.to_string())?;
            data = encoder.finish().map_err(|e| e.to_string())?;
        }
        9 => {
            let mut encoder = jpeg_encoder::Encoder::new(&mut data, s.quality);
            encoder.set_sampling_factor(match (s.h, s.v) {
                (1, 1) => jpeg_encoder::SamplingFactor::F_1_1,
                (2, 1) => jpeg_encoder::SamplingFactor::F_2_1,
                (1, 2) => jpeg_encoder::SamplingFactor::F_1_2,
                (2, 2) => jpeg_encoder::SamplingFactor::F_2_2,
                (4, 1) => jpeg_encoder::SamplingFactor::F_4_1,
                (4, 2) => jpeg_encoder::SamplingFactor::F_4_2,
                _ => unreachable!(),
            });
            encoder.set_optimized_huffman_tables(true);
            encoder.set_progressive(s.flag(PROGRESSIVE));
            let format = if gray {
                jpeg_encoder::ColorType::Luma
            } else {
                jpeg_encoder::ColorType::Rgb
            };
            encoder
                .encode(samples, width, height, format)
                .map_err(|e| e.to_string())?;
        }
        _ => {
            if s.flag(PROGRESSIVE) {
                return Err("image encoder is sequential".into());
            }
            let format = if gray {
                image::ExtendedColorType::L8
            } else {
                image::ExtendedColorType::Rgb8
            };
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut data, s.quality)
                .encode(samples, input.width, input.height, format)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(Bytes::U8(data))
}

pub fn bench(
    fields: &[&str],
    index: usize,
    sample: Duration,
    libs: &Bridges,
    out: &mut impl Write,
) {
    let &[group, file, w, h, channels, precision, config] = fields else {
        panic!("encode manifest row")
    };
    let shape = Shape {
        w: w.parse().unwrap(),
        h: h.parse().unwrap(),
        channels: channels.parse().unwrap(),
        precision: precision.parse().unwrap(),
    };
    let bytes = std::fs::read(file).unwrap();
    let input = Image {
        width: shape.w,
        height: shape.h,
        bit_depth: shape.precision as u8,
        color_type: if shape.channels == 1 {
            ColorType::Grayscale
        } else {
            ColorType::Rgb
        },
        samples: if shape.precision <= 8 {
            Samples::U8(bytes.clone())
        } else {
            Samples::U16(
                bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&p| u16::from_le_bytes(p))
                    .collect(),
            )
        },
    };
    let settings = Settings::new(config);
    let sof = if settings.flag(LOSSLESS) {
        0xc3
    } else if settings.flag(PROGRESSIVE) {
        0xc2
    } else if settings.flag(ARITHMETIC) {
        0xc9
    } else if shape.precision == 12 {
        0xc1
    } else {
        0xc0
    };
    let sampling = ((settings.h << 4) | settings.v) as u8;
    for pos in 0..CODECS.len() {
        let id = (pos + index) % CODECS.len();
        let mut verifier = "libjpeg-turbo";
        let result = run(id, &input, settings, libs).and_then(|jpeg| {
            let encoded = jpeg.as_slice();
            let info = header(encoded).ok_or("encoded output has no frame header")?;
            if info.shape != shape || info.process != sof || info.sampling != sampling {
                return Err(format!(
                    "emitted different JPEG mode: SOF {:02x}, sampling {:02x}, {:?}",
                    info.process, info.sampling, info.shape
                ));
            }
            let decoded = match libs.turbo.decode(encoded, 0) {
                Ok(out) => Pixels::from(out),
                Err(_) => {
                    verifier = "deadlib";
                    decode::deadlib(encoded, shape.precision, native::Upsampling::Linear)?
                }
            };
            if decoded.shape != shape {
                return Err("roundtrip returned a different raster shape".into());
            }
            let (psnr, delta) = difference(shape, decoded.data.as_slice(), &bytes)?;
            if settings.flag(LOSSLESS) && delta != 0 {
                return Err(format!("lossless samples changed (max {delta})"));
            }
            let size = encoded.len();
            drop(decoded);
            drop(jpeg);
            let (ns, cycles) = measure(sample, || {
                run(id, black_box(&input), settings, libs).unwrap()
            });
            Ok((size, ns, cycles, psnr, delta))
        });
        Row {
            group,
            file,
            config,
            shape,
            sof,
            sampling,
            codec: CODECS[id],
            input_bytes: bytes.len(),
            verifier,
        }
        .write(out, result);
    }
}
