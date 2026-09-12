use super::*;
use std::hint::black_box;

#[path = "asset_discovery/media_baseline.rs"]
mod baseline;
#[path = "asset_discovery/fixtures.rs"]
mod fixtures;
use fixtures::{Fixture, pair};

#[test]
fn borrowed_normalization_matches_old_for_generated_paths_and_prefixes() {
    let mut cases = vec![
        "",
        "/",
        "//",
        "///",
        ".",
        "..",
        "../a",
        "/../a",
        "a/../../b",
        "./a",
        "a//b",
        "a/./b",
        "a/../b",
        "a\\..\\b",
        "C:/Songs/file.png",
        "C:\\Songs\\file.png",
        "\\\\server\\share\\file.png",
        "日本/Été.png",
        "a/.../b",
        "a\0b",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    let mut seed = 87u64;
    let parts = ["", ".", "..", "...", "Asset", "日本", "a b", "C:"];
    for _ in 0..1024 {
        let mut value = String::new();
        for _ in 0..8 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            value.push_str(parts[(seed >> 32) as usize % parts.len()]);
            value.push(if seed & 1 == 0 { '/' } else { '\\' });
        }
        cases.push(value);
    }
    for value in cases {
        for (end, _) in value
            .char_indices()
            .chain(std::iter::once((value.len(), '\0')))
        {
            let path = &value[..end];
            assert_eq!(
                collapse_song_asset_path_like_itg(path),
                baseline::collapse_song_asset_path_like_itg(path),
                "{path:?}"
            );
        }
    }
}

#[test]
fn normalized_asset_tags_remain_borrowed_without_churn() {
    for path in [
        "",
        "/",
        "banner.png",
        "Visuals/日本/intro.lua",
        "C:/Songs/Pack/Banner.PNG",
        " /kept spaces ",
    ] {
        crate::perf::assert_no_churn(|| {
            let result = collapse_song_asset_path_like_itg(black_box(path));
            assert!(matches!(result, Cow::Borrowed(_)));
            black_box(result);
        });
    }
}

#[test]
fn resolved_paths_match_old_for_direct_fallback_and_parent_paths() {
    let fixture = Fixture::new("resolve", 0, 0);
    let base = fixture.path.join("Song");
    std::fs::create_dir_all(base.join("Visuals/日本")).unwrap();
    std::fs::write(base.join("Visuals/日本/Intro.PNG"), b"image").unwrap();
    std::fs::write(fixture.path.join("Outside.png"), b"image").unwrap();
    for tag in [
        "",
        " ",
        ".",
        "./",
        "a/..",
        "./Visuals/日本/Intro.PNG",
        "Visuals/日本/Intro.PNG",
        "visuals/日本/intro.png",
        "Visuals\\日本\\Intro.PNG",
        "Visuals/../Visuals/日本/Intro.PNG",
        "../Outside.png",
        "Visuals",
        "Missing.png",
        "\0",
        "Visuals/日本/Intro.PNG/child",
    ] {
        assert_eq!(
            resolve_song_path_like_itg(&base, tag),
            baseline::resolve_song_path_like_itg(&base, tag),
            "{tag:?}"
        );
    }
    let absolute = base
        .join("Visuals/日本/Intro.PNG")
        .to_string_lossy()
        .replace('\\', "/");
    assert_eq!(
        resolve_song_path_like_itg(&base, &absolute),
        baseline::resolve_song_path_like_itg(&base, &absolute)
    );
}

