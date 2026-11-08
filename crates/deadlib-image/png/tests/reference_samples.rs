use deadlib_image_png::{ColorType, Error, Image, Png, Samples};

const FORMATS: [(ColorType, usize); 4] = [
    (ColorType::Grayscale, 1),
    (ColorType::Rgb, 3),
    (ColorType::GrayscaleAlpha, 2),
    (ColorType::Rgba, 4),
];

fn declarations() -> Vec<(ColorType, Vec<u8>)> {
    let mut result: Vec<_> = FORMATS
        .into_iter()
        .flat_map(|(color, channels)| (1..=16).map(move |n| (color, vec![n; channels])))
        .collect();
    result.extend([
        (ColorType::Rgb, vec![5, 6, 5]),
        (ColorType::Rgb, vec![7, 9, 11]),
        (ColorType::Rgb, vec![16, 1, 15]),
        (ColorType::GrayscaleAlpha, vec![12, 1]),
        (ColorType::GrayscaleAlpha, vec![1, 16]),
        (ColorType::Rgba, vec![10, 10, 10, 2]),
        (ColorType::Rgba, vec![5, 6, 5, 1]),
        (ColorType::Rgba, vec![16, 8, 4, 1]),
    ]);
    result
}

fn source(width: u32, height: u32, color: ColorType, bits: &[u8]) -> (Png, Vec<u16>) {
    let original: Vec<u16> = (0..width as usize * height as usize)
        .flat_map(|p| {
            bits.iter().enumerate().map(move |(c, &n)| {
                let maximum = (1u32 << n) - 1;
                (match (p + c) % 7 {
                    0 => 0,
                    1 => maximum,
                    2 => maximum - 1,
                    3 => maximum / 2,
                    _ => (p as u32 * 137 + c as u32 * 93) & maximum,
                }) as u16
            })
        })
        .collect();
    let png =
        Png::from_reference_samples(width, height, color, bits, Samples::U16(original.clone()))
            .unwrap();
    (png, original)
}

fn reference_frame(reader: &mut png::Reader<std::io::Cursor<&[u8]>>, image: &Image) {
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    assert_eq!(
        (
            info.width,
            info.height,
            info.color_type as u8,
            info.bit_depth as u8
        ),
        (
            image.width,
            image.height,
            image.color_type as u8,
            image.bit_depth
        )
    );
    let stride = if reader.info().interlaced {
        reader.output_line_size(reader.info().width).unwrap()
    } else {
        info.line_size
    };
    let expected = values(&image.samples);
    let per_row = image.width as usize * image.color_type.channels();
    for y in 0..image.height as usize {
        for n in 0..per_row {
            let actual = match image.bit_depth {
                16 => {
                    u16::from_be_bytes([bytes[y * stride + n * 2], bytes[y * stride + n * 2 + 1]])
                }
                8 => u16::from(bytes[y * stride + n]),
                depth => u16::from(
                    (bytes[y * stride + n * depth as usize / 8]
                        >> (8 - depth - (n * depth as usize % 8) as u8))
                        & ((1 << depth) - 1),
                ),
            };
            assert_eq!(actual, expected[y * per_row + n]);
        }
    }
}

struct Fragments<'a>(&'a [u8]);
impl std::io::Read for Fragments<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = out.len().min(self.0.len()).min(7);
        out[..n].copy_from_slice(&self.0[..n]);
        self.0 = &self.0[n..];
        Ok(n)
    }
}

