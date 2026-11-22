use deadsync_song_lua::{self as song, *};
use std::{fs, sync::Arc};

type Compiled = CompiledSongLua<
    SongLuaOverlayActor<SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>>,
>;

fn compile_with_context(
    name: &str,
    source: &str,
    configure: impl FnOnce(&mut SongLuaCompileContext),
) -> Compiled {
    let dir = std::env::temp_dir().join(format!("deadsync-field-{name}-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("default.lua");
    fs::write(&entry, source).unwrap();
    let mut context = SongLuaCompileContext::new(&dir, name);
    context.live = true;
    configure(&mut context);
    let compiled = song::compile_song_lua(
        &entry,
        &context,
        SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None,
        |_, _| None,
    )
    .expect("native field test preparation");
    assert_eq!(
        compiled.info.unsupported_perframes, 0,
        "{name}: {:?}",
        compiled.info.unsupported_perframe_captures
    );
    compiled
}

fn assert_callbacks_succeeded(output: SongLuaLiveOutput) {
    assert!(
        output.errors.is_empty(),
        "callback errors: {:?}",
        output.errors
    );
}

fn scene_exec(
    scene: &SongLuaLiveScene,
    source: impl AsRef<[u8]>,
) -> Result<(), deadlib_lua::Error> {
    let code = deadlib_lua::compile_named(source.as_ref(), b"=native NoteField regression")?;
    scene.runtime().vm_mut().execute(&code)?;
    Ok(())
}

fn scene_global_number(scene: &SongLuaLiveScene, name: &str) -> Result<f64, deadlib_lua::Error> {
    let value = scene.runtime().vm_mut().get_global(name.as_bytes())?;
    let deadlib_lua::Value::Number(number) = value else {
        panic!("numeric probe {name}: {value:?}");
    };
    Ok(number)
}

fn scene_update_only(scene: &SongLuaLiveScene) -> Result<bool, deadlib_lua::Error> {
    let (root, _) = song::test_support::live_scene_actor_handles(scene)?;
    scene.runtime().song_multitap_update_only(root)
}

const FIELD_PROBE: &str = r#"
return Def.ActorFrame {InitCommand=function(self)
  PLAYER=SCREENMAN:GetTopScreen():GetChild('PlayerP1')
  FIELD=PLAYER:GetChild('NoteField')
  CURRENT=GAMESTATE:GetPlayerState(PLAYER_1):GetCurrentPlayerOptions()
  SONG=GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song')
  function Mods(mini,tilt,reverse)
    for _,options in ipairs({CURRENT,SONG}) do
      options:Mini(mini)
      options:Tilt(tilt)
      options:Reverse(reverse or 0)
    end
  end
  INIT_ZOOM=FIELD:GetZoomX()
  self:SetUpdateFunction(function()
    FIRST_ZOOM=FIRST_ZOOM or FIELD:GetZoomX()
    FIRST_ROTATION=FIRST_ROTATION or FIELD:GetRotationX()
  end)
end}
"#;

fn field_context(context: &mut SongLuaCompileContext) {
    context.players[0].mini = 0.6;
    context.players[0].perspective.tilt = 0.5;
    context.players[0].step_statistics_enabled = false;
    context.players[0].hide_lifebar = true;
    context.players[1].enabled = false;
    context.players[1].hide_lifebar = true;
    context.music_length_seconds = 0.1;
}

fn frame(seconds: f64) -> SongLuaLiveFrame {
    SongLuaLiveFrame {
        screen_seconds: seconds,
        delta_seconds: 0.125,
        ..Default::default()
    }
}

fn pose(scene: &SongLuaLiveScene) -> [f32; 4] {
    scene_exec(scene, "POSE_X=FIELD:GetZoomX(); POSE_Y=FIELD:GetZoomY(); POSE_Z=FIELD:GetZoomZ(); POSE_RX=FIELD:GetRotationX()").unwrap();
    ["POSE_X", "POSE_Y", "POSE_Z", "POSE_RX"]
        .map(|name| scene_global_number(scene, name).unwrap() as f32)
}

#[test]
fn native_note_field_startup_keeps_init_separate_from_player_update() {
    let compiled = compile_with_context("native-field-startup", FIELD_PROBE, field_context);
    let scene = compiled.live_scene.unwrap();
    assert_eq!(scene_global_number(&scene, "INIT_ZOOM").unwrap(), 1.0);
    assert_eq!(
        scene_global_number(&scene, "FIRST_ZOOM").unwrap() as f32,
        0.7
    );
    assert_eq!(scene_global_number(&scene, "FIRST_ROTATION").unwrap(), 0.0);
}

#[test]
fn native_note_field_matches_native_update_and_draw_capture() {
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/itgmania-actors/multitap-speed-modifiers.json"
    ))
    .unwrap();
    assert_eq!(reference["provenance"]["execution"], "native_executable");
    assert_eq!(
        reference["provenance"]["native_source"]["modified_src"],
        false
    );
    let mut compiled = compile_with_context("native-field-reference", FIELD_PROBE, field_context);
    let mut scene = compiled.live_scene.take().unwrap();
    // The capture starts after the intro has already drawn the selected profile.
    scene.advance(frame(0.125)).unwrap();
    let mut elapsed = 0.125;
    let mut checked = 0;
    for sample in reference["field_transform_phases"].as_array().unwrap() {
        let point = sample["point"].as_str().unwrap();
        let case = sample["case_name"].as_str().unwrap();
        match point {
            "after-gameplay-actors" => {
                scene_exec(
                    &scene,
                    format!(
                        "Mods({},{},{}); PLAYER:visible({}):hibernate({}); FIELD:visible({})",
                        sample["mini"],
                        sample["tilt"],
                        sample["reverse"],
                        sample["player_visible"],
                        if case == "hibernating-player" {
                            0.5
                        } else {
                            0.0
                        },
                        sample["notefield_visible"],
                    ),
                )
                .unwrap();
                elapsed += 0.125;
                assert!(scene.update(frame(elapsed)).unwrap().errors.is_empty());
            }
            "after-native-screen-draw" => {
                assert!(scene.draw().unwrap().errors.is_empty());
            }
            _ => continue,
        }
        let zoom = sample["notefield_zoom"].as_array().unwrap();
        let expected = [
            zoom[0].as_f64().unwrap() as f32,
            zoom[1].as_f64().unwrap() as f32,
            zoom[2].as_f64().unwrap() as f32,
            sample["notefield_rotation"][0].as_f64().unwrap() as f32,
        ];
        assert_eq!(pose(&scene), expected, "{case}: {point}");
        checked += 1;
    }
    assert_eq!(checked, 14);
}

