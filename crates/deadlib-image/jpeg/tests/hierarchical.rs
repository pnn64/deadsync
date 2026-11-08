use deadlib_image_jpeg::{
    CodingOptions, ColorSpace, EncodeInput, EncodeOptions, EntropyCoding, Error, Event, Expansion,
    HierarchicalData, Limits, LosslessOptions, MarkerReader, Process, SampleData,
    decode_coefficients, decode_hierarchical, decode_lossless, encode_lossless_samples,
    encode_samples,
};

fn segment(out: &mut Vec<u8>, marker: u8, data: &[u8]) {
    out.extend([255, marker]);
    out.extend(((data.len() + 2) as u16).to_be_bytes());
    out.extend(data);
}
fn header(out: &mut Vec<u8>, marker: u8, p: u8, w: u16, h: u16, components: &[(u8, u8, u8)]) {
    let mut data = vec![p];
    data.extend(h.to_be_bytes());
    data.extend(w.to_be_bytes());
    data.push(components.len() as u8);
    for &(id, sh, sv) in components {
        data.extend([id, (sh << 4) | sv, 0]);
    }
    segment(out, marker, &data);
}
fn dc_table(out: &mut Vec<u8>) {
    let mut data = vec![0; 17];
    data[5] = 17;
    data.extend(0..=16);
    segment(out, 0xc4, &data);
}
fn ac_table(out: &mut Vec<u8>, extra: Option<u8>) {
    let mut data = vec![0; 17];
    data[0] = 0x10;
    if let Some(symbol) = extra {
        data[2] = 2;
        data.extend([0, symbol]);
    } else {
        data[1] = 1;
        data.push(0);
    }
    segment(out, 0xc4, &data);
}
fn quant(out: &mut Vec<u8>, q: u8) {
    let mut data = vec![q; 65];
    data[0] = 0;
    segment(out, 0xdb, &data);
}
fn scan(out: &mut Vec<u8>, id: u8, parameters: [u8; 3], entropy: &[u8]) {
    segment(
        out,
        0xda,
        &[1, id, 0, parameters[0], parameters[1], parameters[2]],
    );
    out.extend(entropy);
}
fn start(p: u8, w: u16, h: u16, components: &[(u8, u8, u8)]) -> Vec<u8> {
    let mut out = vec![255, 0xd8];
    header(&mut out, 0xde, p, w, h, components);
    dc_table(&mut out);
    out
}
fn end(mut out: Vec<u8>) -> Vec<u8> {
    out.extend([255, 0xd9]);
    out
}

fn expansion_golden() -> Vec<u8> {
    let mut out = start(8, 4, 4, &[(1, 1, 1)]);
    header(&mut out, 0xc3, 8, 2, 2, &[(1, 1, 1)]);
    // Fixed five-bit DC codes for explicit differences [-128,+1,+1,+1]
    // reconstruct base [0,1;1,2]. No native encoder builds this fixture.
    scan(&mut out, 1, [1, 0, 0], &[0x43, 0xf8, 0x61, 0x87]);
    segment(&mut out, 0xdf, &[0x11]);
    header(&mut out, 0xc7, 8, 4, 4, &[(1, 1, 1)]);
    scan(&mut out, 1, [0, 0, 0], &[0; 10]);
    end(out)
}

#[test]
fn annex_j_literal_expansion_and_direct_lossless_differences() {
    let bytes = expansion_golden();
    let image = decode_hierarchical(&bytes, Limits::default()).unwrap();
    assert_eq!(image.frames.len(), 2);
    assert_eq!(image.scans, 2);
    assert_eq!(image.consumed, bytes.len());
    let planes = image.reconstruct_reference().unwrap();
    assert_eq!(
        planes[0].samples,
        [0, 0, 1, 1, 0, 0, 1, 1, 1, 1, 2, 2, 1, 1, 2, 2]
    );
    assert_eq!(
        image.frames[1].expansion,
        Expansion {
            horizontal: true,
            vertical: true
        }
    );
    if let HierarchicalData::Lossless { components, coding } = &image.frames[1].data {
        assert_eq!(components[0].samples, [0; 16]);
        assert_eq!(coding[0].predictor, 0);
    } else {
        panic!("lossless frame required");
    }
    assert!(matches!(
        decode_lossless(&bytes, Limits::default()),
        Err(Error::Unsupported(_))
    ));
    assert!(matches!(
        decode_coefficients(&bytes, Limits::default()),
        Err(Error::Unsupported(_))
    ));
}

