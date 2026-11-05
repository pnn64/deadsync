//! Exact pixel-path comparisons and CPU/allocation benchmarks.
#![allow(
    clippy::duplicate_mod,
    reason = "embedded backend unit tests keep their timing helper private"
)]
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

#[allow(dead_code)]
mod backend {
    include!("../src/lib.rs");

    mod pixel_paths_performance {
        use super::*;
        use crate::{bench, perf};
        use std::hint::black_box;
        use std::io::Write;
        mod original {
            include!("support/pixel_paths_original.rs");
        }
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

        fn pack_candidate(color: [f32; 4]) -> u64 {
            let [r, g, b, a] = color.map(original::encode_half).map(u64::from);
            r | (g << 16) | (b << 32) | (a << 48)
        }
        fn unpack_inline(pixel: u64) -> [f32; 4] {
            std::array::from_fn(|i| original::decode_half((pixel >> (i * 16)) as u16))
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

        fn compare(label: &str, iterations: usize, mut work: impl FnMut(bool)) {
            for (current, variant) in [(false, "original"), (true, "current")] {
                allocation(&format!("{label}/{variant}"), || work(current));
            }
            bench::compare(label, iterations, work);
        }

        fn writer_corpus(trace: &mut impl Write, finite_trace: bool) {
            let mut seed = 0x5467_8901_2345_abcd;
            for _ in 0..65536 {
                let mut pixel = next(&mut seed);
                let mut color = std::array::from_fn(|_| f32::from_bits(next(&mut seed) as u32));
                if finite_trace {
                    // Cross-profile traces use finite arithmetic. The normal test
                    // separately compares every output bit for arbitrary inputs.
                    pixel = (0..4).fold(0, |packed, i| {
                        let bits = (pixel >> (i * 16)) as u16;
                        let bits = if bits & 0x7c00 == 0x7c00 {
                            bits & !0x0400
                        } else {
                            bits
                        };
                        packed | (u64::from(bits) << (i * 16))
                    });
                    color = color.map(|v| (v.to_bits() as u16) as f32 / 65535.0);
                }
                for alpha in [false, true] {
                    for mode in MODES {
                        let mut a = [pixel];
                        let mut b = [pixel];
                        original::half_writer(&mut a, alpha)(0, color, mode);
                        half_writer(&mut b, alpha)(0, color, mode);
                        assert_eq!(a, b);
                        trace.write_all(&b[0].to_le_bytes()).unwrap();
                    }
                }
            }
        }

        #[test]
        fn whole_writer_preserves_random_bits_and_every_blend_mode() {
            writer_corpus(&mut std::io::sink(), false);
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

        #[test]
        #[ignore = "writes deterministic conversion/writer/copy traces"]
        fn dump_pixel_traces() {
            let dir =
                std::path::PathBuf::from(std::env::var_os("DEADSYNC_PIXEL_TRACE_DIR").unwrap());
            std::fs::create_dir_all(&dir).unwrap();
            let mut trace =
                std::io::BufWriter::new(std::fs::File::create(dir.join("half.bin")).unwrap());
            for bits in 0..=u16::MAX {
                let actual = decode_half(bits).to_bits();
                assert_eq!(actual, original::decode_half(bits).to_bits());
                let byte = half_to_byte(bits);
                assert_eq!(byte, original::half_to_byte(bits));
                trace.write_all(&actual.to_le_bytes()).unwrap();
                trace.write_all(&[byte]).unwrap();
            }
            trace.flush().unwrap();
            let mut trace = std::io::BufWriter::with_capacity(
                1 << 20,
                std::fs::File::create(dir.join("writer.bin")).unwrap(),
            );
            writer_corpus(&mut trace, true);
            trace.flush().unwrap();
            let mut trace = std::io::BufWriter::with_capacity(
                1 << 20,
                std::fs::File::create(dir.join("copy.bin")).unwrap(),
            );
            rows_corpus(&mut trace);
            for (width, height) in [(1, 1), (3, 2), (8, 5), (17, 9), (64, 32)] {
                for viewport in [
                    [width, height],
                    [width / 2 + 1, height],
                    [width, height / 2 + 1],
                ] {
                    for float in [false, true] {
                        for depth in [false, true] {
                            for alpha in [false, true] {
                                let mut a = target(width, height, viewport, float, depth);
                                let mut b = target(width, height, viewport, float, depth);
                                if alpha {
                                    original::copy_target_pixels::<true>(&mut a);
                                    copy_target_pixels::<true>(&mut b);
                                } else {
                                    original::copy_target_pixels::<false>(&mut a);
                                    copy_target_pixels::<false>(&mut b);
                                }
                                assert_eq!(a.texture.image, b.texture.image);
                                assert_eq!(a.texture.half_pixels, b.texture.half_pixels);
                                assert_eq!(a.pixels, b.pixels);
                                assert_eq!(capacities(&a), capacities(&b));
                                trace.write_all(b.texture.image.as_raw()).unwrap();
                                for &value in b.half_pixels.iter().chain(&b.texture.half_pixels) {
                                    trace.write_all(&value.to_le_bytes()).unwrap();
                                }
                                for &value in &b.pixels {
                                    trace.write_all(&value.to_le_bytes()).unwrap();
                                }
                                for (&expected, &actual) in a.depth_image.iter().zip(&b.depth_image)
                                {
                                    assert_eq!(expected.to_bits(), actual.to_bits());
                                    trace.write_all(&actual.to_bits().to_le_bytes()).unwrap();
                                }
                                for value in capacities(&b) {
                                    trace.write_all(&(value as u64).to_le_bytes()).unwrap();
                                }
                            }
                        }
                    }
                }
            }
            trace.flush().unwrap();
            let mut trace = std::io::BufWriter::with_capacity(
                1 << 20,
                std::fs::File::create(dir.join("byte.bin")).unwrap(),
            );
            byte_corpus(&mut trace);
            trace.flush().unwrap();
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

        fn check_copy<const ALPHA: bool>(
            width: u32,
            height: u32,
            viewport: [u32; 2],
            float: bool,
            depth: bool,
        ) {
            let mut a = target(width, height, viewport, float, depth);
            let mut b = target(width, height, viewport, float, depth);
            let capacity = capacities(&b);
            original::copy_target_pixels::<ALPHA>(&mut a);
            copy_candidate::<ALPHA>(&mut b);
            assert_eq!(capacities(&a), capacities(&b));
            assert_eq!(capacities(&b), capacity);
            assert_eq!(a.pixels, b.pixels);
            assert_eq!(a.half_pixels, b.half_pixels);
            assert_eq!(a.texture.image, b.texture.image);
            assert_eq!(a.texture.half_pixels, b.texture.half_pixels);
            assert_eq!(
                a.depth.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                b.depth.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
            );
            assert_eq!(
                a.depth_image
                    .iter()
                    .map(|x| x.to_bits())
                    .collect::<Vec<_>>(),
                b.depth_image
                    .iter()
                    .map(|x| x.to_bits())
                    .collect::<Vec<_>>()
            );
        }

        #[test]
        fn copied_viewport_buffers_and_capacities_match() {
            for (width, height) in [(1, 1), (3, 2), (8, 5), (17, 9), (64, 32)] {
                for viewport in [
                    [width, height],
                    [width, height / 2 + 1],
                    [width / 2 + 1, height],
                    [1, 1],
                ] {
                    for float in [false, true] {
                        for depth in [false, true] {
                            check_copy::<false>(width, height, viewport, float, depth);
                            check_copy::<true>(width, height, viewport, float, depth);
                        }
                    }
                }
            }
        }

        const MODES: [BlendMode; 5] = [
            BlendMode::NoEffect,
            BlendMode::Alpha,
            BlendMode::Add,
            BlendMode::Multiply,
            BlendMode::Subtract,
        ];

        #[test]
        fn candidate_float_bits_and_half_storage_match() {
            for bits in 0..=u16::MAX {
                assert_eq!(original::half_to_byte(bits), byte_table(bits));
                for lane in 0..4 {
                    let shift = lane * 16;
                    let pixel = (0x3c00_3800_3400_3000u64 & !(0xffff << shift))
                        | (u64::from(bits) << shift);
                    assert_color(original::unpack_half(pixel), unpack_candidate(pixel));
                    let color = original::unpack_half(pixel);
                    assert_eq!(original::pack_half(color), pack_candidate(color));
                    for alpha in [false, true] {
                        let mut a = [pixel];
                        let mut b = [pixel];
                        original::half_writer(&mut a, alpha)(0, [f32::NAN; 4], BlendMode::NoEffect);
                        half_candidate(&mut b, alpha)(0, [f32::NAN; 4], BlendMode::NoEffect);
                        assert_eq!(a, b);
                    }
                }
            }
            for bits in (0x3880_0000u32..0x4780_0000).step_by(8192) {
                for delta in [4095, 4096, 4097] {
                    for sign in [0, 0x8000_0000] {
                        for lane in 0..4 {
                            let mut color = [0.4, 0.7, 0.9, 1.0];
                            color[lane] = f32::from_bits((bits + delta) | sign);
                            assert_eq!(original::pack_half(color), pack_candidate(color));
                        }
                    }
                }
            }
            let mut seed = 0x1234_5678_9101_1121;
            for _ in 0..131_072 {
                let pixel = next(&mut seed);
                assert_color(original::unpack_half(pixel), unpack_candidate(pixel));
                let a = std::array::from_fn(|_| f32::from_bits(next(&mut seed) as u32));
                let b = std::array::from_fn(|_| f32::from_bits(next(&mut seed) as u32));
                assert_eq!(original::pack_half(a), pack_candidate(a));
                assert_eq!(original::pack_half(b), pack_candidate(b));
                for mode in MODES {
                    assert_color(
                        original::blend_color(a, b, mode),
                        blend_candidate(a, b, mode),
                    );
                }
            }
        }

        #[test]
        #[ignore = "candidate CPU and allocation benchmark"]
        fn benchmark_pixel_candidates() {
            let mut seed = 0x1357_2468_abcd_ef01;
            for kind in 0..6 {
                let values: Vec<_> = (0..1024)
                    .map(|_| {
                        let word = next(&mut seed);
                        match kind {
                            0 => word & 0x83ff_83ff_83ff_83ff | 0x3800_3800_3800_3800,
                            1 => word,
                            2 => 0x3c00_0000_0000_0000,
                            3 => 0,
                            4 => word & 0x83ff_83ff_83ff_83ff,
                            _ => word | 0x7c00_7c00_7c00_7c00,
                        }
                    })
                    .collect();
                for (variant, candidate) in [
                    ("current", unpack_candidate as fn(u64) -> [f32; 4]),
                    ("control", unpack_inline),
                ] {
                    let functions = [original::unpack_half as fn(u64) -> [f32; 4], candidate];
                    compare(&format!("unpack/{variant}/{kind}"), 2000, |current| {
                        let f = black_box(functions[usize::from(current)]);
                        for &value in &values {
                            black_box(f(black_box(value)));
                        }
                    });
                }
            }
            for kind in 0..5 {
                let values: Vec<_> = (0..1024)
                    .map(|_| {
                        let bits = next(&mut seed) as u16;
                        match kind {
                            0 => bits,
                            1 => (bits & 0x3ff) | 0x3000,
                            2 => 0,
                            3 => 0x3c00,
                            _ => bits | 0x8000,
                        }
                    })
                    .collect();
                let functions = [original::half_to_byte as fn(u16) -> u8, byte_table];
                compare(&format!("half-byte/{kind}"), 2000, |current| {
                    let f = black_box(functions[usize::from(current)]);
                    for &value in &values {
                        black_box(f(black_box(value)));
                    }
                });
            }
            for kind in 0..5 {
                let values: Vec<_> = (0..1024)
                    .map(|_| {
                        std::array::from_fn(|_| {
                            let word = next(&mut seed) as u32;
                            match kind {
                                0 => (word as u16) as f32 / 65535.0 + 0.001,
                                1 => f32::from_bits(word),
                                2 => 0.0,
                                3 => f32::from_bits((word & 0x807f_ffff) | 0x3880_0000),
                                _ => f32::from_bits(word | 0x7f80_0000),
                            }
                        })
                    })
                    .collect();
                let functions = [original::pack_half as fn([f32; 4]) -> u64, pack_candidate];
                compare(&format!("pack/{kind}"), 2000, |current| {
                    let f = black_box(functions[usize::from(current)]);
                    for &value in &values {
                        black_box(f(black_box(value)));
                    }
                });
            }
            let colors: Vec<_> = (0..256)
                .map(|_| std::array::from_fn(|_| (next(&mut seed) as u16) as f32 / 65535.0))
                .collect();
            for mode in MODES {
                let functions = [
                    original::blend_color as fn([f32; 4], [f32; 4], BlendMode) -> [f32; 4],
                    blend_candidate,
                ];
                compare(&format!("blend/{mode:?}"), 2000, |current| {
                    let f = black_box(functions[usize::from(current)]);
                    for pair in colors.windows(2) {
                        black_box(f(black_box(pair[0]), black_box(pair[1]), black_box(mode)));
                    }
                });
                for alpha in [false, true] {
                    let mut buffers = [
                        vec![0x3c00_3800_3400_3000; 256],
                        vec![0x3c00_3800_3400_3000; 256],
                    ];
                    compare(&format!("half-write/{mode:?}/{alpha}"), 2000, |current| {
                        let buf = &mut buffers[usize::from(current)];
                        if current {
                            let mut f = half_candidate(buf, alpha);
                            for (index, &color) in colors.iter().enumerate() {
                                f(black_box(index), black_box(color), black_box(mode));
                            }
                        } else {
                            let mut f = original::half_writer(buf, alpha);
                            for (index, &color) in colors.iter().enumerate() {
                                f(black_box(index), black_box(color), black_box(mode));
                            }
                        }
                    });
                }
            }
        }

        #[test]
        #[ignore = "candidate viewport and byte-writer CPU benchmark"]
        fn benchmark_copy_candidates() {
            for width in [4, 16, 256, 1024] {
                for height in [1, 256] {
                    let source = vec![0x1234_5678_9abc_def0u64; width * height];
                    let mut buffers = [vec![0; source.len()], vec![0; source.len()]];
                    compare(&format!("rows/{width}/{height}"), 1000, |current| {
                        let buffer = black_box(&mut buffers[usize::from(current)]);
                        if current {
                            copy_target_rows(
                                black_box(&source),
                                buffer,
                                black_box(width),
                                black_box(width),
                            );
                        } else {
                            for (source, dest) in black_box(&source)
                                .chunks_exact(black_box(width))
                                .zip(buffer.chunks_exact_mut(black_box(width)))
                            {
                                dest[..source.len()].copy_from_slice(source);
                            }
                        }
                        black_box(buffer);
                    });
                }
            }
            for size in [16, 256, 1024] {
                for float in [false, true] {
                    for depth in [false, true] {
                        for full in [false, true] {
                            let viewport = [if full { size } else { size / 2 + 1 }, size];
                            let mut targets = [
                                target(size, size, viewport, float, depth),
                                target(size, size, viewport, float, depth),
                            ];
                            let iterations = (4_000_000 / (size * size)).clamp(4, 4000);
                            compare(
                                &format!("copy/{size}/{float}/{depth}/{full}"),
                                iterations as usize,
                                |current| {
                                    let target = black_box(&mut targets[usize::from(current)]);
                                    if current {
                                        copy_candidate::<true>(target);
                                    } else {
                                        original::copy_target_pixels::<true>(target);
                                    }
                                    black_box(target.texture.image.as_raw());
                                },
                            );
                        }
                    }
                }
            }
        }

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

        fn raster_depth<const OLD: bool>(
            pixels: &mut [u32],
            depth: &mut [f32],
            size: usize,
            mode: deadlib_render_core::DepthMode,
        ) {
            if OLD {
                raster_with(&mut original::byte_writer(pixels), depth, size, mode);
            } else {
                raster_with(&mut byte_writer(pixels), depth, size, mode);
            }
        }

        fn byte_corpus(trace: &mut impl Write) {
            for lane in 0..4 {
                for value in 0..=255u32 {
                    let pixel = (0x1234_5678 & !(255 << (lane * 8))) | (value << (lane * 8));
                    let mut a = [pixel];
                    let mut b = [pixel];
                    original::byte_writer(&mut a)(0, [f32::NAN; 4], BlendMode::NoEffect);
                    byte_writer(&mut b)(0, [f32::NAN; 4], BlendMode::NoEffect);
                    assert_eq!(a, b);
                    assert_eq!(b[0], pixel);
                    trace.write_all(&b[0].to_le_bytes()).unwrap();
                }
            }
            let mut seed = 0x7865_4321_abcd_ef01;
            for _ in 0..65536 {
                let pixel = next(&mut seed) as u32;
                let color = std::array::from_fn(|_| f32::from_bits(next(&mut seed) as u32));
                for mode in MODES {
                    let mut a = [0xabcdef01, pixel, 0x98765432];
                    let mut b = a;
                    original::byte_writer(&mut a)(1, color, mode);
                    byte_writer(&mut b)(1, color, mode);
                    assert_eq!(a, b);
                    trace.write_all(&b[1].to_le_bytes()).unwrap();
                }
            }
            for size in [16, 64] {
                for mode in deadlib_render_core::DepthMode::ALL {
                    let mut a = vec![0x12345678; size * size];
                    let mut b = a.clone();
                    let mut da = vec![1.0; size * size];
                    let mut db = da.clone();
                    raster_depth::<true>(&mut a, &mut da, size, mode);
                    raster_depth::<false>(&mut b, &mut db, size, mode);
                    assert_eq!(a, b);
                    assert!(b.iter().all(|&p| p == 0x12345678));
                    for (x, y) in da.iter().zip(&db) {
                        assert_eq!(x.to_bits(), y.to_bits());
                        trace.write_all(&y.to_bits().to_le_bytes()).unwrap();
                    }
                    if matches!(
                        mode,
                        deadlib_render_core::DepthMode::Write
                            | deadlib_render_core::DepthMode::TestWrite
                    ) {
                        assert!(db.iter().any(|&z| z < 1.0));
                    }
                }
            }
        }

        #[test]
        fn depth_only_byte_writes_preserve_pixels_and_raster_depth() {
            byte_corpus(&mut std::io::sink());
            let mut original_buffer = [0u32];
            let mut current_buffer = [0u32];
            let old = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                original::byte_writer(&mut original_buffer)(1, [1.0; 4], BlendMode::NoEffect)
            }));
            let new = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                byte_writer(&mut current_buffer)(1, [1.0; 4], BlendMode::NoEffect)
            }));
            let message = |error: Box<dyn std::any::Any + Send>| {
                if let Some(s) = error.downcast_ref::<String>() {
                    s.clone()
                } else {
                    error.downcast_ref::<&str>().unwrap().to_string()
                }
            };
            assert_eq!(message(old.unwrap_err()), message(new.unwrap_err()));
            assert_eq!(original_buffer, current_buffer);
        }

        #[test]
        #[ignore = "release byte-writer and complete depth-only raster benchmark"]
        fn benchmark_byte_candidates() {
            let colors: Vec<_> = (0..256)
                .map(|i| [i as f32 / 255.0, 0.4, 0.7, 0.6])
                .collect();
            for (name, mode, alpha) in [
                ("NoEffect", BlendMode::NoEffect, 0.6),
                ("Alpha", BlendMode::Alpha, 0.6),
                ("Add", BlendMode::Add, 0.6),
                ("Multiply", BlendMode::Multiply, 0.6),
                ("Subtract", BlendMode::Subtract, 0.6),
                ("AlphaOpaque", BlendMode::Alpha, 1.0),
                ("AlphaTransparent", BlendMode::Alpha, 0.0),
            ] {
                let mut buffers = [vec![0x12345678; 256], vec![0x12345678; 256]];
                compare(&format!("byte-write/{name}"), 2000, |current| {
                    let buffer = &mut buffers[usize::from(current)];
                    if current {
                        let mut write = byte_writer(buffer);
                        for (i, &color) in colors.iter().enumerate() {
                            let mut c = color;
                            c[3] = alpha;
                            write(black_box(i), black_box(c), black_box(mode));
                        }
                    } else {
                        let mut write = original::byte_writer(buffer);
                        for (i, &color) in colors.iter().enumerate() {
                            let mut c = color;
                            c[3] = alpha;
                            write(black_box(i), black_box(c), black_box(mode));
                        }
                    }
                });
            }
            for size in [16, 64] {
                let mut pixels = [vec![0x12345678; size * size], vec![0x12345678; size * size]];
                let mut depth = [vec![1.0; size * size], vec![1.0; size * size]];
                compare(
                    &format!("depth-raster/{size}"),
                    (500000 / (size * size)).clamp(32, 1000),
                    |current| {
                        let index = usize::from(current);
                        depth[index].fill(1.0);
                        if current {
                            raster_depth::<false>(
                                &mut pixels[index],
                                &mut depth[index],
                                size,
                                deadlib_render_core::DepthMode::TestWrite,
                            );
                        } else {
                            raster_depth::<true>(
                                &mut pixels[index],
                                &mut depth[index],
                                size,
                                deadlib_render_core::DepthMode::TestWrite,
                            );
                        }
                        black_box((&pixels[index], &depth[index]));
                    },
                );
            }
        }
    }
}
