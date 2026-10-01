use super::*;
use crate::perf;
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/gradient_preparation/baseline.rs"
    ));
}

fn pixels(width: u32, height: u32, pattern: usize) -> RgbaImage {
    RgbaImage::from_fn(width, height, |x, y| {
        let alpha = match pattern {
            0 => 255,
            1 => 0,
            _ => [0, 1, 127, 255][(x as usize + y as usize) % 4],
        };
        Rgba([
            (x * 79 + y * 13) as u8,
            (x * 7 + y * 97) as u8,
            (x * 53 + y * 29) as u8,
            alpha,
        ])
    })
}

fn colors(count: usize) -> Vec<[f32; 4]> {
    (0..count)
        .map(|index| {
            let values = [
                -1.0,
                -0.0,
                0.0,
                0.125,
                0.5,
                1.0,
                2.0,
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
            ];
            std::array::from_fn(|channel| values[(index * 3 + channel) % values.len()])
        })
        .collect()
}

fn assert_samples(old: &Option<Vec<[f32; 4]>>, new: &Option<Vec<[f32; 4]>>) {
    assert_eq!(old.is_some(), new.is_some());
    if let (Some(old), Some(new)) = (old, new) {
        assert_eq!(old.len(), new.len());
        for (old, new) in old.iter().zip(new) {
            assert_eq!(old.map(f32::to_bits), new.map(f32::to_bits));
        }
        assert_eq!(
            mine_gradient_texture_key(old),
            mine_gradient_texture_key(new)
        );
    }
}

#[test]
fn sampled_columns_preserve_alpha_weighting_interpolation_and_float_bits() {
    for width in [0, 1, 2, 3, 7, 32, 65, 129, 513] {
        for height in [0, 1, 3, 17] {
            for pattern in 0..3 {
                let image = pixels(width + 6, height + 4, pattern);
                for count in [0, 1, 2, 3, 7, 64, 129, 1024] {
                    let old =
                        baseline::mine_gradient_samples(&image, [3, 2], [width, height], count);
                    let new = mine_gradient_samples(&image, [3, 2], [width, height], count);
                    assert_samples(&old, &new);
                }
            }
        }
    }
}

#[test]
fn invalid_sampling_regions_still_panic_and_empty_regions_return_none() {
    let image = pixels(8, 8, 2);
    for (src, size) in [
        ([0, 0], [9, 1]),
        ([0, 0], [1, 9]),
        ([8, 0], [1, 1]),
        ([0, 8], [1, 1]),
        ([3, 2], [6, 7]),
    ] {
        for count in [0, 1, 64] {
            assert!(
                std::panic::catch_unwind(|| baseline::mine_gradient_samples(
                    &image, src, size, count
                ))
                .is_err()
            );
            assert!(
                std::panic::catch_unwind(|| mine_gradient_samples(&image, src, size, count))
                    .is_err()
            );
        }
    }
    for size in [[0, 8], [8, 0], [0, 0]] {
        assert_eq!(
            baseline::mine_gradient_samples(&image, [u32::MAX; 2], size, 64),
            None
        );
        assert_eq!(mine_gradient_samples(&image, [u32::MAX; 2], size, 64), None);
    }
}

#[test]
fn radial_color_reuse_preserves_every_pixel_for_palette_sizes_and_nonfinite_colors() {
    for count in [1, 2, 3, 7, 16, 32, 64, 65, 129] {
        for palette in [
            colors(count),
            (0..count)
                .map(|i| [i as f32 / count as f32, 0.25, 1.0, 0.75])
                .collect(),
        ] {
            let old = baseline::mine_gradient_texture(&palette);
            let new = mine_gradient_texture(&palette);
            assert_eq!(old.dimensions(), new.dimensions());
            assert_eq!(old.as_raw(), new.as_raw());
        }
    }
    assert!(std::panic::catch_unwind(|| baseline::mine_gradient_texture(&[])).is_err());
    assert!(std::panic::catch_unwind(|| mine_gradient_texture(&[])).is_err());
}

