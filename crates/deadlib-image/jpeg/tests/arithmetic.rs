use deadlib_image_jpeg::{
    CodingOptions, CoefficientImage, Error, Event, Limits, MarkerReader, Process,
    decode_coefficients, encode_coefficients,
};

fn fixtures() -> Vec<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/arithmetic");
    let mut paths: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.extension().is_some_and(|s| s == "jpg"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 15);
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
        assert_eq!(image.frame.process, Process::ArithmeticSequential);
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
