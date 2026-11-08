use deadlib_image_png::{
    AncillaryChunk, ChunkPosition, ColorType, DecodeEvent, DecodeOptions, DecodeStatus,
    EncodeOptions, Error, Image, Limits, Metadata, Png, Samples, StreamDecoder, decode,
    decode_reader, decode_with_options, encode, encode_writer, zlib::Compression,
};
use std::{
    fs,
    io::{Cursor, Read, Write},
    path::Path,
};

fn image(color: ColorType, depth: u8) -> Png {
    let mut png = Png::new(Image {
        width: 3,
        height: 2,
        color_type: color,
        bit_depth: depth,
        samples: if depth == 16 {
            Samples::U16(vec![12345; 6 * color.channels()])
        } else {
            Samples::U8(vec![1; 6 * color.channels()])
        },
    });
    if color == ColorType::Indexed {
        png.palette = vec![[0; 3], [255; 3]];
    }
    png
}
fn chunk(kind: [u8; 4], data: Vec<u8>) -> AncillaryChunk {
    AncillaryChunk {
        kind,
        data,
        position: ChunkPosition::BeforePalette,
    }
}
fn compressed_profile(profile: &[u8]) -> AncillaryChunk {
    let mut data = b"ICC profile\0\0".to_vec();
    let mut compressor =
        flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    compressor.write_all(profile).unwrap();
    data.extend_from_slice(&compressor.finish().unwrap());
    chunk(*b"iCCP", data)
}
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
fn inject(png: &Png, c: &AncillaryChunk) -> Vec<u8> {
    let bytes = encode(png, &Default::default()).unwrap();
    let mut out = bytes[..33].to_vec();
    out.extend_from_slice(&(c.data.len() as u32).to_be_bytes());
    out.extend_from_slice(&c.kind);
    out.extend_from_slice(&c.data);
    let checksum = crc(&out[37..]);
    out.extend_from_slice(&checksum.to_be_bytes());
    out.extend_from_slice(&bytes[33..]);
    out
}
struct OneByte<'a>(&'a [u8]);
impl Read for OneByte<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let count = output.len().min(1);
        self.0.read(&mut output[..count])
    }
}
fn check_events(bytes: &[u8], expected: &Metadata) {
    let mut stream = StreamDecoder::new(DecodeOptions::default());
    let mut offset = 0;
    let mut seen = 0;
    loop {
        let end = bytes.len().min(offset + 1);
        let progress = stream.process(&bytes[offset..end]).unwrap();
        offset += progress.consumed;
        match progress.status {
            DecodeStatus::Event => {
                if let DecodeEvent::Metadata { value, .. } = stream.event().unwrap() {
                    assert_eq!(value, expected);
                    seen += 1;
                }
            }
            DecodeStatus::Done => {
                assert_eq!(seen, 1);
                return;
            }
            DecodeStatus::NeedInput => assert!(offset < bytes.len()),
        }
    }
}
fn both_encodings(png: &Png, options: &EncodeOptions) -> [Vec<u8>; 2] {
    let buffered = encode(png, options).unwrap();
    let mut streamed = Vec::new();
    encode_writer(png, options, &mut streamed).unwrap();
    [buffered, streamed]
}

#[test]
fn independent_icc_profiles_all_matching_formats_and_table_orders() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/icc");
    let mut paths: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|v| v.unwrap().path())
        .filter(|p| p.extension().is_some_and(|v| v == "icc"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 8);
    for path in paths {
        let source = fs::read(&path).unwrap();
        let count = u32::from_be_bytes(source[128..132].try_into().unwrap()) as usize;
        let mut reversed = source.clone();
        for n in 0..count {
            reversed[132 + n * 12..144 + n * 12]
                .copy_from_slice(&source[132 + (count - 1 - n) * 12..144 + (count - 1 - n) * 12]);
        }
        let gray = &source[16..20] == b"GRAY";
        for profile in [source, reversed] {
            let metadata = Metadata::IccProfile {
                name: b"Independent profile".to_vec(),
                profile: profile.clone(),
            };
            for color in [
                ColorType::Grayscale,
                ColorType::GrayscaleAlpha,
                ColorType::Rgb,
                ColorType::Rgba,
                ColorType::Indexed,
            ] {
                if gray != matches!(color, ColorType::Grayscale | ColorType::GrayscaleAlpha) {
                    continue;
                }
                for depth in [1, 2, 4, 8, 16] {
                    if !color.allows_depth(depth) {
                        continue;
                    }
                    let mut png = image(color, depth);
                    for compression in [Compression::Stored, Compression::Fast] {
                        png.ancillary = vec![
                            metadata
                                .to_chunk(ChunkPosition::BeforePalette, compression)
                                .unwrap(),
                        ];
                        for interlaced in [false, true] {
                            for bytes in both_encodings(
                                &png,
                                &EncodeOptions {
                                    interlaced,
                                    ..Default::default()
                                },
                            ) {
                                let actual = decode(&bytes).unwrap();
                                assert_eq!(actual.image, png.image);
                                assert_eq!(actual.ancillary, png.ancillary);
                                assert_eq!(
                                    actual.metadata(Limits::default()).unwrap(),
                                    vec![metadata.clone()]
                                );
                                assert_eq!(
                                    decode_reader(OneByte(&bytes), &Default::default()).unwrap(),
                                    actual
                                );
                                let reader =
                                    png::Decoder::new(Cursor::new(&bytes)).read_info().unwrap();
                                assert_eq!(
                                    reader.info().icc_profile.as_deref(),
                                    Some(profile.as_slice()),
                                    "{}",
                                    path.display()
                                );
                            }
                        }
                    }
                }
            }
            let mismatch = image(
                if gray {
                    ColorType::Rgb
                } else {
                    ColorType::Grayscale
                },
                8,
            );
            check_recovery(&mismatch, compressed_profile(&profile));
        }
    }
}

