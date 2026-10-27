use deadlib_image_png::{
    AncillaryChunk, Animation, BlendOp, ChunkPosition, ColorType, DisposeOp, Frame, FrameControl,
    Image, Limits, Metadata, Png, Samples, Transparency, decode, decode_reader, encode,
    encode_writer,
};

fn crc(bytes: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in bytes {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = (c >> 1) ^ if c & 1 != 0 { 0xedb8_8320 } else { 0 };
        }
    }
    !c
}
fn fixture(png: &Png, chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let bytes = encode(png, &Default::default()).unwrap();
    let mut out = bytes[..33].to_vec();
    for (kind, data) in chunks {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(*kind);
        out.extend_from_slice(data);
        let sum = crc(&out[start..]);
        out.extend_from_slice(&sum.to_be_bytes());
    }
    out.extend_from_slice(&bytes[33..]);
    out
}
fn both_encodings(png: &Png) -> [Vec<u8>; 2] {
    let buffered = encode(png, &Default::default()).unwrap();
    let mut streamed = Vec::new();
    encode_writer(png, &Default::default(), &mut streamed).unwrap();
    [buffered, streamed]
}

#[test]
fn unused_transparency_background_bits_are_masked_and_reencoded_as_zero() {
    for (color, depths) in [
        (ColorType::Grayscale, &[1, 2, 4, 8, 16][..]),
        (ColorType::Rgb, &[8, 16][..]),
        (ColorType::GrayscaleAlpha, &[8, 16][..]),
        (ColorType::Rgba, &[8, 16][..]),
    ] {
        for &depth in depths {
            let mask = ((1u32 << depth) - 1) as u16;
            let mut values = Vec::new();
            for value in [mask, mask - 1] {
                for channel in 0..color.channels() {
                    values.push(
                        if channel == color.channels() - 1
                            && matches!(color, ColorType::GrayscaleAlpha | ColorType::Rgba)
                        {
                            mask
                        } else {
                            value
                        },
                    );
                }
            }
            let png = Png::new(Image {
                width: 2,
                height: 1,
                color_type: color,
                bit_depth: depth,
                samples: if depth == 16 {
                    Samples::U16(values)
                } else {
                    Samples::U8(values.iter().map(|v| *v as u8).collect())
                },
            });
            let gray = matches!(color, ColorType::Grayscale | ColorType::GrayscaleAlpha);
            let words = if gray { 1 } else { 3 };
            let raw = vec![255; words * 2];
            let mut chunks = vec![(b"bKGD", raw.clone())];
            let transparent = matches!(color, ColorType::Grayscale | ColorType::Rgb);
            if transparent {
                chunks.push((b"tRNS", raw));
            }
            let bytes = fixture(&png, &chunks);
            let actual = decode(&bytes).unwrap();
            assert_eq!(actual.image, png.image);
            assert_eq!(
                decode_reader(bytes.as_slice(), &Default::default()).unwrap(),
                actual
            );
            let expected_background = if gray {
                Metadata::BackgroundGray(mask)
            } else {
                Metadata::BackgroundRgb([mask; 3])
            };
            assert_eq!(
                actual.metadata(Limits::default()).unwrap(),
                vec![expected_background.clone()]
            );
            let rgba = actual.to_rgba16(Limits::default()).unwrap();
            let Samples::U16(samples) = &rgba.samples else {
                unreachable!()
            };
            assert_eq!(samples[3], if transparent { 0 } else { 65535 });
            assert_eq!(samples[7], 65535);
            if transparent {
                assert_eq!(
                    actual.transparency,
                    Some(if gray {
                        Transparency::Grayscale(65535)
                    } else {
                        Transparency::Rgb([65535; 3])
                    })
                );
            }
            for encoded in both_encodings(&actual) {
                let canonical = decode(&encoded).unwrap();
                assert_eq!(canonical.to_rgba16(Limits::default()).unwrap(), rgba);
                assert_eq!(
                    canonical.ancillary[0].data,
                    (0..words)
                        .flat_map(|_| mask.to_be_bytes())
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    canonical.metadata(Limits::default()).unwrap(),
                    vec![expected_background.clone()]
                );
                if transparent {
                    assert_eq!(
                        canonical.transparency,
                        Some(if gray {
                            Transparency::Grayscale(mask)
                        } else {
                            Transparency::Rgb([mask; 3])
                        })
                    );
                }
            }
        }
    }
}

