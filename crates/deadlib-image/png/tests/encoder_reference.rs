//! Compare image semantics from independently implemented PNG encoders.
use deadlib_image_png::{
    Animation, BlendOp, ColorType, DisposeOp, EncodeOptions, Filter, Frame, FrameControl, Image,
    Png, Samples, Transparency, decode, encode, encode_writer, zlib::Compression,
};
use std::{io::Cursor, path::Path, process::Command};

const FORMATS: [(ColorType, u8); 15] = [
    (ColorType::Grayscale, 1),
    (ColorType::Grayscale, 2),
    (ColorType::Grayscale, 4),
    (ColorType::Grayscale, 8),
    (ColorType::Grayscale, 16),
    (ColorType::Rgb, 8),
    (ColorType::Rgb, 16),
    (ColorType::Indexed, 1),
    (ColorType::Indexed, 2),
    (ColorType::Indexed, 4),
    (ColorType::Indexed, 8),
    (ColorType::GrayscaleAlpha, 8),
    (ColorType::GrayscaleAlpha, 16),
    (ColorType::Rgba, 8),
    (ColorType::Rgba, 16),
];
const FILTERS: [Filter; 6] = [
    Filter::None,
    Filter::Sub,
    Filter::Up,
    Filter::Average,
    Filter::Paeth,
    Filter::Adaptive,
];

