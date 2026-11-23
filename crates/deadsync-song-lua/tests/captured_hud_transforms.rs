#![cfg(feature = "test-support")]

use deadsync_song_lua::*;

type Compiled = CompiledSongLua<
    SongLuaOverlayActor<SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>>,
>;

fn compile(command: &str) -> Compiled {
    let (compiled, _directory) = compile_mode(command, false);
    assert!(compiled.live_scene.is_none());
    compiled
}

fn compile_mode(command: &str, live: bool) -> (Compiled, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("default.lua");
    std::fs::write(
        &path,
        format!(
            r#"
            return Def.ActorFrame {{
                OnCommand=function(self)
                    local player=SCREENMAN:GetTopScreen():GetChild('PlayerP1')
                    local judgment=player:GetChild('Judgment')
                    local combo=player:GetChild('Combo')
                    {command}
                end,
            }}
            "#
        ),
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(directory.path(), "native HUD wrappers");
    context.live = live;
    context.players[1].enabled = false;
    context.music_length_seconds = 1.0;
    context.song_display_bpms = [60.0; 2];
    context.song_timing_bpms = vec![(0.0, 60.0)];
    context.compile_clock_inputs = vec![
        [0.0, 0.0, 0.0],
        [0.25, 0.25, 0.25],
        [0.5, 0.5, 0.25],
        [1.0, 1.0, 0.5],
    ];
    let compiled = compile_song_lua::<(), (), _>(
        &path,
        &context,
        Default::default(),
        |_, _, _| Ok(Default::default()),
        |_| None,
        |_, _| None,
    )
    .unwrap();
    (compiled, directory)
}

fn sample(child: &SongLuaCapturedChildActor, second: f32) -> &[SongLuaOverlayState] {
    let index = child
        .render_frames
        .partition_point(|frame| frame.second <= second);
    &child.render_frames[index.checked_sub(1).expect("captured HUD frame")].wrappers
}

#[test]
fn untouched_dynamic_hud_keeps_theme_placement() {
    let compiled = compile(
        r#"
        self:SetUpdateFunction(function()
            self:x(GAMESTATE:GetSongPosition():GetMusicSeconds()*40)
            judgment:visible(true)
            combo:visible(true)
        end)
        "#,
    );
    for player in &compiled.player_actors {
        assert!(
            player.judgment.render_frames.is_empty(),
            "native Judgment updates must not opt in"
        );
        assert!(
            player.combo.render_frames.is_empty(),
            "native Combo updates must not opt in"
        );
    }
}

#[test]
fn direct_hud_root_setters_opt_in_even_for_default_values() {
    let compiled = compile("judgment:x(0); combo:zoom(1)");
    let judgment = sample(&compiled.player_actors[0].judgment, 0.5);
    let combo = sample(&compiled.player_actors[0].combo, 0.5);
    assert_eq!(
        judgment.len(),
        1,
        "root setter needs no wrapper or update callback"
    );
    assert_eq!(
        combo.len(),
        1,
        "a default-looking scale still has authored provenance"
    );
    assert_eq!([judgment[0].x, judgment[0].y], [0.0, -30.0]);
    assert_eq!(
        [combo[0].y, combo[0].zoom_x, combo[0].zoom_y],
        [30.0, 1.0, 1.0]
    );
}

#[test]
fn static_hud_wrappers_keep_native_child_position_and_tween() {
    let compiled = compile(
        r#"
        judgment:AddWrapperState():x(80):linear(1):y(40)
        judgment:AddWrapperState():zoom(2)
        combo:AddWrapperState():xy(-50,20)
        judgment:basezoomx(1.5):basezoomy(.75)
        "#,
    );
    let states = sample(&compiled.player_actors[0].judgment, 0.5);
    assert_eq!(states.len(), 3, "outermost wrapper, inner wrapper, child");
    assert_eq!(overlay_state_axis_scale(states[0]), [2.0, 2.0]);
    assert_eq!([states[1].x, states[1].y], [80.0, 20.0]);
    // Player::Update writes the default Judgment root; its wrappers and base
    // scales survive those writes (Player.cpp:979-1052, Actor.cpp:425-465).
    assert_eq!(
        [states[2].x, states[2].y, states[2].zoom_x],
        [0.0, -30.0, 1.0]
    );
    assert_eq!([states[2].basezoom_x, states[2].basezoom_y], [1.5, 0.75]);
    let combo = sample(&compiled.player_actors[0].combo, 0.5);
    assert_eq!(combo.len(), 2);
    assert_eq!([combo[0].x, combo[0].y], [-50.0, 20.0]);
    assert_eq!([combo[1].x, combo[1].y], [0.0, 30.0]);
}

#[test]
fn song_callback_hud_pose_is_captured_after_native_player_update() {
    let compiled = compile(
        r#"
        local wrapper=judgment:AddWrapperState():diffuse(.5,.6,.7,.8)
        combo:AddWrapperState():xy(25,-10)
        self:SetUpdateFunction(function()
            wrapper:y(GAMESTATE:GetSongPosition():GetMusicSeconds()*40)
            -- Dyscontrolled Galaxy preserves the native destination while
            -- replacing the scale and restarting its sleep each frame.
            local judgment_y=judgment:GetDestY()
            judgment:stoptweening():zoom(1.25):y(judgment_y):sleep(1)
            judgment:basezoomx(1.5):basezoomy(.75)
            local combo_y=combo:GetDestY()
            combo:stoptweening():zoom(.8):y(combo_y):sleep(1)
        end)
        "#,
    );
    let states = sample(&compiled.player_actors[0].judgment, 0.5);
    assert_eq!(states.len(), 2);
    assert_eq!(states[0].y, 20.0);
    assert_eq!(states[0].diffuse, [0.5, 0.6, 0.7, 0.8]);
    assert_eq!(
        [states[1].y, states[1].zoom_x, states[1].zoom_y],
        [-30.0, 1.25, 1.25]
    );
    assert_eq!([states[1].basezoom_x, states[1].basezoom_y], [1.5, 0.75]);
    let combo = sample(&compiled.player_actors[0].combo, 0.5);
    assert_eq!([combo[0].x, combo[0].y], [25.0, -10.0]);
    assert_eq!(
        [combo[1].y, combo[1].zoom_x, combo[1].zoom_y],
        [30.0, 0.8, 0.8]
    );
}

#[test]
fn live_hud_publication_keeps_current_pose_and_native_axis_scales() {
    let (mut compiled, _directory) = compile_mode(
        r#"
        self:sleep(1000) -- Keep the native foreground active for its callbacks.
        local wrapper=judgment:AddWrapperState()
        combo:AddWrapperState():y(40)
        self:SetUpdateFunction(function()
            wrapper:x(GAMESTATE:GetSongPosition():GetMusicSeconds()*40)
            judgment:basezoom(2):zoom(3):zoomx(1)
        end)
        "#,
        true,
    );
    let bindings = compiled
        .live_actor_indices()
        .iter()
        .copied()
        .enumerate()
        .map(|(local, global)| (global, SongLuaLiveTarget::Root(local)))
        .collect();
    let mut scene = compiled.live_scene.take().expect("live compiler");
    scene.bind_targets(bindings).unwrap();
    let mut visuals: deadsync_gameplay::SongLuaRuntimeVisuals<
        _,
        SongLuaCapturedActor,
        gameplay::SongLuaRuntimeOverlayStateDelta,
    > = deadsync_gameplay::SongLuaRuntimeVisuals {
        overlays: compiled.overlays,
        foreground_beats: None,
        screen_overlay_index: compiled.screen_overlay_index,
        overlay_eases: Vec::new(),
        overlay_ease_ranges: Vec::new(),
        overlay_events: Vec::new(),
        background_visual_layers: Vec::new(),
        foreground_visual_layers: Vec::new(),
        player_actors: compiled.player_actors,
        player_events: Default::default(),
        player_judgment_events: Default::default(),
        player_combo_events: Default::default(),
        song_foreground: compiled.song_foreground,
        song_foreground_events: Vec::new(),
        hidden_players: compiled.hidden_players,
        screen_layers: compiled.screen_layers,
        screen_layer_events: Default::default(),
        note_hides: Default::default(),
        column_offsets: Default::default(),
        column_splines: Default::default(),
        screen_width: compiled.screen_width,
        screen_height: compiled.screen_height,
    };
    let activation = scene.update(SongLuaLiveFrame::default()).unwrap();
    assert!(activation.errors.is_empty(), "{:?}", activation.errors);
    for second in [0.5, 1.0] {
        let output = scene
            .update(SongLuaLiveFrame {
                music_seconds: second,
                screen_seconds: second,
                beat: second,
                delta_seconds: 0.5,
                ..Default::default()
            })
            .unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        scene.write_visuals(&mut visuals).unwrap();
        let judgment = &visuals.player_actors[0].judgment;
        assert_eq!(
            judgment.render_frames.len(),
            1,
            "live publication replaces its frame"
        );
        let states = sample(judgment, second as f32);
        assert_eq!(states[0].x, second as f32 * 40.0);
        assert_eq!(
            [states[1].y, states[1].zoom_x, states[1].zoom_y],
            [-30.0, 1.0, 3.0]
        );
        assert_eq!(
            [states[1].zoom, states[1].basezoom],
            [1.0, 1.0],
            "scalar aliases cannot override an explicit unit native axis"
        );
        assert_eq!([states[1].basezoom_x, states[1].basezoom_y], [2.0, 2.0]);
        let combo = sample(&visuals.player_actors[0].combo, second as f32);
        assert_eq!([combo[0].y, combo[1].y], [40.0, 30.0]);
    }
    scene.end().unwrap();
}
