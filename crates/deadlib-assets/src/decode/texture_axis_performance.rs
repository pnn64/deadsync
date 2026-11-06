//! Exact pixel checks and paired CPU/allocation measurements.
use super::*;
use std::hint::black_box;
use std::io::Write;

#[path = "../texture_axis_original.rs"]
mod original;

#[path = "../../../../tests/support/kernel_bench.rs"]
mod bench;

fn pixels(width: u32, height: u32) -> RgbaImage {
    let mut state = 0x1357_2468u32;
    RgbaImage::from_fn(width, height, |_, _| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        image::Rgba(state.to_le_bytes())
    })
}

fn same_zoom(input: RgbaImage, size: [u32; 2], trace: &mut impl Write) {
    let expected = original::zoom_image(input.clone(), size);
    let actual = zoom_image(input, size);
    assert_eq!(actual.dimensions(), expected.dimensions());
    assert_eq!(actual.as_raw(), expected.as_raw());
    assert_eq!(actual.as_raw().capacity(), expected.as_raw().capacity());
    trace.write_all(&size[0].to_le_bytes()).unwrap();
    trace.write_all(&size[1].to_le_bytes()).unwrap();
    trace.write_all(actual.as_raw()).unwrap();
}

fn zoom_corpus(trace: &mut impl Write) {
    let mut count = 0;
    for width in [1, 2, 3, 7, 16, 31] {
        for height in [1, 2, 3, 7, 16, 31] {
            let input = pixels(width, height);
            for size in [
                [width, height],
                [width, 1],
                [1, height],
                [width, height * 2],
                [width * 2, height],
                [width, height * 5 + 1],
                [width * 5 + 1, height],
                [width, height / 2 + 1],
                [width / 2 + 1, height],
                [5, 9],
                [64, 32],
            ] {
                same_zoom(input.clone(), size, trace);
                count += 1;
            }
        }
    }
    // Every byte pair, in independent lanes, at all weights from 2 -> 3/4.
    for vertical in [false, true] {
        let input = RgbaImage::from_fn(
            if vertical { 65536 } else { 2 },
            if vertical { 2 } else { 65536 },
            |x, y| {
                let (pair, side) = if vertical { (x, y) } else { (y, x) };
                let a = pair as u8;
                let b = (pair >> 8) as u8;
                image::Rgba(if side == 0 {
                    [a, b, a ^ b, a.wrapping_add(b)]
                } else {
                    [b, a, a.wrapping_add(b), a ^ b]
                })
            },
        );
        for length in [3, 4] {
            same_zoom(
                input.clone(),
                if vertical {
                    [65536, length]
                } else {
                    [length, 65536]
                },
                trace,
            );
            count += 1;
        }
    }
    for (width, height, size) in zoom_fixtures() {
        same_zoom(pixels(width, height), size, trace);
        count += 1;
    }
    // Resize ignores excess storage, while a no-op retains it.
    for tail in 0..19 {
        let mut bytes: Vec<u8> = (0..3 * 7 * 4 + tail).map(|x| (x * 71) as u8).collect();
        bytes.reserve_exact(23);
        let input = RgbaImage::from_raw(3, 7, bytes).unwrap();
        for size in [[3, 7], [3, 19], [11, 7], [11, 19]] {
            same_zoom(input.clone(), size, trace);
            count += 1;
        }
    }
    println!("zoom corpus: {count} images");
}

fn luma_corpus(trace: &mut impl Write) {
    // All 2^24 RGB values and all four hint combinations.
    let input = RgbaImage::from_fn(4096, 4096, |x, y| {
        let rgb = x + y * 4096;
        image::Rgba([
            rgb as u8,
            (rgb >> 8) as u8,
            (rgb >> 16) as u8,
            (rgb ^ (rgb >> 9)) as u8,
        ])
    });
    for (grayscale, alphamap) in [(false, false), (true, false), (false, true), (true, true)] {
        let hints = TextureHints {
            grayscale,
            alphamap,
            ..Default::default()
        };
        let mut expected = input.clone();
        let mut actual = input.clone();
        original::apply_texture_hints(&mut expected, &hints);
        apply_texture_hints(&mut actual, &hints);
        assert_eq!(actual, expected);
        trace.write_all(actual.as_raw()).unwrap();
        for length in 0..68 {
            let mut bytes: Vec<u8> = (0..length).map(|x| (x * 73) as u8).collect();
            bytes.reserve_exact(19);
            let mut actual = RgbaImage::from_raw(0, 0, bytes).unwrap();
            let capacity = actual.as_raw().capacity();
            let mut expected = actual.clone();
            original::apply_texture_hints(&mut expected, &hints);
            apply_texture_hints(&mut actual, &hints);
            assert_eq!(actual, expected);
            assert_eq!(actual.as_raw().capacity(), capacity);
            trace.write_all(actual.as_raw()).unwrap();
        }
    }
    println!("luma corpus: 67108864 RGB/hint pairs and 272 storage tails");
}

