use super::*;
use crate::perf;
use image::{DynamicImage, ImageBuffer, ImageFormat, ImageReader};
use std::hint::black_box;

#[path = "../../../../tests/support/image_loading.rs"]
mod support;
use support::{Tree, compare, pixels};
mod baseline;

fn signature<T>(result: image::ImageResult<T>) -> Result<T, String> {
    result.map_err(|error| format!("{error:?}: {error}"))
}

#[test]
fn fallback_preserves_pixels_color_and_errors_for_extensions_and_formats() {
    let tree = Tree::new();
    let rgba = DynamicImage::ImageRgba8(pixels(17, 11));
    let rgb = DynamicImage::ImageRgb8(rgba.to_rgb8());
    for (format, image) in [
        (ImageFormat::Png, &rgba),
        (ImageFormat::Bmp, &rgba),
        (ImageFormat::Tga, &rgba),
        (ImageFormat::Tiff, &rgba),
        (ImageFormat::WebP, &rgba),
        (ImageFormat::Gif, &rgba),
        (ImageFormat::Jpeg, &rgb),
    ] {
        for name in [
            "image.png",
            "image.jpg",
            "image.bmp",
            "image.tga",
            "image.tiff",
            "image.webp",
            "image.gif",
            "image.PNG",
            "image.unknown",
            "image",
            "image.",
        ] {
            let path = tree.save(name, image, format);
            for warn in [false, true] {
                let old = signature(baseline::open_image_fallback_mode(&path, warn));
                let new = signature(crate::open_image_fallback_mode(&path, warn));
                assert_eq!(new, old, "{format:?}, {name}, warn={warn}");
            }
        }
    }
    for name in [
        "bad.png",
        "bad.jpg",
        "bad.tga",
        "bad.unknown",
        "bad",
        "bad.",
    ] {
        let path = tree.path.join(name);
        for bytes in [&b""[..], &b"not an image"[..], &b"\x89PNG\r\n\x1a\n"[..]] {
            fs::write(&path, bytes).unwrap();
            assert_eq!(
                signature(crate::open_image_fallback_mode(&path, false)),
                signature(baseline::open_image_fallback_mode(&path, false)),
                "{name}"
            );
        }
    }
    for path in [tree.path.join("missing.png"), tree.path.clone()] {
        assert_eq!(
            signature(crate::open_image_fallback_mode(&path, false)),
            signature(baseline::open_image_fallback_mode(&path, false))
        );
    }
    // The first decoder may have consumed more than its signature before failing.
    let path = tree.save("truncated.png", &rgba, ImageFormat::Png);
    let encoded = fs::read(&path).unwrap();
    for len in [16, encoded.len() / 2, encoded.len() - 12] {
        fs::write(&path, &encoded[..len]).unwrap();
        assert_eq!(
            signature(crate::open_image_fallback_mode(&path, false)),
            signature(baseline::open_image_fallback_mode(&path, false))
        );
    }
}

fn formats() -> Vec<DynamicImage> {
    let rgba = DynamicImage::ImageRgba8(pixels(19, 13));
    vec![
        DynamicImage::ImageLuma8(rgba.to_luma8()),
        DynamicImage::ImageLumaA8(rgba.to_luma_alpha8()),
        DynamicImage::ImageRgb8(rgba.to_rgb8()),
        DynamicImage::ImageRgba8(rgba.to_rgba8()),
        DynamicImage::ImageLuma16(ImageBuffer::from_fn(19, 13, |x, y| {
            image::Luma([(x * 3457 + y * 2519) as u16])
        })),
        DynamicImage::ImageLumaA16(rgba.to_luma_alpha16()),
        DynamicImage::ImageRgb16(rgba.to_rgb16()),
        DynamicImage::ImageRgba16(rgba.to_rgba16()),
        DynamicImage::ImageRgb32F(rgba.to_rgb32f()),
        DynamicImage::ImageRgba32F(rgba.to_rgba32f()),
    ]
}

