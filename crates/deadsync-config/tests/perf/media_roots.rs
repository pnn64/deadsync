use super::*;
use crate::perf::{assert_reduced_churn, measure_sampled};
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/media_roots/baseline.rs"
    ));
}

fn dirs(data: &str, exe: &str) -> AppDirs {
    AppDirs {
        data_dir: data.into(),
        cache_dir: "cache".into(),
        exe_dir: exe.into(),
        portable: data == exe,
    }
}

#[test]
fn media_roots_preserve_exact_paths_order_and_duplicates() {
    for (data, exe) in [
        ("", ""),
        ("game", "game"),
        ("data", "exe"),
        ("game/", "game/."),
        (r"C:\game", r"C:\game"),
        (r"\\host\share\game", r"\\host\share\game"),
        (r"\\?\C:\game\.", r"\\?\C:\game"),
        ("日本語/Été", "日本語/Été"),
    ] {
        let dirs = dirs(data, exe);
        for cwd in [
            None,
            Some(Path::new("")),
            Some(Path::new(data)),
            Some(Path::new("checkout")),
        ] {
            for name in [
                "",
                ".",
                "..",
                "SongMovies",
                "assets/noteskins",
                "./assets//noteskins",
                "/rooted",
                r"D:\absolute",
                r"D:relative",
                "日本語/Été",
            ] {
                let old = dirs.original_media_roots(name, cwd);
                let new = dirs.media_roots(name, cwd);
                assert_eq!(old.len(), new.len(), "{data:?} {exe:?} {cwd:?} {name:?}");
                assert!(
                    old.iter()
                        .zip(&new)
                        .all(|(old, new)| old.as_os_str() == new.as_os_str()),
                    "old={old:?} new={new:?}"
                );
            }
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        let mut dirs = dirs("", "");
        dirs.data_dir =
            std::ffi::OsString::from_wide(&[b'C' as u16, 58, 92, 0xd800, 92, 0x65e5]).into();
        dirs.exe_dir = dirs.data_dir.clone();
        let old = dirs.original_media_roots("SongMovies", Some(&dirs.data_dir));
        let new = dirs.media_roots("SongMovies", Some(&dirs.data_dir));
        assert!(
            old.iter()
                .zip(&new)
                .all(|(old, new)| old.as_os_str() == new.as_os_str())
        );
        assert_eq!(old.len(), new.len());
    }
}

#[test]
fn checkout_media_roots_allocate_less() {
    for (data, exe, cwd) in [
        ("game", "game", Some(Path::new("checkout"))),
        ("data", "exe", Some(Path::new("checkout"))),
    ] {
        let dirs = dirs(data, exe);
        assert_reduced_churn(
            || {
                drop(black_box(
                    dirs.original_media_roots("assets/noteskins", cwd),
                ))
            },
            || drop(black_box(dirs.media_roots("assets/noteskins", cwd))),
        );
    }
}

fn pair<T>(name: &str, old: impl FnMut() -> T, new: impl FnMut() -> T) {
    if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
        measure_sampled(&format!("media-roots/{name}/current"), 4096, 1, new);
        measure_sampled(&format!("media-roots/{name}/original"), 4096, 1, old);
    } else {
        measure_sampled(&format!("media-roots/{name}/original"), 4096, 1, old);
        measure_sampled(&format!("media-roots/{name}/current"), 4096, 1, new);
    }
}

#[test]
#[ignore = "manual release CPU/allocation benchmark"]
fn media_roots_benchmark() {
    let original = black_box(
        AppDirs::original_media_roots as fn(&AppDirs, &str, Option<&Path>) -> Vec<PathBuf>,
    );
    let current =
        black_box(AppDirs::media_roots as fn(&AppDirs, &str, Option<&Path>) -> Vec<PathBuf>);
    let long = format!("C:/{}", "日本語/Été/long directory/".repeat(24));
    for (label, data, exe, cwd) in [
        ("portable", "C:/game", "C:/game", None),
        (
            "portable-cwd",
            "C:/game",
            "C:/game",
            Some(Path::new("C:/game")),
        ),
        ("installed", "C:/data", "C:/game", None),
        (
            "installed-cwd",
            "C:/data",
            "C:/game",
            Some(Path::new("C:/checkout")),
        ),
        (
            "long-portable",
            long.as_str(),
            long.as_str(),
            Some(Path::new(&long)),
        ),
    ] {
        let dirs = dirs(data, exe);
        pair(
            label,
            || {
                original(
                    black_box(&dirs),
                    black_box("assets/noteskins"),
                    black_box(cwd),
                )
            },
            || {
                current(
                    black_box(&dirs),
                    black_box("assets/noteskins"),
                    black_box(cwd),
                )
            },
        );
    }
}