fn image(width: u32, height: u32, color: ColorType, depth: u8, salt: usize) -> Image {
    let max = (1u32 << depth) - 1;
    let samples: Vec<u16> = (0..width as usize * height as usize * color.channels())
        .map(|n| match (n + salt) % 7 {
            0 => 0,
            1 => max as u16,
            2 => 1,
            _ => (((n as u32 * 37139) ^ ((n as u32 + salt as u32) * 137)) & max) as u16,
        })
        .collect();
    Image {
        width,
        height,
        color_type: color,
        bit_depth: depth,
        samples: if depth == 16 {
            Samples::U16(samples)
        } else {
            Samples::U8(samples.iter().map(|&n| n as u8).collect())
        },
    }
}
fn fixture(width: u32, height: u32, color: ColorType, depth: u8, salt: usize) -> Png {
    let mut png = Png::new(image(width, height, color, depth, salt));
    if color == ColorType::Indexed {
        png.palette = (0..1usize << depth)
            .map(|n| [n as u8, (n * 71) as u8, (n * 113) as u8])
            .collect();
        png.transparency = Some(Transparency::Palette(
            (0..png.palette.len().min(3))
                .map(|n| (n * 127) as u8)
                .collect(),
        ));
    } else {
        if matches!(color, ColorType::Rgb | ColorType::Rgba) && salt.is_multiple_of(2) {
            png.palette = vec![[0, 17, 255], [255, 71, 0]];
        }
        png.transparency = match color {
            ColorType::Grayscale => Some(Transparency::Grayscale(0)),
            ColorType::Rgb => Some(Transparency::Rgb([0, 1, max_sample(depth)])),
            _ => None,
        };
    }
    png
}
fn max_sample(depth: u8) -> u16 {
    ((1u32 << depth) - 1) as u16
}
fn packed(image: &Image) -> Vec<u8> {
    match &image.samples {
        Samples::U16(v) => v.iter().flat_map(|v| v.to_be_bytes()).collect(),
        Samples::U8(v) if image.bit_depth == 8 => v.clone(),
        Samples::U8(v) => {
            let depth = image.bit_depth as usize;
            let mut result = Vec::new();
            for row in v.chunks(image.width as usize) {
                let mut bytes = vec![0; (row.len() * depth).div_ceil(8)];
                for (x, &value) in row.iter().enumerate() {
                    bytes[x * depth / 8] |= value << (8 - depth - x * depth % 8);
                }
                result.extend(bytes);
            }
            result
        }
    }
}
fn transparency(png: &Png) -> Option<Vec<u8>> {
    png.transparency.as_ref().map(|t| match t {
        Transparency::Grayscale(v) => v.to_be_bytes().to_vec(),
        Transparency::Rgb(v) => v.iter().flat_map(|v| v.to_be_bytes()).collect(),
        Transparency::Palette(v) => v.clone(),
    })
}
fn reference_frame(reader: &mut png::Reader<Cursor<&[u8]>>, expected: &Image) {
    // Rust png OR-scatters sub-byte Adam7 pass samples into this allocation.
    let mut output = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut output).unwrap();
    assert_eq!(
        (
            info.width,
            info.height,
            info.color_type as u8,
            info.bit_depth as u8
        ),
        (
            expected.width,
            expected.height,
            expected.color_type as u8,
            expected.bit_depth
        )
    );
    // Rust png scatters interlaced APNG subframes using the canvas stride.
    let stride = if reader.info().interlaced {
        reader.output_line_size(reader.info().width).unwrap()
    } else {
        info.line_size
    };
    let channels = expected.color_type.channels();
    for y in 0..expected.height as usize {
        for x in 0..expected.width as usize * channels {
            let n = y * expected.width as usize * channels + x;
            match &expected.samples {
                Samples::U16(v) => assert_eq!(
                    u16::from_be_bytes([
                        output[y * stride + 2 * x],
                        output[y * stride + 2 * x + 1]
                    ]),
                    v[n]
                ),
                Samples::U8(v) if expected.bit_depth == 8 => {
                    assert_eq!(output[y * stride + x], v[n])
                }
                Samples::U8(v) => {
                    let d = expected.bit_depth as usize;
                    assert_eq!(
                        (output[y * stride + x * d / 8] >> (8 - d - x * d % 8)) & ((1 << d) - 1),
                        v[n]
                    );
                }
            }
        }
    }
}
fn reference(bytes: &[u8], expected: &Png) {
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    assert_eq!(
        reader.info().palette.as_deref().unwrap_or_default(),
        expected.palette.as_flattened()
    );
    // Rust png exposes one byte per gray/RGB key component at depths <=8,
    // rather than the two-byte PNG wire words accepted by its encoder.
    let reference_transparency = transparency(expected).map(|v| {
        if expected.image.bit_depth <= 8 && expected.image.color_type != ColorType::Indexed {
            v.as_chunks::<2>().0.iter().map(|word| word[1]).collect()
        } else {
            v
        }
    });
    assert_eq!(
        reader.info().trns.as_deref(),
        reference_transparency.as_deref()
    );
    reference_frame(&mut reader, &expected.image);
    if let Some(animation) = &expected.animation {
        let actual = reader.info().animation_control.unwrap();
        assert_eq!(
            (actual.num_frames, actual.num_plays),
            (animation.frames.len() as u32, animation.plays)
        );
        for frame in &animation.frames {
            if let Some(image) = &frame.image {
                reference_frame(&mut reader, image);
            }
            let actual = reader.info().frame_control.unwrap();
            let c = frame.control;
            assert_eq!(
                (
                    actual.width,
                    actual.height,
                    actual.x_offset,
                    actual.y_offset,
                    actual.delay_num,
                    actual.delay_den,
                    actual.dispose_op as u8,
                    actual.blend_op as u8
                ),
                (
                    c.width,
                    c.height,
                    c.x_offset,
                    c.y_offset,
                    c.delay_num,
                    c.delay_den,
                    c.dispose as u8,
                    c.blend as u8
                )
            );
        }
    }
    reader.finish().unwrap();
}
fn native(bytes: &[u8], expected: &Png) {
    let result = decode(bytes).unwrap();
    assert_eq!(result.image, expected.image);
    assert_eq!(result.palette, expected.palette);
    assert_eq!(result.transparency, expected.transparency);
    assert_eq!(result.animation, expected.animation);
    reference(bytes, expected);
}
fn reference_encode(original: &Png, filter: usize, stored: bool) -> Vec<u8> {
    let mut output = Vec::new();
    let mut encoder = png::Encoder::new(&mut output, original.image.width, original.image.height);
    encoder.set_color(png::ColorType::from_u8(original.image.color_type as u8).unwrap());
    encoder.set_depth(png::BitDepth::from_u8(original.image.bit_depth).unwrap());
    encoder.set_compression(if stored {
        png::Compression::NoCompression
    } else {
        png::Compression::Balanced
    });
    encoder.set_filter(
        [
            png::Filter::NoFilter,
            png::Filter::Sub,
            png::Filter::Up,
            png::Filter::Avg,
            png::Filter::Paeth,
            png::Filter::Adaptive,
        ][filter],
    );
    if !original.palette.is_empty() {
        encoder.set_palette(original.palette.as_flattened().to_vec());
    }
    if let Some(value) = transparency(original) {
        encoder.set_trns(value);
    }
    if let Some(a) = &original.animation {
        encoder
            .set_animated(a.frames.len() as u32, a.plays)
            .unwrap();
        encoder.set_sep_def_img(!a.default_is_frame).unwrap();
    }
    let mut writer = encoder.write_header().unwrap();
    if let Some(a) = &original.animation {
        if !a.default_is_frame {
            writer.write_image_data(&packed(&original.image)).unwrap();
        }
        for frame in &a.frames {
            let c = frame.control;
            writer.set_frame_dimension(c.width, c.height).unwrap();
            writer.set_frame_position(c.x_offset, c.y_offset).unwrap();
            writer.set_frame_delay(c.delay_num, c.delay_den).unwrap();
            writer
                .set_blend_op(if c.blend == BlendOp::Source {
                    png::BlendOp::Source
                } else {
                    png::BlendOp::Over
                })
                .unwrap();
            writer
                .set_dispose_op(match c.dispose {
                    DisposeOp::None => png::DisposeOp::None,
                    DisposeOp::Background => png::DisposeOp::Background,
                    DisposeOp::Previous => png::DisposeOp::Previous,
                })
                .unwrap();
            writer
                .write_image_data(&packed(frame.image.as_ref().unwrap_or(&original.image)))
                .unwrap();
        }
    } else {
        writer.write_image_data(&packed(&original.image)).unwrap();
    }
    writer.finish().unwrap();
    output
}

