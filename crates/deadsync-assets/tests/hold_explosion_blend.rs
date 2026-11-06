//! IIDX-Tony-style ActorFrame wrappers must not replace a child's InitCommand.
use deadsync_assets::{init_paths, noteskin};
use deadsync_config::dirs::AppDirs;
use std::{fs, path::PathBuf};

#[test]
fn hold_and_roll_explosions_keep_child_blend_commands() {
    let temporary = tempfile::tempdir().unwrap();
    let song = temporary.path().join("song");
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let dirs = AppDirs {
        data_dir: temporary.path().join("data"),
        exe_dir: workspace.clone(),
        cache_dir: temporary.path().join("cache"),
        portable: true,
    };
    init_paths(dirs.asset_paths(Some(&workspace))).unwrap();
    let style = noteskin::Style {
        num_cols: 4,
        num_players: 1,
    };
    for (name, init_blend, active_blend, expected) in [
        ("blend-add", "self:blend('BlendMode_Add')", "", true),
        ("blend-normal", "self:blend('BlendMode_Normal')", "", false),
        ("blend-default", "", "", false),
        ("blend-frame-add", "", "", false),
        ("blend-parent", "self:blend('BlendMode_Add')", "", true),
        (
            "blend-override",
            "self:blend('BlendMode_Add')",
            "blend,'BlendMode_Normal';",
            false,
        ),
    ] {
        let skin_dir = song.join(name);
        fs::create_dir_all(&skin_dir).unwrap();
        fs::write(
            skin_dir.join("metrics.ini"),
            "[Global]\nFallbackNoteSkin=common\n",
        )
        .unwrap();
        fs::write(
            skin_dir.join("NoteSkin.lua"),
            r#"
local skin = ... or {}
function skin.Load()
    local element = Var "Element"
    if element == "Roll Explosion" then element = "Hold Explosion" end
    return LoadActor(NOTESKIN:GetPath("Down", element))
end
return skin
"#,
        )
        .unwrap();
        let parent_active = match name {
            "blend-frame-add" => "blend,'BlendMode_Add'",
            "blend-parent" => "glow,1,1,1,0.5;diffuse,0.5,0.75,1,0.5;linear,0.2;diffusealpha,0.25",
            _ => "stoptweening",
        };
        fs::write(
            skin_dir.join("Down Explosion.lua"),
            format!(
                r#"
return Def.ActorFrame {{
    NOTESKIN:LoadActor(Var "Button", "Hold Explosion") .. {{
        InitCommand=cmd(playcommand,"HoldingOff";finishtweening);
        HoldingOnCommand=cmd({parent_active});
        HoldingOffCommand=cmd(stoptweening);
        RollOnCommand=cmd({parent_active});
        RollOffCommand=cmd(stoptweening);
    }};
}}
"#
            ),
        )
        .unwrap();
        fs::write(
            skin_dir.join("Down Hold Explosion.lua"),
            format!(
                r#"
return Def.ActorFrame {{
    Def.Sprite {{
        Texture="hold 4x3.png";
        Frames=Sprite.LinearFrames(2,0.01);
        InitCommand=function(self) {init_blend} self:diffusealpha(0) end;
        HoldingOnCommand=cmd({active_blend}diffusealpha,1;effectperiod,.05);
        HoldingOffCommand=cmd(diffusealpha,0);
        RollOnCommand=cmd({active_blend}diffusealpha,1;effectperiod,.05);
        RollOffCommand=cmd(diffusealpha,0);
    }};
}}
"#
            ),
        )
        .unwrap();
        for texture in ["Down Tap Note.png", "Down Receptor.png", "hold 4x3.png"] {
            image::RgbaImage::from_pixel(32, 24, image::Rgba([255, 255, 255, 255]))
                .save(skin_dir.join(texture))
                .unwrap();
        }
        let skin = noteskin::load_song_skin(&style, &song, name, "").unwrap();
        for column in 0..4 {
            for is_roll in [false, true] {
                let slot = skin
                    .hold_visuals_for_col(column, is_roll)
                    .explosion
                    .as_ref()
                    .unwrap_or_else(|| {
                        panic!("{name}, column {column}, roll={is_roll}: explosion resolves")
                    });
                let mut cursor = deadsync_noteskin::ModelTweenCursor::default();
                for time in [0.0, 0.01, 0.1, 1.0] {
                    let draw = slot.model_draw_at(time, time * 2.0);
                    let cached = slot.model_draw_at_cursor(time, time * 2.0, &mut cursor);
                    assert_eq!(
                        draw.blend_add, expected,
                        "{name}, column {column}, roll={is_roll}, t={time}"
                    );
                    let alpha = if name == "blend-parent" {
                        assert_eq!(draw.tint[..3], [0.5, 0.75, 1.0]);
                        0.5 - 0.25 * (time / 0.2).min(1.0)
                    } else {
                        1.0
                    };
                    assert!(
                        (draw.tint[3] - alpha).abs() < 1e-6,
                        "{name}, column {column}, roll={is_roll}: {draw:?}"
                    );
                    assert_eq!(cached.blend_add, draw.blend_add);
                    assert_eq!(cached.tint, draw.tint);
                    if name == "blend-parent" {
                        assert_eq!(
                            slot.model_glow_with_draw(draw, time, time * 2.0, alpha),
                            Some([1.0, 1.0, 1.0, 0.5])
                        );
                        assert_eq!(
                            slot.model_glow_at(time, time * 2.0, alpha),
                            Some([1.0, 1.0, 1.0, 0.5])
                        );
                    }
                    assert!(draw.visible);
                }
            }
        }
    }
}
