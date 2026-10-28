use deadlib_image_jpeg::{
    Adobe, ByteOrder, CodingOptions, ColorSpace, DensityUnit, EncodeInput, EncodeOptions, Error,
    Exif, ICC_CHUNK_BYTES, IccProfile, IccWrite, Jfif, Jfxx, Limits, MAX_ICC_BYTES,
    MetadataOptions, MetadataSegment, SampleData, decode_coefficients, encode_coefficients,
    encode_coefficients_with_metadata, encode_samples_with_metadata,
};

fn segment(marker: u8, data: &[u8]) -> MetadataSegment<'_> {
    MetadataSegment {
        marker,
        data,
        offset: 0,
        preceding_scans: 0,
    }
}
fn profile_bytes() -> Vec<u8> {
    let mut data = vec![0; 132];
    data[..4].copy_from_slice(&132u32.to_be_bytes());
    data[8..12].copy_from_slice(&0x04300000u32.to_be_bytes());
    data[12..16].copy_from_slice(b"mntr");
    data[16..20].copy_from_slice(b"RGB ");
    data[20..24].copy_from_slice(b"XYZ ");
    data[36..40].copy_from_slice(b"acsp");
    data
}
#[test]
fn jfif_fields_thumbnail_and_adobe_roundtrip() {
    let jfif = Jfif {
        version: [1, 2],
        unit: DensityUnit::Inches,
        x_density: 300,
        y_density: 240,
        thumbnail_width: 2,
        thumbnail_height: 1,
        thumbnail_rgb: &[0, 1, 2, 253, 254, 255],
    };
    let bytes = jfif.to_bytes().unwrap();
    assert_eq!(Jfif::parse(&bytes).unwrap(), jfif);
    assert_eq!(segment(0xe0, &bytes).jfif().unwrap(), Some(jfif));
    assert!(segment(0xe1, &bytes).jfif().unwrap().is_none());
    for end in 0..bytes.len() {
        assert!(Jfif::parse(&bytes[..end]).is_err());
    }
    let mut bad = bytes.clone();
    bad[7] = 3;
    assert!(Jfif::parse(&bad).is_err());
    let mut bad = bytes.clone();
    bad[8..10].fill(0);
    assert!(Jfif::parse(&bad).is_err());
    assert!(
        Jfif {
            thumbnail_height: 0,
            ..jfif
        }
        .to_bytes()
        .is_err()
    );
    let adobe = Adobe {
        version: 100,
        flags0: 0x1234,
        flags1: 0x5678,
        transform: 2,
    };
    let mut bytes = adobe.to_bytes().to_vec();
    bytes.extend_from_slice(b"extension");
    assert_eq!(Adobe::parse(&bytes).unwrap(), adobe);
    assert_eq!(segment(0xee, &bytes).adobe().unwrap(), Some(adobe));
    for end in 0..12 {
        assert!(Adobe::parse(&bytes[..end]).is_err());
    }
}

#[test]
fn all_jfxx_thumbnail_types_and_unknown_extensions() {
    let mut rgb = b"JFXX\0\x13\x02\x01".to_vec();
    rgb.extend_from_slice(&[0, 1, 2, 3, 4, 5]);
    assert!(matches!(
        Jfxx::parse(&rgb).unwrap(),
        Jfxx::Rgb {
            width: 2,
            height: 1,
            pixels: [0, 1, 2, 3, 4, 5]
        }
    ));
    let mut indexed = b"JFXX\0\x11\x01\x02".to_vec();
    indexed.extend_from_slice(&[42; 768]);
    indexed.extend_from_slice(&[0, 255]);
    assert!(matches!(
        Jfxx::parse(&indexed).unwrap(),
        Jfxx::Indexed {
            width: 1,
            height: 2,
            indices: [0, 255],
            ..
        }
    ));
    assert!(matches!(
        Jfxx::parse(b"JFXX\0\x10\xff\xd8\xff\xd9").unwrap(),
        Jfxx::Jpeg([255, 216, 255, 217])
    ));
    assert!(matches!(
        Jfxx::parse(b"JFXX\0\x55unknown").unwrap(),
        Jfxx::Unknown {
            code: 0x55,
            data: b"unknown"
        }
    ));
    for end in 0..rgb.len() {
        assert!(Jfxx::parse(&rgb[..end]).is_err());
    }
    for end in 0..indexed.len() {
        assert!(Jfxx::parse(&indexed[..end]).is_err());
    }
}