#[test]
fn retained_reference_expansion_and_modulo_limitations() {
    for (bytes, expected) in [
        (
            &include_bytes!("fixtures/jpegxt-limitations/hierarchical-truncation.jpg")[..],
            &include_bytes!("fixtures/jpegxt-limitations/hierarchical-truncation.samples")[..],
        ),
        (
            &include_bytes!("fixtures/jpegxt-limitations/hierarchical-modulo.jpg")[..],
            &include_bytes!("fixtures/jpegxt-limitations/hierarchical-modulo.samples")[..],
        ),
        (
            &include_bytes!("fixtures/jpegxt-limitations/hierarchical-flat-encoder.jpg")[..],
            &include_bytes!("fixtures/jpegxt-limitations/hierarchical-flat-encoder.samples")[..],
        ),
    ] {
        let image = decode_hierarchical(bytes, Limits::default()).unwrap();
        assert_eq!(
            sample_snapshot(
                image.header.precision,
                image.header.width,
                image.header.height,
                &image.reconstruct_reference().unwrap()
            ),
            expected
        );
    }
    assert_eq!(
        include_bytes!("fixtures/jpegxt-limitations/hierarchical-truncation-reference.raw")[5],
        1
    );
    assert_eq!(
        include_bytes!("fixtures/jpegxt-limitations/hierarchical-modulo-reference.raw"),
        &[0, 0]
    );
    let bytes = include_bytes!("fixtures/jpegxt-limitations/hierarchical-flat-encoder.jpg");
    let image = decode_hierarchical(bytes, Limits::default()).unwrap();
    assert_eq!(image.frames.len(), 4);
    for (i, frame) in image.frames.iter().enumerate() {
        if let HierarchicalData::Lossless { components, .. } = &frame.data {
            let value = match i {
                0 => 1,
                3 => 65535,
                _ => 0,
            };
            assert!(components[0].samples.iter().all(|&v| v == value));
        } else {
            panic!("predictive frame expected");
        }
    }
    assert!(
        include_bytes!("fixtures/jpegxt-limitations/hierarchical-flat-encoder-reference.raw")
            .iter()
            .all(|&v| v == 0)
    );
}

#[test]
fn discarded_partial_mcu_samples_do_not_limit_visible_precision() {
    // P=2, 3x1, 2:1 sampling. All visible source values are 2;
    // the first component's discarded fourth sample is 65535. Direct
    // five-bit DC categories encode differences [0,0,0,0,-3,0].
    let mut out = vec![255, 0xd8];
    dc_table(&mut out);
    header(&mut out, 0xc3, 2, 3, 1, &[(1, 2, 1), (2, 1, 1)]);
    segment(&mut out, 0xda, &[2, 1, 0, 2, 0, 1, 0, 0]);
    out.extend([0, 0, 1, 0]);
    let bytes = end(out);
    let image = decode_lossless(&bytes, Limits::default()).unwrap();
    assert_eq!(image.components[0].samples, [2; 3]);
    assert_eq!(image.components[1].samples, [2; 2]);
    let mut hierarchy = vec![255, 0xd8];
    header(&mut hierarchy, 0xde, 2, 3, 1, &[(1, 2, 1), (2, 1, 1)]);
    hierarchy.extend(&bytes[2..]);
    let image = decode_hierarchical(&hierarchy, Limits::default()).unwrap();
    assert_eq!(image.reconstruct_reference().unwrap()[0].samples, [2; 3]);
    // A category-16 difference at the first visible sample remains invalid.
    let mut bad = bytes.clone();
    let entropy = bad.len() - 6;
    bad[entropy] = 0x80;
    assert!(decode_lossless(&bad, Limits::default()).is_err());
}

