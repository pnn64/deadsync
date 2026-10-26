use deadlib_image_png::{
    Animation, BlendOp, ColorType, DisposeOp, EncodeOptions, Frame, FrameControl, Image, Png,
    Samples, decode, encode,
};
use std::io::Cursor;

fn control(width: u32, height: u32, x_offset: u32, y_offset: u32) -> FrameControl {
    FrameControl {
        width,
        height,
        x_offset,
        y_offset,
        delay_num: 7,
        delay_den: 0,
        dispose: DisposeOp::None,
        blend: BlendOp::Source,
    }
}
fn image(w: u32, h: u32, depth: u8, color: ColorType, seed: usize) -> Image {
    let n = w as usize * h as usize * color.channels();
    let samples = if depth == 16 {
        Samples::U16((0..n).map(|n| (n * 37139 + seed) as u16).collect())
    } else {
        Samples::U8(
            (0..n)
                .map(|n| ((n * 19 + seed) % (1usize << depth)) as u8)
                .collect(),
        )
    };
    Image {
        width: w,
        height: h,
        color_type: color,
        bit_depth: depth,
        samples,
    }
}
fn check_reference(bytes: &[u8], expected: &Image, reader: &mut png::Reader<Cursor<&[u8]>>) {
    let _ = bytes;
    let mut output = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut output).unwrap();
    // png 0.18.1 scatters interlaced APNG rows at the canvas stride, although
    // OutputInfo reports the frame stride. Normalize that reference layout.
    if reader.info().interlaced {
        let stride = reader.output_line_size(reader.info().width).unwrap();
        output = (0..info.height as usize)
            .flat_map(|y| {
                output[y * stride..y * stride + info.line_size]
                    .iter()
                    .copied()
            })
            .collect();
    }
    assert_eq!(info.width, expected.width);
    assert_eq!(info.height, expected.height);
    match &expected.samples {
        Samples::U16(v) => assert_eq!(
            &output[..info.buffer_size()],
            v.iter().flat_map(|v| v.to_be_bytes()).collect::<Vec<_>>()
        ),
        Samples::U8(v) if expected.bit_depth == 8 => assert_eq!(&output[..info.buffer_size()], v),
        Samples::U8(v) => {
            let depth = expected.bit_depth as usize;
            let width = expected.width as usize;
            let stride = (width * depth).div_ceil(8);
            for (n, &sample) in v.iter().enumerate() {
                let (y, x) = (n / width, n % width);
                assert_eq!(
                    sample,
                    (output[y * stride + x * depth / 8] >> (8 - depth - x * depth % 8))
                        & ((1 << depth) - 1),
                    "frame {info:?}, depth={depth}, sample={n}, output={output:?}"
                );
            }
        }
    }
}

#[test]
fn all_formats_apng_default_roles_and_interlace() {
    for color in [
        ColorType::Grayscale,
        ColorType::Rgb,
        ColorType::Indexed,
        ColorType::GrayscaleAlpha,
        ColorType::Rgba,
    ] {
        for depth in [1, 2, 4, 8, 16] {
            if !color.allows_depth(depth) {
                continue;
            }
            for default_is_frame in [false, true] {
                for interlaced in [false, true] {
                    let mut original = Png::new(image(9, 7, depth, color, 1));
                    if color == ColorType::Indexed {
                        original.palette = (0..1usize << depth)
                            .map(|n| [n as u8, n as u8, n as u8])
                            .collect();
                    }
                    let mut frames = Vec::new();
                    if default_is_frame {
                        frames.push(Frame {
                            control: control(9, 7, 0, 0),
                            image: None,
                        });
                    }
                    for n in 0..3 {
                        let mut c = control(3, 2, n, n);
                        c.dispose = [DisposeOp::None, DisposeOp::Background, DisposeOp::Previous]
                            [n as usize];
                        c.blend = if n == 1 {
                            BlendOp::Over
                        } else {
                            BlendOp::Source
                        };
                        frames.push(Frame {
                            control: c,
                            image: Some(image(3, 2, depth, color, n as usize + 17)),
                        });
                    }
                    original.animation = Some(Animation {
                        plays: 3,
                        default_is_frame,
                        frames,
                    });
                    let bytes = encode(
                        &original,
                        &EncodeOptions {
                            interlaced,
                            idat_chunk_size: 1,
                            ..Default::default()
                        },
                    )
                    .unwrap();
                    let actual = decode(&bytes).unwrap();
                    assert_eq!(
                        deadlib_image_png::decode_reader(bytes.as_slice(), &Default::default())
                            .unwrap(),
                        actual
                    );
                    assert_eq!(actual.image, original.image);
                    assert_eq!(actual.animation, original.animation);
                    let mut reader = png::Decoder::new(Cursor::new(bytes.as_slice()))
                        .read_info()
                        .unwrap();
                    assert_eq!(
                        reader.info().animation_control.unwrap().num_frames,
                        original.animation.as_ref().unwrap().frames.len() as u32
                    );
                    check_reference(&bytes, &original.image, &mut reader);
                    for f in &original.animation.as_ref().unwrap().frames {
                        if let Some(i) = &f.image {
                            check_reference(&bytes, i, &mut reader);
                        }
                    }
                    reader.finish().unwrap();
                    for compression in [
                        deadlib_image_png::zlib::Compression::Stored,
                        deadlib_image_png::zlib::Compression::Fast,
                    ] {
                        let mut streamed = Vec::new();
                        deadlib_image_png::encode_writer(
                            &original,
                            &EncodeOptions {
                                interlaced,
                                compression,
                                idat_chunk_size: 1,
                                ..Default::default()
                            },
                            &mut streamed,
                        )
                        .unwrap();
                        let actual = decode(&streamed).unwrap();
                        assert_eq!(actual.image, original.image);
                        assert_eq!(actual.animation, original.animation);
                        assert_eq!(
                            deadlib_image_png::decode_reader(
                                streamed.as_slice(),
                                &Default::default()
                            )
                            .unwrap(),
                            actual
                        );
                        let mut reader = png::Decoder::new(Cursor::new(streamed.as_slice()))
                            .read_info()
                            .unwrap();
                        check_reference(&streamed, &original.image, &mut reader);
                        for f in &original.animation.as_ref().unwrap().frames {
                            if let Some(image) = &f.image {
                                check_reference(&streamed, image, &mut reader);
                            }
                        }
                        reader.finish().unwrap();
                    }
                }
            }
        }
    }
}