#[test]
fn both_writers_all_source_precisions_filters_and_interlace_recover_original_samples() {
    use deadlib_image_png::{
        EncodeOptions, Filter, decode, decode_reader, encode, encode_writer, zlib::Compression,
    };
    let mut files = 0;
    for (color, bits) in declarations() {
        let (mut png, original) = source(17, 9, color, &bits);
        for interlaced in [false, true] {
            png.interlaced = interlaced;
            for compression in [Compression::Stored, Compression::Fast] {
                for filter in [
                    Filter::None,
                    Filter::Sub,
                    Filter::Up,
                    Filter::Average,
                    Filter::Paeth,
                    Filter::Adaptive,
                ] {
                    let options = EncodeOptions {
                        interlaced,
                        compression,
                        filter,
                        idat_chunk_size: 7,
                    };
                    let mut writer = Vec::new();
                    encode_writer(&png, &options, &mut writer).unwrap();
                    for bytes in [encode(&png, &options).unwrap(), writer] {
                        let decoded = decode(&bytes).unwrap();
                        assert_eq!(decoded, png);
                        assert_eq!(
                            decode_reader(Fragments(&bytes), &Default::default()).unwrap(),
                            png
                        );
                        check(&decoded, color, &bits, &original);
                        let mut reference =
                            png::Decoder::new(std::io::Cursor::new(bytes.as_slice()))
                                .read_info()
                                .unwrap();
                        assert_eq!(
                            reference.info().sbit.as_deref(),
                            png.ancillary.first().map(|c| c.data.as_slice())
                        );
                        reference_frame(&mut reference, &png.image);
                        reference.finish().unwrap();
                        files += 1;
                    }
                }
            }
        }
    }
    assert_eq!(files, 3456);
}

#[test]
fn converted_apng_frames_share_precision_and_preserve_reference_values() {
    use deadlib_image_png::{
        Animation, BlendOp, DisposeOp, EncodeOptions, Frame, FrameControl, decode, decode_reader,
        encode, encode_writer, zlib::Compression,
    };
    let mut files = 0;
    for (color, bits) in declarations() {
        let (base, original) = source(17, 9, color, &bits);
        let (rectangle, frame_original) = source(5, 4, color, &bits);
        for default_is_frame in [false, true] {
            let mut png = base.clone();
            let mut frames = Vec::new();
            if default_is_frame {
                frames.push(Frame {
                    control: FrameControl {
                        width: 17,
                        height: 9,
                        x_offset: 0,
                        y_offset: 0,
                        delay_num: 1,
                        delay_den: 60,
                        dispose: DisposeOp::None,
                        blend: BlendOp::Source,
                    },
                    image: None,
                });
            }
            frames.push(Frame {
                control: FrameControl {
                    width: 5,
                    height: 4,
                    x_offset: 3,
                    y_offset: 2,
                    delay_num: 1,
                    delay_den: 60,
                    dispose: DisposeOp::Previous,
                    blend: BlendOp::Over,
                },
                image: Some(rectangle.image.clone()),
            });
            png.animation = Some(Animation {
                plays: 3,
                default_is_frame,
                frames,
            });
            for interlaced in [false, true] {
                png.interlaced = interlaced;
                for compression in [Compression::Stored, Compression::Fast] {
                    let options = EncodeOptions {
                        interlaced,
                        compression,
                        ..Default::default()
                    };
                    let mut writer = Vec::new();
                    encode_writer(&png, &options, &mut writer).unwrap();
                    for bytes in [encode(&png, &options).unwrap(), writer] {
                        let decoded = decode(&bytes).unwrap();
                        assert_eq!(decoded, png);
                        assert_eq!(
                            decode_reader(Fragments(&bytes), &Default::default()).unwrap(),
                            png
                        );
                        check(&decoded, color, &bits, &original);
                        let frame = decoded
                            .animation
                            .as_ref()
                            .unwrap()
                            .frames
                            .last()
                            .unwrap()
                            .image
                            .as_ref()
                            .unwrap();
                        assert_eq!(frame, &rectangle.image);
                        check(&rectangle, color, &bits, &frame_original);
                        let mut reference =
                            png::Decoder::new(std::io::Cursor::new(bytes.as_slice()))
                                .read_info()
                                .unwrap();
                        assert_eq!(
                            reference.info().sbit.as_deref(),
                            base.ancillary.first().map(|c| c.data.as_slice())
                        );
                        reference_frame(&mut reference, &png.image);
                        reference_frame(&mut reference, &rectangle.image);
                        reference.finish().unwrap();
                        files += 1;
                    }
                }
            }
        }
    }
    assert_eq!(files, 1152);
}

