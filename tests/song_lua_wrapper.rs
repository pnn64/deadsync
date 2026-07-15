use deadsync_assets::song_lua::{
    SongLuaCompileContext, SongLuaDifficulty, SongLuaOverlayBlendMode, SongLuaOverlayKind,
    SongLuaPlayerContext, SongLuaSpeedMod, compile_song_lua,
};
use std::fs;
use std::path::PathBuf;

fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("deadsync-song-lua-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn compile_song_lua_exposes_noteskin_helpers() {
    let song_dir = test_dir("noteskin-helpers");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local x = NOTESKIN:GetMetricF("", "TapNoteNoteColorTextureCoordSpacingX")
local y = NOTESKIN:GetMetricFForNoteSkin("", "TapNoteNoteColorTextureCoordSpacingY", "cyber")
local xi = NOTESKIN:GetMetricI("", "TapNoteNoteColorTextureCoordSpacingX")
local yi = NOTESKIN:GetMetricIForNoteSkin("", "TapNoteNoteColorTextureCoordSpacingY", "cyber")
local vivid = NOTESKIN:GetMetricBForNoteSkin("", "TapNoteAnimationIsVivid", "cyber")
local metric_cmd = NOTESKIN:GetMetricA("", "OnCommand")
local metric_cmd_for_skin = NOTESKIN:GetMetricAForNoteSkin("", "OnCommand", "cyber")
local path = NOTESKIN:GetPathForNoteSkin("Down", "Tap Explosion Bright W1", "cyber")
local actor = NOTESKIN:LoadActorForNoteSkin("Down", "Tap Explosion Bright W1", "cyber")
local default_path = NOTESKIN:GetPath("Down", "Tap Explosion Bright W1")
local default_actor = NOTESKIN:LoadActor("Down", "Tap Explosion Bright W1")
local metric_actor = Def.Actor{Name="MetricActor"}

if math.abs(x - 0.125) > 0.0001 then
    error("unexpected noteskin metric x: " .. tostring(x))
end
if math.abs(y - 0.0) > 0.0001 then
    error("unexpected noteskin metric y: " .. tostring(y))
end
if xi ~= 0 or yi ~= 0 then
    error("unexpected noteskin integer metrics: " .. tostring(xi) .. ":" .. tostring(yi))
end
if vivid ~= false then
    error("unexpected noteskin vivid flag: " .. tostring(vivid))
end
if NOTESKIN:DoesNoteSkinExist("cyber") ~= true then
    error("expected cyber noteskin to exist")
end
if metric_cmd(metric_actor) ~= metric_actor or metric_cmd_for_skin(metric_actor) ~= metric_actor then
    error("expected noteskin actor-command metrics to preserve actors")
end
if type(path) ~= "string" or path == "" then
    error("expected noteskin path")
end
if type(default_path) ~= "string" or default_path == "" then
    error("expected default noteskin path")
end
if type(actor) ~= "table" or type(default_actor) ~= "table" then
    error("expected noteskin actor table")
end
if #NOTESKIN:GetNoteSkinNames(false) < 1 then
    error("expected noteskin names")
end

mod_actions = {
    {4, tostring(vivid) .. ":" .. tostring(x) .. ":" .. tostring(xi), true},
}

return Def.ActorFrame{
    actor..{
        Name="NoteskinExplosion",
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Noteskin Helpers");
    context.players = [
        SongLuaPlayerContext {
            enabled: true,
            noteskin_name: "cyber".to_string(),
            ..SongLuaPlayerContext::default()
        },
        SongLuaPlayerContext {
            enabled: false,
            ..SongLuaPlayerContext::default()
        },
    ];

    let compiled = compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "false:0.125:0");
    assert!(
        compiled.overlays.iter().any(|overlay| {
            overlay.name.as_deref() == Some("NoteskinExplosion")
                && matches!(overlay.kind, SongLuaOverlayKind::Sprite { .. })
        }),
        "noteskin actor should materialize as a sprite overlay when it resolves to an image"
    );
}

#[test]
fn compile_song_lua_reuses_noteskin_tap_model_slots() {
    let song_dir = test_dir("noteskin-tap-model-slots");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
	return Def.ActorFrame{
	    NOTESKIN:LoadActorForNoteSkin("Down", "Tap Note", "ddr-note")..{
	        Name="NoteskinTap",
	    },
	}
	"#,
    )
    .unwrap();

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Noteskin Tap Model Slots"),
    )
    .unwrap();
    let overlay = compiled
        .overlays
        .iter()
        .find(|overlay| overlay.name.as_deref() == Some("NoteskinTap"))
        .unwrap();
    let SongLuaOverlayKind::NoteskinActor { slots } = &overlay.kind else {
        panic!("tap note model should keep loaded noteskin slots");
    };
    assert!(slots.len() >= 2);
    assert!(slots.iter().any(|slot| {
        slot.texture_key().to_ascii_lowercase().contains("ddr-note")
            && slot
                .model
                .as_ref()
                .is_some_and(|model| model.vertices.len() > 6)
    }));
    assert!(slots.iter().any(|slot| {
        slot.model.as_ref().is_some_and(|model| {
            model
                .vertices
                .iter()
                .any(|vertex| vertex.tex_matrix_scale == [0.0, 0.0])
        })
    }));
}

