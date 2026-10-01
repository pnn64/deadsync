use crate::perf;
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/gradient_symmetry/baseline.rs"
    ));
}

fn palette(count: usize, pattern: usize) -> Vec<[f32; 4]> {
    let unusual = [
        -1.0,
        -0.0,
        0.0,
        f32::from_bits(1),
        0.125,
        0.5,
        1.0,
        2.0,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::from_bits(0x7fa12345),
    ];
    (0..count)
        .map(|i| {
            std::array::from_fn(|channel| {
                if pattern == 0 {
                    ((i * 79 + channel * 53) % 256) as f32 / 255.0
                } else {
                    unusual[(i * 3 + channel + pattern) % unusual.len()]
                }
            })
        })
        .collect()
}

#[test]
fn mirrored_generation_preserves_all_rgba_bytes_across_palette_and_float_boundaries() {
    for count in [1, 2, 3, 7, 16, 31, 32, 33, 63, 64, 65, 129] {
        for pattern in 0..13 {
            let colors = palette(count, pattern);
            let old = baseline::mine_gradient_texture(&colors);
            let new = mine_gradient_texture(&colors);
            assert_eq!(old.dimensions(), new.dimensions());
            assert_eq!(
                old.as_raw(),
                new.as_raw(),
                "palette={count}, pattern={pattern}"
            );
        }
    }
    assert!(std::panic::catch_unwind(|| baseline::mine_gradient_texture(&[])).is_err());
    assert!(std::panic::catch_unwind(|| mine_gradient_texture(&[])).is_err());
}

#[test]
fn cached_radial_geometry_is_bit_exact_when_reflected_on_either_axis() {
    let profile = &*MINE_GRADIENT_PROFILE;
    let size = MINE_GRADIENT_FRAME_SIZE as usize;
    for y in 0..size {
        for x in 0..size {
            let index = y * size + x;
            for mirrored in [y * size + size - 1 - x, (size - 1 - y) * size + x] {
                assert_eq!(profile.layers[index], profile.layers[mirrored]);
                assert_eq!(
                    profile.edge_alpha[index].to_bits(),
                    profile.edge_alpha[mirrored].to_bits()
                );
            }
        }
    }
}

#[test]
fn mirrored_generation_keeps_heap_scratch_at_zero() {
    let _ = &*MINE_GRADIENT_PROFILE;
    for count in [1, 7, 64] {
        let colors = palette(count, 0);
        perf::assert_churn_budget(1, count * 64 * 64 * 4, || {
            black_box(mine_gradient_texture(&colors));
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
fn benchmark_gradient_symmetry() {
    for count in [1, 7, 64, 129] {
        let colors = palette(count, 0);
        assert_eq!(
            baseline::mine_gradient_texture(&colors),
            mine_gradient_texture(&colors)
        );
        pairs(|label, new| {
            perf::measure_sampled(
                &format!("bulk_gradient_{count}_{label}"),
                128,
                count * 64 * 64,
                || {
                    let colors = black_box(&colors);
                    black_box(if new {
                        mine_gradient_texture(colors)
                    } else {
                        baseline::mine_gradient_texture(colors)
                    });
                },
            );
        });
    }
    let input = RgbaImage::from_fn(1024, 64, |x, y| {
        Rgba([
            (x * 7 + y) as u8,
            (x + y * 19) as u8,
            (x * 31) as u8,
            [0, 1, 127, 255][(x + y) as usize % 4],
        ])
    });
    let colors = mine_gradient_samples(&input, [0, 0], [1024, 64], 64).unwrap();
    assert_eq!(
        baseline::mine_gradient_texture(&colors),
        mine_gradient_texture(&colors)
    );
    pairs(|label, new| {
        perf::measure_sampled(
            &format!("bulk_gradient_full_{label}"),
            128,
            64 * 64 * 64,
            || {
                let colors =
                    mine_gradient_samples(black_box(&input), [0, 0], [1024, 64], 64).unwrap();
                black_box(if new {
                    mine_gradient_texture(&colors)
                } else {
                    baseline::mine_gradient_texture(&colors)
                });
            },
        );
    });
}
