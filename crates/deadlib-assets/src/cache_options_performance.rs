//! Exhaustive option-hash checks and native cache-filename comparisons.
use super::*;
use std::hint::black_box;
use std::io::Write;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "cache_options_original.rs"]
mod original;

fn corpus(trace: &mut impl Write) {
    for enabled in [false, true] {
        let options = BannerCacheOptions { enabled };
        let expected = original::banner_cache_opthash(options);
        let actual = banner_cache_opthash(options);
        assert_eq!(actual, expected);
        trace.write_all(&actual.to_le_bytes()).unwrap();
    }
    let dir = crate::perf_fixture::TempDir::new("cache-options-parity");
    let source = dir.path.join("日本 image.png");
    std::fs::write(&source, b"source").unwrap();
    for enabled in [false, true] {
        for cache in [
            Path::new(""),
            Path::new("relative/cache"),
            dir.path.as_path(),
        ] {
            let a = original::dynamic_image_cache_path_for(
                &source,
                BannerCacheOptions { enabled },
                cache,
            );
            let b = dynamic_image_cache_path_for(&source, BannerCacheOptions { enabled }, cache);
            assert_eq!(a, b);
            let (a, b) = (a.unwrap(), b.unwrap());
            assert_eq!(a.0.capacity(), b.0.capacity());
            assert_eq!(a.1.capacity(), b.1.capacity());
        }
    }
    let missing = dir.path.join("missing");
    assert!(
        original::dynamic_image_cache_path_for(
            &missing,
            BannerCacheOptions { enabled: true },
            &dir.path
        )
        .is_none()
    );
    assert!(
        dynamic_image_cache_path_for(&missing, BannerCacheOptions { enabled: true }, &dir.path)
            .is_none()
    );
}

#[test]
fn both_option_hashes_and_native_filenames_match() {
    corpus(&mut std::io::sink());
}

#[test]
#[ignore = "writes deterministic option-hash trace"]
fn dump_options_trace() {
    let path = std::env::var_os("DEADSYNC_OPTIONS_TRACE_OUT").unwrap();
    corpus(&mut std::fs::File::create(path).unwrap());
}

type OptionsFn = fn(BannerCacheOptions) -> u64;
type PathFn = fn(&Path, BannerCacheOptions, &Path) -> Option<(PathBuf, String)>;

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_cache_options() {
    let functions: [OptionsFn; 2] = [original::banner_cache_opthash, banner_cache_opthash];
    for mode in [0, 1, 2] {
        let values: Vec<_> = (0..128)
            .map(|i| BannerCacheOptions {
                enabled: mode == 1 || (mode == 2 && i & 1 != 0),
            })
            .collect();
        let label = format!("options/hash/{mode}");
        for (variant, f) in functions.into_iter().enumerate() {
            let (_, counts) = crate::perf::measure(|| {
                for &value in &values {
                    black_box(f(value));
                }
            });
            assert_eq!(counts, crate::perf::Churn::default());
            println!(
                "{label}/{}: allocation {counts:?}",
                ["original", "current"][variant]
            );
        }
        bench::compare(&label, 10000, |current| {
            let f = black_box(functions[usize::from(current)]);
            for &value in &values {
                black_box(f(black_box(value)));
            }
        });
    }
    let dir = crate::perf_fixture::TempDir::new("cache-options-benchmark");
    let source = dir.path.join("source.png");
    std::fs::write(&source, b"source").unwrap();
    let functions: [PathFn; 2] = [
        original::dynamic_image_cache_path_for,
        dynamic_image_cache_path_for,
    ];
    for enabled in [false, true] {
        let label = format!("options/path/{enabled}");
        let options = BannerCacheOptions { enabled };
        for f in functions {
            drop(f(&source, options, &dir.path));
        }
        for (variant, f) in functions.into_iter().enumerate() {
            let (_, counts) =
                crate::perf::measure(|| drop(black_box(f(&source, options, &dir.path))));
            println!(
                "{label}/{}: allocation {counts:?}",
                ["original", "current"][variant]
            );
        }
        bench::compare(&label, 1000, |current| {
            black_box(black_box(functions[usize::from(current)])(
                black_box(&source),
                black_box(options),
                black_box(&dir.path),
            ));
        });
    }
}