#[test]
fn modulo_sixteen_bit_and_signed_point_transform() {
    for (base, delta, expected) in [
        (&[0x7f, 0xff, 0, 0xff, 0][..], &[0x0f][..], 0),
        (&[0x87][..], &[0x0b][..], 65535),
    ] {
        let mut out = start(16, 1, 1, &[(1, 1, 1)]);
        header(&mut out, 0xc3, 16, 1, 1, &[(1, 1, 1)]);
        scan(&mut out, 1, [1, 0, 0], base);
        header(&mut out, 0xc7, 16, 1, 1, &[(1, 1, 1)]);
        scan(&mut out, 1, [0, 0, 0], delta);
        let bytes = end(out);
        let image = decode_hierarchical(&bytes, Limits::default()).unwrap();
        assert_eq!(
            image.reconstruct_reference().unwrap()[0].samples,
            [expected]
        );
        assert_eq!(
            image.reconstruct().unwrap().components[0].samples,
            [expected]
        );
    }
    let mut out = start(8, 1, 1, &[(1, 1, 1)]);
    header(&mut out, 0xc3, 8, 1, 1, &[(1, 1, 1)]);
    scan(&mut out, 1, [1, 0, 0], &[0x28, 0xff, 0]); // sample 100: initial difference -28
    header(&mut out, 0xc7, 8, 1, 1, &[(1, 1, 1)]);
    scan(&mut out, 1, [0, 0, 1], &[0x0b]); // coded -1 -> residual -2
    let bytes = end(out);
    let image = decode_hierarchical(&bytes, Limits::default()).unwrap();
    assert_eq!(image.reconstruct_reference().unwrap()[0].samples, [98]);
}

#[test]
fn differential_dct_dc_has_no_prediction_or_level_shift() {
    let mut out = start(8, 16, 8, &[(1, 1, 1)]);
    quant(&mut out, 1);
    ac_table(&mut out, None);
    header(&mut out, 0xc1, 8, 16, 8, &[(1, 1, 1)]);
    scan(&mut out, 1, [0, 63, 0], &[0x00, 0x0f]);
    quant(&mut out, 2);
    header(&mut out, 0xc5, 8, 16, 8, &[(1, 1, 1)]);
    // Two direct DC coefficients +4, each with Q=2, give +1 per sample.
    scan(&mut out, 1, [0, 63, 0], &[0x1c, 0x0e, 0x3f]);
    let bytes = end(out);
    let image = decode_hierarchical(&bytes, Limits::default()).unwrap();
    assert_eq!(
        image.reconstruct().unwrap().components[0].samples,
        [129; 128]
    );
    for (index, frame) in image.frames.iter().enumerate() {
        if let HierarchicalData::Dct(c) = &frame.data {
            assert_eq!(c[0].quantization, [index as u16 + 1; 64]);
            assert_eq!(c[0].blocks[0][0], if index == 0 { 0 } else { 4 });
            assert_eq!(c[0].blocks[1][0], if index == 0 { 0 } else { 4 });
        } else {
            panic!("DCT frame required");
        }
    }
}

#[test]
fn differential_progressive_dc_refinement_and_extended_ac_categories() {
    let mut out = start(8, 16, 8, &[(1, 1, 1)]);
    quant(&mut out, 1);
    ac_table(&mut out, None);
    header(&mut out, 0xc1, 8, 16, 8, &[(1, 1, 1)]);
    scan(&mut out, 1, [0, 63, 0], &[0, 0x0f]);
    header(&mut out, 0xc6, 8, 16, 8, &[(1, 1, 1)]);
    scan(&mut out, 1, [0, 0, 1], &[0x1c, 0x1c]); // +8 DC from raw +4 at Al=1
    scan(&mut out, 1, [0, 0, 0x10], &[0x3f]);
    scan(&mut out, 1, [1, 63, 0], &[0x3f]);
    let bytes = end(out);
    let image = decode_hierarchical(&bytes, Limits::default()).unwrap();
    assert_eq!(
        image.reconstruct().unwrap().components[0].samples,
        [129; 128]
    );
    assert_eq!(image.scans, 4);
    for (precision, category, coefficient, entropy) in [
        (8, 11, 1024, &[3, 0, 15][..]),
        (12, 15, 16384, &[3, 0, 0][..]),
    ] {
        let mut out = start(precision, 8, 8, &[(1, 1, 1)]);
        quant(&mut out, 1);
        ac_table(&mut out, Some(category));
        header(&mut out, 0xc1, precision, 8, 8, &[(1, 1, 1)]);
        scan(&mut out, 1, [0, 63, 0], &[1]);
        header(&mut out, 0xc5, precision, 8, 8, &[(1, 1, 1)]);
        scan(&mut out, 1, [0, 63, 0], entropy);
        let bytes = end(out);
        let image = decode_hierarchical(&bytes, Limits::default()).unwrap();
        if let HierarchicalData::Dct(c) = &image.frames[1].data {
            assert_eq!(c[0].blocks[0][1], coefficient);
        } else {
            panic!("DCT required");
        }
    }
}

