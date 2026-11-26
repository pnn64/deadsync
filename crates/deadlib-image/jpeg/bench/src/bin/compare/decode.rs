use super::{Bridges, Bytes, Header, Pixels, Row, Shape, difference, header, measure};
use deadlib_image_jpeg as native;
use native_samples::Samples;
use std::{
    hint::black_box,
    io::{Cursor, Write},
    time::Duration,
};
use zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};

const CODECS: [&str; 10] = [
    "deadlib",
    "libjpeg-turbo",
    "libjpeg-turbo-fast",
    "mozjpeg",
    "jpegli",
    "zune-jpeg",
    "image",
    "jpeg-decoder",
    "zenjpeg",
    "wuffs",
];
const JPGD: &str = "jpgd";

pub fn deadlib(
    input: &[u8],
    precision: u32,
    method: native::Upsampling,
) -> Result<Pixels<'static>, String> {
    let image = native::decode_with_options(
        input,
        native::DecodeOptions {
            strict: false,
            ..Default::default()
        },
        method,
    )
    .map_err(|e| e.to_string())?;
    let shape = Shape {
        w: image.width,
        h: image.height,
        channels: image.color_type.channels() as u32,
        precision,
    };
    let data = match image.samples {
        Samples::U8(samples) => Bytes::U8(samples),
        // The display API scales non-8-bit samples to the entire u16 range.
        // Recover the original precision for comparison with native libjpeg samples.
        Samples::U16(samples) if precision < 8 => Bytes::U8(
            samples
                .into_iter()
                .map(|v| (v >> (16 - precision)) as u8)
                .collect(),
        ),
        Samples::U16(mut samples) => {
            if precision < 16 {
                samples.iter_mut().for_each(|v| *v >>= 16 - precision);
            }
            Bytes::U16(samples)
        }
    };
    Ok(Pixels { shape, data })
}

fn upsampling(desc: Header) -> native::Upsampling {
    if desc.sampling >> 4 == 4 {
        native::Upsampling::Nearest
    } else {
        native::Upsampling::Linear
    }
}

fn run<'a>(id: usize, input: &[u8], desc: Header, libs: &'a Bridges) -> Result<Pixels<'a>, String> {
    if id == 0 {
        return deadlib(input, desc.shape.precision, upsampling(desc));
    }
    if id <= 4 {
        return match id {
            1 | 2 => libs.turbo.decode(input, i32::from(id == 2)),
            3 => libs.moz.decode(input, 0),
            _ => libs.jpegli.decode(input, 0),
        }
        .map(Pixels::from);
    }
    let gray = desc.shape.channels == 1;
    let ((w, h), channels, data) = match id {
        5 => {
            let options = DecoderOptions::default()
                .set_max_width(65535)
                .set_max_height(65535)
                .jpeg_set_out_colorspace(if gray {
                    ColorSpace::Luma
                } else {
                    ColorSpace::RGB
                });
            let mut decoder =
                zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(input), options);
            let data = decoder.decode().map_err(|e| format!("{e:?}"))?;
            let info = decoder.info().ok_or("zune returned no frame information")?;
            (
                (u32::from(info.width), u32::from(info.height)),
                if gray { 1 } else { 3 },
                data,
            )
        }
        6 => {
            let image = image::load_from_memory_with_format(input, image::ImageFormat::Jpeg)
                .map_err(|e| e.to_string())?;
            let dims = (image.width(), image.height());
            match image {
                image::DynamicImage::ImageLuma8(v) => (dims, 1, v.into_raw()),
                image::DynamicImage::ImageRgb8(v) => (dims, 3, v.into_raw()),
                _ => return Err("unmatched image raster format".into()),
            }
        }
        7 => {
            use jpeg_decoder::PixelFormat::*;
            let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(input));
            let data = decoder.decode().map_err(|e| e.to_string())?;
            let info = decoder
                .info()
                .ok_or("jpeg-decoder returned no frame information")?;
            let dims = (u32::from(info.width), u32::from(info.height));
            match info.pixel_format {
                L8 => (dims, 1, data),
                RGB24 => (dims, 3, data),
                CMYK32 => (
                    dims,
                    3,
                    data.as_chunks::<4>()
                        .0
                        .iter()
                        .map(|p| {
                            let k = u16::from(255 - p[3]);
                            [p[0], p[1], p[2]].map(|c| ((u16::from(255 - c) * k + 127) / 255) as u8)
                        })
                        .collect::<Vec<_>>()
                        .into_flattened(),
                ),
                L16 => {
                    return Ok(Pixels {
                        shape: Shape {
                            w: dims.0,
                            h: dims.1,
                            channels: 1,
                            precision: desc.shape.precision,
                        },
                        data: Bytes::U8(data),
                    });
                }
            }
        }
        8 => {
            let output = zenjpeg::decoder::Decoder::new()
                .output_format(if gray {
                    zenjpeg::decoder::PixelFormat::Gray
                } else {
                    zenjpeg::decoder::PixelFormat::Rgb
                })
                .decode(input, zenjpeg::encoder::Unstoppable)
                .map_err(|e| e.to_string())?;
            let dims = (output.width, output.height);
            (
                dims,
                if gray { 1 } else { 3 },
                output
                    .into_pixels_u8()
                    .ok_or("unmatched zenjpeg raster format")?,
            )
        }
        9 => return libs.extra.decode(input, 1).map(Pixels::from),
        _ => {
            return libs
                .extra
                .decode(input, if gray { 2 } else { 0 })
                .map(Pixels::from);
        }
    };
    Ok(Pixels {
        shape: Shape {
            w,
            h,
            channels,
            precision: 8,
        },
        data: Bytes::U8(data),
    })
}

