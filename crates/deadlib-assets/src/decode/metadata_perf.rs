//! Dimension-query and multiframe-scan CPU/allocation measurements.
use super::*;
use std::hint::black_box;

#[path = "../../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "metadata_original.rs"]
mod original;

fn gif_file(path: &Path, size: u16) {
    let mut encoder = gif::Encoder::new(
        std::fs::File::create(path).unwrap(),
        size + 2,
        size + 3,
        &[1, 2, 3, 4, 5, 6],
    )
    .unwrap();
    encoder
        .write_frame(&gif::Frame {
            width: size,
            height: size,
            left: 1,
            top: 2,
            buffer: vec![1; usize::from(size).pow(2)].into(),
            ..Default::default()
        })
        .unwrap();
}

fn check(path: &Path) {
    assert_eq!(
        texture_source_size(path).map_err(|e| (std::mem::discriminant(&e), e.to_string())),
        original::texture_source_size(path)
            .map_err(|e| (std::mem::discriminant(&e), e.to_string())),
        "{path:?}"
    );
}

#[test]
fn dimensions_preserve_content_extension_and_error_behavior() {
    let dir = crate::perf_fixture::TempDir::new("metadata-parity");
    for format in [
        image::ImageFormat::Png,
        image::ImageFormat::Bmp,
        image::ImageFormat::Jpeg,
    ] {
        let source = dir.path.join("source");
        image::RgbImage::from_pixel(3, 5, image::Rgb([11, 17, 23]))
            .save_with_format(&source, format)
            .unwrap();
        let bytes = std::fs::read(source).unwrap();
        for ext in ["png", "gif", "bmp", "jpg", "unknown", ""] {
            let path = dir.path.join(format!("fixture.{ext}"));
            for len in [
                0,
                1,
                7,
                8,
                13,
                26,
                bytes.len() / 2,
                bytes.len() - 1,
                bytes.len(),
            ] {
                if len <= bytes.len() {
                    std::fs::write(&path, &bytes[..len]).unwrap();
                    check(&path);
                }
            }
        }
    }
    let path = dir.path.join("logical-canvas.png");
    gif_file(&path, 5);
    check(&path);
    assert_eq!(texture_source_size(&path).unwrap(), (7, 8));
    let bytes = std::fs::read(&path).unwrap();
    for len in 0..bytes.len() {
        std::fs::write(&path, &bytes[..len]).unwrap();
        check(&path);
    }
    for ext in ["png", "gif", "bmp", "jpg", "unknown"] {
        let path = dir.path.join(format!("invalid.{ext}"));
        std::fs::write(&path, b"not an image at all").unwrap();
        check(&path);
    }
    check(&dir.path.join("missing.png"));
    check(&dir.path);
}

fn check_name(raw: &str) {
    assert_eq!(
        crate::texture_filename_has_multiframe_hint(raw),
        original::texture_filename_has_multiframe_hint(raw),
        "{raw:?}"
    );
}

#[test]
fn multiframe_scans_preserve_shipped_and_malformed_names() {
    for name in include_str!("../../tests/support/asset_names.txt").lines() {
        check_name(name);
        check_name(&name.to_ascii_uppercase());
        check_name(&name.replace('/', "\\"));
    }
    let pieces = [
        "",
        " ",
        " 0x0",
        " 1x2",
        " 3X4",
        " 999999999999999999999x2",
        " 4x",
        " 5x6x7",
        " 1x2/",
        "_8x9",
        "\t1x2",
        "é中",
        "\0",
        ".png",
        ".1",
        ".",
        "/",
        "\\",
    ];
    for offset in 0..80 {
        let prefix = "a".repeat(offset);
        for a in pieces {
            for b in pieces {
                check_name(&format!("{prefix}{a}{b}"));
            }
        }
    }
}

#[test]
#[ignore = "optimized CPU-cycle and allocation benchmark"]
fn benchmark_metadata() {
    let dir = crate::perf_fixture::TempDir::new("metadata-bench");
    let path = dir.path.join("fixture.png");
    for (format, size) in [
        ("gif", 1),
        ("gif", 256),
        ("png", 1),
        ("png", 256),
        ("bmp", 1),
        ("bmp", 256),
        ("jpg", 32),
        ("error", 1),
    ] {
        match format {
            "gif" => gif_file(&path, size as u16),
            "error" => std::fs::write(&path, b"not an image at all").unwrap(),
            _ => image::RgbImage::from_pixel(size, size, image::Rgb([11, 17, 23]))
                .save_with_format(
                    &path,
                    match format {
                        "png" => image::ImageFormat::Png,
                        "bmp" => image::ImageFormat::Bmp,
                        _ => image::ImageFormat::Jpeg,
                    },
                )
                .unwrap(),
        }
        let work = || {
            black_box(black_box(
                texture_source_size as fn(&Path) -> image::ImageResult<(u32, u32)>,
            )(black_box(&path)))
        };
        let (_, churn) = crate::perf::measure(|| {
            let _ = black_box(work());
        });
        let label = format!("metadata/{format}/{size}");
        println!("{label}: allocation {churn:?}");
        bench::measure(&label, 128, || {
            let _ = black_box(work());
        });
    }
    let shipped: Vec<String> = include_str!("../../tests/support/asset_names.txt")
        .lines()
        .map(str::to_owned)
        .collect();
    let workloads = [
        ("shipped", shipped),
        (
            "short",
            ["", "x", " 1x2.png", "a 4X5.png", " 4x.png", " a.png"]
                .map(str::to_owned)
                .to_vec(),
        ),
        (
            "long",
            vec![
                format!("{} 4x5.png", "a".repeat(1024)),
                format!("{}.png", "a".repeat(1024)),
            ],
        ),
        (
            "spaces",
            vec![
                format!("{}.png", "a ".repeat(100)),
                format!("{} 4x5.png", "a ".repeat(100)),
            ],
        ),
    ];
    for (name, names) in workloads {
        let work = || {
            let scanner =
                black_box(crate::texture_filename_has_multiframe_hint as fn(&str) -> bool);
            for name in &names {
                black_box(scanner(black_box(name)));
            }
        };
        let (_, churn) = crate::perf::measure(work);
        let label = format!("metadata/multiframe/{name}");
        println!("{label}: allocation {churn:?}");
        bench::measure(&label, 128, work);
    }
}