fn exif_bytes(big: bool) -> Vec<u8> {
    let put16 = |v: u16| {
        if big {
            v.to_be_bytes()
        } else {
            v.to_le_bytes()
        }
    };
    let put32 = |v: u32| {
        if big {
            v.to_be_bytes()
        } else {
            v.to_le_bytes()
        }
    };
    let mut data = b"Exif\0\0".to_vec();
    data.extend_from_slice(if big { b"MM" } else { b"II" });
    data.extend_from_slice(&put16(42));
    data.extend_from_slice(&put32(8));
    data.extend_from_slice(&put16(2));
    data.extend_from_slice(&put16(0x010e));
    data.extend_from_slice(&put16(2));
    data.extend_from_slice(&put32(6));
    data.extend_from_slice(&put32(38));
    data.extend_from_slice(&put16(0x0112));
    data.extend_from_slice(&put16(3));
    data.extend_from_slice(&put32(1));
    data.extend_from_slice(&put16(6));
    data.extend_from_slice(&[0, 0]);
    data.extend_from_slice(&put32(44));
    data.extend_from_slice(b"hello\0");
    data.extend_from_slice(&put16(2));
    for (tag, value) in [(0x0201, 74), (0x0202, 4)] {
        data.extend_from_slice(&put16(tag));
        data.extend_from_slice(&put16(4));
        data.extend_from_slice(&put32(1));
        data.extend_from_slice(&put32(value));
    }
    data.extend_from_slice(&put32(0));
    data.extend_from_slice(&[255, 216, 255, 217]);
    data
}

#[test]
fn endian_exif_ifds_offsets_orientation_and_thumbnail() {
    for big in [false, true] {
        let bytes = exif_bytes(big);
        let exif = Exif::parse(&bytes).unwrap();
        assert_eq!(
            exif.byte_order,
            if big {
                ByteOrder::Big
            } else {
                ByteOrder::Little
            }
        );
        assert_eq!(exif.orientation().unwrap(), Some(6));
        assert_eq!(
            exif.jpeg_thumbnail().unwrap(),
            Some(&[255, 216, 255, 217][..])
        );
        let ifd = exif.ifd(exif.first_ifd_offset).unwrap();
        assert_eq!(ifd.entries().len(), 2);
        assert_eq!(
            ifd.find(0x010e).unwrap().unwrap().value_bytes().unwrap(),
            b"hello\0"
        );
        assert_eq!(
            segment(0xe1, &bytes)
                .exif()
                .unwrap()
                .unwrap()
                .orientation()
                .unwrap(),
            Some(6)
        );
        for end in 0..bytes.len() {
            if let Ok(exif) = Exif::parse(&bytes[..end]) {
                let _ = exif.orientation();
                let _ = exif.jpeg_thumbnail();
                for entry in exif.ifd(exif.first_ifd_offset).unwrap().entries() {
                    let _ = entry.value_bytes();
                    let _ = entry.scalar_u32();
                }
            }
        }
    }
    let bytes = exif_bytes(false);
    let mut bad = bytes.clone();
    bad[6 + 34..6 + 38].copy_from_slice(&8u32.to_le_bytes());
    assert!(Exif::parse(&bad).unwrap().jpeg_thumbnail().is_err());
    let mut bad = bytes.clone();
    bad[6 + 18..6 + 22].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(
        Exif::parse(&bad)
            .unwrap()
            .ifd(8)
            .unwrap()
            .find(0x010e)
            .unwrap()
            .unwrap()
            .value_bytes()
            .is_err()
    );
    let mut bad = bytes.clone();
    bad[6 + 30..6 + 32].copy_from_slice(&9u16.to_le_bytes());
    assert!(Exif::parse(&bad).unwrap().orientation().is_err());
}

#[test]
fn icc_out_of_order_chunks_split_header_and_copy_budgets() {
    let profile = profile_bytes();
    let mut a = b"ICC_PROFILE\0\x01\x03".to_vec();
    a.extend_from_slice(&profile[..5]);
    let mut b = b"ICC_PROFILE\0\x02\x03".to_vec();
    b.extend_from_slice(&profile[5..17]);
    let mut c = b"ICC_PROFILE\0\x03\x03".to_vec();
    c.extend_from_slice(&profile[17..]);
    let segments = [
        segment(0xe2, &c),
        segment(0xfe, b"comment"),
        segment(0xe2, &a),
        segment(0xe2, &b),
    ];
    let view = IccProfile::from_segments(&segments, 1000).unwrap().unwrap();
    assert_eq!(view.len(), 132);
    assert_eq!(view.chunks().len(), 3);
    assert!(view.contiguous().is_none());
    let header = view.header().unwrap();
    assert_eq!(&header.data_color_space, b"RGB ");
    assert_eq!(&header.connection_space, b"XYZ ");
    assert_eq!(header.rendering_intent, 0);
    assert_eq!(view.to_vec(132).unwrap(), profile);
    assert!(view.to_vec(131).is_err());
    let mut copied = [0; 132];
    view.copy_to(&mut copied).unwrap();
    assert_eq!(&copied[..], profile);
    assert!(view.copy_to(&mut [0; 131]).is_err());
    assert!(IccProfile::from_segments(&segments, 131).is_err());
}

