use super::*;
use crate::perf;
use std::hint::black_box;
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

#[allow(dead_code)]
mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/smo_preparation/baseline.rs"
    ));
}

fn generated_names() -> Vec<String> {
    let mut names: Vec<String> = [
        "",
        "Pack",
        "Pack/",
        "Pack//Song\\file.SSC",
        "/Pack/a.sm",
        "\\Pack/a.sm",
        "C:/a.sm",
        "C:foo/a.sm",
        "Pack/../a.sm",
        "Pack/./a.sm",
        "Pack/a:b.sm",
        "Pack/CON",
        "Pack/OΣ/ΟΣ.SSC",
        "Pack/İ/雪.ssc",
        "Pack/a\0b",
        "Pack\n/a.sm",
        "Pack/.../song.dwi",
        "A:B/a.sm",
        "nul",
        "CON.ext",
        "..",
        " a.  ",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    for len in [
        1, 148, 149, 150, 151, 159, 160, 161, 200, 767, 768, 769, 2048,
    ] {
        for ch in ['a', '雪', 'Σ', 'İ'] {
            let text: String = std::iter::repeat_n(ch, len).collect();
            names.extend([
                text.clone(),
                format!("{text}/?.A"),
                format!("Pack/{text}.ssc"),
                format!(" {text}. "),
            ]);
        }
    }
    let alphabet = [
        'A', 'b', '9', ' ', '.', '/', '\\', ':', '_', '<', '>', '?', '*', '|', '"', '\0', '\t',
        '\u{85}', '\u{2003}', '雪', 'Σ', 'Ο', 'İ', '\u{301}',
    ];
    let mut seed = 0x513ac729u64;
    for i in 0..2048 {
        let mut name = String::new();
        for _ in 0..i % 190 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            name.push(alphabet[(seed >> 32) as usize % alphabet.len()]);
        }
        names.push(name);
    }
    names
}

#[test]
fn archive_paths_match_legacy_validation_keys_and_destinations() {
    for name in generated_names() {
        let old = baseline::portable_archive_parts(&name);
        let new = portable_archive_parts(&name);
        match (old, new) {
            (Ok(old), Ok(new)) => {
                assert_eq!(old, new.iter().collect::<Vec<_>>(), "{name:?}");
                assert_eq!(old.len(), new.len);
                assert_eq!(
                    old[1..].join("/").to_lowercase(),
                    new.output_key(),
                    "{name:?}"
                );
                for root in [
                    Path::new("staging"),
                    Path::new("C:/stage/雪"),
                    Path::new(""),
                ] {
                    let expected = old[1..]
                        .iter()
                        .fold(root.to_path_buf(), |p, part| p.join(part));
                    assert_eq!(expected, new.output_path(root), "{name:?}");
                }
            }
            (Err(old), Err(new)) => assert_eq!(old, new, "{name:?}"),
            (old, new) => panic!("path disagreement for {name:?}: {old:?} / {new:?}"),
        }
    }
}

#[test]
fn sanitized_names_match_legacy_unicode_limits_and_ids() {
    for name in generated_names() {
        for id in [0, 1, 9, 10, 99, 100, u64::MAX] {
            assert_eq!(
                baseline::sanitized_pack_name(&name, id),
                sanitized_pack_name(&name, id),
                "{name:?}, {id}"
            );
        }
    }
}

fn runtime(installs: usize, message: Option<&str>) -> RuntimeState {
    RuntimeState {
        generation: 7,
        snapshot: Arc::new(Snapshot {
            phase: CatalogPhase::Ready,
            catalog: Arc::from([PackInfo::new(
                1,
                "Fixture".into(),
                1,
                1024,
                None,
                None,
                None,
                None,
            )]),
            revision: 12,
            message: Some("catalog ready".into()),
            installs: (0..installs)
                .map(|i| InstallSnapshot {
                    pack_id: i as u64,
                    phase: InstallPhase::Queued,
                    downloaded_bytes: i as u64,
                    total_bytes: 1024,
                    message: message.map(str::to_string),
                })
                .collect(),
        }),
        ready_song_dirs: vec![PathBuf::from("ready/Song")],
    }
}