#[test]
fn rust_encoder_and_both_native_writers_preserve_static_and_apng_semantics() {
    let mut count = 0;
    for (color, depth) in FORMATS {
        for default_is_frame in [false, true] {
            for dispose in [DisposeOp::None, DisposeOp::Background, DisposeOp::Previous] {
                for blend in [BlendOp::Source, BlendOp::Over] {
                    let mut original =
                        fixture(7, 5, color, depth, dispose as usize + blend as usize);
                    let delay = dispose as usize * 2 + blend as usize;
                    let control = |w, h, x, y| FrameControl {
                        width: w,
                        height: h,
                        x_offset: x,
                        y_offset: y,
                        delay_num: [0, 1, 255, 256, 65535, 0x1234][delay],
                        delay_den: [0, 100, 256, 1, 0xabcd, 65535][delay],
                        dispose,
                        blend,
                    };
                    original.animation = Some(Animation {
                        plays: 0x7fff_ffff,
                        default_is_frame,
                        frames: vec![
                            if default_is_frame {
                                Frame {
                                    control: control(7, 5, 0, 0),
                                    image: None,
                                }
                            } else {
                                Frame {
                                    control: control(3, 2, 0, 0),
                                    image: Some(image(3, 2, color, depth, 13)),
                                }
                            },
                            Frame {
                                control: FrameControl {
                                    delay_num: 0xabcd,
                                    delay_den: 0x1234,
                                    ..control(2, 3, 4, 1)
                                },
                                image: Some(image(2, 3, color, depth, 71)),
                            },
                        ],
                    });
                    for (filter, &kind) in FILTERS.iter().enumerate() {
                        for compression in [Compression::Stored, Compression::Fast] {
                            let independent = reference_encode(
                                &original,
                                filter,
                                compression == Compression::Stored,
                            );
                            native(&independent, &original);
                            for interlaced in [false, true] {
                                let opts = EncodeOptions {
                                    interlaced,
                                    filter: kind,
                                    compression,
                                    idat_chunk_size: 7,
                                };
                                let mut streamed = Vec::new();
                                encode_writer(&original, &opts, &mut streamed).unwrap();
                                for bytes in [encode(&original, &opts).unwrap(), streamed] {
                                    native(&bytes, &original);
                                    count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        let original = fixture(17, 13, color, depth, 1);
        for filter in 0..6 {
            for stored in [false, true] {
                native(&reference_encode(&original, filter, stored), &original);
            }
        }
    }
    assert_eq!(count, 15 * 2 * 3 * 2 * 6 * 2 * 2 * 2);
    println!(
        "Cross-encoder APNG audit: {count} native files, 2160 Rust png animations and 180 static reference encodings; exact native samples, palettes, transparency and controls"
    );
}

#[test]
#[ignore = "build tests/oracle and set DEADLIB_LIBPNG_ENCODER"]
fn c_libpng_encoder_all_formats_filters_interlace_and_compression_levels() {
    let encoder =
        std::env::var_os("DEADLIB_LIBPNG_ENCODER").expect("DEADLIB_LIBPNG_ENCODER required");
    let scratch = std::env::temp_dir().join(format!(
        "deadlib-png-encoder-reference-{}.raw",
        std::process::id()
    ));
    let mut count = 0;
    for (color, depth) in FORMATS {
        for (salt, (width, height)) in [(1, 1), (3, 2), (17, 13)].into_iter().enumerate() {
            let original = fixture(width, height, color, depth, salt);
            for interlaced in [false, true] {
                for (filter, &kind) in FILTERS.iter().enumerate() {
                    for level in [0, 1, 9] {
                        let alpha = transparency(&original);
                        let mut request = [
                            0x4e504731,
                            width,
                            height,
                            u32::from(depth),
                            color as u32,
                            u32::from(interlaced),
                            filter as u32,
                            level,
                            original.palette.len() as u32,
                            alpha.as_ref().map_or(u32::MAX, |v| v.len() as u32),
                        ]
                        .into_iter()
                        .flat_map(u32::to_be_bytes)
                        .collect::<Vec<_>>();
                        request.extend(original.palette.as_flattened());
                        if let Some(v) = alpha {
                            request.extend(v);
                        }
                        request.extend(packed(&original.image));
                        std::fs::write(&scratch, request).unwrap();
                        let result = Command::new(Path::new(&encoder))
                            .arg(&scratch)
                            .output()
                            .unwrap();
                        assert!(
                            result.status.success(),
                            "color={color:?} depth={depth} {width}x{height} Adam7={interlaced} filter={filter} level={level}: {}",
                            String::from_utf8_lossy(&result.stderr)
                        );
                        native(&result.stdout, &original);
                        let opts = EncodeOptions {
                            interlaced,
                            filter: kind,
                            compression: if level == 0 {
                                Compression::Stored
                            } else {
                                Compression::Fast
                            },
                            idat_chunk_size: 7,
                        };
                        let mut streamed = Vec::new();
                        encode_writer(&original, &opts, &mut streamed).unwrap();
                        native(&streamed, &original);
                        native(&encode(&original, &opts).unwrap(), &original);
                        count += 1;
                    }
                }
            }
        }
    }
    std::fs::remove_file(scratch).unwrap();
    assert_eq!(count, 1620);
    println!(
        "C libpng encoder audit: {count} reference files and {} native files; all 15 formats, three geometries, six filters, plain/Adam7, levels 0/1/9, exact samples/palettes/transparency",
        count * 2
    );
}
