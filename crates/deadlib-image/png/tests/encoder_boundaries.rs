//! Native input validation, APNG wire controls and every writer failure offset.
#[allow(dead_code)]
#[path = "support/chunks.rs"]
mod chunks;
#[allow(dead_code)]
#[path = "support/scalar_fields.rs"]
mod scalar;
#[allow(dead_code)]
#[path = "support/apng_recovery.rs"]
mod wire;
use deadlib_image_png::{
    Animation, BlendOp, ColorType, DisposeOp, EncodeOptions, Error, Filter, Frame, FrameControl,
    Image, Png, Samples, encode, encode_writer, zlib::Compression,
};
use std::io::{self, Write};

fn animation(color: ColorType, depth: u8, default_is_frame: bool) -> Png {
    let image = |width, height| Image {
        width,
        height,
        color_type: color,
        bit_depth: depth,
        samples: if depth == 16 {
            Samples::U16(vec![0; width as usize * height as usize * color.channels()])
        } else {
            Samples::U8(vec![0; width as usize * height as usize * color.channels()])
        },
    };
    let control = |width, height, x_offset, y_offset| FrameControl {
        width,
        height,
        x_offset,
        y_offset,
        delay_num: 0x1234,
        delay_den: 0xabcd,
        dispose: DisposeOp::Previous,
        blend: BlendOp::Over,
    };
    let mut png = Png::new(image(3, 2));
    if color == ColorType::Indexed {
        png.palette = vec![[0; 3]];
    }
    png.animation = Some(Animation {
        plays: 7,
        default_is_frame,
        frames: vec![
            if default_is_frame {
                Frame {
                    control: control(3, 2, 0, 0),
                    image: None,
                }
            } else {
                Frame {
                    control: control(1, 1, 0, 0),
                    image: Some(image(1, 1)),
                }
            },
            Frame {
                control: control(1, 1, 1, 1),
                image: Some(image(1, 1)),
            },
        ],
    });
    png
}
fn opts(interlaced: bool) -> EncodeOptions {
    EncodeOptions {
        interlaced,
        compression: Compression::Stored,
        filter: Filter::None,
        idat_chunk_size: 7,
    }
}
fn reject(png: &Png) {
    assert!(matches!(encode(png, &opts(false)), Err(Error::Invalid(_))));
    let mut output = Vec::new();
    assert!(matches!(
        encode_writer(png, &opts(false), &mut output),
        Err(Error::Invalid(_))
    ));
    assert!(output.is_empty());
}

