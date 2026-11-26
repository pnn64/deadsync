use super::{Bytes, Case, foreign, measure};
use deadlib_image_png::Samples;
use png_decode_bench::{image_rgba8, native_rgba8, png_assets_rgba8};
use std::{
    hint::black_box,
    io::{Cursor, Write},
    time::Duration,
};
use zune_core::{bytestream::ZCursor, options::DecoderOptions, result::DecodingResult};

const CODECS: [&str; 9] = [
    "deadlib",
    "rust-png",
    "image",
    "zune-png",
    "blazediff",
    "wuffs",
    "libspng",
    "libpng",
    "stb_image",
];

fn rgba<T: Copy, const N: usize>(samples: &[T], max: T) -> Vec<T> {
    if N == 1 {
        return samples
            .iter()
            .map(|&g| [g, g, g, max])
            .collect::<Vec<_>>()
            .into_flattened();
    }
    samples
        .as_chunks::<N>()
        .0
        .iter()
        .map(|s| match N {
            2 => [s[0], s[0], s[0], s[1]],
            3 => [s[0], s[1], s[2], max],
            _ => unreachable!(),
        })
        .collect::<Vec<_>>()
        .into_flattened()
}

fn complete<T: Copy>(channels: usize, samples: Vec<T>, max: T) -> Vec<T> {
    match channels {
        1 => rgba::<T, 1>(&samples, max),
        2 => rgba::<T, 2>(&samples, max),
        3 => rgba::<T, 3>(&samples, max),
        4 => samples,
        _ => unreachable!(),
    }
}

fn png16(input: &[u8]) -> Result<(u32, u32, Vec<u16>), String> {
    let mut decoder = png::Decoder::new(Cursor::new(input));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut out = vec![0; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut out).map_err(|e| e.to_string())?;
    let samples = out[..frame.buffer_size()]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&be| u16::from_be_bytes(be))
        .collect();
    Ok((
        frame.width,
        frame.height,
        complete(frame.color_type.samples(), samples, u16::MAX),
    ))
}

fn zune(input: &[u8], depth: u8) -> Result<(u32, u32, Bytes), String> {
    let packed_gray = input[25] == 0 && input[24] < 8;
    let options = DecoderOptions::new_fast()
        .png_set_add_alpha_channel(!packed_gray)
        .png_set_strip_to_8bit(depth == 8);
    let mut decoder = zune_png::PngDecoder::new_with_options(ZCursor::new(input), options);
    let samples = decoder.decode().map_err(|e| format!("{e:?}"))?;
    let (w, h) = decoder.dimensions().unwrap();
    let channels = decoder.colorspace().unwrap().num_components();
    let samples = match samples {
        DecodingResult::U8(out) => Bytes::U8(complete(channels, out, 255)),
        DecodingResult::U16(out) => Bytes::U16(complete(channels, out, u16::MAX)),
        _ => unreachable!(),
    };
    Ok((w as u32, h as u32, samples))
}

fn run(codec: usize, input: &[u8], depth: u8) -> Result<(u32, u32, Bytes), String> {
    if codec == 0 && depth == 16 {
        let native = deadlib_image_png::decode(input).map_err(|e| e.to_string())?;
        let (w, h) = (native.image.width, native.image.height);
        let output = if native.image.color_type == deadlib_image_png::ColorType::Rgba {
            native.image
        } else {
            native
                .to_rgba16(Default::default())
                .map_err(|e| e.to_string())?
        };
        let Samples::U16(samples) = output.samples else {
            unreachable!()
        };
        return Ok((w, h, Bytes::U16(samples)));
    }
    if codec == 1 && depth == 16 {
        let (w, h, samples) = png16(input)?;
        return Ok((w, h, Bytes::U16(samples)));
    }
    let output = match codec {
        0 => native_rgba8(input),
        1 => png_assets_rgba8(input),
        2 if depth == 8 => image_rgba8(input),
        2 => {
            let image = image::load_from_memory_with_format(input, image::ImageFormat::Png)
                .map_err(|e| e.to_string())?
                .into_rgba16();
            return Ok((image.width(), image.height(), Bytes::U16(image.into_raw())));
        }
        3 => return zune(input, depth),
        4 if depth == 8 => {
            let image = blazediff_png::decode(input).map_err(|e| e.to_string())?;
            return Ok((image.width, image.height, Bytes::U8(image.data)));
        }
        4 => {
            let image = blazediff_png::decode_with(
                input,
                &blazediff_png::DecodeOptions {
                    format: blazediff_png::DecodeFormat::Rgba16,
                    ..Default::default()
                },
            )
            .map_err(|e| e.to_string())?;
            return Ok((image.width, image.height, Bytes::U8(image.data)));
        }
        _ => {
            let image = foreign::decode((codec - 5) as i32, input, depth)?;
            return Ok((image.width, image.height, Bytes::Foreign(image)));
        }
    };
    Ok((
        output.width(),
        output.height(),
        Bytes::U8(output.into_raw()),
    ))
}

pub fn bench(case: &Case<'_>, index: usize, sample: Duration, out: &mut impl Write) {
    for depth in [8, 16] {
        if depth == 16 && case.depth != 16 {
            continue;
        }
        let (_, _, expected) =
            run(1, case.bytes, depth).unwrap_or_else(|e| panic!("{}: {e}", case.name));
        bench_pixels(case, index, sample, out, depth, &expected, false);
    }
}

pub fn bench_pixels(
    case: &Case<'_>,
    index: usize,
    sample: Duration,
    out: &mut impl Write,
    depth: u8,
    expected: &Bytes,
    fpng: bool,
) {
    let count = CODECS.len() + usize::from(fpng);
    for pos in 0..count {
        let id = (pos + index) % count;
        let codec = CODECS.get(id).copied().unwrap_or("fpng");
        let operation = || {
            let input = black_box(case.bytes);
            if id == CODECS.len() {
                let result = foreign::decode(4, input, depth)?;
                Ok((result.width, result.height, Bytes::Foreign(result)))
            } else {
                run(id, input, depth)
            }
        };
        let result = operation().and_then(|(w, h, pixels)| {
            if (w, h) != (case.width, case.height) || pixels.as_slice() != expected.as_slice() {
                return Err("pixel or dimension mismatch".to_owned());
            }
            let size = pixels.as_slice().len();
            drop(pixels);
            let (ns, cycles) = if sample.is_zero() {
                (0, 0)
            } else {
                measure(sample, || operation().unwrap())
            };
            Ok((size, ns, cycles))
        });
        case.row(
            out,
            if fpng {
                "decode-fpng"
            } else if depth == 8 {
                "decode-rgba8"
            } else {
                "decode-rgba16"
            },
            codec,
            result,
        );
    }
}