fn check_recovery(base: &Png, raw: AncillaryChunk) {
    let bytes = inject(base, &raw);
    let actual = decode(&bytes).unwrap();
    assert_eq!(actual.image, base.image);
    assert_eq!(actual.ancillary, vec![raw.clone()]);
    let expected = Metadata::Unknown {
        kind: raw.kind,
        data: raw.data,
    };
    assert_eq!(
        actual.metadata(Limits::default()).unwrap(),
        vec![expected.clone()]
    );
    assert_eq!(
        decode_reader(OneByte(&bytes), &Default::default()).unwrap(),
        actual
    );
    check_events(&bytes, &expected);
    assert!(encode(&actual, &Default::default()).is_err());
    let mut out = Vec::new();
    assert!(encode_writer(&actual, &Default::default(), &mut out).is_err());
    assert!(out.is_empty());
}

#[test]
fn icc_header_tag_bounds_duplicates_and_overlap_errors_recover_as_raw_metadata() {
    let base = image(ColorType::Rgb, 8);
    let source = include_bytes!("fixtures/icc/sRGB-v4.icc");
    for end in [0, 1, 127, 128, 131] {
        check_recovery(&base, compressed_profile(&source[..end]));
        assert!(
            Metadata::IccProfile {
                name: b"Invalid".to_vec(),
                profile: source[..end].to_vec()
            }
            .to_chunk(ChunkPosition::BeforePalette, Compression::Fast)
            .is_err()
        );
    }
    for (offset, value) in [
        (0, source.len() as u32 - 4),
        (36, 0),
        (16, u32::from_be_bytes(*b"CMYK")),
        (20, u32::from_be_bytes(*b"RGB ")),
        (12, u32::from_be_bytes(*b"link")),
        (12, u32::from_be_bytes(*b"abst")),
        (64, 4),
        (128, u32::MAX),
        (136, 0),
        (136, u32::MAX),
        (136, 133),
        (140, u32::MAX),
    ] {
        let mut invalid = source.to_vec();
        invalid[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        check_recovery(&base, compressed_profile(&invalid));
        assert!(
            Metadata::IccProfile {
                name: b"Invalid".to_vec(),
                profile: invalid
            }
            .to_chunk(ChunkPosition::BeforePalette, Compression::Stored)
            .is_err()
        );
    }
    let mut duplicate = source.to_vec();
    duplicate[9] = 0x40; // Uniqueness/contiguity rules were added for ICC v4.4.
    duplicate[144..148].copy_from_slice(&source[132..136]);
    check_recovery(&base, compressed_profile(&duplicate));
    let mut overlap = source.to_vec();
    overlap[9] = 0x40;
    let first = u32::from_be_bytes(source[136..140].try_into().unwrap());
    overlap[148..152].copy_from_slice(&(first + 4).to_be_bytes());
    check_recovery(&base, compressed_profile(&overlap));
    // An invalid ICC header must not bypass the cumulative inflation budget.
    let bytes = inject(&base, &compressed_profile(&vec![0; 100_000]));
    assert_eq!(
        decode_with_options(
            &bytes,
            &DecodeOptions {
                limits: Limits {
                    max_metadata_bytes: 1024,
                    ..Default::default()
                },
                ..Default::default()
            }
        ),
        Err(Error::LimitExceeded)
    );
}

#[test]
fn exif_header_endianness_first_directory_bounds_and_future_magic() {
    let base = image(ColorType::Rgba, 16);
    for little in [false, true] {
        for offset in [8u32, 16] {
            let mut data = if little {
                b"II\x2a\0".to_vec()
            } else {
                b"MM\0\x2a".to_vec()
            };
            data.extend_from_slice(&if little {
                offset.to_le_bytes()
            } else {
                offset.to_be_bytes()
            });
            data.resize(offset as usize + 6, 0);
            let value = Metadata::Exif(data.clone());
            let mut png = base.clone();
            png.ancillary = vec![
                value
                    .to_chunk(ChunkPosition::BeforePalette, Compression::Fast)
                    .unwrap(),
            ];
            for bytes in both_encodings(&png, &Default::default()) {
                let actual = decode(&bytes).unwrap();
                assert_eq!(actual.ancillary, png.ancillary);
                assert_eq!(
                    actual.metadata(Limits::default()).unwrap(),
                    vec![value.clone()]
                );
                assert_eq!(
                    decode_reader(OneByte(&bytes), &Default::default()).unwrap(),
                    actual
                );
                check_events(&bytes, &value);
                let reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
                assert_eq!(
                    reader.info().exif_metadata.as_deref(),
                    Some(data.as_slice())
                );
            }
            for end in 0..data.len() {
                check_recovery(&base, chunk(*b"eXIf", data[..end].to_vec()));
            }
            for invalid_offset in [0u32, 4, 9, u32::MAX] {
                let mut invalid = data.clone();
                invalid[4..8].copy_from_slice(&if little {
                    invalid_offset.to_le_bytes()
                } else {
                    invalid_offset.to_be_bytes()
                });
                check_recovery(&base, chunk(*b"eXIf", invalid));
            }
            data[offset as usize..offset as usize + 2].fill(255);
            check_recovery(&base, chunk(*b"eXIf", data));
        }
    }
    for magic in [*b"II\x2b\0", *b"MM\0\x2b", *b"Exif", *b"ZZZZ"] {
        let mut invalid = vec![0; 14];
        invalid[..4].copy_from_slice(&magic);
        check_recovery(&base, chunk(*b"eXIf", invalid.clone()));
        assert!(
            Metadata::Exif(invalid)
                .to_chunk(ChunkPosition::BeforePalette, Compression::Fast)
                .is_err()
        );
    }
}

#[test]
fn icc_v44_table_contiguity_and_shared_storage() {
    let mut source = include_bytes!("fixtures/icc/sRGB-v4.icc").to_vec();
    source[9] = 0x40; // v4.2 -> v4.4, with an unspecified optional profile ID.
    source[84..100].fill(0);
    let metadata = Metadata::IccProfile {
        name: b"Version 4.4 framing".to_vec(),
        profile: source.clone(),
    };
    let mut png = image(ColorType::Rgb, 8);
    png.ancillary = vec![
        metadata
            .to_chunk(ChunkPosition::BeforePalette, Compression::Fast)
            .unwrap(),
    ];
    for bytes in both_encodings(&png, &Default::default()) {
        assert_eq!(
            decode(&bytes).unwrap().metadata(Limits::default()).unwrap(),
            vec![metadata.clone()]
        );
    }
    let mut trailing = source.clone();
    trailing.extend_from_slice(&[0; 4]);
    let length = trailing.len() as u32;
    trailing[..4].copy_from_slice(&length.to_be_bytes());
    check_recovery(&image(ColorType::Rgb, 8), compressed_profile(&trailing));
    let count = u32::from_be_bytes(source[128..132].try_into().unwrap()) as usize;
    let mut found_shared = false;
    for n in 0..count {
        for m in n + 1..count {
            let (a, b) = (132 + n * 12, 132 + m * 12);
            if source[a + 4..a + 12] == source[b + 4..b + 12] {
                let mut invalid = source.clone();
                invalid[a + 8..a + 12].fill(0);
                check_recovery(&image(ColorType::Rgb, 8), compressed_profile(&invalid));
                found_shared = true;
                break;
            }
        }
        if found_shared {
            break;
        }
    }
    assert!(
        found_shared,
        "independent profile must exercise shared curve storage"
    );
}

#[test]
fn color_precedence_skips_unsupported_profiles_and_propagates_limits() {
    let invalid = compressed_profile(&[0; 132]);
    let mut png = image(ColorType::Rgb, 8);
    png.ancillary = vec![
        Metadata::Gamma(45455)
            .to_chunk(ChunkPosition::BeforePalette, Compression::Fast)
            .unwrap(),
    ];
    let bytes = inject(&png, &invalid);
    let actual = decode(&bytes).unwrap();
    assert_eq!(actual.color_description().unwrap().kind, *b"gAMA");
    assert_eq!(
        actual.color_description_with_limits(Limits {
            max_metadata_bytes: 16,
            ..Default::default()
        }),
        Err(Error::LimitExceeded)
    );
    let mut higher = actual.clone();
    higher.ancillary.push(
        Metadata::Cicp([9, 16, 0, 1])
            .to_chunk(ChunkPosition::BeforePalette, Compression::Fast)
            .unwrap(),
    );
    assert_eq!(
        higher
            .color_description_with_limits(Limits {
                max_metadata_bytes: 4,
                ..Default::default()
            })
            .unwrap()
            .unwrap()
            .kind,
        *b"cICP"
    );
    let valid = include_bytes!("fixtures/icc/sRGB-v4.icc");
    png.ancillary.insert(0, compressed_profile(valid));
    assert_eq!(png.color_description().unwrap().kind, *b"iCCP");
    assert_eq!(
        png.color_description_with_limits(Limits {
            max_metadata_bytes: 16,
            ..Default::default()
        }),
        Err(Error::LimitExceeded)
    );
}

#[test]
#[ignore = "build tests/oracle and set DEADLIB_LIBPNG_ORACLE; see tests/oracle/README.md"]
fn libpng_preserves_independent_icc_and_exif_payloads() {
    use std::process::Command;
    let oracle =
        std::env::var_os("DEADLIB_LIBPNG_ORACLE").expect("DEADLIB_LIBPNG_ORACLE is required");
    let scratch = std::env::temp_dir().join(format!(
        "deadlib-png-profile-oracle-{}.png",
        std::process::id()
    ));
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/icc");
    let mut count = 0;
    for path in fs::read_dir(fixtures)
        .unwrap()
        .map(|v| v.unwrap().path())
        .filter(|p| p.extension().is_some_and(|v| v == "icc"))
    {
        let profile = fs::read(path).unwrap();
        let gray = &profile[16..20] == b"GRAY";
        for color in [
            ColorType::Grayscale,
            ColorType::GrayscaleAlpha,
            ColorType::Rgb,
            ColorType::Rgba,
            ColorType::Indexed,
        ] {
            if gray != matches!(color, ColorType::Grayscale | ColorType::GrayscaleAlpha) {
                continue;
            }
            for depth in [1, 2, 4, 8, 16] {
                if !color.allows_depth(depth) {
                    continue;
                }
                for exif in [
                    b"II\x2a\0\x08\0\0\0\0\0\0\0\0\0".to_vec(),
                    b"MM\0\x2a\0\0\0\x08\0\0\0\0\0\0".to_vec(),
                ] {
                    let mut png = image(color, depth);
                    png.ancillary = vec![
                        Metadata::IccProfile {
                            name: b"Independent profile".to_vec(),
                            profile: profile.clone(),
                        }
                        .to_chunk(ChunkPosition::BeforePalette, Compression::Fast)
                        .unwrap(),
                        Metadata::Exif(exif.clone())
                            .to_chunk(ChunkPosition::BeforePalette, Compression::Fast)
                            .unwrap(),
                    ];
                    for interlaced in [false, true] {
                        for bytes in both_encodings(
                            &png,
                            &EncodeOptions {
                                interlaced,
                                ..Default::default()
                            },
                        ) {
                            fs::write(&scratch, bytes).unwrap();
                            let result = Command::new(&oracle)
                                .arg("--metadata")
                                .arg(&scratch)
                                .output()
                                .unwrap();
                            assert!(
                                result.status.success(),
                                "{}",
                                String::from_utf8_lossy(&result.stderr)
                            );
                            let data = &result.stdout;
                            assert!(data.len() >= 8);
                            assert_eq!(
                                u32::from_be_bytes(data[..4].try_into().unwrap()) as usize,
                                profile.len(),
                                "{}",
                                String::from_utf8_lossy(&result.stderr)
                            );
                            assert_eq!(
                                u32::from_be_bytes(data[4..8].try_into().unwrap()) as usize,
                                exif.len()
                            );
                            assert_eq!(&data[8..8 + profile.len()], profile);
                            assert_eq!(&data[8 + profile.len()..], exif);
                            count += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(count, 480);
    fs::remove_file(scratch).unwrap();
    println!("libpng preserved exact ICC and Exif bytes in {count} files");
}
