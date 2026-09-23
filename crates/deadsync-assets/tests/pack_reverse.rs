use deadsync_assets::{init_paths, noteskin};
use deadsync_config::dirs::AppDirs;
use deadsync_noteskin::pack::Selection;
use std::{fs, path::PathBuf};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn hold_body_texture(noteskin: &noteskin::Noteskin, col: usize) -> String {
    noteskin
        .hold_visuals_for_col(col, false)
        .body_inactive
        .as_ref()
        .expect("hold body")
        .texture_key()
        .to_ascii_lowercase()
}

#[test]
fn reverse_player_gets_the_hold_art_a_pack_skin_picks_for_reverse() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let fixture = Fixture(std::env::temp_dir().join(format!(
        "deadsync-pack-reverse-{}-{stamp}",
        std::process::id()
    )));
    let dirs = AppDirs {
        data_dir: fixture.0.clone(),
        cache_dir: fixture.0.join("cache"),
        exe_dir: fixture.0.clone(),
        portable: true,
    };
    init_paths(dirs.asset_paths(Some(&fixture.0))).unwrap();

    let target = fixture.0.join("assets/noteskins/swap");
    let base = target.join("base");
    fs::create_dir_all(&base).unwrap();
    fs::write(
        base.join("metrics.ini"),
        "[Global]\nFallbackNoteSkin=common\n",
    )
    .unwrap();
    fs::write(
        base.join("NoteSkin.lua"),
        r#"local skin = {}
function skin.Load()
    local button = Var "Button"
    local element = Var "Element"
    local options = GAMESTATE:GetPlayerState(Var "Player"):GetPlayerOptionsString("ModsLevel_Preferred")
    local reverse = string.find(options:lower(), "reverse")
    if not string.find(element, "Hold Body") then
        button = "Down"
    elseif reverse and button == "Up" then
        button = "Down"
    elseif reverse and button == "Down" then
        button = "Up"
    end
    return Def.Sprite { Texture = NOTESKIN:GetPath(button, element) }
end
return skin
"#,
    )
    .unwrap();
    let mut textures = vec![
        "Down Tap Note.png".to_owned(),
        "Down Receptor.png".to_owned(),
    ];
    for button in ["Left", "Down", "Up", "Right"] {
        for state in ["Active", "Inactive"] {
            textures.push(format!("{button} Hold Body {state}.png"));
        }
    }
    for texture in &textures {
        image::RgbaImage::from_pixel(64, 64, image::Rgba([255, 255, 255, 255]))
            .save(base.join(texture))
            .unwrap();
    }
    image::RgbaImage::new(2048, 2048)
        .save(target.join("preview.png"))
        .unwrap();
    fs::write(target.join("pack.json"), r#"{"schema":1,"id":"swap","version":"1","source":"fixture","skins":[{"id":"swap-skin","base":"base","preview":"preview.png","options":[]}]}"#).unwrap();
    noteskin::refresh_packs(&target).unwrap();
    let style = noteskin::Style {
        num_cols: 4,
        num_players: 1,
    };

    let shared = noteskin::load_player_itg_skin_cached(&style, "swap-skin", false).unwrap();
    let reverse = noteskin::load_player_itg_skin_cached(&style, "swap-skin", true).unwrap();

    for (col, shared_button, reverse_button) in [
        (0, "left", "left"),
        (1, "down", "up"),
        (2, "up", "down"),
        (3, "right", "right"),
    ] {
        let expected = |button| format!("{button} hold body inactive");
        assert!(hold_body_texture(&shared, col).contains(&expected(shared_button)));
        assert!(hold_body_texture(&reverse, col).contains(&expected(reverse_button)));
    }
    let catalog = noteskin::pack_catalog();
    let pack = catalog
        .iter()
        .find(|pack| pack.skin("swap-skin").is_some())
        .unwrap();
    let compiled = dirs
        .noteskin_cache_dir()
        .join("dance")
        .join(pack.compiler_key(&Selection::parse("swap-skin").unwrap()));
    let loaders = fs::read_dir(compiled)
        .unwrap()
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "bin"))
        .count();
    assert_eq!(loaders, 2);
}
