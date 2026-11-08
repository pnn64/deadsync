use deadlib_image_jpeg::{
    CodingOptions, Error, Limits, MetadataSegment, Process, decode_coefficients,
    encode_coefficients,
};

fn fixtures() -> Vec<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    ["sequential", "progressive"]
        .into_iter()
        .flat_map(|mode| {
            std::fs::read_dir(root.join(mode))
                .unwrap()
                .map(|p| p.unwrap().path())
                .filter(|p| p.extension().is_some_and(|s| s == "jpg"))
        })
        .collect()
}

#[test]
fn all_fixture_coefficients_quantizers_and_metadata_survive_rewriting() {
    for path in fixtures() {
        let input = std::fs::read(path).unwrap();
        let original = decode_coefficients(&input, Limits::default()).unwrap();
        for progressive in [false, true] {
            for optimize_huffman in [false, true] {
                for restart_interval in [0, 1, 7] {
                    let bytes = encode_coefficients(
                        &original,
                        CodingOptions {
                            progressive,
                            optimize_huffman,
                            restart_interval,
                            ..CodingOptions::default()
                        },
                    )
                    .unwrap();
                    let changed = decode_coefficients(&bytes, Limits::default()).unwrap();
                    assert_eq!(changed.frame.precision, original.frame.precision);
                    assert_eq!(changed.frame.width, original.frame.width);
                    assert_eq!(changed.frame.height, original.frame.height);
                    assert_eq!(changed.frame.process == Process::Progressive, progressive);
                    for (a, b) in original.components.iter().zip(&changed.components) {
                        assert_eq!(a.quantization, b.quantization);
                        assert_eq!(a.width, b.width);
                        assert_eq!(a.height, b.height);
                        for y in 0..a.height.div_ceil(8) {
                            for x in 0..a.width.div_ceil(8) {
                                assert_eq!(
                                    a.blocks[y * a.blocks_wide + x],
                                    b.blocks[y * b.blocks_wide + x]
                                );
                            }
                        }
                    }
                    let metadata = |image: &deadlib_image_jpeg::CoefficientImage<'_>| {
                        image
                            .metadata
                            .iter()
                            .map(|s| (s.marker, s.data.to_vec()))
                            .collect::<Vec<_>>()
                    };
                    assert_eq!(metadata(&original), metadata(&changed));
                }
            }
        }
    }
}

#[test]
fn malformed_public_models_and_output_limits_return_errors() {
    let input = include_bytes!("fixtures/sequential/8-420.jpg");
    let image = decode_coefficients(input, Limits::default()).unwrap();
    for max_output_bytes in [0, 1, 2, 20, 128] {
        assert!(matches!(
            encode_coefficients(
                &image,
                CodingOptions {
                    max_output_bytes,
                    ..CodingOptions::default()
                }
            ),
            Err(Error::LimitExceeded("encoded bytes"))
        ));
    }
    let mut bad = image.clone();
    bad.components[0].blocks.clear();
    assert!(encode_coefficients(&bad, CodingOptions::default()).is_err());
    let mut bad = image.clone();
    bad.components[0].quantization[3] = 0;
    assert!(encode_coefficients(&bad, CodingOptions::default()).is_err());
    let mut bad = image.clone();
    bad.components[0].blocks[0][1] = i16::MAX;
    assert!(encode_coefficients(&bad, CodingOptions::default()).is_err());
    let mut bad = image.clone();
    bad.frame.width = 0;
    assert!(encode_coefficients(&bad, CodingOptions::default()).is_err());
    let mut bad = image.clone();
    bad.metadata.push(MetadataSegment {
        marker: 0xc0,
        data: &[],
        offset: 0,
        preceding_scans: 0,
    });
    assert!(encode_coefficients(&bad, CodingOptions::default()).is_err());
}

#[test]
fn metadata_after_original_scans_is_retained_when_scan_count_changes() {
    let input = include_bytes!("fixtures/progressive/8-420.jpg");
    let mut image = decode_coefficients(input, Limits::default()).unwrap();
    image.metadata.push(MetadataSegment {
        marker: 0xfe,
        data: b"after progressive scan 4",
        offset: 0,
        preceding_scans: 4,
    });
    image.metadata.push(MetadataSegment {
        marker: 0xe2,
        data: b"after progressive scan 10",
        offset: 0,
        preceding_scans: 10,
    });
    for progressive in [false, true] {
        let bytes = encode_coefficients(
            &image,
            CodingOptions {
                progressive,
                ..CodingOptions::default()
            },
        )
        .unwrap();
        let changed = decode_coefficients(&bytes, Limits::default()).unwrap();
        assert_eq!(
            changed.metadata[changed.metadata.len() - 2].data,
            b"after progressive scan 4"
        );
        assert_eq!(
            changed.metadata.last().unwrap().data,
            b"after progressive scan 10"
        );
        assert_eq!(
            changed.metadata.last().unwrap().preceding_scans,
            changed.scans
        );
    }
}