#[test]
fn sixteen_bit_transparency_compares_both_bytes_before_scaling() {
    for color in [ColorType::Grayscale, ColorType::Rgb] {
        let values: Vec<_> = [0x1234, 0x12ff, 0xff34]
            .into_iter()
            .flat_map(|v| std::iter::repeat_n(v, color.channels()))
            .collect();
        let mut png = Png::new(Image {
            width: 3,
            height: 1,
            color_type: color,
            bit_depth: 16,
            samples: Samples::U16(values),
        });
        png.transparency = Some(if color == ColorType::Grayscale {
            Transparency::Grayscale(0x1234)
        } else {
            Transparency::Rgb([0x1234; 3])
        });
        for encoded in both_encodings(&png) {
            let native = decode(&encoded).unwrap();
            let Samples::U16(rgba) = native.to_rgba16(Limits::default()).unwrap().samples else {
                unreachable!()
            };
            assert_eq!([rgba[3], rgba[7], rgba[11]], [0, 65535, 65535]);
            assert_eq!([rgba[0], rgba[4], rgba[8]], [0x1234, 0x12ff, 0xff34]);
        }
    }
}

#[test]
fn histograms_cannot_claim_used_indices_are_unused() {
    let mut png = Png::new(Image {
        width: 2,
        height: 1,
        color_type: ColorType::Indexed,
        bit_depth: 2,
        samples: Samples::U8(vec![0, 2]),
    });
    png.palette = vec![[0; 3], [127; 3], [255; 3]];
    let histogram = |values: &[u16]| AncillaryChunk {
        kind: *b"hIST",
        data: values.iter().flat_map(|v| v.to_be_bytes()).collect(),
        position: ChunkPosition::BeforeData,
    };
    png.ancillary = vec![histogram(&[1, 0, 1])];
    for encoded in both_encodings(&png) {
        assert_eq!(decode(&encoded).unwrap().ancillary, png.ancillary);
    }
    for values in [[0, 0, 1], [1, 0, 0]] {
        png.ancillary = vec![histogram(&values)];
        assert!(encode(&png, &Default::default()).is_err());
        let mut out = Vec::new();
        assert!(encode_writer(&png, &Default::default(), &mut out).is_err());
        assert!(out.is_empty());
    }
    // Usage in animation frames also forbids a zero frequency.
    png.image.samples = Samples::U8(vec![0, 0]);
    png.ancillary = vec![histogram(&[1, 0, 0])];
    png.animation = Some(Animation {
        plays: 0,
        default_is_frame: false,
        frames: vec![Frame {
            control: FrameControl {
                width: 1,
                height: 1,
                x_offset: 0,
                y_offset: 0,
                delay_num: 1,
                delay_den: 1,
                dispose: DisposeOp::None,
                blend: BlendOp::Source,
            },
            image: Some(Image {
                width: 1,
                height: 1,
                color_type: ColorType::Indexed,
                bit_depth: 2,
                samples: Samples::U8(vec![2]),
            }),
        }],
    });
    assert!(encode(&png, &Default::default()).is_err());
    png.ancillary = vec![histogram(&[1, 0, 1])];
    for encoded in both_encodings(&png) {
        assert!(decode(&encoded).is_ok());
    }
}

#[test]
fn metadata_access_rejects_invalid_sample_formats_without_panicking() {
    let mut png = Png::new(Image {
        width: 1,
        height: 1,
        color_type: ColorType::Grayscale,
        bit_depth: 255,
        samples: Samples::U8(vec![0]),
    });
    png.ancillary.push(AncillaryChunk {
        kind: *b"bKGD",
        data: vec![0, 0],
        position: ChunkPosition::BeforePalette,
    });
    assert!(png.metadata(Limits::default()).is_err());
    png.image.bit_depth = 0;
    assert!(png.metadata(Limits::default()).is_err());
}

