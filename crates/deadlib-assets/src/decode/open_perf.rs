//! File-dispatch CPU/allocation measurements and frozen decoder checks.
use super::*;
use std::hint::black_box;

#[path = "../../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "open_original.rs"]
mod original;

fn png_file(path: &Path, size: u32) {
    let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), size, size);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(
            &(0..size as usize * size as usize * 4)
                .map(|i| (i * 71) as u8)
                .collect::<Vec<_>>(),
        )
        .unwrap();
}

fn gif_file(path: &Path, size: u16) {
    let palette: Vec<_> = (0..=255u8).flat_map(|i| [i, i ^ 85, i ^ 170]).collect();
    let mut encoder = gif::Encoder::new(
        std::fs::File::create(path).unwrap(),
        size + 2,
        size + 3,
        &palette,
    )
    .unwrap();
    encoder
        .write_frame(&gif::Frame {
            width: size,
            height: size,
            left: 1,
            top: 2,
            transparent: Some(17),
            buffer: (0..usize::from(size).pow(2))
                .map(|i| (i * 71) as u8)
                .collect::<Vec<_>>()
                .into(),
            ..Default::default()
        })
        .unwrap();
}

fn bmp_file(path: &Path, size: u32) {
    let pitch = (size as usize).div_ceil(4) * 4;
    let offset = 54 + 256 * 4;
    let mut bytes = vec![0; offset + pitch * size as usize];
    bytes[..2].copy_from_slice(b"BM");
    let len = bytes.len() as u32;
    bytes[2..6].copy_from_slice(&len.to_le_bytes());
    bytes[10..14].copy_from_slice(&(offset as u32).to_le_bytes());
    bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&size.to_le_bytes());
    bytes[22..26].copy_from_slice(&size.to_le_bytes());
    bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&8u16.to_le_bytes());
    for (i, color) in bytes[54..offset]
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .enumerate()
    {
        *color = [i as u8 ^ 170, i as u8 ^ 85, i as u8, 0];
    }
    for (i, byte) in bytes[offset..].iter_mut().enumerate() {
        *byte = (i * 71) as u8;
    }
    std::fs::write(path, bytes).unwrap();
}

fn check(path: &Path) {
    for key in [false, true] {
        assert_eq!(
            decode_image(path, key).map_err(|e| (std::mem::discriminant(&e), e.to_string())),
            original::decode_image_before(path, key)
                .map_err(|e| (std::mem::discriminant(&e), e.to_string())),
            "{path:?}, key={key}"
        );
    }
    assert_eq!(
        texture_source_size(path).map_err(|e| (std::mem::discriminant(&e), e.to_string())),
        original::texture_source_size_before(path)
            .map_err(|e| (std::mem::discriminant(&e), e.to_string())),
        "dimensions {path:?}"
    );
}

#[test]
fn dispatch_matches_frozen_pixels_error_categories_and_dimensions() {
    let dir = crate::perf_fixture::TempDir::new("open-parity");
    let path = dir.path.join("fixture.png");
    for format in ["png", "gif", "bmp"] {
        match format {
            "png" => png_file(&path, 3),
            "gif" => gif_file(&path, 3),
            _ => bmp_file(&path, 3),
        }
        let bytes = std::fs::read(&path).unwrap();
        for len in [
            0,
            1,
            7,
            8,
            13,
            26,
            54,
            bytes.len() / 2,
            bytes.len() - 1,
            bytes.len(),
        ] {
            if len > bytes.len() {
                continue;
            }
            std::fs::write(&path, &bytes[..len]).unwrap();
            check(&path);
        }
    }
    std::fs::remove_file(&path).unwrap();
    check(&path);
    check(&dir.path);
}

#[test]
fn dispatch_matches_committed_image_corpus() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/itgmania-song-lua-micro");
    let mut count = 0;
    for directory in [
        "texture-surface",
        "bitmap-file-decoding",
        "indexed-bitmap-files",
        "model-texture-preparation",
    ] {
        for entry in std::fs::read_dir(root.join(directory)).unwrap() {
            let path = entry.unwrap().path();
            if path
                .extension()
                .is_some_and(|e| e == "png" || e == "gif" || e == "bmp" || e == "jpg")
            {
                check(&path);
                count += 1;
            }
        }
    }
    assert!(count >= 20, "image corpus missing");
}

#[test]
#[ignore = "optimized CPU-cycle and allocation benchmark"]
fn benchmark_file_dispatch() {
    let dir = crate::perf_fixture::TempDir::new("open-bench");
    let path = dir.path.join("fixture.png");
    for format in ["png", "gif", "bmp"] {
        for size in [1, 16, 256] {
            match format {
                "png" => png_file(&path, size),
                "gif" => gif_file(&path, size as u16),
                _ => bmp_file(&path, size),
            }
            let work = || {
                black_box(decode_image as fn(&Path, bool) -> image::ImageResult<RgbaImage>)(
                    black_box(&path),
                    false,
                )
                .unwrap()
            };
            let (_, churn) = crate::perf::measure(|| {
                black_box(work());
            });
            let label = format!("open/{format}/{size}");
            println!("{label}: allocation {churn:?}");
            bench::measure(&label, if size < 256 { 128 } else { 16 }, || {
                black_box(work());
            });
        }
    }
    // A native-BMP rejection must close its input before the fallback decoder.
    let source = image::RgbaImage::from_pixel(1, 1, image::Rgba([13, 17, 19, 255]));
    let mut file = std::fs::File::create(&path).unwrap();
    image::codecs::bmp::BmpEncoder::new(&mut file)
        .encode(source.as_raw(), 1, 1, image::ExtendedColorType::Rgba8)
        .unwrap();
    drop(file);
    let work = || {
        black_box(decode_image as fn(&Path, bool) -> image::ImageResult<RgbaImage>)(
            black_box(&path),
            false,
        )
        .unwrap()
    };
    let (_, churn) = crate::perf::measure(|| {
        black_box(work());
    });
    println!("open/bmp/fallback: allocation {churn:?}");
    bench::measure("open/bmp/fallback", 128, || {
        black_box(work());
    });
}
