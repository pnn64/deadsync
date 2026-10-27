use deadlib_assets::fix_hidden_alpha;
use image::{Rgba, RgbaImage};
use std::hint::black_box;

#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod alloc;
#[allow(dead_code)]
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;

fn original(image: &mut RgbaImage) {
    let pixels = image.as_raw().as_chunks::<4>().0;
    let first = pixels
        .iter()
        .find(|p| p[3] != 0)
        .map(|p| [p[0], p[1], p[2]]);
    let rgb = match first {
        None => [0; 3],
        Some(first) => {
            let last = pixels.iter().rev().find(|p| p[3] != 0).unwrap();
            if first == last[..3] { first } else { [0; 3] }
        }
    };
    for pixel in image.as_mut().as_chunks_mut::<4>().0 {
        if pixel[3] == 0 {
            pixel[..3].copy_from_slice(&rgb);
        }
    }
}

fn fixture(size: u32, kind: usize) -> RgbaImage {
    RgbaImage::from_fn(size, size, |x, y| {
        let i = y * size + x;
        let alpha = match kind {
            0 => 255,
            1 => 0,
            2 => {
                if i.is_multiple_of(2) {
                    0
                } else {
                    255
                }
            }
            3 => {
                if i.wrapping_mul(0x9e3779b9).count_ones().is_multiple_of(2) {
                    0
                } else {
                    127
                }
            }
            _ => {
                if i > size * size / 4 && i < size * size * 3 / 4 {
                    255
                } else {
                    0
                }
            }
        };
        Rgba([i as u8, (i >> 4) as u8, (i >> 8) as u8, alpha])
    })
}

#[test]
fn cleanup_preserves_every_byte_and_does_not_allocate() {
    for size in [0, 1, 2, 3, 7, 16, 65] {
        for kind in 0..5 {
            let source = fixture(size, kind);
            for suffix in [0, 1, 3, 4, 17] {
                let mut bytes = source.as_raw().clone();
                bytes.extend((0..suffix).map(|i| (i * 71) as u8));
                let mut actual = RgbaImage::from_raw(size, size, bytes).unwrap();
                let mut expected = actual.clone();
                original(&mut expected);
                alloc::assert_no_churn(|| fix_hidden_alpha(&mut actual));
                assert_eq!(
                    actual.as_raw(),
                    expected.as_raw(),
                    "size={size}, kind={kind}, suffix={suffix}"
                );
            }
        }
    }
    for rgb in [[0; 3], [255; 3], [12, 34, 56]] {
        let mut image = RgbaImage::from_raw(
            3,
            1,
            [
                rgb[0], rgb[1], rgb[2], 1, 99, 88, 77, 0, rgb[0], rgb[1], rgb[2], 255,
            ]
            .to_vec(),
        )
        .unwrap();
        let mut expected = image.clone();
        original(&mut expected);
        fix_hidden_alpha(&mut image);
        assert_eq!(image, expected);
    }
    let mut image = RgbaImage::from_fn(256, 1, |x, _| Rgba([31, 63, 127, x as u8]));
    let mut expected = image.clone();
    original(&mut expected);
    fix_hidden_alpha(&mut image);
    assert_eq!(image, expected);
}

#[test]
#[ignore = "CPU-cycle benchmark; run with --release --ignored --nocapture --test-threads=1"]
fn benchmark_hidden_alpha() {
    for size in [64, 512] {
        for kind in 0..5 {
            let mut image = fixture(size, kind);
            let (_, churn) = alloc::measure(|| fix_hidden_alpha(&mut image));
            println!("hidden-alpha/{size}/kind={kind}: allocation {churn:?}");
            bench::measure(&format!("hidden-alpha/{size}/kind={kind}"), 256, || {
                fix_hidden_alpha(black_box(&mut image));
                black_box(image.as_raw());
            });
        }
    }
}