#[test]
fn invalid_icc_sequences_are_rejected_without_breaking_pixel_decoding() {
    let valid = b"ICC_PROFILE\0\x01\x02abc";
    let other = b"ICC_PROFILE\0\x02\x02def";
    for data in [
        b"ICC_PROFILE\0".as_slice(),
        b"ICC_PROFILE\0\0\x01x",
        b"ICC_PROFILE\0\x02\x01x",
        b"ICC_PROFILE\0\x01\0x",
    ] {
        assert!(IccProfile::from_segments(&[segment(0xe2, data)], 100).is_err());
    }
    assert!(IccProfile::from_segments(&[segment(0xe2, valid)], 100).is_err());
    assert!(IccProfile::from_segments(&[segment(0xe2, valid), segment(0xe2, valid)], 100).is_err());
    assert!(
        IccProfile::from_segments(
            &[
                segment(0xe2, valid),
                segment(0xe2, b"ICC_PROFILE\0\x02\x03def")
            ],
            100
        )
        .is_err()
    );
    let mut late = segment(0xe2, other);
    late.preceding_scans = 1;
    assert!(IccProfile::from_segments(&[segment(0xe2, valid), late], 100).is_err());
    let input = include_bytes!("fixtures/sequential/8-gray.jpg");
    let original = decode_coefficients(input, Limits::default()).unwrap();
    let extra = [segment(0xe2, valid)];
    let bytes = encode_coefficients_with_metadata(
        &original,
        CodingOptions::default(),
        MetadataOptions {
            additional_segments: &extra,
            ..MetadataOptions::default()
        },
    )
    .unwrap();
    let image = decode_coefficients(&bytes, Limits::default()).unwrap();
    assert!(image.icc_profile().is_err());
    image.reconstruct().unwrap();
    let bytes = encode_coefficients(&image, CodingOptions::default()).unwrap();
    assert!(
        decode_coefficients(&bytes, Limits::default())
            .unwrap()
            .icc_profile()
            .is_err()
    );
}

#[test]
fn icc_writer_all_boundaries_and_255_chunks() {
    let input = include_bytes!("fixtures/sequential/8-gray.jpg");
    let original = decode_coefficients(input, Limits::default()).unwrap();
    for length in [
        1,
        ICC_CHUNK_BYTES,
        ICC_CHUNK_BYTES + 1,
        ICC_CHUNK_BYTES * 2,
        ICC_CHUNK_BYTES * 2 + 1,
        MAX_ICC_BYTES,
    ] {
        let profile: Vec<_> = (0..length)
            .map(|i| (i.wrapping_mul(137) % 251) as u8)
            .collect();
        let bytes = encode_coefficients_with_metadata(
            &original,
            CodingOptions::default(),
            MetadataOptions {
                icc: IccWrite::Replace(&profile),
                ..MetadataOptions::default()
            },
        )
        .unwrap();
        let decoded = decode_coefficients(&bytes, Limits::default()).unwrap();
        let view = decoded.icc_profile().unwrap().unwrap();
        assert_eq!(view.len(), length);
        assert_eq!(view.chunks().len(), length.div_ceil(ICC_CHUNK_BYTES));
        for (a, b) in view.chunks().zip(profile.chunks(ICC_CHUNK_BYTES)) {
            assert_eq!(a, b);
        }
        assert_eq!(decoded.components[0].blocks, original.components[0].blocks);
    }
    assert!(
        encode_coefficients_with_metadata(
            &original,
            CodingOptions::default(),
            MetadataOptions {
                icc: IccWrite::Replace(&[]),
                ..MetadataOptions::default()
            }
        )
        .is_err()
    );
    let too_big = vec![0; MAX_ICC_BYTES + 1];
    assert!(matches!(
        encode_coefficients_with_metadata(
            &original,
            CodingOptions::default(),
            MetadataOptions {
                icc: IccWrite::Replace(&too_big),
                ..MetadataOptions::default()
            }
        ),
        Err(Error::LimitExceeded(_))
    ));
}

