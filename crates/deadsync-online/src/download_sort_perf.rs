use super::download_sort_original as original;
use super::*;
use crate::perf::measure;
use std::hint::black_box;

#[path = "../../../tests/support/paired_bench.rs"]
mod paired_bench;

fn check(paths: &[PathBuf]) {
    let mut expected = paths.to_vec();
    original::sort_pack_paths(&mut expected);
    let mut actual = paths.to_vec();
    let capacity = actual.capacity();
    let mut retained: Vec<_> = actual
        .iter()
        .map(|p| p.as_os_str().as_encoded_bytes().as_ptr())
        .collect();
    retained.sort_unstable();
    sort_pack_paths(&mut actual);
    assert_eq!(actual, expected);
    assert_eq!(actual.capacity(), capacity);
    let mut after: Vec<_> = actual
        .iter()
        .map(|p| p.as_os_str().as_encoded_bytes().as_ptr())
        .collect();
    after.sort_unstable();
    assert_eq!(
        after, retained,
        "sorting must move the existing path buffers"
    );
    let mut reference = paths.to_vec();
    reference.sort_by(|a, b| {
        cmp_ascii_case_insensitive(&path_file_name_lossy(a), &path_file_name_lossy(b))
    });
    assert_eq!(actual, reference, "stable case-insensitive filename order");
}

#[test]
fn direct_permutation_matches_all_small_input_orders() {
    fn visit(values: &mut [usize], at: usize) {
        if at == values.len() {
            let paths: Vec<_> = values
                .iter()
                .map(|v| PathBuf::from(format!("Songs/{v}")))
                .collect();
            check(&paths);
            return;
        }
        for next in at..values.len() {
            values.swap(at, next);
            visit(values, at + 1);
            values.swap(at, next);
        }
    }
    for count in 0..=7 {
        visit(&mut (0..count).collect::<Vec<_>>(), 0);
    }
}

#[test]
fn direct_permutation_preserves_ties_unicode_and_unusual_names() {
    let mut paths: Vec<_> = [
        "",
        "/",
        ".",
        "Songs/Alpha",
        "Extra/alpha",
        "Third/ALPHA",
        "Songs/Z",
        "Songs/é",
        "Songs/É",
        "Songs/東京",
        "Songs/i\u{307}",
        "Songs/İ",
        "Songs/pack.",
        "Songs/pack ",
        "Songs/../A",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect();
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        paths.push(PathBuf::from(std::ffi::OsString::from_wide(&[
            0x41, 0xd800,
        ])));
        paths.push(PathBuf::from(std::ffi::OsString::from_wide(&[
            0x61, 0xdc00,
        ])));
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        paths.push(PathBuf::from(std::ffi::OsString::from_vec(vec![
            b'A', 0xff,
        ])));
        paths.push(PathBuf::from(std::ffi::OsString::from_vec(vec![
            b'a', 0xfe,
        ])));
    }
    for _ in 0..paths.len() {
        check(&paths);
        paths.rotate_left(1);
    }
    paths.reverse();
    check(&paths);
}

fn fixture(count: usize, style: &str) -> Vec<PathBuf> {
    let mut paths: Vec<_> = (0..count)
        .map(|i| {
            let name = match style {
                "short" => format!("{:03}", i % 1000),
                "unicode" => format!("東京 Étage Pack {i:05}"),
                _ => format!("Dance Pack {i:05} - Doubles"),
            };
            PathBuf::from(format!("Songs/{name}"))
        })
        .collect();
    if style == "shuffled" {
        let mut seed = 31_u64;
        for i in (1..count).rev() {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            paths.swap(i, (seed as usize) % (i + 1));
        }
    } else if style != "sorted" {
        paths.reverse();
    }
    paths
}

#[test]
fn sorting_removes_trivial_work_and_never_increases_allocation_traffic() {
    for (count, style) in [
        (0, "sorted"),
        (1, "sorted"),
        (24, "short"),
        (1200, "shuffled"),
        (200, "unicode"),
    ] {
        let input = fixture(count, style);
        let mut old = input.clone();
        let mut new = input.clone();
        let (_, before) = measure(|| original::sort_pack_paths(&mut old));
        let (_, after) = measure(|| sort_pack_paths(&mut new));
        assert_eq!(new, old);
        assert!(after.allocated_bytes <= before.allocated_bytes);
        assert!(after.allocs <= before.allocs);
        assert!(after.reallocs <= before.reallocs);
        if count < 2 {
            assert_eq!(after, crate::perf::Churn::default());
        }
        if style == "short" {
            assert!(after.allocated_bytes < before.allocated_bytes);
        }
    }
}

#[test]
#[ignore = "paired release benchmark; run explicitly with --ignored --nocapture"]
fn benchmark_download_folder_sorting() {
    for (count, style) in [
        (0, "sorted"),
        (1, "sorted"),
        (24, "reverse"),
        (200, "reverse"),
        (1200, "shuffled"),
        (1200, "sorted"),
        (1200, "short"),
        (200, "unicode"),
        (4096, "reverse"),
    ] {
        let input = fixture(count, style);
        let label = format!("paths-{count}-{style}");
        check(&input);
        let mut old = input.clone();
        let mut new = input.clone();
        let (_, before) = measure(|| original::sort_pack_paths(&mut old));
        let (_, after) = measure(|| sort_pack_paths(&mut new));
        assert_eq!(new, old);
        println!("{label} churn: original {before:?}, current {after:?}");
        paired_bench::compare_prepared(
            &label,
            5,
            || input.clone(),
            |mut paths, current| {
                if current {
                    sort_pack_paths(black_box(&mut paths));
                } else {
                    original::sort_pack_paths(black_box(&mut paths));
                }
                black_box(paths);
            },
        );
    }
}