#[test]
fn native_note_field_publishes_mini_on_update_and_retains_draw_rotation() {
    let mut compiled = compile_with_context("native-field-phases", FIELD_PROBE, |context| {
        field_context(context);
        context.defer_initial_live_update = true;
    });
    let mut scene = compiled.live_scene.take().unwrap();
    assert!(!scene.has_custom_draw().unwrap());
    assert_eq!(pose(&scene), [1.0, 1.0, 1.0, 0.0]);
    assert!(scene.update(frame(0.125)).unwrap().errors.is_empty());
    assert_eq!(pose(&scene), [0.7, 0.7, 0.7, 0.0]);
    assert!(scene.draw().unwrap().errors.is_empty());
    // Values independently captured from the unmodified ITGmania positioner.
    assert_eq!(pose(&scene), [0.664999962, 0.664999962, 0.664999962, -15.0]);

    scene_exec(&scene, "Mods(0.6,-0.5)").unwrap();
    scene.update(frame(0.25)).unwrap();
    assert_eq!(pose(&scene), [0.7, 0.7, 0.7, -15.0]);
    scene.draw().unwrap();
    assert_eq!(pose(&scene), [0.664999962, 0.664999962, 0.664999962, 15.0]);
    scene_exec(&scene, "Mods(0.6,-0.5,1)").unwrap();
    scene.advance(frame(0.375)).unwrap();
    assert_eq!(pose(&scene)[3], -15.0);

    // Destination getters remain authored until the next native phase, and
    // native writes change the queued destination without discarding its timer.
    scene_exec(&scene, "FIELD:linear(1):zoomx(2):rotationx(9)").unwrap();
    assert_eq!(pose(&scene)[0], 2.0);
    assert_eq!(pose(&scene)[3], 9.0);
    scene.update(frame(0.5)).unwrap();
    assert_eq!(pose(&scene), [0.7, 0.7, 0.7, 9.0]);
    scene_exec(&scene, "assert(FIELD:GetTweenTimeLeft()>0.8)").unwrap();
    scene.draw().unwrap();
    assert_eq!(pose(&scene), [0.664999962, 0.664999962, 0.664999962, -15.0]);
    scene_exec(&scene, "assert(FIELD:GetTweenTimeLeft()>0.8); FIELD:finishtweening(); PLAYER:set_oitg_zoom_mode(true); FIELD:zoomz(3):basezoom(2)").unwrap();
    scene.advance(frame(0.625)).unwrap();
    assert_eq!(pose(&scene), [0.664999962, 0.664999962, 3.0, -15.0]);
    scene_exec(&scene, "Mods(0.6,0.37)").unwrap();
    scene.advance(frame(0.75)).unwrap();
    // SCALE's f32 operation order differs from the simplified -30*tilt.
    assert_eq!(pose(&scene)[3], -11.099998474121094_f32);
}