#[test]
fn progress_updates_preserve_values_and_immutable_snapshots() {
    for count in [0, 1, 64] {
        for message in [None, Some("queued"), Some("Downloading pack archive...")] {
            for shared in [false, true] {
                let mut old = runtime(count, message);
                let mut new = runtime(count, message);
                for id in [0, 63, 100, 0] {
                    let readers =
                        shared.then(|| (Arc::clone(&old.snapshot), Arc::clone(&new.snapshot)));
                    let saved = readers.as_ref().map(|(a, _)| (**a).clone());
                    baseline::update_install_progress(&mut old, id, u64::MAX, 0);
                    update_install_progress(&mut new, id, u64::MAX, 0);
                    assert_eq!(old.snapshot, new.snapshot);
                    assert_eq!(old.generation, new.generation);
                    assert_eq!(old.ready_song_dirs, new.ready_song_dirs);
                    if let Some((a, b)) = readers {
                        assert_eq!(a, b);
                        assert_eq!(*a, saved.unwrap());
                    }
                }
                baseline::update_install(&mut old, 0, |install| {
                    install.phase = InstallPhase::Error;
                    install.message = Some("failure 雪".into());
                });
                update_install(&mut new, 0, |install| {
                    install.phase = InstallPhase::Error;
                    install.message = Some("failure 雪".into());
                });
                assert_eq!(old.snapshot, new.snapshot);
                let before = Arc::clone(&new.snapshot);
                update_install(&mut new, 12345, |_| {
                    panic!("missing install must not be updated")
                });
                assert!(Arc::ptr_eq(&before, &new.snapshot));
                let weak = Arc::downgrade(&new.snapshot);
                drop(before);
                update_install_progress(&mut new, 0, 31, 32);
                assert_eq!(weak.upgrade().is_some(), count == 0);
            }
        }
    }
}

#[test]
fn validated_paths_and_unique_progress_updates_have_no_churn() {
    for name in [
        "Pack/Song/chart.ssc",
        "Pack\\雪\\ΟΣ.SSC",
        "Pack///Song//nested/file.ogg",
        "Pack/",
    ] {
        perf::assert_no_churn(|| {
            let parts = portable_archive_parts(black_box(name)).unwrap();
            black_box(parts.iter().count());
        });
    }
    let mut state = runtime(64, Some("Downloading pack archive..."));
    let pointer = Arc::as_ptr(&state.snapshot);
    let message_pointer = state.snapshot.installs[0]
        .message
        .as_ref()
        .unwrap()
        .as_ptr();
    perf::assert_no_churn(|| {
        for bytes in 0..64 {
            update_install_progress(&mut state, 0, bytes, 1024);
        }
        update_install_progress(&mut state, 999, 0, 0);
    });
    assert_eq!(pointer, Arc::as_ptr(&state.snapshot));
    assert_eq!(
        message_pointer,
        state.snapshot.installs[0]
            .message
            .as_ref()
            .unwrap()
            .as_ptr()
    );
    let reader = Arc::clone(&state.snapshot);
    perf::assert_no_churn(|| update_install_progress(&mut state, 999, 0, 0));
    assert!(Arc::ptr_eq(&reader, &state.snapshot));
    let parts = portable_archive_parts("Pack/Song/chart.ssc").unwrap();
    perf::assert_churn_budget(1, 128, || {
        black_box(parts.output_key());
    });
    perf::assert_churn_budget(1, 128, || {
        black_box(parts.output_path(Path::new("staging")));
    });
}

