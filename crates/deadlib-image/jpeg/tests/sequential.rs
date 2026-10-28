use deadlib_image_jpeg::{ColorSpace, Error, Limits, Process, Upsampling, decode_coefficients};

fn word(data: &[u8], pos: &mut usize) -> usize {
    let value = u32::from_le_bytes(data[*pos..*pos + 4].try_into().unwrap()) as usize;
    *pos += 4;
    value
}
fn short(data: &[u8], pos: &mut usize) -> u16 {
    let value = u16::from_le_bytes(data[*pos..*pos + 2].try_into().unwrap());
    *pos += 2;
    value
}
fn fixtures() -> Vec<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sequential");
    let mut paths: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.extension().is_some_and(|s| s == "jpg"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 25);
    paths
}

#[test]
fn pinned_libjpeg_turbo_coefficients_and_idct_planes() {
    for path in fixtures() {
        let input = std::fs::read(&path).unwrap();
        let image = decode_coefficients(&input, Limits::default()).unwrap();
        let expected = std::fs::read(path.with_extension("coef")).unwrap();
        assert_eq!(&expected[..4], b"JCO1");
        let mut pos = 4;
        assert_eq!(word(&expected, &mut pos), image.frame.precision as usize);
        assert_eq!(word(&expected, &mut pos), image.frame.width as usize);
        assert_eq!(word(&expected, &mut pos), image.frame.height as usize);
        assert_eq!(word(&expected, &mut pos), image.components.len());
        for c in &image.components {
            for value in [
                c.spec.id as usize,
                c.spec.horizontal_sampling as usize,
                c.spec.vertical_sampling as usize,
                c.width,
                c.height,
                c.width.div_ceil(8),
                c.height.div_ceil(8),
            ] {
                assert_eq!(word(&expected, &mut pos), value, "{}", path.display());
            }
            for q in c.quantization {
                assert_eq!(short(&expected, &mut pos), q);
            }
            for y in 0..c.height.div_ceil(8) {
                for x in 0..c.width.div_ceil(8) {
                    for v in c.blocks[y * c.blocks_wide + x] {
                        assert_eq!(short(&expected, &mut pos) as i16, v, "{}", path.display());
                    }
                }
            }
        }
        assert_eq!(pos, expected.len());
        let planes = image.reconstruct().unwrap();
        let expected = std::fs::read(path.with_extension("planes")).unwrap();
        let mut pos = 0;
        for c in &planes.components {
            for &v in &c.samples {
                let reference = short(&expected, &mut pos);
                assert!(
                    v.abs_diff(reference) <= 1,
                    "{}: {v} vs {reference}",
                    path.display()
                );
            }
        }
        assert_eq!(pos, expected.len());
        if planes.color_space == ColorSpace::Cmyk {
            assert!(planes.to_image(Upsampling::Nearest).is_err());
        } else {
            planes
                .to_image(Upsampling::Nearest)
                .unwrap()
                .validate()
                .unwrap();
            planes
                .to_image(Upsampling::Linear)
                .unwrap()
                .validate()
                .unwrap();
        }
        assert_eq!(image.consumed, input.len());
        if path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .contains("multiscan")
        {
            assert_eq!(image.scans, 3);
        }
    }
}

#[test]
fn every_fixture_truncation_and_resource_limits() {
    for path in fixtures() {
        let bytes = std::fs::read(path).unwrap();
        for end in 0..bytes.len() {
            assert!(
                decode_coefficients(&bytes[..end], Limits::default()).is_err(),
                "prefix {end}"
            );
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
                max_metadata_bytes: 0,
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
    }
}

fn segment(out: &mut Vec<u8>, marker: u8, data: &[u8]) {
    out.extend_from_slice(&[255, marker]);
    out.extend_from_slice(&((data.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(data);
}
fn constant(precision: u8, wide_quant: bool, ac_symbol: u8) -> Vec<u8> {
    let mut bytes = vec![255, 216];
    let mut q = vec![if wide_quant { 16 } else { 0 }];
    for _ in 0..64 {
        if wide_quant {
            q.push(0);
        }
        q.push(1);
    }
    segment(&mut bytes, 219, &q);
    let mut table = vec![0, 1];
    table.extend_from_slice(&[0; 15]);
    table.push(0);
    table.extend_from_slice(&[16, 1]);
    table.extend_from_slice(&[0; 15]);
    table.push(ac_symbol);
    segment(&mut bytes, 196, &table);
    segment(
        &mut bytes,
        if precision == 8 { 192 } else { 193 },
        &[
            precision, 0, 9, 0, 23, 3, 1, 0x31, 0, 2, 0x21, 0, 3, 0x11, 0,
        ],
    );
    segment(&mut bytes, 218, &[3, 1, 0, 2, 0, 3, 0, 0, 63, 0]);
    // Two MCUs, six blocks each: a one-bit zero DC and one-bit EOB per block.
    bytes.extend_from_slice(&[0, 0, 0, 255, 217]);
    bytes
}

#[test]
fn fractional_sampling_precision_and_baseline_quantizer_rules() {
    for precision in [8, 12] {
        let bytes = constant(precision, false, 0);
        let image = decode_coefficients(&bytes, Limits::default()).unwrap();
        assert_eq!(
            image.frame.process,
            if precision == 8 {
                Process::Baseline
            } else {
                Process::ExtendedSequential
            }
        );
        assert_eq!(
            image.components.iter().map(|c| c.width).collect::<Vec<_>>(),
            [23, 16, 8]
        );
        let planes = image.reconstruct().unwrap();
        for c in &planes.components {
            assert!(c.samples.iter().all(|&v| v == 1 << (precision - 1)));
        }
        planes
            .to_image(Upsampling::Linear)
            .unwrap()
            .validate()
            .unwrap();
    }
    assert!(decode_coefficients(&constant(8, true, 0), Limits::default()).is_err());
    assert!(decode_coefficients(&constant(12, true, 0), Limits::default()).is_ok());
    assert!(decode_coefficients(&constant(8, false, 0xf1), Limits::default()).is_err());
}

#[test]
fn restart_sequence_and_duplicate_components_are_rejected() {
    let path = fixtures()
        .into_iter()
        .find(|p| p.file_stem().unwrap() == "8-restart")
        .unwrap();
    let mut bytes = std::fs::read(path).unwrap();
    let position = bytes.windows(2).position(|w| w == [255, 208]).unwrap();
    bytes[position + 1] = 211;
    assert!(matches!(
        decode_coefficients(&bytes, Limits::default()),
        Err(Error::Invalid("restart sequence"))
    ));
    let mut bytes = constant(8, false, 0);
    let sos = bytes.windows(2).position(|w| w == [255, 218]).unwrap();
    bytes[sos + 7] = 1;
    assert!(decode_coefficients(&bytes, Limits::default()).is_err());
}

#[test]
fn deterministic_mutations_remain_bounded_and_panic_free() {
    let path = fixtures()
        .into_iter()
        .find(|p| p.file_stem().unwrap() == "12-420")
        .unwrap();
    let seed = std::fs::read(path).unwrap();
    let limits = Limits {
        max_pixels: 10000,
        max_memory_bytes: 1 << 20,
        max_metadata_bytes: 10000,
        max_scans: 30,
    };
    let mut state = 0x12481248u32;
    for _ in 0..4000 {
        let mut bytes = seed.clone();
        for _ in 0..3 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let i = state as usize % bytes.len();
            bytes[i] ^= (state >> 24) as u8;
        }
        if let Ok(image) = decode_coefficients(&bytes, limits) {
            let _ = image.reconstruct();
        }
    }
}
