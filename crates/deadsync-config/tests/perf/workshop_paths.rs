use super::*;
use crate::perf::{assert_reduced_churn, measure_sampled};
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/workshop_paths/baseline.rs"
    ));
}

fn fixture() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/perf-1707-workshop");
    for base in [
        "game",
        "checkout",
        "checkout/deadsync",
        "nested/deadsync",
        "blocked/deadsync",
    ] {
        std::fs::create_dir_all(root.join(base).join("assets/noteskins")).unwrap();
    }
    std::fs::create_dir_all(root.join("blocked/assets")).unwrap();
    std::fs::write(
        root.join("blocked/assets/noteskins"),
        b"file, not directory",
    )
    .unwrap();
    assert!(!root.join("missing").exists());
    root
}

fn dirs(root: &Path, portable: bool, exe: &str) -> AppDirs {
    let exe_dir = root.join(exe);
    let data_dir = if portable {
        exe_dir.clone()
    } else {
        root.join("data")
    };
    AppDirs {
        cache_dir: data_dir.join("cache"),
        data_dir,
        exe_dir,
        portable,
    }
}

#[test]
fn workshop_and_asset_paths_preserve_exact_paths_and_overlay_precedence() {
    let root = fixture();
    for portable in [false, true] {
        for exe in ["game", "missing", "blocked", "game/.", ""] {
            let dirs = dirs(&root, portable, exe);
            let cwds = [
                root.join("checkout"),
                root.join("nested"),
                root.join("blocked"),
                root.join("missing"),
                root.join("checkout/."),
            ];
            for cwd in std::iter::once(None).chain(cwds.iter().map(|path| Some(path.as_path()))) {
                let old = dirs.original_workshop_dir(cwd);
                let new = dirs.workshop_dir(cwd);
                assert_eq!(old.as_os_str(), new.as_os_str(), "exe={exe}, cwd={cwd:?}");
                let old = dirs.original_asset_paths(cwd);
                let new = dirs.asset_paths(cwd);
                // Debug retains raw separator spelling, unlike Path equality.
                assert_eq!(format!("{old:?}"), format!("{new:?}"));
                assert_eq!(
                    new.noteskin_pack_roots[0].as_os_str(),
                    dirs.workshop_pack_root(cwd).as_os_str()
                );
            }
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        let raw: PathBuf = std::ffi::OsString::from_wide(&[67, 58, 92, 0xd800, 92, 0x65e5]).into();
        let dirs = AppDirs {
            data_dir: raw.clone(),
            cache_dir: raw.clone(),
            exe_dir: raw.clone(),
            portable: true,
        };
        for cwd in [None, Some(raw.as_path())] {
            assert_eq!(
                dirs.original_workshop_dir(cwd).as_os_str(),
                dirs.workshop_dir(cwd).as_os_str()
            );
            assert_eq!(
                format!("{:?}", dirs.original_asset_paths(cwd)),
                format!("{:?}", dirs.asset_paths(cwd))
            );
        }
    }
}

#[test]
fn workshop_paths_remove_parent_child_round_trips_and_root_copies() {
    let root = fixture();
    let dirs = dirs(&root, false, "game");
    for cwd in [
        None,
        Some(root.join("checkout")),
        Some(root.join("nested")),
        Some(root.join("missing")),
    ] {
        assert_reduced_churn(
            || drop(black_box(dirs.original_workshop_dir(cwd.as_deref()))),
            || drop(black_box(dirs.workshop_dir(cwd.as_deref()))),
        );
        assert_reduced_churn(
            || drop(black_box(dirs.original_asset_paths(cwd.as_deref()))),
            || drop(black_box(dirs.asset_paths(cwd.as_deref()))),
        );
    }
}

#[test]
#[ignore = "manual release CPU/allocation benchmark"]
fn workshop_paths_benchmark() {
    let root = fixture();
    let cwd_root = root.join("checkout");
    let nested_root = root.join("nested");
    let missing_root = root.join("missing");
    let blocked_root = root.join("blocked");
    let installed = dirs(&root, false, "game");
    let portable = dirs(&root, true, "game");
    let absent = dirs(&root, false, "missing");
    for (name, dirs, cwd) in [
        ("installed", &installed, None),
        ("portable", &portable, None),
        ("checkout", &installed, Some(cwd_root.as_path())),
        ("nested", &installed, Some(nested_root.as_path())),
        ("file-first", &installed, Some(blocked_root.as_path())),
        ("fallback", &absent, Some(missing_root.as_path())),
    ] {
        let run = |variant,
                   workshop: fn(&AppDirs, Option<&Path>) -> PathBuf,
                   assets: fn(&AppDirs, Option<&Path>) -> AssetPaths| {
            measure_sampled(
                &format!("workshop-paths/workshop-{name}/{variant}"),
                512,
                1,
                || workshop(black_box(dirs), black_box(cwd)),
            );
            measure_sampled(
                &format!("workshop-paths/assets-{name}/{variant}"),
                512,
                1,
                || assets(black_box(dirs), black_box(cwd)),
            );
        };
        let old = (
            black_box(AppDirs::original_workshop_dir as fn(&AppDirs, Option<&Path>) -> PathBuf),
            black_box(AppDirs::original_asset_paths as fn(&AppDirs, Option<&Path>) -> AssetPaths),
        );
        let new = (
            black_box(AppDirs::workshop_dir as fn(&AppDirs, Option<&Path>) -> PathBuf),
            black_box(AppDirs::asset_paths as fn(&AppDirs, Option<&Path>) -> AssetPaths),
        );
        if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            run("current", new.0, new.1);
            run("original", old.0, old.1);
        } else {
            run("original", old.0, old.1);
            run("current", new.0, new.1);
        }
    }
}
