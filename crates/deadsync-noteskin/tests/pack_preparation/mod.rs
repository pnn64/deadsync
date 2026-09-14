use super::*;
use crate::perf;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};

mod baseline;
use baseline::LegacyPack;

struct Fixture(PathBuf);
static NEXT: AtomicU64 = AtomicU64::new(0);

impl Fixture {
    fn new(choices: usize, files: usize) -> (Self, InstalledPack) {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        fs::create_dir_all(&parent).unwrap();
        let root = parent.canonicalize().unwrap().join(format!(
            "pack-preparation-{:010}-{:010}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("base")).unwrap();
        fs::create_dir(root.join("choices")).unwrap();
        for name in ["metrics.ini", "NoteSkin.lua"] {
            fs::write(root.join("base").join(name), b"fixture").unwrap();
        }
        for index in 0..files {
            for dir in ["base", "choices"] {
                fs::write(root.join(dir).join(format!("{index}.png")), b"fixture").unwrap();
            }
        }
        image::RgbaImage::new(2048, 2048)
            .save(root.join("preview.png"))
            .unwrap();
        let mut pack = pack_fixture(choices);
        pack.root = root.clone();
        for choice in &mut pack.manifest.skins[0].options {
            choice.files = (0..files)
                .map(|index| FileSwap {
                    target: format!("{index}.png"),
                    source: format!("choices/{index}.png"),
                })
                .collect();
        }
        let fixture = Self(root);
        fixture.write(&pack.manifest);
        (fixture, pack)
    }

    fn write(&self, manifest: &Manifest) {
        fs::write(
            self.0.join("pack.json"),
            serde_json::to_vec(manifest).unwrap(),
        )
        .unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .canonicalize()
            .unwrap();
        assert_eq!(self.0.parent(), Some(parent.as_path()));
        assert!(
            self.0
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("pack-preparation-")
        );
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn pack_fixture(count: usize) -> InstalledPack {
    InstalledPack {
        root: PathBuf::new(),
        fingerprint: "0123456789abcdef".into(),
        manifest: Manifest {
            schema: 1,
            id: "sample".into(),
            version: "1".into(),
            source: "fixture".into(),
            skins: vec![Skin {
                id: "sample-cel".into(),
                base: "base".into(),
                preview: "preview.png".into(),
                options: (0..count)
                    .map(|index| Choice {
                        slot: SLOTS[index % SLOTS.len()].into(),
                        id: format!("choice-{index}"),
                        label: format!("Choice {index}"),
                        cell: (index % 1024) as u16,
                        files: vec![FileSwap {
                            target: "note.PNG".into(),
                            source: if index % 3 == 0 {
                                "script.lua"
                            } else {
                                "note.png"
                            }
                            .into(),
                        }],
                        metrics: if index % 3 == 1 {
                            vec![MetricSwap {
                                section: "NoteDisplay".into(),
                                key: "TapNoteAnimationLength".into(),
                                value: "4".into(),
                            }]
                        } else {
                            vec![]
                        },
                    })
                    .collect(),
            }],
        },
    }
}

fn selection(count: usize) -> Selection {
    Selection {
        skin: "sample-cel".into(),
        options: SLOTS
            .iter()
            .take(count)
            .enumerate()
            .map(|(index, slot)| ((*slot).into(), format!("choice-{index}")))
            .collect(),
    }
}

#[test]
fn streamed_keys_preserve_bytes_and_only_allocate_the_result() {
    let pack = pack_fixture(64);
    let old = LegacyPack(pack.clone());
    for count in 0..=SLOTS.len() {
        for skin in ["sample-cel", "unknown", "sample-cel?ignored", "", "Δ\0skin"] {
            for length in [0, 1, 15, 16, 31, 32, 33, 63, 64, 128] {
                let mut selected = selection(count);
                selected.skin = skin.into();
                if let Some(value) = selected.options.values_mut().next() {
                    *value = "x".repeat(length);
                }
                assert_eq!(pack.runtime_key(&selected), old.runtime_key(&selected));
                assert_eq!(pack.compiler_key(&selected), old.compiler_key(&selected));
                perf::assert_churn_budget(1, selected.skin.len() + 17, || {
                    black_box(pack.compiler_key(black_box(&selected)));
                });
            }
        }
    }
    // Exercise actual filtering, including an all-PNG and an empty-file choice.
    for mask in 0..(1 << SLOTS.len()) {
        let mut selected = selection(SLOTS.len());
        selected
            .options
            .retain(|slot, _| mask & (1 << SLOTS.iter().position(|s| s == slot).unwrap()) != 0);
        assert_eq!(pack.compiler_key(&selected), old.compiler_key(&selected));
    }
    let mut empty = pack.clone();
    empty.manifest.skins[0].options[0].files.clear();
    assert_eq!(
        empty.compiler_key(&selection(1)),
        LegacyPack(empty.clone()).compiler_key(&selection(1))
    );
}

fn signature(result: Result<InstalledPack, Error>) -> Result<(PathBuf, String, Vec<u8>), String> {
    result
        .map(|pack| {
            (
                pack.root,
                pack.fingerprint,
                serde_json::to_vec(&pack.manifest).unwrap(),
            )
        })
        .map_err(|error| format!("{error:?}: {error}"))
}

fn assert_load_matches(root: &Path) {
    assert_eq!(
        signature(InstalledPack::load(root)),
        signature(LegacyPack::load(root))
    );
}

#[test]
fn cached_validation_preserves_manifests_fingerprints_and_failures() {
    let (fixture, pack) = Fixture::new(12, 3);
    assert_load_matches(&fixture.0);
    perf::assert_reduced_churn(
        || {
            black_box(LegacyPack::load(&fixture.0).unwrap());
        },
        || {
            black_box(InstalledPack::load(&fixture.0).unwrap());
        },
    );
    for variant in 0..18 {
        let mut manifest = pack.manifest.clone();
        let skin = &mut manifest.skins[0];
        match variant {
            0 => skin.options[0].id = "INVALID".into(),
            1 => skin.options.push(skin.options[0].clone()),
            2 => skin.options[0].files[0].target = "../pack.json".into(),
            3 => skin.options[0].files[0].source = "/escape".into(),
            4 => skin.options[0].files[0].source = "missing.png".into(),
            5 => skin.options[0].files[0].source = "choices".into(),
            6 => skin.options[0].files[1] = skin.options[0].files[0].clone(),
            7 => skin.options[0].files[1].target = "./0.png".into(),
            8 => skin.options[0].metrics.push(MetricSwap {
                section: "bad".into(),
                key: "x".into(),
                value: "x".into(),
            }),
            9 => skin.options[0].cell = 1024,
            10 => skin.options[0].label.clear(),
            11 => skin.options[0].files[0].source = "choices\\0.png".into(),
            12 => skin.options[0].files[0].source = "C:/file.png".into(),
            13 => skin.base = "choices".into(),
            14 => skin.preview = "base/0.png".into(),
            15 => {
                let duplicate = skin.clone();
                manifest.skins.push(duplicate);
            }
            16 => manifest.schema = 2,
            _ => skin.options[0].files.clear(),
        }
        fixture.write(&manifest);
        assert_load_matches(&fixture.0);
    }
    fixture.write(&pack.manifest);
    fs::remove_file(fixture.0.join("base/NoteSkin.lua")).unwrap();
    assert_load_matches(&fixture.0);
}

#[test]
fn cached_roots_reject_symlink_escapes_and_accept_internal_aliases() {
    let (fixture, pack) = Fixture::new(1, 2);
    let (outside, _) = Fixture::new(0, 0);
    #[cfg(windows)]
    fn link(target: &PathBuf, path: PathBuf) -> std::io::Result<()> {
        use std::os::windows::process::CommandExt;
        match std::os::windows::fs::symlink_dir(target, &path) {
            Ok(()) => return Ok(()),
            Err(error) if error.raw_os_error() == Some(1314) => {}
            Err(error) => return Err(error),
        }
        // Directory junctions exercise canonical containment without requiring
        // Developer Mode or administrator privileges on Windows.
        let output = std::process::Command::new("powershell.exe")
            .creation_flags(0x0800_0000)
            .args(["-NoProfile", "-NonInteractive", "-Command",
                "New-Item -ItemType Junction -Path $env:DEADSYNC_TEST_LINK -Target $env:DEADSYNC_TEST_TARGET -ErrorAction Stop | Out-Null"])
            .env("DEADSYNC_TEST_LINK", path.to_string_lossy().trim_start_matches(r"\\?\"))
            .env("DEADSYNC_TEST_TARGET", target.to_string_lossy().trim_start_matches(r"\\?\"))
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }
    #[cfg(unix)]
    let link = std::os::unix::fs::symlink;
    link(&outside.0, fixture.0.join("escape")).expect("symlink fixture must be available");
    link(&fixture.0.join("choices"), fixture.0.join("alias")).unwrap();
    for source in ["escape/pack.json", "alias/0.png"] {
        let mut manifest = pack.manifest.clone();
        manifest.skins[0].options[0].files[0].source = source.into();
        fixture.write(&manifest);
        assert_load_matches(&fixture.0);
        assert_eq!(
            InstalledPack::load(&fixture.0).is_ok(),
            source.starts_with("alias")
        );
    }
    // Remove directory links explicitly before recursively removing fixtures.
    #[cfg(windows)]
    for name in ["escape", "alias"] {
        fs::remove_dir(fixture.0.join(name)).unwrap();
    }
    #[cfg(unix)]
    for name in ["escape", "alias"] {
        fs::remove_file(fixture.0.join(name)).unwrap();
    }
}

fn compare<T>(
    name: &str,
    iterations: usize,
    units: usize,
    old: impl FnMut() -> T,
    new: impl FnMut() -> T,
) {
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        perf::measure_sampled(&format!("{name}/new"), iterations, units, new);
        perf::measure_sampled(&format!("{name}/old"), iterations, units, old);
    } else {
        perf::measure_sampled(&format!("{name}/old"), iterations, units, old);
        perf::measure_sampled(&format!("{name}/new"), iterations, units, new);
    }
}

#[test]
#[ignore = "manual release before/after CPU and allocation benchmark"]
fn benchmark_pack_preparation() {
    let pack = pack_fixture(512);
    let old = LegacyPack(pack.clone());
    for count in [0, 1, 6, 11] {
        let selected = selection(count);
        compare(
            &format!("runtime_key/{count}"),
            4096,
            1,
            || old.runtime_key(black_box(&selected)),
            || pack.runtime_key(black_box(&selected)),
        );
        compare(
            &format!("compiler_key/{count}"),
            4096,
            1,
            || old.compiler_key(black_box(&selected)),
            || pack.compiler_key(black_box(&selected)),
        );
    }
    for (choices, files) in [(0, 0), (1, 1), (64, 4), (512, 4)] {
        let (fixture, _) = Fixture::new(choices, files);
        assert_load_matches(&fixture.0);
        compare(
            &format!("pack_load/{choices}x{files}"),
            if choices >= 512 { 2 } else { 8 },
            choices.max(1),
            || LegacyPack::load(black_box(&fixture.0)).unwrap(),
            || InstalledPack::load(black_box(&fixture.0)).unwrap(),
        );
    }
}
