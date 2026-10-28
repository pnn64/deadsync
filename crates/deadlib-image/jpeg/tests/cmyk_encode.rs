use deadlib_image_jpeg::{
    CmykPolarity, CodingOptions, ColorSpace, EncodeInput, EncodeOptions, IccWrite, Limits,
    MetadataOptions, SampleData, Subsampling, Upsampling, decode_coefficients, encode_samples,
    quantize,
};

#[test]
fn both_source_polarities_produce_identical_files_across_precision_scaling() {
    for source_precision in [8, 12, 16] {
        let maximum = (1u32 << source_precision) - 1;
        let inks: Vec<_> = (0..11 * 9 * 4)
            .map(|i| ((i * 1021 + i * i % 997) as u32 % maximum) as u16)
            .collect();
        let inverse: Vec<_> = inks.iter().map(|&s| maximum as u16 - s).collect();
        for precision in [8, 12] {
            for ycck in [false, true] {
                for progressive in [false, true] {
                    for subsampling in [Subsampling::S444, Subsampling::S422, Subsampling::S420] {
                        let input = EncodeInput {
                            width: 11,
                            height: 9,
                            precision: source_precision,
                            color_space: ColorSpace::Cmyk,
                            samples: SampleData::U16(&inks),
                        };
                        let options = EncodeOptions {
                            quality: 90,
                            precision,
                            subsampling,
                            cmyk_to_ycck: ycck,
                            cmyk_polarity: CmykPolarity::Conventional,
                            coding: CodingOptions {
                                progressive,
                                restart_interval: 1,
                                ..CodingOptions::default()
                            },
                            ..EncodeOptions::default()
                        };
                        let a = encode_samples(&input, options).unwrap();
                        let b = encode_samples(
                            &EncodeInput {
                                samples: SampleData::U16(&inverse),
                                ..input
                            },
                            EncodeOptions {
                                cmyk_polarity: CmykPolarity::AdobeInverted,
                                ..options
                            },
                        )
                        .unwrap();
                        assert_eq!(a, b);
                        let image = decode_coefficients(&a, Limits::default()).unwrap();
                        assert_eq!(
                            image.color_space(),
                            if ycck {
                                ColorSpace::Ycck
                            } else {
                                ColorSpace::Cmyk
                            }
                        );
                        assert_eq!(image.cmyk_polarity(), CmykPolarity::AdobeInverted);
                    }
                }
            }
        }
    }
}

#[test]
fn ycck_presets_keep_y_and_k_resolution_and_honor_custom_factors() {
    let input = EncodeInput {
        width: 8,
        height: 8,
        precision: 8,
        color_space: ColorSpace::Cmyk,
        samples: SampleData::U8(&[128; 256]),
    };
    for subsampling in [
        Subsampling::S444,
        Subsampling::S422,
        Subsampling::S420,
        Subsampling::S440,
        Subsampling::S411,
    ] {
        let image = quantize(
            &input,
            EncodeOptions {
                subsampling,
                cmyk_to_ycck: true,
                ..EncodeOptions::default()
            },
        )
        .unwrap();
        assert_eq!(
            image.components[0].spec.horizontal_sampling,
            image.components[3].spec.horizontal_sampling
        );
        assert_eq!(
            image.components[0].spec.vertical_sampling,
            image.components[3].spec.vertical_sampling
        );
        assert_eq!(
            image.components[0].quantization,
            image.components[3].quantization
        );
        assert!(
            image
                .components
                .iter()
                .map(|c| c.spec.horizontal_sampling as usize * c.spec.vertical_sampling as usize)
                .sum::<usize>()
                <= 10
        );
    }
    assert!(
        quantize(
            &input,
            EncodeOptions {
                subsampling: Subsampling::S410,
                cmyk_to_ycck: true,
                ..EncodeOptions::default()
            }
        )
        .is_err()
    );
    let custom = Subsampling::Custom([(2, 2), (1, 1), (1, 1), (1, 1)]);
    let image = quantize(
        &input,
        EncodeOptions {
            subsampling: custom,
            cmyk_to_ycck: true,
            ..EncodeOptions::default()
        },
    )
    .unwrap();
    assert_eq!(image.components[3].spec.horizontal_sampling, 1);
}

#[test]
fn native_cmyk_raster_encoder_carries_polarity_and_metadata() {
    let source = EncodeInput {
        width: 8,
        height: 8,
        precision: 8,
        color_space: ColorSpace::Cmyk,
        samples: SampleData::U8(&[0; 256]),
    };
    let original = quantize(
        &source,
        EncodeOptions {
            quality: 100,
            cmyk_polarity: CmykPolarity::Conventional,
            ..EncodeOptions::default()
        },
    )
    .unwrap();
    let mut cmyk = original
        .reconstruct()
        .unwrap()
        .to_cmyk(Upsampling::Nearest)
        .unwrap();
    cmyk.convert_polarity(CmykPolarity::Conventional).unwrap();
    assert!(cmyk.samples.iter().all(|&v| v == 0));
    for precision in [8, 12] {
        for cmyk_to_ycck in [false, true] {
            let options = EncodeOptions {
                precision,
                quality: 100,
                cmyk_to_ycck,
                ..EncodeOptions::default()
            };
            let bytes = cmyk
                .encode_with_metadata(
                    options,
                    MetadataOptions {
                        icc: IccWrite::Replace(b"opaque test profile"),
                        ..MetadataOptions::default()
                    },
                )
                .unwrap();
            let image = decode_coefficients(&bytes, Limits::default()).unwrap();
            assert_eq!(
                image.icc_profile().unwrap().unwrap().to_vec(1000).unwrap(),
                b"opaque test profile"
            );
            let planes = image.reconstruct().unwrap();
            let rgb = planes.to_image(Upsampling::Nearest).unwrap();
            match rgb.samples {
                deadlib_image::Samples::U8(v) => assert!(v.iter().all(|&s| s == 255)),
                deadlib_image::Samples::U16(v) => assert!(v.iter().all(|&s| s == 65535)),
            }
            let a = cmyk.encode(options).unwrap();
            cmyk.convert_polarity(CmykPolarity::AdobeInverted).unwrap();
            assert_eq!(a, cmyk.encode(options).unwrap());
            cmyk.convert_polarity(CmykPolarity::Conventional).unwrap();
        }
    }
}

#[test]
fn encoded_ycck_input_keeps_its_component_meaning() {
    let samples: Vec<_> = (0..8 * 8 * 4).map(|i| ((i * 17) % 256) as u16).collect();
    let input = EncodeInput {
        width: 8,
        height: 8,
        precision: 8,
        color_space: ColorSpace::Ycck,
        samples: SampleData::U16(&samples),
    };
    let a = encode_samples(&input, EncodeOptions::default()).unwrap();
    let b = encode_samples(
        &input,
        EncodeOptions {
            cmyk_to_ycck: true,
            cmyk_polarity: CmykPolarity::Conventional,
            ..EncodeOptions::default()
        },
    )
    .unwrap();
    assert_eq!(a, b);
}