#[test]
fn srgb_requires_compatible_fallback_gamma_and_chromaticities_when_encoding() {
    use deadlib_image_png::zlib::Compression;
    let chromaticities = [31270, 32900, 64000, 33000, 30000, 60000, 15000, 6000];
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
            let mut png = Png::new(Image {
                width: 1,
                height: 1,
                color_type: color,
                bit_depth: depth,
                samples: if depth == 16 {
                    Samples::U16(vec![12345; color.channels()])
                } else {
                    Samples::U8(vec![1; color.channels()])
                },
            });
            if color == ColorType::Indexed {
                png.palette = vec![[0; 3], [255; 3]];
            }
            for intent in 0..4 {
                for value in [
                    Metadata::Srgb(intent),
                    Metadata::Gamma(45455),
                    Metadata::Chromaticities(chromaticities),
                ] {
                    png.ancillary.push(
                        value
                            .to_chunk(ChunkPosition::BeforePalette, Compression::Fast)
                            .unwrap(),
                    );
                }
                for encoded in both_encodings(&png) {
                    let actual = decode(&encoded).unwrap();
                    assert_eq!(
                        actual.metadata(Limits::default()).unwrap(),
                        png.metadata(Limits::default()).unwrap()
                    );
                    assert_eq!(actual.color_description().unwrap().kind, *b"sRGB");
                }
                png.ancillary.clear();
            }
        }
    }
    let base = Png::new(Image {
        width: 1,
        height: 1,
        color_type: ColorType::Rgba,
        bit_depth: 8,
        samples: Samples::U8(vec![0; 4]),
    });
    let srgb = Metadata::Srgb(0)
        .to_chunk(ChunkPosition::BeforePalette, Compression::Fast)
        .unwrap();
    let mut invalid_fallbacks = Vec::new();
    for gamma in [1, 45454, 45456, 0x7fff_ffff] {
        invalid_fallbacks.push(Metadata::Gamma(gamma));
    }
    for channel in 0..8 {
        let mut values = chromaticities;
        values[channel] += 1;
        invalid_fallbacks.push(Metadata::Chromaticities(values));
    }
    for value in invalid_fallbacks {
        let fallback = value
            .to_chunk(ChunkPosition::BeforePalette, Compression::Fast)
            .unwrap();
        for reverse in [false, true] {
            let mut chunks = vec![
                (b"sRGB", srgb.data.clone()),
                (&fallback.kind, fallback.data.clone()),
            ];
            if reverse {
                chunks.reverse();
            }
            let bytes = fixture(&base, &chunks);
            let actual = decode(&bytes).unwrap();
            assert_eq!(
                decode_reader(bytes.as_slice(), &Default::default()).unwrap(),
                actual
            );
            // The mismatched lower-precedence value remains available as raw
            // metadata; native image decoding follows the supported sRGB chunk.
            assert_eq!(actual.color_description().unwrap().kind, *b"sRGB");
            assert!(actual.metadata(Limits::default()).unwrap().contains(&value));
            assert!(encode(&actual, &Default::default()).is_err());
            let mut out = Vec::new();
            assert!(encode_writer(&actual, &Default::default(), &mut out).is_err());
            assert!(out.is_empty());
            let mut higher = actual.clone();
            higher.ancillary.push(
                Metadata::Cicp([9, 16, 0, 1])
                    .to_chunk(ChunkPosition::BeforePalette, Compression::Fast)
                    .unwrap(),
            );
            assert!(encode(&higher, &Default::default()).is_err());
            let mut without_srgb = actual;
            without_srgb.ancillary.retain(|c| &c.kind != b"sRGB");
            for encoded in both_encodings(&without_srgb) {
                assert!(decode(&encoded).is_ok());
            }
        }
    }
}
