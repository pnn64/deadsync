//! Pixel-path correctness and allocation regressions.
#![allow(
    clippy::duplicate_mod,
    reason = "embedded backend unit tests keep their timing helper private"
)]
#[allow(dead_code)]
#[path = "../../../tests/support/allocations.rs"]
mod perf;

#[allow(dead_code)]
mod backend {
    include!("../src/lib.rs");

    mod pixel_paths_performance {
        use super::*;
        use crate::perf;
        use std::hint::black_box;
        use std::io::Write;
        fn byte_table(bits: u16) -> u8 {
            half_to_byte(bits)
        }

        fn unpack_candidate(pixel: u64) -> [f32; 4] {
            unpack_half(pixel)
        }

        fn blend_candidate(dst: [f32; 4], src: [f32; 4], mode: BlendMode) -> [f32; 4] {
            let alpha = clamp01(src[3]);
            let inv = 1.0 - alpha;
            let mut color = std::array::from_fn(|i| match mode {
                BlendMode::NoEffect => dst[i],
                BlendMode::Alpha => src[i].mul_add(alpha, dst[i] * inv),
                BlendMode::Add => src[i].mul_add(alpha, dst[i]),
                BlendMode::Multiply => src[i] * dst[i],
                BlendMode::Subtract => dst[i].mul_add(inv, -(src[i] * alpha)),
            });
            color[3] = if mode == BlendMode::NoEffect {
                dst[3]
            } else if mode == BlendMode::Subtract {
                dst[3].mul_add(inv, -src[3])
            } else {
                src[3] + dst[3] * inv
            };
            color
        }


        fn half_candidate(
            buffer: &mut [u64],
            alpha: bool,
        ) -> impl FnMut(usize, [f32; 4], BlendMode) + '_ {
            half_writer(buffer, alpha)
        }

        fn next(seed: &mut u64) -> u64 {
            *seed ^= *seed << 13;
            *seed ^= *seed >> 7;
            *seed ^= *seed << 17;
            *seed
        }

        fn assert_color(a: [f32; 4], b: [f32; 4]) {
            assert_eq!(a.map(f32::to_bits), b.map(f32::to_bits));
        }

        fn allocation(label: &str, work: impl FnOnce()) {
            let (_, counts) = perf::measure(work);
            assert_eq!(counts, perf::Churn::default());
            println!("{label}: allocation {counts:?}");
        }


        fn rows_corpus(trace: &mut impl Write) {
            for source_width in 1..=9 {
                for dest_width in source_width..=9 {
                    for source_len in (0..=36).step_by(3) {
                        for dest_len in (0..=36).step_by(3) {
                            let source: Vec<u64> = (0..source_len)
                                .map(|i| 0x9876_5432_abcd_ef01u64.wrapping_mul(i + 1))
                                .collect();
                            let mut a = vec![0x1122_3344_5566_7788; dest_len];
                            let mut b = a.clone();
                            for (src, dst) in source
                                .chunks_exact(source_width)
                                .zip(a.chunks_exact_mut(dest_width))
                            {
                                dst[..src.len()].copy_from_slice(src);
                            }
                            copy_target_rows(&source, &mut b, source_width, dest_width);
                            assert_eq!(a, b);
                            for value in b {
                                trace.write_all(&value.to_le_bytes()).unwrap();
                            }
                        }
                    }
                }
            }
        }

        #[test]
        fn cropped_and_contiguous_rows_preserve_partial_storage() {
            rows_corpus(&mut std::io::sink());
        }


        fn target(
            width: u32,
            height: u32,
            viewport: [u32; 2],
            float: bool,
            depth: bool,
        ) -> OffscreenTarget {
            let mut target = create_offscreen_target(7, width, height, viewport, float, depth);
            let mut seed = 0x9876_5432_abcd_ef01;
            for word in &mut target.pixels {
                *word = next(&mut seed) as u32;
            }
            for word in &mut target.half_pixels {
                *word = next(&mut seed);
            }
            for word in &mut target.texture.half_pixels {
                *word = next(&mut seed);
            }
            for word in target.depth.iter_mut().chain(&mut target.depth_image) {
                *word = f32::from_bits(next(&mut seed) as u32);
            }
            for byte in target.texture.image.as_mut() {
                *byte = next(&mut seed) as u8;
            }
            target
        }

        fn capacities(target: &OffscreenTarget) -> [usize; 6] {
            [
                target.pixels.capacity(),
                target.half_pixels.capacity(),
                target.texture.half_pixels.capacity(),
                target.depth.capacity(),
                target.depth_image.capacity(),
                target.texture.image.as_raw().capacity(),
            ]
        }


        const MODES: [BlendMode; 5] = [
            BlendMode::NoEffect,
            BlendMode::Alpha,
            BlendMode::Add,
            BlendMode::Multiply,
            BlendMode::Subtract,
        ];


        fn copy_candidate<const ALPHA: bool>(target: &mut OffscreenTarget) {
            copy_target_pixels::<ALPHA>(target);
        }

        fn raster_with(
            writer: &mut impl FnMut(usize, [f32; 4], BlendMode),
            depth: &mut [f32],
            size: usize,
            mode: deadlib_render_core::DepthMode,
        ) {
            let vertex = |x, y| ScreenVertexTexColor {
                inv_w: 1.0,
                z: 0.4,
                x,
                y,
                u: 0.4,
                v: 0.6,
                color: [0.4, 0.5, 0.6, 1.0],
            };
            let vertices = [
                vertex(0.0, 0.0),
                vertex(size as f32, 0.0),
                vertex(0.0, size as f32),
            ];
            let texels = Texels {
                bytes: &[255; 16],
                half: &[],
                width: 2,
                height: 2,
            };
            rasterize_triangle_tex_color(
                &black_box(vertices[0]),
                &black_box(vertices[1]),
                &black_box(vertices[2]),
                black_box(BlendMode::NoEffect),
                false,
                black_box(texels),
                SamplerDesc {
                    filter: SamplerFilter::Nearest,
                    wrap: SamplerWrap::Clamp,
                    mipmaps: false,
                },
                true,
                size,
                size,
                0,
                size,
                writer,
                &mut DepthRows {
                    pixels: depth,
                    unorm16: false,
                    mode,
                },
            );
        }


    }
}