#[test]
fn banner_build_preserves_all_png_color_types_and_conversion_pixels() {
    let tree = Tree::new();
    let opts = BannerCacheOptions { enabled: true };
    for source in formats() {
        let expected = source.to_rgba8();
        assert_eq!(source.clone().into_rgba8(), expected);
        if matches!(
            source,
            DynamicImage::ImageRgb32F(_) | DynamicImage::ImageRgba32F(_)
        ) {
            continue;
        }
        let path = tree.save("source.png", &source, ImageFormat::Png);
        assert_eq!(
            signature(build_cached_banner_rgba(&path, opts)),
            signature(baseline::build_cached_banner_rgba(&path, opts))
        );
        assert_eq!(build_cached_banner_rgba(&path, opts).unwrap(), expected);
    }
    for name in ["missing.png", "missing.mp4"] {
        let path = tree.path.join(name);
        assert_eq!(
            signature(build_cached_banner_rgba(&path, opts)),
            signature(baseline::build_cached_banner_rgba(&path, opts))
        );
    }
    let owned = DynamicImage::ImageRgba8(pixels(320, 80));
    let pointer = owned.as_bytes().as_ptr();
    let mut output = None;
    perf::assert_no_churn(|| output = Some(owned.into_rgba8()));
    assert_eq!(output.as_ref().unwrap().as_raw().as_ptr(), pointer);
}

#[test]
fn loading_and_banner_build_reduce_allocation_churn() {
    let tree = Tree::new();
    let source = DynamicImage::ImageRgba8(pixels(1024, 512));
    let mismatch = tree.save("wrong.jpg", &source, ImageFormat::Png);
    perf::assert_reduced_churn(
        || {
            black_box(baseline::open_image_fallback_mode(&mismatch, false).unwrap());
        },
        || {
            black_box(crate::open_image_fallback_mode(&mismatch, false).unwrap());
        },
    );
    let source = tree.save("source.png", &source, ImageFormat::Png);
    let opts = BannerCacheOptions { enabled: true };
    // Use the current loader on both sides to isolate the owned conversion.
    perf::assert_reduced_churn(
        || {
            black_box(
                crate::open_image_fallback_quiet(&source)
                    .unwrap()
                    .to_rgba8(),
            );
        },
        || {
            black_box(build_cached_banner_rgba(&source, opts).unwrap());
        },
    );
}

#[test]
#[ignore = "manual release before/after CPU and allocation benchmark"]
fn benchmark_image_loading() {
    let tree = Tree::new();
    let old_open = black_box(
        baseline::open_image_fallback_mode as fn(&Path, bool) -> image::ImageResult<DynamicImage>,
    );
    let new_open = black_box(
        crate::open_image_fallback_mode as fn(&Path, bool) -> image::ImageResult<DynamicImage>,
    );
    for (width, height) in [(1, 1), (320, 80), (1024, 512)] {
        let source = DynamicImage::ImageRgba8(pixels(width, height));
        for (variant, filename) in [
            ("correct", "correct.png"),
            ("mismatch", "mismatch.jpg"),
            ("unknown", "unknown.data"),
        ] {
            let path = tree.save(filename, &source, ImageFormat::Png);
            assert_eq!(
                old_open(&path, false).unwrap(),
                new_open(&path, false).unwrap()
            );
            compare(
                &format!("fallback/{variant}/{width}x{height}"),
                || old_open(black_box(&path), false).unwrap(),
                || new_open(black_box(&path), false).unwrap(),
            );
        }
        let path = tree.save("banner.png", &source, ImageFormat::Png);
        let opts = BannerCacheOptions { enabled: true };
        compare(
            &format!("banner/{width}x{height}"),
            || baseline::build_cached_banner_rgba(black_box(&path), opts).unwrap(),
            || build_cached_banner_rgba(black_box(&path), opts).unwrap(),
        );
    }
    for (name, bytes) in [
        ("invalid.png", b"invalid PNG".as_slice()),
        ("unknown.data", b"unrecognized".as_slice()),
    ] {
        let path = tree.path.join(name);
        fs::write(&path, bytes).unwrap();
        compare(
            &format!("fallback/{name}"),
            || old_open(black_box(&path), false).unwrap_err(),
            || new_open(black_box(&path), false).unwrap_err(),
        );
    }
    for (width, height) in [(1, 1), (320, 80), (1024, 512), (2048, 2048)] {
        let source = DynamicImage::ImageRgba8(pixels(width, height));
        let run = |variant: &str, convert: fn(DynamicImage) -> RgbaImage| {
            let convert = black_box(convert);
            perf::measure_sampled_with_setup(
                &format!("rgba_owned/{width}x{height}/{variant}"),
                32,
                1,
                || Some(source.clone()),
                |input| {
                    let image = input.take().unwrap();
                    *input = Some(DynamicImage::ImageRgba8(convert(image)));
                },
            );
        };
        if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            run("new", DynamicImage::into_rgba8);
            run("old", |image| image.to_rgba8());
        } else {
            run("old", |image| image.to_rgba8());
            run("new", DynamicImage::into_rgba8);
        }
    }
}