#[test]
fn decode_independent_reference_apng() {
    for separate in [false, true] {
        let mut bytes = Vec::new();
        let mut enc = png::Encoder::new(&mut bytes, 2, 2);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_animated(3, 7).unwrap();
        enc.set_sep_def_img(separate).unwrap();
        let mut writer = enc.write_header().unwrap();
        let count = if separate { 4 } else { 3 };
        for n in 0..count {
            writer.write_image_data(&[n as u8; 16]).unwrap();
        }
        writer.finish().unwrap();
        let decoded = decode(&bytes).unwrap();
        let a = decoded.animation.unwrap();
        assert_eq!(a.plays, 7);
        assert_eq!(a.default_is_frame, !separate);
        assert_eq!(a.frames.len(), 3);
        for (n, f) in a.frames.iter().enumerate() {
            if let Some(i) = &f.image {
                assert_eq!(
                    i.samples,
                    Samples::U8(vec![(n + usize::from(separate)) as u8; 16])
                );
            }
        }
    }
}

#[test]
fn independent_twenty_frame_fixture() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/apng/basic_f20.png");
    let bytes = std::fs::read(path).unwrap();
    let decoded = decode(&bytes).unwrap();
    assert_eq!(decoded.animation.as_ref().unwrap().frames.len(), 20);
    let mut reader = png::Decoder::new(Cursor::new(bytes.as_slice()))
        .read_info()
        .unwrap();
    let a = decoded.animation.as_ref().unwrap();
    if !a.default_is_frame {
        check_reference(&bytes, &decoded.image, &mut reader);
    }
    for f in &a.frames {
        check_reference(
            &bytes,
            f.image.as_ref().unwrap_or(&decoded.image),
            &mut reader,
        );
    }
    reader.finish().unwrap();
}

#[test]
fn malformed_animation_controls_and_aggregate_limits() {
    fn crc(bytes: &[u8]) -> u32 {
        let mut state = !0u32;
        for &b in bytes {
            state ^= u32::from(b);
            for _ in 0..8 {
                state = (state >> 1) ^ if state & 1 != 0 { 0xedb8_8320 } else { 0 };
            }
        }
        !state
    }
    fn mutate(bytes: &[u8], kind: &[u8; 4], offset: usize, value: u8) -> Vec<u8> {
        let mut out = bytes.to_vec();
        let mut pos = 8;
        while pos < out.len() {
            let len = u32::from_be_bytes(out[pos..pos + 4].try_into().unwrap()) as usize;
            if &out[pos + 4..pos + 8] == kind {
                out[pos + 8 + offset] = value;
                let c = crc(&out[pos + 4..pos + 8 + len]);
                out[pos + 8 + len..pos + 12 + len].copy_from_slice(&c.to_be_bytes());
                return out;
            }
            pos += len + 12;
        }
        panic!("chunk not found");
    }
    let mut png = Png::new(image(2, 2, 8, ColorType::Rgba, 1));
    png.animation = Some(Animation {
        plays: 0,
        default_is_frame: false,
        frames: vec![Frame {
            control: control(1, 1, 1, 1),
            image: Some(image(1, 1, 8, ColorType::Rgba, 2)),
        }],
    });
    let bytes = encode(&png, &EncodeOptions::default()).unwrap();
    for (kind, offset, value) in [
        (b"acTL", 3, 0),
        (b"acTL", 3, 2),
        (b"fcTL", 3, 1),
        (b"fcTL", 7, 3),
        (b"fcTL", 15, 2),
        (b"fcTL", 24, 3),
        (b"fcTL", 25, 2),
        (b"fdAT", 3, 0),
    ] {
        assert!(
            decode(&mutate(&bytes, kind, offset, value)).is_err(),
            "{kind:?} {offset}"
        );
    }
    use deadlib_image_png::{DecodeOptions, Error, Limits, decode_with_options};
    assert_eq!(
        decode_with_options(
            &bytes,
            &DecodeOptions {
                limits: Limits {
                    max_frames: 0,
                    ..Default::default()
                }
            }
        ),
        Err(Error::LimitExceeded)
    );
    assert_eq!(
        decode_with_options(
            &bytes,
            &DecodeOptions {
                limits: Limits {
                    max_output_bytes: 19,
                    ..Default::default()
                }
            }
        ),
        Err(Error::LimitExceeded)
    );
}