#[test]
fn component_groups_and_fractional_reference_edge_cropping() {
    let mut out = start(8, 2, 1, &[(0, 2, 1), (255, 1, 1)]);
    header(&mut out, 0xc3, 8, 2, 1, &[(0, 2, 1)]);
    scan(&mut out, 0, [1, 0, 0], &[0, 0x3f]);
    header(&mut out, 0xc3, 8, 1, 1, &[(255, 1, 1)]);
    scan(&mut out, 255, [1, 0, 0], &[7]);
    let bytes = end(out);
    let image = decode_hierarchical(&bytes, Limits::default()).unwrap();
    let planes = image.reconstruct_reference().unwrap();
    assert_eq!(planes[0].samples, [128, 128]);
    assert_eq!(planes[1].samples, [128]);
    let mut out = start(8, 5, 1, &[(1, 4, 1), (2, 3, 1)]);
    header(&mut out, 0xc3, 8, 3, 1, &[(1, 4, 1), (2, 3, 1)]);
    scan(&mut out, 1, [1, 0, 0], &[0, 1]);
    scan(&mut out, 2, [1, 0, 0], &[0, 1]);
    segment(&mut out, 0xdf, &[0x10]);
    header(&mut out, 0xc7, 8, 5, 1, &[(1, 4, 1), (2, 3, 1)]);
    scan(&mut out, 1, [0, 0, 0], &[0, 0, 0, 0x7f]);
    scan(&mut out, 2, [0, 0, 0], &[0, 0, 15]);
    let bytes = end(out);
    let image = decode_hierarchical(&bytes, Limits::default()).unwrap();
    let planes = image.reconstruct_reference().unwrap();
    assert_eq!(planes[0].samples, [128; 5]);
    assert_eq!(planes[1].samples, [128; 4]);
}

#[test]
fn baseline_arithmetic_exception_and_final_lossless_order() {
    let values = [128; 64];
    let input = EncodeInput {
        width: 8,
        height: 8,
        precision: 8,
        color_space: ColorSpace::Grayscale,
        samples: SampleData::U8(&values),
    };
    let base = encode_samples(
        &input,
        EncodeOptions {
            quality: 100,
            coding: CodingOptions::default(),
            ..EncodeOptions::default()
        },
    )
    .unwrap();
    let mut residual = encode_lossless_samples(
        &input,
        LosslessOptions {
            entropy_coding: EntropyCoding::Arithmetic,
            ..LosslessOptions::default()
        },
    )
    .unwrap();
    // This constant source has all coded differences zero. It is also a direct
    // zero residual after changing only its SOF and lossless predictor selector.
    for event in MarkerReader::new(&residual.clone()).unwrap() {
        match event.unwrap() {
            Event::Segment {
                marker: 0xcb,
                offset,
                ..
            } => residual[offset + 1] = 0xcf,
            Event::Scan { offset, header, .. } => {
                residual[offset + 4 + 1 + 2 * header[0] as usize] = 0
            }
            _ => {}
        }
    }
    let mut bytes = vec![255, 0xd8];
    header(&mut bytes, 0xde, 8, 8, 8, &[(1, 1, 1)]);
    bytes.extend(&base[2..base.len() - 2]);
    bytes.extend(&residual[2..]);
    let image = decode_hierarchical(&bytes, Limits::default()).unwrap();
    assert_eq!(image.frames[0].frame.process, Process::Baseline);
    assert_eq!(image.reconstruct_reference().unwrap()[0].samples, [128; 64]);
    let mut mixed = bytes.clone();
    let offset = image.frames[0].offset;
    mixed[offset + 1] = 0xc1;
    assert!(decode_hierarchical(&mixed, Limits::default()).is_err());
    let mut later = bytes[..bytes.len() - 2].to_vec();
    later.extend(&residual[2..]);
    assert!(decode_hierarchical(&later, Limits::default()).is_err());
}

