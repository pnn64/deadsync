include!("dynamic_original.rs");

use std::{
    hint::black_box,
    time::{Duration, SystemTime},
};

fn original_image(cache: &Path, source: &Path) -> Option<RgbaImage> {
    with_cached_banner_original(cache, source, load_raw_cached_banner_image_original)
}

fn original_validation(cache: &Path, source: &Path) -> Option<()> {
    with_cached_banner_original(cache, source, validate_raw_cached_banner_original)
}

fn current_validation(cache: &Path, source: &Path) -> Option<()> {
    with_cached_banner(cache, source, validate_raw_banner)
}

fn set_modified(path: &Path, seconds: u64) {
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(
            fs::FileTimes::new()
                .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)),
        )
        .unwrap();
}

#[test]
fn cache_handle_reuse_matches_original_results_and_cleanup() {
    let dir = crate::perf_fixture::TempDir::new("cache-differential");
    let source = dir.path.join("source.png");
    let cache = dir.path.join("banner.rgba");
    let image = RgbaImage::from_fn(7, 3, |x, y| image::Rgba([x as u8, y as u8, 19, 255]));
    let mut valid = Vec::new();
    write_raw_cached_banner(&mut valid, &image).unwrap();
    let mut bad_magic = valid.clone();
    bad_magic[0] ^= 1;
    let mut oversized = valid.clone();
    oversized.push(0);
    let mut overflowing = valid.clone();
    overflowing[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut empty = valid[..BANNER_CACHE_HEADER_SIZE].to_vec();
    empty[8..12].copy_from_slice(&0_u32.to_le_bytes());
    let cases = [
        valid.clone(),
        bad_magic,
        valid[..7].to_vec(),
        valid[..valid.len() - 1].to_vec(),
        oversized,
        overflowing,
        empty,
    ];
    for (index, bytes) in cases.iter().enumerate() {
        for source_age in [None, Some(3600), Some(7200), Some(10800)] {
            let run = |current, validate| {
                fs::write(&cache, bytes).unwrap();
                set_modified(&cache, 7200);
                if let Some(seconds) = source_age {
                    fs::write(&source, b"source").unwrap();
                    set_modified(&source, seconds);
                } else {
                    let _ = fs::remove_file(&source);
                }
                let result = if validate {
                    if current {
                        current_validation(&cache, &source)
                    } else {
                        original_validation(&cache, &source)
                    }
                    .map(|()| (0, 0, Vec::new()))
                } else {
                    if current {
                        load_cached_banner_image(&cache, &source)
                    } else {
                        original_image(&cache, &source)
                    }
                    .map(|image| (image.width(), image.height(), image.into_raw()))
                };
                (result, cache.exists())
            };
            for validate in [false, true] {
                assert_eq!(
                    run(false, validate),
                    run(true, validate),
                    "case {index}, source {source_age:?}, validate {validate}"
                );
            }
        }
    }
    fs::remove_file(&cache).unwrap();
    assert_eq!(
        original_image(&cache, &source),
        load_cached_banner_image(&cache, &source)
    );
    fs::create_dir(&cache).unwrap();
    assert_eq!(
        original_image(&cache, &source),
        load_cached_banner_image(&cache, &source)
    );
    assert!(cache.is_dir());
}

#[test]
fn cache_path_streaming_preserves_hashes_and_reduces_allocations() {
    for path in [
        "",
        "a",
        "a/b/c",
        r"C:\packs\song\banner.png",
        r"\\server\share\\banner.png",
        "曲/écran\\画像.png",
    ] {
        let mut original = XxHash64::with_seed(0);
        original.write(path.replace('\\', "/").as_bytes());
        assert_eq!(banner_path_hash(path), original.finish(), "{path:?}");
    }
    let dir = crate::perf_fixture::TempDir::new("cache-key");
    let source = dir.path.join("écran 画像.png");
    fs::write(&source, []).unwrap();
    for enabled in [false, true] {
        let opts = BannerCacheOptions { enabled };
        let (original, before) = crate::perf::measure(|| {
            dynamic_image_cache_path_for_original(&source, opts, &dir.path)
        });
        let (current, after) =
            crate::perf::measure(|| dynamic_image_cache_path_for(&source, opts, &dir.path));
        assert_eq!(original, current);
        assert!(after.allocs < before.allocs, "{before:?} -> {after:?}");
        assert!(
            after.allocated_bytes < before.allocated_bytes,
            "{before:?} -> {after:?}"
        );
    }
    assert_eq!(
        dynamic_image_cache_path_for(
            &source.with_extension("missing"),
            BannerCacheOptions { enabled: true },
            &dir.path
        ),
        None
    );
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_cache_path_allocations() {
    let dir = crate::perf_fixture::TempDir::new("cache-key-bench");
    let source = dir.path.join("banner.png");
    fs::write(&source, []).unwrap();
    let opts = BannerCacheOptions { enabled: true };
    let (_, original) =
        crate::perf::measure(|| dynamic_image_cache_path_for_original(&source, opts, &dir.path));
    let (_, current) =
        crate::perf::measure(|| dynamic_image_cache_path_for(&source, opts, &dir.path));
    println!("cache key churn: original {original:?}, current {current:?}");
    crate::paired_bench::compare("canonical cache key", 300, |current| {
        black_box(if current {
            dynamic_image_cache_path_for(black_box(&source), opts, &dir.path)
        } else {
            dynamic_image_cache_path_for_original(black_box(&source), opts, &dir.path)
        });
    });
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_cache_file_handle() {
    let dir = crate::perf_fixture::TempDir::new("cache-read-bench");
    let source = dir.path.join("source.png");
    let cache = dir.path.join("banner.rgba");
    fs::write(&source, b"source").unwrap();
    set_modified(&source, 3600);
    for (width, height) in [(1, 1), (256, 80), (1024, 512)] {
        let image = RgbaImage::from_pixel(width, height, image::Rgba([13, 97, 43, 255]));
        assert!(save_raw_cached_banner_image(&cache, &image));
        set_modified(&cache, 7200);
        assert_eq!(original_image(&cache, &source), Some(image.clone()));
        assert_eq!(load_cached_banner_image(&cache, &source), Some(image));
        for validate in [false, true] {
            crate::paired_bench::compare(
                &format!("cache handle/{width}x{height}/validate={validate}"),
                100,
                |current| {
                    if validate {
                        black_box(if current {
                            current_validation(&cache, &source)
                        } else {
                            original_validation(&cache, &source)
                        })
                        .unwrap();
                    } else {
                        black_box(if current {
                            load_cached_banner_image(&cache, &source)
                        } else {
                            original_image(&cache, &source)
                        })
                        .unwrap();
                    }
                },
            );
        }
    }
}
