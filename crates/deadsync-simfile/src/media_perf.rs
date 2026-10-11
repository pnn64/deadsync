use super::*;
use std::hint::black_box;
#[path = "media_perf_alloc.rs"]
mod allocations;
#[path = "media_original.rs"]
mod original;
#[path = "../../../tests/perf/runtime_support.rs"]
mod support;

#[cfg(windows)]
fn invalid_name(prefix: &str, long: bool) -> PathBuf {
    use std::os::windows::ffi::OsStringExt;
    let mut units = prefix.encode_utf16().collect::<Vec<_>>();
    units.push(0xd800);
    units.extend(
        if long {
            "x".repeat(180)
        } else {
            "x".to_owned()
        }
        .encode_utf16(),
    );
    PathBuf::from(std::ffi::OsString::from_wide(&units))
}
#[cfg(not(windows))]
fn invalid_name(prefix: &str, long: bool) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    let mut bytes = prefix.as_bytes().to_vec();
    bytes.push(0xff);
    bytes.extend_from_slice(
        if long {
            "x".repeat(180)
        } else {
            "x".to_owned()
        }
        .as_bytes(),
    );
    PathBuf::from(std::ffi::OsString::from_vec(bytes))
}
fn samples() -> Vec<(&'static str, PathBuf)> {
    vec![
        ("empty", PathBuf::new()),
        ("root", PathBuf::from("/")),
        ("parent", PathBuf::from("..")),
        ("short-miss", PathBuf::from("banner.png")),
        ("short-hit", PathBuf::from("._banner.png")),
        (
            "ascii-miss-200",
            PathBuf::from(format!("{}.png", "a".repeat(200))),
        ),
        (
            "ascii-hit-200",
            PathBuf::from(format!("._{}.png", "a".repeat(200))),
        ),
        (
            "unicode-miss-64",
            PathBuf::from(format!("{}.png", "\u{97f3}".repeat(64))),
        ),
        (
            "unicode-hit-64",
            PathBuf::from(format!("._{}.png", "\u{97f3}".repeat(64))),
        ),
        ("nested-miss", PathBuf::from("Songs/Pack/Song/banner.png")),
        ("nested-hit", PathBuf::from("Songs/Pack/Song/._banner.png")),
        ("invalid-miss", invalid_name("banner", false)),
        ("invalid-hit", invalid_name("._", false)),
        ("invalid-long-miss", invalid_name("banner", true)),
        ("invalid-long-hit", invalid_name("._", true)),
    ]
}
#[test]
fn encoded_prefix_matches_lossy_filename_semantics() {
    for (_, path) in samples() {
        assert_eq!(
            is_mac_resource_fork(&path),
            original::is_mac_resource_fork_original(&path),
            "{path:?}"
        );
        let (actual, churn) = allocations::measure(|| is_mac_resource_fork(&path));
        assert_eq!(actual, original::is_mac_resource_fork_original(&path));
        assert_eq!(churn, allocations::Churn::default(), "{path:?}");
    }
    for path in [
        ".",
        "._",
        "._/",
        "_./a",
        "dir/../._a",
        "dir/./._a",
        "dir/._a/..",
        "dir/._a/.",
        "//server/share/._a",
        "C:/",
        "C:._a",
        "dir\\._a",
        "\u{fffd}._a",
        ".\u{fffd}_a",
        "\u{1f600}._a",
    ] {
        assert_eq!(
            is_mac_resource_fork(Path::new(path)),
            original::is_mac_resource_fork_original(Path::new(path)),
            "{path}"
        );
    }
}
#[test]
fn encoded_prefix_matches_native_invalid_encodings() {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        for unit in 0..=u16::MAX {
            for units in [
                vec![46, 95, unit, 120],
                vec![unit, 46, 95, 120],
                vec![46, unit, 95, 120],
            ] {
                let path = PathBuf::from(std::ffi::OsString::from_wide(&units));
                assert_eq!(
                    is_mac_resource_fork(&path),
                    original::is_mac_resource_fork_original(&path),
                    "{units:?}"
                );
            }
        }
        for units in [
            [46, 95, 0xd800, 0xdc00],
            [0xd800, 0xdc00, 46, 95],
            [46, 95, 0xdc00, 0xd800],
        ] {
            let path = PathBuf::from(std::ffi::OsString::from_wide(&units));
            assert_eq!(
                is_mac_resource_fork(&path),
                original::is_mac_resource_fork_original(&path)
            );
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        for byte in 0..=u8::MAX {
            for bytes in [
                vec![b'.', b'_', byte, b'x'],
                vec![byte, b'.', b'_', b'x'],
                vec![b'.', byte, b'_', b'x'],
            ] {
                let path = PathBuf::from(std::ffi::OsString::from_vec(bytes));
                assert_eq!(
                    is_mac_resource_fork(&path),
                    original::is_mac_resource_fork_original(&path)
                );
            }
        }
    }
}
fn original_count(paths: &[PathBuf]) -> usize {
    paths
        .iter()
        .filter(|p| original::is_mac_resource_fork_original(p))
        .count()
}
fn current_count(paths: &[PathBuf]) -> usize {
    paths.iter().filter(|p| is_mac_resource_fork(p)).count()
}
#[test]
#[ignore = "paired release benchmark"]
fn benchmark_runtime_resource_forks() {
    for (name, path) in samples() {
        let label = format!("resource-fork-{name}");
        let (a, old) = allocations::measure(|| original::is_mac_resource_fork_original(&path));
        let (b, new) = allocations::measure(|| is_mac_resource_fork(&path));
        assert_eq!(a, b);
        assert_eq!(new, allocations::Churn::default());
        if name.starts_with("invalid") {
            assert!(old.allocs > 0);
        }
        println!("ALLOC {label}: original {old:?}, current {new:?}");
        support::compare(
            &label,
            100,
            || {
                black_box(original::is_mac_resource_fork_original(black_box(&path)));
            },
            || {
                black_box(is_mac_resource_fork(black_box(&path)));
            },
        );
    }
    for mixed in [false, true] {
        let paths = (0..1024)
            .map(|i| {
                let prefix = if i % 7 == 0 { "._" } else { "" };
                let title = if mixed && i % 2 == 0 {
                    "\u{97f3}".repeat(48)
                } else {
                    "banner".to_owned()
                };
                PathBuf::from(format!("Songs/Pack/Song/{prefix}{title}-{i}.png"))
            })
            .collect::<Vec<_>>();
        let label = format!(
            "resource-fork-catalog-{}-1024",
            if mixed { "mixed" } else { "ascii" }
        );
        let (a, old) = allocations::measure(|| original_count(&paths));
        let (b, new) = allocations::measure(|| current_count(&paths));
        assert_eq!(a, b);
        assert_eq!(new, allocations::Churn::default());
        println!("ALLOC {label}: original {old:?}, current {new:?}");
        support::compare(
            &label,
            100,
            || {
                black_box(original_count(black_box(&paths)));
            },
            || {
                black_box(current_count(black_box(&paths)));
            },
        );
    }
}
