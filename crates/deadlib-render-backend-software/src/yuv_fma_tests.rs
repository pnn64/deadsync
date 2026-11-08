use super::*;

const COEFFICIENTS: [[f32; 4]; 3] = [
    [1.402, -0.344136, -0.714136, 1.772],
    [1.5748, -0.187324, -0.468124, 1.8556],
    [1.4746, -0.164553, -0.571353, 1.8814],
];
const LEVELS: [[f32; 4]; 2] = [
    [1.0, 0.0, 1.0, -128.0 / 255.0],
    [255.0 / 219.0, -16.0 / 219.0, 255.0 / 224.0, -128.0 / 224.0],
];

fn random_float(state: &mut u32) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    f32::from_bits(*state)
}

#[test]
fn batches_match_portable_with_float_patterns_offsets_and_guards() {
    let mut state = 0x1234_5678;
    for (width, height) in [
        (2, 2),
        (6, 10),
        (2, 510),
        (2, 512),
        (2, 514),
        (30, 34),
        (32, 32),
        (34, 32),
        (130, 6),
    ] {
        let count = (width * height) as usize;
        for offset in 0..4 {
            let y: Vec<_> = (0..count + offset)
                .map(|i| (i * 73 + i / 61) as u8)
                .collect();
            let u: Vec<_> = (0..count / 4 + offset)
                .map(|i| (i * 31 + i / 17) as u8)
                .collect();
            let v: Vec<_> = (0..count / 4 + offset)
                .map(|i| (i * 97 + i / 23) as u8)
                .collect();
            let mut expected = vec![73; count * 4 + 16];
            let mut batched = expected.clone();
            let mut selected = expected.clone();
            for case in 0..128 {
                let (levels, coeffs) = if case < 6 {
                    (LEVELS[case % 2], COEFFICIENTS[case / 2])
                } else {
                    (
                        std::array::from_fn(|_| random_float(&mut state)),
                        std::array::from_fn(|_| random_float(&mut state)),
                    )
                };
                let upload = Yuv420Upload {
                    width,
                    height,
                    y: &y[offset..],
                    u: &u[offset..],
                    v: &v[offset..],
                    levels,
                    coeffs,
                };
                assert!(upload.is_valid());
                let end = offset + count * 4;
                write_yuv420_inner::<false>(upload, &mut expected[offset..end]);
                // Exercise batch arithmetic even on hosts without hardware FMA.
                write_yuv420_inner::<true>(upload, &mut batched[offset..end]);
                write_yuv420(upload, &mut selected[offset..end]);
                assert_eq!(
                    batched, expected,
                    "batch {width}x{height}, offset={offset}, case={case}"
                );
                assert_eq!(
                    selected, expected,
                    "selected {width}x{height}, offset={offset}, case={case}"
                );
            }
        }
    }
}

#[test]
fn updates_preserve_storage_resize_and_reject_invalid_planes() {
    let mut image = RgbaImage::new(1, 1);
    for (width, height) in [(2, 2), (32, 32), (6, 10), (2, 2)] {
        let count = (width * height) as usize;
        let y: Vec<_> = (0..count).map(|i| (i * 73) as u8).collect();
        let u: Vec<_> = (0..count / 4).map(|i| (i * 31) as u8).collect();
        let v: Vec<_> = (0..count / 4).map(|i| (i * 97) as u8).collect();
        let upload = Yuv420Upload {
            width,
            height,
            y: &y,
            u: &u,
            v: &v,
            levels: LEVELS[1],
            coeffs: COEFFICIENTS[1],
        };
        let mut expected = vec![0; count * 4];
        write_yuv420_inner::<false>(upload, &mut expected);
        update_yuv420_image(&mut image, upload).unwrap();
        assert_eq!(image.dimensions(), (width, height));
        assert_eq!(image.as_raw(), &expected);
        assert_eq!(yuv420_to_rgba(upload).unwrap(), image);
        let pointer = image.as_ptr();
        update_yuv420_image(&mut image, upload).unwrap();
        assert_eq!(image.as_ptr(), pointer);
        let valid = image.clone();
        for invalid in [
            Yuv420Upload { width: 0, ..upload },
            Yuv420Upload {
                width: width + 1,
                ..upload
            },
            Yuv420Upload {
                height: height + 1,
                ..upload
            },
            Yuv420Upload {
                y: &y[..y.len() - 1],
                ..upload
            },
            Yuv420Upload {
                u: &u[..u.len() - 1],
                ..upload
            },
            Yuv420Upload {
                v: &v[..v.len() - 1],
                ..upload
            },
        ] {
            assert!(update_yuv420_image(&mut image, invalid).is_err());
            assert!(yuv420_to_rgba(invalid).is_err());
            assert_eq!(image, valid);
            assert_eq!(image.as_ptr(), pointer);
        }
    }
}

#[test]
#[ignore = "exhaustive 2^24 YUV triples across six color profiles; run in release mode"]
fn all_yuv_triples_match_portable_for_six_color_profiles() {
    let width = 512;
    let height = 512;
    let count = (width * height) as usize;
    let mut y = vec![0; count];
    let u: Vec<_> = (0..count / 4).map(|i| i as u8).collect();
    let v: Vec<_> = (0..count / 4).map(|i| (i / 256) as u8).collect();
    let mut expected = vec![0; count * 4];
    let mut selected = expected.clone();
    let mut batched = expected.clone();
    for coeffs in COEFFICIENTS {
        for levels in LEVELS {
            for group in 0..64 {
                // Each U/V block gets four successive luma values per pass.
                for (i, value) in y.iter_mut().enumerate() {
                    *value = (group * 4 + i % 2 + (i / width as usize % 2) * 2) as u8;
                }
                let upload = Yuv420Upload {
                    width,
                    height,
                    y: &y,
                    u: &u,
                    v: &v,
                    levels,
                    coeffs,
                };
                write_yuv420_inner::<false>(upload, &mut expected);
                write_yuv420_inner::<true>(upload, &mut batched);
                write_yuv420(upload, &mut selected);
                assert_eq!(
                    batched, expected,
                    "batch: group {group}, coeffs={coeffs:?}, levels={levels:?}"
                );
                assert_eq!(
                    selected, expected,
                    "selected: group {group}, coeffs={coeffs:?}, levels={levels:?}"
                );
            }
        }
    }
}