#[test]
fn native_note_field_draw_guards_belong_to_player_not_notefield() {
    let mut compiled = compile_with_context("native-field-guards", FIELD_PROBE, field_context);
    let mut scene = compiled.live_scene.take().unwrap();
    scene.advance(frame(0.125)).unwrap();
    assert_eq!(pose(&scene)[3], -15.0);
    scene_exec(
        &scene,
        "Mods(0.2,0.8); PLAYER:AddWrapperState(); WRAPPER=PLAYER:GetWrapperState(1)",
    )
    .unwrap();
    for (index, guard) in [
        "PLAYER:visible(false)",
        "PLAYER:visible(true):diffusealpha(0)",
        "PLAYER:diffusealpha(1):hibernate(10)",
        "PLAYER:hibernate(0); WRAPPER:visible(false)",
    ]
    .into_iter()
    .enumerate()
    {
        scene_exec(&scene, guard).unwrap();
        scene.update(frame(0.25 + index as f64 * 0.125)).unwrap();
        assert_eq!(pose(&scene), [0.9, 0.9, 0.9, -15.0], "{guard}");
        scene.draw().unwrap();
        assert_eq!(pose(&scene), [0.9, 0.9, 0.9, -15.0], "{guard}");
    }
    scene_exec(&scene, "WRAPPER:visible(true); FIELD:visible(false):hibernate(10); FIELD:AddWrapperState(); FIELD:GetWrapperState(1):visible(false)").unwrap();
    scene.advance(frame(0.875)).unwrap();
    assert_eq!(pose(&scene), [0.827999949, 0.827999949, 0.827999949, -24.0]);
}

