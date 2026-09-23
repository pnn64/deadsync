//! Its own process: unit tests clear the shared noteskin runtime cache while
//! they run, which would evict the chart skin between the two loads.
use deadsync_assets::{init_paths, noteskin};
use deadsync_config::dirs::AppDirs;
use std::{fs, path::PathBuf, sync::Arc};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn chart_skin_runtime_is_reused_without_reading_the_skin_again() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let fixture = Fixture(std::env::temp_dir().join(format!(
        "deadsync-chart-skin-reuse-{}-{stamp}",
        std::process::id()
    )));
    let dirs = AppDirs {
        data_dir: fixture.0.clone(),
        cache_dir: fixture.0.join("cache"),
        exe_dir: fixture.0.clone(),
        portable: true,
    };
    init_paths(dirs.asset_paths(Some(&fixture.0))).unwrap();

    let song_dir = fixture.0.join("song");
    let skin_dir = song_dir.join("Reused");
    fs::create_dir_all(&skin_dir).unwrap();
    fs::write(
        skin_dir.join("metrics.ini"),
        "[Global]\nFallbackNoteSkin=common\n",
    )
    .unwrap();
    fs::write(
        skin_dir.join("NoteSkin.lua"),
        r#"local skin = {}
function skin.Load()
    return Def.Sprite { Texture = NOTESKIN:GetPath("Down", Var "Element") }
end
return skin
"#,
    )
    .unwrap();
    for texture in [
        "Down Tap Note.png",
        "Down Receptor.png",
        "Down Hold Body Active.png",
        "Down Hold Body Inactive.png",
    ] {
        image::RgbaImage::from_pixel(64, 64, image::Rgba([255, 255, 255, 255]))
            .save(skin_dir.join(texture))
            .unwrap();
    }
    let style = noteskin::Style {
        num_cols: 4,
        num_players: 1,
    };

    let loaded = noteskin::load_song_itg_skin_cached(&style, "reused", &song_dir, false)
        .expect("chart skin");
    fs::write(
        skin_dir.join("metrics.ini"),
        "[Global]\nFallbackNoteSkin=no-such-skin\n",
    )
    .unwrap();
    let reused = noteskin::load_song_itg_skin_cached(&style, "reused", &song_dir, false)
        .expect("resident chart skin");

    assert!(Arc::ptr_eq(&loaded.noteskin, &reused.noteskin));
}
