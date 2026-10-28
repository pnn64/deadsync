use deadlib_image_jpeg::{
    CodingOptions, CoefficientImage, Error, Event, Limits, MarkerReader, Process,
    decode_coefficients, encode_coefficients,
};

fn fixtures() -> Vec<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut paths = Vec::new();
    for group in ["arithmetic", "arithmetic-progressive"] {
        let group: Vec<_> = std::fs::read_dir(root.join(group))
            .unwrap()
            .map(|p| p.unwrap().path())
            .filter(|p| p.extension().is_some_and(|s| s == "jpg"))
            .collect();
        assert_eq!(group.len(), 15);
        paths.extend(group);
    }
    paths.sort();
    assert_eq!(paths.len(), 30);
    paths
}

// Serialize only visible coefficients, using the independent C oracle's format.
fn coefficients(image: &CoefficientImage<'_>) -> Vec<u8> {
    let mut out = b"JCO1".to_vec();
    let word = |out: &mut Vec<u8>, v: usize| out.extend_from_slice(&(v as u32).to_le_bytes());
    for v in [
        image.frame.precision as usize,
        image.frame.width as usize,
        image.frame.height as usize,
        image.components.len(),
    ] {
        word(&mut out, v);
    }
    for c in &image.components {
        for v in [
            c.spec.id as usize,
            c.spec.horizontal_sampling as usize,
            c.spec.vertical_sampling as usize,
            c.width,
            c.height,
            c.width.div_ceil(8),
            c.height.div_ceil(8),
        ] {
            word(&mut out, v);
        }
        for q in c.quantization {
            out.extend_from_slice(&q.to_le_bytes());
        }
        for y in 0..c.height.div_ceil(8) {
            for x in 0..c.width.div_ceil(8) {
                for v in c.blocks[y * c.blocks_wide + x] {
                    out.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
    }
    out
}

fn without_marker(bytes: &[u8], marker: u8) -> Vec<u8> {
    let mut result = bytes.to_vec();
    let mut ranges = Vec::new();
    for event in MarkerReader::new(bytes).unwrap() {
        if let Event::Segment {
            marker: m,
            offset,
            data,
        } = event.unwrap()
            && m == marker
        {
            ranges.push(offset..offset + 4 + data.len());
        }
    }
    for range in ranges.into_iter().rev() {
        result.drain(range);
    }
    result
}

#[test]
fn pinned_arithmetic_coefficients_and_huffman_transcodes() {
    for path in fixtures() {
        let bytes = std::fs::read(&path).unwrap();
        let expected = std::fs::read(path.with_extension("coef")).unwrap();
        let image = decode_coefficients(&bytes, Limits::default()).unwrap();
        let progressive = path.parent().unwrap().ends_with("arithmetic-progressive");
        assert_eq!(
            image.frame.process,
            if progressive {
                Process::ArithmeticProgressive
            } else {
                Process::ArithmeticSequential
            }
        );
        assert_eq!(image.consumed, bytes.len());
        assert_eq!(coefficients(&image), expected, "{}", path.display());
        assert!(
            image
                .components
                .iter()
                .all(|c| c.successive_approximation == [0; 64])
        );
        image.reconstruct().unwrap();
        for progressive in [false, true] {
            let encoded = encode_coefficients(
                &image,
                CodingOptions {
                    progressive,
                    restart_interval: 1,
                    ..CodingOptions::default()
                },
            )
            .unwrap();
            let decoded = decode_coefficients(&encoded, Limits::default()).unwrap();
            assert_eq!(coefficients(&decoded), expected);
        }
    }
}

#[test]
fn default_conditioning_and_invalid_dac_segments() {
    let original = include_bytes!("fixtures/arithmetic/8-gray.jpg");
    let expected = include_bytes!("fixtures/arithmetic/8-gray.coef");
    let stripped = without_marker(original, 0xcc);
    assert!(stripped.len() < original.len());
    let image = decode_coefficients(&stripped, Limits::default()).unwrap();
    assert_eq!(&coefficients(&image), expected);
    for data in [
        &[][..],
        &[0],
        &[4, 0],
        &[0x20, 1],
        &[0, 0x01],
        &[0x10, 0],
        &[0x10, 64],
    ] {
        let mut bytes = vec![255, 216, 255, 204];
        bytes.extend_from_slice(&((data.len() + 2) as u16).to_be_bytes());
        bytes.extend_from_slice(data);
        bytes.extend_from_slice(&original[2..]);
        assert!(matches!(
            decode_coefficients(&bytes, Limits::default()),
            Err(Error::Invalid(_))
        ));
    }
}

#[test]
fn arithmetic_restart_sequence_and_fill() {
    let original = include_bytes!("fixtures/arithmetic/12-gray.jpg");
    let expected = include_bytes!("fixtures/arithmetic/12-gray.coef");
    let at = original.windows(2).position(|w| w == [255, 208]).unwrap();
    let mut changed = original.to_vec();
    changed[at + 1] = 211;
    assert!(matches!(
        decode_coefficients(&changed, Limits::default()),
        Err(Error::Invalid("arithmetic restart sequence"))
    ));
    let mut changed = original.to_vec();
    changed.drain(at..at + 2);
    assert!(decode_coefficients(&changed, Limits::default()).is_err());
    let mut changed = original.to_vec();
    changed.insert(at, 255);
    assert_eq!(
        &coefficients(&decode_coefficients(&changed, Limits::default()).unwrap()),
        expected
    );
    let mut changed = original[..original.len() - 2].to_vec();
    changed.extend_from_slice(&[255, 208, 255, 217]);
    assert!(matches!(
        decode_coefficients(&changed, Limits::default()),
        Err(Error::Invalid("unexpected arithmetic restart"))
    ));
}

#[test]
fn arithmetic_prefixes_limits_and_trailing_bytes() {
    for path in fixtures() {
        let bytes = std::fs::read(path).unwrap();
        for end in 0..bytes.len() {
            assert!(decode_coefficients(&bytes[..end], Limits::default()).is_err());
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
                max_scans: 0,
                ..Limits::default()
            },
        ] {
            assert!(matches!(
                decode_coefficients(&bytes, limits),
                Err(Error::LimitExceeded(_))
            ));
        }
        let mut trailer = bytes.clone();
        trailer.extend_from_slice(b"next image");
        assert_eq!(
            decode_coefficients(&trailer, Limits::default())
                .unwrap()
                .consumed,
            bytes.len()
        );
    }
}

#[test]
fn bounded_arithmetic_mutation_smoke() {
    let original = include_bytes!("fixtures/arithmetic/12-420.jpg");
    let mut state = 0x17f0_8492u32;
    for _ in 0..5000 {
        let mut bytes = original.to_vec();
        for _ in 0..3 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let position = state as usize % bytes.len();
            bytes[position] ^= (state >> 24) as u8;
        }
        let limits = Limits {
            max_pixels: 4096,
            max_memory_bytes: 1 << 20,
            max_metadata_bytes: 65536,
            max_scans: 32,
        };
        if let Ok(image) = decode_coefficients(&bytes, limits) {
            let _ = image.reconstruct();
        }
    }
}

fn scans(bytes: &[u8]) -> Vec<(usize, usize, usize)> {
    MarkerReader::new(bytes)
        .unwrap()
        .filter_map(|e| match e.unwrap() {
            Event::Scan {
                header,
                entropy,
                offset,
            } => Some((offset, header.len(), entropy.len())),
            _ => None,
        })
        .collect()
}

#[test]
fn arithmetic_progressive_history_precision_and_unused_selectors() {
    let original = include_bytes!("fixtures/arithmetic-progressive/12-444.jpg");
    let expected = include_bytes!("fixtures/arithmetic-progressive/12-444.coef");
    let full = decode_coefficients(original, Limits::default()).unwrap();
    assert_eq!(full.scans, 98); // DC plus two bands/component, at 14 precision levels.
    assert!(
        full.components
            .iter()
            .any(|c| c.blocks.iter().any(|b| b[0] < 0))
    );
    let (offset, length, entropy) = scans(original)[0];
    let end = offset + 4 + length + entropy;
    let mut partial = original[..end].to_vec();
    partial.extend_from_slice(&[255, 217]);
    let partial = decode_coefficients(&partial, Limits::default()).unwrap();
    assert_eq!(partial.scans, 1);
    for (a, b) in partial.components.iter().zip(&full.components) {
        assert_eq!(a.successive_approximation[0], 13);
        assert_eq!(&a.successive_approximation[1..], &[-1; 63]);
        for (a, b) in a.blocks.iter().zip(&b.blocks) {
            assert_eq!(a[0], (b[0] >> 13) << 13);
            assert_eq!(&a[1..], &[0; 63]);
        }
    }
    partial.reconstruct().unwrap();
    let mut changed = original.to_vec();
    for (offset, length, _) in scans(original) {
        let header = &mut changed[offset + 4..offset + 4 + length];
        let n = header[0] as usize;
        let ss = header[1 + 2 * n];
        let ah = header[3 + 2 * n] >> 4;
        if ss == 0 && ah != 0 {
            for j in 0..n {
                header[2 + 2 * j] = 0x33;
            }
        } else if ss != 0 {
            header[2] |= 0x30;
        }
    }
    assert_eq!(
        &coefficients(&decode_coefficients(&changed, Limits::default()).unwrap()),
        expected
    );
}

#[test]
fn arithmetic_progressive_order_quantizers_and_bounded_mutations() {
    let original = include_bytes!("fixtures/arithmetic-progressive/12-444.jpg");
    let (offset, length, entropy) = scans(original)[0];
    let mut changed = original.to_vec();
    changed[offset + 4 + length - 1] = 0xed;
    assert!(matches!(
        decode_coefficients(&changed, Limits::default()),
        Err(Error::Invalid("progressive scan parameters"))
    ));
    changed[offset + 4 + length - 1] = 0xdc;
    assert!(matches!(
        decode_coefficients(&changed, Limits::default()),
        Err(Error::Invalid("progressive scan order"))
    ));
    let end = offset + 4 + length + entropy;
    let mut changed = original[..end].to_vec();
    changed.extend_from_slice(&original[offset..end]);
    changed.extend_from_slice(&original[end..]);
    assert!(matches!(
        decode_coefficients(&changed, Limits::default()),
        Err(Error::Invalid("progressive scan order"))
    ));
    let mut changed = original[..end].to_vec();
    let mut dqt = vec![255, 219, 1, 6]; // 2 + four 65-byte quantizers.
    for id in 0..4 {
        dqt.push(id);
        dqt.extend_from_slice(&[2; 64]);
    }
    changed.extend_from_slice(&dqt);
    changed.extend_from_slice(&original[end..]);
    assert!(matches!(
        decode_coefficients(&changed, Limits::default()),
        Err(Error::Invalid(
            "quantizer changed between progressive scans"
        ))
    ));
    let mut state = 0x5ee1_c003u32;
    for _ in 0..2000 {
        let mut bytes = original.to_vec();
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        let position = state as usize % bytes.len();
        bytes[position] ^= (state >> 24) as u8;
        let limits = Limits {
            max_pixels: 4096,
            max_memory_bytes: 1 << 20,
            max_metadata_bytes: 65536,
            max_scans: 256,
        };
        if let Ok(image) = decode_coefficients(&bytes, limits) {
            let _ = image.reconstruct();
        }
    }
}