#[test]
fn native_note_field_player_proxies_publish_only_when_submitted() {
    for custom in [false, true] {
        let source = format!(
            r#"
return Def.ActorFrame {{InitCommand=function(self)
  PLAYER=SCREENMAN:GetTopScreen():GetChild('PlayerP1')
  FIELD=PLAYER:GetChild('NoteField')
  PLAYER:visible(false)
  if {custom} then self:SetDrawFunction(function() PROXY:Draw() end) end
end,
  Def.ActorProxy {{Name='Proxy',OnCommand=function(self)
    PROXY=self
    self:SetTarget(PLAYER)
  end}}
}}
"#
        );
        let mut compiled = compile_with_context(
            &format!("native-field-proxy-{custom}"),
            &source,
            |context| {
                field_context(context);
                context.defer_initial_live_update = true;
            },
        );
        let mut scene = compiled.live_scene.take().unwrap();
        scene
            .bind_targets(
                compiled
                    .live_actor_indices()
                    .iter()
                    .copied()
                    .enumerate()
                    .map(|(local, global)| (global, SongLuaLiveTarget::Root(local)))
                    .collect(),
            )
            .unwrap();
        assert_eq!(scene.has_custom_draw().unwrap(), custom);
        let output = scene.advance(frame(0.125)).unwrap();
        if custom {
            assert_eq!(
                output
                    .draw_frames
                    .iter()
                    .flat_map(|frame| &frame.ops)
                    .filter(|op| matches!(op, SongLuaDrawOp::Draw { .. }))
                    .count(),
                1
            );
        } else {
            assert!(
                output.draw_frames.is_empty(),
                "ordinary proxy publication is not a custom draw frame"
            );
        }
        assert_eq!(pose(&scene), [0.664999962, 0.664999962, 0.664999962, -15.0]);
        scene_exec(
            &scene,
            "assert(not PLAYER:GetVisible()); PROXY:visible(false); FIELD:rotationx(9)",
        )
        .unwrap();
        scene.advance(frame(0.25)).unwrap();
        assert_eq!(pose(&scene), [0.7, 0.7, 0.7, 9.0]);
        scene_exec(&scene, "PROXY:visible(true); PLAYER:AddWrapperState(); PW=PLAYER:GetWrapperState(1); PW:visible(false)").unwrap();
        scene.advance(frame(0.375)).unwrap();
        assert_eq!(pose(&scene)[3], 9.0);
        scene_exec(&scene, "PW:visible(true); PLAYER:hibernate(10)").unwrap();
        scene.advance(frame(0.5)).unwrap();
        assert_eq!(pose(&scene)[3], 9.0);
        scene_exec(&scene, "PLAYER:hibernate(0):diffusealpha(0)").unwrap();
        scene.advance(frame(0.625)).unwrap();
        assert_eq!(pose(&scene)[3], 9.0);
        scene_exec(&scene, "PLAYER:diffusealpha(1); FIELD:visible(false)").unwrap();
        scene.advance(frame(0.75)).unwrap();
        assert_eq!(pose(&scene)[3], -15.0);
        scene_exec(&scene, "PROXY:SetTarget(FIELD); FIELD:rotationx(9)").unwrap();
        scene.advance(frame(0.875)).unwrap();
        assert_eq!(pose(&scene), [0.7, 0.7, 0.7, 9.0]);
    }
}

#[test]
fn native_note_field_preparation_proxies_respect_opacity() {
    for custom in [false, true] {
        for (alpha, ancestor_alpha, wrapper) in
            [(0, 1, false), (1, 1, false), (1, 0, false), (1, 0, true)]
        {
            let source = format!(
                r#"
return Def.ActorFrame {{InitCommand=function(self)
  PLAYER=SCREENMAN:GetTopScreen():GetChild('PlayerP1')
  FIELD=PLAYER:GetChild('NoteField')
  PLAYER:visible(false)
  FIELD:rotationx(9)
  self:diffusealpha({ancestor_alpha})
  if {custom} then self:SetDrawFunction(function() PROXY:Draw() end) end
end,
  Def.ActorProxy {{Name='Proxy',OnCommand=function(self)
    PROXY=self
    self:SetTarget(PLAYER):diffusealpha({alpha})
    if {wrapper} then self:AddWrapperState() end
  end}},
  Def.ActorFrame {{Name='Observed',InitCommand=function(self)
    self:SetUpdateFunction(function(self) self:x(FIELD:GetRotationX()) end)
  end, Def.Quad {{}}}}
}}
"#
            );
            let compiled = compile_with_context(
                &format!(
                    "native-field-proxy-preparation-{custom}-{alpha}-{ancestor_alpha}-{wrapper}"
                ),
                &source,
                |context| {
                    field_context(context);
                    context.live = false;
                },
            );
            let observer = compiled
                .overlays
                .iter()
                .position(|actor| actor.name.as_deref() == Some("Observed"))
                .unwrap();
            let saw_draw_rotation = compiled.overlay_updates.iter().any(|track| {
                track.overlay_index == observer && track.target == SongLuaOverlayUpdateTarget::X
                    && track.samples.iter().any(|sample| matches!(sample.value, SongLuaOverlayUpdateValue::F32(value) if value == -15.0))
            });
            assert_eq!(
                saw_draw_rotation,
                alpha != 0 && ancestor_alpha != 0,
                "custom={custom}, alpha={alpha}, ancestor_alpha={ancestor_alpha}, wrapper={wrapper}"
            );
        }
    }
}

