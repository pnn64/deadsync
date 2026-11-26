use super::{Bytes, Case, decode, foreign, measure};
use deadlib_image_png::{ColorType, EncodeOptions, Image, Png, Samples, zlib::Compression};
use std::{
    hint::black_box,
    io::{Cursor, Write},
    time::Duration,
};

const MODES: [&str; 15] = [
    "deadlib-fast",
    "deadlib-rle",
    "deadlib-rle-up",
    "rust-png-fast",
    "rust-png-fastest",
    "rust-png-balanced",
    "libpng-1",
    "libpng-6",
    "libspng-1",
    "libspng-6",
    "fpng",
    "fpng-slower",
    "fpnge-1",
    "fpnge-4",
    "blazediff-1",
];

pub fn modes() -> Vec<usize> {
    match std::env::var("PNG_COMPARE_ENCODE_MODES") {
        Ok(value) => value
            .split(',')
            .map(|name| {
                MODES
                    .iter()
                    .position(|&mode| mode == name)
                    .unwrap_or_else(|| panic!("unknown encoder mode {name:?}"))
            })
            .collect(),
        Err(std::env::VarError::NotPresent) => (0..MODES.len()).collect(),
        Err(error) => panic!("PNG_COMPARE_ENCODE_MODES: {error}"),
    }
}

fn native_raw(bytes: &[u8]) -> Result<(png::OutputInfo, Vec<u8>, Vec<u8>), String> {
    let mut reader = png::Decoder::new(Cursor::new(bytes))
        .read_info()
        .map_err(|e| e.to_string())?;
    let mut packed = vec![0; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut packed).map_err(|e| e.to_string())?;
    packed.truncate(frame.buffer_size());
    reader.finish().map_err(|e| e.to_string())?;
    let palette = reader
        .info()
        .palette
        .as_deref()
        .unwrap_or_default()
        .to_vec();
    Ok((frame, packed, palette))
}

fn rgba_image(bytes: &[u8]) -> Png {
    let image = png_decode_bench::png_assets_rgba8(bytes);
    Png::new(Image {
        width: image.width(),
        height: image.height(),
        color_type: ColorType::Rgba,
        bit_depth: 8,
        samples: Samples::U8(image.into_raw()),
    })
}

fn encode(
    id: usize,
    image: &Png,
    packed: &[u8],
    raster: &foreign::Raster<'_>,
) -> Result<Bytes, String> {
    let mut output = Vec::new();
    match id {
        0..=2 => {
            deadlib_image_png::encode_writer(
                image,
                &EncodeOptions {
                    interlaced: image.interlaced,
                    compression: if id == 0 {
                        Compression::Fast
                    } else {
                        Compression::Rle
                    },
                    filter: if id == 2 {
                        deadlib_image_png::Filter::Up
                    } else {
                        deadlib_image_png::Filter::Adaptive
                    },
                    ..Default::default()
                },
                &mut output,
            )
            .map_err(|e| e.to_string())?;
        }
        3..=5 => {
            let mut encoder = png::Encoder::new(&mut output, image.image.width, image.image.height);
            encoder.set_color(png::ColorType::from_u8(image.image.color_type as u8).unwrap());
            encoder.set_depth(png::BitDepth::from_u8(image.image.bit_depth).unwrap());
            if !image.palette.is_empty() {
                encoder.set_palette(image.palette.as_flattened());
            }
            encoder.set_compression(match id {
                3 => png::Compression::Fast,
                4 => png::Compression::Fastest,
                _ => png::Compression::Balanced,
            });
            if id != 4 {
                encoder.set_filter(png::Filter::Adaptive);
            }
            let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
            writer.write_image_data(packed).map_err(|e| e.to_string())?;
            writer.finish().map_err(|e| e.to_string())?;
        }
        6..=13 => {
            let (codec, level) = [
                (2, 1),
                (2, 6),
                (1, 1),
                (1, 6),
                (4, 0),
                (4, 1),
                (5, 1),
                (5, 4),
            ][id - 6];
            return foreign::encode(codec, raster, level).map(Bytes::Foreign);
        }
        14 => {
            output = blazediff_png::encode_ref(
                blazediff_png::ImageRef {
                    data: packed,
                    width: image.image.width,
                    height: image.image.height,
                },
                &blazediff_png::EncodeOptions {
                    color: blazediff_png::ColorMode::Rgba8,
                    compression: 1,
                    interlace: image.interlaced,
                    ..Default::default()
                },
            )
            .map_err(|e| e.to_string())?;
        }
        _ => unreachable!(),
    }
    Ok(Bytes::U8(output))
}

