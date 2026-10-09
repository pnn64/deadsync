use super::*;
use std::hint::black_box;
mod original {
    use super::*;
    include!("scan_original.rs");
}
mod allocations {
    include!("scan_perf_alloc.rs");
}
mod paired {
    include!("scan_paired_bench.rs");
}

fn pack_fixture(count: usize, order: &str) -> Vec<SongPack> {
    (0..count)
        .map(|i| {
            let key = match order {
                "reverse" => count - i,
                "shuffled" => (i * 197) % count.max(1),
                "ties" => i % 7,
                _ => i,
            };
            let name = format!("Library pack {key:05}");
            let mut pack = song_pack(&name, &name, Path::new("Songs"));
            pack.year = i as i32;
            pack.directory = PathBuf::from(format!("Songs/unique-{i}"));
            pack
        })
        .collect()
}

fn scan_fixture(count: usize, duplicates: bool) -> Vec<PackScan> {
    (0..count)
        .map(|i| {
            let key = if duplicates {
                i % 13
            } else {
                (i * 197) % count.max(1)
            };
            let name = match i % 4 {
                0 => format!("  PACK {key:05}  "),
                1 => format!("pack {key:05}"),
                2 => format!("Pack {key:05}\t"),
                _ => format!("\u{2003}pAcK {key:05}\u{2003}"),
            };
            pack_scan(&name, &name, false, None, &[], Path::new("Songs"))
        })
        .collect()
}

fn assert_sort_equal(input: &[SongPack]) {
    let mut before = input.to_vec();
    let mut after = input.to_vec();
    original::sort_song_packs(&mut before);
    sort_song_packs(&mut after);
    assert_eq!(format!("{before:?}"), format!("{after:?}"));
}

#[test]
fn pack_permutation_matches_original_for_every_small_permutation() {
    fn permute(packs: &mut [SongPack], at: usize) {
        if at == packs.len() {
            assert_sort_equal(packs);
            return;
        }
        for i in at..packs.len() {
            packs.swap(at, i);
            permute(packs, at + 1);
            packs.swap(at, i);
        }
    }
    for count in 0..=7 {
        permute(&mut pack_fixture(count, "sorted"), 0);
    }
}

#[test]
fn pack_permutation_preserves_stable_ties_and_non_ascii_names() {
    for count in [0, 1, 2, 8, 64, 1024] {
        for order in ["sorted", "reverse", "shuffled", "ties"] {
            assert_sort_equal(&pack_fixture(count, order));
        }
    }
    let names = [
        "", "same", "SAME", "\u{00e9}", "\u{00c9}", " a", "a ", "\u{2003}",
    ];
    let mut packs: Vec<_> = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let mut p = song_pack(name, name, Path::new("Songs"));
            p.year = i as i32;
            p
        })
        .collect();
    assert_sort_equal(&packs);
    sort_song_packs(&mut packs);
    let ties: Vec<_> = packs
        .iter()
        .filter(|p| p.group_name.eq_ignore_ascii_case("same"))
        .map(|p| p.year)
        .collect();
    assert_eq!(ties, [1, 2]);
}

#[test]
fn group_representatives_match_original_and_first_equal_input() {
    for count in [0, 1, 2, 16, 257, 4096] {
        for duplicates in [false, true] {
            let packs = scan_fixture(count, duplicates);
            let actual = pack_group_representatives(&packs);
            assert_eq!(actual, original::pack_group_representatives(&packs));
            for (index, pack) in packs.iter().enumerate() {
                let first = packs
                    .iter()
                    .position(|p| ci_eq(&pack.group_name, &p.group_name))
                    .unwrap();
                assert_eq!(actual[index], first);
            }
        }
    }
    let names = [
        "\u{00c9}",
        "\u{00e9}",
        "\u{00c9} ",
        "",
        "  ",
        "Same",
        "SAME",
        "same",
    ];
    let packs: Vec<_> = names
        .iter()
        .map(|name| pack_scan(name, name, false, None, &[], Path::new("Songs")))
        .collect();
    assert_eq!(pack_group_representatives(&packs), [0, 1, 0, 3, 3, 5, 5, 5]);
}

fn directory_fixture(label: &str, directories: usize, files: usize, forks: usize) -> PathBuf {
    let root = test_dir(label);
    for i in 0..directories {
        fs::create_dir(root.join(format!("Pack {i:04}"))).unwrap();
    }
    for i in 0..files {
        fs::write(root.join(format!("chart-{i:04}.ssc")), b"#TITLE:Song;").unwrap();
    }
    for i in 0..forks {
        fs::create_dir(root.join(format!("._Pack {i:04}"))).unwrap();
    }
    root
}