#[test]
fn native_note_field_top_screen_wrappers_block_proxy_publication() {
    for live in [false, true] {
        for (index, guard) in ["visible(false)", "diffusealpha(0)", "hibernate(10)"]
            .into_iter()
            .enumerate()
        {
            let source = format!(
                r#"
return Def.ActorFrame {{InitCommand=function(self)
  local top=SCREENMAN:GetTopScreen()
  top:AddWrapperState()
  TOP_WRAPPER=top:GetWrapperState(1)
  TOP_WRAPPER:{guard}
  PLAYER=top:GetChild('PlayerP1')
  FIELD=PLAYER:GetChild('NoteField')
  PLAYER:visible(false)
  FIELD:rotationx(9)
  self:SetDrawFunction(function() PROXY:Draw() end)
end,
  Def.ActorProxy {{Name='Proxy',OnCommand=function(self)
    PROXY=self
    self:SetTarget(PLAYER)
  end}},
  Def.ActorFrame {{Name='Observed',InitCommand=function(self)
    self:SetUpdateFunction(function(self) self:x(FIELD:GetRotationX()) end)
  end, Def.Quad {{}}}}
}}
"#
            );
            let mut compiled = compile_with_context(
                &format!("native-field-screen-wrapper-{live}-{index}"),
                &source,
                |context| {
                    field_context(context);
                    context.live = live;
                    context.defer_initial_live_update = live;
                },
            );
            if live {
                let mut scene = compiled.live_scene.take().unwrap();
                assert_callbacks_succeeded(scene.take_startup_output());
                assert_callbacks_succeeded(scene.advance(frame(0.125)).unwrap());
                assert_eq!(pose(&scene)[3], 9.0, "{guard}");
                scene_exec(
                    &scene,
                    "TOP_WRAPPER:visible(true):diffusealpha(1):hibernate(0)",
                )
                .unwrap();
                assert_callbacks_succeeded(scene.advance(frame(0.25)).unwrap());
                assert_eq!(pose(&scene)[3], -15.0, "released {guard}");
            } else {
                let observer = compiled
                    .overlays
                    .iter()
                    .position(|actor| actor.name.as_deref() == Some("Observed"))
                    .unwrap();
                assert!(!compiled.overlay_updates.iter().any(|track| {
                    track.overlay_index == observer && track.target == SongLuaOverlayUpdateTarget::X
                        && track.samples.iter().any(|sample| matches!(sample.value, SongLuaOverlayUpdateValue::F32(value) if value == -15.0))
                }), "top-screen wrapper must abort native preparation draw: {guard}");
            }
        }
    }
}

