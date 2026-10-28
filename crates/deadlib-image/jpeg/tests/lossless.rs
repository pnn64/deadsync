use deadlib_image::{ColorType, Samples};
use deadlib_image_jpeg::{
    ColorSpace, Error, Event, Limits, MarkerReader, Upsampling, decode_lossless,
};
use std::path::Path;

fn segment(out: &mut Vec<u8>, marker: u8, data: &[u8]) {
    out.extend([0xff, marker]);
    out.extend(((data.len() + 2) as u16).to_be_bytes());
    out.extend(data);
}

#[allow(clippy::too_many_arguments)]
fn golden(
    precision: u8,
    width: u16,
    height: u16,
    components: &[(u8, u8, u8)],
    predictor: u8,
    point: u8,
    restart: u16,
    entropy: &[u8],
) -> Vec<u8> {
    let mut out = vec![0xff, 0xd8];
    // Independent literal canonical table: symbols 0..16 have five-bit codes
    // 00000..10000. Category 16 transmits no appended magnitude bits.
    let mut dht = vec![0; 17];
    dht[5] = 17;
    dht.extend(0..=16);
    segment(&mut out, 0xc4, &dht);
    let mut sof = vec![precision];
    sof.extend(height.to_be_bytes());
    sof.extend(width.to_be_bytes());
    sof.push(components.len() as u8);
    for &(id, h, v) in components {
        sof.extend([id, h << 4 | v, 0]);
    }
    segment(&mut out, 0xc3, &sof);
    if restart != 0 {
        segment(&mut out, 0xdd, &restart.to_be_bytes());
    }
    let mut sos = vec![components.len() as u8];
    for &(id, _, _) in components {
        sos.extend([id, 0]);
    }
    sos.extend([predictor, 0, point]);
    segment(&mut out, 0xda, &sos);
    out.extend(entropy);
    out.extend([0xff, 0xd9]);
    out
}

fn fixtures() -> Vec<std::path::PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lossless");
    let mut paths: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|r| r.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "jpg"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 45);
    paths
}

fn u32_at(bytes: &[u8], offset: &mut usize) -> u32 {
    let value = u32::from_le_bytes(bytes[*offset..*offset + 4].try_into().unwrap());
    *offset += 4;
    value
}

#[test]
fn pinned_c_samples_all_precisions_predictors_and_component_layouts() {
    for path in fixtures() {
        let bytes = std::fs::read(&path).unwrap();
        let expected = std::fs::read(path.with_extension("samples")).unwrap();
        let image = decode_lossless(&bytes, Limits::default()).unwrap();
        assert_eq!(image.consumed, bytes.len());
        assert_eq!(&expected[..4], b"JLP1");
        let mut offset = 4;
        assert_eq!(u32_at(&expected, &mut offset), image.frame.precision as u32);
        assert_eq!(u32_at(&expected, &mut offset), image.frame.width as u32);
        assert_eq!(u32_at(&expected, &mut offset), image.frame.height as u32);
        assert_eq!(
            u32_at(&expected, &mut offset),
            image.components.len() as u32
        );
        for c in &image.components {
            for v in [
                c.spec.id as u32,
                c.spec.horizontal_sampling as u32,
                c.spec.vertical_sampling as u32,
                c.width as u32,
                c.height as u32,
            ] {
                assert_eq!(u32_at(&expected, &mut offset), v);
            }
            for &v in &c.samples {
                assert_eq!(
                    v,
                    u16::from_le_bytes(expected[offset..offset + 2].try_into().unwrap()),
                    "{}",
                    path.display()
                );
                offset += 2;
            }
        }
        assert_eq!(offset, expected.len());
        assert_eq!(image.color_space(), ColorSpace::Rgb);
        let pointers: Vec<_> = image
            .components
            .iter()
            .map(|c| c.samples.as_ptr())
            .collect();
        let planes = image.into_planar().unwrap();
        assert!(
            planes
                .components
                .iter()
                .zip(pointers)
                .all(|(c, p)| c.samples.as_ptr() == p)
        );
        let shared = planes.to_image(Upsampling::Linear).unwrap();
        shared.validate().unwrap();
        assert_eq!(shared.color_type, ColorType::Rgb);
        let max = (1u32 << planes.precision) - 1;
        for (i, value) in (0..planes.width as usize * planes.height as usize)
            .flat_map(|i| planes.components.iter().map(move |c| c.samples[i]))
            .enumerate()
        {
            match &shared.samples {
                Samples::U8(v) => assert_eq!(v[i], value as u8),
                Samples::U16(v) => {
                    assert_eq!(v[i], ((u32::from(value) * 65535 + max / 2) / max) as u16)
                }
            }
        }
    }
}

