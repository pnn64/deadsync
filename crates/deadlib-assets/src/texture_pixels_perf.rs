use super::*;
use std::{hint::black_box, time::Instant};

include!("texture_pixels_original.rs");

fn pixels(width: u32, height: u32) -> RgbaImage {
    let mut state = 0x1234_5678u32;
    RgbaImage::from_fn(width, height, |_, _| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        image::Rgba(state.to_le_bytes())
    })
}

fn write_gif(path: &Path, width: u16, height: u16, offset: bool) {
    let palette: Vec<u8> = (0..=255u8).flat_map(|i| [i, i ^ 0x55, i ^ 0xaa]).collect();
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = gif::Encoder::new(file, width + 2, height + 3, &palette).unwrap();
    let frame = gif::Frame {
        width,
        height,
        left: u16::from(offset),
        top: u16::from(offset) * 2,
        transparent: Some(17),
        buffer: (0..usize::from(width) * usize::from(height))
            .map(|i| (i.wrapping_mul(71) ^ (i / usize::from(width))) as u8)
            .collect::<Vec<_>>()
            .into(),
        ..Default::default()
    };
    encoder.write_frame(&frame).unwrap();
}

#[test]
fn png_and_gif_match_original_pixels_and_errors() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // Native sources cover packed indices, interlacing, transparency,
    // 16-bit channels, and GIF frame/canvas offsets.
    let mut paths = Vec::new();
    for directory in [
        "texture-surface",
        "bitmap-file-decoding",
        "indexed-bitmap-files",
        "model-texture-preparation",
        "texture-sprite-preparation/bitmap-sprite-preparation",
    ] {
        for entry in std::fs::read_dir(
            root.join("tests/fixtures/itgmania-song-lua-micro")
                .join(directory),
        )
        .unwrap()
        {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "png" || e == "gif") {
                paths.push(path);
            }
        }
    }
    let mut checked = 0;
    type Decoder = fn(&Path, bool) -> image::ImageResult<RgbaImage>;
    for path in paths {
        let signature = image_signature(&path).unwrap();
        let (before, after): (Decoder, Decoder) = if signature == *b"\x89PNG\r\n\x1a\n" {
            (decode_png_original, decode_png)
        } else if &signature[..3] == b"GIF" {
            (decode_gif_original, decode_gif)
        } else {
            continue;
        };
        checked += 1;
        for key in [false, true] {
            assert_eq!(
                before(&path, key).map_err(|e| e.to_string()),
                after(&path, key).map_err(|e| e.to_string()),
                "{path:?}, key={key}"
            );
        }
    }
    assert!(checked >= 20, "exercise the committed PNG/GIF corpus");

    let dir = crate::perf_fixture::TempDir::new("pixel-parity");
    for (w, h) in [(1, 1), (3, 5), (127, 9), (256, 65)] {
        let png = dir.path.join("source.png");
        let source = pixels(w, h);
        source.save(&png).unwrap();
        assert_eq!(decode_png(&png, false).unwrap(), source);
        for offset in [false, true] {
            let gif = dir.path.join("source.gif");
            write_gif(&gif, w as u16, h as u16, offset);
            for key in [false, true] {
                assert_eq!(
                    decode_gif_original(&gif, key).unwrap(),
                    decode_gif(&gif, key).unwrap()
                );
            }
        }
        let bytes = std::fs::read(&png).unwrap();
        for length in [0, 8, 32, bytes.len() / 2] {
            std::fs::write(&png, &bytes[..length]).unwrap();
            assert_eq!(
                decode_png_original(&png, false).map_err(|e| e.to_string()),
                decode_png(&png, false).map_err(|e| e.to_string())
            );
        }
    }
}

#[test]
fn resize_matches_original_fixed_point_rounding() {
    for source in [
        [1, 1],
        [1, 7],
        [7, 1],
        [3, 5],
        [16, 9],
        [63, 37],
        [129, 257],
    ] {
        let image = pixels(source[0], source[1]);
        for target in [
            [1, 1],
            [1, 8],
            [8, 1],
            [5, 3],
            [16, 16],
            [31, 67],
            [256, 128],
        ] {
            assert_eq!(
                zoom_image_original(image.clone(), target),
                zoom_image(image.clone(), target),
                "{source:?} -> {target:?}"
            );
        }
    }
}