static TEMP_ID: AtomicU64 = AtomicU64::new(1);
struct FixtureDir(PathBuf);
impl FixtureDir {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "deadsync-smo-perf-{}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for FixtureDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn write_zip(path: &Path, names: &[String]) {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for name in names {
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        if name.ends_with('/') {
            zip.add_directory(name, options).unwrap();
        } else {
            zip.start_file(name, options).unwrap();
            zip.write_all(b"fixture data").unwrap();
        }
    }
    fs::write(path, zip.finish().unwrap().into_inner()).unwrap();
}

#[test]
fn archive_inspection_and_extraction_match_legacy() {
    let root = FixtureDir::new();
    let mut cases: Vec<Vec<String>> = [
        vec![],
        vec!["Pack/"],
        vec!["Pack/a.ogg"],
        vec!["Pack/a.ssc"],
        vec!["Pack/a.ssc", "Other/b.ssc"],
        vec!["Pack/A.ssc", "Pack/a.SSC"],
        vec!["Pack/../a.ssc"],
        vec!["Pack\\..\\a.ssc"],
        vec!["Pack/a:b.ssc"],
        vec!["a.ssc"],
        vec!["Pack/ΟΣ.ssc", "Pack/ος.SSC"],
        vec!["Pack/", "Pack/Song/", "Pack/Song/a.ssc", "Pack/Song/a.ogg"],
        vec!["Pack//雪\\chart.SSC", "Pack/Second/other.sm"],
    ]
    .into_iter()
    .map(|v| v.into_iter().map(str::to_string).collect())
    .collect();
    cases.push(vec![format!("Pack/{}.ssc", "a".repeat(770))]);
    for (i, names) in cases.iter().enumerate() {
        let archive = root.0.join(format!("{i}.zip"));
        write_zip(&archive, names);
        let bytes = fs::metadata(&archive).unwrap().len();
        let old = baseline::inspect_archive(&archive, bytes);
        let new = inspect_archive(&archive, bytes);
        assert_eq!(format!("{old:?}"), format!("{new:?}"), "{names:?}");
        let old_path = root.0.join(format!("old-{i}"));
        let new_path = root.0.join(format!("new-{i}"));
        let old = baseline::extract_archive(&archive, &old_path, bytes);
        let new = extract_archive(&archive, &new_path, bytes);
        assert_eq!(old, new, "{names:?}");
        if old.is_ok() {
            assert_eq!(tree(&old_path), tree(&new_path));
        } else {
            assert!(!old_path.exists() && !new_path.exists());
        }
    }
}

fn tree(root: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    fn visit(base: &Path, dir: &Path, out: &mut Vec<(PathBuf, Option<Vec<u8>>)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                out.push((path.strip_prefix(base).unwrap().to_path_buf(), None));
                visit(base, &path, out);
            } else {
                out.push((
                    path.strip_prefix(base).unwrap().to_path_buf(),
                    Some(fs::read(&path).unwrap()),
                ));
            }
        }
    }
    let mut out = Vec::new();
    visit(root, root, &mut out);
    out.sort();
    out
}

fn pair<A, B>(
    name: &str,
    iterations: usize,
    units: usize,
    mut old: impl FnMut() -> A,
    mut new: impl FnMut() -> B,
) {
    let mut before = || perf::measure_sampled(&format!("{name}_old"), iterations, units, &mut old);
    let mut after = || perf::measure_sampled(&format!("{name}_new"), iterations, units, &mut new);
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        after();
        before();
    } else {
        before();
        after();
    }
}