#[test]
fn child_directory_filter_matches_original_and_preserves_errors() {
    let root = directory_fixture("filter-regression", 4, 20, 5);
    let expected = original::child_dirs(&root).unwrap();
    assert_eq!(child_dirs(&root).unwrap(), expected);
    assert_eq!(expected.len(), 4);
    for path in [root.join("missing"), root.join("chart-0000.ssc")] {
        assert_eq!(
            child_dirs(&path).unwrap_err().kind(),
            original::child_dirs(&path).unwrap_err().kind()
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[cfg_attr(windows, ignore = "requires Windows symbolic-link privilege")]
fn child_directory_filter_preserves_symlinks() {
    let root = directory_fixture("filter-symlinks", 1, 1, 0);
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(root.join("Pack 0000"), root.join("linked-pack"))
            .unwrap();
        std::os::windows::fs::symlink_file(root.join("chart-0000.ssc"), root.join("linked-file"))
            .unwrap();
        std::os::windows::fs::symlink_dir(root.join("missing"), root.join("broken-link")).unwrap();
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("Pack 0000"), root.join("linked-pack")).unwrap();
        std::os::unix::fs::symlink(root.join("chart-0000.ssc"), root.join("linked-file")).unwrap();
        std::os::unix::fs::symlink(root.join("missing"), root.join("broken-link")).unwrap();
    }
    assert_eq!(
        child_dirs(&root).unwrap(),
        original::child_dirs(&root).unwrap()
    );
    assert_eq!(child_dirs(&root).unwrap().len(), 2);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn library_scan_allocations_remove_redundant_buffers() {
    let packs = pack_fixture(128, "shuffled");
    let mut old = packs.clone();
    let mut new = packs;
    let (_, a) = allocations::measure(|| original::sort_song_packs(&mut old));
    let (_, b) = allocations::measure(|| sort_song_packs(&mut new));
    assert_eq!(a.allocs, b.allocs + 1);
    assert_eq!(
        a.allocated_bytes,
        b.allocated_bytes + 128 * std::mem::size_of::<usize>()
    );
    let scans = scan_fixture(128, true);
    let (_, a) = allocations::measure(|| original::pack_group_representatives(&scans));
    let (_, b) = allocations::measure(|| pack_group_representatives(&scans));
    assert_eq!(a.allocs, b.allocs + 1);
    assert_eq!(a.allocated_bytes, b.allocated_bytes);
    let root = directory_fixture("filter-allocations", 8, 64, 16);
    let (_, a) = allocations::measure(|| original::child_dirs(&root).unwrap());
    let (_, b) = allocations::measure(|| child_dirs(&root).unwrap());
    assert_eq!(a.allocs, b.allocs + 144);
    assert!(a.allocated_bytes > b.allocated_bytes);
    fs::remove_dir_all(root).unwrap();
}

fn compare_sort(label: &str, input: &[SongPack]) {
    let count = if input.len() < 32 {
        2048
    } else if input.len() < 512 {
        64
    } else {
        16
    };
    let mut elapsed = [Vec::new(), Vec::new()];
    for sample in 0..10 {
        for variant in [sample % 2, 1 - sample % 2] {
            // Fresh unsorted fixtures are prepared and destroyed outside timing.
            let mut inputs = vec![input.to_vec(); count];
            let start = Instant::now();
            if variant == 0 {
                for packs in &mut inputs {
                    original::sort_song_packs(black_box(packs.as_mut_slice()));
                    black_box(&*packs);
                }
            } else {
                for packs in &mut inputs {
                    sort_song_packs(black_box(packs.as_mut_slice()));
                    black_box(&*packs);
                }
            }
            let ns = start.elapsed().as_nanos() as f64 / count as f64;
            if sample > 0 {
                elapsed[variant].push(ns);
            }
        }
    }
    for times in &mut elapsed {
        times.sort_by(f64::total_cmp);
    }
    println!(
        "{label}: original {:.2} ns/op, current {:.2} ns/op, {:.3}x throughput",
        elapsed[0][4],
        elapsed[1][4],
        elapsed[0][4] / elapsed[1][4]
    );
}

#[test]
#[ignore = "paired release benchmark; run alone with --nocapture"]
fn benchmark_library_scan() {
    for count in [8, 128, 1024, 4096] {
        for order in ["sorted", "reverse", "shuffled", "ties"] {
            let input = pack_fixture(count, order);
            let mut old = input.clone();
            let mut new = input.clone();
            let (_, a) = allocations::measure(|| original::sort_song_packs(&mut old));
            let (_, b) = allocations::measure(|| sort_song_packs(&mut new));
            println!("ALLOC sort/{count}/{order}: original {a:?}, current {b:?}");
            compare_sort(&format!("sort/{count}/{order}"), &input);
        }
    }
    for count in [8, 128, 1024, 4096] {
        for duplicates in [false, true] {
            let input = scan_fixture(count, duplicates);
            let (_, a) = allocations::measure(|| original::pack_group_representatives(&input));
            let (_, b) = allocations::measure(|| pack_group_representatives(&input));
            let label = format!("groups/{count}/{duplicates}");
            println!("ALLOC {label}: original {a:?}, current {b:?}");
            paired::compare(
                &label,
                16,
                || {
                    black_box(original::pack_group_representatives(black_box(&input)));
                },
                || {
                    black_box(pack_group_representatives(black_box(&input)));
                },
            );
        }
    }
    for (label, dirs, files, forks) in [
        ("empty", 0, 0, 0),
        ("directories", 64, 0, 0),
        ("mixed", 32, 256, 32),
        ("files", 0, 512, 0),
    ] {
        let root = directory_fixture(&format!("bench-{label}"), dirs, files, forks);
        let (_, a) = allocations::measure(|| original::child_dirs(&root).unwrap());
        let (_, b) = allocations::measure(|| child_dirs(&root).unwrap());
        println!("ALLOC dirs/{label}: original {a:?}, current {b:?}");
        paired::compare(
            &format!("dirs/{label}"),
            4,
            || {
                black_box(original::child_dirs(black_box(&root)).unwrap());
            },
            || {
                black_box(child_dirs(black_box(&root)).unwrap());
            },
        );
        fs::remove_dir_all(root).unwrap();
    }
}
