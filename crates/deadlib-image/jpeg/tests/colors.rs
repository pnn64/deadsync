use deadlib_image_jpeg::{
    Adobe, CmykPolarity, ColorSpace, Event, Limits, MarkerReader, PlanarImage, Upsampling,
    decode_coefficients, decode_with_upsampling,
};

const COLOR_FIXTURES: [&[u8]; 4] = [
    include_bytes!("fixtures/sequential/8-420.jpg"),
    include_bytes!("fixtures/progressive/8-420.jpg"),
    include_bytes!("fixtures/sequential/8-cmyk.jpg"),
    include_bytes!("fixtures/progressive/8-cmyk.jpg"),
];

fn append_adobe(bytes: &mut Vec<u8>, transforms: &[u8]) {
    for &transform in transforms {
        bytes.extend_from_slice(&[0xff, 0xee, 0, 14]);
        bytes.extend_from_slice(
            &Adobe {
                version: 100,
                flags0: 0,
                flags1: 0,
                transform,
            }
            .to_bytes(),
        );
    }
}

// Replace only pre-scan color hints; keep the fixture's compressed samples.
// Later markers go after the first scan, including between progressive scans.
fn with_adobe_markers(input: &[u8], before: &[u8], after: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0xff, 0xd8];
    append_adobe(&mut bytes, before);
    let mut reader = MarkerReader::new(input).unwrap();
    let mut first_scan = true;
    while let Some(event) = reader.next() {
        let event = event.unwrap();
        let offset = match event {
            Event::Segment {
                marker,
                data,
                offset,
            } => {
                if first_scan
                    && ((marker == 0xe0 && data.starts_with(b"JFIF\0"))
                        || (marker == 0xee && data.starts_with(b"Adobe")))
                {
                    continue;
                }
                offset
            }
            Event::Scan { offset, .. } | Event::End { offset } => offset,
        };
        bytes.extend_from_slice(&input[offset..reader.consumed()]);
        if first_scan && matches!(event, Event::Scan { .. }) {
            append_adobe(&mut bytes, after);
            first_scan = false;
        }
    }
    bytes
}

fn assert_pixel_paths_match(input: &[u8], expected: &PlanarImage) {
    let coefficients = decode_coefficients(input, Limits::default()).unwrap();
    assert_eq!(coefficients.color_space(), expected.color_space);
    assert_eq!(coefficients.cmyk_polarity(), expected.cmyk_polarity);
    let planes = coefficients.reconstruct().unwrap();
    for method in [Upsampling::Nearest, Upsampling::Linear] {
        let pixels = expected.to_image(method).unwrap();
        assert_eq!(planes.to_image(method).unwrap(), pixels);
        assert_eq!(
            decode_with_upsampling(input, Limits::default(), method).unwrap(),
            pixels,
        );
    }
}

#[test]
fn adobe_transform_component_mismatches_preserve_pixels() {
    for input in COLOR_FIXTURES {
        let original = decode_coefficients(input, Limits::default()).unwrap();
        let expected = original.reconstruct().unwrap();
        let transform = match original.frame.components.len() {
            3 => {
                assert_eq!(expected.color_space, ColorSpace::YCbCr);
                2
            }
            4 => {
                assert_eq!(expected.color_space, ColorSpace::Cmyk);
                assert_eq!(expected.cmyk_polarity, CmykPolarity::AdobeInverted);
                1
            }
            _ => unreachable!(),
        };
        // The image/zune-compatible interpretation changes only the hint, not
        // the encoded components or either native interpolation policy.
        let bytes = with_adobe_markers(input, &[transform], &[]);
        assert_pixel_paths_match(&bytes, &expected);
    }
}

#[test]
fn last_pre_scan_adobe_marker_controls_color_and_polarity() {
    for input in COLOR_FIXTURES {
        for last in 0..=2 {
            let baseline = with_adobe_markers(input, &[last], &[]);
            let expected = decode_coefficients(&baseline, Limits::default())
                .unwrap()
                .reconstruct()
                .unwrap();
            let color = match (expected.components.len(), last) {
                (3, 0) => ColorSpace::Rgb,
                (3, _) => ColorSpace::YCbCr,
                (4, 2) => ColorSpace::Ycck,
                (4, _) => ColorSpace::Cmyk,
                _ => unreachable!(),
            };
            assert_eq!(expected.color_space, color);
            for first in 0..=2 {
                let bytes = with_adobe_markers(input, &[first, last], &[255, (last + 1) % 3]);
                assert_pixel_paths_match(&bytes, &expected);
            }
        }
    }
}

#[test]
fn late_adobe_markers_do_not_invert_untagged_cmyk() {
    for input in &COLOR_FIXTURES[2..] {
        let baseline = with_adobe_markers(input, &[], &[]);
        let expected = decode_coefficients(&baseline, Limits::default())
            .unwrap()
            .reconstruct()
            .unwrap();
        assert_eq!(expected.color_space, ColorSpace::Cmyk);
        assert_eq!(expected.cmyk_polarity, CmykPolarity::Conventional);
        let bytes = with_adobe_markers(input, &[], &[0, 1, 2]);
        assert_pixel_paths_match(&bytes, &expected);
    }
}