#[test]
#[ignore = "manual release benchmark; seven batches and separate allocation counting"]
fn smo_preparation_bench() {
    for (name, raw) in [
        ("short", "Clean Pack".to_string()),
        ("reserved", "CON.txt".into()),
        ("invalid", "  Bad/Pack: Name. ".into()),
        ("ascii_limit", "a".repeat(160)),
        ("ascii_long", "a".repeat(2048)),
        ("unicode", "雪Σİ".repeat(80)),
        ("mixed", "  雪Σ?/Title. ".repeat(128)),
    ] {
        pair(
            &format!("sanitize_{name}"),
            1000,
            raw.len(),
            || {
                black_box(baseline::sanitized_pack_name as fn(&str, u64) -> _)(
                    black_box(&raw),
                    123456,
                )
            },
            || black_box(sanitized_pack_name as fn(&str, u64) -> _)(black_box(&raw), 123456),
        );
    }
    for (name, path) in [
        ("shallow", "Pack/Song/chart.ssc".to_string()),
        ("deep", format!("Pack/{}chart.ssc", "nested/".repeat(32))),
        ("unicode", "Pack/雪/ΟΣ.SSC".into()),
        ("mixed", "Pack//Song\\nested///chart.SSC".into()),
        ("invalid", "Pack/Song/nested/../chart.ssc".into()),
    ] {
        pair(
            &format!("validate_{name}"),
            1000,
            path.len(),
            || black_box(baseline::portable_archive_parts as fn(&str) -> _)(black_box(&path)),
            || {
                black_box(
                    portable_archive_parts
                        as fn(&str) -> Result<ArchiveParts<'_>, StepManiaOnlineError>,
                )(black_box(&path))
            },
        );
        if name != "invalid" {
            pair(
                &format!("key_{name}"),
                1000,
                path.len(),
                || {
                    let p = baseline::portable_archive_parts(black_box(&path)).unwrap();
                    p[1..].join("/").to_lowercase()
                },
                || {
                    portable_archive_parts(black_box(&path))
                        .unwrap()
                        .output_key()
                },
            );
            pair(
                &format!("destination_{name}"),
                1000,
                path.len(),
                || {
                    let p = baseline::portable_archive_parts(black_box(&path)).unwrap();
                    p[1..]
                        .iter()
                        .fold(PathBuf::from("C:/staging"), |p, c| p.join(c))
                },
                || {
                    portable_archive_parts(black_box(&path))
                        .unwrap()
                        .output_path(Path::new("C:/staging"))
                },
            );
        }
    }
    for count in [0, 1, 64] {
        for mode in ["unique", "shared", "missing", "shared_missing"] {
            let mut old = runtime(count, Some("Downloading pack archive..."));
            let mut new = runtime(count, Some("Downloading pack archive..."));
            let id = if mode.ends_with("missing") { 999 } else { 0 };
            let shared = mode.starts_with("shared");
            let mut old_tick = 0;
            let mut new_tick = 0;
            pair(
                &format!("progress_{count}_{mode}"),
                1000,
                1,
                || {
                    let reader = shared.then(|| Arc::clone(&old.snapshot));
                    old_tick += 1;
                    baseline::update_install_progress(black_box(&mut old), id, old_tick, 4096);
                    black_box(&old.snapshot);
                    drop(reader);
                },
                || {
                    let reader = shared.then(|| Arc::clone(&new.snapshot));
                    new_tick += 1;
                    update_install_progress(black_box(&mut new), id, new_tick, 4096);
                    black_box(&new.snapshot);
                    drop(reader);
                },
            );
        }
    }
    let root = FixtureDir::new();
    for (name, count, unicode) in [
        ("one", 1, false),
        ("many", 256, false),
        ("unicode", 256, true),
    ] {
        let names: Vec<_> = (0..count)
            .map(|i| {
                if unicode {
                    format!("Pack/雪-{i}/ΟΣ.ssc")
                } else {
                    format!("Pack/Song-{i}/chart.ssc")
                }
            })
            .collect();
        let archive = root.0.join(format!("{name}.zip"));
        write_zip(&archive, &names);
        let bytes = fs::metadata(&archive).unwrap().len();
        pair(
            &format!("inspect_{name}"),
            16,
            count,
            || baseline::inspect_archive(black_box(&archive), bytes),
            || inspect_archive(black_box(&archive), bytes),
        );
    }
}
