//! Compare the real file decoder and preparation pipeline with the frozen body.
use super::*;
use std::hint::black_box;
use std::io::Write;

#[path = "../../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "alpha_cleanup_kernel.rs"]
mod kernel;
#[path = "alpha_cleanup_original.rs"]
mod original;

fn raw_corpus(trace: &mut impl Write) {
    let mut count = 0;
    for non_default in [false, true] {
        for alphamap in [false, true] {
            for grayscale in [false, true] {
                for length in 0..80 {
                    let hints = TextureHints {
                        non_default,
                        alphamap,
                        grayscale,
                        ..Default::default()
                    };
                    let make = || {
                        let mut raw = Vec::with_capacity(length + 19);
                        raw.extend((0..length).map(|i| (i * 71) as u8));
                        RgbaImage::from_raw(0, 0, raw).unwrap()
                    };
                    let mut a = make();
                    let mut b = make();
                    let capacity = b.as_raw().capacity();
                    original::finish(&mut a, &hints);
                    kernel::finish(&mut b, &hints);
                    assert_eq!(a, b);
                    assert_eq!(b.as_raw().capacity(), capacity);
                    trace.write_all(b.as_raw()).unwrap();
                    count += 1;
                }
            }
        }
    }
    println!("preparation-only corpus: {count} hint/storage cases");
}

#[test]
fn preparation_kernel_preserves_storage_tails_and_capacity() {
    raw_corpus(&mut std::io::sink());
}

fn fixture(width: u32, height: u32, mode: u8) -> RgbaImage {
    RgbaImage::from_fn(width, height, |x, y| {
        let rgb = match mode {
            0 => [0, 0, 0],
            1 => [255, 255, 255],
            2 => [1, 0, 1],
            3 => [x as u8, y as u8, (x + y) as u8],
            4 => [127, 31, 241],
            _ => [if x & 1 == 0 { 255 } else { 0 }, 17, (y * 73) as u8],
        };
        image::Rgba([
            rgb[0],
            rgb[1],
            rgb[2],
            if mode == 4 {
                0
            } else {
                (x * 43 + y * 71) as u8
            },
        ])
    })
}

fn compare(path: &Path, hints: &TextureHints, trace: &mut impl Write) {
    let a = original::decode_texture(path, hints);
    let b = decode_texture(path, hints);
    match (a, b) {
        (Ok(a), Ok(b)) => {
            assert_eq!(a.source_size, b.source_size);
            assert_eq!(a.image, b.image);
            assert_eq!(a.image.as_raw().capacity(), b.image.as_raw().capacity());
            trace.write_all(&[1]).unwrap();
            for value in b
                .source_size
                .into_iter()
                .chain([b.image.width(), b.image.height()])
            {
                trace.write_all(&value.to_le_bytes()).unwrap();
            }
            trace
                .write_all(&(b.image.as_raw().capacity() as u64).to_le_bytes())
                .unwrap();
            trace.write_all(b.image.as_raw()).unwrap();
        }
        (Err(a), Err(b)) => {
            assert_eq!(std::mem::discriminant(&a), std::mem::discriminant(&b));
            assert_eq!(a.to_string(), b.to_string());
            trace.write_all(&[0]).unwrap();
        }
        _ => panic!("decode success differs"),
    }
}

