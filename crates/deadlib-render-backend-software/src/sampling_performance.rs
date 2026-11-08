use super::*;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "sampling_original.rs"]
mod original;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

fn coordinates() -> Vec<[f32; 2]> {
    let mut state = 0x1234_5678u32;
    (0..4096)
        .map(|_| {
            std::array::from_fn(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state % 65536) as f32 / 8192.0 - 4.0
            })
        })
        .collect()
}

fn assert_color(actual: Option<[f32; 4]>, expected: Option<[f32; 4]>) {
    assert_eq!(
        actual.map(|c| c.map(f32::to_bits)),
        expected.map(|c| c.map(f32::to_bits))
    );
}

#[test]
fn sampling_preserves_coordinate_and_color_bits() {
    let image = RgbaImage::from_fn(63, 65, |x, y| {
        image::Rgba([
            (x * 17) as u8,
            (y * 27) as u8,
            (x + y) as u8,
            (x * 31 + y * 7) as u8,
        ])
    });
    let mut inputs = vec![
        0.0,
        -0.0,
        0.5,
        -0.5,
        1.0,
        -1.0,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ];
    for exponent in 0..=255u32 {
        for sign in [0, 0x8000_0000] {
            for mantissa in [0, 1, 0x3f_ffff, 0x40_0000, 0x7f_ffff] {
                inputs.push(f32::from_bits(sign | exponent << 23 | mantissa));
            }
        }
    }
    let mut state = 0x1234_5678u32;
    for _ in 0..100_000 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        inputs.push(f32::from_bits(state));
    }
    for wrap in [SamplerWrap::Clamp, SamplerWrap::Repeat] {
        let sampler = SamplerDesc {
            wrap,
            filter: SamplerFilter::Linear,
            mipmaps: false,
        };
        for &u in &inputs {
            assert_eq!(
                wrap_uv(u, wrap).to_bits(),
                original::original_wrap_uv(u, wrap).to_bits(),
                "u={:08x}",
                u.to_bits()
            );
            for v in [u, 0.0, 0.25, -0.75] {
                assert_color(
                    sample_tex_linear::<false, 4>((&image).into(), 63, 65, u, v, sampler),
                    original::original_sample_tex_linear::<false, 4>(
                        (&image).into(),
                        63,
                        65,
                        u,
                        v,
                        sampler,
                    ),
                );
            }
        }
    }
}

#[test]
fn raster_bounds_preserve_clipping_and_exceptional_coordinates() {
    let mut state = 0x1234_5678u32;
    for _ in 0..100_000 {
        let coords: [f32; 4] = std::array::from_fn(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            f32::from_bits(state)
        });
        let [a, b, c, d] = coords;
        for (width, height, start, end) in [(1, 1, 0, 1), (640, 480, 0, 480), (63, 65, 7, 49)] {
            assert_eq!(
                raster_bounds(a, b, c, d, width, height, start, end),
                original::original_raster_bounds(a, b, c, d, width, height, start, end),
                "coords={coords:?}"
            );
        }
    }
}

#[test]
fn raster_bounds_match_at_integer_boundaries() {
    let check = |value: f32| {
        assert_eq!(
            raster_bounds(0.0, value, 0.0, value, 65_537, 65_537, 0, 65_537),
            original::original_raster_bounds(0.0, value, 0.0, value, 65_537, 65_537, 0, 65_537),
            "bits={:08x}",
            value.to_bits(),
        );
    };
    for integer in -65_536..=65_536 {
        let bits = (integer as f32).to_bits();
        for bits in [bits.wrapping_sub(1), bits, bits.wrapping_add(1)] {
            let value = f32::from_bits(bits);
            check(value);
        }
    }
    let mut state = 0x1234_5678u32;
    for _ in 0..1_000_000 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let value = f32::from_bits(state);
        check(value);
    }
}