#[test]
fn malformed_hierarchies_limits_metadata_and_public_models() {
    let original = expansion_golden();
    for data in [
        &[][..],
        &[0][..],
        &[0x12][..],
        &[0x20][..],
        &[0x11, 0x11][..],
    ] {
        assert!(Expansion::parse(data).is_err());
    }
    let mut ranges = Vec::new();
    for event in MarkerReader::new(&original).unwrap() {
        if let Event::Segment {
            marker: 0xde | 0xdf,
            data,
            offset,
        } = event.unwrap()
        {
            ranges.push((original[offset + 1], offset, offset + 4 + data.len()));
        }
    }
    for &(marker, start, finish) in &ranges {
        let mut bytes = original.clone();
        bytes.drain(start..finish);
        assert!(
            decode_hierarchical(&bytes, Limits::default()).is_err(),
            "removed {marker:x}"
        );
        let mut bytes = original.clone();
        bytes.splice(finish..finish, original[start..finish].iter().copied());
        assert!(decode_hierarchical(&bytes, Limits::default()).is_err());
    }
    for limits in [
        Limits {
            max_pixels: 1,
            ..Limits::default()
        },
        Limits {
            max_memory_bytes: 1,
            ..Limits::default()
        },
        Limits {
            max_scans: 1,
            ..Limits::default()
        },
    ] {
        assert!(matches!(
            decode_hierarchical(&original, limits),
            Err(Error::LimitExceeded(_))
        ));
    }
    let mut bytes = original[..2].to_vec();
    segment(&mut bytes, 0xfe, b"borrowed\0comment");
    bytes.extend(&original[2..]);
    let consumed = bytes.len();
    bytes.extend(b"unused");
    let image = decode_hierarchical(&bytes, Limits::default()).unwrap();
    assert_eq!(image.consumed, consumed);
    assert_eq!(image.metadata[0].data.as_ptr(), bytes[6..].as_ptr());
    assert!(matches!(
        decode_hierarchical(
            &bytes,
            Limits {
                max_metadata_bytes: 1,
                ..Limits::default()
            }
        ),
        Err(Error::LimitExceeded(_))
    ));
    let mut bad = image.clone();
    bad.header.precision = 17;
    assert!(bad.reconstruct().is_err());
    let mut bad = image.clone();
    bad.metadata[0].preceding_scans = image.scans + 1;
    assert!(bad.reconstruct().is_err());
    let mut bad = image.clone();
    bad.frames[1].frame.width = 5;
    assert!(bad.reconstruct().is_err());
    let mut bad = image.clone();
    if let HierarchicalData::Lossless { components, .. } = &mut bad.frames[0].data {
        components[0].samples.clear();
    }
    assert!(bad.reconstruct().is_err());
    for end in 0..original.len() {
        assert!(decode_hierarchical(&original[..end], Limits::default()).is_err());
    }
}

#[test]
fn all_components_and_deep_huffman_lossless_hierarchies() {
    let specs: Vec<_> = (0..255).map(|id| (id, 1, 1)).collect();
    let mut out = start(2, 1, 1, &specs);
    for id in 0..255 {
        header(&mut out, 0xc3, 2, 1, 1, &[(id, 1, 1)]);
        scan(&mut out, id, [1, 0, 0], &[0x07]);
    }
    let bytes = end(out);
    let image = decode_hierarchical(&bytes, Limits::default()).unwrap();
    assert_eq!(image.frames.len(), 255);
    for (i, c) in image.reconstruct_reference().unwrap().iter().enumerate() {
        assert_eq!(usize::from(c.spec.id), i);
        assert_eq!(c.samples, [2]);
    }
    let mut out = start(2, 1, 1, &[(255, 1, 1)]);
    for i in 0..12 {
        header(
            &mut out,
            if i == 0 { 0xc3 } else { 0xc7 },
            2,
            1,
            1,
            &[(255, 1, 1)],
        );
        scan(&mut out, 255, [u8::from(i == 0), 0, 0], &[0x07]);
    }
    let bytes = end(out);
    let image = decode_hierarchical(&bytes, Limits::default()).unwrap();
    assert_eq!(image.frames.len(), 12);
    assert_eq!(image.reconstruct_reference().unwrap()[0].samples, [2]);
}

