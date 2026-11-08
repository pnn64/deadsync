//! Differential checks and CPU-cycle benchmarks of production binary16 paths.
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod alloc;
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "support/half_original.rs"]
mod original;

#[allow(dead_code)]
mod backend {
    include!("../src/lib.rs");

    mod half_kernels {
        use super::*;
        use crate::{alloc, bench, original};
        use std::hint::black_box;

        #[test]
        fn decode_preserves_every_binary16_bit_pattern() {
            for bits in 0..=u16::MAX {
                assert_eq!(
                    decode_half(bits).to_bits(),
                    original::decode_half(bits).to_bits(),
                    "{bits:04x}"
                );
            }
        }

        #[test]
        fn encode_preserves_rounding_boundaries_and_nan_signs() {
            // All exponent/sign combinations, normal rounding ties and their neighbors.
            for upper in 0..=u16::MAX {
                for lower in [0, 1, 0x0fff, 0x1000, 0x1001, 0x1fff, 0x7fff, 0xffff] {
                    let bits = (u32::from(upper) << 16) | lower;
                    let value = f32::from_bits(bits);
                    assert_eq!(
                        encode_half(value),
                        original::encode_half(value),
                        "{bits:08x}"
                    );
                }
            }
            // Every midpoint between finite binary16 values, including subnormal ties.
            for bits in 0..0x7bff {
                let lo = original::decode_half(bits);
                let hi = original::decode_half(bits + 1);
                let midpoint = ((lo + hi) * 0.5).to_bits();
                for value_bits in [midpoint - 1, midpoint, midpoint + 1] {
                    for sign in [0, 0x8000_0000] {
                        let value = f32::from_bits(value_bits | sign);
                        assert_eq!(
                            encode_half(value),
                            original::encode_half(value),
                            "{:08x}",
                            value.to_bits()
                        );
                    }
                }
            }
        }

        #[test]
        fn float_capture_preserves_every_binary16_pattern_and_padding() {
            for viewport in [[256, 256], [253, 251]] {
                let mut target = create_offscreen_target(7, 256, 256, viewport, true, true);
                for (i, pixel) in target.half_pixels.iter_mut().enumerate() {
                    *pixel = u64::from(i as u16)
                        | (u64::from(i.wrapping_add(12345) as u16) << 16)
                        | (u64::from(i.wrapping_mul(73) as u16) << 32)
                        | (u64::from(i.wrapping_mul(319) as u16) << 48);
                }
                // Padding participates in the byte conversion too.
                for (i, pixel) in target.texture.half_pixels.iter_mut().enumerate() {
                    *pixel = u64::from(i as u16).wrapping_mul(0x0001_0001_0001_0001);
                }
                target.depth.fill(0.375);
                target.depth_image.fill(0.75);
                for preserve in [false, true] {
                    if preserve {
                        copy_target_pixels::<true>(&mut target);
                    } else {
                        copy_target_pixels::<false>(&mut target);
                    }
                    for (rgba, &pixel) in target
                        .texture
                        .image
                        .as_raw()
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .zip(&target.texture.half_pixels)
                    {
                        let expected: [u8; 4] = std::array::from_fn(|i| {
                            (original::decode_half((pixel >> (i * 16)) as u16).clamp(0.0, 1.0)
                                * 255.0)
                                .round() as u8
                        });
                        assert_eq!(*rgba, expected, "{pixel:016x}");
                    }
                    assert_eq!(target.texture.half_pixels[0], target.half_pixels[0]);
                    assert_eq!(target.depth_image[0], 0.375);
                    assert_eq!(
                        target.depth_image[255],
                        if viewport[0] == 256 { 0.375 } else { 0.75 }
                    );
                }
            }
        }

        fn half_fixture(count: usize, pattern: &str) -> Vec<u16> {
            (0..count)
                .map(|i| match pattern {
                    "normal" => 0x2000 + ((i * 73) % 0x1c01) as u16,
                    "subnormal" => (i % 1024) as u16 | (((i % 2) as u16) << 15),
                    "mixed" => i.wrapping_mul(319) as u16,
                    _ => unreachable!("unknown fixture"),
                })
                .collect()
        }

        fn measure(label: &str, count: usize, mut work: impl FnMut()) {
            let (_, churn) = alloc::measure(&mut work);
            println!("{label}: allocation {churn:?}");
            assert_eq!(churn, alloc::Churn::default());
            bench::measure(label, count, work);
        }

        #[test]
        #[ignore = "release CPU-cycle benchmark"]
        fn benchmark_encode() {
            for pattern in ["normal", "subnormal", "mixed"] {
                let input: Vec<_> = half_fixture(4096, pattern)
                    .into_iter()
                    .map(original::decode_half)
                    .collect();
                let mut output = vec![0; input.len()];
                measure(&format!("half/encode/{pattern}/4096"), 500, || {
                    for (out, &value) in black_box(&mut output).iter_mut().zip(black_box(&input)) {
                        *out = encode_half(value);
                    }
                    black_box(&output);
                });
            }
        }

        #[test]
        #[ignore = "release CPU-cycle benchmark"]
        fn benchmark_decode() {
            for pattern in ["normal", "subnormal", "mixed"] {
                let input = half_fixture(4096, pattern);
                let mut output = vec![0.0; input.len()];
                measure(&format!("half/decode/{pattern}/4096"), 500, || {
                    for (out, &bits) in black_box(&mut output).iter_mut().zip(black_box(&input)) {
                        *out = decode_half(bits);
                    }
                    black_box(&output);
                });
            }
        }

        #[test]
        #[ignore = "release CPU-cycle benchmark"]
        fn benchmark_writer() {
            let colors: Vec<_> = half_fixture(4096 * 4, "normal")
                .as_chunks::<4>()
                .0
                .iter()
                .map(|bits| bits.map(original::decode_half))
                .collect();
            let mut output = vec![pack_half([0.25, 0.5, 0.75, 1.0]); colors.len()];
            for mode in [BlendMode::Alpha, BlendMode::Multiply] {
                measure(&format!("half/writer/{mode:?}/4096"), 50, || {
                    // Multiplication decays into subnormals, just as fading render targets do.
                    let mut write = half_writer(black_box(&mut output), true);
                    for (index, &color) in black_box(&colors).iter().enumerate() {
                        write(index, color, black_box(mode));
                    }
                });
            }
        }

        #[test]
        #[ignore = "release CPU-cycle benchmark"]
        fn benchmark_capture() {
            for [width, height] in [[64, 64], [512, 512], [1920, 1080]] {
                for pattern in ["normal", "mixed"] {
                    let mut target =
                        create_offscreen_target(7, width, height, [width, height], true, false);
                    let samples = half_fixture(65536, pattern);
                    for (i, pixel) in target.half_pixels.iter_mut().enumerate() {
                        *pixel = (0..4).fold(0, |packed, c| {
                            packed
                                | (u64::from(samples[(i + c * 12345) % samples.len()]) << (c * 16))
                        });
                    }
                    measure(
                        &format!("half/capture/{pattern}/{width}x{height}"),
                        if width < 100 { 200 } else { 3 },
                        || {
                            copy_target_pixels::<true>(black_box(&mut target));
                            black_box(&target.texture.image);
                        },
                    );
                }
            }
        }
    }
}