#[test]
fn unknown_adobe_transforms_do_not_guess_a_color_space() {
    for input in COLOR_FIXTURES {
        for transform in [3, 255] {
            let bytes = with_adobe_markers(input, &[0, transform], &[1]);
            let coefficients = decode_coefficients(&bytes, Limits::default()).unwrap();
            assert_eq!(coefficients.color_space(), ColorSpace::Unknown);
            let planes = coefficients.reconstruct().unwrap();
            for method in [Upsampling::Nearest, Upsampling::Linear] {
                assert!(planes.to_image(method).is_err());
                assert!(decode_with_upsampling(&bytes, Limits::default(), method).is_err());
            }
        }
    }
}

#[test]
fn cmyk_and_ycck_native_output_and_rgb_paths_agree() {
    for input in [
        include_bytes!("fixtures/sequential/8-cmyk.jpg").as_slice(),
        include_bytes!("fixtures/progressive/8-cmyk.jpg").as_slice(),
        include_bytes!("fixtures/metadata/ycck-8.jpg").as_slice(),
        include_bytes!("fixtures/metadata/ycck-12.jpg").as_slice(),
    ] {
        let image = decode_coefficients(input, Limits::default()).unwrap();
        let planes = image.reconstruct().unwrap();
        assert_eq!(planes.cmyk_polarity, CmykPolarity::AdobeInverted);
        for method in [Upsampling::Nearest, Upsampling::Linear] {
            let mut cmyk = planes.to_cmyk(method).unwrap();
            let original = cmyk.samples.clone();
            assert_eq!(
                cmyk.samples.len(),
                image.frame.width as usize * image.frame.height as usize * 4
            );
            assert_eq!(cmyk.to_image().unwrap(), planes.to_image(method).unwrap());
            cmyk.convert_polarity(CmykPolarity::Conventional).unwrap();
            assert_eq!(cmyk.to_image().unwrap(), planes.to_image(method).unwrap());
            cmyk.convert_polarity(CmykPolarity::AdobeInverted).unwrap();
            assert_eq!(cmyk.samples, original);
        }
    }
}

#[test]
fn untagged_cmyk_uses_explicit_conventional_polarity() {
    let bytes = include_bytes!("fixtures/sequential/8-cmyk.jpg");
    let mut image = decode_coefficients(bytes, Limits::default()).unwrap();
    image.metadata.retain(|s| s.marker != 0xee);
    let mut planes = image.reconstruct().unwrap();
    assert_eq!(planes.cmyk_polarity, CmykPolarity::Conventional);
    let conventional = planes.to_cmyk(Upsampling::Nearest).unwrap();
    assert_eq!(conventional.polarity, CmykPolarity::Conventional);
    planes.cmyk_polarity = CmykPolarity::AdobeInverted;
    let inverse = planes.to_image(Upsampling::Nearest).unwrap();
    assert_ne!(inverse, conventional.to_image().unwrap());
}

#[test]
fn direct_rgb_avoids_cmyk_intermediate_under_memory_budget() {
    let input = include_bytes!("fixtures/sequential/8-cmyk.jpg");
    let image = decode_coefficients(
        input,
        Limits {
            max_memory_bytes: 19000,
            ..Limits::default()
        },
    )
    .unwrap();
    let planes = image.reconstruct().unwrap();
    planes
        .to_image(Upsampling::Nearest)
        .unwrap()
        .validate()
        .unwrap();
    assert!(planes.to_cmyk(Upsampling::Nearest).is_err());
}

#[test]
fn malformed_public_reconstruction_models_return_errors() {
    let input = include_bytes!("fixtures/sequential/8-cmyk.jpg");
    let image = decode_coefficients(input, Limits::default()).unwrap();
    let mut bad = image.clone();
    bad.components[0].blocks.clear();
    assert!(bad.reconstruct().is_err());
    let mut bad = image.clone();
    bad.frame.precision = 0;
    assert!(bad.reconstruct().is_err());
    let mut bad = image.clone();
    bad.components[0].spec.horizontal_sampling = 0;
    assert!(bad.reconstruct().is_err());
    let mut bad = image.clone();
    bad.components[0].quantization[0] = 0;
    assert!(bad.reconstruct().is_err());
    let planes = image.reconstruct().unwrap();
    let mut bad = planes.clone();
    bad.components[0].samples.clear();
    assert!(bad.to_image(Upsampling::Linear).is_err());
    assert!(bad.to_cmyk(Upsampling::Nearest).is_err());
    let mut bad = planes.clone();
    bad.components[0].samples[0] = 65535;
    assert!(bad.to_image(Upsampling::Nearest).is_err());
    let mut bad = planes.clone();
    bad.precision = 17;
    assert!(bad.to_image(Upsampling::Nearest).is_err());
    let mut cmyk = planes.to_cmyk(Upsampling::Nearest).unwrap();
    cmyk.samples[0] = 65535;
    assert!(cmyk.convert_polarity(CmykPolarity::Conventional).is_err());
    assert!(cmyk.to_image().is_err());
}
