use deadlib_image_png::{
    ColorType, DecodeOptions, EncodeOptions, Error, Image, Png, Samples, Transparency, decode,
    decode_reader, encode_writer, zlib::Compression,
};
use std::io::{self, Write};
struct Fragmented {
    bytes: Vec<u8>,
    size: usize,
    calls: usize,
    fail_at: Option<usize>,
}
impl Write for Fragmented {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        self.calls += 1;
        if self.calls.is_multiple_of(3) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let left = self.fail_at.map_or(usize::MAX, |n| n - self.bytes.len());
        if left == 0 {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let n = input.len().min(self.size).min(left);
        self.bytes.extend_from_slice(&input[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        panic!("destination flush is caller-owned")
    }
}
fn fixture(color: ColorType, depth: u8, height: u32) -> Png {
    let count = 17 * height as usize * color.channels();
    let samples = if depth == 16 {
        Samples::U16((0..count).map(|n| (n * 37139 + n * n) as u16).collect())
    } else {
        Samples::U8(
            (0..count)
                .map(|n| ((n * 37 + n * n) % (1usize << depth)) as u8)
                .collect(),
        )
    };
    let mut png = Png::new(Image {
        width: 17,
        height,
        color_type: color,
        bit_depth: depth,
        samples,
    });
    if color == ColorType::Indexed {
        png.palette = (0..1usize << depth)
            .map(|n| [n as u8, (n * 37) as u8, (n * 71) as u8])
            .collect();
        png.transparency = Some(Transparency::Palette(vec![0, 127]));
    }
    png
}

#[test]
fn all_formats_fragmented_writes_and_chunks() {
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
            let png = fixture(color, depth, 13);
            for interlaced in [false, true] {
                for compression in [Compression::Stored, Compression::Fast] {
                    for idat_chunk_size in [1, 7, 65536] {
                        for size in [1, 3, 31] {
                            let mut writer = Fragmented {
                                bytes: Vec::new(),
                                size,
                                calls: 0,
                                fail_at: None,
                            };
                            let stats = encode_writer(
                                &png,
                                &EncodeOptions {
                                    interlaced,
                                    compression,
                                    idat_chunk_size,
                                    ..Default::default()
                                },
                                &mut writer,
                            )
                            .unwrap();
                            assert_eq!(stats.bytes_written, writer.bytes.len());
                            let result = decode(&writer.bytes).unwrap();
                            assert_eq!(result.image, png.image);
                            assert_eq!(result.palette, png.palette);
                            assert_eq!(result.transparency, png.transparency);
                            assert_eq!(result.interlaced, interlaced);
                            assert_eq!(
                                decode_reader(writer.bytes.as_slice(), &DecodeOptions::default())
                                    .unwrap(),
                                result
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn write_failures_and_input_validation() {
    let png = fixture(ColorType::Rgba, 16, 7);
    struct Zero;
    impl Write for Zero {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Ok(0)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    assert!(matches!(
        encode_writer(&png, &EncodeOptions::default(), Zero),
        Err(Error::Io(_))
    ));
    let mut writer = Fragmented {
        bytes: Vec::new(),
        size: 3,
        calls: 0,
        fail_at: Some(100),
    };
    assert!(matches!(
        encode_writer(&png, &EncodeOptions::default(), &mut writer),
        Err(Error::Io(_))
    ));
    assert_eq!(writer.bytes.len(), 100);
    let mut invalid = png;
    invalid.image.samples = Samples::U16(Vec::new());
    let mut out = Vec::new();
    assert!(encode_writer(&invalid, &EncodeOptions::default(), &mut out).is_err());
    assert!(out.is_empty());
}

#[test]
fn bounded_encoder_scratch_and_maximum_chunk_size() {
    for compression in [Compression::Stored, Compression::Fast] {
        let mut capacities = Vec::new();
        for height in [2, 4096] {
            let png = fixture(ColorType::Rgba, 16, height);
            let mut out = Vec::new();
            let options = EncodeOptions {
                compression,
                idat_chunk_size: 1_000_000,
                ..Default::default()
            };
            let stats = encode_writer(&png, &options, &mut out).unwrap();
            assert!(stats.peak_image_buffer_bytes < 800 * 1024);
            capacities.push(stats.peak_image_buffer_bytes);
            let mut pos = 8;
            while pos < out.len() {
                let n = u32::from_be_bytes(out[pos..pos + 4].try_into().unwrap()) as usize;
                if &out[pos + 4..pos + 8] == b"IDAT" {
                    assert!(n <= 65536);
                }
                pos += n + 12;
            }
            assert_eq!(decode(&out).unwrap().image, png.image);
        }
        assert_eq!(capacities[0], capacities[1]);
    }
}
