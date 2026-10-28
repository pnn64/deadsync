use deadlib_image_jpeg::{CmykPolarity, Limits, Upsampling, decode_coefficients};

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
