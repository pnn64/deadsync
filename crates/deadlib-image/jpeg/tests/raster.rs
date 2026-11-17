use deadlib_image::{ColorType, Image, Samples};
use deadlib_image_jpeg::{
    CodingOptions, ColorSpace, EncodeInput, EncodeOptions, Error, Event, Limits, MarkerReader,
    Process, SampleData, ScanSpec, Subsampling, Upsampling, decode, decode_coefficients,
    decode_hierarchical, decode_lossless, decode_with_upsampling, encode_samples,
};
use std::path::{Path, PathBuf};

fn reference(bytes: &[u8], limits: Limits, method: Upsampling) -> Result<Image, Error> {
    for event in MarkerReader::new(bytes)? {
        if let Event::Segment { marker, .. } = event? {
            if marker == 0xde {
                return decode_hierarchical(bytes, limits)?
                    .reconstruct()?
                    .to_image(method);
            }
            if let Ok(process) = Process::try_from(marker) {
                if process.is_lossless() {
                    return decode_lossless(bytes, limits)?
                        .into_planar()?
                        .to_image(method);
                }
                break;
            }
        }
    }
    decode_coefficients(bytes, limits)?
        .reconstruct()?
        .to_image(method)
}

fn jpeg_files(root: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            jpeg_files(&path, files);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("jpg"))
        {
            files.push(path);
        }
    }
}

#[test]
fn pinned_processes_match_the_existing_pixel_reconstruction() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut paths = Vec::new();
    for group in [
        "sequential",
        "progressive",
        "arithmetic",
        "arithmetic-progressive",
        "arithmetic-model",
        "lossless",
        "lossless-arithmetic",
        "lossless-special",
        "metadata",
        "mozjpeg",
        "dnl",
        "hierarchical",
        "hierarchical-native",
        "pyramid-native",
        "t83",
    ] {
        jpeg_files(&root.join(group), &mut paths);
    }
    paths.sort();
    let mut accepted = 0;
    for path in paths {
        let bytes = std::fs::read(&path).unwrap();
        for method in [Upsampling::Nearest, Upsampling::Linear] {
            let expected = reference(&bytes, Limits::default(), method);
            let actual = decode_with_upsampling(&bytes, Limits::default(), method);
            match expected {
                Ok(expected) => {
                    assert_eq!(actual.unwrap(), expected, "{}: {method:?}", path.display());
                    accepted += 1;
                }
                Err(_) => assert!(actual.is_err(), "unexpected acceptance: {}", path.display()),
            }
        }
    }
    assert!(accepted > 500, "accepted {accepted}");
}