// Parse literal framing and independently inflate each complete frame with
// flate2. This checker invokes no production PNG decoder or serializer.
fn known_wire(bytes: &[u8], source: &Png, interlaced: bool) {
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    let a = source.animation.as_ref().unwrap();
    let mut offset = 8;
    let (mut sequence, mut frame, mut seen_data, mut ended_data, mut saw_control) =
        (0u32, 0usize, false, false, false);
    let mut compressed = vec![Vec::new()];
    while offset < bytes.len() {
        let size = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let kind = &bytes[offset + 4..offset + 8];
        let data = &bytes[offset + 8..offset + 8 + size];
        assert_eq!(
            chunks::crc(&bytes[offset + 4..offset + 8 + size]),
            u32::from_be_bytes(
                bytes[offset + 8 + size..offset + 12 + size]
                    .try_into()
                    .unwrap()
            )
        );
        if seen_data && kind != b"IDAT" {
            ended_data = true;
        }
        match kind {
            b"IHDR" => {
                assert_eq!(offset, 8);
                assert_eq!(
                    data,
                    [
                        0,
                        0,
                        0,
                        3,
                        0,
                        0,
                        0,
                        2,
                        source.image.bit_depth,
                        source.image.color_type as u8,
                        0,
                        0,
                        u8::from(interlaced)
                    ]
                );
            }
            b"PLTE" => assert_eq!(data, source.palette.as_flattened()),
            b"acTL" => {
                assert!(!seen_data && !saw_control);
                assert_eq!(data, [2u32.to_be_bytes(), a.plays.to_be_bytes()].concat());
                saw_control = true;
            }
            b"fcTL" => {
                assert!(saw_control);
                let c = a.frames[frame].control;
                let mut expected = [sequence, c.width, c.height, c.x_offset, c.y_offset]
                    .into_iter()
                    .flat_map(u32::to_be_bytes)
                    .collect::<Vec<_>>();
                expected.extend(c.delay_num.to_be_bytes());
                expected.extend(c.delay_den.to_be_bytes());
                expected.extend([c.dispose as u8, c.blend as u8]);
                assert_eq!(data, expected);
                sequence += 1;
                if frame != 0 || !a.default_is_frame {
                    assert!(seen_data);
                    compressed.push(Vec::new());
                } else {
                    assert!(!seen_data);
                }
                frame += 1;
            }
            b"IDAT" => {
                assert!(!ended_data);
                seen_data = true;
                compressed[0].extend_from_slice(data);
            }
            b"fdAT" => {
                assert!(seen_data && frame != 0);
                assert_eq!(&data[..4], sequence.to_be_bytes());
                sequence += 1;
                compressed.last_mut().unwrap().extend_from_slice(&data[4..]);
            }
            b"IEND" => {
                assert!(data.is_empty());
                assert_eq!(offset + 12, bytes.len());
            }
            _ => panic!("unexpected chunk {kind:?}"),
        }
        offset += size + 12;
    }
    assert_eq!(frame, a.frames.len());
    let images = std::iter::once(&source.image)
        .chain(a.frames.iter().filter_map(|f| f.image.as_ref()))
        .collect::<Vec<_>>();
    assert_eq!(compressed.len(), images.len());
    for (bytes, image) in compressed.iter().zip(images) {
        let expected = wire::filtered(image, interlaced);
        let mut actual = Vec::with_capacity(expected.len() + 1);
        let mut inflater = flate2::Decompress::new(true);
        assert_eq!(
            inflater
                .decompress_vec(bytes, &mut actual, flate2::FlushDecompress::Finish)
                .unwrap(),
            flate2::Status::StreamEnd
        );
        assert_eq!(inflater.total_in() as usize, bytes.len());
        assert_eq!(actual, expected);
    }
}

#[test]
fn native_image_animation_and_frame_storage_errors_reject_before_output() {
    let mut count = 0;
    for (color, depth) in scalar::FORMATS {
        for default_is_frame in [false, true] {
            let original = animation(color, depth, default_is_frame);
            let mut invalid = Vec::new();
            for plays in [0x8000_0000, u32::MAX] {
                let mut p = original.clone();
                p.animation.as_mut().unwrap().plays = plays;
                invalid.push(p);
            }
            let mut p = original.clone();
            p.animation.as_mut().unwrap().frames.clear();
            invalid.push(p);
            for dimension in [0, 1, 0x7fff_ffff, 0x8000_0000, u32::MAX] {
                let mut p = original.clone();
                p.image.width = dimension;
                invalid.push(p);
                let mut p = original.clone();
                p.image.height = dimension;
                invalid.push(p);
            }
            for frame in 0..2 {
                let mut p = original.clone();
                p.animation.as_mut().unwrap().frames[frame].image =
                    if frame == 0 && default_is_frame {
                        Some(p.image.clone())
                    } else {
                        None
                    };
                invalid.push(p);
                if frame == 0 && default_is_frame {
                    continue;
                }
                for field in 0..5 {
                    let mut p = original.clone();
                    let image = p.animation.as_mut().unwrap().frames[frame]
                        .image
                        .as_mut()
                        .unwrap();
                    match field {
                        0 => image.width = 2,
                        1 => image.height = 2,
                        2 => image.bit_depth = if depth == 16 { 8 } else { 16 },
                        3 => {
                            image.color_type = if color == ColorType::Rgba {
                                ColorType::Rgb
                            } else {
                                ColorType::Rgba
                            }
                        }
                        _ => {
                            image.samples = if depth == 16 {
                                Samples::U8(vec![0; color.channels()])
                            } else {
                                Samples::U16(vec![0; color.channels()])
                            }
                        }
                    }
                    invalid.push(p);
                }
                if color == ColorType::Indexed {
                    let mut p = original.clone();
                    p.animation.as_mut().unwrap().frames[frame]
                        .image
                        .as_mut()
                        .unwrap()
                        .samples = Samples::U8(vec![1]);
                    invalid.push(p);
                }
            }
            for p in invalid {
                reject(&p);
                count += 1;
            }
        }
    }
    println!(
        "Encoder image/frame validation: {count} malformed native inputs, both writers reject before output"
    );
}

