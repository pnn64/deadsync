use deadlib_assets::fix_hidden_alpha;
use image::{Rgba, RgbaImage};
use std::hint::black_box;
#[path = "../../../tests/support/perf.rs"]
mod alloc;
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "support/hidden_alpha_original.rs"]
mod original;

fn image(size: u32, kind: usize) -> RgbaImage {
    RgbaImage::from_fn(size, size, |x, y| {
        let n = x.wrapping_mul(73) ^ y.wrapping_mul(151);
        let a = match kind {
            0 => 0,
            1 => 255,
            2 => (n % 256) as u8,
            _ => {
                if n & 1 == 0 {
                    0
                } else {
                    255
                }
            }
        };
        Rgba([n as u8, (n >> 3) as u8, (n >> 7) as u8, a])
    })
}

#[test]
fn hidden_rgb_matches_original_for_all_alpha_values_and_edge_colors() {
    for size in [0, 1, 3, 17, 64] {
        for kind in 0..4 {
            for equal_edges in [false, true] {
                let mut actual = image(size, kind);
                if equal_edges && size > 1 {
                    actual.put_pixel(0, 0, Rgba([17, 99, 201, 1]));
                    actual.put_pixel(size - 1, size - 1, Rgba([17, 99, 201, 128]));
                }
                let mut expected = actual.clone();
                original::fix_hidden_alpha(&mut expected);
                alloc::assert_no_churn(|| fix_hidden_alpha(&mut actual));
                assert_eq!(actual, expected);
            }
        }
    }
    // Image buffers may contain spare complete pixels and a partial pixel.
    // Preserve the original treatment of the entire backing buffer.
    for extra in 0..8 {
        let mut actual = RgbaImage::from_raw(1, 1, vec![7; 4 + extra]).unwrap();
        let mut expected = actual.clone();
        original::fix_hidden_alpha(&mut expected);
        fix_hidden_alpha(&mut actual);
        assert_eq!(actual, expected);
    }
}

#[test]
#[ignore = "paired release CPU benchmark"]
fn benchmark_hidden_alpha_kernel() {
    for size in [64, 512] {
        for kind in 0..4 {
            let mut old = image(size, kind);
            let mut new = old.clone();
            bench::compare(
                &format!("alpha/{size}/kind={kind}"),
                if size == 64 { 4000 } else { 100 },
                |current| {
                    let fix = black_box(if current {
                        fix_hidden_alpha
                    } else {
                        original::fix_hidden_alpha
                    });
                    fix(black_box(if current { &mut new } else { &mut old }));
                },
            );
        }
    }
}