#[test]
fn repeat_indices_preserve_edges_and_general_remainders() {
    for size in (0..=257).chain([
        i32::MAX as usize,
        i32::MAX as usize + 1,
        u32::MAX as usize,
        usize::MAX,
    ]) {
        for index in (-1024..=1024).chain([i32::MIN, i32::MAX]) {
            if index == i32::MIN && size as i32 == -1 {
                // Both versions retain Rust's signed-remainder overflow panic.
                continue;
            }
            assert_eq!(
                wrap_index(index, size, SamplerWrap::Repeat),
                original::original_wrap_index(index, size, SamplerWrap::Repeat),
                "index={index}, size={size}"
            );
        }
    }
    assert!(
        std::panic::catch_unwind(|| wrap_index(i32::MIN, usize::MAX, SamplerWrap::Repeat)).is_err()
    );
    assert!(
        std::panic::catch_unwind(|| original::original_wrap_index(
            i32::MIN,
            usize::MAX,
            SamplerWrap::Repeat
        ))
        .is_err()
    );
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_sampling() {
    let coords = coordinates();
    let image = RgbaImage::from_fn(63, 65, |x, y| {
        image::Rgba([
            (x * 17) as u8,
            (y * 27) as u8,
            (x + y) as u8,
            (x * 31 + y * 7) as u8,
        ])
    });
    for wrap in [SamplerWrap::Clamp, SamplerWrap::Repeat] {
        let sampler = SamplerDesc {
            wrap,
            filter: SamplerFilter::Linear,
            mipmaps: false,
        };
        let work = || {
            for &[u, v] in black_box(&coords) {
                black_box(sample_tex_linear::<false, 4>(
                    black_box((&image).into()),
                    63,
                    65,
                    black_box(u),
                    black_box(v),
                    black_box(sampler),
                ));
            }
        };
        let label = format!("sampling/{wrap:?}");
        let (_, counts) = perf::measure(work);
        println!("{label}: allocation {counts:?}");
        bench::measure(&label, 100, work);
        let work = || {
            for &[u, v] in black_box(&coords) {
                black_box(wrap_uv(black_box(u), black_box(wrap)));
                black_box(wrap_uv(black_box(v), black_box(wrap)));
            }
        };
        bench::measure(&format!("wrap/{wrap:?}"), 100, work);
    }
    let work = || {
        for &[x, y] in black_box(&coords) {
            black_box(raster_bounds(
                black_box(x * 100.0),
                black_box(x * 100.0 + 32.25),
                black_box(y * 100.0),
                black_box(y * 100.0 + 32.25),
                black_box(640),
                black_box(480),
                black_box(0),
                black_box(480),
            ));
        }
    };
    let (_, counts) = perf::measure(work);
    println!("bounds: allocation {counts:?}");
    bench::measure("bounds", 100, work);
    let work = || {
        for _ in 0..4096 {
            black_box(wrap_index(
                black_box(-1),
                black_box(63),
                SamplerWrap::Repeat,
            ));
            black_box(wrap_index(
                black_box(63),
                black_box(63),
                SamplerWrap::Repeat,
            ));
        }
    };
    let (_, counts) = perf::measure(work);
    println!("edges: allocation {counts:?}");
    bench::measure("edges", 100, work);
    let image = RgbaImage::from_pixel(3, 3, image::Rgba([71, 97, 23, 195]));
    let sampler = SamplerDesc {
        wrap: SamplerWrap::Repeat,
        filter: SamplerFilter::Linear,
        mipmaps: false,
    };
    let work = || {
        for &[u, v] in black_box(&coords) {
            black_box(sample_tex_linear::<false, 4>(
                black_box((&image).into()),
                3,
                3,
                black_box(u),
                black_box(v),
                black_box(sampler),
            ));
        }
    };
    let (_, counts) = perf::measure(work);
    println!("sampling/small: allocation {counts:?}");
    bench::measure("sampling/small", 100, work);
}