#[test]
fn native_note_field_draw_is_between_background_and_foreground() {
    let dir = tempfile::tempdir().unwrap();
    let paths = [
        dir.path().join("background.lua"),
        dir.path().join("foreground.lua"),
    ];
    for (path, label) in paths.iter().zip(["BG", "FG"]) {
        fs::write(
            path,
            format!(
                r#"
return Def.ActorFrame {{InitCommand=function(self)
  FIELD=SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField')
  self:SetUpdateFunction(function()
    {label}_U={{FIELD:GetZoomX(),FIELD:GetRotationX()}}
    {label}_FIRST={label}_FIRST or {label}_U
  end)
  self:SetDrawFunction(function()
    {label}_D={{FIELD:GetZoomX(),FIELD:GetRotationX()}}
  end)
end}}
"#
            ),
        )
        .unwrap();
    }
    let mut context = SongLuaCompileContext::new(dir.path(), "native field layer phases");
    field_context(&mut context);
    context.live = true;
    context.background_layer_count = 1;
    let mut layers = song::compile_song_lua_layers(
        &[&paths[0], &paths[1]],
        1,
        &context,
        SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None::<SongLuaOverlayModelLayer<()>>,
        |_, _| None,
    )
    .unwrap();
    let mut scene = layers[1].live_scene.take().unwrap();
    scene_exec(&scene, "assert(BG_FIRST[1]==1 and BG_FIRST[2]==0); assert(math.abs(FG_FIRST[1]-0.7)<1e-6 and FG_FIRST[2]==0)").unwrap();
    scene.advance(frame(0.125)).unwrap();
    scene_exec(&scene, "assert(math.abs(BG_D[1]-0.7)<1e-6 and BG_D[2]==0); assert(math.abs(FG_D[1]-0.665)<1e-6 and FG_D[2]==-15)").unwrap();
    scene_exec(&scene, "local ps=GAMESTATE:GetPlayerState(PLAYER_1); ps:GetCurrentPlayerOptions():Tilt(-0.5); ps:GetPlayerOptions('ModsLevel_Song'):Tilt(-0.5)").unwrap();
    scene.advance(frame(0.25)).unwrap();
    scene_exec(&scene, "assert(math.abs(BG_U[1]-0.665)<1e-6 and BG_U[2]==-15); assert(math.abs(FG_U[1]-0.7)<1e-6 and FG_U[2]==-15); assert(BG_D[2]==-15 and FG_D[2]==15)").unwrap();
}

#[test]
fn native_note_field_mod_pose_is_not_exported_as_an_ordinary_field_wrapper() {
    let compiled = compile_with_context(
        "native-field-wrappers",
        r#"
return Def.ActorFrame {OnCommand=function(self)
  local nf=SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField')
  nf:AddWrapperState()
  nf:GetWrapperState(1):zoom(0.8):rotationz(17)
  self:SetUpdateFunction(function() end)
end}
"#,
        |context| {
            field_context(context);
            context.live = false;
        },
    );
    let frames = &compiled.player_actors[0].note_field_frames;
    assert!(!frames.is_empty());
    for frame in frames {
        assert_eq!(frame.wrappers.len(), 1);
        let wrapper = frame.wrappers[0];
        assert_eq!([wrapper.zoom_x, wrapper.zoom_y, wrapper.zoom_z], [0.8; 3]);
        assert_eq!(wrapper.rot_x_deg, 0.0);
        assert_eq!(wrapper.rot_z_deg, 17.0);
    }
    assert_eq!(compiled.player_actors[0].initial_state.zoom_x, 1.0);
    assert_eq!(compiled.player_actors[0].initial_state.rot_x_deg, 0.0);
}

