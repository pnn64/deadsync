use deadlib_present::anim::{EffectClock, EffectMode};
use deadsync_assets::song_lua::{
    SongLuaCompileContext, SongLuaDifficulty, SongLuaEaseTarget, SongLuaOverlayBlendMode,
    SongLuaOverlayKind, SongLuaPlayerContext, SongLuaSpeedMod, THEME_RECEPTOR_Y_STD,
    compile_song_lua,
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
fn compile_song_lua_supports_flip69_sample_if_present() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("songs/ITL Online 2026 Unlocks/[10] flip69 (DX) [Telperion]");
    let entry = root.join("multitap/Default.lua");
    if !entry.is_file() {
        return;
    }

    let mut context = SongLuaCompileContext::new(&root, "[5604] [10] flip69");
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
    assert!(!compiled.overlays.is_empty());
    assert!(!compiled.overlay_eases.is_empty());
    assert!(!compiled.note_hides.is_empty());
    let field = compiled
        .overlays
        .iter()
        .find(|overlay| overlay.name.as_deref() == Some("MultitapFrameP1"))
        .unwrap();
    assert_eq!(field.initial_state.x, context.players[0].screen_x);
    assert_eq!(field.initial_state.y, context.players[0].screen_y);
    let first_arrow = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("MultitapArrowP1_1"))
        .unwrap();
    assert!(
        compiled
            .overlay_eases
            .iter()
            .any(|ease| { ease.overlay_index == first_arrow && ease.to.rot_z_deg == Some(90.0) })
    );
    let SongLuaOverlayKind::NoteskinActor { slots } = &compiled.overlays[first_arrow].kind else {
        panic!("ddr-note multitap arrow should reuse the loaded noteskin actor");
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
    let first_frame = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("MultitapP1_1"))
        .unwrap();
    assert!(compiled.overlay_eases.iter().any(|ease| {
        ease.overlay_index == first_frame
            && ease
                .to
                .y
                .is_some_and(|y| (y - THEME_RECEPTOR_Y_STD).abs() <= 0.01)
    }));
    let first_deco = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("MultitapDeco1_1"))
        .unwrap();
    assert!(compiled.overlay_eases.iter().any(|ease| {
        ease.overlay_index == first_deco
            && ease
                .to
                .effect_color1
                .is_some_and(|color| color[0] > 0.99 && color[1] < 0.6 && color[2] < 0.6)
    }));
    let first_deco_child = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.parent_index == Some(first_deco))
        .unwrap();
    assert!(compiled.overlay_eases.iter().any(|ease| {
        ease.overlay_index == first_deco_child
            && ease
                .to
                .effect_color1
                .is_some_and(|color| color[0] > 0.99 && color[1] < 0.6 && color[2] < 0.6)
    }));
    let explosion_message = "__songlua_multitap_explosion_p1_4";
    assert!(compiled.messages.iter().any(|message| {
        message.message == explosion_message && (message.beat - 40.0).abs() <= 0.01
    }));
    assert!(compiled.overlays.iter().any(|overlay| {
        overlay.message_commands.iter().any(|command| {
            command.message == explosion_message
                && command
                    .blocks
                    .iter()
                    .any(|block| block.delta.visible.is_some() || block.delta.diffuse.is_some())
        })
    }));
}