fn fixtures() -> Vec<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hierarchical");
    let mut paths: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|r| r.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "jpg"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 42);
    paths
}
fn sample_snapshot(
    p: u8,
    w: u16,
    h: u16,
    components: &[deadlib_image_jpeg::ComponentPlane],
) -> Vec<u8> {
    let mut out = b"JLP1".to_vec();
    for v in [
        u32::from(p),
        u32::from(w),
        u32::from(h),
        components.len() as u32,
    ] {
        out.extend(v.to_le_bytes());
    }
    for c in components {
        for v in [
            u32::from(c.spec.id),
            u32::from(c.spec.horizontal_sampling),
            u32::from(c.spec.vertical_sampling),
            c.width as u32,
            c.height as u32,
        ] {
            out.extend(v.to_le_bytes());
        }
        for v in &c.samples {
            out.extend(v.to_le_bytes());
        }
    }
    out
}
fn coefficient_snapshot(
    frame: &deadlib_image_jpeg::FrameHeader,
    components: &[deadlib_image_jpeg::CoefficientComponent],
) -> Vec<u8> {
    let mut out = b"JCO1".to_vec();
    for v in [
        u32::from(frame.precision),
        u32::from(frame.width),
        u32::from(frame.height),
        components.len() as u32,
    ] {
        out.extend(v.to_le_bytes());
    }
    for c in components {
        let (bw, bh) = (c.width.div_ceil(8), c.height.div_ceil(8));
        for v in [
            u32::from(c.spec.id),
            u32::from(c.spec.horizontal_sampling),
            u32::from(c.spec.vertical_sampling),
            c.width as u32,
            c.height as u32,
            bw as u32,
            bh as u32,
        ] {
            out.extend(v.to_le_bytes());
        }
        for q in c.quantization {
            out.extend(q.to_le_bytes());
        }
        for y in 0..bh {
            for x in 0..bw {
                for v in c.blocks[y * c.blocks_wide + x] {
                    out.extend(v.to_le_bytes());
                }
            }
        }
    }
    out
}

#[test]
fn pinned_independent_frame_data_and_sample_regressions() {
    let mut markers = [false; 256];
    for path in fixtures() {
        let bytes = std::fs::read(&path).unwrap();
        let image = decode_hierarchical(&bytes, Limits::default()).unwrap();
        let output = image.reconstruct().unwrap();
        // Frame entropy data were compared exactly with the independently
        // instrumented C decoder. Final samples pin the Annex J native filter;
        // C raster differences are reported separately in the manifest.
        assert_eq!(
            sample_snapshot(
                image.header.precision,
                image.header.width,
                image.header.height,
                &output.components
            ),
            std::fs::read(path.with_extension("samples")).unwrap(),
            "{}",
            path.display()
        );
        for (i, f) in image.frames.iter().enumerate() {
            markers[f.frame.process as usize] = true;
            let (suffix, snapshot) = match &f.data {
                HierarchicalData::Dct(c) => ("coef", coefficient_snapshot(&f.frame, c)),
                HierarchicalData::Lossless { components, .. } => (
                    "samples",
                    sample_snapshot(f.frame.precision, f.frame.width, f.frame.height, components),
                ),
            };
            assert_eq!(
                snapshot,
                std::fs::read(path.with_extension(format!("frame-{i}.{suffix}"))).unwrap(),
                "{} frame {i}",
                path.display()
            );
        }
        assert_eq!(image.consumed, bytes.len());
    }
    for marker in [
        0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf,
    ] {
        assert!(markers[marker], "SOF{marker:x}");
    }
}

#[test]
fn all_hierarchy_fixture_prefixes_and_bounded_mutations() {
    let limits = Limits {
        max_pixels: 4096,
        max_memory_bytes: 1 << 20,
        max_scans: 128,
        ..Limits::default()
    };
    let mut state = 0x546789abu32;
    for path in fixtures() {
        let bytes = std::fs::read(path).unwrap();
        for end in 0..bytes.len() {
            assert!(decode_hierarchical(&bytes[..end], limits).is_err());
        }
        for _ in 0..100 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let mut bad = bytes.clone();
            let at = state as usize % bad.len();
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            bad[at] ^= (state >> 24) as u8 | 1;
            if let Ok(image) = decode_hierarchical(&bad, limits) {
                let _ = image.reconstruct();
            }
        }
    }
}