#[test]
fn native_note_field_analytic_eligibility_requires_neutral_current_and_song_mods() {
    let mut compiled = compile_with_context(
        "native-field-analytic-eligibility",
        r#"
return Def.ActorFrame {
  Def.ActorFrame {Name='Update',InitCommand=function(self)
    UPDATE=self
    self:SetUpdateFunction(function() end)
  end, Def.Quad {}}
}
"#,
        |context| {
            context.defer_initial_live_update = true;
            context.music_length_seconds = 0.1;
            context.players[1].enabled = false;
            for player in &mut context.players {
                player.mini = 0.0;
                player.perspective.tilt = 0.0;
                player.perspective.skew = 0.0;
            }
        },
    );
    let mut scene = compiled.live_scene.take().unwrap();
    assert_callbacks_succeeded(scene.take_startup_output());
    // Match song_multitap.rs's isolated-plan tests. Theme transitions and native
    // recurring jobs independently prevent the full screen from being replaced.
    // Keep both players neutral because this plan has no attached-player filter.
    scene_exec(
        &scene,
        r#"
local ps=GAMESTATE:GetPlayerState(PLAYER_1)
CURRENT=ps:GetCurrentPlayerOptions()
SONG=ps:GetPlayerOptions('ModsLevel_Song')
__songlua_top_screen=nil
__songlua_top_screen_player_1=nil
__songlua_top_screen_player_2=nil
-- Rebuild the cached plan after isolating it.
UPDATE:SetUpdateFunction(function() end)
"#,
    )
    .unwrap();
    assert!(
        scene_update_only(&scene).unwrap(),
        "neutral single-Update plan"
    );
    for level in ["CURRENT", "SONG"] {
        for (modifier, value) in [
            ("Mini", 0.6),
            ("Tilt", 0.5),
            ("Skew", 0.3),
            ("Reverse", 1.0),
            ("Centered", 1.0),
        ] {
            scene_exec(&scene, format!("{level}:{modifier}({value})")).unwrap();
            assert!(
                !scene_update_only(&scene).unwrap(),
                "{level}:{modifier}({value}) requires chronological replay"
            );
            scene_exec(&scene, format!("{level}:{modifier}(0)")).unwrap();
            assert!(
                scene_update_only(&scene).unwrap(),
                "resetting {level}:{modifier} restores neutral eligibility"
            );
        }
    }
}

fn factory(name: &str, live: bool, mini: f32, tilt: f32) -> Compiled {
    compile_with_context(
        name,
        r#"
-- The authored arrow uses Sprite texturetranslate. Keep that native actor type
-- while retaining the factory, field getters, and ArrowEffects calculations.
NOTESKIN.LoadActorForNoteSkin=function() return Def.Sprite{} end
local factory=LoadActor('multitap/Default.lua')
factory.InitCommand=function(self) FACTORY=self end
return factory
"#,
        |context| {
            context.live = live;
            context.defer_initial_live_update = live;
            context.music_length_seconds = 4.1;
            context.players[0].difficulty = SongLuaDifficulty::Challenge;
            context.players[0].mini = mini;
            context.players[0].perspective.tilt = tilt;
            context.players[0].speedmod = SongLuaSpeedMod::X(1.0);
            context.players[0].step_statistics_enabled = false;
            context.players[1].enabled = false;
            context.players[1].mini = 0.9;
            context.players[1].perspective.tilt = -0.8;
            let dir = context.song_dir.join("multitap");
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                dir.join("Default.lua"),
                include_str!("../../../tests/fixtures/song-lua/multitap-factory.lua"),
            )
            .unwrap();
            fs::write(
                dir.join("multitap_data.lua"),
                // The factory allocates a zoom spline for every native column.
                // Unused lanes still need descriptors to establish their size.
                "multitaps={_version={1,2},Challenge={{lane=1,taps={4,8},peak=6},{lane=2,taps={}},{lane=3,taps={}},{lane=4,taps={}}}}",
            )
            .unwrap();
        },
    )
}