#[test]
fn row_windows_match_all_sampling_ratios_and_odd_boundaries() {
    for (width, height) in [
        (1, 1),
        (1, 19),
        (17, 1),
        (3, 5),
        (17, 19),
        (31, 33),
        (49, 65),
        (65, 97),
    ] {
        for (space, channels) in [
            (ColorSpace::Grayscale, 1),
            (ColorSpace::Rgb, 3),
            (ColorSpace::YCbCr, 3),
            (ColorSpace::Cmyk, 4),
            (ColorSpace::Ycck, 4),
        ] {
            let mut state = 0x12345678u32;
            let samples: Vec<_> = (0..usize::from(width) * usize::from(height) * channels)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    state as u8
                })
                .collect();
            for subsampling in [
                Subsampling::S444,
                Subsampling::S420,
                Subsampling::S422,
                Subsampling::S440,
                Subsampling::S411,
                Subsampling::S410,
                Subsampling::Custom([(3, 1), (2, 1), (1, 1), (1, 1)]),
                Subsampling::Custom([(1, 3), (1, 2), (1, 1), (1, 1)]),
                Subsampling::Custom([(1, 1), (2, 1), (1, 1), (1, 1)]),
                Subsampling::Custom([(2, 1), (3, 1), (1, 1), (1, 1)]),
                Subsampling::Custom([(1, 1), (1, 2), (1, 1), (1, 1)]),
            ] {
                for restart_interval in [0, 3] {
                    let ids = if space == ColorSpace::Cmyk {
                        *b"CMYK"
                    } else {
                        [1, 2, 3, 4]
                    };
                    let separate = ids.map(|id| ScanSpec::new(&[id], 0, 63, 0, 0).unwrap());
                    let script = if matches!(space, ColorSpace::Cmyk | ColorSpace::Ycck)
                        && subsampling == Subsampling::S410
                    {
                        &separate[..]
                    } else {
                        &[]
                    };
                    let bytes = encode_samples(
                        &EncodeInput {
                            width,
                            height,
                            precision: 8,
                            color_space: space,
                            samples: SampleData::U8(&samples),
                        },
                        EncodeOptions {
                            quality: 87,
                            rgb_to_ycbcr: false,
                            subsampling,
                            coding: CodingOptions {
                                restart_interval,
                                scan_script: script,
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                    )
                    .unwrap();
                    for method in [Upsampling::Nearest, Upsampling::Linear] {
                        let expected = reference(&bytes, Limits::default(), method).unwrap();
                        let actual =
                            decode_with_upsampling(&bytes, Limits::default(), method).unwrap();
                        assert_eq!(
                            actual, expected,
                            "{width}x{height} {space:?} {subsampling:?} r{restart_interval} {method:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn storage_limits_allow_pixels_without_full_image_intermediates() {
    for (color_type, channels, memory) in [
        (ColorType::Grayscale, 1, 50_000),
        (ColorType::Rgb, 3, 170_000),
    ] {
        let image = Image {
            width: 257,
            height: 193,
            color_type,
            bit_depth: 8,
            samples: Samples::U8((0..257 * 193 * channels).map(|i| (i * 71) as u8).collect()),
        };
        let bytes = deadlib_image_jpeg::encode(&image, EncodeOptions::default()).unwrap();
        let expected = reference(&bytes, Limits::default(), Upsampling::Linear).unwrap();
        let limits = Limits {
            max_memory_bytes: memory,
            ..Default::default()
        };
        assert!(reference(&bytes, limits, Upsampling::Linear).is_err());
        assert_eq!(decode(&bytes, limits).unwrap(), expected);
        for limits in [
            Limits {
                max_pixels: 1,
                ..Default::default()
            },
            Limits {
                max_memory_bytes: 1,
                ..Default::default()
            },
            Limits {
                max_metadata_bytes: 0,
                ..Default::default()
            },
            Limits {
                max_scans: 0,
                ..Default::default()
            },
        ] {
            assert!(decode(&bytes, limits).is_err());
        }
    }
}

#[test]
fn truncations_and_mutations_preserve_strict_acceptance() {
    for bytes in [
        include_bytes!("fixtures/sequential/8-420.jpg").as_slice(),
        include_bytes!("fixtures/sequential/8-gray.jpg").as_slice(),
        include_bytes!("fixtures/sequential/8-cmyk.jpg").as_slice(),
    ] {
        for end in 0..bytes.len() {
            assert!(decode(&bytes[..end], Limits::default()).is_err());
        }
        for at in 0..bytes.len() {
            let mut changed = bytes.to_vec();
            changed[at] ^= 0x5a;
            let expected = reference(&changed, Limits::default(), Upsampling::Linear);
            let actual = decode(&changed, Limits::default());
            assert_eq!(actual.is_ok(), expected.is_ok(), "offset {at}");
            if let Ok(expected) = expected {
                assert_eq!(actual.unwrap(), expected, "offset {at}");
            }
        }
    }
}

#[test]
fn unused_late_tables_cannot_change_or_repair_an_earlier_scan() {
    let bytes = include_bytes!("fixtures/sequential/8-gray.jpg").as_slice();
    let (start, end, data) = MarkerReader::new(bytes)
        .unwrap()
        .find_map(|event| match event.unwrap() {
            Event::Segment {
                marker: 0xdb,
                offset,
                data,
            } => Some((
                offset,
                data.as_ptr() as usize - bytes.as_ptr() as usize + data.len(),
                data,
            )),
            _ => None,
        })
        .unwrap();
    let mut table = data.to_vec();
    table[1..].fill(255);
    let length = (table.len() as u16 + 2).to_be_bytes();
    let mut segment = vec![0xff, 0xdb, length[0], length[1]];
    segment.extend(table);
    let mut late = bytes[..bytes.len() - 2].to_vec();
    late.extend_from_slice(&segment);
    late.extend_from_slice(&[0xff, 0xd9]);
    let expected = reference(bytes, Limits::default(), Upsampling::Linear).unwrap();
    assert_eq!(decode(&late, Limits::default()).unwrap(), expected);
    let mut missing = bytes.to_vec();
    missing.drain(start..end);
    missing.truncate(missing.len() - 2);
    missing.extend(segment);
    missing.extend_from_slice(&[0xff, 0xd9]);
    assert!(reference(&missing, Limits::default(), Upsampling::Linear).is_err());
    assert!(decode(&missing, Limits::default()).is_err());
}

#[test]
fn grayscale_sampling_factors_do_not_change_noninterleaved_mcu_order() {
    let source = Image {
        width: 1,
        height: 19,
        color_type: ColorType::Grayscale,
        bit_depth: 8,
        samples: Samples::U8((0..19).map(|i| (i * 13) as u8).collect()),
    };
    let mut bytes = deadlib_image_jpeg::encode(&source, EncodeOptions::default()).unwrap();
    let factor = MarkerReader::new(&bytes)
        .unwrap()
        .find_map(|event| match event.unwrap() {
            Event::Segment {
                marker: 0xc0, data, ..
            } => Some(data.as_ptr() as usize - bytes.as_ptr() as usize + 7),
            _ => None,
        })
        .unwrap();
    for sampling in [0x11, 0x24, 0x42, 0x44] {
        bytes[factor] = sampling;
        assert_eq!(
            decode(&bytes, Limits::default()).unwrap(),
            reference(&bytes, Limits::default(), Upsampling::Linear).unwrap()
        );
    }
}