#[test]
fn compile_song_lua_supports_kenpo_sample_if_present() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let Some(root) = [
        manifest.join("../lua-songs/[11] KENPO SAITO (DX) [Scrypts]"),
        manifest.join("songs/ITL Online 2026/[11] KENPO SAITO (DX) [Scrypts]"),
        manifest.join("songs/lua-songs/[11] KENPO SAITO (DX) [Scrypts]"),
    ]
    .into_iter()
    .find(|root| root.join("template/main.lua").is_file()) else {
        return;
    };
    let entry = root.join("template/main.lua");

    let mut context = SongLuaCompileContext::new(&root, "KENPO SAITO");
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
    assert_eq!(compiled.info.unsupported_function_eases, 0);
    assert!(compiled.hidden_players[0] || compiled.hidden_players[1]);
    assert!(!compiled.player_actors[0].initial_state.visible);
    let proxy_index = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("ProxyOverlay"))
        .expect("KENPO sample should compile the overlay proxy");
    let white_flash_index = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("WhiteFlashSprite"))
        .expect("KENPO sample should compile the white flash quad");
    let black_fade_index = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("BlackFadeSprite"))
        .expect("KENPO sample should compile the black fade quad");
    let pp1_index = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("PP[1]"))
        .expect("KENPO sample should compile the player proxy actor");
    assert!(proxy_index < white_flash_index);
    assert!(white_flash_index < black_fade_index);
    assert_eq!(
        compiled.overlays[white_flash_index].initial_state.size,
        Some([1920.0, 1440.0])
    );
    let has_white_flash_ease =
        |start: f32, limit: f32, from_alpha: f32, to_alpha: f32, easing: &str| {
            compiled.overlay_eases.iter().any(|ease| {
                ease.overlay_index == white_flash_index
                    && (ease.start - start).abs() <= 0.001
                    && (ease.limit - limit).abs() <= 0.001
                    && ease.easing.as_deref() == Some(easing)
                    && ease
                        .from
                        .diffuse
                        .is_some_and(|color| (color[3] - from_alpha).abs() <= 0.001)
                    && ease
                        .to
                        .diffuse
                        .is_some_and(|color| (color[3] - to_alpha).abs() <= 0.001)
            })
        };
    assert!(has_white_flash_ease(27.0, 1.0, 0.0, 1.0, "linear"));
    assert!(has_white_flash_ease(28.0, 0.5, 1.0, 0.0, "outExpo"));
    let bg_quad = compiled
        .overlays
        .iter()
        .find(|overlay| overlay.name.as_deref() == Some("BGQuad"))
        .expect("KENPO sample should compile the black AFT backing quad");
    assert_eq!(bg_quad.initial_state.diffuse, [0.0, 0.0, 0.0, 1.0]);
    assert!(bg_quad.initial_state.size.is_some());
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
            .unwrap_or_else(|| panic!("KENPO sample should compile {name}"));
        assert_eq!(overlay.initial_state.diffuse, diffuse);
        assert_eq!(overlay.initial_state.blend, SongLuaOverlayBlendMode::Add);
        assert_eq!(overlay.initial_state.effect_magnitude, [0.0; 3]);
    }
    assert!(compiled.eases.iter().any(|ease| {
        matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "tiny")
            && (ease.start - 26.5).abs() <= 0.001
            && (ease.limit - 1.5).abs() <= 0.001
            && (ease.to + 200.0).abs() <= 0.001
    }));
    assert!(compiled.eases.iter().any(|ease| {
        matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "flip")
            && (ease.start - 26.5).abs() <= 0.001
            && (ease.limit - 1.5).abs() <= 0.001
            && (ease.to - 50.0).abs() <= 0.001
    }));
    assert!(compiled.eases.iter().any(|ease| {
        matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "dark")
            && (ease.start - 28.0).abs() <= 0.001
            && (ease.limit - 0.1).abs() <= 0.001
            && (ease.to - 100.0).abs() <= 0.001
    }));
    assert!(compiled.eases.iter().any(|ease| {
        matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "skewx")
            && (ease.start - 166.0).abs() <= 0.001
            && (ease.limit - 0.125).abs() <= 0.001
            && (ease.to.abs() - 3.0).abs() <= 0.001
    }));
    assert!(compiled.eases.iter().any(|ease| {
        matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "skewx")
            && (ease.start - 182.0).abs() <= 0.001
            && (ease.limit - 0.125).abs() <= 0.001
            && (ease.to.abs() - 3.0).abs() <= 0.001
    }));
    assert!(compiled.eases.iter().any(|ease| {
        matches!(ease.target, SongLuaEaseTarget::PlayerRotationX)
            && (ease.start - 189.0).abs() <= 0.001
            && (ease.limit - 1.0).abs() <= 0.001
            && (ease.to - 20.0).abs() <= 0.001
    }));
    assert!(compiled.overlay_eases.iter().any(|ease| {
        ease.overlay_index == pp1_index
            && (ease.start - 189.0).abs() <= 0.001
            && (ease.limit - 1.0).abs() <= 0.001
            && ease.to.effect_mode == Some(EffectMode::Wag)
            && ease
                .to
                .effect_magnitude
                .is_some_and(|value| value == [0.0, 20.0, 0.0])
            && ease.to.effect_clock == Some(EffectClock::Beat)
            && ease.to.effect_period.is_some_and(|value| value == 1.0)
    }));
}