pub fn bench(
    fields: &[&str],
    index: usize,
    sample: Duration,
    libs: &Bridges,
    out: &mut impl Write,
) {
    let &[group, file] = fields else {
        panic!("decode manifest row")
    };
    let bytes = std::fs::read(file).unwrap();
    let Some(desc) = header(&bytes) else {
        eprintln!("{}: no standalone frame header", file);
        return;
    };
    let (verifier, expected) = match libs.turbo.decode(&bytes, 0) {
        Ok(buffer) => ("libjpeg-turbo", Ok(Pixels::from(buffer))),
        Err(_) => (
            "deadlib",
            deadlib(&bytes, desc.shape.precision, upsampling(desc)),
        ),
    };
    let shape = expected.as_ref().map_or(desc.shape, |p| p.shape);
    let mut checked = Vec::new();
    let count = CODECS.len() + 1;
    for position in 0..count {
        let id = (position + index) % count;
        let check = run(id, &bytes, desc, libs).and_then(|actual| {
            let expected = expected
                .as_ref()
                .map_err(|e| format!("no comparable display output: {e}"))?;
            if actual.shape != expected.shape {
                return Err(format!(
                    "shape mismatch: {:?} != {:?}",
                    actual.shape, expected.shape
                ));
            }
            let (psnr, delta) =
                difference(shape, actual.data.as_slice(), expected.data.as_slice())?;
            if psnr < 28.0 {
                return Err(format!(
                    "pixel agreement below 28 dB ({psnr:.2} dB, max {delta})"
                ));
            }
            if matches!(desc.process, 0xc3 | 0xcb) && desc.shape.channels == 1 && delta != 0 {
                return Err(format!("lossless gray pixels differ (max {delta})"));
            }
            Ok((actual.data.as_slice().len(), psnr, delta))
        });
        checked.push((id, check));
    }
    drop(expected);
    for (id, check) in checked {
        let result = check.map(|(size, psnr, delta)| {
            let (ns, cycles) = measure(sample, || run(id, black_box(&bytes), desc, libs).unwrap());
            (size, ns, cycles, psnr, delta)
        });
        Row {
            group,
            file,
            config: if desc.sampling >> 4 == 4 {
                "decode-nearest-wide"
            } else {
                "decode"
            },
            shape,
            sof: desc.process,
            sampling: desc.sampling,
            codec: CODECS.get(id).copied().unwrap_or(JPGD),
            input_bytes: bytes.len(),
            verifier,
        }
        .write(out, result);
    }
}