#[test]
fn recursive_entries_match_old_order_including_non_utf8_and_links() {
    let fixture = Fixture::new("entries", 24, 3);
    std::fs::write(fixture.path.join("._fork.png"), b"fork").unwrap();
    std::fs::create_dir(fixture.path.join("empty")).unwrap();
    let target = fixture.path.join("target.bin");
    std::fs::write(&target, b"target").unwrap();
    let file_link = fixtures::link(&target, &fixture.path.join("file-link"), false);
    fixtures::link(
        &fixture.path.join("Layer0"),
        &fixture.path.join("directory-link"),
        true,
    );
    fixtures::link(
        &fixture.path.join("missing"),
        &fixture.path.join("broken-link"),
        false,
    );
    #[cfg(windows)]
    let invalid_name = {
        use std::os::windows::ffi::OsStringExt;
        std::ffi::OsString::from_wide(&[b'x' as u16, 0xD800, b'y' as u16])
    };
    #[cfg(unix)]
    let invalid_name = {
        use std::os::unix::ffi::OsStringExt;
        std::ffi::OsString::from_vec(vec![b'x', 0xFF, b'y'])
    };
    std::fs::write(fixture.path.join(invalid_name), b"non-utf8").unwrap();
    for path in [
        fixture.path.clone(),
        fixture.path.join("."),
        fixture.path.join("missing"),
        target,
    ] {
        assert_eq!(
            list_song_dir_rel_entries(&path),
            baseline::list_song_dir_rel_entries(&path),
            "{path:?}"
        );
    }
    let entries = list_song_dir_rel_entries(&fixture.path);
    assert_eq!(entries.iter().any(|name| name == "file-link"), file_link);
    assert!(
        entries
            .iter()
            .any(|name| name.starts_with("directory-link/"))
    );
    assert!(!entries.iter().any(|name| name == "broken-link"));
}

#[test]
#[ignore = "manual release CPU/allocation benchmark"]
fn asset_discovery_bench() {
    for (label, value) in [
        ("empty", "".to_owned()),
        ("plain", "banner.png".to_owned()),
        ("nested", "Visuals/日本/Intro/Background.png".to_owned()),
        ("long", "Visuals/".repeat(32) + "banner.png"),
        ("collapse", "a//b/.././c/banner.png".to_owned()),
        ("backslash", r"Visuals\日本\Intro\Background.png".to_owned()),
        ("parents", "../../Visuals/banner.png".to_owned()),
    ] {
        pair(
            &format!("normalize_{label}"),
            16_384,
            1,
            || {
                Cow::Owned(baseline::collapse_song_asset_path_like_itg(black_box(
                    &value,
                )))
            },
            || collapse_song_asset_path_like_itg(black_box(&value)),
        );
    }
    let resolution = Fixture::new("resolve-bench", 0, 0);
    std::fs::create_dir_all(resolution.path.join("Visuals/日本")).unwrap();
    std::fs::write(resolution.path.join("banner.png"), b"png").unwrap();
    std::fs::write(resolution.path.join("Visuals/日本/Intro.png"), b"png").unwrap();
    for (label, tag) in [
        ("plain", "banner.png"),
        ("nested", "Visuals/日本/Intro.png"),
        ("collapse", "Visuals/../banner.png"),
        ("missing", "missing.png"),
        ("case", "visuals/日本/intro.PNG"),
    ] {
        pair(
            &format!("resolve_{label}"),
            128,
            1,
            || baseline::resolve_song_path_like_itg(black_box(&resolution.path), black_box(tag)),
            || resolve_song_path_like_itg(black_box(&resolution.path), black_box(tag)),
        );
    }
    for (label, files, depth) in [
        ("empty", 0, 0),
        ("small", 8, 0),
        ("flat128", 128, 0),
        ("flat512", 512, 0),
        ("nested", 32, 4),
    ] {
        let fixture = Fixture::new("entries-bench", files, depth);
        assert_eq!(
            list_song_dir_rel_entries(&fixture.path),
            baseline::list_song_dir_rel_entries(&fixture.path)
        );
        pair(
            &format!("entries_{label}"),
            32,
            (files * (depth + 1) + depth).max(1),
            || baseline::list_song_dir_rel_entries(black_box(&fixture.path)),
            || list_song_dir_rel_entries(black_box(&fixture.path)),
        );
    }
}