#[cfg(windows)]
fn thread_cycles() -> u64 {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn QueryThreadCycleTime(thread: *mut std::ffi::c_void, cycles: *mut u64) -> i32;
    }
    let mut cycles = 0;
    // SAFETY: live thread pseudo-handle and valid synchronous output pointer.
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) },
        0
    );
    cycles
}

#[cfg(not(windows))]
fn thread_cycles() -> u64 {
    0
}

#[cfg(windows)]
pub(super) fn pin_worker() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn SetThreadAffinityMask(thread: *mut std::ffi::c_void, mask: usize) -> usize;
    }
    // SAFETY: changes only the current benchmark worker's CPU affinity.
    assert_ne!(
        unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) },
        0
    );
}

pub(super) fn compare<T>(
    label: &str,
    count: usize,
    mut prepare: impl FnMut() -> T,
    mut work: impl FnMut(T, bool) -> RgbaImage,
) {
    let mut samples = Vec::new();
    for round in 0..10 {
        for variant in [round % 2, 1 - round % 2] {
            let inputs: Vec<_> = (0..count).map(|_| prepare()).collect();
            let start_cycles = thread_cycles();
            let start = Instant::now();
            let (_, churn) = crate::perf::measure(|| {
                for input in inputs {
                    black_box(work(input, black_box(variant == 1)));
                }
            });
            let nanos = start.elapsed().as_nanos() as u64;
            let cycles = thread_cycles() - start_cycles;
            if round > 0 {
                samples.push(serde_json::json!({"round":round,"current":variant == 1,
                    "iterations":count,"nanoseconds":nanos,"cycles":cycles,
                    "allocations":churn.allocs,"reallocations":churn.reallocs,
                    "allocated_bytes":churn.allocated_bytes}));
            }
        }
    }
    println!(
        "PIXEL_BENCH {}",
        serde_json::json!({"workload":label,"samples":samples})
    );
}

#[test]
#[ignore = "paired release benchmark; --ignored --nocapture --test-threads=1"]
fn benchmark_texture_pixels() {
    #[cfg(windows)]
    pin_worker();
    let dir = crate::perf_fixture::TempDir::new("pixel-bench");
    let filter = std::env::var("DEADSYNC_PIXEL_BENCH").unwrap_or_default();
    if filter.is_empty() || filter == "png" {
        for size in [64, 512, 1024] {
            let path = dir.path.join("rgba.png");
            pixels(size, size).save(&path).unwrap();
            compare(
                &format!("png_rgba_{size}"),
                20,
                || (),
                |(), current| {
                    if current {
                        decode_png(black_box(&path), false)
                    } else {
                        decode_png_original(black_box(&path), false)
                    }
                    .unwrap()
                },
            );
        }
    }
    if filter.is_empty() || filter == "gif" {
        for size in [64, 512, 1024] {
            let path = dir.path.join("indexed.gif");
            write_gif(&path, size, size, true);
            compare(
                &format!("gif_offset_{size}"),
                20,
                || (),
                |(), current| {
                    if current {
                        decode_gif(black_box(&path), false)
                    } else {
                        decode_gif_original(black_box(&path), false)
                    }
                    .unwrap()
                },
            );
        }
    }
    if filter.is_empty() || filter == "zoom" {
        for (source, target) in [(64, 128), (512, 256), (256, 1024)] {
            let image = pixels(source, source);
            compare(
                &format!("zoom_{source}_to_{target}"),
                10,
                || image.clone(),
                |image, current| {
                    if current {
                        zoom_image(black_box(image), black_box([target; 2]))
                    } else {
                        zoom_image_original(black_box(image), black_box([target; 2]))
                    }
                },
            );
        }
    }
}