#[test]
fn frame_rectangles_and_integer_edges_have_literal_controls_and_exact_frame_streams() {
    let values = [
        0,
        1,
        2,
        3,
        4,
        255,
        256,
        65535,
        65536,
        0x7fff_ffff,
        0x8000_0000,
        u32::MAX,
    ];
    let (mut accepted, mut rejected) = (0, 0);
    for (color, depth) in scalar::FORMATS {
        for default_is_frame in [false, true] {
            let original = animation(color, depth, default_is_frame);
            for frame in 0..2 {
                for field in 0..4 {
                    for value in values {
                        let mut p = original.clone();
                        let f = &mut p.animation.as_mut().unwrap().frames[frame];
                        match field {
                            0 => f.control.width = value,
                            1 => f.control.height = value,
                            2 => f.control.x_offset = value,
                            _ => f.control.y_offset = value,
                        }
                        let c = f.control;
                        let valid = c.width > 0
                            && c.height > 0
                            && u64::from(c.x_offset) + u64::from(c.width) <= 3
                            && u64::from(c.y_offset) + u64::from(c.height) <= 2
                            && (!(frame == 0 && default_is_frame)
                                || (c.width == 3
                                    && c.height == 2
                                    && c.x_offset == 0
                                    && c.y_offset == 0));
                        if valid {
                            if let Some(image) = &mut f.image {
                                image.width = c.width;
                                image.height = c.height;
                                let size = c.width as usize * c.height as usize * color.channels();
                                image.samples = if depth == 16 {
                                    Samples::U16(vec![0; size])
                                } else {
                                    Samples::U8(vec![0; size])
                                };
                            }
                            for interlaced in [false, true] {
                                let o = opts(interlaced);
                                let mut bytes = Vec::new();
                                encode_writer(&p, &o, &mut bytes).unwrap();
                                known_wire(&bytes, &p, interlaced);
                                known_wire(&encode(&p, &o).unwrap(), &p, interlaced);
                            }
                            accepted += 1;
                        } else {
                            reject(&p);
                            rejected += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(accepted + rejected, 30 * 2 * 4 * values.len());
    println!(
        "Encoder APNG rectangle audit: {accepted} valid / {rejected} invalid inputs, both default roles and all native formats; both writers, plain/Adam7, independent CRC/control/zlib answers"
    );
}

struct FaultWriter {
    bytes: Vec<u8>,
    at: usize,
    fragment: usize,
    calls: usize,
}
impl Write for FaultWriter {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        self.calls += 1;
        if self.calls.is_multiple_of(4) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if self.bytes.len() == self.at {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let n = input
            .len()
            .min(self.fragment)
            .min(self.at - self.bytes.len());
        self.bytes.extend_from_slice(&input[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        panic!("encoder must leave destination flushing to the caller")
    }
}
#[test]
fn every_output_byte_failure_short_writes_and_invalid_writer_counts() {
    let mut count = 0;
    for default_is_frame in [false, true] {
        for interlaced in [false, true] {
            for compression in [Compression::Stored, Compression::Fast] {
                let source = animation(ColorType::Rgba, 16, default_is_frame);
                let o = EncodeOptions {
                    compression,
                    ..opts(interlaced)
                };
                let mut expected = Vec::new();
                encode_writer(&source, &o, &mut expected).unwrap();
                for at in 0..=expected.len() {
                    let mut writer = FaultWriter {
                        bytes: Vec::new(),
                        at,
                        fragment: 1 + at % 7,
                        calls: 0,
                    };
                    let result = encode_writer(&source, &o, &mut writer);
                    assert_eq!(writer.bytes, expected[..at]);
                    if at == expected.len() {
                        assert_eq!(result.unwrap().bytes_written, at);
                    } else {
                        assert!(matches!(result, Err(Error::Io(_))));
                    }
                    count += 1;
                }
            }
        }
    }
    struct Oversized;
    impl Write for Oversized {
        fn write(&mut self, input: &[u8]) -> io::Result<usize> {
            Ok(input.len() + 1)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    assert!(matches!(
        encode_writer(
            &animation(ColorType::Rgb, 8, false),
            &opts(false),
            Oversized
        ),
        Err(Error::Io(_))
    ));
    println!(
        "Encoder writer audit: {count} exact prefix/failure offsets with short and Interrupted writes; invalid byte counts rejected"
    );
}