#[test]
#[ignore = "build tests/oracle and set DEADLIB_LIBPNG_ORACLE"]
fn libpng_native_and_sbit_shift_recover_exact_reference_samples() {
    use deadlib_image_png::{EncodeOptions, encode, encode_writer, zlib::Compression};
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let oracle =
        std::env::var_os("DEADLIB_LIBPNG_ORACLE").expect("DEADLIB_LIBPNG_ORACLE is required");
    let path = std::env::temp_dir().join(format!(
        "deadlib-reference-samples-{}-{}.png",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut files = 0;
    for (color, bits) in declarations() {
        let (mut png, original) = source(17, 9, color, &bits);
        for interlaced in [false, true] {
            png.interlaced = interlaced;
            for compression in [Compression::Stored, Compression::Fast] {
                let options = EncodeOptions {
                    interlaced,
                    compression,
                    ..Default::default()
                };
                let mut writer = Vec::new();
                encode_writer(&png, &options, &mut writer).unwrap();
                for bytes in [encode(&png, &options).unwrap(), writer] {
                    fs::write(&path, bytes).unwrap();
                    for shift in [false, true] {
                        let mut command = Command::new(&oracle);
                        if shift {
                            command.arg("--reference-samples");
                        }
                        let result = command.arg(&path).output().unwrap();
                        assert!(
                            result.status.success(),
                            "{}",
                            String::from_utf8_lossy(&result.stderr)
                        );
                        let mut data = result.stdout.as_slice();
                        let mut word = || {
                            let value = u32::from_be_bytes(data[..4].try_into().unwrap());
                            data = &data[4..];
                            value
                        };
                        assert_eq!(
                            (word(), word(), word(), word()),
                            (17, 9, u32::from(png.image.bit_depth), color as u32)
                        );
                        if shift {
                            let count = word() as usize;
                            let expected =
                                png.ancillary.first().map_or(&[][..], |c| c.data.as_slice());
                            assert_eq!(&data[..count], expected);
                            data = &data[count..];
                        }
                        let depth = png.image.bit_depth as usize;
                        let per_row = 17 * color.channels();
                        let stride = (per_row * depth).div_ceil(8);
                        assert_eq!(data.len(), stride * 9);
                        let expected = if shift {
                            original.clone()
                        } else {
                            values(&png.image.samples)
                        };
                        for y in 0..9 {
                            for n in 0..per_row {
                                let actual = match depth {
                                    16 => u16::from_be_bytes([
                                        data[y * stride + n * 2],
                                        data[y * stride + n * 2 + 1],
                                    ]),
                                    8 => u16::from(data[y * stride + n]),
                                    _ => u16::from(
                                        (data[y * stride + n * depth / 8]
                                            >> (8 - depth - n * depth % 8))
                                            & ((1 << depth) - 1),
                                    ),
                                };
                                assert_eq!(
                                    actual,
                                    expected[y * per_row + n],
                                    "{color:?} {bits:?} shift={shift}"
                                );
                            }
                        }
                    }
                    files += 1;
                }
            }
        }
    }
    fs::remove_file(path).unwrap();
    assert_eq!(files, 576);
    println!(
        "libpng reference scaling: 576 exact stored-sample files and 576 exact sBIT/unshift recoveries"
    );
}

fn target(color: ColorType, bits: &[u8]) -> u8 {
    let largest = bits.iter().copied().max().unwrap();
    match (color, largest) {
        (ColorType::Grayscale, 1) => 1,
        (ColorType::Grayscale, 2) => 2,
        (ColorType::Grayscale, 3 | 4) => 4,
        (_, 1..=8) => 8,
        _ => 16,
    }
}

fn rounded(value: u16, source: u8, destination: u8) -> u16 {
    // Independent doubled rational expression, evaluated in u64.
    let denominator = (1u64 << source) - 1;
    let numerator = u64::from(value) * ((1u64 << destination) - 1);
    ((numerator * 2 + denominator) / (denominator * 2)) as u16
}

fn values(samples: &Samples) -> Vec<u16> {
    match samples {
        Samples::U8(v) => v.iter().map(|&v| u16::from(v)).collect(),
        Samples::U16(v) => v.clone(),
    }
}

fn check(png: &Png, color: ColorType, bits: &[u8], original: &[u16]) {
    let depth = target(color, bits);
    assert_eq!(png.image.color_type, color);
    assert_eq!(png.image.bit_depth, depth);
    png.image.validate().unwrap();
    let stored = values(&png.image.samples);
    assert_eq!(stored.len(), original.len());
    for (n, (&actual, &input)) in stored.iter().zip(original).enumerate() {
        let source = bits[n % bits.len()];
        assert_eq!(actual, rounded(input, source, depth));
        assert_eq!(actual >> (depth - source), input);
        if input == 0 {
            assert_eq!(actual, 0);
        }
        if u32::from(input) == (1u32 << source) - 1 {
            assert_eq!(u32::from(actual), (1u32 << depth) - 1);
        }
    }
    if bits.iter().all(|&n| n == depth) {
        assert!(png.ancillary.is_empty());
    } else {
        assert_eq!(png.ancillary.len(), 1);
        assert_eq!(png.ancillary[0].kind, *b"sBIT");
        assert_eq!(png.ancillary[0].data, bits);
        assert_eq!(
            png.ancillary[0].position,
            deadlib_image_png::ChunkPosition::BeforePalette
        );
    }
}

#[test]
fn every_uniform_source_value_and_container_preserves_high_bits_and_endpoints() {
    let mut samples_checked = 0usize;
    for (color, channels) in FORMATS {
        for source in 1..=16 {
            let maximum = (1usize << source) - 1;
            let bits = vec![source; channels];
            let original: Vec<u16> = (0..=maximum)
                .flat_map(|v| (0..channels).map(move |c| ((v + c * 31) & maximum) as u16))
                .collect();
            let png = Png::from_reference_samples(
                (maximum + 1) as u32,
                1,
                color,
                &bits,
                Samples::U16(original.clone()),
            )
            .unwrap();
            check(&png, color, &bits, &original);
            samples_checked += original.len();
            let count = (maximum + 1).min(256);
            let narrow: Vec<u8> = (0..count)
                .flat_map(|v| (0..channels).map(move |c| ((v + c * 31) & maximum) as u8))
                .collect();
            let original: Vec<u16> = narrow.iter().map(|&v| u16::from(v)).collect();
            let png =
                Png::from_reference_samples(count as u32, 1, color, &bits, Samples::U8(narrow))
                    .unwrap();
            check(&png, color, &bits, &original);
        }
    }
    assert_eq!(samples_checked, 1_310_700);
}

#[test]
fn every_mixed_channel_depth_declaration_uses_the_smallest_legal_depth() {
    let mut declarations = 0;
    for (color, channels) in FORMATS {
        for code in 0..16usize.pow(channels as u32) {
            let bits: Vec<u8> = (0..channels)
                .map(|n| ((code >> (n * 4)) & 15) as u8 + 1)
                .collect();
            let original: Vec<u16> = (0..5)
                .flat_map(|pattern| {
                    let bits = &bits;
                    (0..channels).map(move |c| {
                        let maximum = (1u32 << bits[c]) - 1;
                        (match pattern {
                            0 => 0,
                            1 => maximum,
                            2 => maximum / 2,
                            3 => maximum - 1,
                            _ => (code as u32 + c as u32 * 913) % (maximum + 1),
                        }) as u16
                    })
                })
                .collect();
            let png =
                Png::from_reference_samples(5, 1, color, &bits, Samples::U16(original.clone()))
                    .unwrap();
            check(&png, color, &bits, &original);
            declarations += 1;
        }
    }
    assert_eq!(declarations, 69_904);
}

#[test]
fn each_low_precision_value_is_exact_when_another_channel_requires_sixteen_bits() {
    for source in 1..16 {
        let original: Vec<u16> = (0..1u32 << source)
            .flat_map(|v| [v as u16, (v * 379) as u16, 0])
            .collect();
        let bits = [source, 16, 16];
        let png = Png::from_reference_samples(
            1u32 << source,
            1,
            ColorType::Rgb,
            &bits,
            Samples::U16(original.clone()),
        )
        .unwrap();
        check(&png, ColorType::Rgb, &bits, &original);
    }
}

#[test]
fn native_containers_are_reused_and_invalid_reference_inputs_are_rejected() {
    for bits in [[5, 6, 5, 1], [12, 13, 14, 16]] {
        let original: Vec<u16> = bits.iter().map(|&n| ((1u32 << n) - 1) as u16).collect();
        if bits[0] <= 8 {
            let input: Vec<u8> = original.iter().map(|&v| v as u8).collect();
            let pointer = input.as_ptr();
            let png = Png::from_reference_samples(1, 1, ColorType::Rgba, &bits, Samples::U8(input))
                .unwrap();
            let Samples::U8(output) = &png.image.samples else {
                panic!("wrong sample container")
            };
            assert_eq!(pointer, output.as_ptr());
            check(&png, ColorType::Rgba, &bits, &original);
        } else {
            let input = original.clone();
            let pointer = input.as_ptr();
            let png =
                Png::from_reference_samples(1, 1, ColorType::Rgba, &bits, Samples::U16(input))
                    .unwrap();
            let Samples::U16(output) = &png.image.samples else {
                panic!("wrong sample container")
            };
            assert_eq!(pointer, output.as_ptr());
            check(&png, ColorType::Rgba, &bits, &original);
        }
    }
    for (color, channels) in FORMATS {
        for length in 0..=5 {
            if length != channels {
                assert!(
                    Png::from_reference_samples(
                        1,
                        1,
                        color,
                        &vec![8; length],
                        Samples::U8(vec![0; channels])
                    )
                    .is_err()
                );
            }
        }
        for channel in 0..channels {
            for invalid in [0].into_iter().chain(17..=255) {
                let mut bits = vec![8; channels];
                bits[channel] = invalid;
                assert!(
                    Png::from_reference_samples(1, 1, color, &bits, Samples::U8(vec![0; channels]))
                        .is_err()
                );
            }
            for source in 1..16 {
                let bits = vec![source; channels];
                let mut input = vec![0; channels];
                input[channel] = 1u16 << source;
                assert_eq!(
                    Png::from_reference_samples(1, 1, color, &bits, Samples::U16(input)),
                    Err(Error::Invalid("reference sample range"))
                );
            }
        }
        for count in [0, channels - 1, channels + 1] {
            assert!(
                Png::from_reference_samples(
                    1,
                    1,
                    color,
                    &vec![8; channels],
                    Samples::U8(vec![0; count])
                )
                .is_err()
            );
        }
        for (width, height) in [(0, 1), (1, 0), (0x8000_0000, 1), (1, u32::MAX)] {
            assert!(
                Png::from_reference_samples(
                    width,
                    height,
                    color,
                    &vec![8; channels],
                    Samples::U8(vec![0; channels])
                )
                .is_err()
            );
        }
    }
    assert!(
        Png::from_reference_samples(1, 1, ColorType::Indexed, &[8], Samples::U8(vec![0])).is_err()
    );
    // Indexed PNGs retain their separate, unscaled native-index contract.
    let mut indexed = Png::new(Image {
        width: 1,
        height: 1,
        color_type: ColorType::Indexed,
        bit_depth: 4,
        samples: Samples::U8(vec![7]),
    });
    indexed.palette = vec![[0, 0, 0]; 8];
    assert_eq!(
        deadlib_image_png::decode(
            &deadlib_image_png::encode(&indexed, &Default::default()).unwrap()
        )
        .unwrap(),
        indexed
    );
}