#[test]
fn compile_song_lua_extracts_model_overlay_layers() {
    let song_dir = test_dir("model-overlay-layers");
    let entry = song_dir.join("default.lua");
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets/graphics/menu_bg_technique/ring_model.txt");
    let model_path = model_path.to_string_lossy().replace('\\', "\\\\");
    fs::write(
        &entry,
        format!(
            r#"
return Def.ActorFrame{{
    Def.Model{{
        Meshes="{model_path}",
        Materials="{model_path}",
        Bones="{model_path}",
        OnCommand=function(self)
            self:xy(SCREEN_CENTER_X, SCREEN_CENTER_Y)
                :zoom(0.75)
                :baserotationx(-60)
                :baserotationy(20)
                :baserotationz(50)
                :diffuse(1, 1, 1, 0.8)
        end,
    }},
}}
"#
        ),
    )
    .unwrap();

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Model Overlay Layers"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    let SongLuaOverlayKind::Model { layers } = &compiled.overlays[0].kind else {
        panic!("expected Model overlay");
    };
    assert!(!layers.is_empty());
    assert!(layers.iter().all(|layer| !layer.vertices.is_empty()));
    assert!(layers.iter().all(|layer| !layer.texture_key.is_empty()));
    assert!(layers[0].model_size[0] > 0.0);
    assert_eq!(compiled.overlays[0].initial_state.rot_x_deg, -60.0);
    assert_eq!(compiled.overlays[0].initial_state.rot_y_deg, 20.0);
    assert_eq!(compiled.overlays[0].initial_state.rot_z_deg, 50.0);
}

#[test]
fn compile_song_lua_loads_bundled_noteskin_actor_fixture() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/song_lua");
    let entry = root.join("noteskin-overlay.lua");
    assert!(entry.is_file(), "missing fixture: {}", entry.display());

    let mut context = SongLuaCompileContext::new(&root, "Noteskin Overlay Fixture");
    context.style_name = "double".to_string();
    context.music_length_seconds = 48.0;
    context.players = [
        SongLuaPlayerContext {
            enabled: true,
            difficulty: SongLuaDifficulty::Challenge,
            speedmod: SongLuaSpeedMod::X(2.0),
            noteskin_name: "ddr-note".to_string(),
            ..SongLuaPlayerContext::default()
        },
        SongLuaPlayerContext {
            enabled: false,
            difficulty: SongLuaDifficulty::Challenge,
            speedmod: SongLuaSpeedMod::X(2.0),
            ..SongLuaPlayerContext::default()
        },
    ];

    let compiled = compile_song_lua(&entry, &context).unwrap();
    let arrow_index = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("FixtureArrow"))
        .unwrap();
    let SongLuaOverlayKind::NoteskinActor { slots } = &compiled.overlays[arrow_index].kind else {
        panic!("fixture arrow should reuse the bundled ddr-note actor");
    };
    assert!(slots.len() >= 2);
    assert!(slots.iter().any(|slot| {
        slot.texture_key().to_ascii_lowercase().contains("ddr-note")
            && slot
                .model
                .as_ref()
                .is_some_and(|model| model.vertices.len() > 6)
    }));
    assert!(slots.iter().any(|slot| {
        slot.model.as_ref().is_some_and(|model| {
            model
                .vertices
                .iter()
                .any(|vertex| vertex.tex_matrix_scale == [1.0, 1.0])
        })
    }));
    assert!(slots.iter().any(|slot| {
        slot.model.as_ref().is_some_and(|model| {
            model
                .vertices
                .iter()
                .any(|vertex| vertex.tex_matrix_scale == [0.0, 0.0])
        })
    }));
    assert!(slots.iter().any(|slot| slot.uv_velocity[1] < -0.5));
    assert_eq!(compiled.overlays[arrow_index].initial_state.rot_z_deg, 90.0);
}

#[test]
fn compile_song_lua_supports_rgb_aft_fixture() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/song_lua");
    let entry = root.join("aft.lua");
    assert!(entry.is_file(), "missing fixture: {}", entry.display());

    let mut context = SongLuaCompileContext::new(&root, "RGB AFT Fixture");
    context.style_name = "double".to_string();
    context.players = [
        SongLuaPlayerContext {
            enabled: true,
            difficulty: SongLuaDifficulty::Challenge,
            speedmod: SongLuaSpeedMod::X(2.0),
            ..SongLuaPlayerContext::default()
        },
        SongLuaPlayerContext {
            enabled: false,
            difficulty: SongLuaDifficulty::Challenge,
            speedmod: SongLuaSpeedMod::C(516.0),
            ..SongLuaPlayerContext::default()
        },
    ];

    let compiled = compile_song_lua(&entry, &context).unwrap();
    assert!(compiled.hidden_players[0]);
    assert!(!compiled.player_actors[0].initial_state.visible);
    let bg_quad = compiled
        .overlays
        .iter()
        .find(|overlay| overlay.name.as_deref() == Some("BGQuad"))
        .expect("fixture should compile the black AFT backing quad");
    assert_eq!(bg_quad.initial_state.diffuse, [0.0, 0.0, 0.0, 1.0]);
    assert!(
        compiled
            .overlays
            .iter()
            .any(|overlay| matches!(overlay.kind, SongLuaOverlayKind::ActorFrameTexture))
    );
    assert!(
        compiled
            .overlays
            .iter()
            .any(|overlay| matches!(overlay.kind, SongLuaOverlayKind::AftSprite { .. }))
    );
    for (name, diffuse) in [
        ("AFTSpriteR", [1.0, 0.0, 0.0, 1.0]),
        ("AFTSpriteG", [0.0, 1.0, 0.0, 1.0]),
        ("AFTSpriteB", [0.0, 0.0, 1.0, 1.0]),
    ] {
        let overlay = compiled
            .overlays
            .iter()
            .find(|overlay| overlay.name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("fixture should compile {name}"));
        assert_eq!(overlay.initial_state.diffuse, diffuse);
        assert_eq!(overlay.initial_state.blend, SongLuaOverlayBlendMode::Add);
        assert_eq!(overlay.initial_state.effect_magnitude, [0.0; 3]);
    }
}