fn support(id: usize, input: &Png) -> bool {
    let color = input.image.color_type;
    let depth = input.image.bit_depth;
    match id {
        3..=5 => !input.interlaced,
        10..=11 => {
            !input.interlaced && depth == 8 && matches!(color, ColorType::Rgb | ColorType::Rgba)
        }
        12..=13 => !input.interlaced && depth >= 8 && color != ColorType::Indexed,
        14 => depth == 8 && color == ColorType::Rgba,
        _ => true,
    }
}

pub fn bench(
    case: &Case<'_>,
    index: usize,
    sample: Duration,
    modes: &[usize],
    out: &mut impl Write,
) {
    let native_format = case.group == "formats";
    let packed;
    let image = if native_format {
        packed = native_raw(case.bytes).unwrap().1;
        let original = deadlib_image_png::decode(case.bytes).unwrap();
        Png {
            interlaced: original.interlaced,
            palette: original.palette,
            ..Png::new(original.image)
        }
    } else {
        let image = rgba_image(case.bytes);
        let Samples::U8(samples) = &image.image.samples else {
            unreachable!()
        };
        packed = samples.clone();
        image
    };
    let raster = foreign::Raster::new(&image, &packed);
    let encode_case = Case {
        color: image.image.color_type as u8,
        depth: image.image.bit_depth,
        adam7: u8::from(image.interlaced),
        ..*case
    };
    for pos in 0..modes.len() {
        let id = modes[(pos + index) % modes.len()];
        let result = if !support(id, &image) {
            Err("unsupported output format".to_owned())
        } else {
            let operation = || {
                encode(
                    id,
                    black_box(&image),
                    black_box(&packed),
                    black_box(&raster),
                )
            };
            operation().and_then(|output| {
                let bytes = output.as_slice();
                let size = bytes.len();
                let (frame, decoded, palette) = native_raw(bytes)?;
                if (frame.width, frame.height) != (image.image.width, image.image.height)
                    || bytes[24..26] != [image.image.bit_depth, image.image.color_type as u8]
                    || bytes[28] != u8::from(image.interlaced)
                    || !packed_equal(&image, &packed, &decoded)
                    || palette != image.palette.as_flattened()
                {
                    return Err("sample, palette or output format mismatch".to_owned());
                }
                drop(output);
                let (ns, cycles) = if sample.is_zero() {
                    (0, 0)
                } else {
                    measure(sample, || operation().unwrap())
                };
                Ok((size, ns, cycles))
            })
        };
        encode_case.row(
            out,
            if native_format {
                "encode-native"
            } else {
                "encode-rgba8"
            },
            MODES[id],
            result,
        );
    }
}

// PNG doesn't specify the padding bits after the last sample in packed rows.
fn packed_equal(image: &Png, expected: &[u8], decoded: &[u8]) -> bool {
    if expected == decoded {
        return true;
    }
    let used = (image.image.width as usize
        * image.image.bit_depth as usize
        * image.image.color_type.channels())
        % 8;
    if used == 0 || expected.len() != decoded.len() {
        return false;
    }
    let row = expected.len() / image.image.height as usize;
    let mask = 255 << (8 - used);
    expected
        .chunks_exact(row)
        .zip(decoded.chunks_exact(row))
        .all(|(a, b)| a[..row - 1] == b[..row - 1] && (a[row - 1] & mask) == (b[row - 1] & mask))
}

pub fn bench_fpng(case: &Case<'_>, index: usize, sample: Duration, out: &mut impl Write) {
    let image = rgba_image(case.bytes);
    let Samples::U8(pixels) = &image.image.samples else {
        unreachable!()
    };
    let raster = foreign::Raster::new(&image, pixels);
    let compressed = foreign::encode(4, &raster, 0).unwrap();
    let input = Case::new(case.group, case.name, compressed.bytes());
    let expected = Bytes::U8(pixels.clone());
    decode::bench_pixels(&input, index, sample, out, 8, &expected, true);
}
