use super::*;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "itl_index_encoding_baseline.rs"]
mod baseline;

fn original_encode(by_key: &OnlineItlSelfCacheMap) -> Vec<u8> {
    // The parent's complete encoding block, before its unchanged error mapping.
    let std_by_key: HashMap<_, _> = by_key.iter().collect();
    bincode::encode_to_vec(&std_by_key, bincode::config::standard()).unwrap()
}

fn current_encode(by_key: &OnlineItlSelfCacheMap) -> Vec<u8> {
    bincode::encode_to_vec(OnlineItlSelfIndexRef(by_key), bincode::config::standard()).unwrap()
}

fn fixture(count: usize, long_keys: bool) -> OnlineItlSelfCacheMap {
    (0..count)
        .map(|i| {
            (
                OnlineItlSelfScoreKey {
                    chart_hash: format!("chart-{i:016x}"),
                    api_key: format!(
                        "fixture-api-{}-{}",
                        i % 3,
                        if long_keys {
                            "日本-".repeat(80)
                        } else {
                            String::new()
                        }
                    ),
                },
                [0, 250, 251, u32::MAX][i % 4],
            )
        })
        .collect()
}

#[test]
fn itl_index_encoding_preserves_map_layout_values_and_input() {
    for (count, long) in [
        (0, false),
        (1, false),
        (250, false),
        (251, false),
        (1024, false),
        (128, true),
    ] {
        let mut map = fixture(count, long);
        map.reserve(100);
        map.insert(
            OnlineItlSelfScoreKey {
                chart_hash: String::new(),
                api_key: "a\0b\n😀".to_string(),
            },
            123,
        );
        let before = map.clone();
        let original = original_encode(&map);
        let current = current_encode(&map);
        assert_eq!(current.len(), original.len());
        let expected: OnlineItlSelfIndexMap = map
            .iter()
            .map(|(key, value)| (key.clone(), *value))
            .collect();
        for bytes in [&original, &current] {
            let (decoded, consumed) = bincode::decode_from_slice::<OnlineItlSelfIndexMap, _>(
                bytes,
                bincode::config::standard(),
            )
            .unwrap();
            assert_eq!(consumed, bytes.len());
            assert_eq!(decoded, expected);
        }
        // Vec<(key, value)> has the same count/pair layout, and retains cache order.
        let pairs: Vec<_> = map.iter().collect();
        assert_eq!(
            current,
            bincode::encode_to_vec(pairs, bincode::config::standard()).unwrap()
        );
        assert_eq!(map, before);
        crate::perf::assert_reduced_churn(
            || {
                black_box(original_encode(&map));
            },
            || {
                black_box(current_encode(&map));
            },
        );
    }
    let empty = OnlineItlSelfCacheMap::new();
    assert_eq!(current_encode(&empty), original_encode(&empty));
}

struct Directory {
    path: PathBuf,
    root: PathBuf,
}

impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let root = workspace.join("target/perf-1703-files");
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        assert!(root.starts_with(&workspace) && root != workspace);
        let path = root.join(format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self { path, root }
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let path = self.path.canonicalize().unwrap();
        assert!(path.starts_with(&self.root) && path != self.root);
        fs::remove_dir_all(path).unwrap();
    }
}

fn error_kind(
    result: Result<(), OnlineItlSelfIndexWriteError>,
    requested: &Path,
) -> (u8, Option<std::io::ErrorKind>) {
    match result {
        Ok(()) => (0, None),
        Err(OnlineItlSelfIndexWriteError::CreateDir { dir, error }) => {
            assert_eq!(dir, requested.parent().unwrap());
            (1, Some(error.kind()))
        }
        Err(OnlineItlSelfIndexWriteError::Encode { path }) => {
            assert_eq!(path, requested);
            (2, None)
        }
        Err(OnlineItlSelfIndexWriteError::WriteTemp { path, error }) => {
            assert_eq!(path, requested.with_extension("tmp"));
            (3, Some(error.kind()))
        }
        Err(OnlineItlSelfIndexWriteError::Commit { path, error }) => {
            assert_eq!(path, requested);
            (4, Some(error.kind()))
        }
    }
}

#[test]
fn itl_index_writes_preserve_overwrites_errors_and_temporary_cleanup() {
    let directory = Directory::new();
    let original = directory.path.join("original/nested/index.bin");
    let current = directory.path.join("current/nested/index.bin");
    for count in [128, 1, 0] {
        let map = fixture(count, false);
        baseline::save_online_itl_self_index_file(&original, &map).unwrap();
        save_online_itl_self_index_file(&current, &map).unwrap();
        assert_eq!(
            load_online_itl_self_index_file(&original),
            load_online_itl_self_index_file(&current)
        );
        let expected: OnlineItlSelfIndexMap = map.into_iter().collect();
        assert_eq!(load_online_itl_self_index_file(&current), Some(expected));
        assert!(!original.with_extension("tmp").exists());
        assert!(!current.with_extension("tmp").exists());
    }
    let map = fixture(8, false);
    let writers = [
        baseline::save_online_itl_self_index_file,
        save_online_itl_self_index_file,
    ];
    for scenario in [1, 3, 4] {
        let mut results = Vec::new();
        for (i, write) in writers.iter().enumerate() {
            let folder = directory.path.join(format!("{scenario}-{i}"));
            fs::create_dir(&folder).unwrap();
            let path = if scenario == 1 {
                let blocked = folder.join("blocked");
                fs::write(&blocked, []).unwrap();
                blocked.join("index.bin")
            } else {
                let path = folder.join("index.bin");
                fs::create_dir(if scenario == 3 {
                    path.with_extension("tmp")
                } else {
                    path.clone()
                })
                .unwrap();
                path
            };
            results.push(error_kind(write(&path, &map), &path));
            if scenario == 4 {
                assert!(path.is_dir());
                assert!(!path.with_extension("tmp").exists());
            }
            if scenario == 3 {
                assert!(path.with_extension("tmp").is_dir());
                assert!(!path.exists());
            }
        }
        assert_eq!(results[0], results[1]);
        assert_eq!(results[0].0, scenario);
    }
    let no_parent = Path::new("");
    assert!(no_parent.parent().is_none());
    baseline::save_online_itl_self_index_file(no_parent, &map).unwrap();
    save_online_itl_self_index_file(no_parent, &map).unwrap();
}

#[test]
#[ignore = "manual original/current ITL encoding and complete file-write benchmarks; run in release"]
fn itl_index_encoding_benchmark() {
    for (count, long) in [
        (0, false),
        (1, false),
        (128, false),
        (1024, false),
        (128, true),
    ] {
        let map = fixture(count, long);
        let order = if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            [true, false]
        } else {
            [false, true]
        };
        for new in order {
            let variant = if new { "new" } else { "old" };
            let work = if new { current_encode } else { original_encode };
            crate::perf::measure_sampled(
                &format!("itl-index/entries={count}/long={long}/{variant}"),
                128,
                1,
                || work(black_box(&map)),
            );
        }
    }
    let directory = Directory::new();
    for count in [128, 1024] {
        let map = fixture(count, false);
        let path = directory.path.join("index.bin");
        let order = if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            [true, false]
        } else {
            [false, true]
        };
        for new in order {
            let variant = if new { "new" } else { "old" };
            let work = if new {
                save_online_itl_self_index_file
            } else {
                baseline::save_online_itl_self_index_file
            };
            crate::perf::measure_sampled(
                &format!("itl-write/entries={count}/{variant}"),
                16,
                1,
                || work(black_box(&path), black_box(&map)).unwrap(),
            );
        }
    }
}