#[test]
fn native_note_field_actual_factory_copies_mods_and_preserves_receptor_position() {
    let mut compiled = factory("native-field-actual-factory", true, 0.0, 0.0);
    let mut scene = compiled.live_scene.take().unwrap();
    assert_callbacks_succeeded(scene.take_startup_output());
    assert_callbacks_succeeded(scene.advance(SongLuaLiveFrame::default()).unwrap());
    scene_exec(
        &scene,
        r#"
function find(actor,name)
  if actor:GetName()==name then return actor end
  local ok,children=pcall(actor.GetChildren,actor)
  if not ok then return end
  for _,child in pairs(children) do
    local result=find(child,name)
    if result then return result end
  end
end
FIELD=SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField')
COPIED=find(FACTORY,'MultitapFrameP1')
TAP=find(FACTORY,'MultitapP1_1')
EXPLOSION=find(FACTORY,'MultitapExplosionP1_1')
local ps=GAMESTATE:GetPlayerState(PLAYER_1)
CURRENT=ps:GetCurrentPlayerOptions()
SONG=ps:GetPlayerOptions('ModsLevel_Song')
function Mods(mini,tilt,reverse,centered)
  for _,options in ipairs({CURRENT,SONG}) do
    options:Mini(mini)
    options:Tilt(tilt)
    options:Reverse(reverse)
    options:Centered(centered)
  end
end
"#,
    )
    .unwrap();
    // Test zero travel at the first hit: Mini scales the actor and compensates
    // the receptor's local Y, so the receptor row remains at -135.
    let hit_frame = |seconds| SongLuaLiveFrame {
        beat: 4.0,
        music_seconds: 4.0,
        screen_seconds: seconds,
        delta_seconds: 0.125,
        ..Default::default()
    };
    scene_exec(&scene, "Mods(0.6,0,0,0)").unwrap();
    assert_callbacks_succeeded(scene.update(hit_frame(0.125)).unwrap());
    scene_exec(&scene, "assert(math.abs(COPIED:GetZoomX()-0.7)<1e-6); assert(math.abs(TAP:GetY()*COPIED:GetZoomY()+135)<1e-4); assert(math.abs(EXPLOSION:GetY()*COPIED:GetZoomY()+135)<1e-4)").unwrap();

    scene_exec(&scene, "Mods(0.6,0.5,0,0)").unwrap();
    assert_callbacks_succeeded(scene.advance(hit_frame(0.25)).unwrap());
    assert_callbacks_succeeded(scene.update(hit_frame(0.375)).unwrap());
    scene_exec(&scene, "assert(math.abs(COPIED:GetZoomX()-0.665)<1e-6); assert(COPIED:GetRotationX()==-15); assert(math.abs(COPIED:GetY()-(FIELD:GetY()-22.5))<1e-6)").unwrap();
    scene_exec(&scene, "Mods(0.6,-0.5,1,0)").unwrap();
    assert_callbacks_succeeded(scene.advance(hit_frame(0.5)).unwrap());
    assert_callbacks_succeeded(scene.update(hit_frame(0.625)).unwrap());
    scene_exec(&scene, "assert(COPIED:GetRotationX()==-15); assert(math.abs(COPIED:GetY()-(FIELD:GetY()+10))<1e-6)").unwrap();

    scene_exec(&scene, "Mods(0,0,0,1)").unwrap();
    assert_callbacks_succeeded(scene.update(hit_frame(0.75)).unwrap());
    scene_exec(&scene, "assert(math.abs(EXPLOSION:GetY())<1e-5)").unwrap();
}

#[test]
fn native_note_field_compiled_factory_replays_non_neutral_fields() {
    let compiled = factory("native-field-compiled-factory", false, 0.6, 0.5);
    let index = compiled
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("MultitapFrameP1"))
        .expect("copied field actor");
    assert!(compiled.overlay_updates.iter().any(|track| {
        track.overlay_index == index
            && track.target == SongLuaOverlayUpdateTarget::ZoomX
            && track.samples.iter().any(|sample| matches!(sample.value, SongLuaOverlayUpdateValue::F32(value) if (value - 0.665).abs() < 1e-6))
    }), "compiled factory must retain Mini and tilt in its chronological field scale: {:?}",
        compiled.overlay_updates.iter().filter(|track| track.overlay_index == index).collect::<Vec<_>>());
    assert!(compiled.overlay_updates.iter().any(|track| {
        track.overlay_index == index
            && track.target == SongLuaOverlayUpdateTarget::RotationX
            && track.samples.iter().any(|sample| matches!(sample.value, SongLuaOverlayUpdateValue::F32(value) if value == -15.0))
    }), "compiled factory must copy the rotation published after its first update");
}