#[test]
fn metadata_pixel_encoding_profile_preserve_remove_replace_and_limits() {
    let exif = exif_bytes(true);
    let profile = profile_bytes();
    let extras = [
        segment(0xe1, &exif),
        segment(0xe2, b"unrelated APP2"),
        segment(0xfe, b"comment"),
    ];
    let input = EncodeInput {
        width: 1,
        height: 1,
        precision: 8,
        color_space: ColorSpace::Grayscale,
        samples: SampleData::U8(&[128]),
    };
    let bytes = encode_samples_with_metadata(
        &input,
        EncodeOptions::default(),
        MetadataOptions {
            additional_segments: &extras,
            icc: IccWrite::Replace(&profile),
        },
    )
    .unwrap();
    let decoded = decode_coefficients(&bytes, Limits::default()).unwrap();
    assert_eq!(
        decoded.exif().unwrap().unwrap().orientation().unwrap(),
        Some(6)
    );
    assert_eq!(decoded.jfif().unwrap().unwrap().version, [1, 2]);
    assert_eq!(
        decoded
            .icc_profile()
            .unwrap()
            .unwrap()
            .to_vec(1000)
            .unwrap(),
        profile
    );
    for progressive in [false, true] {
        let bytes = encode_coefficients_with_metadata(
            &decoded,
            CodingOptions {
                progressive,
                ..CodingOptions::default()
            },
            MetadataOptions {
                icc: IccWrite::Remove,
                ..MetadataOptions::default()
            },
        )
        .unwrap();
        let changed = decode_coefficients(&bytes, Limits::default()).unwrap();
        assert!(changed.icc_profile().unwrap().is_none());
        assert!(changed.metadata.iter().any(|s| s.data == b"unrelated APP2"));
        let bytes = encode_coefficients_with_metadata(
            &decoded,
            CodingOptions {
                progressive,
                ..CodingOptions::default()
            },
            MetadataOptions {
                icc: IccWrite::Replace(b"replacement"),
                ..MetadataOptions::default()
            },
        )
        .unwrap();
        let changed = decode_coefficients(&bytes, Limits::default()).unwrap();
        assert_eq!(
            changed
                .icc_profile()
                .unwrap()
                .unwrap()
                .to_vec(1000)
                .unwrap(),
            b"replacement"
        );
    }
    let limits = Limits {
        max_metadata_bytes: 60,
        ..Limits::default()
    };
    assert!(
        encode_samples_with_metadata(
            &input,
            EncodeOptions {
                limits,
                ..EncodeOptions::default()
            },
            MetadataOptions {
                icc: IccWrite::Replace(&profile),
                ..MetadataOptions::default()
            }
        )
        .is_err()
    );
}

#[test]
fn real_srgb_header_late_color_markers_and_malformed_exif_mutations() {
    let profile = include_bytes!("fixtures/metadata/srgb.icc");
    let mut data = b"ICC_PROFILE\0\x01\x01".to_vec();
    data.extend_from_slice(profile);
    let chunks = [segment(0xe2, &data)];
    let view = IccProfile::from_segments(&chunks, 10000).unwrap().unwrap();
    assert_eq!(view.contiguous(), Some(profile.as_slice()));
    assert_eq!(&view.header().unwrap().data_color_space, b"RGB ");
    let input = include_bytes!("fixtures/sequential/8-420.jpg");
    let mut bytes = input[..input.len() - 2].to_vec();
    let adobe = Adobe {
        version: 100,
        flags0: 0,
        flags1: 0,
        transform: 0,
    }
    .to_bytes();
    bytes.extend_from_slice(&[255, 238, 0, 14]);
    bytes.extend_from_slice(&adobe);
    bytes.extend_from_slice(&[255, 217]);
    let image = decode_coefficients(&bytes, Limits::default()).unwrap();
    assert_eq!(image.color_space(), ColorSpace::YCbCr);
    assert!(image.adobe().unwrap().is_none());
    assert_eq!(
        image
            .metadata
            .last()
            .unwrap()
            .adobe()
            .unwrap()
            .unwrap()
            .transform,
        0
    );
    let original = exif_bytes(false);
    let mut state = 0x85467321u32;
    for _ in 0..5000 {
        let mut bytes = original.clone();
        for _ in 0..4 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let index = state as usize % bytes.len();
            bytes[index] ^= (state >> 24) as u8;
        }
        if let Ok(exif) = Exif::parse(&bytes) {
            let _ = exif.orientation();
            let _ = exif.jpeg_thumbnail();
            for entry in exif.ifd(exif.first_ifd_offset).unwrap().entries() {
                let _ = entry.value_bytes();
                let _ = entry.scalar_u32();
            }
        }
    }
}