#[test]
fn sampled_colors_and_generated_texture_match_parent_end_to_end() {
    let image = pixels(517, 37, 2);
    for count in [1, 3, 32, 64, 129] {
        let old = baseline::mine_gradient_samples(&image, [2, 3], [513, 32], count).unwrap();
        let new = mine_gradient_samples(&image, [2, 3], [513, 32], count).unwrap();
        assert_eq!(
            baseline::mine_gradient_texture(&old),
            mine_gradient_texture(&new)
        );
    }
}

#[test]
fn sampling_allocates_only_its_result_and_generation_adds_no_heap_scratch() {
    let image = pixels(1024, 64, 2);
    for width in [1, 7, 64, 1024] {
        perf::assert_reduced_churn(
            || {
                black_box(baseline::mine_gradient_samples(
                    &image,
                    [0, 0],
                    [width, 64],
                    64,
                ));
            },
            || {
                black_box(mine_gradient_samples(&image, [0, 0], [width, 64], 64));
            },
        );
        perf::assert_churn_budget(1, 64 * 16, || {
            black_box(mine_gradient_samples(&image, [0, 0], [width, 64], 64));
        });
    }
    let _ = &*MINE_GRADIENT_PROFILE;
    for count in [1, 7, 64] {
        let palette = colors(count);
        perf::assert_churn_budget(1, count * 64 * 64 * 4, || {
            black_box(mine_gradient_texture(&palette));
        });
    }
}

fn pairs(mut work: impl FnMut(&str, bool)) {
    if std::env::var("DEADSYNC_PERF_ORDER").as_deref() == Ok("new-first") {
        work("new", true);
        work("old", false);
    } else {
        work("old", false);
        work("new", true);
    }
}

#[test]
#[ignore = "manual paired release CPU, throughput and allocation benchmark"]
fn benchmark_gradient_preparation() {
    for (name, size, count) in [
        ("wide", [1024, 64], 64),
        ("square", [64, 64], 64),
        ("upsample", [8, 64], 64),
        ("single", [1, 64], 64),
        ("one_sample", [1024, 64], 1),
    ] {
        let image = pixels(size[0], size[1], 2);
        assert_samples(
            &baseline::mine_gradient_samples(&image, [0, 0], size, count),
            &mine_gradient_samples(&image, [0, 0], size, count),
        );
        pairs(|label, new| {
            perf::measure_sampled(&format!("tex_sample_{name}_{label}"), 256, count, || {
                let image = black_box(&image);
                black_box(if new {
                    mine_gradient_samples(image, [0, 0], size, count)
                } else {
                    baseline::mine_gradient_samples(image, [0, 0], size, count)
                });
            });
        });
    }
    for count in [1, 7, 64, 129] {
        let palette = colors(count);
        assert_eq!(
            baseline::mine_gradient_texture(&palette),
            mine_gradient_texture(&palette)
        );
        pairs(|label, new| {
            perf::measure_sampled(
                &format!("tex_radial_{count}_{label}"),
                64,
                count * 64 * 64,
                || {
                    let palette = black_box(&palette);
                    black_box(if new {
                        mine_gradient_texture(palette)
                    } else {
                        baseline::mine_gradient_texture(palette)
                    });
                },
            );
        });
    }
    let image = pixels(1024, 64, 2);
    pairs(|label, new| {
        perf::measure_sampled(&format!("tex_mine_full_{label}"), 64, 64 * 64 * 64, || {
            let image = black_box(&image);
            if new {
                let palette = mine_gradient_samples(image, [0, 0], [1024, 64], 64).unwrap();
                black_box(mine_gradient_texture(&palette));
            } else {
                let palette =
                    baseline::mine_gradient_samples(image, [0, 0], [1024, 64], 64).unwrap();
                black_box(baseline::mine_gradient_texture(&palette));
            }
        });
    });
}
