use super::*;
use crate::perf;
use image::{DynamicImage, ImageFormat};
use std::hint::black_box;

#[path = "../../../../tests/support/image_loading.rs"]
mod support;
use support::{Tree, compare, pixels};
mod baseline;

// The crop/resample sequence inside thumbnail at 12a1090ed, isolated from PNG I/O.
fn old_thumbnail_frame(image: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    let frame = imageops::crop_imm(image, 0, 0, width, height).to_image();
    imageops::thumbnail(&frame, CELL - 4, CELL - 4)
}

fn signature(result: Result<RgbaImage, Error>) -> Result<RgbaImage, String> {
    result.map_err(|error| format!("{error:?}: {error}"))
}

#[test]
fn borrowed_frames_preserve_pixels_for_small_large_and_clamped_regions() {
    for (width, height) in [
        (0, 0),
        (1, 1),
        (2, 3),
        (17, 13),
        (59, 61),
        (60, 60),
        (63, 67),
        (128, 96),
        (1024, 512),
    ] {
        let image = pixels(width, height);
        for (frame_width, frame_height) in [
            (width, height),
            (width / 2, height / 3),
            (width + 1, height + 1),
            (1, 1),
        ] {
            assert_eq!(
                thumbnail_frame(&image, frame_width, frame_height),
                old_thumbnail_frame(&image, frame_width, frame_height),
                "{width}x{height}, frame {frame_width}x{frame_height}"
            );
        }
    }
    let source = pixels(1024, 512);
    perf::assert_reduced_churn(
        || {
            black_box(old_thumbnail_frame(&source, 512, 512));
        },
        || {
            black_box(thumbnail_frame(&source, 512, 512));
        },
    );
}

#[test]
fn thumbnails_preserve_sheet_name_rules_pixels_and_errors() {
    let tree = Tree::new();
    let source = DynamicImage::ImageRgba8(pixels(137, 79));
    for name in [
        "plain.png",
        "sheet 1x1.png",
        "sheet 2x3.png",
        "sheet 4X2.png",
        "sheet 0x1.png",
        "sheet 1x0.png",
        "sheet 138x1.png",
        "sheet 1x80.png",
        "sheet 137x79.png",
        "sheet 4294967296x2 2x3.png",
        "sheet x2 2x.png",
        "sheet (res 400x200) 2x3.png",
        "sheet (RES 400x200) 2x3.png",
        "sheet 2(res ignored)x3.png",
        "sheet (res 100x100.png",
        "sheet (res 4x4) (res 2x2).png",
        "sheet İ 2x3.png",
        "sheet 3x2 0x1.png",
        "sheet 0x1 2x3.png",
        "sheet -2x3.png",
    ] {
        let path = tree.save(name, &source, ImageFormat::Png);
        assert_eq!(
            signature(thumbnail(&path)),
            signature(baseline::thumbnail(&path)),
            "{name}"
        );
    }
    for image in [
        DynamicImage::ImageLuma8(source.to_luma8()),
        DynamicImage::ImageRgb8(source.to_rgb8()),
        DynamicImage::ImageRgba16(source.to_rgba16()),
    ] {
        let path = tree.save("color 2x1.png", &image, ImageFormat::Png);
        assert_eq!(
            signature(thumbnail(&path)),
            signature(baseline::thumbnail(&path))
        );
    }
    for (width, height) in [(8193, 1), (1, 8193)] {
        let path = tree.save(
            "oversize 1x1.png",
            &DynamicImage::ImageRgba8(pixels(width, height)),
            ImageFormat::Png,
        );
        let expected = signature(baseline::thumbnail(&path));
        assert!(expected.is_err());
        assert_eq!(signature(thumbnail(&path)), expected);
    }
    for path in [tree.path.join("missing.png"), tree.path.clone()] {
        assert_eq!(
            signature(thumbnail(&path)),
            signature(baseline::thumbnail(&path))
        );
    }
    let path = tree.save("wrong 2x1.jpg", &source, ImageFormat::Png);
    assert_eq!(
        signature(thumbnail(&path)),
        signature(baseline::thumbnail(&path))
    );
    for data in [&b""[..], &b"broken"[..], &b"\x89PNG\r\n\x1a\n"[..]] {
        let path = tree.path.join("broken 2x1.png");
        fs::write(&path, data).unwrap();
        assert_eq!(
            signature(thumbnail(&path)),
            signature(baseline::thumbnail(&path))
        );
    }
}

#[test]
fn sheet_thumbnail_loading_reduces_churn() {
    let tree = Tree::new();
    let path = tree.save(
        "sheet 2x1.png",
        &DynamicImage::ImageRgba8(pixels(1024, 512)),
        ImageFormat::Png,
    );
    perf::assert_reduced_churn(
        || {
            black_box(baseline::thumbnail(&path).unwrap());
        },
        || {
            black_box(thumbnail(&path).unwrap());
        },
    );
}

#[test]
#[ignore = "manual release before/after CPU and allocation benchmark"]
fn benchmark_thumbnail_loading() {
    let tree = Tree::new();
    let old = black_box(baseline::thumbnail as fn(&Path) -> Result<RgbaImage, Error>);
    let new = black_box(thumbnail as fn(&Path) -> Result<RgbaImage, Error>);
    for (width, height) in [(64, 64), (320, 160), (1024, 512)] {
        let source = DynamicImage::ImageRgba8(pixels(width, height));
        for (variant, name) in [
            ("plain", "plain.png"),
            ("sheet1x1", "sheet 1x1.png"),
            ("sheet2x1", "sheet 2x1.png"),
            ("sheet16x8", "sheet 16x8.png"),
        ] {
            let path = tree.save(name, &source, ImageFormat::Png);
            assert_eq!(old(&path).unwrap(), new(&path).unwrap());
            compare(
                &format!("thumbnail/{variant}/{width}x{height}"),
                || old(black_box(&path)).unwrap(),
                || new(black_box(&path)).unwrap(),
            );
        }
    }
    let old = black_box(old_thumbnail_frame as fn(&RgbaImage, u32, u32) -> RgbaImage);
    let new = black_box(thumbnail_frame as fn(&RgbaImage, u32, u32) -> RgbaImage);
    for (width, height) in [(64, 64), (256, 256), (1024, 512)] {
        let source = pixels(width * 2, height * 2);
        compare(
            &format!("thumbnail_frame/{width}x{height}"),
            || old(black_box(&source), width, height),
            || new(black_box(&source), width, height),
        );
    }
}