#[test]
fn literal_subsampled_interleaving_padding_and_restart_borders() {
    // Values and codes worked directly from Annex H.2's prediction equations;
    // the C oracle supports only 1x1 lossless factors. These cover 2x1/1x2,
    // odd edges, dummy samples and the first/other rows inside a restarted MCU.
    let horizontal = golden(
        8,
        3,
        2,
        &[(1, 2, 1), (2, 1, 1)],
        4,
        0,
        0,
        &[0x00, 0x60, 0x0c, 0x01, 0x0b, 0x00, 0x90, 0x00, 0x1f],
    );
    let vertical = golden(
        8,
        2,
        3,
        &[(1, 1, 2), (2, 1, 1)],
        4,
        0,
        0,
        &[0x00, 0xa0, 0x06, 0x00, 0x85, 0x00, 0x48, 0x00, 0x0f],
    );
    let restart = golden(
        8,
        2,
        3,
        &[(1, 1, 2), (2, 1, 1)],
        4,
        0,
        2,
        &[
            0x00, 0xa0, 0x06, 0x00, 0xbf, 0xff, 0xd0, 0x1c, 0x00, 0x90, 0xc0, 0x17,
        ],
    );
    for bytes in [horizontal, vertical, restart] {
        let image = decode_lossless(&bytes, Limits::default()).unwrap();
        assert_eq!(image.components[0].samples, [128, 129, 130, 131, 132, 133]);
        assert_eq!(image.components[1].samples, [128, 127, 126, 125]);
    }
}

#[test]
fn category_sixteen_modulo_and_unclamped_full_integer_prediction() {
    let bytes = golden(16, 1, 1, &[(1, 1, 1)], 1, 0, 0, &[0x87]);
    assert_eq!(
        decode_lossless(&bytes, Limits::default())
            .unwrap()
            .components[0]
            .samples,
        [0]
    );
    // [65535,0;0,65535]: final prediction -65535, modulo difference -2;
    // first row/column differences +1 wrap through 65536.
    let bytes = golden(
        16,
        2,
        2,
        &[(1, 1, 1)],
        4,
        0,
        0,
        &[0x7f, 0xff, 0x00, 0xf0, 0xc3, 0x13],
    );
    assert_eq!(
        decode_lossless(&bytes, Limits::default())
            .unwrap()
            .components[0]
            .samples,
        [65535, 0, 0, 65535]
    );
    for precision in 2..=16 {
        let bytes = golden(precision, 1, 1, &[(1, 1, 1)], 7, precision - 1, 0, &[0x07]);
        let image = decode_lossless(&bytes, Limits::default()).unwrap();
        assert_eq!(image.components[0].samples, [1 << (precision - 1)]);
    }
}

#[test]
fn full_frame_component_count_and_separate_scan_order() {
    // The reference encoder emits this ten-component SOF3 stream, but its
    // decoder rejects component 4 in SOS. Expected values are constructed
    // from source integers independently of either decoder (fixture manifest).
    let bytes = include_bytes!("fixtures/lossless-special/p2-ten-components.jpg");
    let expected = include_bytes!("fixtures/lossless-special/p2-ten-components.samples");
    let image = decode_lossless(bytes, Limits::default()).unwrap();
    assert_eq!(image.components.len(), 10);
    let mut offset = 20;
    for c in &image.components {
        offset += 20;
        for &v in &c.samples {
            assert_eq!(
                v,
                u16::from_le_bytes(expected[offset..offset + 2].try_into().unwrap())
            );
            offset += 2;
        }
    }
    assert_eq!(offset, expected.len());
    let components: Vec<_> = (0..255).map(|id| (id, 4, 4)).collect();
    let original = golden(2, 1, 1, &components, 1, 0, 0, &[]);
    let scan = original.windows(2).position(|s| s == [0xff, 0xda]).unwrap();
    let mut bytes = original[..scan].to_vec();
    for id in (0..255).rev() {
        segment(&mut bytes, 0xda, &[1, id, 0, 1, 0, 0]);
        bytes.push(0x07);
    }
    bytes.extend([0xff, 0xd9]);
    let image = decode_lossless(&bytes, Limits::default()).unwrap();
    assert_eq!(image.scans, 255);
    assert_eq!(image.components.len(), 255);
    assert!(image.components.iter().all(|c| c.samples == [2]));
    assert_eq!(image.color_space(), ColorSpace::Unknown);
    assert!(matches!(
        image.into_planar().unwrap().to_image(Upsampling::Linear),
        Err(Error::Unsupported(_))
    ));
}