fn corpus(trace: &mut impl Write) {
    let dir = crate::perf_fixture::TempDir::new("alpha-cleanup-parity");
    let path = dir.path.join("fixture.png");
    let mut count = 0;
    for (width, height) in [(1, 1), (3, 2), (16, 16), (31, 17)] {
        for mode in 0..6 {
            fixture(width, height, mode).save(&path).unwrap();
            for non_default in [false, true] {
                for alphamap in [false, true] {
                    for grayscale in [false, true] {
                        for size in 0..3 {
                            let hints = TextureHints {
                                non_default,
                                alphamap,
                                grayscale,
                                max_size: if size == 1 { Some(8) } else { None },
                                min_size: if size == 2 { Some(3) } else { None },
                                ..Default::default()
                            };
                            compare(&path, &hints, trace);
                            count += 1;
                        }
                    }
                }
            }
        }
    }
    for r in 0..4 {
        for g in 0..2 {
            for b in 0..10 {
                if (u32::from(r) * 30 + u32::from(g) * 59 + u32::from(b) * 11) / 100 != 0 {
                    continue;
                }
                RgbaImage::from_pixel(3, 3, image::Rgba([r, g, b, 255]))
                    .save(&path)
                    .unwrap();
                compare(
                    &path,
                    &TextureHints {
                        non_default: true,
                        alphamap: true,
                        ..Default::default()
                    },
                    trace,
                );
                count += 1;
            }
        }
    }
    std::fs::write(&path, b"not an image").unwrap();
    compare(
        &path,
        &TextureHints {
            non_default: true,
            alphamap: true,
            ..Default::default()
        },
        trace,
    );
    count += 1;
    compare(
        &dir.path.join("missing.png"),
        &TextureHints::default(),
        trace,
    );
    count += 1;
    println!("alpha cleanup corpus: {count} native decode results");
    raw_corpus(trace);
}

#[test]
fn alphamap_cleanup_preserves_native_pixels_hints_errors_and_sizes() {
    corpus(&mut std::io::sink());
}

#[test]
#[ignore = "writes deterministic native preparation trace"]
fn dump_alpha_trace() {
    let path = std::env::var_os("DEADSYNC_ALPHA_TRACE_OUT").unwrap();
    let mut trace =
        std::io::BufWriter::with_capacity(1024 * 1024, std::fs::File::create(path).unwrap());
    corpus(&mut trace);
}

type DecodeFn = fn(&Path, &TextureHints) -> image::ImageResult<DecodedTexture>;

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_native_alpha_cleanup() {
    for size in [16, 256, 1024] {
        for mode in [0, 1, 3] {
            let hints = TextureHints {
                non_default: true,
                alphamap: true,
                ..Default::default()
            };
            let mut images = [fixture(size, size, mode), fixture(size, size, mode)];
            let functions: [fn(&mut RgbaImage, &TextureHints); 2] =
                [original::finish, kernel::finish];
            let label = format!("prepare/body/{size}/{mode}");
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
                (2_000_000 / (size as usize * size as usize)).clamp(16, 10000),
                |current| {
                    let i = usize::from(current);
                    black_box(functions[i])(black_box(&mut images[i]), black_box(&hints));
                    black_box(images[i].as_raw());
                },
            );
        }
    }
    let dir = crate::perf_fixture::TempDir::new("alpha-cleanup-benchmark");
    let path = dir.path.join("fixture.png");
    for (name, size, mode, non_default, alphamap, grayscale) in [
        ("white", 16, 1, true, true, false),
        ("mixed", 16, 3, true, true, false),
        ("white", 256, 1, true, true, false),
        ("mixed", 256, 3, true, true, false),
        ("black", 256, 0, true, true, false),
        ("hidden", 256, 4, true, true, false),
        ("white", 1024, 1, true, true, false),
        ("gray", 256, 3, true, false, true),
        ("default", 256, 3, false, true, false),
    ] {
        fixture(size, size, mode).save(&path).unwrap();
        let hints = TextureHints {
            non_default,
            alphamap,
            grayscale,
            ..Default::default()
        };
        let functions: [DecodeFn; 2] = [original::decode_texture, decode_texture];
        for f in functions {
            drop(f(&path, &hints).unwrap());
        }
        let label = format!("decode/{name}/{size}");
        for (variant, f) in functions.into_iter().enumerate() {
            let (_, counts) = crate::perf::measure(|| drop(black_box(f(&path, &hints).unwrap())));
            println!(
                "{label}/{}: allocation {counts:?}",
                ["original", "current"][variant]
            );
        }
        bench::compare(&label, if size == 1024 { 16 } else { 64 }, |current| {
            let f = black_box(functions[usize::from(current)]);
            drop(black_box(f(black_box(&path), black_box(&hints)).unwrap()));
        });
    }
}
