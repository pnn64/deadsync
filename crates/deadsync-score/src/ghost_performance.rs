//! Differential checks and real ghost-history CPU/allocation measurements.
#![allow(
    clippy::duplicate_mod,
    reason = "other private score tests use the same timing helper"
)]
use super::*;
use crate::perf;
use std::hint::black_box;
use std::io::Write;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "ghost_original.rs"]
mod original;
#[path = "ghost_repeat_path.rs"]
mod repeated;

struct TestDir {
    path: PathBuf,
    base: PathBuf,
}
impl TestDir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = fs::canonicalize(std::env::temp_dir()).unwrap();
        let path = base.join(format!(
            "deadsync-ghost-perf-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self { path, base }
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let resolved = fs::canonicalize(&self.path).unwrap();
        assert_eq!(resolved.parent(), Some(self.base.as_path()));
        assert!(
            resolved
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("deadsync-ghost-perf-")
        );
        fs::remove_dir_all(resolved).unwrap();
    }
}

fn scalar_finite(points: &[f64]) -> bool {
    points.iter().all(|point| point.is_finite())
}

fn next(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

fn histories(n: usize) -> [Vec<f64>; 3] {
    std::array::from_fn(|i| (0..n).map(|j| (j as f64 + i as f64) / 16.0).collect())
}

fn compare_histories(old: &[Vec<f64>; 3], new: &[Vec<f64>; 3]) {
    for (a, b) in old.iter().zip(new) {
        assert_eq!(a.capacity(), b.capacity());
        assert_eq!(
            a.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            b.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
        );
    }
}

#[test]
fn path_bytes_and_validation_errors_match() {
    let profiles = [
        "",
        ".",
        "relative/profile",
        "C:",
        "C:profile",
        r"C:\profiles\one\",
        r"\\host\share\profile",
        r"\\?\C:\one\.\two\..\profile",
        "日本/é/profile",
    ];
    let hashes = [
        "",
        "0",
        "abcDEF0123456789",
        "xyz",
        "../AB",
        "a.b",
        "ß",
        "123\0",
        "💙",
    ];
    for profile in profiles {
        for hash in hashes.iter().copied().chain(
            ["A".repeat(64), "F".repeat(8192)]
                .iter()
                .map(String::as_str),
        ) {
            match (
                original::chart_path(Path::new(profile), hash),
                chart_path(Path::new(profile), hash),
            ) {
                (Ok(a), Ok(b)) => assert_eq!(a.as_os_str(), b.as_os_str()),
                (Err(a), Err(b)) => {
                    assert_eq!(a.kind(), b.kind());
                    assert_eq!(a.to_string(), b.to_string());
                }
                _ => panic!("path result differs"),
            }
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        let profile = PathBuf::from(std::ffi::OsString::from_wide(&[
            67, 58, 92, 0xd800, 92, 112,
        ]));
        for hash in ["0", "abcdef0123456789"] {
            assert_eq!(
                original::chart_path(&profile, hash).unwrap().as_os_str(),
                chart_path(&profile, hash).unwrap().as_os_str()
            );
        }
    }
}

#[test]
fn finite_blocks_and_update_states_match() {
    let mut seed = 0x1234_5678_9abc_def0;
    for length in 0..80 {
        for bad in 0..=length {
            for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.0, f64::MAX] {
                let mut points: Vec<_> = (0..length)
                    .map(|_| f64::from_bits(next(&mut seed) & 0xffef_ffff_ffff_ffff))
                    .collect();
                if bad < length {
                    points[bad] = value;
                }
                assert_eq!(scalar_finite(&points), all_points_finite(&points));
            }
        }
    }
    for exponent in 0..2048u64 {
        for sign in [0, 1u64 << 63] {
            for mantissa in [0, 1, (1u64 << 51) - 1, 1u64 << 51, (1u64 << 52) - 1] {
                let value = f64::from_bits(sign | (exponent << 52) | mantissa);
                for lane in 0..8 {
                    let mut values = [1.0; 8];
                    values[lane] = value;
                    assert_eq!(scalar_finite(&values), all_points_finite(&values));
                }
            }
        }
    }
    for _ in 0..2048 {
        let mut a = original::GhostData::default();
        let mut b = GhostData::default();
        for _ in 0..5 {
            let values = std::array::from_fn(|_| {
                (0..(next(&mut seed) % 40))
                    .map(|_| f64::from_bits(next(&mut seed)))
                    .collect()
            });
            assert_eq!(a.update_best(&values), b.update_best(&values));
            for (old, new) in [(&a.itg, &b.itg), (&a.ex, &b.ex), (&a.hard_ex, &b.hard_ex)] {
                assert_eq!(old.capacity(), new.capacity());
                assert_eq!(
                    old.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    new.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
                );
            }
        }
    }
}

#[test]
fn real_json_load_save_and_unchanged_records_match() {
    let dir = TestDir::new();
    let before = dir.path.join("before");
    let after = dir.path.join("after");
    for root in [&before, &after] {
        fs::create_dir_all(root.join("GhostData")).unwrap();
    }
    for n in [0, 1, 7, 8, 9, 128] {
        let data = histories(n);
        let a = original::save_best(&before, "ABC0", &data).unwrap();
        let b = save_best(&after, "ABC0", &data).unwrap();
        assert_eq!(a, b);
        if before.join("GhostData/ABC0.json").exists() {
            assert_eq!(
                fs::read(before.join("GhostData/ABC0.json")).unwrap(),
                fs::read(after.join("GhostData/ABC0.json")).unwrap()
            );
            let a = original::load(&before, "ABC0").unwrap().into_histories();
            let b = load(&after, "ABC0").unwrap().into_histories();
            compare_histories(&a, &b);
        }
    }
    for bytes in [
        b"{".as_slice(),
        br#"{"itg":[1e9999],"ex":[]}"#,
        br#"{"itg":[-0.0,1.0],"ex":[2],"unknown":true}"#,
    ] {
        fs::write(before.join("GhostData/DEAD.json"), bytes).unwrap();
        match (original::load(&before, "DEAD"), load(&before, "DEAD")) {
            (Ok(a), Ok(b)) => compare_histories(&a.into_histories(), &b.into_histories()),
            (Err(a), Err(b)) => {
                assert_eq!(a.kind(), b.kind());
                assert_eq!(a.to_string(), b.to_string());
            }
            _ => panic!("load result differs"),
        }
    }
    let big = before.join("GhostData/FFFF.json");
    for length in [MAX_FILE_BYTES, MAX_FILE_BYTES + 1, MAX_FILE_BYTES + 2] {
        fs::File::create(&big).unwrap().set_len(length).unwrap();
        let old = original::load(&before, "FFFF").unwrap_err();
        let new = load(&before, "FFFF").unwrap_err();
        assert_eq!(old.kind(), new.kind());
        assert_eq!(old.to_string(), new.to_string());
        if length > MAX_FILE_BYTES {
            assert_eq!(new.to_string(), "ghost file too large");
        }
    }
    for root in [&before, &after] {
        fs::create_dir(root.join("GhostData/BEEF.json.tmp")).unwrap();
    }
    let values = histories(4);
    let old = original::save_best(&before, "BEEF", &values).unwrap_err();
    let new = save_best(&after, "BEEF", &values).unwrap_err();
    assert_eq!(old.kind(), new.kind());
    assert_eq!(old.raw_os_error(), new.raw_os_error());
    assert_eq!(old.to_string(), new.to_string());
    assert!(
        before.join("GhostData/BEEF.json.tmp").is_dir()
            && after.join("GhostData/BEEF.json.tmp").is_dir()
    );
    let source = dir.path.join("source");
    fs::create_dir_all(source.join("GhostData")).unwrap();
    fs::write(
        source.join("GhostData/CAFE.json"),
        br#"{"itg":[0,-0.0,2],"ex":[1,2],"hard_ex":[3]}"#,
    )
    .unwrap();
    fs::write(source.join("GhostData/not-a-hash.json"), b"{}").unwrap();
    fs::write(source.join("GhostData/deadbeef.json"), b"{").unwrap();
    fs::write(source.join("GhostData/readme.txt"), b"ignored").unwrap();
    assert_eq!(
        original::import_directory(&source, &before).unwrap(),
        import_directory(&source, &after).unwrap()
    );
    assert_eq!(
        fs::read(before.join("GhostData/CAFE.json")).unwrap(),
        fs::read(after.join("GhostData/CAFE.json")).unwrap()
    );
}

fn compare(label: &str, iterations: usize, mut work: impl FnMut(bool)) {
    for (current, name) in [(false, "original"), (true, "current")] {
        let (_, churn) = perf::measure(|| work(current));
        println!("{label}/{name}: allocation {churn:?}");
    }
    bench::compare(label, iterations, work);
}

#[test]
#[ignore = "real ghost-history CPU and allocation benchmark"]
fn benchmark_ghost_paths() {
    type PathFn = fn(&Path, &str) -> io::Result<PathBuf>;
    let functions: [PathFn; 2] = [original::chart_path, chart_path];
    for (name, profile) in [
        ("short", "p"),
        ("normal", r"C:\Users\Player\profiles\日本"),
        ("verbatim", r"\\?\C:\Users\Player\profiles\日本"),
        ("long", &"directory/".repeat(64)),
    ] {
        let hash = "A0b1C2d3".repeat(8);
        let profile = Path::new(profile);
        compare(&format!("path/{name}"), 100000, |current| {
            black_box(functions[usize::from(current)](
                black_box(profile),
                black_box(&hash),
            ))
            .unwrap();
        });
    }
    for n in [8, 64, 1024, 16384] {
        for bad in [None, Some(0), Some(n / 2), Some(n - 1)] {
            let mut points = vec![1.0; n];
            if let Some(index) = bad {
                points[index] = f64::INFINITY;
            }
            let functions = [scalar_finite as fn(&[f64]) -> bool, all_points_finite];
            compare(
                &format!(
                    "finite/{n}/{}",
                    bad.map_or_else(|| "valid".to_string(), |i| i.to_string())
                ),
                (2_000_000 / bad.map_or(n, |index| index + 1)).clamp(100, 1_000_000),
                |current| {
                    black_box(black_box(functions[usize::from(current)])(black_box(
                        &points,
                    )));
                },
            );
        }
    }
    for n in [8, 64, 1024, 16384] {
        let data = histories(n);
        let mut old = original::GhostData {
            itg: vec![0.0; n],
            ex: vec![0.0; n],
            hard_ex: vec![0.0; n],
        };
        let mut new = GhostData {
            itg: vec![0.0; n],
            ex: vec![0.0; n],
            hard_ex: vec![0.0; n],
        };
        compare(
            &format!("update/winning/{n}"),
            (500_000 / n).clamp(50, 10000),
            |current| {
                if current {
                    for values in [&mut new.itg, &mut new.ex, &mut new.hard_ex] {
                        *values.last_mut().unwrap() = -1.0;
                    }
                    black_box(new.update_best(black_box(&data)));
                } else {
                    for values in [&mut old.itg, &mut old.ex, &mut old.hard_ex] {
                        *values.last_mut().unwrap() = -1.0;
                    }
                    black_box(old.update_best(black_box(&data)));
                }
            },
        );
    }
    let dir = TestDir::new();
    let hash = "ABCDEF0123456789";
    for n in [0, 64, 4096] {
        let data = histories(n);
        save_best(&dir.path, hash, &data).unwrap();
        let functions = [
            original::save_best as fn(&Path, &str, &[Vec<f64>; 3]) -> io::Result<bool>,
            save_best,
        ];
        compare(
            &format!("save/unchanged/{n}"),
            if n < 100 { 200 } else { 20 },
            |current| {
                black_box(functions[usize::from(current)](
                    black_box(&dir.path),
                    black_box(hash),
                    black_box(&data),
                ))
                .unwrap();
            },
        );
        let functions = [
            repeated::save_best as fn(&Path, &str, &[Vec<f64>; 3]) -> io::Result<bool>,
            save_best,
        ];
        compare(
            &format!("save/path-reuse/{n}"),
            if n < 100 { 200 } else { 20 },
            |current| {
                black_box(functions[usize::from(current)](
                    black_box(&dir.path),
                    black_box(hash),
                    black_box(&data),
                ))
                .unwrap();
            },
        );
        if n != 0 {
            compare(
                &format!("load/{n}"),
                if n < 100 { 200 } else { 20 },
                |current| {
                    if current {
                        black_box(load(black_box(&dir.path), black_box(hash))).unwrap();
                    } else {
                        black_box(original::load(black_box(&dir.path), black_box(hash))).unwrap();
                    }
                },
            );
        }
    }
}

#[test]
#[ignore = "writes deterministic path, finite-validation and update trace"]
fn dump_ghost_trace() {
    path_bytes_and_validation_errors_match();
    finite_blocks_and_update_states_match();
    real_json_load_save_and_unchanged_records_match();
    let mut trace = std::io::BufWriter::with_capacity(
        1 << 20,
        fs::File::create(std::env::var_os("DEADSYNC_GHOST_TRACE_OUT").unwrap()).unwrap(),
    );
    for profile in [
        "",
        ".",
        r"C:\profile",
        r"\\?\C:\one\.\two\..\profile",
        "日本/é",
    ] {
        for hash in ["", "0", "ABCdef0123456789", "../AB", "ß"] {
            match chart_path(Path::new(profile), hash) {
                Ok(path) => {
                    let bytes = path.as_os_str().as_encoded_bytes();
                    trace.write_all(&[1]).unwrap();
                    trace
                        .write_all(&(bytes.len() as u64).to_le_bytes())
                        .unwrap();
                    trace.write_all(bytes).unwrap();
                }
                Err(error) => {
                    trace.write_all(&[0]).unwrap();
                    trace.write_all(error.to_string().as_bytes()).unwrap();
                    trace.write_all(&[0]).unwrap();
                }
            }
        }
    }
    for exponent in 0..2048u64 {
        for sign in [0, 1u64 << 63] {
            for mantissa in [0, 1, (1u64 << 51) - 1, 1u64 << 51, (1u64 << 52) - 1] {
                let value = f64::from_bits(sign | (exponent << 52) | mantissa);
                for lane in 0..8 {
                    let mut values = [1.0; 8];
                    values[lane] = value;
                    let result = all_points_finite(&values);
                    assert_eq!(result, scalar_finite(&values));
                    trace.write_all(&[u8::from(result)]).unwrap();
                }
            }
        }
    }
    let mut old = original::GhostData::default();
    let mut new = GhostData::default();
    let mut seed = 0x9abc_def0_1234_5678;
    for _ in 0..4096 {
        let values = std::array::from_fn(|_| {
            (0..next(&mut seed) % 40)
                .map(|_| f64::from_bits(next(&mut seed)))
                .collect()
        });
        let changed = new.update_best(&values);
        assert_eq!(changed, old.update_best(&values));
        trace.write_all(&[u8::from(changed)]).unwrap();
        for (a, b) in [
            (&old.itg, &new.itg),
            (&old.ex, &new.ex),
            (&old.hard_ex, &new.hard_ex),
        ] {
            assert_eq!(a.capacity(), b.capacity());
            assert_eq!(a.len(), b.len());
            trace
                .write_all(&(b.capacity() as u64).to_le_bytes())
                .unwrap();
            trace.write_all(&(b.len() as u64).to_le_bytes()).unwrap();
            for (&x, &y) in a.iter().zip(b) {
                assert_eq!(x.to_bits(), y.to_bits());
                trace.write_all(&y.to_bits().to_le_bytes()).unwrap();
            }
        }
    }
    trace.flush().unwrap();
}
