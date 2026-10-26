#![no_main]
use deadlib_image_png::{
    Animation, BlendOp, ChunkPosition, ColorType, DisposeOp, EncodeOptions, Frame, FrameControl,
    Image, Metadata, Png, Samples, Transparency, decode, decode_reader, encode_writer,
    zlib::Compression,
};
use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fn image(bytes: &[u8], width: u32, height: u32, color: ColorType, depth: u8, salt: usize) -> Image {
    let n = width as usize * height as usize * color.channels();
    let samples = if depth == 16 {
        Samples::U16(
            (0..n)
                .map(|n| {
                    u16::from_be_bytes([
                        bytes[(n * 2 + salt) % bytes.len()],
                        bytes[(n * 2 + salt + 1) % bytes.len()],
                    ])
                })
                .collect(),
        )
    } else {
        Samples::U8(
            (0..n)
                .map(|n| bytes[(n + salt) % bytes.len()] & ((1u16 << depth) - 1) as u8)
                .collect(),
        )
    };
    Image {
        width,
        height,
        color_type: color,
        bit_depth: depth,
        samples,
    }
}
fn reference_frame(reader: &mut png::Reader<Cursor<&[u8]>>, image: &Image) {
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
    let samples = image.width as usize * image.color_type.channels();
    for y in 0..image.height as usize {
        for x in 0..samples {
            match &image.samples {
                Samples::U16(v) => assert_eq!(
                    v[y * samples + x],
                    u16::from_be_bytes([bytes[y * stride + x * 2], bytes[y * stride + x * 2 + 1]])
                ),
                Samples::U8(v) if image.bit_depth == 8 => {
                    assert_eq!(v[y * samples + x], bytes[y * stride + x])
                }
                Samples::U8(v) => {
                    let depth = image.bit_depth as usize;
                    assert_eq!(
                        v[y * samples + x],
                        (bytes[y * stride + x * depth / 8] >> (8 - depth - x * depth % 8))
                            & ((1 << depth) - 1)
                    );
                }
            }
        }
    }
}
fuzz_target!(|bytes: &[u8]| {
    if bytes.len() < 8 {
        return;
    }
    use ColorType::*;
    let formats = [
        (Grayscale, 1),
        (Grayscale, 2),
        (Grayscale, 4),
        (Grayscale, 8),
        (Grayscale, 16),
        (Rgb, 8),
        (Rgb, 16),
        (Indexed, 1),
        (Indexed, 2),
        (Indexed, 4),
        (Indexed, 8),
        (GrayscaleAlpha, 8),
        (GrayscaleAlpha, 16),
        (Rgba, 8),
        (Rgba, 16),
    ];
    let (color, depth) = formats[bytes[0] as usize % 15];
    let width = 1 + u32::from(bytes[1] % 32);
    let height = 1 + u32::from(bytes[2] % 32);
    let options = EncodeOptions {
        interlaced: bytes[3] & 2 != 0,
        compression: if bytes[3] & 1 == 0 {
            Compression::Stored
        } else {
            Compression::Fast
        },
        filter: [
            deadlib_image_png::Filter::None,
            deadlib_image_png::Filter::Sub,
            deadlib_image_png::Filter::Up,
            deadlib_image_png::Filter::Average,
            deadlib_image_png::Filter::Paeth,
            deadlib_image_png::Filter::Adaptive,
        ][bytes[4] as usize % 6],
        idat_chunk_size: 1 + usize::from(bytes[6] % 31),
    };
    let mut png = Png::new(image(bytes, width, height, color, depth, 7));
    png.interlaced = options.interlaced;
    if color == Indexed {
        png.palette = (0..1usize << depth)
            .map(|n| [n as u8, (n * 37) as u8, (n * 71) as u8])
            .collect();
    }
    if bytes[3] & 16 != 0 {
        png.transparency = match &png.image.samples {
            Samples::U8(v) if color == Grayscale => Some(Transparency::Grayscale(u16::from(v[0]))),
            Samples::U16(v) if color == Grayscale => Some(Transparency::Grayscale(v[0])),
            Samples::U8(v) if color == Rgb => Some(Transparency::Rgb([
                u16::from(v[0]),
                u16::from(v[1]),
                u16::from(v[2]),
            ])),
            Samples::U16(v) if color == Rgb => Some(Transparency::Rgb([v[0], v[1], v[2]])),
            _ if color == Indexed => Some(Transparency::Palette(vec![0, 127])),
            _ => None,
        };
    }
    if bytes[3] & 32 != 0 {
        png.ancillary.push(
            Metadata::Text {
                keyword: b"Comment".to_vec(),
                text: bytes.iter().map(|v| b'a' + v % 26).collect(),
                compressed: bytes[3] & 64 != 0,
            }
            .to_chunk(
                if png.palette.is_empty() {
                    ChunkPosition::BeforePalette
                } else {
                    ChunkPosition::BeforeData
                },
                options.compression,
            )
            .unwrap(),
        );
    }
    if bytes[3] & 4 != 0 {
        let default_is_frame = bytes[3] & 8 != 0;
        let mut frames = Vec::new();
        if default_is_frame {
            frames.push(Frame {
                control: FrameControl {
                    width,
                    height,
                    x_offset: 0,
                    y_offset: 0,
                    delay_num: 0,
                    delay_den: 0,
                    dispose: DisposeOp::Previous,
                    blend: BlendOp::Over,
                },
                image: None,
            });
        }
        for n in 0..2 {
            let w = 1 + u32::from(bytes[(n + 5) % bytes.len()]) % width;
            let h = 1 + u32::from(bytes[(n + 6) % bytes.len()]) % height;
            frames.push(Frame {
                control: FrameControl {
                    width: w,
                    height: h,
                    x_offset: u32::from(bytes[(n + 2) % bytes.len()]) % (width - w + 1),
                    y_offset: u32::from(bytes[(n + 3) % bytes.len()]) % (height - h + 1),
                    delay_num: u16::from(bytes[5]),
                    delay_den: u16::from(bytes[7]),
                    dispose: [DisposeOp::None, DisposeOp::Background, DisposeOp::Previous]
                        [bytes[(n + 4) % bytes.len()] as usize % 3],
                    blend: if bytes[(n + 1) % bytes.len()] & 1 == 0 {
                        BlendOp::Source
                    } else {
                        BlendOp::Over
                    },
                },
                image: Some(image(bytes, w, h, color, depth, n + 1)),
            });
        }
        png.animation = Some(Animation {
            plays: u32::from(bytes[5]),
            default_is_frame,
            frames,
        });
    }
    let mut encoded = Vec::new();
    let stats = encode_writer(&png, &options, &mut encoded).unwrap();
    assert_eq!(stats.bytes_written, encoded.len());
    assert!(stats.peak_image_buffer_bytes < 1 << 20);
    assert_eq!(decode(&encoded).unwrap(), png);
    assert_eq!(
        decode_reader(encoded.as_slice(), &Default::default()).unwrap(),
        png
    );
    let mut reference = png::Decoder::new(Cursor::new(encoded.as_slice()))
        .read_info()
        .unwrap();
    reference_frame(&mut reference, &png.image);
    if let Some(a) = &png.animation {
        for f in &a.frames {
            if let Some(image) = &f.image {
                reference_frame(&mut reference, image);
            }
        }
    }
    reference.finish().unwrap();
});
