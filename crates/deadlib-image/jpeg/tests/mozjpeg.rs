use deadlib_image_jpeg::{
    CodingOptions, EntropyCoding, Error, Event, Limits, MarkerReader, Process, decode_coefficients,
    encode_coefficients,
};
#[allow(dead_code)]
#[path = "support/components.rs"]
mod support;

fn fixtures() -> Vec<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mozjpeg");
    let mut paths: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jpg"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 30);
    paths
}

#[test]
fn pinned_mozjpeg_coefficients_quantizers_and_float_idct() {
    let mut processes = Vec::new();
    for path in fixtures() {
        let bytes = std::fs::read(&path).unwrap();
        let image = decode_coefficients(&bytes, Limits::default()).unwrap();
        support::coefficient_dump(
            &std::fs::read(path.with_extension("coef")).unwrap(),
            &image.frame,
            &image.components,
        );
        processes.push(image.frame.process);
        let planes = image.reconstruct().unwrap();
        let expected = std::fs::read(path.with_extension("planes")).unwrap();
        let samples: Vec<_> = planes.components.iter().flat_map(|c| &c.samples).collect();
        assert_eq!(expected.len(), samples.len() * 2);
        for (pair, &value) in expected.as_chunks::<2>().0.iter().zip(samples) {
            assert!(u16::from_le_bytes(*pair).abs_diff(value) <= 1, "{path:?}");
        }
    }
    for process in [
        Process::Baseline,
        Process::ExtendedSequential,
        Process::Progressive,
        Process::ArithmeticSequential,
        Process::ArithmeticProgressive,
    ] {
        assert!(processes.contains(&process), "{process:?}");
    }
}

#[test]
fn native_rewrites_preserve_mozjpeg_coefficients() {
    for path in fixtures() {
        let bytes = std::fs::read(&path).unwrap();
        let image = decode_coefficients(&bytes, Limits::default()).unwrap();
        let expected = std::fs::read(path.with_extension("coef")).unwrap();
        for entropy_coding in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
            for progressive in [false, true] {
                let output = encode_coefficients(
                    &image,
                    CodingOptions {
                        entropy_coding,
                        progressive,
                        successive_approximation: 3,
                        restart_interval: 1,
                        ..CodingOptions::default()
                    },
                )
                .unwrap();
                let decoded = decode_coefficients(&output, Limits::default()).unwrap();
                support::coefficient_dump(&expected, &decoded.frame, &decoded.components);
            }
        }
    }
}

#[test]
fn mozjpeg_boundaries_mutations_and_reference_producer_defect() {
    let limits = Limits {
        max_pixels: 65_536,
        max_memory_bytes: 4 * 1024 * 1024,
        max_scans: 128,
        ..Limits::default()
    };
    let (mut boundaries, mut mutations) = (0, 0);
    for path in fixtures() {
        let bytes = std::fs::read(&path).unwrap();
        let mut ends = Vec::new();
        if bytes.len() <= 1_500 {
            ends.extend(0..bytes.len());
        } else {
            ends.extend((0..bytes.len()).step_by(32));
            for event in MarkerReader::new(&bytes).unwrap() {
                let offset = match event.unwrap() {
                    Event::Segment { offset, .. } | Event::Scan { offset, .. } => offset,
                    _ => continue,
                };
                ends.extend(offset.saturating_sub(1)..(offset + 5).min(bytes.len()));
            }
        }
        ends.sort_unstable();
        ends.dedup();
        for end in ends {
            assert!(decode_coefficients(&bytes[..end], limits).is_err());
            boundaries += 1;
        }
        let mut seed = 0x91a7_0c32u32;
        for _ in 0..100 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let offset = seed as usize % bytes.len();
            let mut changed = bytes.clone();
            changed[offset] ^= ((seed >> 16) as u8) | 1;
            if let Ok(image) = decode_coefficients(&changed, limits) {
                let _ = image.reconstruct();
            }
            mutations += 1;
        }
    }
    let malformed = include_bytes!("fixtures/mozjpeg-limitations/optimized-restarts.jpg");
    assert!(matches!(
        decode_coefficients(malformed, limits),
        Err(Error::Invalid("entropy padding"))
    ));
    let valid = include_bytes!("fixtures/mozjpeg-limitations/fastcrush-restarts.jpg");
    decode_coefficients(valid, limits)
        .unwrap()
        .reconstruct()
        .unwrap();
    let empty_dht = include_bytes!("fixtures/mozjpeg-limitations/empty-dht.jpg");
    assert!(matches!(
        decode_coefficients(empty_dht, limits),
        Err(Error::Invalid("empty DHT"))
    ));
    let without_empty = include_bytes!("fixtures/mozjpeg-limitations/without-empty-dht.jpg");
    decode_coefficients(without_empty, limits).unwrap();
    eprintln!("Checked {boundaries} mozjpeg prefixes/boundaries and {mutations} bounded mutations");
}
