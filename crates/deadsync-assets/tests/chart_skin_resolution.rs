//! Its own process: unit tests clear the shared noteskin runtime cache while
//! they run, which would evict runtimes between the loads compared here.
use deadsync_assets::{init_paths, noteskin};
use deadsync_config::dirs::AppDirs;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

/// Asset paths are set once per process, so this file's tests share a root
/// and each works in folders of its own.
fn root() -> &'static Path {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let root = std::env::temp_dir().join(format!(
            "deadsync-chart-skin-resolution-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let dirs = AppDirs {
            data_dir: root.clone(),
            cache_dir: root.join("cache"),
            exe_dir: root.clone(),
            portable: true,
        };
        init_paths(dirs.asset_paths(Some(&root))).unwrap();
        root
    })
}

struct Cleanup(Vec<PathBuf>);
impl Drop for Cleanup {
    fn drop(&mut self) {
        for dir in &self.0 {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

const STYLE: noteskin::Style = noteskin::Style {
    num_cols: 4,
    num_players: 1,
};

/// A skin that needs nothing outside its own folder, since an installed
/// skin's fallbacks resolve only from its own noteskin root.
fn write_skin(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    let name = dir.file_name().unwrap().to_string_lossy();
    fs::write(
        dir.join("metrics.ini"),
        format!("[Global]\nFallbackNoteSkin={name}\n"),
    )
    .unwrap();
    fs::write(
        dir.join("NoteSkin.lua"),
        r#"local skin = {}
function skin.Load()
    return Def.Sprite { Texture = NOTESKIN:GetPath("Down", Var "Element") }
end
return skin
"#,
    )
    .unwrap();
    for element in TEXTURED_ELEMENTS {
        image::RgbaImage::from_pixel(64, 64, image::Rgba([255, 255, 255, 255]))
            .save(dir.join(format!("Down {element}.png")))
            .unwrap();
    }
}

const TEXTURED_ELEMENTS: [&str; 4] = [
    "Tap Note",
    "Receptor",
    "Hold Body Active",
    "Hold Body Inactive",
];

fn write_skin_missing_its_fallback(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join("metrics.ini"),
        "[Global]\nFallbackNoteSkin=no-such-fallback\n",
    )
    .unwrap();
}

fn installed_skin_dir(name: &str) -> PathBuf {
    root().join("assets/noteskins/dance").join(name)
}

fn song_dir(name: &str) -> PathBuf {
    let dir = root().join("songs").join(name);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn tap_texture(noteskin: &noteskin::Noteskin) -> String {
    noteskin.notes[0].texture_key().to_ascii_lowercase()
}

#[test]
fn a_chart_skin_found_only_among_installed_skins_is_the_installed_runtime() {
    let skin = "chart-installed-only";
    let song = song_dir("installed-only");
    let _cleanup = Cleanup(vec![installed_skin_dir(skin), song.clone()]);
    write_skin(&installed_skin_dir(skin));

    let installed = noteskin::load_player_itg_skin_cached(&STYLE, skin, false).unwrap();
    let chart = noteskin::load_song_itg_skin_cached(&STYLE, skin, &song, false).unwrap();

    assert!(Arc::ptr_eq(&installed, &chart.noteskin));
    assert_eq!(chart.dir, None);
}

#[test]
fn a_chart_skin_found_nowhere_is_an_error_rather_than_the_default() {
    let song = song_dir("found-nowhere");
    let _cleanup = Cleanup(vec![song.clone()]);

    let error = noteskin::load_song_itg_skin_cached(&STYLE, "chart-found-nowhere", &song, false)
        .expect_err("an unknown chart skin");

    assert!(error.contains("chart-found-nowhere"), "{error}");
}

#[test]
fn a_song_copy_of_an_installed_skin_gets_a_runtime_of_its_own() {
    let skin = "chart-shadowed";
    let song = song_dir("shadowing-song");
    let _cleanup = Cleanup(vec![installed_skin_dir(skin), song.clone()]);
    write_skin(&installed_skin_dir(skin));
    write_skin(&song.join(skin));

    let installed = noteskin::load_player_itg_skin_cached(&STYLE, skin, false).unwrap();
    let chart = noteskin::load_song_itg_skin_cached(&STYLE, skin, &song, false).unwrap();
    let installed_again = noteskin::load_player_itg_skin_cached(&STYLE, skin, false).unwrap();

    assert!(!Arc::ptr_eq(&installed, &chart.noteskin));
    assert_eq!(chart.dir, Some(song.join(skin)));
    assert!(tap_texture(&chart.noteskin).contains("shadowing-song"));
    assert!(!tap_texture(&installed).contains("shadowing-song"));
    assert!(Arc::ptr_eq(&installed, &installed_again));
}

#[test]
fn a_song_copy_that_fails_to_load_gives_way_to_the_installed_skin() {
    let skin = "chart-broken-copy";
    let song = song_dir("broken-copy");
    let _cleanup = Cleanup(vec![installed_skin_dir(skin), song.clone()]);
    write_skin(&installed_skin_dir(skin));
    write_skin_missing_its_fallback(&song.join(skin));

    let installed = noteskin::load_player_itg_skin_cached(&STYLE, skin, false).unwrap();
    let chart = noteskin::load_song_itg_skin_cached(&STYLE, skin, &song, false).unwrap();

    assert!(Arc::ptr_eq(&installed, &chart.noteskin));
    assert_eq!(chart.dir, None);
    let copy_error = chart.copy_error.expect("the copy's error");
    assert!(copy_error.contains("no-such-fallback"), "{copy_error}");
}

#[test]
fn a_song_copy_and_installed_skin_that_both_fail_report_both_errors() {
    let skin = "chart-broken-twice";
    let song = song_dir("broken-twice");
    let _cleanup = Cleanup(vec![installed_skin_dir(skin), song.clone()]);
    write_skin(&installed_skin_dir(skin));
    fs::write(installed_skin_dir(skin).join("NoteSkin.lua"), "return {").unwrap();
    write_skin_missing_its_fallback(&song.join(skin));

    let error = noteskin::load_song_itg_skin_cached(&STYLE, skin, &song, false)
        .expect_err("both copies are broken");

    assert!(error.contains("no-such-fallback"), "{error}");
    assert!(error.contains("installed noteskin"), "{error}");
}

#[test]
fn forgetting_song_skins_picks_up_skins_added_edited_or_fixed_since() {
    let song = song_dir("reloaded");
    let _cleanup = Cleanup(vec![song.clone()]);
    let load = |skin| noteskin::load_song_itg_skin_cached(&STYLE, skin, &song, false);

    assert!(load("chart-added").is_err());
    write_skin(&song.join("chart-added"));
    write_skin_missing_its_fallback(&song.join("chart-fixed"));
    assert!(load("chart-fixed").is_err());
    write_skin(&song.join("chart-fixed"));
    let edited = song.join("chart-edited");
    write_skin(&edited);
    let before = load("chart-edited").unwrap();
    let lua = fs::read_to_string(edited.join("NoteSkin.lua")).unwrap();
    fs::write(
        edited.join("NoteSkin.lua"),
        lua.replace("\"Down\"", "\"Edited\""),
    )
    .unwrap();
    for element in TEXTURED_ELEMENTS {
        fs::copy(
            edited.join(format!("Down {element}.png")),
            edited.join(format!("Edited {element}.png")),
        )
        .unwrap();
    }

    noteskin::forget_song_skins();

    assert!(load("chart-added").is_ok());
    assert!(load("chart-fixed").is_ok());
    let after = load("chart-edited").unwrap();
    assert!(!Arc::ptr_eq(&before.noteskin, &after.noteskin));
    assert!(tap_texture(&after.noteskin).contains("edited tap note"));
}