#[test]
fn axis_identity_respects_float_coordinate_limit() {
    for x in 0..1 << 22 {
        // Reproduce equal-dimension zoom_axis's downsample branch exactly.
        let center = x as f32 + 0.5;
        let left = (center - 0.25) as u32;
        let right = (center + 0.25) as u32;
        let weight = if left == right {
            1 << 24
        } else {
            ((1.0 - (center - (left as f32 + 0.5)) / (right - left) as f32) * 16777216.0) as u32
        };
        assert_eq!((left, weight), (x, 1 << 24));
        assert!(right < 1 << 22);
    }
    assert_eq!(
        zoom_axis((1 << 22) + 1, (1 << 22) + 1)
            .nth(1 << 22)
            .unwrap()
            .1,
        (1 << 22) + 1
    );
}

#[test]
fn oversized_equal_axis_preserves_original_bounds_panic() {
    for (width, height, size) in [
        ((1 << 22) + 1, 1, [(1 << 22) + 1, 2]),
        (1, (1 << 22) + 1, [2, (1 << 22) + 1]),
    ] {
        let input = RgbaImage::new(width, height);
        let expected = std::panic::catch_unwind(|| original::zoom_image(input.clone(), size));
        let actual = std::panic::catch_unwind(|| zoom_image(input.clone(), size));
        let expected = expected.expect_err("original large equal-axis resize must panic");
        let actual = actual.expect_err("optimized large equal-axis resize must panic");
        assert_eq!(
            actual.downcast_ref::<String>(),
            expected.downcast_ref::<String>()
        );
        assert!(actual.downcast_ref::<String>().is_some());
    }
}

#[test]
fn resized_pixels_and_capacities_match() {
    zoom_corpus(&mut std::io::sink());
}

#[test]
fn exhaustive_luminance_and_storage_tails_match() {
    luma_corpus(&mut std::io::sink());
}

#[test]
#[ignore = "writes deterministic pixel trace"]
fn dump_texture_axis_trace() {
    let path = std::env::var_os("DEADSYNC_TEXTURE_AXIS_TRACE_OUT").unwrap();
    let mut trace =
        std::io::BufWriter::with_capacity(1024 * 1024, std::fs::File::create(path).unwrap());
    luma_corpus(&mut trace);
    zoom_corpus(&mut trace);
}

fn zoom_fixtures() -> Vec<(u32, u32, [u32; 2])> {
    vec![
        (16, 16, [32, 16]),
        (16, 16, [16, 32]),
        (256, 256, [512, 256]),
        (256, 256, [256, 512]),
        (257, 129, [129, 129]),
        (257, 129, [257, 65]),
        (1024, 512, [128, 512]),
        (1024, 512, [1024, 64]),
        (256, 1, [32, 1]),
        (1, 256, [1, 32]),
        (256, 256, [512, 512]),
        (256, 256, [128, 128]),
        (257, 129, [513, 259]),
        (1024, 512, [64, 32]),
        (256, 256, [256, 256]),
    ]
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_texture_axis() {
    for (width, height, size) in zoom_fixtures() {
        let input = pixels(width, height);
        let label = format!("zoom/{width}x{height}/{}x{}", size[0], size[1]);
        let functions: [fn(RgbaImage, [u32; 2]) -> RgbaImage; 2] =
            [original::zoom_image, zoom_image];
        for (variant, f) in functions.into_iter().enumerate() {
            let (_, counts) = crate::perf::measure(|| drop(black_box(f(input.clone(), size))));
            println!(
                "{label}/{}: allocation {counts:?}",
                ["original", "current"][variant]
            );
        }
        bench::compare(&label, 100, |new| {
            let f = black_box(functions[usize::from(new)]);
            black_box(f(black_box(input.clone()), black_box(size)));
        });
    }
    for size in [1, 16, 256, 1024] {
        for (name, grayscale, alphamap) in [
            ("none", false, false),
            ("gray", true, false),
            ("alpha", false, true),
            ("both", true, true),
        ] {
            let hints = TextureHints {
                grayscale,
                alphamap,
                ..Default::default()
            };
            let mut images = [pixels(size, size), pixels(size, size)];
            let functions: [fn(&mut RgbaImage, &TextureHints); 2] =
                [original::apply_texture_hints, apply_texture_hints];
            let label = format!("luma/{name}/{size}");
            for (variant, f) in functions.into_iter().enumerate() {
                let (_, counts) = crate::perf::measure(|| f(&mut images[variant], &hints));
                assert_eq!(counts, crate::perf::Churn::default());
                println!(
                    "{label}/{}: allocation {counts:?}",
                    ["original", "current"][variant]
                );
            }
            bench::compare(
                &label,
                if grayscale || alphamap {
                    (2_000_000 / (size as usize * size as usize)).clamp(16, 100_000)
                } else {
                    100_000
                },
                |new| {
                    let variant = usize::from(new);
                    black_box(functions[variant])(
                        black_box(&mut images[variant]),
                        black_box(&hints),
                    );
                    black_box(images[variant].as_raw());
                },
            );
        }
    }
}
