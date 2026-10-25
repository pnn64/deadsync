//! Exact-output checks and CPU/allocation measurements for texture preparation.
use super::*;
use std::hint::black_box;

#[path = "../../../../tests/support/kernel_bench.rs"]
pub(super) mod bench;
#[path = "../preparation_original.rs"]
mod original;

fn pixels(width: u32, height: u32) -> RgbaImage {
    let mut state = 0x1234_5678u32;
    RgbaImage::from_fn(width, height, |_, _| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        image::Rgba(state.to_le_bytes())
    })
}

#[test]
fn luminance_matches_for_every_rgb_and_hint_combination() {
    // All 2^24 RGB triples, including every division boundary; vary alpha too.
    let input = RgbaImage::from_fn(4096, 4096, |x, y| {
        let rgb = x + y * 4096;
        image::Rgba([
            rgb as u8,
            (rgb >> 8) as u8,
            (rgb >> 16) as u8,
            (rgb ^ (rgb >> 9)) as u8,
        ])
    });
    for grayscale in [false, true] {
        for alphamap in [false, true] {
            let hints = TextureHints {
                grayscale,
                alphamap,
                ..Default::default()
            };
            let mut expected = input.clone();
            let mut actual = input.clone();
            original::apply_texture_hints(&mut expected, &hints);
            apply_texture_hints(&mut actual, &hints);
            assert_eq!(actual.as_raw(), expected.as_raw());
        }
    }
    // ImageBuffer can own trailing storage; preserve the original iteration extent.
    for length in 0..32 {
        let mut actual = RgbaImage::from_raw(0, 0, vec![77; length]).unwrap();
        let mut expected = actual.clone();
        let hints = TextureHints {
            grayscale: true,
            ..Default::default()
        };
        original::apply_texture_hints(&mut expected, &hints);
        apply_texture_hints(&mut actual, &hints);
        assert_eq!(actual.as_raw(), expected.as_raw());
    }
}

#[test]
fn streamed_zoom_matches_original_rounding_and_multistep_resizes() {
    for width in [1, 2, 3, 7, 16, 31] {
        for height in [1, 2, 3, 7, 16, 31] {
            let input = pixels(width, height);
            for size in [[1, 1], [2, 3], [5, 7], [16, 16], [33, 19], [64, 64]] {
                assert_eq!(
                    zoom_image(input.clone(), size),
                    original::zoom_image(input.clone(), size),
                    "{width}x{height} -> {size:?}"
                );
            }
        }
    }
}

fn png_file(path: &Path, size: u32, indexed: bool, transparent: bool) {
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, size, size);
    encoder.set_color(if indexed {
        png::ColorType::Indexed
    } else {
        png::ColorType::Rgb
    });
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_palette(
        (0..=255u8)
            .flat_map(|i| [i, i ^ 0x55, i ^ 0xaa])
            .collect::<Vec<_>>(),
    );
    if transparent {
        encoder.set_trns(if indexed {
            (0..=255u8).collect::<Vec<_>>()
        } else {
            vec![0, 1, 0, 2, 0, 3]
        });
    }
    let mut writer = encoder.write_header().unwrap();
    let length = size as usize * size as usize * if indexed { 1 } else { 3 };
    writer
        .write_image_data(&(0..length).map(|i| (i * 71) as u8).collect::<Vec<_>>())
        .unwrap();
}

#[test]
fn png_metadata_matches_original_pixels_and_errors() {
    let dir = std::env::temp_dir().join(format!("deadsync-preparation-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fixture.png");
    for indexed in [false, true] {
        for transparent in [false, true] {
            png_file(&path, 17, indexed, transparent);
            for key in [false, true] {
                assert_eq!(
                    decode_png(&path, key).unwrap(),
                    original::decode_png(&path, key).unwrap()
                );
            }
            let bytes = std::fs::read(&path).unwrap();
            for length in [0, 8, 32, bytes.len() / 2, bytes.len() - 1] {
                std::fs::write(&path, &bytes[..length]).unwrap();
                let a = decode_png(&path, true).map_err(|e| e.to_string());
                let b = original::decode_png(&path, true).map_err(|e| e.to_string());
                assert_eq!(a, b);
            }
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn native_png_fixtures_match_starting_decoder() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/itgmania-song-lua-micro");
    let mut checked = 0;
    for directory in [
        "texture-surface",
        "bitmap-file-decoding",
        "indexed-bitmap-files",
        "model-texture-preparation",
        "texture-sprite-preparation/bitmap-sprite-preparation",
    ] {
        for entry in std::fs::read_dir(root.join(directory)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "png") {
                for key in [false, true] {
                    assert_eq!(
                        decode_png(&path, key).map_err(|e| e.to_string()),
                        original::decode_png(&path, key).map_err(|e| e.to_string()),
                        "{} key={key}",
                        path.display()
                    );
                }
                checked += 1;
            }
        }
    }
    assert!(checked >= 20, "native PNG fixtures missing: {checked}");
}

fn allocations(label: &str, work: impl FnOnce()) {
    let (_, counts) = crate::perf::measure(work);
    println!("{label}: allocation {counts:?}");
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_luma() {
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
            let source = pixels(size, size);
            let mut image = source.clone();
            let label = format!("luma/{name}/{size}");
            allocations(&label, || apply_texture_hints(&mut image, &hints));
            // Repeated transforms use the same evolving buffers in both binaries.
            // Arithmetic and hint selection are independent of pixel values.
            let iterations = if grayscale || alphamap {
                (2_000_000 / (size as usize * size as usize)).clamp(16, 100_000)
            } else {
                100_000
            };
            bench::measure(&label, iterations, || {
                let transform: fn(&mut RgbaImage, &TextureHints) = black_box(apply_texture_hints);
                transform(black_box(&mut image), black_box(&hints));
                black_box(image.as_raw());
            });
        }
    }
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_zoom() {
    for (width, height, size) in [
        (16, 16, [32, 32]),
        (256, 256, [128, 128]),
        (256, 256, [512, 512]),
        (1024, 512, [64, 32]),
        (1, 256, [2, 512]),
    ] {
        let image = pixels(width, height);
        let label = format!("zoom/{width}x{height}/{}x{}", size[0], size[1]);
        allocations(&label, || {
            black_box(zoom_image(image.clone(), size));
        });
        bench::measure(&label, 32, || {
            let transform: fn(RgbaImage, [u32; 2]) -> RgbaImage = black_box(zoom_image);
            black_box(transform(black_box(image.clone()), black_box(size)));
        });
    }
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_png_metadata() {
    let dir = std::env::temp_dir().join(format!(
        "deadsync-preparation-bench-{:010}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fixture.png");
    for indexed in [false, true] {
        for transparent in [false, true] {
            for size in [16, 256] {
                png_file(&path, size, indexed, transparent);
                let label = format!("png/indexed={indexed}/trns={transparent}/{size}");
                allocations(&label, || {
                    black_box(decode_png(&path, false).unwrap());
                });
                bench::measure(&label, 100, || {
                    let decode: fn(&Path, bool) -> image::ImageResult<RgbaImage> =
                        black_box(decode_png);
                    black_box(decode(black_box(&path), false).unwrap());
                });
            }
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