#[test]
fn invalid_parameters_tables_restart_padding_and_sample_range() {
    for (predictor, point) in [(0, 0), (8, 0), (1, 8), (1, 0x10)] {
        let bytes = golden(8, 1, 1, &[(1, 1, 1)], predictor, point, 0, &[0x07]);
        assert!(decode_lossless(&bytes, Limits::default()).is_err());
    }
    for (p, w, h, rst, entropy) in [
        (2, 1, 1, 0, &[0x15][..]),                   // out of range 4
        (8, 1, 1, 0, &[0x00][..]),                   // zero padding
        (8, 2, 2, 1, &[0x00][..]),                   // restart in middle of row
        (8, 1, 2, 1, &[0x07, 0xff, 0xd1, 0x07][..]), // incorrect RST0
        (8, 1, 2, 1, &[0x07, 0x07][..]),             // missing restart
        (8, 1, 1, 0, &[0x07, 0x00][..]),
    ] {
        // excess bytes
        let bytes = golden(p, w, h, &[(1, 1, 1)], 1, 0, rst, entropy);
        assert!(decode_lossless(&bytes, Limits::default()).is_err());
    }
    for selector in [1, 0x40, 0x10] {
        let mut bytes = golden(8, 1, 1, &[(1, 1, 1)], 1, 0, 0, &[0x07]);
        let scan = bytes.windows(2).position(|s| s == [0xff, 0xda]).unwrap();
        bytes[scan + 6] = selector;
        assert!(decode_lossless(&bytes, Limits::default()).is_err());
    }
    let mut bytes = golden(16, 1, 1, &[(1, 1, 1)], 1, 0, 0, &[0x87]);
    bytes[39] = 17; // DHT symbol 16 becomes illegal category 17.
    assert!(decode_lossless(&bytes, Limits::default()).is_err());
    let mut bytes = golden(8, 1, 1, &[(1, 1, 1)], 1, 0, 0, &[0x07]);
    let end = bytes.len() - 2;
    bytes.splice(end..end, [0xff, 0xda, 0, 8, 1, 1, 0, 1, 0, 0, 0x07]);
    assert!(decode_lossless(&bytes, Limits::default()).is_err());
}

#[test]
fn entropy_destinations_and_interleaved_scan_constraints() {
    for table in 0..=3 {
        let mut bytes = golden(8, 1, 1, &[(1, 1, 1)], 1, 0, 0, &[0x07]);
        bytes[6] = table;
        let sos = bytes.windows(2).position(|s| s == [0xff, 0xda]).unwrap();
        bytes[sos + 6] = table << 4;
        assert_eq!(
            decode_lossless(&bytes, Limits::default())
                .unwrap()
                .components[0]
                .samples,
            [128]
        );
    }
    let bytes = golden(8, 1, 1, &[(1, 4, 4), (2, 1, 1)], 1, 0, 0, &[]);
    assert!(decode_lossless(&bytes, Limits::default()).is_err());
    for ids in [[2, 1], [1, 1], [3, 2]] {
        let mut bytes = golden(8, 1, 1, &[(1, 1, 1), (2, 1, 1)], 1, 0, 0, &[0x00, 0x3f]);
        let sos = bytes.windows(2).position(|s| s == [0xff, 0xda]).unwrap();
        bytes[sos + 5] = ids[0];
        bytes[sos + 7] = ids[1];
        assert!(decode_lossless(&bytes, Limits::default()).is_err());
    }
}

#[test]
fn borrowed_metadata_trailing_bytes_and_storage_limits() {
    let base = golden(16, 1, 1, &[(1, 1, 1)], 1, 0, 0, &[0x87]);
    let mut bytes = base[..2].to_vec();
    segment(&mut bytes, 0xfe, b"original\0comment");
    bytes.extend(&base[2..base.len() - 2]);
    segment(&mut bytes, 0xe3, b"after scan");
    bytes.extend([0xff, 0xd9]);
    let consumed = bytes.len();
    bytes.extend(b"unused");
    let image = decode_lossless(&bytes, Limits::default()).unwrap();
    assert_eq!(image.consumed, consumed);
    assert_eq!(image.metadata[0].data.as_ptr(), bytes[6..].as_ptr());
    assert_eq!(image.metadata[0].preceding_scans, 0);
    assert_eq!(image.metadata[1].preceding_scans, 1);
    for limits in [
        Limits {
            max_pixels: 0,
            ..Limits::default()
        },
        Limits {
            max_memory_bytes: 1,
            ..Limits::default()
        },
        Limits {
            max_metadata_bytes: 1,
            ..Limits::default()
        },
        Limits {
            max_scans: 0,
            ..Limits::default()
        },
    ] {
        assert!(matches!(
            decode_lossless(&bytes, limits),
            Err(Error::LimitExceeded(_))
        ));
    }
    let mut image = decode_lossless(&bytes, Limits::default()).unwrap();
    image.frame.precision = 17;
    assert!(image.into_planar().is_err());
}

#[test]
fn every_fixture_truncation_and_bounded_mutations() {
    let limits = Limits {
        max_pixels: 4096,
        max_memory_bytes: 1024 * 1024,
        max_scans: 16,
        ..Limits::default()
    };
    let mut state = 0x12345678u32;
    for path in fixtures() {
        let bytes = std::fs::read(path).unwrap();
        for end in 0..bytes.len() {
            assert!(decode_lossless(&bytes[..end], limits).is_err());
        }
        for _ in 0..100 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let mut bad = bytes.clone();
            let position = state as usize % bad.len();
            bad[position] ^= ((state >> 16) as u8) | 1;
            if let Ok(image) = decode_lossless(&bad, limits) {
                image.into_planar().unwrap();
            }
        }
        assert!(
            MarkerReader::new(&bytes)
                .unwrap()
                .any(|e| matches!(e, Ok(Event::Scan { .. })))
        );
    }
}
