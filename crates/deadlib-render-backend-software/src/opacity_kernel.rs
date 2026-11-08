use super::*;
use std::hint::black_box;
include!("opacity_kernel_original.rs");
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;

#[test]
fn opacity_matches_original_with_every_alpha_and_buffer_tail() {
    for length in 0..=300 {
        let mut image = RgbaImage::from_raw((length / 4) as u32, 1, vec![255; length]).unwrap();
        assert_eq!(texture_is_opaque(&image), original_opaque(&image));
        for pixel in 0..length / 4 {
            for alpha in [0, 1, 127, 254, 255] {
                image.as_mut()[pixel * 4 + 3] = alpha;
                assert_eq!(texture_is_opaque(&image), original_opaque(&image));
                image.as_mut()[pixel * 4 + 3] = 255;
            }
        }
    }
    for alpha in 0..=255 {
        let image = RgbaImage::from_raw(1, 1, vec![3, 7, 11, alpha]).unwrap();
        assert_eq!(
            texture_is_opaque(&image),
            alpha == 255,
            "alpha={alpha}, dims={:?}, raw={:?}, original={}",
            image.dimensions(),
            image.as_raw(),
            original_opaque(&image)
        );
    }
}

#[test]
#[ignore = "release CPU benchmark"]
fn benchmark_opacity_kernel() {
    for size in [64, 512, 1920] {
        for kind in 0..4 {
            let mut image = RgbaImage::from_pixel(size, size, image::Rgba([71, 97, 23, 255]));
            if kind > 0 {
                let index = match kind {
                    1 => 3,
                    2 => image.as_raw().len() / 8 * 4 + 3,
                    _ => image.as_raw().len() - 1,
                };
                image.as_mut()[index] = 254;
            }
            assert_eq!(texture_is_opaque(&image), original_opaque(&image));
            bench::compare(
                &format!("opacity/{size}/kind={kind}"),
                if kind == 1 {
                    1_000_000
                } else if size < 100 {
                    5000
                } else {
                    200
                },
                |current| {
                    let scan: fn(&RgbaImage) -> bool = black_box(if current {
                        texture_is_opaque
                    } else {
                        original_opaque
                    });
                    black_box(scan(black_box(&image)));
                },
            );
        }
    }
}
