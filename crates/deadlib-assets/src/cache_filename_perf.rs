//! Cache filename equality and CPU/allocation measurements.
use super::*;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "cache_filename_original.rs"]
mod original;

#[test]
fn cache_filenames_keep_hashes_roots_and_missing_source_behavior() {
    let dir = crate::perf_fixture::TempDir::new("cache-filename-parity");
    let source = dir.path.join("source\u{e9}\u{4e2d}.png");
    fs::write(&source, []).unwrap();
    for path in [&source, &dir.path, &source.with_extension("missing")] {
        for enabled in [false, true] {
            for cache in [
                "",
                "cache",
                "cache/",
                "/",
                "C:",
                "C:\\",
                "C:\\cache\\",
                "\\\\server\\share\\cache",
                "./a/../cache",
                "cache\u{e9}\u{4e2d}",
                "cache\0",
            ] {
                assert_eq!(
                    dynamic_image_cache_path_for(
                        path,
                        BannerCacheOptions { enabled },
                        Path::new(cache)
                    ),
                    original::cache_path_before(
                        path,
                        BannerCacheOptions { enabled },
                        Path::new(cache)
                    ),
                    "{path:?}, {cache:?}"
                );
            }
        }
    }
}

#[test]
#[ignore = "optimized CPU and allocation benchmark"]
fn benchmark_cache_filenames() {
    let dir = crate::perf_fixture::TempDir::new("cache-filename-bench");
    let source = dir.path.join("source.png");
    fs::write(&source, []).unwrap();
    for (name, cache) in [
        ("empty", PathBuf::new()),
        ("short", PathBuf::from("cache")),
        ("long", PathBuf::from("cache/directory/".repeat(16))),
    ] {
        for enabled in [false, true] {
            let work = || {
                black_box(
                    dynamic_image_cache_path_for
                        as fn(&Path, BannerCacheOptions, &Path) -> Option<(PathBuf, String)>,
                )(
                    black_box(&source),
                    BannerCacheOptions { enabled },
                    black_box(&cache),
                )
            };
            let (_, churn) = crate::perf::measure(|| {
                black_box(work());
            });
            let label = format!("cachepaths/{name}/{enabled}");
            println!("{label}: allocation {churn:?}");
            bench::measure(&label, 256, || {
                black_box(work());
            });
        }
    }
}
