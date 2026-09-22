use super::*;
use deadlib_render_core::BackendType;
use deadsync_assets::noteskin::SpriteSlot;
use deadsync_assets::song_lua::{
    CompiledSongLua, SongLuaOverlayActor, SongLuaOverlayKind, compile_song_lua,
    compile_song_lua_layers,
};
use deadsync_chart::ChartData;
use deadsync_gameplay::{GameplayConfig, GameplaySession, GameplayViewport};
use deadsync_profile as profile_data;
use deadsync_rules::{scroll::ScrollSpeedSetting, timing::TimingData};
type GameplayCoreState =
    super::GameplayCoreState<deadsync_profile_gameplay::GameplayProfile, SpriteSlot>;

use deadlib_present::actors::{ActorResourceArena, RetainedActorFrame};
use deadlib_present::compose::ComposeScratch;
use deadlib_present::space::current_window_px;

use deadlib_present::actors::TextAttribute;

use deadlib_present::compose::{
    NullTextureContext,
    build_screen_segments_cached_with_scratch_and_texture_context_and_actor_resources,
};

use deadlib_present::dsl::TextBuilder;

use deadlib_render_core::frame_compare::compare_render_frames_semantic;

use deadsync_song_lua::SongLuaOverlayStateDelta;

#[test]
fn song_lua_tap_glow_clock_survives_repeated_hits_and_music_rate() {
    let initial = SongLuaOverlayState::default();
    let overlays = vec![SongLuaOverlayActor {
        kind: SongLuaOverlayKind::Quad,
        name: None,
        parent_index: None,
        initial_state: initial,
        message_commands: vec![SongLuaOverlayMessageCommand {
            message: "__songlua_tap_1_3_W1".into(),
            aux: None,
            blocks: vec![SongLuaOverlayCommandBlock {
                start: 0.0,
                duration: 0.0,
                easing: None,
                opt1: None,
                opt2: None,
                delta: SongLuaOverlayStateDelta {
                    effect_mode: Some(deadlib_present::anim::EffectMode::GlowShift),
                    effect_period: Some(0.05),
                    effect_color1: Some([1.0, 1.0, 1.0, 0.0]),
                    effect_color2: Some([1.0, 1.0, 1.0, 0.5]),
                    ..Default::default()
                },
            }],
        }],
    }];
    let mut order = song_lua_overlay_order_cache_from(&overlays, &[]);
    let mut feedback = deadsync_gameplay::GameplayVisualFeedbackState::default();
    for (at, now, music) in [(10.037, 10.0495, 8.0), (10.416, 10.4495, 8.6)] {
        feedback.last_tap_judgments[2] = Some(deadsync_gameplay::ColumnTapJudgment {
            grade: JudgeGrade::Fantastic,
            blue_fantastic: false,
            at_screen_s: at,
        });
        let mut local = vec![initial];
        let mut composed = vec![initial];
        apply_song_lua_taps(
            &feedback,
            4,
            now,
            music,
            &overlays,
            &mut order,
            &mut local,
            &mut composed,
            [854.0, 480.0],
        );
        let effect = song_lua_proxy_effect(composed[0], music, 0.0, 0);
        assert!(
            (effect.glow[3] - 0.25).abs() < 0.00005,
            "same timer phase across hits and playback rates: {:?}",
            effect.glow
        );
    }
}

#[test]
fn song_lua_tap_commands_follow_player_grade_and_judgment_time() {
    let initial = SongLuaOverlayState {
        diffuse: [1.0, 1.0, 1.0, 0.0],
        ..Default::default()
    };
    let overlays = vec![SongLuaOverlayActor {
        kind: SongLuaOverlayKind::Quad,
        name: None,
        parent_index: None,
        initial_state: initial,
        message_commands: vec![SongLuaOverlayMessageCommand {
            message: "__songlua_tap_2_1_W1".into(),
            aux: None,
            blocks: [(0.0, 1.0), (0.5, 0.0)]
                .into_iter()
                .map(|(duration, alpha)| SongLuaOverlayCommandBlock {
                    start: 0.0,
                    duration,
                    easing: Some("linear".into()),
                    opt1: None,
                    opt2: None,
                    delta: SongLuaOverlayStateDelta {
                        diffuse: Some([1.0, 1.0, 1.0, alpha]),
                        ..Default::default()
                    },
                })
                .collect(),
        }],
    }];
    let mut order = song_lua_overlay_order_cache_from(&overlays, &[]);
    let mut feedback = deadsync_gameplay::GameplayVisualFeedbackState::default();
    for (column, grade, at, now, alpha) in [
        (0, JudgeGrade::Fantastic, 1.0, 1.0, 0.0),
        (4, JudgeGrade::Excellent, 1.0, 1.0, 0.0),
        (4, JudgeGrade::Fantastic, 1.0, 0.9, 0.0),
        (4, JudgeGrade::Fantastic, 1.0, 1.0, 1.0),
        (4, JudgeGrade::Fantastic, 1.0, 1.25, 0.5),
        (4, JudgeGrade::Fantastic, 1.0, 1.5, 0.0),
        (4, JudgeGrade::Fantastic, 1.3, 1.3, 1.0),
    ] {
        feedback.last_tap_judgments.fill(None);
        feedback.last_tap_judgments[column] = Some(deadsync_gameplay::ColumnTapJudgment {
            grade,
            blue_fantastic: false,
            at_screen_s: at,
        });
        let mut local = vec![initial];
        let mut composed = vec![initial];
        apply_song_lua_taps(
            &feedback,
            4,
            now,
            now,
            &overlays,
            &mut order,
            &mut local,
            &mut composed,
            [854.0, 480.0],
        );
        assert_eq!(local[0].diffuse[3], alpha);
        assert_eq!(composed[0].diffuse[3], alpha);
    }
}

#[test]
#[ignore = "requires the sibling lua-songs corpus"]
fn cuphead_cagney_stays_offscreen_during_cala_phase() {
    let corpus = std::fs::canonicalize(workspace_root())
        .expect("deadsync workspace should resolve")
        .parent()
        .expect("deadsync should have a workspace parent")
        .join("lua-songs/Cuphead [TaroNuke]");
    let entries = [
        corpus.join("bg/default.lua"),
        corpus.join("lua/default.lua"),
    ];
    let entry_paths = entries
        .iter()
        .map(std::path::PathBuf::as_path)
        .collect::<Vec<_>>();
    let simfile = std::fs::read_to_string(corpus.join("botanic.sm"))
        .expect("Cuphead simfile should be readable");
    let bpms = simfile
        .split_once("#BPMS:")
        .and_then(|(_, tail)| tail.split_once(';'))
        .map(|(bpms, _)| bpms)
        .expect("Cuphead simfile should contain BPMS");
    let mut context =
        deadsync_assets::song_lua::SongLuaCompileContext::new(&corpus, "Botanic Panic".to_owned());
    context.song_timing_bpms = deadsync_assets::song_lua::parse_song_timing_bpms(bpms);
    context.music_length_seconds = 140.0;
    context.screen_width = 854.0;
    context.screen_height = 480.0;
    context.players = std::array::from_fn(|_| deadsync_assets::song_lua::SongLuaPlayerContext {
        enabled: true,
        ..Default::default()
    });
    let compiled_layers = compile_song_lua_layers(&entry_paths, 1, &context)
        .expect("Cuphead background and foreground lua should compile together");
    let compiled = &compiled_layers[1];
    let idle = compiled
        .overlays
        .iter()
        .position(|actor| {
            matches!(
                &actor.kind,
                SongLuaOverlayKind::Sprite { texture_path, .. }
                    if texture_path.to_string_lossy().replace('\\', "/").contains("/cagney/idle")
            )
        })
        .expect("Cuphead should contain Cagney's idle sprite");
    let SongLuaOverlayKind::Sprite {
        texture_key: idle_texture_key,
        ..
    } = &compiled.overlays[idle].kind
    else {
        unreachable!("Cagney idle was selected from sprite overlays");
    };
    let idle_texture_key = Arc::clone(idle_texture_key);
    let seconds = compiled
        .messages
        .iter()
        .map(|message| Some(message.beat))
        .collect::<Vec<_>>();
    let events = deadsync_song_lua::gameplay::build_song_lua_overlay_message_events_with_seconds(
        &compiled, &seconds,
    );
    let mut overlays = compiled.overlays.clone();
    let mut tracks = compiled
        .overlay_updates
        .iter()
        .map(
            |track| deadsync_song_lua::SongLuaOverlayRuntimeUpdateTrack {
                overlay_index: track.overlay_index,
                target: track.target,
                samples: track
                    .samples
                    .iter()
                    .map(
                        |sample| deadsync_song_lua::SongLuaOverlayRuntimeUpdateSample {
                            second: sample.beat,
                            value: sample.value.clone(),
                        },
                    )
                    .collect(),
            },
        )
        .collect::<Vec<_>>();
    tracks.sort_by_key(|track| track.overlay_index);
    overlays.push(SongLuaOverlayActor {
        kind: SongLuaOverlayKind::UpdateTracks { tracks },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    });
    let ranges = vec![0..0; overlays.len()];
    let mut order_cache = song_lua_overlay_order_cache_from(&overlays, &[]);
    let mut caches = Vec::new();
    let mut local = Vec::new();
    let mut composed = Vec::new();
    const CALA_PHASE_BEAT: u32 = 180;
    for beat in 0..=CALA_PHASE_BEAT {
        song_lua_overlay_state_sets_from_into::<SpriteSlot>(
            beat as f32,
            &overlays,
            &events,
            &[],
            &ranges,
            854.0,
            480.0,
            &mut order_cache,
            &mut caches,
            &mut local,
            &mut composed,
        );
    }

    assert!(
        local[idle].visible,
        "ITG keeps the idle sprite locally visible"
    );
    let idle_parent = overlays[idle]
        .parent_index
        .expect("Cagney idle should retain its ActorFrame parent");
    assert!(
        (local[idle].x - 587.0).abs() <= 0.01,
        "Cagney's sprite-local position changed during Cala's phase"
    );
    assert!(
        (local[idle_parent].x - 427.0).abs() <= 0.01,
        "Cagney's parent must hold its completed beat-111 exit tween"
    );
    assert!(
        (composed[idle].x - 1_014.0).abs() <= 0.01,
        "Cagney's beat-111 parent move must persist through Cala's phase"
    );
    let mut asset_manager = AssetManager::new();
    asset_manager.queue_texture_upload(
        idle_texture_key.to_string(),
        image::RgbaImage::new(1_152, 1_056),
    );
    assert_eq!(
        song_lua_overlay_sprite_size(
            composed[idle],
            asset_manager
                .texture_context()
                .bind_texture(&idle_texture_key)
                .unwrap()
        ),
        Some([288.0, 352.0]),
        "Cagney culling must use one 4x3 sheet cell, not the whole texture"
    );
    let topology = SongLuaOverlayTopologyIndex::new(&overlays);
    let camera_state = topology.camera_state(&composed, idle);
    assert!(camera_state.is_none(), "Cagney must remain in screen space");
    assert!(
        build_song_lua_overlay_actor(
            &overlays[idle],
            composed[idle],
            camera_state,
            &asset_manager,
            0,
            854.0,
            480.0,
            CALA_PHASE_BEAT as f32,
            CALA_PHASE_BEAT as f32,
            CALA_PHASE_BEAT as f32,
        )
        .is_none(),
        "Cagney leaked into Cala's phase despite being fully offscreen"
    );
}

fn workspace_root() -> std::path::PathBuf {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if manifest.join("assets").is_dir() {
        manifest
    } else {
        manifest.join("../..")
    }
}

fn empty_text_attributes() -> Arc<[TextAttribute]> {
    Arc::from([])
}

use deadlib_present::actors::{SizeSpec, TextAlign};

#[test]
fn aft_sprite_reflects_song_lua_in_plane_transform_into_world_space() {
    let state = SongLuaOverlayState {
        x: 0.5 * screen_width(),
        y: 0.5 * screen_height(),
        rot_z_deg: 30.0,
        skew_x: 0.375,
        skew_y: -0.25,
        ..SongLuaOverlayState::default()
    };
    let actors = build_song_lua_aft_sprite_actor(
        state,
        render_target_texture_handle(17),
        [screen_width(), screen_height()],
        0,
        screen_width(),
        screen_height(),
        0.0,
        0.0,
        0.0,
        None,
    )
    .expect("visible AFT sprite should render");

    assert!(matches!(
        actors.first(),
        Some(Actor::Sprite {
            rot_z_deg: -30.0,
            skew: [-0.375, 0.25],
            ..
        })
    ));
}

#[test]
fn song_lua_screen_layers_hide_matching_native_hud() {
    assert_eq!(
        hidden_gameplay_hud_layers(false, [false, false], [false; 2]),
        [false, false]
    );
    assert_eq!(
        hidden_gameplay_hud_layers(false, [true, false], [false; 2]),
        [true, false]
    );
    assert_eq!(
        hidden_gameplay_hud_layers(false, [false, true], [false; 2]),
        [false, true]
    );
    assert_eq!(
        hidden_gameplay_hud_layers(true, [false, false], [false; 2]),
        [true, true]
    );
    assert_eq!(
        hidden_gameplay_hud_layers(false, [true, true], [true, true]),
        [false, false],
        "hidden targets must still populate the Underlay/Overlay proxies"
    );
    assert_eq!(
        hidden_gameplay_hud_layers(false, [true, true], [true, false]),
        [false, true]
    );
    assert_eq!(
        hidden_gameplay_hud_layers(true, [true, true], [true, true]),
        [true, true],
        "the explicit HUD override still applies"
    );
}

#[test]
fn song_lua_hidden_judgment_and_combo_remain_capturable() {
    let child = SongLuaCapturedChildActor {
        initial_state: SongLuaOverlayState {
            visible: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut cache = SongLuaMessageStateCache::default();
    for captured in [false, true, false, true] {
        assert_eq!(
            song_lua_child_visible(18.0, &child, &[], &mut cache, captured),
            captured
        );
        assert!(
            !child.initial_state.visible,
            "proxy capture must not unhide the original"
        );
    }
}

fn test_sprite_kind(key: &str) -> SongLuaOverlayKind {
    SongLuaOverlayKind::Sprite {
        texture_path: std::path::PathBuf::from(key),
        texture_key: Arc::from(key),
        states: Arc::from([]),
    }
}

fn test_sprite_path_kind(path: std::path::PathBuf) -> SongLuaOverlayKind {
    let texture_key = Arc::from(path.to_string_lossy().into_owned());
    SongLuaOverlayKind::Sprite {
        texture_path: path,
        texture_key,
        states: Arc::from([]),
    }
}

fn test_message_command(delta: SongLuaOverlayStateDelta) -> SongLuaOverlayMessageCommand {
    SongLuaOverlayMessageCommand {
        message: String::new(),
        aux: None,
        blocks: vec![SongLuaOverlayCommandBlock {
            start: 0.0,
            duration: 0.75,
            easing: Some("inOutQuad".to_string()),
            opt1: None,
            opt2: None,
            delta,
        }],
    }
}

#[test]
fn song_lua_message_state_cache_matches_replay_across_advances_and_seeks() {
    let commands = vec![
        test_message_command(SongLuaOverlayStateDelta {
            x: Some(100.0),
            draw_order: Some(5),
            ..SongLuaOverlayStateDelta::default()
        }),
        test_message_command(SongLuaOverlayStateDelta {
            y: Some(-40.0),
            z: Some(12.0),
            ..SongLuaOverlayStateDelta::default()
        }),
    ];
    let events = (0..128)
        .map(|index| SongLuaOverlayMessageRuntime {
            event_second: index as f32 * 0.5,
            command_index: index % commands.len(),
        })
        .collect::<Vec<_>>();
    let initial = SongLuaOverlayState {
        x: 7.0,
        y: 11.0,
        ..SongLuaOverlayState::default()
    };
    let mut cache = SongLuaMessageStateCache::default();

    for now in [-1.0, 0.0, 0.125, 1.0, 7.25, 31.75, 63.75, 24.25, 24.5, 63.9] {
        let expected = replay_song_lua_message_state(now, initial, &commands, Some(&events));
        let actual =
            song_lua_message_state_cached(now, initial, &commands, Some(&events), &mut cache);
        assert_eq!(actual, expected, "now={now}");
    }
}

#[test]
fn song_lua_cached_tween_applies_terminal_flags_and_rewinds() {
    let commands = [SongLuaOverlayMessageCommand {
        message: "show".to_owned(),
        aux: None,
        blocks: vec![SongLuaOverlayCommandBlock {
            start: 0.0,
            duration: 1.0,
            easing: Some("instant".to_owned()),
            opt1: None,
            opt2: None,
            delta: SongLuaOverlayStateDelta {
                texture_filtering: Some(false),
                depth_test: Some(true),
                ..SongLuaOverlayStateDelta::default()
            },
        }],
    }];
    let events = [SongLuaOverlayMessageRuntime {
        event_second: 1.0,
        command_index: 0,
    }];
    let initial = SongLuaOverlayState::default();
    let mut cache = SongLuaMessageStateCache::default();
    // An instant curve reaches its terminal factor while the block is
    // still active, exercising interpolation rather than completed deltas.
    for (now, filtering, depth) in [
        (0.5, true, false),
        (1.0, false, true),
        (1.5, false, true),
        (2.0, false, true),
        (0.5, true, false),
        (1.25, false, true),
    ] {
        let actual =
            song_lua_message_state_cached(now, initial, &commands, Some(&events), &mut cache);
        assert_eq!(actual.texture_filtering, filtering, "now={now}");
        assert_eq!(actual.depth_test, depth, "now={now}");
    }
}

#[test]
fn song_lua_static_overlay_state_skips_runtime_caches() {
    let overlay = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::Actor,
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState {
            x: 123.0,
            y: -45.0,
            diffuse: [0.25, 0.5, 0.75, 0.875],
            draw_order: 17,
            ..SongLuaOverlayState::default()
        },
        message_commands: Vec::new(),
    };
    let events = vec![Vec::new()];
    let ranges = vec![0..0];
    let mut dynamic_cache = SongLuaMessageStateCache::default();
    let mut static_cache = SongLuaMessageStateCache::default();
    let mut dynamic_cursors = Vec::new();
    let mut static_cursors = Vec::new();

    for now in [-1.0, 0.0, 42.0, 10_000.0] {
        let expected = song_lua_overlay_render_state_dynamic(
            now,
            0,
            &overlay,
            Some(&events[0]),
            &[],
            &ranges,
            &[],
            0..0,
            &mut dynamic_cursors,
            None,
            &mut dynamic_cache,
        );
        let actual = song_lua_overlay_render_state_from(
            now,
            0,
            &overlay,
            &events,
            &[],
            &ranges,
            &[],
            0..0,
            &mut static_cursors,
            None,
            &mut static_cache,
        );
        assert_eq!(actual, expected, "now={now}");
    }
    assert!(dynamic_cache.initialized);
    assert!(!static_cache.initialized);
}

#[test]
fn song_lua_overlay_update_track_restores_cropped_state() {
    use deadsync_song_lua::{
        SongLuaOverlayRuntimeUpdateSample, SongLuaOverlayRuntimeUpdateTrack,
        SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue,
    };
    let tracks = vec![SongLuaOverlayRuntimeUpdateTrack {
        overlay_index: 0,
        target: SongLuaOverlayUpdateTarget::CropTop,
        samples: vec![
            SongLuaOverlayRuntimeUpdateSample {
                second: 0.0,
                value: SongLuaOverlayUpdateValue::F32(0.0),
            },
            SongLuaOverlayRuntimeUpdateSample {
                second: 1.0,
                value: SongLuaOverlayUpdateValue::F32(0.8),
            },
            SongLuaOverlayRuntimeUpdateSample {
                second: 2.0,
                value: SongLuaOverlayUpdateValue::F32(0.0),
            },
        ],
    }];
    let mut cursors = vec![0; tracks.len()];
    let mut active = SongLuaOverlayState::default();
    apply_song_lua_overlay_runtime_updates_for(1.5, &tracks, 0..1, &mut cursors, None, &mut active);
    assert!((active.croptop - 0.4).abs() <= f32::EPSILON);

    let mut restored = SongLuaOverlayState {
        croptop: 0.9,
        ..SongLuaOverlayState::default()
    };
    apply_song_lua_overlay_runtime_updates_for(
        3.0,
        &tracks,
        0..1,
        &mut cursors,
        None,
        &mut restored,
    );
    assert_eq!(restored.croptop, 0.0);
}

#[test]
fn song_lua_overlay_visibility_and_slice_geometry_change_atomically() {
    use deadsync_song_lua::{
        SongLuaOverlayRuntimeUpdateSample as Sample, SongLuaOverlayRuntimeUpdateTrack as Track,
        SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Value,
    };
    let tracks = vec![
        Track {
            overlay_index: 0,
            target: Target::CropBottom,
            samples: vec![
                Sample {
                    second: 0.0,
                    value: Value::F32(0.0),
                },
                Sample {
                    second: 0.1,
                    value: Value::F32(0.8),
                },
                Sample {
                    second: 0.2,
                    value: Value::F32(0.0),
                },
            ],
        },
        Track {
            overlay_index: 1,
            target: Target::Visible,
            samples: vec![
                Sample {
                    second: 0.0,
                    value: Value::Bool(false),
                },
                Sample {
                    second: 0.1,
                    value: Value::Bool(true),
                },
                Sample {
                    second: 0.2,
                    value: Value::Bool(false),
                },
            ],
        },
    ];

    let mut crop = SongLuaOverlayState::default();
    let mut visible = SongLuaOverlayState::default();
    let mut cursors = vec![0; tracks.len()];
    let snap = song_lua_overlay_update_snap(0.05, &tracks, &[1], &mut cursors);
    apply_song_lua_overlay_runtime_updates_for(0.05, &tracks, 0..1, &mut cursors, snap, &mut crop);
    apply_song_lua_overlay_runtime_updates_for(
        0.05,
        &tracks,
        1..2,
        &mut cursors,
        snap,
        &mut visible,
    );
    assert_eq!(crop.cropbottom, 0.8);
    assert!(visible.visible);

    let mut crop = SongLuaOverlayState::default();
    let mut visible = SongLuaOverlayState::default();
    let snap = song_lua_overlay_update_snap(0.15, &tracks, &[1], &mut cursors);
    apply_song_lua_overlay_runtime_updates_for(0.15, &tracks, 0..1, &mut cursors, snap, &mut crop);
    apply_song_lua_overlay_runtime_updates_for(
        0.15,
        &tracks,
        1..2,
        &mut cursors,
        snap,
        &mut visible,
    );
    assert_eq!(crop.cropbottom, 0.8);
    assert!(visible.visible);
}

#[test]
fn song_lua_dormant_visibility_track_stays_hidden_until_trigger() {
    use deadsync_song_lua::{
        SongLuaOverlayRuntimeUpdateSample as Sample, SongLuaOverlayRuntimeUpdateTrack as Track,
        SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Value,
    };
    let tracks = vec![Track {
        overlay_index: 0,
        target: Target::Visible,
        samples: vec![
            Sample {
                second: 0.0,
                value: Value::Bool(false),
            },
            Sample {
                second: 60.0,
                value: Value::Bool(true),
            },
        ],
    }];
    let mut cursors = vec![0];
    let snap = song_lua_overlay_update_snap(1.0, &tracks, &[0], &mut cursors);
    let mut state = SongLuaOverlayState {
        visible: false,
        ..SongLuaOverlayState::default()
    };
    apply_song_lua_overlay_runtime_updates_for(1.0, &tracks, 0..1, &mut cursors, snap, &mut state);

    assert!(snap.is_none());
    assert!(!state.visible);
}

#[test]
fn song_lua_empty_overlay_state_clears_reused_outputs() {
    let mut message_caches = vec![SongLuaMessageStateCache::default()];
    let mut local_states = vec![SongLuaOverlayState::default()];
    let mut states = vec![SongLuaOverlayState::default()];
    let mut order_cache = SongLuaOverlayOrderCache::default();

    song_lua_overlay_state_sets_from_into::<SpriteSlot>(
        42.0,
        &[],
        &[],
        &[],
        &[],
        640.0,
        480.0,
        &mut order_cache,
        &mut message_caches,
        &mut local_states,
        &mut states,
    );

    assert!(message_caches.is_empty());
    assert!(local_states.is_empty());
    assert!(states.is_empty());
}

#[test]
fn song_lua_proxy_free_analysis_skips_capture_visits() {
    let overlays = vec![SongLuaOverlayActor {
        kind: SongLuaOverlayKind::Quad,
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    }];
    let states = vec![SongLuaOverlayState::default()];
    let index = SongLuaProxyRequestIndex::new(&overlays);
    let mut visit_scratch = SongLuaCaptureVisitScratch::with_capacity(overlays.len());
    let sources = [SongLuaPlayerProxySources::default(); 2];

    assert_eq!(
        song_lua_proxy_requests_indexed(&overlays, &states, &index, &mut visit_scratch),
        SongLuaScreenProxyRequests::default()
    );
    assert_eq!(
        song_lua_replacement_active_players_indexed(
            &overlays,
            &states,
            &sources,
            &index,
            &mut visit_scratch,
        ),
        [false; 2]
    );
    assert_eq!(visit_scratch.generation, 0);
}

#[test]
fn song_lua_empty_captured_timeline_returns_initial_state() {
    let actor = SongLuaCapturedActor {
        initial_state: SongLuaOverlayState {
            x: 123.0,
            y: -45.0,
            draw_order: 17,
            ..SongLuaOverlayState::default()
        },
        message_commands: Vec::new(),
        manual_hud_draw: false,
        ..SongLuaCapturedActor::default()
    };
    let mut expected_cache = SongLuaMessageStateCache::default();
    let mut fast_cache = SongLuaMessageStateCache::default();

    for now in [-1.0, 0.0, 42.0, 10_000.0] {
        let expected = song_lua_message_state_cached(
            now,
            actor.initial_state,
            &actor.message_commands,
            Some(&[]),
            &mut expected_cache,
        );
        let actual = song_lua_captured_actor_state_from(now, &actor, Some(&[]), &mut fast_cache);
        assert_eq!(actual, expected, "now={now}");
    }
    assert!(!fast_cache.initialized);
}

#[test]
fn empty_song_lua_layer_skips_preparation_without_changing_output() {
    let mut actors = vec![Actor::CameraPop];
    let mut order_cache = SongLuaOverlayOrderCache::default();
    let mut order = vec![7];
    let mut aft = SongLuaAftCaptureScratch::default();
    assert_eq!(
        prepare_song_lua_layer::<SpriteSlot>(
            &mut actors,
            &[],
            &[],
            SongLuaOverlayState::default(),
            &mut order_cache,
            &mut order,
            &mut aft,
            SONG_LUA_FOREGROUND_DEPTH,
        ),
        None
    );

    assert!(matches!(actors.as_slice(), [Actor::CameraPop]));
    assert!(order.is_empty());
}

#[test]
fn song_lua_message_block_cursor_matches_replay_across_block_rewinds() {
    let command = SongLuaOverlayMessageCommand {
        message: "LongCommand".to_string(),
        aux: None,
        blocks: (0..128)
            .map(|index| SongLuaOverlayCommandBlock {
                start: index as f32 * 0.25,
                duration: 0.2,
                easing: Some("inOutQuad".to_string()),
                opt1: None,
                opt2: None,
                delta: SongLuaOverlayStateDelta {
                    x: Some(index as f32 * 3.0),
                    y: (index % 5 == 0).then_some(-(index as f32)),
                    ..SongLuaOverlayStateDelta::default()
                },
            })
            .collect(),
    };
    let commands = vec![command];
    let events = vec![SongLuaOverlayMessageRuntime {
        event_second: 1.0,
        command_index: 0,
    }];
    let initial = SongLuaOverlayState {
        x: 11.0,
        y: 7.0,
        ..SongLuaOverlayState::default()
    };
    let mut cache = SongLuaMessageStateCache::default();

    for now in [0.0, 1.0, 1.1, 8.75, 24.4, 32.8, 9.25, 9.3, 31.0] {
        let expected = replay_song_lua_message_state(now, initial, &commands, Some(&events));
        let actual =
            song_lua_message_state_cached(now, initial, &commands, Some(&events), &mut cache);
        assert_eq!(actual, expected, "now={now}");
    }
}

#[test]
fn song_lua_dynamic_order_cache_tracks_draw_and_z_key_changes() {
    let mut overlays = (0..8)
        .map(|index| SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Quad,
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState {
                draw_order: index,
                ..SongLuaOverlayState::default()
            },
            message_commands: Vec::new(),
        })
        .collect::<Vec<_>>();
    let mut states = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    let mut cache = song_lua_overlay_order_cache_from(&overlays, &[]);
    cache.dynamic_draw_order[0] = true;
    cache.static_root_order = None;
    let mut actual = Vec::new();

    for changed_index in [0usize, 5, 2, 7] {
        states[changed_index].draw_order = 20 - changed_index as i32;
        song_lua_overlay_order_into(&overlays, &states, &mut cache, None, &mut actual);
        let mut expected = (0..overlays.len()).collect::<Vec<_>>();
        expected.sort_by_key(|&index| (states[index].draw_order, index));
        assert_eq!(actual, expected);
    }

    overlays[0].initial_state.draw_by_z_position = true;
    states[0].draw_by_z_position = true;
    for overlay in overlays.iter_mut().skip(1) {
        overlay.parent_index = Some(0);
    }
    let mut cache = song_lua_overlay_order_cache_from(&overlays, &[]);
    for (index, state) in states.iter_mut().enumerate().skip(1) {
        state.z = (index % 3) as f32;
    }
    song_lua_overlay_order_into(&overlays, &states, &mut cache, None, &mut actual);
    let mut expected_children = (1..overlays.len()).collect::<Vec<_>>();
    expected_children.sort_by(|&left, &right| {
        states[left]
            .z
            .total_cmp(&states[right].z)
            .then_with(|| left.cmp(&right))
    });
    let mut expected = vec![0];
    expected.extend(expected_children);
    assert_eq!(actual, expected);
}

#[test]
fn song_lua_static_order_flatten_matches_recursive_tree() {
    let overlays = (0..64)
        .map(|index| SongLuaOverlayActor {
            kind: SongLuaOverlayKind::ActorFrame,
            name: None,
            parent_index: (index > 0).then(|| (index - 1) / 4),
            initial_state: SongLuaOverlayState {
                draw_order: ((index * 37) % 17) as i32,
                ..SongLuaOverlayState::default()
            },
            message_commands: Vec::new(),
        })
        .collect::<Vec<_>>();
    let states = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    let mut cache = song_lua_overlay_order_cache_from(&overlays, &[]);
    let mut recursive = Vec::with_capacity(overlays.len());
    let mut flat = Vec::with_capacity(overlays.len());

    song_lua_push_order(&overlays, &states, &mut cache, None, &mut recursive);
    song_lua_overlay_order_into(&overlays, &states, &mut cache, None, &mut flat);

    assert!(cache.static_root_order.is_some());
    assert_eq!(flat, recursive);
}

#[test]
fn projected_overlay_bounded_scratch_stays_inline() {
    for (left, right) in [(0.0, 0.0), (0.25, 0.0), (0.25, 0.5), (1.0, 1.0)] {
        let slices = song_lua_projected_overlay_axis_slices(left, right);
        assert!(!slices.spilled(), "fade=({left}, {right})");
        assert!((2..=4).contains(&slices.len()));
        assert_eq!(slices.first(), Some(&0.0));
        assert_eq!(slices.last(), Some(&1.0));
    }
}

#[test]
fn song_meter_progress_uses_itg_first_second_anchor() {
    assert_eq!(song_meter_progress(-1.0, 2.0, 12.0), 0.0);
    assert_eq!(song_meter_progress(2.0, 2.0, 12.0), 0.0);
    assert!((song_meter_progress(7.0, 2.0, 12.0) - 0.5).abs() <= 1e-6);
    assert_eq!(song_meter_progress(12.0, 2.0, 12.0), 1.0);
}

fn transformed_player_fixture() -> SongLuaCaptureTransform {
    SongLuaCaptureTransform {
        z_shift: 900,
        tint: [0.8, 0.7, 0.6, 0.5],
        blend: Some(BlendMode::Add),
        playfield_center_x: 213.5,
        target_x: 237.5,
        target_y: 228.0,
        rotation_x: 4.0,
        rotation_z: 8.0,
        rotation_y: 13.0,
        skew_x: 0.1,
        skew_y: -0.05,
        zoom_x: 0.9,
        zoom_y: 1.1,
        zoom_z: 1.0,
    }
}

#[test]
fn player_transform_cache_reuses_the_exact_resolved_plan() {
    let metrics = PlayerTransformMetrics {
        screen_width: 854.0,
        screen_height: 480.0,
        screen_center_y: 240.0,
    };
    let transform = transformed_player_fixture();
    let expected =
        player_actor_assembly_for_transform_with_metrics(false, true, transform, metrics);
    let mut cache = PlayerActorAssemblyCache::default();

    let first = cache.resolve_with_metrics(false, true, transform, metrics);
    let second = cache.resolve_with_metrics(false, true, transform, metrics);

    assert_eq!(first, expected);
    assert_eq!(second, expected);
    assert_eq!(cache.stats().rebuilds, 1);
    assert_eq!(cache.stats().hits, 1);
}

#[test]
fn player_transform_cache_rebuilds_for_every_plan_source() {
    let metrics = PlayerTransformMetrics {
        screen_width: 854.0,
        screen_height: 480.0,
        screen_center_y: 240.0,
    };
    let base = transformed_player_fixture();
    let changed_transforms = [
        SongLuaCaptureTransform {
            z_shift: 901,
            ..base
        },
        SongLuaCaptureTransform {
            tint: [0.9, 0.7, 0.6, 0.5],
            ..base
        },
        SongLuaCaptureTransform {
            blend: Some(BlendMode::Multiply),
            ..base
        },
        SongLuaCaptureTransform {
            playfield_center_x: 214.5,
            ..base
        },
        SongLuaCaptureTransform {
            target_x: 238.5,
            ..base
        },
        SongLuaCaptureTransform {
            target_y: 229.0,
            ..base
        },
        SongLuaCaptureTransform {
            rotation_x: 5.0,
            ..base
        },
        SongLuaCaptureTransform {
            rotation_z: 9.0,
            ..base
        },
        SongLuaCaptureTransform {
            rotation_y: 14.0,
            ..base
        },
        SongLuaCaptureTransform {
            skew_x: 0.2,
            ..base
        },
        SongLuaCaptureTransform {
            skew_y: -0.1,
            ..base
        },
        SongLuaCaptureTransform {
            zoom_x: 0.8,
            ..base
        },
        SongLuaCaptureTransform {
            zoom_y: 1.2,
            ..base
        },
        SongLuaCaptureTransform {
            zoom_z: 0.9,
            ..base
        },
    ];

    for changed in changed_transforms {
        let mut cache = PlayerActorAssemblyCache::default();
        cache.resolve_with_metrics(false, true, base, metrics);
        let actual = cache.resolve_with_metrics(false, true, changed, metrics);
        let expected =
            player_actor_assembly_for_transform_with_metrics(false, true, changed, metrics);
        assert_eq!(actual, expected);
        assert_eq!(cache.stats().rebuilds, 2);
        assert_eq!(cache.stats().hits, 0);
    }

    for (requests_proxy, visible) in [(true, true), (false, false)] {
        let mut cache = PlayerActorAssemblyCache::default();
        cache.resolve_with_metrics(false, true, base, metrics);
        let actual = cache.resolve_with_metrics(requests_proxy, visible, base, metrics);
        let expected = player_actor_assembly_for_transform_with_metrics(
            requests_proxy,
            visible,
            base,
            metrics,
        );
        assert_eq!(actual, expected);
        assert_eq!(cache.stats().rebuilds, 1);
        assert_eq!(cache.stats().hits, 0);
    }

    for changed_metrics in [
        PlayerTransformMetrics {
            screen_width: 640.0,
            ..metrics
        },
        PlayerTransformMetrics {
            screen_height: 720.0,
            ..metrics
        },
        PlayerTransformMetrics {
            screen_center_y: 300.0,
            ..metrics
        },
    ] {
        let mut cache = PlayerActorAssemblyCache::default();
        cache.resolve_with_metrics(false, true, base, metrics);
        let actual = cache.resolve_with_metrics(false, true, base, changed_metrics);
        let expected =
            player_actor_assembly_for_transform_with_metrics(false, true, base, changed_metrics);
        assert_eq!(actual, expected);
        assert_eq!(cache.stats().rebuilds, 2);
    }
}

fn matrix_bits(matrix: Matrix4) -> [u32; 16] {
    matrix.to_cols_array().map(f32::to_bits)
}

#[test]
fn transformed_field_camera_cache_reuses_the_exact_product() {
    let field =
        Matrix4::from_rotation_x(0.17) * Matrix4::from_translation(Vector3::new(4.0, 8.0, 12.0));
    let suffix = Matrix4::from_rotation_z(0.11) * Matrix4::from_scale(Vector3::new(0.9, 1.1, 1.0));
    let root = Matrix4::from_translation(Vector3::new(2.0, 3.0, 4.0));
    let expected = field * suffix;
    let mut cache = PlayerFieldCameraCache::default();

    let first = cache.resolve(7, 13, Some(field), root, suffix);
    let second = cache.resolve(7, 13, Some(field), root, suffix);

    assert_eq!(matrix_bits(first), matrix_bits(expected));
    assert_eq!(matrix_bits(second), matrix_bits(expected));
    assert_eq!(cache.stats().rebuilds, 1);
    assert_eq!(cache.stats().hits, 1);
}

#[test]
fn transformed_field_camera_cache_follows_both_source_generations() {
    let field = Matrix4::from_rotation_x(0.17);
    let changed_field = Matrix4::from_rotation_x(0.23);
    let suffix = Matrix4::from_rotation_z(0.11);
    let changed_suffix = Matrix4::from_rotation_z(0.19);
    let root = Matrix4::from_translation(Vector3::new(2.0, 3.0, 4.0));
    let mut cache = PlayerFieldCameraCache::default();

    cache.resolve(7, 13, Some(field), root, suffix);
    let field_changed = cache.resolve(8, 13, Some(changed_field), root, suffix);
    let transform_changed = cache.resolve(8, 14, Some(changed_field), root, changed_suffix);
    let invalid_field = cache.resolve(9, 14, None, root, changed_suffix);

    assert_eq!(
        matrix_bits(field_changed),
        matrix_bits(changed_field * suffix)
    );
    assert_eq!(
        matrix_bits(transform_changed),
        matrix_bits(changed_field * changed_suffix)
    );
    assert_eq!(matrix_bits(invalid_field), matrix_bits(root));
    assert_eq!(cache.stats().rebuilds, 4);
    assert_eq!(cache.stats().hits, 0);
}

#[test]
fn direct_player_segments_respect_fallback_boundaries() {
    let identity = SongLuaCaptureTransform {
        z_shift: 0,
        tint: [1.0; 4],
        blend: None,
        playfield_center_x: screen_center_x(),
        target_x: screen_center_x(),
        target_y: screen_center_y(),
        rotation_x: 0.0,
        rotation_z: 0.0,
        rotation_y: 0.0,
        skew_x: 0.0,
        skew_y: 0.0,
        zoom_x: 1.0,
        zoom_y: 1.0,
        zoom_z: 1.0,
    };
    assert_eq!(
        player_actor_assembly_for_transform(false, true, identity),
        PlayerActorAssembly::DirectZ { z_shift: 0 }
    );
    assert_eq!(
        player_actor_assembly_for_transform(
            false,
            true,
            SongLuaCaptureTransform {
                z_shift: 900,
                ..identity
            },
        ),
        PlayerActorAssembly::DirectZ { z_shift: 900 }
    );
    assert_eq!(
        player_actor_assembly_for_transform(
            false,
            true,
            SongLuaCaptureTransform {
                tint: [0.8, 0.7, 0.6, 0.5],
                ..identity
            },
        ),
        PlayerActorAssembly::DirectZ { z_shift: 0 }
    );
    assert_eq!(
        player_actor_assembly_for_transform(
            false,
            true,
            SongLuaCaptureTransform {
                blend: Some(BlendMode::Add),
                ..identity
            },
        ),
        PlayerActorAssembly::DirectZ { z_shift: 0 }
    );
    assert_eq!(
        player_actor_assembly_for_transform(true, true, identity),
        PlayerActorAssembly::Captured
    );
    assert_eq!(
        player_actor_assembly_for_transform(true, false, identity),
        PlayerActorAssembly::Hidden
    );
    assert_eq!(
        player_actor_assembly_for_transform(false, false, identity),
        PlayerActorAssembly::Hidden
    );
    assert!(matches!(
        player_actor_assembly_for_transform(
            false,
            true,
            SongLuaCaptureTransform {
                target_x: identity.target_x + 1.0,
                ..identity
            },
        ),
        PlayerActorAssembly::DirectTransform { .. }
    ));
    assert!(matches!(
        player_actor_assembly_for_transform(
            false,
            true,
            SongLuaCaptureTransform {
                rotation_y: 1.0,
                ..identity
            },
        ),
        PlayerActorAssembly::DirectFold { .. }
    ));
}

#[test]
fn dynamic_player_transforms_have_zero_legacy_fallbacks() {
    let metrics = PlayerTransformMetrics {
        screen_width: 854.0,
        screen_height: 480.0,
        screen_center_y: 240.0,
    };
    let identity = SongLuaCaptureTransform {
        z_shift: 0,
        tint: [1.0; 4],
        blend: None,
        playfield_center_x: 213.5,
        target_x: 213.5,
        target_y: 240.0,
        rotation_x: 0.0,
        rotation_z: 0.0,
        rotation_y: 0.0,
        skew_x: 0.0,
        skew_y: 0.0,
        zoom_x: 1.0,
        zoom_y: 1.0,
        zoom_z: 1.0,
    };
    let transforms: [SongLuaCaptureTransform; 14] = std::array::from_fn(|source| {
        let mut transform = identity;
        match source {
            0 => transform.z_shift = 900,
            1 => transform.tint = [0.8, 0.7, 0.6, 0.5],
            2 => transform.blend = Some(BlendMode::Add),
            3 => transform.playfield_center_x += 24.0,
            4 => transform.target_x += 24.0,
            5 => transform.target_y -= 12.0,
            6 => transform.rotation_x = 4.0,
            7 => transform.rotation_z = 8.0,
            8 => transform.rotation_y = 13.0,
            9 => transform.skew_x = 0.1,
            10 => transform.skew_y = -0.05,
            11 => transform.zoom_x = 0.9,
            12 => transform.zoom_y = 1.1,
            13 => transform.zoom_z = 0.9,
            _ => unreachable!("fixed transform source domain"),
        }
        transform
    });
    let assemblies = transforms.map(|transform| {
        player_actor_assembly_for_transform_with_metrics(false, true, transform, metrics)
    });

    assert_eq!(
        assemblies
            .iter()
            .filter(|assembly| matches!(assembly, PlayerActorAssembly::Captured))
            .count(),
        0,
    );
    assert!(assemblies.into_iter().all(|assembly| matches!(
        assembly,
        PlayerActorAssembly::DirectZ { .. }
            | PlayerActorAssembly::DirectFold { .. }
            | PlayerActorAssembly::DirectTransform { .. }
    )));
    assert_eq!(
        player_actor_assembly_for_transform_with_metrics(true, true, identity, metrics),
        PlayerActorAssembly::Captured,
        "whole-player capture is selected only by an explicit proxy request",
    );
}

#[test]
fn forced_center_view_uses_layout_player_x() {
    let view = ViewOverride {
        force_center_1player: true,
        ..ViewOverride::default()
    };

    assert_eq!(song_lua_player_target_x(None, 320.0, 800.0, view), 800.0);
}

#[test]
fn forced_center_view_preserves_explicit_player_x() {
    let view = ViewOverride {
        force_center_1player: true,
        ..ViewOverride::default()
    };

    assert_eq!(
        song_lua_player_target_x(Some(640.0), 320.0, 800.0, view),
        640.0
    );
}

#[test]
fn default_view_uses_player_state_x() {
    assert_eq!(
        song_lua_player_target_x(None, 320.0, 800.0, ViewOverride::default()),
        320.0
    );
}

fn test_proxy_overlay(player_index: usize) -> SongLuaOverlayActor {
    SongLuaOverlayActor {
        kind: SongLuaOverlayKind::ActorProxy {
            target: SongLuaProxyTarget::Player { player_index },
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    }
}

fn test_capture_overlay(name: &str) -> SongLuaOverlayActor {
    SongLuaOverlayActor {
        kind: SongLuaOverlayKind::ActorFrameTexture {
            alpha_buffer: false,
            depth_buffer: false,
            preserve_texture: false,
        },
        name: Some(name.to_string()),
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    }
}

fn test_alpha_capture_overlay(name: &str) -> SongLuaOverlayActor {
    let mut overlay = test_capture_overlay(name);
    overlay.kind = SongLuaOverlayKind::ActorFrameTexture {
        alpha_buffer: true,
        depth_buffer: false,
        preserve_texture: false,
    };
    overlay.initial_state.size = Some([854.0, 480.0]);
    overlay
}

fn test_capture_proxy_child(
    parent_index: usize,
    target: SongLuaProxyTarget,
) -> SongLuaOverlayActor {
    SongLuaOverlayActor {
        kind: SongLuaOverlayKind::ActorProxy { target },
        name: None,
        parent_index: Some(parent_index),
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    }
}

fn test_aft_overlay(capture_name: &str, visible: bool) -> SongLuaOverlayActor {
    SongLuaOverlayActor {
        kind: SongLuaOverlayKind::AftSprite {
            capture_name: capture_name.to_string(),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState {
            visible,
            ..SongLuaOverlayState::default()
        },
        message_commands: Vec::new(),
    }
}

fn test_direct_aft_overlay(capture_name: &str) -> SongLuaOverlayActor {
    let mut overlay = test_aft_overlay(capture_name, true);
    overlay.initial_state.x = 0.5 * screen_width();
    overlay.initial_state.y = 0.5 * screen_height();
    overlay
}

fn test_rgb_aft_overlay(name: &str, capture_name: &str, diffuse: [f32; 4]) -> SongLuaOverlayActor {
    let mut overlay = test_aft_overlay(capture_name, true);
    overlay.name = Some(name.to_string());
    overlay.initial_state.x = screen_width() * 0.5;
    overlay.initial_state.y = screen_height() * 0.5;
    overlay.initial_state.diffuse = diffuse;
    overlay.initial_state.blend = SongLuaOverlayBlendMode::Add;
    overlay
}

fn test_covering_aft_overlays() -> Vec<SongLuaOverlayActor> {
    let mut sprite = test_aft_overlay("ScreenCapture", true);
    sprite.initial_state.x = 427.0;
    sprite.initial_state.y = 240.0;
    vec![
        test_capture_overlay("ScreenCapture"),
        test_capture_proxy_child(0, SongLuaProxyTarget::Underlay { hidden: false }),
        test_capture_proxy_child(0, SongLuaProxyTarget::Overlay { hidden: false }),
        test_capture_proxy_child(0, SongLuaProxyTarget::Player { player_index: 0 }),
        sprite,
    ]
}

fn test_nested_alpha_covering_aft_overlays(scale: f32) -> Vec<SongLuaOverlayActor> {
    let mut black = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::Quad,
        name: None,
        parent_index: Some(0),
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    black.initial_state.x = 427.0;
    black.initial_state.y = 240.0;
    black.initial_state.stretch_rect = Some([0.0, 0.0, 854.0, 480.0]);

    let mut inner = test_aft_overlay("ScreenTex", true);
    inner.parent_index = Some(5);
    inner.initial_state.x = 427.0;
    inner.initial_state.y = 240.0;
    inner.initial_state.zoom = 1.0 / scale;

    let mut outer = test_aft_overlay("ScreenPixel", true);
    outer.initial_state.x = 427.0;
    outer.initial_state.y = 240.0;
    outer.initial_state.zoom = scale;
    outer.initial_state.texture_filtering = false;

    vec![
        test_alpha_capture_overlay("ScreenTex"),
        black,
        test_capture_proxy_child(0, SongLuaProxyTarget::Underlay { hidden: false }),
        test_capture_proxy_child(0, SongLuaProxyTarget::Overlay { hidden: false }),
        test_capture_proxy_child(0, SongLuaProxyTarget::Player { player_index: 0 }),
        test_alpha_capture_overlay("ScreenPixel"),
        inner,
        outer,
    ]
}

#[test]
fn fullscreen_opaque_aft_marks_captured_screen_sources_occluded() {
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    let overlays = test_covering_aft_overlays();
    let states = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    let index = SongLuaProxyRequestIndex::new(&overlays);
    let mut visits = SongLuaCaptureVisitScratch::with_capacity(overlays.len());

    let requests = song_lua_covering_capture_requests(
        &overlays,
        &states,
        &states,
        &index,
        854.0,
        480.0,
        &mut visits,
    );

    assert!(requests.underlay);
    assert!(requests.overlay);
    assert!(requests.players[0].player);
    assert!(!requests.players[1].player);
}

#[test]
fn nested_alpha_afts_propagate_their_opaque_screen_region() {
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    for scale in [2.0, 5.0, 10.0] {
        let overlays = test_nested_alpha_covering_aft_overlays(scale);
        let states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();
        let index = SongLuaProxyRequestIndex::new(&overlays);
        let mut visits = SongLuaCaptureVisitScratch::with_capacity(overlays.len());

        let requests = song_lua_covering_capture_requests(
            &overlays,
            &states,
            &states,
            &index,
            854.0,
            480.0,
            &mut visits,
        );

        assert!(requests.underlay, "scale {scale}");
        assert!(requests.overlay, "scale {scale}");
        assert!(requests.players[0].player, "scale {scale}");
    }
}

#[test]
fn multi_sprite_alpha_aft_keeps_original_screen_sources() {
    let mut overlays = test_nested_alpha_covering_aft_overlays(2.0);
    let mut second_slice = overlays[6].clone();
    second_slice.name = Some("SecondSlice".to_owned());
    overlays.push(second_slice);
    let states = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    let index = SongLuaProxyRequestIndex::new(&overlays);
    let mut visits = SongLuaCaptureVisitScratch::with_capacity(overlays.len());

    let requests = song_lua_covering_capture_requests(
        &overlays,
        &states,
        &states,
        &index,
        854.0,
        480.0,
        &mut visits,
    );

    assert!(!requests.underlay);
    assert!(!requests.overlay);
    assert!(!requests.players[0].player);
}

#[test]
fn alpha_aft_without_an_opaque_child_keeps_original_screen_sources() {
    let mut overlays = test_nested_alpha_covering_aft_overlays(10.0);
    overlays[1].initial_state.diffuse[3] = 0.5;
    let states = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    let index = SongLuaProxyRequestIndex::new(&overlays);
    let mut visits = SongLuaCaptureVisitScratch::with_capacity(overlays.len());

    let requests = song_lua_covering_capture_requests(
        &overlays,
        &states,
        &states,
        &index,
        854.0,
        480.0,
        &mut visits,
    );

    assert!(!requests.underlay);
    assert!(!requests.overlay);
    assert!(!requests.players[0].player);
}

#[test]
fn partial_translucent_and_alpha_afts_keep_original_screen_sources() {
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    let mut overlays = test_covering_aft_overlays();
    let index = SongLuaProxyRequestIndex::new(&overlays);
    let mut visits = SongLuaCaptureVisitScratch::with_capacity(overlays.len());
    let requests_for = |overlays: &[SongLuaOverlayActor],
                        visits: &mut SongLuaCaptureVisitScratch| {
        let states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();
        song_lua_covering_capture_requests(overlays, &states, &states, &index, 854.0, 480.0, visits)
    };

    overlays[4].initial_state.zoom = 0.5;
    assert!(!requests_for(&overlays, &mut visits).underlay);
    overlays[4].initial_state.zoom = 1.0;
    overlays[4].initial_state.diffuse[3] = 0.5;
    assert!(!requests_for(&overlays, &mut visits).underlay);
    overlays[4].initial_state.diffuse[3] = 1.0;
    overlays[0].initial_state.visible = false;
    assert!(!requests_for(&overlays, &mut visits).underlay);
    overlays[0].initial_state.visible = true;
    overlays[4].initial_state.depth_test = true;
    assert!(!requests_for(&overlays, &mut visits).underlay);
    overlays[4].initial_state.depth_test = false;
    overlays[0].kind = SongLuaOverlayKind::ActorFrameTexture {
        alpha_buffer: true,
        depth_buffer: false,
        preserve_texture: false,
    };
    assert!(!requests_for(&overlays, &mut visits).underlay);
}

fn test_source_actor() -> Actor {
    Actor::Frame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children: Vec::new(),
        background: None,
        z: 0,
    }
}

fn test_order_overlay(
    kind: SongLuaOverlayKind,
    parent_index: Option<usize>,
    draw_order: i32,
) -> SongLuaOverlayActor {
    SongLuaOverlayActor {
        kind,
        name: None,
        parent_index,
        initial_state: SongLuaOverlayState {
            draw_order,
            ..SongLuaOverlayState::default()
        },
        message_commands: Vec::new(),
    }
}

fn test_skewed_overlay_point(
    center: [f32; 2],
    local: [f32; 2],
    skew_x: f32,
    skew_y: f32,
) -> [f32; 2] {
    let y = skew_y.mul_add(local[0], local[1]);
    let x = skew_x.mul_add(y, local[0]);
    [center[0] + x, center[1] + y]
}

fn test_transform_point(matrix: Matrix4, local: [f32; 2]) -> [f32; 2] {
    let point = matrix * Vector4::new(local[0], local[1], 0.0, 1.0);
    [point.x, point.y]
}

fn first_textured_mesh_transform(actors: &[Actor]) -> Matrix4 {
    actors
        .iter()
        .find_map(|actor| match actor {
            Actor::TexturedMesh {
                local_transform, ..
            } => Some(*local_transform),
            Actor::Frame { children, .. } => children.iter().find_map(|child| match child {
                Actor::TexturedMesh {
                    local_transform, ..
                } => Some(*local_transform),
                _ => None,
            }),
            _ => None,
        })
        .expect("expected textured mesh actor")
}

trait SongLuaActorListTestExt {
    fn expect_actor(self, message: &str) -> Actor;
    fn expect_actors(self, message: &str) -> SongLuaActorList;
}

impl SongLuaActorListTestExt for Option<SongLuaActorList> {
    fn expect_actor(self, message: &str) -> Actor {
        let mut actors = self.expect_actors(message);
        assert_eq!(
            actors.len(),
            1,
            "{message}: expected one actor, got {}",
            actors.len()
        );
        actors.remove(0)
    }

    fn expect_actors(self, message: &str) -> SongLuaActorList {
        self.unwrap_or_else(|| panic!("{message}"))
    }
}

#[test]
fn song_lua_note_field_proxy_source_preserves_camera_transform() {
    let segments = [Arc::<[Actor]>::from([
        Actor::CameraPush {
            view_proj: Matrix4::IDENTITY,
        },
        test_source_actor(),
        Actor::CameraPop,
    ])];
    let mut out = Vec::new();

    append_song_lua_player_transform(
        segments.iter().flat_map(|segment| segment.iter().cloned()),
        std::iter::empty(),
        3,
        0,
        true,
        &mut out,
        0,
        [1.0; 4],
        None,
        screen_center_x(),
        screen_center_x(),
        screen_center_y(),
        0.0,
        0.0,
        0.0,
        0.5,
        0.0,
        1.0,
        1.0,
        1.0,
    );

    let Some(Actor::CameraPush { view_proj }) = out.first() else {
        panic!("expected transformed notefield camera");
    };
    let point = test_transform_point(*view_proj, [0.0, -20.0]);
    assert!((point[0] - 10.0).abs() <= 0.000_1);
    assert!((point[1] + 20.0).abs() <= 0.000_1);
}

#[test]
fn song_lua_proxy_active_players_requires_a_render_source() {
    let overlays = vec![test_proxy_overlay(0)];
    let overlay_states = vec![SongLuaOverlayState::default()];
    let index = SongLuaProxyRequestIndex::new(&overlays);
    let empty_sources = [
        SongLuaPlayerProxySources::default(),
        SongLuaPlayerProxySources::default(),
    ];

    assert_eq!(
        song_lua_proxy_active_players_indexed(&overlays, &overlay_states, &empty_sources, &index,),
        [false, false]
    );

    let source = vec![Arc::<[Actor]>::from(vec![test_source_actor()])];
    let sources = [
        SongLuaPlayerProxySources {
            player: Some(SongLuaProxySource::new(source.as_slice())),
            ..SongLuaPlayerProxySources::default()
        },
        SongLuaPlayerProxySources::default(),
    ];
    assert_eq!(
        song_lua_proxy_active_players_indexed(&overlays, &overlay_states, &sources, &index),
        [true, false]
    );
}

#[test]
fn song_lua_proxy_requests_ignore_unreferenced_capture_children() {
    let overlays = vec![
        test_capture_overlay("cap"),
        test_capture_proxy_child(0, SongLuaProxyTarget::Player { player_index: 0 }),
    ];
    let overlay_states = vec![SongLuaOverlayState::default(); overlays.len()];
    let index = SongLuaProxyRequestIndex::new(&overlays);
    let mut visit_scratch = SongLuaCaptureVisitScratch::with_capacity(overlays.len());
    let requests =
        song_lua_proxy_requests_indexed(&overlays, &overlay_states, &index, &mut visit_scratch);

    assert!(!requests.players[0].player);
    assert!(!requests.players[0].note_field);
    assert!(!requests.players[0].judgment);
    assert!(!requests.players[0].combo);
    assert!(!requests.underlay);
    assert!(!requests.overlay);
}

#[test]
fn song_lua_proxy_requests_follow_visible_aft_capture_usage() {
    let overlays = vec![
        test_capture_overlay("cap"),
        test_capture_proxy_child(0, SongLuaProxyTarget::Judgment { player_index: 0 }),
        test_direct_aft_overlay("cap"),
    ];
    let overlay_states = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    let index = SongLuaProxyRequestIndex::new(&overlays);
    let mut visit_scratch = SongLuaCaptureVisitScratch::with_capacity(overlays.len());
    let requests =
        song_lua_proxy_requests_indexed(&overlays, &overlay_states, &index, &mut visit_scratch);

    assert!(!requests.players[0].player);
    assert!(!requests.players[0].note_field);
    assert!(requests.players[0].judgment);
    assert!(!requests.players[0].combo);
}

#[test]
fn song_lua_proxy_analysis_separates_root_and_captured_judgment() {
    let root = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::ActorProxy {
            target: SongLuaProxyTarget::Judgment { player_index: 0 },
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let overlays = vec![
        root,
        test_capture_overlay("cap"),
        test_capture_proxy_child(1, SongLuaProxyTarget::Judgment { player_index: 0 }),
        test_direct_aft_overlay("cap"),
    ];
    let overlay_states = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    let index = SongLuaProxyRequestIndex::new(&overlays);
    let mut visit_scratch = SongLuaCaptureVisitScratch::with_capacity(overlays.len());
    let analysis = song_lua_proxy_request_analysis_indexed(
        &overlays,
        &overlay_states,
        &index,
        &mut visit_scratch,
    );

    assert_eq!(analysis.root_judgments, [1, 0]);
    assert!(analysis.all.players[0].judgment);
    assert!(analysis.captured.players[0].judgment);
}

#[test]
fn song_lua_proxy_analysis_separates_root_and_captured_combo() {
    let root = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::ActorProxy {
            target: SongLuaProxyTarget::Combo { player_index: 0 },
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let overlays = vec![
        root,
        test_capture_overlay("cap"),
        test_capture_proxy_child(1, SongLuaProxyTarget::Combo { player_index: 0 }),
        test_direct_aft_overlay("cap"),
    ];
    let overlay_states = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    let index = SongLuaProxyRequestIndex::new(&overlays);
    let mut visit_scratch = SongLuaCaptureVisitScratch::with_capacity(overlays.len());
    let analysis = song_lua_proxy_request_analysis_indexed(
        &overlays,
        &overlay_states,
        &index,
        &mut visit_scratch,
    );

    assert_eq!(analysis.root_combos, [1, 0]);
    assert!(analysis.all.players[0].combo);
    assert!(analysis.captured.players[0].combo);
}

#[test]
fn song_lua_aft_collects_mixed_visual_capture_requests() {
    let overlays = vec![
        test_capture_overlay("cap"),
        test_capture_proxy_child(0, SongLuaProxyTarget::NoteField { player_index: 0 }),
        SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Quad,
            name: None,
            parent_index: Some(0),
            initial_state: SongLuaOverlayState {
                size: Some([8.0, 8.0]),
                ..SongLuaOverlayState::default()
            },
            message_commands: Vec::new(),
        },
        test_aft_overlay("cap", true),
    ];
    let overlay_states = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    let index = SongLuaProxyRequestIndex::new(&overlays);
    let mut visit_scratch = SongLuaCaptureVisitScratch::with_capacity(overlays.len());
    let analysis = song_lua_proxy_request_analysis_indexed(
        &overlays,
        &overlay_states,
        &index,
        &mut visit_scratch,
    );

    assert!(analysis.captured.players[0].note_field);
    assert_eq!(index.direct_proxy_capacity(&overlays), 0);
}

#[test]
fn song_lua_aft_collects_transformed_capture_requests() {
    let overlays = vec![
        test_capture_overlay("cap"),
        test_capture_proxy_child(0, SongLuaProxyTarget::NoteField { player_index: 0 }),
        test_aft_overlay("cap", true),
    ];
    let overlay_states = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    let index = SongLuaProxyRequestIndex::new(&overlays);
    let mut visit_scratch = SongLuaCaptureVisitScratch::with_capacity(overlays.len());
    let analysis = song_lua_proxy_request_analysis_indexed(
        &overlays,
        &overlay_states,
        &index,
        &mut visit_scratch,
    );

    assert!(analysis.captured.players[0].note_field);
}

#[test]
fn song_lua_direct_proxy_capacity_counts_every_root_destination() {
    let root = |target| SongLuaOverlayActor {
        kind: SongLuaOverlayKind::ActorProxy { target },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let overlays = vec![
        root(SongLuaProxyTarget::Player { player_index: 0 }),
        root(SongLuaProxyTarget::Combo { player_index: 0 }),
        root(SongLuaProxyTarget::Combo { player_index: 0 }),
        root(SongLuaProxyTarget::Judgment { player_index: 0 }),
        test_capture_overlay("cap"),
        test_capture_proxy_child(4, SongLuaProxyTarget::NoteField { player_index: 0 }),
        test_aft_overlay("cap", true),
    ];
    let index = SongLuaProxyRequestIndex::new(&overlays);

    assert_eq!(index.direct_proxy_capacity(&overlays), 5);
}

#[test]
fn song_lua_proxy_analysis_separates_root_and_captured_player() {
    let root_player = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::ActorProxy {
            target: SongLuaProxyTarget::Player { player_index: 0 },
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let overlays = vec![
        root_player,
        test_capture_overlay("cap"),
        test_capture_proxy_child(1, SongLuaProxyTarget::Player { player_index: 0 }),
        test_aft_overlay("cap", true),
    ];
    let states = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    let index = SongLuaProxyRequestIndex::new(&overlays);
    let mut visits = SongLuaCaptureVisitScratch::with_capacity(overlays.len());
    let analysis = song_lua_proxy_request_analysis_indexed(&overlays, &states, &index, &mut visits);

    assert_eq!(analysis.root_players, [1, 0]);
    assert!(analysis.captured.players[0].player);
    assert_eq!(index.direct_proxy_capacity(&overlays), 2);
}

#[test]
fn song_lua_root_note_field_can_replace_player_from_direct_source() {
    let overlays = vec![SongLuaOverlayActor {
        kind: SongLuaOverlayKind::ActorProxy {
            target: SongLuaProxyTarget::NoteField { player_index: 0 },
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    }];
    let overlay_states = vec![SongLuaOverlayState::default()];
    let index = SongLuaProxyRequestIndex::new(&overlays);
    let mut visit_scratch = SongLuaCaptureVisitScratch::with_capacity(overlays.len());
    let analysis = song_lua_proxy_request_analysis_indexed(
        &overlays,
        &overlay_states,
        &index,
        &mut visit_scratch,
    );
    assert_eq!(analysis.root_note_fields, [1, 0]);
    assert!(!analysis.captured.players[0].note_field);

    let sources = [
        SongLuaPlayerProxySources {
            direct_note_field: true,
            ..SongLuaPlayerProxySources::default()
        },
        SongLuaPlayerProxySources::default(),
    ];
    assert_eq!(
        song_lua_replacement_active_players_indexed(
            &overlays,
            &overlay_states,
            &sources,
            &index,
            &mut visit_scratch,
        ),
        [true, false]
    );
}

#[test]
fn song_lua_direct_proxies_record_exact_overlay_insertions() {
    let quad = || SongLuaOverlayActor {
        kind: SongLuaOverlayKind::Quad,
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState {
            size: Some([8.0, 8.0]),
            ..SongLuaOverlayState::default()
        },
        message_commands: Vec::new(),
    };
    let judgment_proxy = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::ActorProxy {
            target: SongLuaProxyTarget::Judgment { player_index: 0 },
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState {
            x: 100.0,
            y: 80.0,
            diffuse: [0.5, 0.75, 1.0, 0.8],
            blend: SongLuaOverlayBlendMode::Add,
            ..SongLuaOverlayState::default()
        },
        message_commands: Vec::new(),
    };
    let field_proxy = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::ActorProxy {
            target: SongLuaProxyTarget::NoteField { player_index: 0 },
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState {
            x: 64.0,
            y: 48.0,
            ..SongLuaOverlayState::default()
        },
        message_commands: Vec::new(),
    };
    let combo_proxy = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::ActorProxy {
            target: SongLuaProxyTarget::Combo { player_index: 0 },
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState {
            x: 320.0,
            y: 120.0,
            diffuse: [1.0, 0.5, 0.25, 0.75],
            ..SongLuaOverlayState::default()
        },
        message_commands: Vec::new(),
    };
    let second_combo_proxy = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::ActorProxy {
            target: SongLuaProxyTarget::Combo { player_index: 0 },
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState {
            x: 420.0,
            y: 180.0,
            ..SongLuaOverlayState::default()
        },
        message_commands: Vec::new(),
    };
    let overlays = vec![
        quad(),
        field_proxy,
        quad(),
        judgment_proxy,
        combo_proxy,
        quad(),
        second_combo_proxy,
    ];
    let overlay_states = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    let proxy_sources = SongLuaScreenProxySources {
        direct_note_fields: [
            Some(SongLuaDirectProxySource {
                draws: SongLuaDirectDraws::Field,
                draw_start: 5,
                draw_end: 9,
                target: [160.0, 240.0],
                tint: [1.0; 4],
                x_fold: None,
                camera: Some(Matrix4::IDENTITY),
                player_camera: None,
            }),
            None,
        ],
        direct_judgments: [
            Some(SongLuaDirectProxySource {
                draws: SongLuaDirectDraws::Judgment,
                draw_start: 2,
                draw_end: 4,
                target: [213.0, 240.0],
                tint: [1.0; 4],
                x_fold: None,
                camera: None,
                player_camera: None,
            }),
            None,
        ],
        direct_combos: [
            Some(SongLuaDirectProxySource {
                draws: SongLuaDirectDraws::Combo,
                draw_start: 0,
                draw_end: 1,
                target: [213.0, 240.0],
                tint: [1.0; 4],
                x_fold: None,
                camera: None,
                player_camera: None,
            }),
            None,
        ],
        ..SongLuaScreenProxySources::default()
    };
    let mut order_cache = song_lua_overlay_order_cache_from(&overlays, &[]);
    let mut topology_index = SongLuaOverlayTopologyIndex::new(&overlays);
    let mut out = Vec::new();
    let mut direct = SongLuaDirectProxies::with_capacity(4);
    let mut order_scratch = Vec::new();
    let mut capture_states = Vec::new();
    let mut capture_order_scratch = Vec::new();
    let mut aft_capture_scratch = SongLuaAftCaptureScratch::new(&overlays, &topology_index);
    let mut projected_mesh_scratch = song_lua_projected_mesh_scratch_for(&overlays);

    let mut targets = Vec::new();
    push_song_lua_layer_actors(
        &mut out,
        &mut targets,
        &overlays,
        &mut order_cache,
        &mut topology_index,
        &overlay_states,
        &overlay_states,
        SongLuaOverlayState::default(),
        &proxy_sources,
        Some(&mut direct),
        None,
        &AssetManager::new(),
        screen_width(),
        screen_height(),
        0.0,
        0.0,
        0.0,
        &mut order_scratch,
        &mut capture_states,
        &mut capture_order_scratch,
        &mut aft_capture_scratch,
        &mut projected_mesh_scratch,
        SONG_LUA_FOREGROUND_DEPTH,
    );

    assert_eq!(out.len(), 3);
    assert_eq!(direct.len(), 4);
    let field = direct.entries[0];
    assert_eq!(field.actor_insert, 1);
    assert_eq!(field.player, 0);
    assert_eq!(field.draws, SongLuaDirectDraws::Field);
    assert_eq!((field.draw_start, field.draw_end), (5, 9));
    assert_eq!(field.offset, [-96.0, -192.0]);
    assert_eq!(field.camera, Some(Matrix4::IDENTITY));
    let judgment = direct.entries[1];
    assert_eq!(judgment.actor_insert, 2);
    assert_eq!(judgment.player, 0);
    assert_eq!(judgment.draws, SongLuaDirectDraws::Judgment);
    assert_eq!((judgment.draw_start, judgment.draw_end), (2, 4));
    assert_eq!(judgment.offset, [-113.0, -160.0]);
    assert_eq!(
        judgment.style,
        FlatProxyStyle::new([1.0; 4], [0.5, 0.75, 1.0, 0.8], None)
    );
    assert_eq!(judgment.blend, BlendMode::Add);
    let combo = direct.entries[2];
    assert_eq!(combo.actor_insert, 2);
    assert_eq!(combo.player, 0);
    assert_eq!(combo.draws, SongLuaDirectDraws::Combo);
    assert_eq!((combo.draw_start, combo.draw_end), (0, 1));
    assert_eq!(combo.offset, [107.0, -120.0]);
    assert_eq!(
        combo.style,
        FlatProxyStyle::new([1.0; 4], [1.0, 0.5, 0.25, 0.75], None)
    );
    let second_combo = direct.entries[3];
    assert_eq!(second_combo.actor_insert, 3);
    assert_eq!(second_combo.player, 0);
    assert_eq!(second_combo.draws, SongLuaDirectDraws::Combo);
    assert_eq!((second_combo.draw_start, second_combo.draw_end), (0, 1));
    assert_eq!(second_combo.offset, [207.0, -60.0]);
}

#[test]
fn song_lua_proxy_requests_hide_captured_screen_layers() {
    let overlays = vec![
        test_capture_overlay("screen"),
        test_capture_proxy_child(0, SongLuaProxyTarget::Underlay { hidden: true }),
        test_capture_proxy_child(0, SongLuaProxyTarget::Overlay { hidden: true }),
        test_aft_overlay("screen", true),
    ];
    let overlay_states = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    let index = SongLuaProxyRequestIndex::new(&overlays);
    let mut visit_scratch = SongLuaCaptureVisitScratch::with_capacity(overlays.len());
    let requests =
        song_lua_proxy_requests_indexed(&overlays, &overlay_states, &index, &mut visit_scratch);

    assert!(requests.underlay);
    assert!(requests.overlay);
    assert!(requests.hide_underlay);
    assert!(requests.hide_overlay);
}

#[test]
fn song_lua_proxy_requests_skip_hidden_aft_capture_usage() {
    let overlays = vec![
        test_capture_overlay("cap"),
        test_capture_proxy_child(0, SongLuaProxyTarget::Combo { player_index: 0 }),
        test_aft_overlay("cap", false),
    ];
    let overlay_states = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    let index = SongLuaProxyRequestIndex::new(&overlays);
    let mut visit_scratch = SongLuaCaptureVisitScratch::with_capacity(overlays.len());
    let requests =
        song_lua_proxy_requests_indexed(&overlays, &overlay_states, &index, &mut visit_scratch);

    assert!(!requests.players[0].combo);
}

#[test]
fn song_lua_capture_marks_handle_nested_duplicates_and_cycles() {
    let mut nested = test_aft_overlay("capture-b", true);
    nested.parent_index = Some(0);
    let mut cycle = test_aft_overlay("capture-a", true);
    cycle.parent_index = Some(1);
    let overlays = vec![
        test_capture_overlay("Capture-A"),
        test_capture_overlay("Capture-B"),
        nested,
        test_capture_proxy_child(1, SongLuaProxyTarget::NoteField { player_index: 0 }),
        cycle,
        test_aft_overlay("capture-a", true),
        test_aft_overlay("capture-a", true),
    ];
    let overlay_states = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    let index = SongLuaProxyRequestIndex::new(&overlays);
    let mut visit_scratch = SongLuaCaptureVisitScratch::with_capacity(overlays.len());

    for _ in 0..2 {
        let requests =
            song_lua_proxy_requests_indexed(&overlays, &overlay_states, &index, &mut visit_scratch);
        assert!(requests.players[0].note_field);
    }

    let source = vec![Arc::<[Actor]>::from(vec![test_source_actor()])];
    let sources = [
        SongLuaPlayerProxySources {
            note_field: Some(SongLuaProxySource::new(source.as_slice())),
            ..SongLuaPlayerProxySources::default()
        },
        SongLuaPlayerProxySources::default(),
    ];
    assert_eq!(
        song_lua_replacement_active_players_indexed(
            &overlays,
            &overlay_states,
            &sources,
            &index,
            &mut visit_scratch,
        ),
        [true, false]
    );
}

#[test]
fn song_lua_proxy_index_finds_nested_player_replacement() {
    let overlays = vec![
        test_capture_overlay("PlayerCapture"),
        test_capture_proxy_child(0, SongLuaProxyTarget::Player { player_index: 0 }),
        test_aft_overlay("playercapture", true),
    ];
    let overlay_states = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    let source = vec![Arc::<[Actor]>::from(vec![test_source_actor()])];
    let sources = [
        SongLuaPlayerProxySources {
            player: Some(SongLuaProxySource::new(source.as_slice())),
            ..SongLuaPlayerProxySources::default()
        },
        SongLuaPlayerProxySources::default(),
    ];
    let index = SongLuaProxyRequestIndex::new(&overlays);
    let mut visit_scratch = SongLuaCaptureVisitScratch::with_capacity(overlays.len());

    assert_eq!(
        song_lua_replacement_active_players_indexed(
            &overlays,
            &overlay_states,
            &sources,
            &index,
            &mut visit_scratch,
        ),
        [true, false]
    );
}

#[test]
fn song_lua_dynamic_camera_scope_tracks_nested_state() {
    let mut outer = test_order_overlay(SongLuaOverlayKind::ActorFrame, None, 0);
    outer.initial_state.fov = Some(50.0);
    let mut inner = test_order_overlay(SongLuaOverlayKind::ActorFrame, Some(0), 0);
    inner.message_commands = vec![test_message_command(SongLuaOverlayStateDelta {
        fov: Some(25.0),
        ..SongLuaOverlayStateDelta::default()
    })];
    let overlays = vec![
        outer,
        inner,
        test_order_overlay(SongLuaOverlayKind::Actor, Some(1), 0),
    ];
    let mut states = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    let topology = SongLuaOverlayTopologyIndex::new(&overlays);
    assert!(topology.dynamic_camera_scope);
    assert_eq!(
        topology
            .camera_state(&states, 2)
            .and_then(|state| state.fov),
        Some(50.0),
    );

    states[1].fov = Some(25.0);
    assert_eq!(
        topology
            .camera_state(&states, 2)
            .and_then(|state| state.fov),
        Some(25.0),
    );
}

#[test]
fn song_lua_overlay_center_coords_stay_centered_under_actorframe() {
    let parent = SongLuaOverlayState {
        x: 427.0,
        y: 240.0,
        ..SongLuaOverlayState::default()
    };
    let child = SongLuaOverlayState {
        x: 427.0,
        y: 240.0,
        ..SongLuaOverlayState::default()
    };
    let composed = song_lua_overlay_compose_state(
        &SongLuaOverlayKind::ActorFrame,
        parent,
        child,
        854.0,
        480.0,
    );
    assert_eq!(composed.x, 427.0);
    assert_eq!(composed.y, 240.0);
}

#[test]
fn song_lua_overlay_root_actorframe_keeps_absolute_center_child() {
    let parent = SongLuaOverlayState::default();
    let child = SongLuaOverlayState {
        x: 427.0,
        y: 240.0,
        ..SongLuaOverlayState::default()
    };
    let composed = song_lua_overlay_compose_state(
        &SongLuaOverlayKind::ActorFrame,
        parent,
        child,
        854.0,
        480.0,
    );
    assert_eq!(composed.x, 427.0);
    assert_eq!(composed.y, 240.0);
}

#[test]
fn song_lua_overlay_local_offsets_still_compose_from_centered_actorframe() {
    let parent = SongLuaOverlayState {
        x: 427.0,
        y: 240.0,
        ..SongLuaOverlayState::default()
    };
    let child = SongLuaOverlayState {
        x: -180.0,
        y: 0.0,
        ..SongLuaOverlayState::default()
    };
    let composed = song_lua_overlay_compose_state(
        &SongLuaOverlayKind::ActorFrame,
        parent,
        child,
        854.0,
        480.0,
    );
    assert_eq!(composed.x, 247.0);
    assert_eq!(composed.y, 240.0);
}

#[test]
fn song_lua_overlay_inherits_actorframe_vibration() {
    let parent = SongLuaOverlayState {
        vibrate: true,
        effect_magnitude: [20.0, 12.0, 4.0],
        inherited_vibrate: [3.0, 2.0, 1.0],
        ..SongLuaOverlayState::default()
    };
    let child = SongLuaOverlayState {
        vibrate: true,
        effect_magnitude: [5.0, 6.0, 7.0],
        ..SongLuaOverlayState::default()
    };
    let composed = song_lua_overlay_compose_state(
        &SongLuaOverlayKind::ActorFrame,
        parent,
        child,
        854.0,
        480.0,
    );

    assert_eq!(composed.inherited_vibrate, [23.0, 14.0, 5.0]);
    assert_eq!(
        song_lua_overlay_vibrate_magnitude(composed),
        [28.0, 20.0, 12.0]
    );
}

#[test]
fn song_lua_overlay_nested_center_survives_parent_zoom_and_rotation() {
    let outer = SongLuaOverlayState {
        x: 427.0,
        y: 240.0,
        ..SongLuaOverlayState::default()
    };
    let inner = SongLuaOverlayState {
        zoom: 1.5,
        rot_z_deg: 67.0,
        ..SongLuaOverlayState::default()
    };
    let inner =
        song_lua_overlay_compose_state(&SongLuaOverlayKind::ActorFrame, outer, inner, 854.0, 480.0);
    let circle = song_lua_overlay_compose_state(
        &SongLuaOverlayKind::ActorFrame,
        inner,
        SongLuaOverlayState::default(),
        854.0,
        480.0,
    );

    assert!((circle.x - 427.0).abs() <= 0.000_1);
    assert!((circle.y - 240.0).abs() <= 0.000_1);
}

#[test]
fn song_lua_overlay_rotation_matches_rage_matrix_xyz() {
    let x_axis = song_lua_overlay_local_transform([0.0, 90.0, 0.0], 0.0, 0.0)
        * Vector4::new(1.0, 0.0, 0.0, 1.0);
    assert!(x_axis.x.abs() <= 0.000_1);
    assert!((x_axis.z - 1.0).abs() <= 0.000_1);

    let y_axis = song_lua_overlay_local_transform([90.0, 0.0, 0.0], 0.0, 0.0)
        * Vector4::new(0.0, 1.0, 0.0, 1.0);
    assert!(y_axis.y.abs() <= 0.000_1);
    assert!((y_axis.z + 1.0).abs() <= 0.000_1);

    let z_axis = song_lua_overlay_local_transform([0.0, 0.0, 90.0], 0.0, 0.0)
        * Vector4::new(1.0, 0.0, 0.0, 1.0);
    assert!(z_axis.x.abs() <= 0.000_1);
    assert!((z_axis.y - 1.0).abs() <= 0.000_1);

    let [rx, ry, rz] = [40.0_f32, 30.0, 17.0].map(f32::to_radians);
    let (sx, cx) = rx.sin_cos();
    let (sy, cy) = ry.sin_cos();
    let (sz, cz) = rz.sin_cos();
    let point = Vector4::new(123.0, -45.0, 19.0, 1.0);
    // This is RageMatrixRotationXYZ plus RageVec4TransformCoord, copied as
    // equations so a combined-axis order regression cannot pass unnoticed.
    let rage = Vector4::new(
        (cz * cy) * point.x + (-sz * cy) * point.y + (-sy) * point.z,
        (cz * sy * sx + sz * cx) * point.x
            + (-sz * sy * sx + cz * cx) * point.y
            + (cy * sx) * point.z,
        (cz * sy * cx - sz * sx) * point.x
            + (-sz * sy * cx - cz * sx) * point.y
            + (cy * cx) * point.z,
        1.0,
    );
    let actual = song_lua_overlay_local_transform([40.0, 30.0, 17.0], 0.0, 0.0) * point;
    assert!((actual - rage).abs().max_element() <= 0.000_1);
}

#[test]
fn song_lua_overlay_perspective_keeps_logical_center_fixed() {
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    let camera = SongLuaOverlayState {
        fov: Some(120.0),
        ..SongLuaOverlayState::default()
    };
    let view_proj = song_lua_overlay_view_proj(camera, 854.0, 480.0)
        .expect("positive FOV should produce a projection");
    let projected = song_lua_project_overlay_point(view_proj, [427.0, 240.0, 0.0])
        .expect("logical center should be projectable");

    assert!((projected[0] - 0.5 * screen_width()).abs() <= 0.000_1);
    assert!((projected[1] - 0.5 * screen_height()).abs() <= 0.000_1);
}

#[test]
fn song_lua_projection_preserves_vertices_behind_camera_for_gpu_clipping() {
    let camera = SongLuaOverlayState {
        fov: Some(120.0),
        ..SongLuaOverlayState::default()
    };
    let view_proj = song_lua_overlay_view_proj(camera, 854.0, 480.0)
        .expect("positive FOV should produce a projection");
    let model = Matrix4::from_translation(Vector3::new(427.0, 240.0, 0.0))
        * song_lua_overlay_local_transform([50.0, 20.0, 397.350_98], 0.0, 0.0);
    let local_transform = song_lua_projected_local_transform(view_proj, model);
    let screen_projection = glam::camera::rh::proj::opengl::orthographic(
        0.0,
        screen_width(),
        screen_height(),
        0.0,
        -1.0,
        1.0,
    );
    let corners = [
        Vector4::new(-266.25, -266.25, 0.0, 1.0),
        Vector4::new(266.25, -266.25, 0.0, 1.0),
        Vector4::new(266.25, 266.25, 0.0, 1.0),
        Vector4::new(-266.25, 266.25, 0.0, 1.0),
    ];
    let mut crosses_camera = false;
    for corner in corners {
        let native_clip = view_proj * model * corner;
        let rendered_clip = screen_projection * local_transform * corner;
        crosses_camera |= native_clip.w <= 0.0;
        assert!((native_clip - rendered_clip).abs().max_element() <= 0.000_2);
    }
    assert!(
        crosses_camera,
        "the Step Your Game Up circle regression must exercise GPU clipping"
    );
}

#[test]
fn song_lua_projection_matches_step_your_game_up_fixture() {
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    let fixture_path = workspace_root().join(
            "tests/fixtures/itgmania-song-lua/Step Your Game Up (Director's Cut)/stepyourgameup.ssc.semantic.json",
        );
    let fixture: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&fixture_path)
            .unwrap_or_else(|error| panic!("could not read {}: {error}", fixture_path.display())),
    )
    .unwrap_or_else(|error| panic!("invalid {}: {error}", fixture_path.display()));
    let track = fixture["projected_vertex_tracks"]
        .as_array()
        .and_then(|tracks| {
            tracks.iter().find(|track| {
                track["texture"]
                    .as_str()
                    .is_some_and(|texture| texture.ends_with("tpe3 circ 2.png"))
            })
        })
        .expect("fixture has no projected tpe3 circle");
    let sample = track["samples"]
        .as_array()
        .and_then(|samples| {
            samples.iter().find(|sample| {
                sample[0]
                    .as_f64()
                    .is_some_and(|beat| (beat - 200.0).abs() <= 0.05)
            })
        })
        .expect("fixture has no tpe3 circle sample near beat 200");
    let camera = sample[7].as_array().expect("fixture sample has no camera");
    let camera_state = SongLuaOverlayState {
        fov: Some(camera[0].as_f64().expect("camera FOV is not numeric") as f32),
        vanishpoint: Some([
            camera[1].as_f64().expect("camera X is not numeric") as f32,
            camera[2].as_f64().expect("camera Y is not numeric") as f32,
        ]),
        ..SongLuaOverlayState::default()
    };
    let view_proj = song_lua_overlay_view_proj(camera_state, 854.0, 480.0)
        .expect("fixture camera should be projectable");
    let world = sample[4]
        .as_array()
        .expect("fixture sample has no world vertices");
    let expected_clip = sample[5]
        .as_array()
        .expect("fixture sample has no clip vertices");
    let mut crosses_camera = false;
    for (world, expected) in world.iter().zip(expected_clip) {
        let world = world.as_array().expect("world vertex is not an array");
        let expected = expected.as_array().expect("clip vertex is not an array");
        let actual = view_proj
            * Vector4::new(
                world[0].as_f64().expect("world X is not numeric") as f32,
                world[1].as_f64().expect("world Y is not numeric") as f32,
                world[2].as_f64().expect("world Z is not numeric") as f32,
                1.0,
            );
        let expected = Vector4::new(
            expected[0].as_f64().expect("clip X is not numeric") as f32,
            expected[1].as_f64().expect("clip Y is not numeric") as f32,
            expected[2].as_f64().expect("clip Z is not numeric") as f32,
            expected[3].as_f64().expect("clip W is not numeric") as f32,
        );
        crosses_camera |= actual.w <= 0.0;
        assert!((actual - expected).abs().max_element() <= 0.002);
    }
    assert!(
        crosses_camera,
        "fixture no longer exercises near-plane clipping"
    );
}

#[test]
fn active_exact_overlay_ease_overrides_sampled_update_value() {
    let from = deadsync_song_lua::gameplay::song_lua_runtime_overlay_state_delta(
        SongLuaOverlayStateDelta {
            x: Some(-235.0),
            ..SongLuaOverlayStateDelta::default()
        },
    );
    let to = deadsync_song_lua::gameplay::song_lua_runtime_overlay_state_delta(
        SongLuaOverlayStateDelta {
            x: Some(-260.0),
            ..SongLuaOverlayStateDelta::default()
        },
    );
    let eases = [
        deadsync_gameplay::build_song_lua_overlay_ease_window_runtime(
            0,
            1.0,
            3.0,
            3.0,
            None,
            from,
            to,
            Some("linear"),
            None,
            None,
        ),
    ];
    let ranges = [0..1];
    let mut start = SongLuaOverlayState {
        x: 1.0,
        ..SongLuaOverlayState::default()
    };
    reapply_active_song_lua_overlay_runtime_eases_for(1.0, 0, &eases, &ranges, &mut start);
    assert!((start.x + 235.0).abs() <= f32::EPSILON);

    let mut middle = SongLuaOverlayState {
        x: 1.0,
        ..SongLuaOverlayState::default()
    };
    reapply_active_song_lua_overlay_runtime_eases_for(2.0, 0, &eases, &ranges, &mut middle);
    assert!((middle.x + 247.5).abs() <= 0.000_1);
}

#[test]
fn song_lua_overlay_culls_fully_offscreen_sprite_frame() {
    let hidden = SongLuaOverlayState {
        x: 1_014.0,
        y: 240.0,
        ..SongLuaOverlayState::default()
    };
    let edge = SongLuaOverlayState {
        x: 850.0,
        y: 240.0,
        ..SongLuaOverlayState::default()
    };
    let transparent_edge_sliver = SongLuaOverlayState {
        x: 978.8,
        y: 285.6,
        zoom: 0.94,
        ..SongLuaOverlayState::default()
    };
    assert!(song_lua_overlay_sprite_offscreen(
        hidden,
        [288.0, 352.0],
        854.0,
        480.0,
    ));
    assert!(!song_lua_overlay_sprite_offscreen(
        edge,
        [288.0, 352.0],
        854.0,
        480.0,
    ));
    assert!(song_lua_overlay_sprite_offscreen(
        transparent_edge_sliver,
        [288.0, 352.0],
        854.0,
        480.0,
    ));
}

#[test]
fn song_lua_overlay_nested_rotated_skew_keeps_affine_transform() {
    let outer = SongLuaOverlayState {
        x: 427.0,
        y: 240.0,
        rot_z_deg: -90.0,
        ..SongLuaOverlayState::default()
    };
    let inner = SongLuaOverlayState {
        x: -100.0,
        y: 12.0,
        skew_x: 0.25,
        ..SongLuaOverlayState::default()
    };
    let sprite = SongLuaOverlayState {
        x: 20.0,
        y: 30.0,
        rot_z_deg: 90.0,
        ..SongLuaOverlayState::default()
    };
    let expected_inner_linear =
        song_lua_overlay_linear_2d(outer) * song_lua_overlay_linear_2d(inner);
    let expected_inner_pos = Vector2::new(outer.x, outer.y)
        + song_lua_overlay_linear_2d(outer) * Vector2::new(inner.x, inner.y);
    let expected_sprite_pos =
        expected_inner_pos + expected_inner_linear * Vector2::new(sprite.x, sprite.y);
    let expected_linear = expected_inner_linear * song_lua_overlay_linear_2d(sprite);

    let inner =
        song_lua_overlay_compose_state(&SongLuaOverlayKind::ActorFrame, outer, inner, 854.0, 480.0);
    let sprite = song_lua_overlay_compose_state(
        &SongLuaOverlayKind::ActorFrame,
        inner,
        sprite,
        854.0,
        480.0,
    );

    for (actual, expected) in song_lua_overlay_linear_2d(sprite)
        .to_cols_array()
        .into_iter()
        .zip(expected_linear.to_cols_array())
    {
        assert!((actual - expected).abs() <= 0.000_1);
    }
    assert!((sprite.x - expected_sprite_pos.x).abs() <= 0.000_1);
    assert!((sprite.y - expected_sprite_pos.y).abs() <= 0.000_1);
}

#[test]
fn song_lua_overlay_texture_translate_stacks_from_parent() {
    let parent = SongLuaOverlayState {
        texcoord_offset: Some([0.25, 0.5]),
        ..SongLuaOverlayState::default()
    };
    let child = SongLuaOverlayState {
        texcoord_offset: Some([0.125, -0.25]),
        ..SongLuaOverlayState::default()
    };
    let composed = song_lua_overlay_compose_state(
        &SongLuaOverlayKind::ActorFrame,
        parent,
        child,
        854.0,
        480.0,
    );
    assert_eq!(composed.texcoord_offset, Some([0.375, 0.25]));
}

#[test]
fn song_lua_aft_capture_uses_local_proxy_origin() {
    let root = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::ActorFrame,
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState {
            x: 427.0,
            y: 240.0,
            ..SongLuaOverlayState::default()
        },
        message_commands: Vec::new(),
    };
    let mut capture = test_capture_overlay("cap");
    capture.parent_index = Some(0);
    let overlays = vec![
        root,
        capture,
        test_capture_proxy_child(1, SongLuaProxyTarget::Player { player_index: 0 }),
    ];
    let local_states = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    let overlay_states = song_lua_overlay_states_from_local(&overlays, &local_states, 854.0, 480.0);
    assert_eq!(overlay_states[2].x, 427.0);
    assert_eq!(overlay_states[2].y, 240.0);

    let source = vec![Arc::<[Actor]>::from(vec![test_source_actor()])];
    let proxy_sources = SongLuaScreenProxySources {
        players: [
            SongLuaPlayerProxySources {
                player: Some(SongLuaProxySource::new(source.as_slice())),
                ..SongLuaPlayerProxySources::default()
            },
            SongLuaPlayerProxySources::default(),
        ],
        ..SongLuaScreenProxySources::default()
    };
    let mut order_cache = song_lua_overlay_order_cache_from(&overlays, &[]);
    let topology_index = SongLuaOverlayTopologyIndex::new(&overlays);
    let mut capture_states = Vec::new();
    let mut order_scratch = Vec::new();
    let mut projected_mesh_scratch = song_lua_projected_mesh_scratch_for(&overlays);
    let actors = song_lua_capture_children(
        &overlays,
        &overlay_states,
        &local_states,
        &mut order_cache,
        &topology_index,
        &AssetManager::new(),
        1,
        &proxy_sources,
        854.0,
        480.0,
        &mut capture_states,
        &mut order_scratch,
        &mut projected_mesh_scratch,
    );

    match actors.as_slice() {
        [Actor::SharedFrame { offset, tint, .. }] => {
            assert_eq!(*offset, [0.0, 0.0]);
            assert_eq!(*tint, [1.0; 4]);
        }
        other => panic!("expected one direct capture proxy, got {other:?}"),
    }
}

#[test]
fn song_lua_aft_capture_draws_hidden_local_actor_target() {
    let mut target = test_order_overlay(SongLuaOverlayKind::ActorFrame, None, 0);
    target.initial_state.visible = false;
    let mut marker = test_order_overlay(SongLuaOverlayKind::Quad, Some(0), 0);
    marker.initial_state.size = Some([64.0, 32.0]);
    marker.initial_state.diffuse = [0.8, 0.2, 0.1, 0.7];
    let overlays = vec![
        target,
        marker,
        test_capture_overlay("cap"),
        test_capture_proxy_child(2, SongLuaProxyTarget::Actor { overlay_index: 0 }),
    ];
    let local_states = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    let overlay_states = song_lua_overlay_states_from_local(&overlays, &local_states, 640.0, 480.0);
    assert!(!overlay_states[0].visible);
    assert!(!overlay_states[1].visible);

    let mut order_cache = song_lua_overlay_order_cache_from(&overlays, &[]);
    let topology_index = SongLuaOverlayTopologyIndex::new(&overlays);
    let mut capture_states = Vec::new();
    let mut order_scratch = Vec::new();
    let mut projected_mesh_scratch = song_lua_projected_mesh_scratch_for(&overlays);
    let actors = song_lua_capture_children(
        &overlays,
        &overlay_states,
        &local_states,
        &mut order_cache,
        &topology_index,
        &AssetManager::new(),
        2,
        &SongLuaScreenProxySources::default(),
        640.0,
        480.0,
        &mut capture_states,
        &mut order_scratch,
        &mut projected_mesh_scratch,
    );

    let [Actor::SharedFrame { children, .. }] = actors.as_slice() else {
        panic!("expected one local actor proxy, got {actors:?}");
    };
    assert!(matches!(children.as_ref(), [Actor::Sprite { .. }]));
}

#[test]
fn song_lua_hidden_screen_capture_is_not_drawn_twice() {
    let mut hidden_dest = Some(SongLuaActorSegments::new());
    let mut hidden_actors = vec![test_source_actor()];
    song_lua_capture_new_actors(&mut hidden_dest, &mut hidden_actors, 0, None, false);
    assert!(hidden_actors.is_empty());
    assert_eq!(hidden_dest.as_ref().map_or(0, SmallVec::len), 1);

    let mut visible_dest = Some(SongLuaActorSegments::new());
    let mut visible_actors = vec![test_source_actor()];
    song_lua_capture_new_actors(&mut visible_dest, &mut visible_actors, 0, None, true);
    assert!(matches!(
        visible_actors.as_slice(),
        [Actor::SharedFrame { .. }]
    ));
    assert_eq!(visible_dest.as_ref().map_or(0, SmallVec::len), 1);
}

#[test]
fn song_lua_actor_proxy_keeps_overlay_z_layer() {
    let source = vec![Arc::<[Actor]>::from(vec![test_source_actor()])];
    let actor = song_lua_build_proxy_actor(
        SongLuaOverlayState::default(),
        1234,
        source.as_slice(),
        640.0,
        480.0,
    )
    .expect("actor proxy should render with a source");

    let Actor::SharedFrame { z, children, .. } = actor else {
        panic!("expected direct shared proxy actor");
    };
    assert_eq!(z, 1234);
    assert_eq!(children.len(), 1);
}

#[test]
fn song_lua_actor_proxy_zoom_fills_display_sized_aft() {
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    let source = [Arc::<[Actor]>::from([test_source_actor()])];
    let display_width = 1600.0;
    let display_height = 900.0;
    let scale = display_width / 854.0;
    let state = SongLuaOverlayState {
        zoom: scale,
        ..SongLuaOverlayState::default()
    };

    let actor = song_lua_build_proxy_actor_in_space_with_scratch(
        state,
        1234,
        SongLuaProxySource::new(&source),
        854.0,
        480.0,
        display_width,
        display_height,
        None,
    )
    .expect("zoomed ActorProxy should render");
    let Actor::SharedTransform { transform, .. } = actor else {
        panic!("zoomed ActorProxy must preserve its transform");
    };

    let top_left = Vector3::new(-display_width * 0.5, display_height * 0.5, 0.0);
    let source_bottom_right = top_left + Vector3::new(854.0, -480.0, 0.0);
    let transformed_top_left = transform.transform_point3(top_left);
    let transformed_bottom_right = transform.transform_point3(source_bottom_right);
    assert!((transformed_top_left.x - top_left.x).abs() < 0.001);
    assert!((transformed_top_left.y - top_left.y).abs() < 0.001);
    assert!((transformed_bottom_right.x - display_width * 0.5).abs() < 0.001);
    assert!(
        (transformed_bottom_right.y - 480.0f32.mul_add(-scale, display_height * 0.5)).abs() < 0.001
    );
}

#[test]
fn song_lua_actor_proxy_keeps_source_z_inside_proxy_layer() {
    let source = vec![Arc::<[Actor]>::from(vec![Actor::Frame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children: vec![Actor::Frame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children: Vec::new(),
            background: None,
            z: 96,
        }],
        background: None,
        z: 83,
    }])];
    let actor = song_lua_build_proxy_actor(
        SongLuaOverlayState::default(),
        1234,
        source.as_slice(),
        640.0,
        480.0,
    )
    .expect("actor proxy should render with a source");

    let Actor::SharedFrame { z, children, .. } = actor else {
        panic!("expected direct shared proxy actor");
    };
    assert_eq!(z, 1234);
    let [Actor::Frame { z, children, .. }] = children.as_ref() else {
        panic!("expected local source frame");
    };
    assert_eq!(*z, 0);
    let [Actor::Frame { z, .. }] = children.as_slice() else {
        panic!("expected local source child frame");
    };
    assert_eq!(*z, 0);
}

#[test]
fn song_lua_actor_proxy_preserves_source_z_order_locally() {
    let mut low = test_source_actor();
    let mut high = test_source_actor();
    if let Actor::Frame { z, .. } = &mut low {
        *z = -20;
    }
    if let Actor::Frame { offset, z, .. } = &mut high {
        *offset = [99.0, 0.0];
        *z = 20;
    }
    let source = vec![Arc::<[Actor]>::from(vec![high, low])];
    let actor = song_lua_build_proxy_actor(
        SongLuaOverlayState::default(),
        1234,
        source.as_slice(),
        640.0,
        480.0,
    )
    .expect("actor proxy should render with a source");

    let Actor::SharedFrame { children, .. } = actor else {
        panic!("expected direct shared proxy actor");
    };
    let [
        Actor::Frame {
            offset: first_offset,
            ..
        },
        Actor::Frame {
            offset: second_offset,
            z,
            ..
        },
    ] = children.as_ref()
    else {
        panic!("expected sorted local source frames");
    };
    assert_eq!(*first_offset, [0.0, 0.0]);
    assert_eq!(*second_offset, [99.0, 0.0]);
    assert_eq!(*z, 0);
    assert_eq!(
        children
            .iter()
            .map(|actor| match actor {
                Actor::Frame { z, .. } => *z,
                other => panic!("expected source frame, got {other:?}"),
            })
            .collect::<Vec<_>>(),
        [0, 0]
    );
}

#[test]
fn song_lua_actor_proxy_flattens_retained_fragment_z() {
    let mut header = test_source_actor();
    let mut filter = test_source_actor();
    if let Actor::Frame { offset, z, .. } = &mut header {
        *offset = [83.0, 0.0];
        *z = 83;
    }
    if let Actor::Frame { offset, z, .. } = &mut filter {
        *offset = [-99.0, 0.0];
        *z = -99;
    }
    let retained = Actor::RetainedFrame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Fill, SizeSpec::Fill],
        frame: Arc::new(RetainedActorFrame::new(vec![header, filter])),
        z: 0,
        tint: [1.0; 4],
        blend: None,
        visible: true,
    };
    let source = vec![Arc::<[Actor]>::from(vec![retained])];
    let actor = song_lua_build_proxy_actor(
        SongLuaOverlayState::default(),
        12,
        source.as_slice(),
        640.0,
        480.0,
    )
    .expect("retained source should render through a proxy");

    let Actor::SharedFrame { children, .. } = actor else {
        panic!("expected direct shared proxy actor");
    };
    let [
        Actor::Frame {
            offset: first_offset,
            z: first_z,
            ..
        },
        Actor::Frame {
            offset: second_offset,
            z: second_z,
            ..
        },
    ] = children.as_ref()
    else {
        panic!("expected flattened retained children, got {children:?}");
    };
    assert_eq!(*first_offset, [-99.0, 0.0]);
    assert_eq!(*second_offset, [83.0, 0.0]);
    assert_eq!([*first_z, *second_z], [0, 0]);
}

#[test]
fn song_lua_actor_proxy_keeps_camera_scope_around_sorted_source() {
    let mut low = test_source_actor();
    let mut high = test_source_actor();
    if let Actor::Frame { z, .. } = &mut low {
        *z = -20;
    }
    if let Actor::Frame { offset, z, .. } = &mut high {
        *offset = [99.0, 0.0];
        *z = 20;
    }
    let source = vec![Arc::<[Actor]>::from(vec![
        Actor::CameraPush {
            view_proj: Matrix4::IDENTITY,
        },
        high,
        low,
        Actor::CameraPop,
    ])];
    let actor = song_lua_build_proxy_actor(
        SongLuaOverlayState::default(),
        1234,
        source.as_slice(),
        640.0,
        480.0,
    )
    .expect("actor proxy should render with a source");

    let Actor::SharedFrame { children, .. } = actor else {
        panic!("expected direct shared proxy actor");
    };
    let [
        Actor::CameraPush { .. },
        Actor::Frame {
            offset: first_offset,
            ..
        },
        Actor::Frame {
            offset: second_offset,
            z,
            ..
        },
        Actor::CameraPop,
    ] = children.as_ref()
    else {
        panic!("expected sorted actors inside original camera scope");
    };
    assert_eq!(*first_offset, [0.0, 0.0]);
    assert_eq!(*second_offset, [99.0, 0.0]);
    assert_eq!(*z, 0);
}

#[test]
fn song_lua_proxy_reserve_tracks_target_size_and_shape() {
    let proxy = |target| SongLuaOverlayActor {
        kind: SongLuaOverlayKind::ActorProxy { target },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let small = vec![
        proxy(SongLuaProxyTarget::Combo { player_index: 0 }),
        proxy(SongLuaProxyTarget::Overlay { hidden: false }),
    ];
    let note_field = vec![
        proxy(SongLuaProxyTarget::Judgment { player_index: 0 }),
        proxy(SongLuaProxyTarget::NoteField { player_index: 0 }),
    ];
    let player = vec![
        proxy(SongLuaProxyTarget::NoteField { player_index: 0 }),
        proxy(SongLuaProxyTarget::Player { player_index: 0 }),
    ];

    assert_eq!(
        song_lua_proxy_pool_counts(&small, &SongLuaProxyRequestIndex::new(&small)),
        [1, 0, 0, 1]
    );
    assert_eq!(
        song_lua_proxy_pool_counts(&note_field, &SongLuaProxyRequestIndex::new(&note_field)),
        [1, 1, 0, 0]
    );
    assert_eq!(
        song_lua_proxy_pool_counts(&player, &SongLuaProxyRequestIndex::new(&player)),
        [0, 1, 1, 0]
    );

    let mut scratch = SongLuaProxyActorScratch::with_proxy_counts_and_banks(0, [1, 1, 1, 1], 1);
    scratch.begin_frame();
    assert_eq!(
        scratch.reserve_proxy_group(SONG_LUA_PLAYER_PROXY_CLASS),
        Some(SongLuaProxyGroup {
            frame_index: None,
            segment_start: 2,
        })
    );
    assert_eq!(
        scratch.reserve_proxy_group(SONG_LUA_SMALL_PROXY_CLASS),
        Some(SongLuaProxyGroup {
            frame_index: None,
            segment_start: 0,
        })
    );
    assert_eq!(
        scratch.reserve_proxy_group(SONG_LUA_NOTEFIELD_PROXY_CLASS),
        Some(SongLuaProxyGroup {
            frame_index: None,
            segment_start: 1,
        })
    );
    assert_eq!(
        scratch.reserve_proxy_group(SONG_LUA_MULTI_PROXY_CLASS),
        Some(SongLuaProxyGroup {
            frame_index: Some(0),
            segment_start: 3,
        })
    );
    let bank = &scratch.banks[0];
    let segments = &bank.proxy_segments;
    assert_eq!(segments.len(), 8);
    assert_eq!(bank.proxy_frames.len(), 1);
    assert_eq!(
        segments[0].stats().capacity,
        SONG_LUA_SCREEN_CAPTURE_CAPACITY
    );
    assert_eq!(
        segments[1].stats().capacity,
        NOTEFIELD_ACTOR_SCRATCH_CAPACITY
    );
    assert_eq!(segments[2].stats().capacity, PLAYER_ACTOR_SCRATCH_CAPACITY);
    assert!(
        segments[3..]
            .iter()
            .all(|segment| { segment.stats().capacity == SONG_LUA_SCREEN_CAPTURE_CAPACITY })
    );
}

#[test]
fn song_lua_proxy_prewarm_reuses_local_z_camera_storage() {
    let mut low = test_source_actor();
    let mut high = test_source_actor();
    if let Actor::Frame { z, .. } = &mut low {
        *z = -20;
    }
    if let Actor::Frame { offset, z, .. } = &mut high {
        *offset = [99.0, 0.0];
        *z = 20;
    }
    let source = [Arc::<[Actor]>::from(vec![
        Actor::CameraPush {
            view_proj: Matrix4::IDENTITY,
        },
        high,
        low,
        Actor::CameraPop,
    ])];
    let mut scratch = SongLuaProxyActorScratch::with_proxy_counts_and_banks(0, [1, 0, 0, 0], 1);

    for _ in 0..2 {
        scratch.begin_frame();
        let actor = song_lua_build_proxy_actor_with_scratch(
            SongLuaOverlayState::default(),
            1234,
            SongLuaProxySource::new(&source),
            640.0,
            480.0,
            Some(&mut scratch),
        )
        .expect("prewarmed proxy should render");
        let Actor::SharedFrame { z, children, .. } = actor else {
            panic!("expected prewarmed shared proxy frame");
        };
        assert_eq!(z, 1234);
        let [Actor::Frame { children, .. }] = children.as_ref() else {
            panic!("expected reusable normalized source backing");
        };
        let [
            Actor::CameraPush { .. },
            Actor::Frame {
                offset: first_offset,
                z: first_z,
                ..
            },
            Actor::Frame {
                offset: second_offset,
                z: second_z,
                ..
            },
            Actor::CameraPop,
        ] = children.as_slice()
        else {
            panic!("expected camera markers around the sorted local run");
        };
        assert_eq!(*first_offset, [0.0, 0.0]);
        assert_eq!(*second_offset, [99.0, 0.0]);
        assert_eq!([*first_z, *second_z], [0, 0]);
    }

    let bank = &scratch.banks[0];
    assert_eq!(bank.proxy_segments[0].stats().growths, 0);
    assert_eq!(bank.proxy_segments[0].stats().replacements, 0);
}

#[test]
fn song_lua_proxy_prewarm_reuses_segment_join_frame() {
    let source: [Arc<[Actor]>; 3] = std::array::from_fn(|_| Arc::from([Actor::CameraPop]));
    let mut scratch = SongLuaProxyActorScratch::with_proxy_counts_and_banks(0, [0, 0, 0, 1], 1);

    for _ in 0..2 {
        scratch.begin_frame();
        let actor = song_lua_build_proxy_frame_actor_with_scratch(
            SongLuaOverlayState::default(),
            1234,
            SongLuaProxySource::new(&source),
            640.0,
            480.0,
            Some(&mut scratch),
        )
        .expect("prewarmed segmented proxy should render");
        let Actor::SharedFrame { z, children, .. } = actor else {
            panic!("expected prewarmed outer proxy frame");
        };
        assert_eq!(z, 1234);
        assert_eq!(children.len(), source.len());
        assert!(
            children
                .iter()
                .all(|actor| matches!(actor, Actor::SharedFrame { .. }))
        );
    }

    assert_eq!(scratch.banks[0].proxy_frames[0]._replacements, 0);
}

#[test]
fn song_lua_capture_style_tints_sprite_glow() {
    let actor = Actor::Sprite {
        align: [0.5, 0.5],
        offset: [0.0, 0.0],
        world_z: 0.0,
        size: [SizeSpec::Px(16.0), SizeSpec::Px(16.0)],
        source: SpriteSource::Solid,
        tint: [0.8, 0.6, 0.4, 0.5],
        glow: [0.5, 0.25, 1.0, 0.4],
        z: 2,
        cell: None,
        grid: None,
        uv_rect: None,
        visible: true,
        flip_x: false,
        flip_y: false,
        cropleft: 0.0,
        cropright: 0.0,
        croptop: 0.0,
        cropbottom: 0.0,
        fadeleft: 0.0,
        faderight: 0.0,
        fadetop: 0.0,
        fadebottom: 0.0,
        blend: BlendMode::Alpha,
        mask_source: false,
        mask_dest: false,
        rot_x_deg: 0.0,
        rot_y_deg: 0.0,
        rot_z_deg: 0.0,
        skew: [0.0, 0.0],
        local_offset: [0.0, 0.0],
        local_offset_rot_sin_cos: [0.0, 1.0],
        texcoordvelocity: None,
        animate: false,
        state_delay: 0.0,
        scale: [1.0, 1.0],
        shadow_len: [0.0, 0.0],
        shadow_color: [0.2, 0.4, 0.6, 0.5],
        effect: deadlib_present::anim::EffectState::default(),
    };

    let styled =
        song_lua_style_capture_actor(actor, [0.5, 0.25, 0.1, 0.5], Some(BlendMode::Add), 7);

    let Actor::Sprite {
        tint,
        glow,
        shadow_color,
        blend,
        z,
        ..
    } = styled
    else {
        panic!("expected sprite actor");
    };
    assert_eq!(tint, [0.4, 0.15, 0.040_000_003, 0.25]);
    assert_eq!(glow, [0.25, 0.0625, 0.1, 0.2]);
    assert_eq!(shadow_color, [0.1, 0.1, 0.060_000_002, 0.25]);
    assert_eq!(blend, BlendMode::Add);
    assert_eq!(z, 9);
}

#[test]
fn song_lua_capture_style_preserves_shadow_and_styles_child() {
    let actor = Actor::Shadow {
        len: [2.0, -3.0],
        color: [0.8, 0.6, 0.4, 0.5],
        child: Box::new(Actor::Mesh {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Px(1.0), SizeSpec::Px(1.0)],
            tint: [0.8, 0.6, 0.4, 0.5],
            vertices: Arc::from([]),
            visible: true,
            blend: BlendMode::Alpha,
            z: 3,
        }),
    };

    let styled = song_lua_style_capture_actor(actor, [0.5, 0.25, 0.1, 0.5], None, 4);

    let Actor::Shadow { len, color, child } = styled else {
        panic!("expected shadow actor");
    };
    assert_eq!(len, [2.0, -3.0]);
    assert_eq!(color, [0.4, 0.15, 0.040_000_003, 0.25]);
    let Actor::Mesh { tint, z, .. } = child.as_ref() else {
        panic!("expected styled mesh child");
    };
    assert_eq!(*tint, [0.4, 0.15, 0.040_000_003, 0.25]);
    assert_eq!(*z, 7);
}

#[test]
fn song_lua_capture_style_shares_mesh_vertices_and_composes_tint() {
    let vertices = Arc::<[MeshVertex]>::from(vec![MeshVertex {
        pos: [0.0, 0.0],
        color: [0.8, 0.6, 0.4, 0.5],
    }]);
    let actor = Actor::Mesh {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Px(1.0), SizeSpec::Px(1.0)],
        tint: [0.8, 0.6, 0.4, 0.5],
        vertices: Arc::clone(&vertices),
        visible: true,
        blend: BlendMode::Alpha,
        z: 3,
    };

    let styled = song_lua_style_capture_actor(actor, [0.5, 0.25, 0.1, 0.5], None, 4);

    let Actor::Mesh {
        vertices: styled_vertices,
        tint,
        blend,
        z,
        ..
    } = styled
    else {
        panic!("expected mesh actor");
    };
    assert!(Arc::ptr_eq(&styled_vertices, &vertices));
    assert_eq!(styled_vertices[0].color, [0.8, 0.6, 0.4, 0.5]);
    assert_eq!(tint, [0.4, 0.15, 0.040_000_003, 0.25]);
    assert_eq!(blend, BlendMode::Alpha);
    assert_eq!(z, 7);
}

#[test]
fn song_lua_capture_style_tints_textured_mesh() {
    let actor = Actor::TexturedMesh {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        world_z: 0.0,
        size: [SizeSpec::Px(1.0), SizeSpec::Px(1.0)],
        local_transform: Matrix4::IDENTITY,
        texture: Arc::from("mesh"),
        tint: [0.8, 0.6, 0.4, 0.5],
        glow: [0.5, 0.25, 1.0, 0.4],
        vertices: Arc::from(vec![TexturedMeshVertex::default(); 3]),
        geom_cache_key: INVALID_TMESH_CACHE_KEY,
        uv_scale: [1.0, 1.0],
        uv_offset: [0.0, 0.0],
        uv_tex_shift: [0.0, 0.0],
        depth_test: false,
        visible: true,
        blend: BlendMode::Alpha,
        z: 3,
    };

    let styled = song_lua_style_capture_actor(actor, [0.5, 0.25, 0.1, 0.5], None, 4);

    let Actor::TexturedMesh {
        tint,
        glow,
        blend,
        z,
        ..
    } = styled
    else {
        panic!("expected textured mesh actor");
    };
    assert_eq!(tint, [0.4, 0.15, 0.040_000_003, 0.25]);
    assert_eq!(glow, [0.25, 0.0625, 0.1, 0.2]);
    assert_eq!(blend, BlendMode::Alpha);
    assert_eq!(z, 7);
}

#[test]
fn aft_capture_scratch_prewarms_both_frame_banks() {
    let overlays = vec![
        test_capture_overlay("CaptureAFT"),
        test_capture_proxy_child(0, SongLuaProxyTarget::Player { player_index: 0 }),
        test_aft_overlay("CaptureAFT", true),
    ];
    let topology = SongLuaOverlayTopologyIndex::new(&overlays);
    let scratch = SongLuaAftCaptureScratch::new(&overlays, &topology);
    let banks = scratch.slots[0].as_ref().expect("AFT scratch banks");

    assert_eq!(song_lua_aft_capture_capacity(&overlays, &topology, 0), 5);
    assert!(banks.iter().all(|bank| bank.capacity() >= 5));
    assert!(banks.iter().all(|bank| bank.stats().growths == 0));
}

fn compile_display_size_fixture(display_size: (u32, u32)) -> CompiledSongLua {
    crate::tests::init_paths();
    let simfile = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/song_lua/display-size.ssc");
    let song = deadsync_simfile::app_runtime::parse_song_for_test(&simfile, 0.0).unwrap();
    let chart = Arc::new(song.charts[0].clone());
    std::thread::spawn(move || {
        assert_eq!(current_window_px(), (0, 0));
        let prepared = prepare_fixture(
            &song,
            &[chart.clone(), chart],
            [&TimingData::default(); MAX_PLAYERS],
            &std::array::from_fn(|_| profile_data::Profile::default()),
            &[ScrollSpeedSetting::XMod(1.0); MAX_PLAYERS],
            1.0,
            GameplayViewport::design(),
            display_size,
            &GameplaySession::default(),
            &GameplayConfig::default(),
            BackendType::VulkanWgpu,
        );
        prepared.primary.unwrap().compiled
    })
    .join()
    .unwrap()
}

#[test]
fn song_lua_preload_preserves_display_size_on_worker() {
    for display_size in [(640, 480), (1600, 900), (3840, 1080)] {
        let compiled = compile_display_size_fixture(display_size);
        assert_eq!(
            [compiled.screen_width, compiled.screen_height],
            [854.0, 480.0]
        );
        let captures: Vec<_> = compiled
            .overlays
            .iter()
            .filter(|actor| matches!(actor.kind, SongLuaOverlayKind::ActorFrameTexture { .. }))
            .collect();
        assert_eq!(captures.len(), 3);
        for capture in captures {
            assert_eq!(
                capture.initial_state.size,
                Some([display_size.0 as f32, display_size.1 as f32])
            );
        }
        let output = compiled
            .overlays
            .iter()
            .find(|actor| actor.name.as_deref() == Some("Output"))
            .unwrap();
        assert_eq!(output.initial_state.zoom_x, 854.0 / display_size.0 as f32);
        assert_eq!(output.initial_state.zoom_y, 480.0 / display_size.1 as f32);
    }
}

#[cfg(all(
    target_os = "windows",
    not(target_pointer_width = "32"),
    not(target_vendor = "win7")
))]
#[test]
#[ignore = "requires a Vulkan device and a window system"]
fn song_lua_preloaded_aft_chain_preserves_pixels() {
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let overlays = compile_display_size_fixture((1600, 900)).overlays;
    let metrics = deadlib_present::space::Metrics::centered(854.0, 480.0);
    deadlib_present::space::set_current_metrics(metrics);
    deadlib_present::space::set_current_window_px(1600, 900);
    let local: Vec<_> = overlays.iter().map(|a| a.initial_state).collect();
    let mut states = Vec::new();
    song_lua_overlay_states_from_local_all_into(&overlays, &local, 854.0, 480.0, &mut states);
    let topology = SongLuaOverlayTopologyIndex::new(&overlays);
    let mut order = song_lua_overlay_order_cache_from(&overlays, &[]);
    let mut captures = SongLuaAftCaptureScratch::new(&overlays, &topology);
    let mut projected = song_lua_projected_mesh_scratch_for(&overlays);
    let mut actors = Vec::new();
    let mut targets = Vec::new();
    push_song_lua_layer_actors(
        &mut actors,
        &mut targets,
        &overlays,
        &mut order,
        &topology,
        &local,
        &states,
        SongLuaOverlayState::default(),
        &SongLuaScreenProxySources::default(),
        None,
        None,
        &AssetManager::new(),
        854.0,
        480.0,
        0.0,
        0.0,
        0.0,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut captures,
        &mut projected,
        SONG_LUA_FOREGROUND_DEPTH,
    );
    let frame = deadlib_present::compose::build_passes(
        std::iter::once(ActorSegment::new(&actors)),
        &targets,
        [0.2, 0.2, 0.2, 1.0],
        &metrics,
        &deadlib_present::font::FontMap::default(),
        0.0,
        &mut deadlib_present::compose::TextLayoutCache::default(),
        &mut deadlib_present::compose::ComposeScratch::default(),
        &CaptureTextureContext,
        None,
    );
    assert_eq!(frame.render_targets.len(), 3);
    for target in &frame.render_targets {
        assert_eq!((target.width, target.height), (1600, 900));
        assert!(!target.ops.is_empty());
    }
    let event_loop = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        event_loop
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(854, 480)),
            )
            .unwrap(),
    );
    let mut backend = deadlib_render::create_backend(
        BackendType::VulkanWgpu,
        window,
        metrics.projection(),
        false,
        deadlib_render_core::PresentModePolicy::Immediate,
        false,
        true,
    )
    .unwrap();
    let mut textures = deadlib_render::TextureHandleMap::default();
    textures.insert(
        1,
        backend
            .create_texture(
                &deadlib_assets::white_texture_image().image,
                Default::default(),
            )
            .unwrap(),
    );
    backend.request_screenshot();
    backend.draw(&frame, &textures, false).unwrap();
    let image = backend.capture_frame().unwrap();
    assert_eq!(image.get_pixel(200, 240).0, [255, 0, 0, 255]);
    assert_eq!(image.get_pixel(650, 240).0, [0, 255, 0, 255]);
}

#[test]
fn song_lua_aft_passes_keep_capture_dependencies_ordered() {
    let overlays = test_nested_alpha_covering_aft_overlays(2.0);
    let states: Vec<_> = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect();
    let topology = SongLuaOverlayTopologyIndex::new(&overlays);
    let mut order = song_lua_overlay_order_cache_from(&overlays, &[]);
    let mut captures = SongLuaAftCaptureScratch::new(&overlays, &topology);
    let mut projected = song_lua_projected_mesh_scratch_for(&overlays);
    let mut actors = Vec::new();
    let mut targets = Vec::with_capacity(2);
    push_song_lua_layer_actors(
        &mut actors,
        &mut targets,
        &overlays,
        &mut order,
        &topology,
        &states,
        &states,
        SongLuaOverlayState::default(),
        &SongLuaScreenProxySources::default(),
        None,
        None,
        &AssetManager::new(),
        854.0,
        480.0,
        0.0,
        0.0,
        0.0,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut captures,
        &mut projected,
        SONG_LUA_FOREGROUND_DEPTH,
    );
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].texture_handle, topology.aft_texture_handles[0]);
    assert_eq!(targets[1].texture_handle, topology.aft_texture_handles[5]);
    let frame = deadlib_present::compose::build_passes(
        std::iter::once(ActorSegment::new(&actors)),
        &targets,
        [0.0; 4],
        &deadlib_present::space::Metrics::centered(854.0, 480.0),
        &deadlib_present::font::FontMap::default(),
        0.0,
        &mut deadlib_present::compose::TextLayoutCache::default(),
        &mut deadlib_present::compose::ComposeScratch::default(),
        &deadlib_present::texture::NullTextureContext,
        None,
    );
    assert!(frame.render_targets[1].ops.iter().any(|op| matches!(op,
            deadlib_render_core::DrawOp::Sprite(run) if
                deadlib_render_core::render_target_base_handle(run.texture_handle) == targets[0].texture_handle)));
    assert!(frame.ops.iter().any(|op| matches!(op,
            deadlib_render_core::DrawOp::Sprite(run) if
                deadlib_render_core::render_target_base_handle(run.texture_handle) == targets[1].texture_handle)));
}

#[test]
fn song_lua_coincident_rgb_aft_renders_once_then_samples_three_times() {
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    deadlib_present::space::set_current_window_px(1600, 900);
    let mut overlays = vec![
        test_capture_overlay("CaptureAFT"),
        test_capture_proxy_child(0, SongLuaProxyTarget::Player { player_index: 0 }),
        test_rgb_aft_overlay("AFTSpriteR", "CaptureAFT", [1.0, 0.0, 0.0, 1.0]),
        test_rgb_aft_overlay("AFTSpriteG", "CaptureAFT", [0.0, 1.0, 0.0, 1.0]),
        test_rgb_aft_overlay("AFTSpriteB", "CaptureAFT", [0.0, 0.0, 1.0, 1.0]),
    ];
    overlays[4].initial_state.texture_filtering = false;
    let overlay_states = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    let source = vec![Arc::<[Actor]>::from(vec![test_source_actor()])];
    let proxy_sources = SongLuaScreenProxySources {
        players: [
            SongLuaPlayerProxySources {
                player: Some(SongLuaProxySource::new(source.as_slice())),
                ..SongLuaPlayerProxySources::default()
            },
            SongLuaPlayerProxySources::default(),
        ],
        ..SongLuaScreenProxySources::default()
    };
    let mut order_cache = song_lua_overlay_order_cache_from(&overlays, &[]);
    let mut topology_index = SongLuaOverlayTopologyIndex::new(&overlays);
    let mut out = Vec::new();
    let mut order_scratch = Vec::new();
    let mut capture_states = Vec::new();
    let mut capture_order_scratch = Vec::new();
    let mut aft_capture_scratch = SongLuaAftCaptureScratch::new(&overlays, &topology_index);
    let mut projected_mesh_scratch = song_lua_projected_mesh_scratch_for(&overlays);

    let mut targets = Vec::new();
    push_song_lua_layer_actors(
        &mut out,
        &mut targets,
        &overlays,
        &mut order_cache,
        &mut topology_index,
        &overlay_states,
        &overlay_states,
        SongLuaOverlayState::default(),
        &proxy_sources,
        None,
        None,
        &AssetManager::new(),
        screen_width(),
        screen_height(),
        0.0,
        0.0,
        0.0,
        &mut order_scratch,
        &mut capture_states,
        &mut capture_order_scratch,
        &mut aft_capture_scratch,
        &mut projected_mesh_scratch,
        SONG_LUA_FOREGROUND_DEPTH,
    );

    assert_eq!(out.len(), 3);
    assert_eq!(targets.len(), 1);
    let deadlib_present::actors::RenderTarget {
        texture_handle,
        size,
        logical_size,
        alpha,
        children,
        ..
    } = &targets[0];
    assert_eq!(*size, [854, 480]);
    assert_eq!(*logical_size, [854.0, 480.0]);
    assert!(!alpha);
    let [Actor::Frame { children, .. }] = children.as_ref() else {
        panic!("expected reusable capture frame");
    };
    let [
        Actor::CameraPush { .. },
        Actor::SharedFrame { blend, tint, .. },
        Actor::CameraPop,
    ] = children.as_slice()
    else {
        panic!("expected direct captured source frame");
    };
    assert_eq!(*blend, Some(BlendMode::Alpha));
    assert_eq!(*tint, [1.0; 4]);
    for (index, actor) in out.iter().enumerate() {
        let Actor::Sprite {
            source: SpriteSource::RenderTarget { handle, .. },
            ..
        } = actor
        else {
            panic!("expected an AFT texture consumer");
        };
        assert_eq!(
            *handle,
            render_target_sample_handle(*texture_handle, index == 2)
        );
    }
}

#[test]
fn song_lua_kenpo_capture_keeps_rotated_notes_and_rgb_split() {
    let native: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/itgmania-actors/kenpo-capture.json"
    )))
    .expect("native capture fixture");
    let metrics = deadlib_present::space::Metrics::centered(854.0, 480.0);
    deadlib_present::space::set_current_metrics(metrics);
    let mut overlays = vec![
        test_capture_overlay("CaptureAFT"),
        test_capture_proxy_child(0, SongLuaProxyTarget::NoteField { player_index: 0 }),
        test_capture_proxy_child(0, SongLuaProxyTarget::Judgment { player_index: 0 }),
        test_rgb_aft_overlay("R", "CaptureAFT", [1.0, 0.0, 0.0, 1.0]),
        test_rgb_aft_overlay("G", "CaptureAFT", [0.0, 1.0, 0.0, 1.0]),
        test_rgb_aft_overlay("B", "CaptureAFT", [0.0, 0.0, 1.0, 1.0]),
    ];
    overlays[1].initial_state = SongLuaOverlayState {
        x: 427.0,
        y: 240.0,
        rot_x_deg: 20.0,
        effect_mode: deadlib_present::anim::EffectMode::Wag,
        effect_magnitude: [0.0, 20.0, 0.0],
        effect_period: 1.0,
        ..Default::default()
    };
    overlays[2].initial_state.x = 427.0;
    overlays[2].initial_state.y = 240.0;
    for overlay in &mut overlays[3..] {
        overlay.initial_state.vibrate = true;
        overlay.initial_state.effect_magnitude = [10.0; 3];
    }
    let note = [Arc::<[Actor]>::from([test_capture_quad(
        [650.0, 115.0],
        [64.0; 2],
    )])];
    let judgment = [Arc::<[Actor]>::from([test_capture_quad(
        [427.0, 220.0],
        [120.0, 24.0],
    )])];
    let sources = SongLuaScreenProxySources {
        players: [
            SongLuaPlayerProxySources {
                note_field: Some(SongLuaProxySource::offset(&note, [-427.0, -240.0])),
                judgment: Some(SongLuaProxySource::offset(&judgment, [-427.0, -240.0])),
                ..Default::default()
            },
            SongLuaPlayerProxySources::default(),
        ],
        ..Default::default()
    };
    let states: Vec<_> = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect();
    let mut order = song_lua_overlay_order_cache_from(&overlays, &[]);
    let topology = SongLuaOverlayTopologyIndex::new(&overlays);
    let mut captures = SongLuaAftCaptureScratch::new(&overlays, &topology);
    let mut projected = song_lua_projected_mesh_scratch_for(&overlays);
    let assets = AssetManager::new();
    let mut text = TextLayoutCache::default();
    let mut compose = ComposeScratch::default();
    // Sample both directions of the final sway, its zero crossings, and
    // multiple RGB vibration frames. ±1 depth clipped this entire note.
    for (sample_index, time) in [0.0, 0.25, 0.5, 0.75, 1.0, 1.25].into_iter().enumerate() {
        let mut actors = Vec::new();
        let mut targets = Vec::new();
        push_song_lua_layer_actors(
            &mut actors,
            &mut targets,
            &overlays,
            &mut order,
            &topology,
            &states,
            &states,
            SongLuaOverlayState::default(),
            &sources,
            None,
            None,
            &assets,
            854.0,
            480.0,
            time,
            time,
            time,
            &mut Vec::new(),
            &mut Vec::new(),
            &mut Vec::new(),
            &mut captures,
            &mut projected,
            SONG_LUA_FOREGROUND_DEPTH,
        );
        let frame = deadlib_present::compose::build_passes(
            std::iter::once(ActorSegment::new(&actors)),
            &targets,
            [0.0; 4],
            &metrics,
            &font::FontMap::default(),
            time,
            &mut text,
            &mut compose,
            &CaptureTextureContext,
            None,
        );
        assert_eq!(frame.render_targets.len(), 1);
        let target = &frame.render_targets[0];
        assert_eq!(
            target.sprite_instances.len(),
            2,
            "note and judgment are captured"
        );
        let mut corners = 0;
        for op in &target.ops {
            let deadlib_render_core::DrawOp::Sprite(run) = op else {
                continue;
            };
            let camera = target.cameras[run.camera as usize];
            for sprite in &target.sprite_instances[run.instance_start as usize..]
                [..run.instance_count as usize]
            {
                for [x, y] in [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]] {
                    corners += 1;
                    let clip = camera
                        * Vector4::new(
                            sprite.center[0] + x * sprite.size[0],
                            sprite.center[1] + y * sprite.size[1],
                            sprite.center[2],
                            1.0,
                        );
                    assert!(
                        clip.z.abs() <= clip.w,
                        "capture clipped at {time}: {clip:?}"
                    );
                    assert!(clip.x.abs() < clip.w && clip.y.abs() < clip.w);
                    let name = if sprite.size[0] == 64.0 {
                        "note"
                    } else {
                        "judgment"
                    };
                    let native_actor = native["samples"][sample_index]["actors"]
                        .as_array()
                        .expect("native actors")
                        .iter()
                        .find(|actor| actor["name"] == name)
                        .expect("native note or judgment");
                    let vertices = native_actor["draws"][0]["vertices"]
                        .as_array()
                        .expect("native vertices");
                    assert!(
                        vertices.iter().any(|vertex| (0..4).all(|axis| {
                            (vertex["clip"][axis].as_f64().expect("clip coordinate") as f32
                                - clip[axis])
                                .abs()
                                < 0.000_002
                        })),
                        "{name} clip differs from native at {time}: {clip:?}"
                    );
                }
            }
        }
        assert_eq!(corners, 8, "both native capture sources must be drawn");
        assert_eq!(frame.sprite_instances.len(), 3);
        let rgb = &frame.sprite_instances;
        assert_ne!(rgb[0].center[..2], rgb[1].center[..2]);
        assert_ne!(rgb[1].center[..2], rgb[2].center[..2]);
        assert_ne!(rgb[0].center[..2], rgb[2].center[..2]);
        for (channel, sprite) in rgb.iter().enumerate() {
            assert_eq!(sprite.tint[channel], 1.0);
            assert!(
                sprite.center[2].abs() <= 0.01,
                "RGB sprite escaped the main depth range"
            );
        }
        if time == 0.25 {
            let effected = song_lua_proxy_effect(states[1], time, time, 1);
            assert!(
                (effected.rot_y_deg - 20.0).abs() < 0.0001,
                "proxy wag must execute"
            );
        }
    }
}

struct CaptureTextureContext;

fn test_capture_quad(position: [f32; 2], size: [f32; 2]) -> Actor {
    let mut actor = deadlib_present::dsl::SpriteBuilder::solid();
    actor.align(0.5, 0.5);
    actor.xy(position[0], position[1]);
    actor.size(size[0], size[1]);
    actor.build(0)
}

impl deadlib_present::texture::TextureContext for CaptureTextureContext {
    fn texture_registry_generation(&self) -> u64 {
        1
    }
    fn texture_dims(&self, _: &str) -> Option<deadlib_present::texture::TextureMeta> {
        Some(deadlib_present::texture::TextureMeta { w: 1, h: 1 })
    }
    fn sprite_sheet_dims(&self, _: &str) -> (u32, u32) {
        (1, 1)
    }
    fn texture_handle(&self, _: &str) -> u64 {
        1
    }
}

#[test]
fn song_lua_kenpo_nested_rotation_matches_native_motion() {
    let native: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/itgmania-actors/kenpo-motion.json"
    )))
    .expect("native nested rotation fixture");
    let metrics = deadlib_present::space::Metrics::centered(854.0, 480.0);
    deadlib_present::space::set_current_metrics(metrics);
    let notes =
        [Arc::<[Actor]>::from([-224.0, 32.0, 224.0].map(|x| {
            test_capture_quad([427.0 + x, 115.0], [64.0; 2])
        }))];
    let mut scratch = SharedActorFrameScratch::with_capacity(16);
    for sample in native["samples"].as_array().expect("native samples") {
        let seconds = sample["time"].as_f64().expect("sample time") as f32;
        let beat = sample["beat"].as_f64().expect("sample beat") as f32;
        // KENPO's wrapper X rotation alternates linearly every half beat;
        // the outer proxy wags around Y on the BGM beat clock at 77 BPM.
        let phase = beat.rem_euclid(1.0);
        let rotation_x = if phase < 0.5 {
            20.0 - 80.0 * phase
        } else {
            -60.0 + 80.0 * phase
        };
        let source = prepare_proxy_source(
            notes.clone(),
            ProxyCapturePart::Field,
            SongLuaCaptureTransform {
                z_shift: 0,
                tint: [1.0; 4],
                blend: None,
                playfield_center_x: 427.0,
                target_x: 427.0,
                target_y: 240.0,
                rotation_x,
                rotation_y: 0.0,
                rotation_z: 0.0,
                skew_x: 0.0,
                skew_y: 0.0,
                zoom_x: 1.0,
                zoom_y: 1.0,
                zoom_z: 1.0,
            },
            &mut scratch,
        )
        .expect("captured field");
        let state = song_lua_proxy_effect(
            SongLuaOverlayState {
                x: 427.0,
                y: 240.0,
                zoom_z: 854.0 / 640.0,
                effect_mode: deadlib_present::anim::EffectMode::Wag,
                effect_clock: deadlib_present::anim::EffectClock::Beat,
                effect_magnitude: [0.0, 20.0, 0.0],
                effect_period: 1.0,
                ..Default::default()
            },
            seconds,
            beat,
            0,
        );
        let proxy =
            song_lua_build_proxy_actor_with_scratch(state, 0, source.view(), 854.0, 480.0, None)
                .expect("rotating proxy");
        let frame = deadlib_present::compose::build_screen_with_texture_context(
            &[
                Actor::CameraPush {
                    view_proj: glam::camera::rh::proj::opengl::orthographic(
                        -427.0, 427.0, -240.0, 240.0, -1000.0, 1000.0,
                    ),
                },
                proxy,
                Actor::CameraPop,
            ],
            [0.0; 4],
            &metrics,
            &font::FontMap::default(),
            seconds,
            &CaptureTextureContext,
        );
        let mut column = 0;
        for op in &frame.ops {
            let deadlib_render_core::DrawOp::Sprite(run) = op else {
                continue;
            };
            let camera = frame.cameras[run.camera as usize];
            for sprite in &frame.sprite_instances[run.instance_start as usize..]
                [..run.instance_count as usize]
            {
                let name = ["left", "middle", "right"][column];
                let actor = sample["actors"]
                    .as_array()
                    .expect("native actors")
                    .iter()
                    .find(|actor| actor["name"] == name)
                    .expect("native note");
                let vertices = actor["draws"][0]["vertices"]
                    .as_array()
                    .expect("native vertices");
                for [x, y] in [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]] {
                    let clip = camera
                        * Vector4::new(
                            sprite.center[0] + x * sprite.size[0],
                            sprite.center[1] + y * sprite.size[1],
                            sprite.center[2],
                            1.0,
                        );
                    assert!(
                        vertices.iter().any(|v| (0..4).all(|axis| {
                            (v["clip"][axis].as_f64().expect("native clip") as f32 - clip[axis])
                                .abs()
                                < 0.000_02
                        })),
                        "{name} at beat {beat}: {clip:?}, native={vertices:?}"
                    );
                }
                column += 1;
            }
        }
        assert_eq!(column, 3, "all native columns must be drawn");
    }
}

#[test]
fn song_lua_proxy_scratch_rotates_while_prior_frame_is_retained() {
    let mut scratch = SongLuaProxyActorScratch::new(1);
    scratch.begin_frame();
    let first = scratch
        .next_screen()
        .expect("first frame has screen capture storage")
        .refill([0.0, 0.0], |actors| actors.push(Actor::CameraPop))
        .expect("first capture is populated");

    scratch.begin_frame();
    let second_slot = scratch
        .next_screen()
        .expect("second frame has screen capture storage");
    let second = second_slot
        .refill([0.0, 0.0], |actors| actors.push(Actor::CameraPop))
        .expect("second capture is populated");
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(second_slot.stats().replacements, 0);

    drop(first);
    drop(second);
    scratch.begin_frame();
    let first_bank_slot = scratch
        .next_screen()
        .expect("released first bank is reusable");
    assert!(
        first_bank_slot
            .refill([0.0, 0.0], |actors| actors.push(Actor::CameraPop))
            .is_some()
    );
    assert_eq!(first_bank_slot.stats().replacements, 0);
}

#[test]
fn song_lua_hud_proxy_flattens_identity_capture_without_retaining_it() {
    let mut capture_scratch = SharedActorFrameScratch::with_capacity(2);
    let capture = [capture_scratch
        .refill([0.0, 0.0], |actors| {
            actors.extend([Actor::CameraPop, Actor::CameraPop]);
        })
        .expect("HUD capture is populated")];
    let mut proxy_scratch = SharedActorFrameScratch::with_capacity(2);
    let transform = SongLuaCaptureTransform {
        z_shift: 0,
        tint: [1.0; 4],
        blend: None,
        playfield_center_x: screen_center_x(),
        target_x: screen_center_x(),
        target_y: screen_center_y(),
        rotation_x: 0.0,
        rotation_z: 0.0,
        rotation_y: 0.0,
        skew_x: 0.0,
        skew_y: 0.0,
        zoom_x: 1.0,
        zoom_y: 1.0,
        zoom_z: 1.0,
    };

    let proxy =
        song_lua_render_captured_source(None, Some(&capture), transform, &mut proxy_scratch)
            .expect("HUD proxy is populated");

    assert_eq!(Arc::strong_count(&capture[0]), 2);
    let [Actor::Frame { children, .. }] = proxy[0].as_ref() else {
        panic!("proxy scratch keeps one frame wrapper");
    };
    assert_eq!(children.len(), 2);
    assert!(
        children
            .iter()
            .all(|actor| matches!(actor, Actor::CameraPop))
    );
}

#[test]
fn identity_proxy_capture_reuses_source_with_origin_offset() {
    let metrics = deadlib_present::space::Metrics::centered(854.0, 480.0);
    deadlib_present::space::set_current_metrics(metrics);
    let vertices: Arc<[MeshVertex]> = Arc::from([
        MeshVertex {
            pos: [0.0, 0.0],
            color: [1.0; 4],
        },
        MeshVertex {
            pos: [12.0, 0.0],
            color: [1.0; 4],
        },
        MeshVertex {
            pos: [0.0, 9.0],
            color: [1.0; 4],
        },
    ]);
    let mesh = |x: f32, z: i16| Actor::Mesh {
        align: [0.0, 0.0],
        offset: [x, 4.0],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        tint: [0.5, 0.25, 0.75, 0.8],
        vertices: Arc::clone(&vertices),
        visible: true,
        blend: BlendMode::Alpha,
        z,
    };
    let mut capture_scratch = SharedActorFrameScratch::with_capacity(5);
    let capture = [capture_scratch
        .refill([0.0, 0.0], |actors| {
            actors.extend([
                Actor::CameraPush {
                    view_proj: Matrix4::IDENTITY,
                },
                mesh(20.0, 3),
                mesh(10.0, 1),
                Actor::CameraPop,
                mesh(30.0, 2),
            ]);
        })
        .expect("notefield capture is populated")];
    let transform = SongLuaCaptureTransform {
        z_shift: 0,
        tint: [1.0; 4],
        blend: None,
        playfield_center_x: screen_center_x(),
        target_x: screen_center_x(),
        target_y: screen_center_y(),
        rotation_x: 0.0,
        rotation_z: 0.0,
        rotation_y: 0.0,
        skew_x: 0.0,
        skew_y: 0.0,
        zoom_x: 1.0,
        zoom_y: 1.0,
        zoom_z: 1.0,
    };
    let mut captured_scratch = SharedActorFrameScratch::with_capacity(5);
    let captured =
        song_lua_render_captured_source(Some(&capture), None, transform, &mut captured_scratch)
            .expect("captured proxy source is populated");
    let mut unused_direct_scratch = SharedActorFrameScratch::with_capacity(5);
    let direct = prepare_proxy_source(
        [Arc::clone(&capture[0])],
        ProxyCapturePart::Field,
        transform,
        &mut unused_direct_scratch,
    )
    .expect("direct proxy capture is populated");
    assert!(Arc::ptr_eq(&capture[0], &direct.segments[0]));
    assert_eq!(direct.offset, [-transform.target_x, -transform.target_y]);

    let proxy_state = SongLuaOverlayState {
        x: transform.target_x,
        y: transform.target_y,
        ..SongLuaOverlayState::default()
    };
    let mut captured_proxy_scratch = SongLuaProxyActorScratch::new(1);
    captured_proxy_scratch.begin_frame();
    let captured_actor = song_lua_build_proxy_actor_with_scratch(
        proxy_state,
        321,
        SongLuaProxySource::offset(&captured, [-transform.target_x, -transform.target_y]),
        screen_width(),
        screen_height(),
        Some(&mut captured_proxy_scratch),
    )
    .expect("captured proxy renders");
    let mut direct_proxy_scratch = SongLuaProxyActorScratch::new(1);
    direct_proxy_scratch.begin_frame();
    let direct_actor = song_lua_build_proxy_actor_with_scratch(
        proxy_state,
        321,
        direct.view(),
        screen_width(),
        screen_height(),
        Some(&mut direct_proxy_scratch),
    )
    .expect("direct proxy renders");

    let resources = ActorResourceArena::new(0);
    let fonts = font::FontMap::default();
    let compose = |actor| {
        let mut text = TextLayoutCache::default();
        let mut scratch = ComposeScratch::default();
        build_screen_segments_cached_with_scratch_and_texture_context_and_actor_resources(
            &[ActorSegment::new(std::slice::from_ref(actor))],
            [0.0, 0.0, 0.0, 1.0],
            &metrics,
            &fonts,
            0.0,
            &mut text,
            &mut scratch,
            &NullTextureContext,
            &resources,
        )
    };
    let captured_frame = compose(&captured_actor);
    let direct_frame = compose(&direct_actor);
    assert_eq!(
        compare_render_frames_semantic(&captured_frame, &direct_frame),
        Ok(())
    );
}

#[test]
fn note_field_proxy_drops_player_camera_scope() {
    let camera = Matrix4::from_scale(Vector3::new(2.0, 3.0, 1.0));
    let capture = [Arc::<[Actor]>::from([
        Actor::CameraPush { view_proj: camera },
        test_source_actor(),
        Actor::CameraPop,
    ])];
    let transform = SongLuaCaptureTransform {
        z_shift: 0,
        tint: [1.0; 4],
        blend: None,
        playfield_center_x: screen_center_x(),
        target_x: screen_center_x(),
        target_y: screen_center_y(),
        rotation_x: 0.0,
        rotation_z: 0.0,
        rotation_y: 0.0,
        skew_x: 0.0,
        skew_y: 0.0,
        zoom_x: 1.0,
        zoom_y: 1.0,
        zoom_z: 1.0,
    };
    let mut scratch = SharedActorFrameScratch::with_capacity(3);

    let prepared = prepare_proxy_source(capture, ProxyCapturePart::Field, transform, &mut scratch)
        .expect("camera-wrapped NoteField capture should remain populated");
    let actors = song_lua_captured_segment_actors(&prepared.segments[0]);

    assert_eq!(actors.len(), 1);
    assert!(matches!(actors[0], Actor::Frame { .. }));
}

#[test]
fn direct_field_proxy_cancels_player_translation_and_layer_z() {
    let playfield_center_x = screen_center_x() - 100.0;
    let transform = SongLuaCaptureTransform {
        z_shift: SONG_LUA_PLAYER_LAYER_Z_BASE,
        tint: [0.7, 0.6, 0.9, 0.8],
        blend: Some(BlendMode::Multiply),
        playfield_center_x,
        target_x: screen_center_x() + 50.0,
        target_y: screen_center_y() - 20.0,
        rotation_x: 7.0,
        rotation_z: 11.0,
        rotation_y: 27.0,
        skew_x: 0.125,
        skew_y: -0.0625,
        zoom_x: 0.875,
        zoom_y: 1.125,
        zoom_z: 0.75,
    };
    assert!(song_lua_player_transform_is_direct_proxy(transform));
    assert!(!song_lua_player_transform_is_direct_identity(transform));
    let draws = [FlatDraw::Sprite(deadlib_present::actors::FlatSprite {
        center: [playfield_center_x + 24.0, screen_center_y() - 40.0],
        world_z: 0.0,
        size: [32.0, 32.0],
        source: deadlib_present::actors::SpriteSource::Solid,
        tint: [0.8, 0.6, 0.4, 1.0],
        glow: [0.0; 4],
        uv_rect: [0.0, 0.0, 1.0, 1.0],
        flip_x: false,
        flip_y: false,
        fade: [0.0; 4],
        blend: BlendMode::Alpha,
        rot_y_deg: 0.0,
        rot_z_deg: 0.0,
        z: 140,
    })];
    let camera = Matrix4::IDENTITY;
    let mut field_scratch = SharedActorFrameScratch::with_capacity(3);
    let source =
        prepare_field_proxy_source(&[], &draws, Some(camera), transform, &mut field_scratch)
            .expect("translated field proxy source should render");
    let proxy_state = SongLuaOverlayState {
        x: screen_center_x() + 120.0,
        y: screen_center_y() + 30.0,
        ..SongLuaOverlayState::default()
    };
    let mut proxy_scratch = SongLuaProxyActorScratch::new(1);
    proxy_scratch.begin_frame();
    let actor = song_lua_build_proxy_actor_with_scratch(
        proxy_state,
        321,
        source.view(),
        screen_width(),
        screen_height(),
        Some(&mut proxy_scratch),
    )
    .expect("translated field proxy should render");
    let metrics = deadlib_present::space::Metrics::centered(screen_width(), screen_height());
    let resources = ActorResourceArena::new(0);
    let fonts = font::FontMap::default();
    let compose = |segment: ActorSegment<'_>| {
        let mut text = TextLayoutCache::default();
        let mut scratch = ComposeScratch::default();
        build_screen_segments_cached_with_scratch_and_texture_context_and_actor_resources(
            &[segment],
            [0.0, 0.0, 0.0, 1.0],
            &metrics,
            &fonts,
            0.0,
            &mut text,
            &mut scratch,
            &NullTextureContext,
            &resources,
        )
    };
    let actor_frame = compose(ActorSegment::new(std::slice::from_ref(&actor)));
    let direct_camera = song_lua_direct_field_camera(Some(camera), transform)
        .expect("translation should resolve a field camera");
    let direct_style = FlatProxyStyle::new(
        transform.tint,
        proxy_state.diffuse,
        song_lua_player_x_fold(transform),
    );
    let direct_frame = compose(ActorSegment::flat_proxy_styled_with_cameras(
        &draws,
        [
            proxy_state.x - transform.target_x,
            proxy_state.y - transform.target_y,
        ],
        321,
        &direct_style,
        song_lua_overlay_blend(proxy_state.blend),
        None,
        Some(&direct_camera),
    ));
    assert_eq!(
        compare_render_frames_semantic(&actor_frame, &direct_frame),
        Ok(())
    );
}

#[test]
fn field_proxy_overflow_keeps_actor_and_direct_draw_inside_camera() {
    let camera = Matrix4::from_rotation_x(0.17);
    let mut text = TextBuilder::new();
    text.settext(TextContent::static_str("overflow"));
    text.z(81);
    let actors = [text.build(0)];
    let draws = [FlatDraw::Sprite(deadlib_present::actors::FlatSprite {
        center: [320.0, 240.0],
        world_z: 0.0,
        size: [32.0, 2.0],
        source: deadlib_present::actors::SpriteSource::Solid,
        tint: [1.0; 4],
        glow: [0.0; 4],
        uv_rect: [0.0, 0.0, 1.0, 1.0],
        flip_x: false,
        flip_y: false,
        fade: [0.0; 4],
        blend: BlendMode::Alpha,
        rot_y_deg: 0.0,
        rot_z_deg: 0.0,
        z: 80,
    })];
    let transform = SongLuaCaptureTransform {
        z_shift: 0,
        tint: [1.0; 4],
        blend: None,
        playfield_center_x: screen_center_x(),
        target_x: screen_center_x(),
        target_y: screen_center_y(),
        rotation_x: 0.0,
        rotation_z: 0.0,
        rotation_y: 0.0,
        skew_x: 0.0,
        skew_y: 0.0,
        zoom_x: 1.0,
        zoom_y: 1.0,
        zoom_z: 1.0,
    };
    let mut scratch = SharedActorFrameScratch::with_capacity(4);
    let source = prepare_field_proxy_source(&actors, &draws, Some(camera), transform, &mut scratch)
        .expect("hybrid field proxy source should render");
    let children = song_lua_captured_segment_actors(&source.segments[0]);

    assert!(matches!(children[0], Actor::CameraPush { view_proj } if view_proj == camera));
    assert!(matches!(children[1], Actor::Text { z: 81, .. }));
    assert!(matches!(children[2], Actor::Sprite { z: 80, .. }));
    assert!(matches!(children[3], Actor::CameraPop));
}

#[test]
fn direct_hud_proxy_cancels_player_translation_and_layer_z() {
    let playfield_center_x = screen_center_x() - 100.0;
    let transform = SongLuaCaptureTransform {
        z_shift: SONG_LUA_PLAYER_LAYER_Z_BASE,
        tint: [0.7, 0.6, 0.9, 0.8],
        blend: Some(BlendMode::Add),
        playfield_center_x,
        target_x: screen_center_x() + 50.0,
        target_y: screen_center_y() - 20.0,
        rotation_x: 7.0,
        rotation_z: 11.0,
        rotation_y: 27.0,
        skew_x: 0.125,
        skew_y: -0.0625,
        zoom_x: 0.875,
        zoom_y: 1.125,
        zoom_z: 0.75,
    };
    assert!(song_lua_player_transform_is_direct_hud_proxy(transform));
    assert!(!song_lua_player_transform_is_direct_identity(transform));
    let draws = [FlatDraw::Sprite(deadlib_present::actors::FlatSprite {
        center: [playfield_center_x + 24.0, screen_center_y() - 40.0],
        world_z: 0.0,
        size: [32.0, 32.0],
        source: deadlib_present::actors::SpriteSource::Solid,
        tint: [0.8, 0.6, 0.4, 1.0],
        glow: [0.0; 4],
        uv_rect: [0.0, 0.0, 1.0, 1.0],
        flip_x: false,
        flip_y: false,
        fade: [0.0; 4],
        blend: BlendMode::Alpha,
        rot_y_deg: 0.0,
        rot_z_deg: 0.0,
        z: 90,
    })];
    let mut hud_scratch = SharedActorFrameScratch::with_capacity(1);
    let source = prepare_flat_proxy_source(&draws, transform, &mut hud_scratch)
        .expect("translated HUD proxy source should render");
    let proxy_state = SongLuaOverlayState {
        x: screen_center_x() + 120.0,
        y: screen_center_y() + 30.0,
        ..SongLuaOverlayState::default()
    };
    let mut proxy_scratch = SongLuaProxyActorScratch::new(1);
    proxy_scratch.begin_frame();
    let actor = song_lua_build_proxy_actor_with_scratch(
        proxy_state,
        321,
        source.view(),
        screen_width(),
        screen_height(),
        Some(&mut proxy_scratch),
    )
    .expect("translated HUD proxy should render");
    let metrics = deadlib_present::space::Metrics::centered(screen_width(), screen_height());
    let resources = ActorResourceArena::new(0);
    let fonts = font::FontMap::default();
    let compose = |segment: ActorSegment<'_>| {
        let mut text = TextLayoutCache::default();
        let mut scratch = ComposeScratch::default();
        build_screen_segments_cached_with_scratch_and_texture_context_and_actor_resources(
            &[segment],
            [0.0, 0.0, 0.0, 1.0],
            &metrics,
            &fonts,
            0.0,
            &mut text,
            &mut scratch,
            &NullTextureContext,
            &resources,
        )
    };
    let actor_frame = compose(ActorSegment::new(std::slice::from_ref(&actor)));
    let direct_camera = song_lua_direct_field_camera(None, transform)
        .expect("translation should resolve a HUD camera");
    let direct_style = FlatProxyStyle::new(
        transform.tint,
        proxy_state.diffuse,
        song_lua_player_x_fold(transform),
    );
    let direct_frame = compose(ActorSegment::flat_proxy_styled_with_cameras(
        &draws,
        [
            proxy_state.x - transform.target_x,
            proxy_state.y - transform.target_y,
        ],
        321,
        &direct_style,
        song_lua_overlay_blend(proxy_state.blend),
        None,
        Some(&direct_camera),
    ));
    assert_eq!(
        compare_render_frames_semantic(&actor_frame, &direct_frame),
        Ok(())
    );
}

#[test]
fn transformed_player_proxy_flattens_depth_before_perspective() {
    let metrics = deadlib_present::space::Metrics::centered(854.0, 480.0);
    deadlib_present::space::set_current_metrics(metrics);
    let base = Matrix4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.001, // perspective divide varies with source Z
        0.0, 0.0, 0.0, 1.0,
    ]);
    let suffix = Matrix4::from_translation(Vector3::new(12.0, -8.0, 0.0))
        * Matrix4::from_scale(Vector3::new(0.9, 1.1, 1.0));
    let proxy_state = SongLuaOverlayState {
        x: screen_center_x(),
        y: screen_center_y(),
        zoom_z: 0.0,
        ..SongLuaOverlayState::default()
    };
    let direct = song_lua_direct_proxy(
        proxy_state,
        321,
        SongLuaDirectProxySource {
            draws: SongLuaDirectDraws::Field,
            draw_start: 0,
            draw_end: 1,
            target: [0.0, 0.0],
            tint: [1.0; 4],
            x_fold: None,
            camera: Some(base * suffix),
            player_camera: Some(SongLuaDirectPlayerCamera { base, suffix }),
        },
        0,
        0,
        screen_width(),
        screen_height(),
    )
    .expect("flattened Player proxy should render");
    let proxy_transform = song_lua_proxy_transform(
        proxy_state,
        [0.0, 0.0],
        screen_width(),
        screen_height(),
        screen_width(),
        screen_height(),
    );
    let camera = direct.camera.expect("Player proxy should resolve a camera");

    assert_eq!(direct.enclosing_camera, None);
    assert_eq!(camera, base * proxy_transform * suffix);
    let project = |z| {
        let clip = camera * Vector4::new(410.0, 180.0, z, 1.0);
        [clip.x / clip.w, clip.y / clip.w]
    };
    let near = project(-96.0);
    let far = project(96.0);
    assert!((near[0] - far[0]).abs() <= 1e-5);
    assert!((near[1] - far[1]).abs() <= 1e-5);
}

#[test]
fn song_lua_player_child_proxy_source_is_player_local() {
    let origin = [screen_center_x(), screen_center_y()];
    let mut actors = vec![test_source_actor()];
    let mut scratch = SharedActorFrameScratch::with_capacity(1);
    let source =
        song_lua_player_child_proxy_source(&mut actors, origin[0], origin[1], &mut scratch)
            .expect("child proxy source should render");
    let actor = song_lua_build_proxy_actor(
        SongLuaOverlayState {
            x: origin[0],
            y: origin[1],
            ..SongLuaOverlayState::default()
        },
        0,
        source.as_slice(),
        screen_width(),
        screen_height(),
    )
    .expect("actor proxy should render with a source");

    let Actor::SharedFrame {
        offset, children, ..
    } = actor
    else {
        panic!("expected direct shared proxy actor");
    };
    assert_eq!(offset, origin);
    let [Actor::Frame { offset, .. }] = children.as_ref() else {
        panic!("expected localized child source");
    };
    assert_eq!(*offset, [-origin[0], -origin[1]]);
}

#[test]
fn song_lua_quad_keeps_zoomed_size_in_scale() {
    let overlay = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::Quad,
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let actor = build_song_lua_overlay_actor(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            size: Some([100.0, 50.0]),
            zoom: 0.5,
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        321,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("quad overlay should render");

    match actor {
        Actor::Sprite {
            size,
            scale,
            z,
            visible,
            ..
        } => {
            let expected_scale = [
                100.0 * 0.5 * screen_width() / 640.0,
                50.0 * 0.5 * screen_height() / 480.0,
            ];
            assert_eq!(z, 321);
            assert!(visible);
            assert!((scale[0] - expected_scale[0]).abs() <= 0.000_1);
            assert!((scale[1] - expected_scale[1]).abs() <= 0.000_1);
            match size {
                [SizeSpec::Px(w), SizeSpec::Px(h)] => {
                    assert_eq!(w, 0.0);
                    assert_eq!(h, 0.0);
                }
                other => panic!("expected explicit quad size, got {other:?}"),
            }
        }
        other => panic!("expected sprite-backed quad, got {other:?}"),
    }
}

#[test]
fn song_lua_shared_vec_preserves_observers_and_reuses_unique_storage() {
    let mut shared = None;
    let mut replacements = 0;
    let first = update_song_lua_shared_vec(&mut shared, 8, &mut replacements, |out| {
        out.extend([1, 2, 3]);
    });
    assert_eq!(replacements, 0);
    let second = update_song_lua_shared_vec(&mut shared, 8, &mut replacements, |out| {
        out.push(4);
    });
    assert_eq!(first.as_slice(), &[1, 2, 3]);
    assert_eq!(second.as_slice(), &[4]);
    assert_eq!(replacements, 1);

    let weak = Arc::downgrade(&second);
    drop(second);
    let third = update_song_lua_shared_vec(&mut shared, 8, &mut replacements, |out| {
        out.extend([5, 6]);
    });
    assert!(weak.upgrade().is_none());
    assert_eq!(third.as_slice(), &[5, 6]);
    assert_eq!(replacements, 2);
    let pointer = third.as_ptr();
    let capacity = third.capacity();
    drop(third);

    let fourth = update_song_lua_shared_vec(&mut shared, 8, &mut replacements, |out| {
        out.push(7);
    });
    assert_eq!(fourth.as_slice(), &[7]);
    assert_eq!(fourth.as_ptr(), pointer);
    assert_eq!(fourth.capacity(), capacity);
    assert_eq!(replacements, 2);
}

#[test]
fn song_lua_actor_multi_vertex_builds_mesh_overlay() {
    let overlay = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::ActorMultiVertex {
            vertices: Arc::from(vec![
                SongLuaOverlayMeshVertex {
                    pos: [0.0, 0.0],
                    color: [1.0, 0.0, 0.0, 1.0],
                    uv: [0.0, 0.0],
                },
                SongLuaOverlayMeshVertex {
                    pos: [10.0, 0.0],
                    color: [0.0, 1.0, 0.0, 1.0],
                    uv: [1.0, 0.0],
                },
                SongLuaOverlayMeshVertex {
                    pos: [0.0, 10.0],
                    color: [0.0, 0.0, 1.0, 1.0],
                    uv: [0.0, 1.0],
                },
            ]),
            texture_path: None,
            texture_key: None,
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let actor = build_song_lua_overlay_actor(
        &overlay,
        SongLuaOverlayState {
            x: 40.0,
            y: 50.0,
            zoom_x: 2.0,
            diffuse: [0.5, 0.5, 0.5, 0.75],
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        321,
        screen_width(),
        screen_height(),
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("ActorMultiVertex overlay should render");

    let Actor::Mesh {
        offset,
        vertices,
        z,
        blend,
        ..
    } = actor
    else {
        panic!("expected mesh-backed ActorMultiVertex overlay");
    };
    assert_eq!(offset, [40.0, 50.0]);
    assert_eq!(z, 321);
    assert_eq!(blend, BlendMode::Alpha);
    assert_eq!(vertices.len(), 3);
    assert_eq!(vertices[1].pos, [20.0, -0.0]);
    assert_eq!(vertices[2].pos, [0.0, -10.0]);
    assert_eq!(vertices[0].color, [0.5, 0.0, 0.0, 0.75]);

    let mut scratch = SongLuaProjectedMeshScratch::mesh(3);
    let build_reused = |scratch: &mut SongLuaProjectedMeshScratch| {
        build_song_lua_overlay_actor_with_scratch(
            &overlay,
            SongLuaOverlayState {
                x: 40.0,
                y: 50.0,
                zoom_x: 2.0,
                diffuse: [0.5, 0.5, 0.5, 0.75],
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            321,
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            0.0,
            Some(scratch),
        )
        .expect_actor("ActorMultiVertex overlay should reuse its mesh")
    };
    let Actor::ReusableMesh {
        vertices: reused_vertices,
        tint,
        ..
    } = build_reused(&mut scratch)
    else {
        panic!("expected reusable ActorMultiVertex mesh");
    };
    assert_eq!(vertices.len(), reused_vertices.len());
    for (expected, actual) in vertices.iter().zip(reused_vertices.iter()) {
        assert_eq!(expected.pos, actual.pos);
        assert_eq!(expected.color, actual.color);
    }
    assert_eq!(tint, [1.0; 4]);
    let buffer_ptr = Arc::as_ptr(&reused_vertices);
    drop(reused_vertices);
    let Actor::ReusableMesh {
        vertices: next_vertices,
        ..
    } = build_reused(&mut scratch)
    else {
        panic!("expected reusable ActorMultiVertex mesh");
    };
    assert_eq!(Arc::as_ptr(&next_vertices), buffer_ptr);
    assert_eq!(scratch.replacements, 0);
}

#[test]
fn song_lua_actor_multi_vertex_builds_textured_mesh_overlay() {
    let texture_key = "song-lua-amv-texture.png".to_string();
    let mut asset_manager = AssetManager::new();
    asset_manager.queue_texture_upload(texture_key.clone(), image::RgbaImage::new(16, 16));
    let overlay = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::ActorMultiVertex {
            vertices: Arc::from(vec![
                SongLuaOverlayMeshVertex {
                    pos: [0.0, 0.0],
                    color: [1.0, 1.0, 1.0, 1.0],
                    uv: [0.0, 0.0],
                },
                SongLuaOverlayMeshVertex {
                    pos: [16.0, 0.0],
                    color: [0.0, 1.0, 0.0, 1.0],
                    uv: [1.0, 0.0],
                },
                SongLuaOverlayMeshVertex {
                    pos: [0.0, 16.0],
                    color: [0.0, 0.0, 1.0, 0.5],
                    uv: [0.0, 1.0],
                },
            ]),
            texture_path: Some(std::path::PathBuf::from(&texture_key)),
            texture_key: Some(Arc::from(texture_key.as_str())),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let actor = build_song_lua_overlay_actor(
        &overlay,
        SongLuaOverlayState {
            x: 12.0,
            y: 24.0,
            diffuse: [0.5, 0.25, 0.75, 0.5],
            ..SongLuaOverlayState::default()
        },
        None,
        &asset_manager,
        322,
        screen_width(),
        screen_height(),
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("textured ActorMultiVertex overlay should render");

    let Actor::TexturedMesh {
        offset,
        texture,
        tint,
        vertices,
        z,
        blend,
        ..
    } = actor
    else {
        panic!("expected textured mesh-backed ActorMultiVertex overlay");
    };
    assert_eq!(offset, [12.0, 24.0]);
    assert_eq!(texture.as_ref(), texture_key.as_str());
    assert_eq!(tint, [0.5, 0.25, 0.75, 0.5]);
    assert_eq!(z, 322);
    assert_eq!(blend, BlendMode::Alpha);
    assert_eq!(vertices.len(), 3);
    assert_eq!(vertices[1].uv, [1.0, 0.0]);
    assert_eq!(vertices[2].color, [0.0, 0.0, 1.0, 0.5]);

    let mut scratch = SongLuaProjectedMeshScratch::textured(3);
    let reused = build_song_lua_overlay_actor_with_scratch(
        &overlay,
        SongLuaOverlayState {
            x: 12.0,
            y: 24.0,
            diffuse: [0.5, 0.25, 0.75, 0.5],
            ..SongLuaOverlayState::default()
        },
        None,
        &asset_manager,
        322,
        screen_width(),
        screen_height(),
        0.0,
        0.0,
        0.0,
        Some(&mut scratch),
    )
    .expect_actor("textured ActorMultiVertex overlay should reuse its mesh");
    let Actor::ReusableTexturedMesh {
        offset: reused_offset,
        tint: reused_tint,
        vertices: reused_vertices,
        ..
    } = reused
    else {
        panic!("expected reusable textured ActorMultiVertex mesh");
    };
    assert_eq!(reused_offset, offset);
    assert_eq!(reused_tint, tint);
    assert_eq!(reused_vertices.as_slice(), vertices.as_ref());
    drop(reused_vertices);

    let build_glow = |scratch: &mut SongLuaProjectedMeshScratch| {
        build_song_lua_overlay_actor_with_scratch(
            &overlay,
            SongLuaOverlayState {
                x: 12.0,
                y: 24.0,
                diffuse: [0.5, 0.25, 0.75, 0.5],
                glow: [0.2, 0.4, 0.8, 0.75],
                ..SongLuaOverlayState::default()
            },
            None,
            &asset_manager,
            322,
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            0.0,
            Some(scratch),
        )
        .expect_actors("glowing ActorMultiVertex should reuse both meshes")
    };
    let glowing = build_glow(&mut scratch);
    let [
        Actor::ReusableTexturedMesh {
            vertices: base_vertices,
            ..
        },
        Actor::ReusableTexturedMesh {
            vertices: glow_vertices,
            blend: glow_blend,
            ..
        },
    ] = glowing.as_slice()
    else {
        panic!("expected reusable base and glow meshes, got {glowing:?}");
    };
    assert_eq!(*glow_blend, BlendMode::Alpha);
    assert_ne!(Arc::as_ptr(base_vertices), Arc::as_ptr(glow_vertices));
    for (source, glow_vertex) in vertices.iter().zip(glow_vertices.iter()) {
        assert_eq!(glow_vertex.pos, source.pos);
        assert_eq!(glow_vertex.uv, source.uv);
        assert_eq!(glow_vertex.color, [1.0, 1.0, 1.0, source.color[3]]);
    }
    let base_ptr = Arc::as_ptr(base_vertices);
    let glow_ptr = Arc::as_ptr(glow_vertices);
    drop(glowing);

    let next = build_glow(&mut scratch);
    let [
        Actor::ReusableTexturedMesh {
            vertices: next_base,
            ..
        },
        Actor::ReusableTexturedMesh {
            vertices: next_glow,
            ..
        },
    ] = next.as_slice()
    else {
        panic!("expected reusable base and glow meshes, got {next:?}");
    };
    assert_eq!(Arc::as_ptr(next_base), base_ptr);
    assert_eq!(Arc::as_ptr(next_glow), glow_ptr);
    assert_eq!(scratch.replacements, 0);
}

#[test]
fn song_lua_model_builds_textured_mesh_layers() {
    let texture_key = "song-lua-model-texture.png".to_string();
    let mut asset_manager = AssetManager::new();
    asset_manager.queue_texture_upload(texture_key.clone(), image::RgbaImage::new(16, 16));
    let overlay = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::Model {
            layers: Arc::from(vec![SongLuaOverlayModelLayer {
                texture_key: Arc::from(texture_key.as_str()),
                vertices: Arc::from(vec![
                    TexturedMeshVertex {
                        pos: [0.0, 0.0, 0.0],
                        uv: [0.0, 0.0],
                        tex_matrix_scale: [1.0, 1.0],
                        color: [1.0, 1.0, 1.0, 1.0],
                    },
                    TexturedMeshVertex {
                        pos: [16.0, 0.0, 0.0],
                        uv: [1.0, 0.0],
                        tex_matrix_scale: [1.0, 1.0],
                        color: [1.0, 1.0, 1.0, 1.0],
                    },
                    TexturedMeshVertex {
                        pos: [0.0, 16.0, 0.0],
                        uv: [0.0, 1.0],
                        tex_matrix_scale: [1.0, 1.0],
                        color: [1.0, 1.0, 1.0, 1.0],
                    },
                ]),
                model_size: [16.0, 16.0],
                uv_scale: [1.0, 1.0],
                uv_offset: [0.125, 0.25],
                uv_tex_shift: [0.0, 0.0],
                uv_velocity: [0.0, -1.0],
                uv_cycle_seconds: Some(2.0),
                draw: SongLuaOverlayModelDraw {
                    pos: [2.0, 3.0, 4.0],
                    rot: [0.0, 0.0, 0.0],
                    zoom: [1.0, 1.0, 1.0],
                    tint: [1.0, 0.5, 0.25, 0.75],
                    vert_align: 0.5,
                    blend_add: false,
                    visible: true,
                },
            }]),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let actor = build_song_lua_overlay_actor(
        &overlay,
        SongLuaOverlayState {
            x: 12.0,
            y: 24.0,
            texcoord_offset: Some([0.25, -0.125]),
            diffuse: [0.5, 0.25, 0.75, 0.5],
            ..SongLuaOverlayState::default()
        },
        None,
        &asset_manager,
        323,
        screen_width(),
        screen_height(),
        0.0,
        0.0,
        1.0,
    )
    .expect_actor("Model overlay should render");

    let Actor::TexturedMesh {
        offset,
        texture,
        tint,
        vertices,
        z,
        blend,
        uv_offset,
        uv_tex_shift,
        ..
    } = &actor
    else {
        panic!("expected textured mesh model layer");
    };
    assert_eq!(*offset, [12.0, 24.0]);
    assert_eq!(texture.as_ref(), texture_key.as_str());
    assert_eq!(*tint, [0.5, 0.125, 0.1875, 0.375]);
    assert_eq!(*z, 323);
    assert_eq!(*blend, BlendMode::Alpha);
    assert_eq!(*uv_offset, [0.375, -0.375]);
    assert_eq!(*uv_tex_shift, [0.25, -0.625]);
    assert_eq!(vertices.len(), 3);

    let SongLuaOverlayKind::Model { layers } = &overlay.kind else {
        unreachable!("test overlay is a model");
    };
    let multi_layer = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::Model {
            layers: Arc::from(vec![
                layers[0].clone(),
                layers[0].clone(),
                layers[0].clone(),
            ]),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let multi_state = SongLuaOverlayState {
        x: 12.0,
        y: 24.0,
        glow: [0.25, 0.5, 0.75, 0.5],
        ..SongLuaOverlayState::default()
    };
    let expected = build_song_lua_overlay_actor(
        &multi_layer,
        multi_state,
        None,
        &asset_manager,
        323,
        screen_width(),
        screen_height(),
        0.0,
        0.0,
        1.0,
    )
    .expect_actors("multi-layer model should render");
    let mut direct = Vec::with_capacity(expected.len());
    assert_eq!(
        append_song_lua_multi_actor_overlay(
            &mut direct,
            &multi_layer,
            multi_state,
            &asset_manager,
            323,
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            1.0,
            None,
        ),
        Some(true)
    );
    assert_eq!(expected.len(), 6);
    assert_eq!(format!("{expected:?}"), format!("{direct:?}"));

    let mut scratches = song_lua_projected_mesh_scratch_for(std::slice::from_ref(&multi_layer));
    let mut model_scratch = scratches.pop().expect("model scratch should be prewarmed");
    let prewarmed = model_scratch
        .model_glow_vertices
        .as_ref()
        .expect("model glow vertices should be compiled during entry")
        .clone();
    assert_eq!(prewarmed.len(), 3);
    let mut warmed = Vec::with_capacity(expected.len());
    let mut append_warmed = |out: &mut Vec<Actor>| {
        out.clear();
        assert_eq!(
            append_song_lua_multi_actor_overlay(
                out,
                &multi_layer,
                multi_state,
                &asset_manager,
                323,
                screen_width(),
                screen_height(),
                0.0,
                0.0,
                1.0,
                Some(&mut model_scratch),
            ),
            Some(true)
        );
    };
    append_warmed(&mut warmed);
    let mut normalized = warmed.clone();
    for actor in &mut normalized {
        if let Actor::TexturedMesh { geom_cache_key, .. } = actor {
            *geom_cache_key = INVALID_TMESH_CACHE_KEY;
        }
    }
    assert_eq!(format!("{expected:?}"), format!("{normalized:?}"));
    for (layer_index, prewarmed_vertices) in prewarmed.iter().enumerate() {
        let Actor::TexturedMesh {
            geom_cache_key: base_key,
            ..
        } = &warmed[layer_index * 2]
        else {
            panic!("expected prewarmed static model base mesh");
        };
        let Actor::TexturedMesh {
            vertices,
            geom_cache_key: glow_key,
            blend,
            ..
        } = &warmed[layer_index * 2 + 1]
        else {
            panic!("expected prewarmed static model glow mesh");
        };
        assert_ne!(*base_key, INVALID_TMESH_CACHE_KEY);
        assert_ne!(*glow_key, INVALID_TMESH_CACHE_KEY);
        assert_ne!(base_key, glow_key);
        assert_eq!(*blend, BlendMode::Alpha);
        assert!(Arc::ptr_eq(vertices, prewarmed_vertices));
    }
    append_warmed(&mut warmed);
    for (layer_index, prewarmed_vertices) in prewarmed.iter().enumerate() {
        let Actor::TexturedMesh { vertices, .. } = &warmed[layer_index * 2 + 1] else {
            panic!("expected prewarmed static model glow mesh");
        };
        assert!(Arc::ptr_eq(vertices, prewarmed_vertices));
    }
}

#[test]
fn song_lua_multitap_model_preserves_vertical_squash_in_all_lanes() {
    crate::tests::init_paths();
    let slots = deadsync_assets::noteskin::load_itg_model_slots_from_path(
        &workspace_root().join("assets/noteskins/dance/cyber/_down tap note model.txt"),
    )
    .expect("cyber tap model should load");
    let mut assets = AssetManager::new();
    for slot in slots.iter() {
        assets.queue_texture_upload(slot.texture_key().to_owned(), image::RgbaImage::new(16, 16));
    }
    for rotation in [90.0_f32, 0.0, 180.0, 270.0] {
        for squash in [0.9, 1.0, 1.1] {
            let state = song_lua_overlay_compose_state(
                &SongLuaOverlayKind::ActorFrame,
                SongLuaOverlayState {
                    zoom_y: squash,
                    ..Default::default()
                },
                SongLuaOverlayState {
                    rot_x_deg: -3.8146973e-6,
                    rot_z_deg: rotation,
                    ..Default::default()
                },
                854.0,
                480.0,
            );
            let actors = song_lua_noteskin_actor(
                &slots,
                state,
                &assets,
                323,
                1.0,
                1.0,
                song_lua_overlay_axis_scale(state),
                [1.0; 3],
                [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg],
                [0.0; 3],
                [1.0; 4],
                [0.0; 4],
                BlendMode::Alpha,
                0.0,
                0.0,
            )
            .expect("squashed multitap model should render");
            let actual = first_textured_mesh_transform(&actors);
            // Native ActorFrame scale * child base rotation * Model's Y flip.
            let expected = Matrix4::from_scale(Vector3::new(1.0, squash, 1.0))
                * Matrix4::from_rotation_z(rotation.to_radians())
                * Matrix4::from_scale(Vector3::new(1.0, -1.0, 1.0));
            for point in [
                Vector4::new(20.0, 0.0, 0.0, 1.0),
                Vector4::new(0.0, 20.0, 0.0, 1.0),
            ] {
                let actual = actual * point;
                let expected = expected * point;
                assert!(
                    (actual - expected).abs().max_element() < 0.0001,
                    "rotation={rotation}, squash={squash}: {actual:?} != {expected:?}"
                );
            }
        }
    }
}

#[test]
fn song_lua_noteskin_actor_rotation_matches_noteskin_base_rotation() {
    crate::tests::init_paths();
    let model_path =
        workspace_root().join("assets/noteskins/dance/ddr-note/_down tap note model.txt");
    let slots = deadsync_assets::noteskin::load_itg_model_slots_from_path(&model_path)
        .expect("ddr-note tap model should load");
    let mut rotated_slots = slots.iter().cloned().collect::<Vec<_>>();
    for slot in &mut rotated_slots {
        slot.set_rotation_deg(90);
    }
    let rotated_slots = Arc::<[SpriteSlot]>::from(rotated_slots.into_boxed_slice());
    let mut asset_manager = AssetManager::new();
    for slot in slots.iter().chain(rotated_slots.iter()) {
        asset_manager
            .queue_texture_upload(slot.texture_key().to_owned(), image::RgbaImage::new(16, 16));
    }

    let actor_rotation = song_lua_noteskin_actor(
        &slots,
        SongLuaOverlayState {
            rot_z_deg: 90.0,
            ..SongLuaOverlayState::default()
        },
        &asset_manager,
        323,
        1.0,
        1.0,
        [1.0, 1.0],
        [1.0, 1.0, 1.0],
        [0.0, 0.0, 90.0],
        [0.0, 0.0, 0.0],
        [1.0, 1.0, 1.0, 1.0],
        [0.0, 0.0, 0.0, 0.0],
        BlendMode::Alpha,
        0.0,
        0.0,
    )
    .expect("noteskin actor with song-lua rotation should render");
    let mut direct_rotation = Vec::with_capacity(actor_rotation.len());
    assert!(append_song_lua_noteskin_actors(
        &mut direct_rotation,
        &slots,
        SongLuaOverlayState {
            rot_z_deg: 90.0,
            ..SongLuaOverlayState::default()
        },
        &asset_manager,
        323,
        1.0,
        1.0,
        [1.0, 1.0],
        [1.0, 1.0, 1.0],
        [0.0, 0.0, 90.0],
        [0.0, 0.0, 0.0],
        [1.0, 1.0, 1.0, 1.0],
        [0.0, 0.0, 0.0, 0.0],
        BlendMode::Alpha,
        0.0,
        0.0,
        None,
    ));
    assert_eq!(
        format!("{actor_rotation:?}"),
        format!("{direct_rotation:?}")
    );
    let base_rotation = song_lua_noteskin_actor(
        &rotated_slots,
        SongLuaOverlayState::default(),
        &asset_manager,
        323,
        1.0,
        1.0,
        [1.0, 1.0],
        [1.0, 1.0, 1.0],
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
        [1.0, 1.0, 1.0, 1.0],
        [0.0, 0.0, 0.0, 0.0],
        BlendMode::Alpha,
        0.0,
        0.0,
    )
    .expect("noteskin actor with pre-rotated slots should render");
    let actor_matrix = first_textured_mesh_transform(&actor_rotation);
    let base_matrix = first_textured_mesh_transform(&base_rotation);
    let actor_cols = actor_matrix.to_cols_array();
    let base_cols = base_matrix.to_cols_array();

    assert!(
        actor_cols
            .iter()
            .zip(base_cols.iter())
            .all(|(left, right)| (left - right).abs() <= 0.000_1)
    );

    let overlay = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::NoteskinActor {
            slots: Arc::clone(&slots),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let state = SongLuaOverlayState {
        rot_z_deg: 90.0,
        glow: [0.25, 0.5, 0.75, 0.5],
        ..SongLuaOverlayState::default()
    };
    let expected = build_song_lua_overlay_actor(
        &overlay,
        state,
        None,
        &asset_manager,
        323,
        screen_width(),
        screen_height(),
        0.0,
        0.0,
        1.0,
    )
    .expect_actors("noteskin model should render");
    let mut scratches = song_lua_projected_mesh_scratch_for(std::slice::from_ref(&overlay));
    let scratch = scratches
        .first_mut()
        .expect("noteskin model scratch should prewarm");
    let prewarmed_glow = scratch
        .noteskin_glow_vertices
        .as_ref()
        .expect("noteskin glow geometry should prewarm")
        .clone();
    let mut warmed = Vec::with_capacity(expected.len());
    assert_eq!(
        append_song_lua_multi_actor_overlay(
            &mut warmed,
            &overlay,
            state,
            &asset_manager,
            323,
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            1.0,
            Some(scratch),
        ),
        Some(true)
    );
    let mut normalized = warmed.clone();
    for actor in &mut normalized {
        if let Actor::TexturedMesh { geom_cache_key, .. } = actor {
            *geom_cache_key = INVALID_TMESH_CACHE_KEY;
        }
    }
    assert_eq!(format!("{expected:?}"), format!("{normalized:?}"));
    for (slot_index, actors) in warmed.as_chunks::<2>().0.iter().enumerate() {
        let [
            Actor::TexturedMesh {
                geom_cache_key: base_key,
                ..
            },
            Actor::TexturedMesh {
                vertices,
                geom_cache_key: glow_key,
                blend,
                ..
            },
        ] = actors
        else {
            panic!("expected prewarmed noteskin base/glow pair");
        };
        assert_ne!(*base_key, INVALID_TMESH_CACHE_KEY);
        assert_ne!(*glow_key, INVALID_TMESH_CACHE_KEY);
        assert_ne!(base_key, glow_key);
        assert_eq!(*blend, BlendMode::Alpha);
        let expected = prewarmed_glow[slot_index]
            .as_ref()
            .expect("rendered model slot should have prewarmed glow geometry");
        assert!(Arc::ptr_eq(vertices, expected));
    }
}

#[test]
fn song_lua_song_meter_display_builds_progress_quad() {
    let overlay = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::SongMeterDisplay {
            stream_width: 100.0,
            stream_state: SongLuaOverlayState {
                zoom_y: 18.0,
                diffuse: [1.0, 0.0, 0.0, 0.8],
                ..SongLuaOverlayState::default()
            },
            music_length_seconds: 100.0,
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let actor = build_song_lua_overlay_actor(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 20.0,
            diffuse: [0.5, 1.0, 1.0, 1.0],
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        323,
        screen_width(),
        screen_height(),
        0.0,
        0.0,
        25.0,
    )
    .expect_actor("SongMeterDisplay overlay should render");

    match actor {
        Actor::Sprite {
            offset,
            scale,
            tint,
            z,
            visible,
            ..
        } => {
            assert_eq!(offset, [270.0, 20.0]);
            assert_eq!(scale, [25.0, 18.0]);
            assert_eq!(tint, [0.5, 0.0, 0.0, 0.8]);
            assert_eq!(z, 323);
            assert!(visible);
        }
        other => panic!("expected sprite-backed SongMeterDisplay quad, got {other:?}"),
    }
}

#[test]
fn song_lua_graph_display_builds_line_quad() {
    let overlay = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::GraphDisplay {
            size: [120.0, 60.0],
            body_values: Arc::from([0.5, 0.5]),
            body_state: SongLuaOverlayState {
                visible: false,
                ..SongLuaOverlayState::default()
            },
            line_state: Box::new(SongLuaOverlayState {
                y: 1.0,
                diffuse: [0.8, 0.7, 0.6, 0.5],
                ..SongLuaOverlayState::default()
            }),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let actor = build_song_lua_overlay_actor(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 100.0,
            valign: 0.0,
            diffuse: [0.5, 1.0, 1.0, 1.0],
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        324,
        screen_width(),
        screen_height(),
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("GraphDisplay overlay should render");

    match actor {
        Actor::Mesh {
            vertices,
            z,
            visible,
            ..
        } => {
            assert_eq!(z, 324);
            assert_eq!(vertices.len(), 6);
            assert_eq!(vertices[0].pos, [260.0, 131.5]);
            assert_eq!(vertices[1].pos, [260.0, 130.5]);
            assert_eq!(vertices[2].pos, [380.0, 130.5]);
            assert_eq!(vertices[0].color, [0.4, 0.7, 0.6, 0.5]);
            assert!(visible);
        }
        other => panic!("expected mesh-backed GraphDisplay line, got {other:?}"),
    }
}

#[test]
fn song_lua_graph_display_builds_body_and_line_quads() {
    let overlay = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::GraphDisplay {
            size: [120.0, 60.0],
            body_values: Arc::from([0.25, 0.75]),
            body_state: SongLuaOverlayState {
                diffuse: [0.2, 0.5, 1.0, 0.75],
                ..SongLuaOverlayState::default()
            },
            line_state: Box::new(SongLuaOverlayState {
                y: 1.0,
                diffuse: [0.8, 0.7, 0.6, 0.5],
                ..SongLuaOverlayState::default()
            }),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let actor = build_song_lua_overlay_actor(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 100.0,
            valign: 0.0,
            diffuse: [0.5, 1.0, 1.0, 1.0],
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        324,
        screen_width(),
        screen_height(),
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("GraphDisplay overlay should render");

    let Actor::Frame { children, .. } = actor else {
        panic!("expected GraphDisplay body and line frame");
    };
    assert_eq!(children.len(), 2);
    match &children[0] {
        Actor::Mesh {
            vertices, visible, ..
        } => {
            assert_eq!(vertices.len(), 6);
            assert_eq!(vertices[0].pos, [260.0, 145.0]);
            assert_eq!(vertices[1].pos, [260.0, 160.0]);
            assert_eq!(vertices[2].pos, [380.0, 160.0]);
            assert_eq!(vertices[5].pos, [380.0, 115.0]);
            assert_eq!(vertices[0].color, [0.1, 0.5, 1.0, 0.75]);
            assert!(*visible);
        }
        other => panic!("expected mesh-backed GraphDisplay body, got {other:?}"),
    }
    match &children[1] {
        Actor::Mesh { vertices, .. } => {
            assert_eq!(vertices.len(), 6);
            assert_eq!(vertices[0].color, [0.4, 0.7, 0.6, 0.5]);
        }
        other => panic!("expected mesh-backed GraphDisplay line, got {other:?}"),
    }
}

#[test]
fn song_lua_graph_display_reuses_prewarmed_meshes_and_frame() {
    let values = Arc::from([0.25, 0.75]);
    let state = SongLuaOverlayState {
        x: 320.0,
        y: 100.0,
        valign: 0.0,
        diffuse: [0.5, 1.0, 1.0, 1.0],
        ..SongLuaOverlayState::default()
    };
    let body_state = SongLuaOverlayState {
        diffuse: [0.2, 0.5, 1.0, 0.75],
        ..SongLuaOverlayState::default()
    };
    let line_state = SongLuaOverlayState {
        y: 1.0,
        diffuse: [0.8, 0.7, 0.6, 0.5],
        ..SongLuaOverlayState::default()
    };
    let mut scratch = SongLuaProjectedMeshScratch::graph(6);
    let build = |scratch: &mut SongLuaProjectedMeshScratch| {
        song_lua_graph_display_actor(
            state,
            &values,
            body_state,
            line_state,
            [120.0, 60.0],
            1.0,
            1.0,
            324,
            Some(scratch),
        )
        .expect("GraphDisplay should render")
    };

    let actor = build(&mut scratch);
    let Actor::SharedFrame { children, .. } = &actor else {
        panic!("expected reused GraphDisplay shared frame, got {actor:?}");
    };
    let [
        Actor::Frame {
            children: graph_children,
            ..
        },
    ] = children.as_ref()
    else {
        panic!("expected GraphDisplay identity frame");
    };
    let [
        Actor::ReusableMesh {
            vertices: body_vertices,
            ..
        },
        Actor::ReusableMesh {
            vertices: line_vertices,
            ..
        },
    ] = graph_children.as_slice()
    else {
        panic!("expected reusable GraphDisplay body and line meshes");
    };
    assert_eq!(body_vertices[0].pos, [260.0, 145.0]);
    assert_eq!(body_vertices[5].pos, [380.0, 115.0]);
    assert_eq!(line_vertices[0].color, [0.4, 0.7, 0.6, 0.5]);
    let body_ptr = Arc::as_ptr(body_vertices);
    let line_ptr = Arc::as_ptr(line_vertices);
    drop(actor);

    let next = build(&mut scratch);
    let Actor::SharedFrame { children, .. } = &next else {
        panic!("expected reused GraphDisplay shared frame");
    };
    let [
        Actor::Frame {
            children: graph_children,
            ..
        },
    ] = children.as_ref()
    else {
        panic!("expected GraphDisplay identity frame");
    };
    let [
        Actor::ReusableMesh {
            vertices: next_body,
            ..
        },
        Actor::ReusableMesh {
            vertices: next_line,
            ..
        },
    ] = graph_children.as_slice()
    else {
        panic!("expected reusable GraphDisplay body and line meshes");
    };
    assert_eq!(Arc::as_ptr(next_body), body_ptr);
    assert_eq!(Arc::as_ptr(next_line), line_ptr);
    assert_eq!(scratch.replacements, 0);
    assert_eq!(
        scratch.graph_frame.as_ref().unwrap().stats().replacements,
        0
    );
    assert_eq!(scratch.graph_frame.as_ref().unwrap().stats().growths, 0);

    let changed_state = SongLuaOverlayState {
        x: state.x + 10.0,
        ..state
    };
    let changed = song_lua_graph_display_actor(
        changed_state,
        &values,
        body_state,
        line_state,
        [120.0, 60.0],
        1.0,
        1.0,
        324,
        Some(&mut scratch),
    )
    .expect("changed GraphDisplay should render");
    let Actor::SharedFrame { children, .. } = &changed else {
        panic!("expected changed GraphDisplay shared frame");
    };
    let [
        Actor::Frame {
            children: graph_children,
            ..
        },
    ] = children.as_ref()
    else {
        panic!("expected changed GraphDisplay identity frame");
    };
    let [
        Actor::ReusableMesh { vertices: body, .. },
        Actor::ReusableMesh { vertices: line, .. },
    ] = graph_children.as_slice()
    else {
        panic!("expected changed reusable GraphDisplay meshes");
    };
    assert_eq!(body[0].pos, [270.0, 145.0]);
    assert_ne!(Arc::as_ptr(body), body_ptr);
    assert_ne!(Arc::as_ptr(line), line_ptr);
    assert_eq!(scratch.replacements, 2);
}

#[test]
fn song_lua_quad_uses_textured_mesh_under_perspective_camera() {
    let overlay = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::Quad,
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let actor = build_song_lua_overlay_actor(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            size: Some([100.0, 50.0]),
            rot_x_deg: 45.0,
            ..SongLuaOverlayState::default()
        },
        Some(SongLuaOverlayState {
            fov: Some(120.0),
            ..SongLuaOverlayState::default()
        }),
        &AssetManager::new(),
        654,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("perspective song lua quad should render");

    match actor {
        Actor::TexturedMesh {
            texture,
            vertices,
            z,
            ..
        } => {
            assert_eq!(z, 654);
            assert_eq!(vertices.len(), 6);
            assert!(Arc::ptr_eq(&texture, &white_texture_key()));
        }
        other => panic!("expected projected textured mesh, got {other:?}"),
    }
}

#[test]
fn song_lua_quad_applies_bounce_effect_offset_at_runtime() {
    let overlay = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::Quad,
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let actor = build_song_lua_overlay_actor(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            size: Some([100.0, 50.0]),
            effect_mode: deadlib_present::anim::EffectMode::Bounce,
            effect_clock: deadlib_present::anim::EffectClock::Beat,
            effect_period: 2.0,
            effect_offset: 1.0,
            effect_magnitude: [10.0, 20.0, 5.0],
            z_bias: 2.5,
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        777,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("effect quad should render");

    match actor {
        Actor::Sprite {
            offset,
            world_z,
            scale,
            z,
            ..
        } => {
            let x_scale = screen_width() / 640.0;
            let y_scale = screen_height() / 480.0;
            assert_eq!(z, 777);
            assert!((320.0_f32 + 10.0).mul_add(-x_scale, offset[0]).abs() <= 0.000_1);
            assert!((240.0_f32 + 20.0).mul_add(-y_scale, offset[1]).abs() <= 0.000_1);
            assert!((world_z - 7.5).abs() <= 0.000_1);
            assert!(scale[0] > 0.0);
            assert!(scale[1] > 0.0);
        }
        other => panic!("expected sprite-backed quad, got {other:?}"),
    }
}

#[test]
fn song_lua_vibrate_applies_effect_magnitude_at_runtime() {
    let effect = EffectState {
        magnitude: [20.0, 10.0, 5.0],
        ..EffectState::default()
    };
    let mut tint = [1.0; 4];
    let mut glow = [0.0; 4];
    let mut still = [0.0; 3];
    let mut scale = [1.0; 3];
    let mut rotation = [0.0; 3];
    song_lua_apply_overlay_effect(
        effect,
        false,
        [0.0; 3],
        0.5,
        0.0,
        0,
        &mut tint,
        &mut glow,
        &mut still,
        &mut scale,
        &mut rotation,
    );
    assert_eq!(still, [0.0; 3]);

    let mut shaken = [0.0; 3];
    song_lua_apply_overlay_effect(
        effect,
        false,
        effect.magnitude,
        0.5,
        0.0,
        0,
        &mut tint,
        &mut glow,
        &mut shaken,
        &mut scale,
        &mut rotation,
    );
    assert!(shaken.iter().any(|value| value.abs() > 0.001));
    assert!(shaken[0].abs() <= effect.magnitude[0]);
    assert!(shaken[1].abs() <= effect.magnitude[1]);
    assert!(shaken[2].abs() <= effect.magnitude[2]);

    let mut same_frame = [0.0; 3];
    song_lua_apply_overlay_effect(
        effect,
        false,
        effect.magnitude,
        0.51,
        0.0,
        0,
        &mut tint,
        &mut glow,
        &mut same_frame,
        &mut scale,
        &mut rotation,
    );
    assert_eq!(same_frame, shaken);

    let mut next_frame = [0.0; 3];
    song_lua_apply_overlay_effect(
        effect,
        false,
        effect.magnitude,
        0.52,
        0.0,
        0,
        &mut tint,
        &mut glow,
        &mut next_frame,
        &mut scale,
        &mut rotation,
    );
    assert_ne!(next_frame, shaken);
}

#[test]
fn song_lua_quad_applies_custom_effect_timing_at_runtime() {
    let overlay = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::Quad,
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let actor = build_song_lua_overlay_actor(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            size: Some([100.0, 50.0]),
            effect_mode: deadlib_present::anim::EffectMode::Bob,
            effect_clock: deadlib_present::anim::EffectClock::Time,
            effect_period: 2.0,
            effect_timing: Some([0.0, 1.0, 0.0, 0.0, 1.0]),
            effect_magnitude: [10.0, 20.0, 5.0],
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        778,
        640.0,
        480.0,
        0.5,
        0.0,
        0.0,
    )
    .expect_actor("custom-timed effect quad should render");

    match actor {
        Actor::Sprite {
            offset, world_z, z, ..
        } => {
            let x_scale = screen_width() / 640.0;
            let y_scale = screen_height() / 480.0;
            assert_eq!(z, 778);
            assert!(320.0f32.mul_add(-x_scale, offset[0]).abs() <= 0.000_1);
            assert!(240.0f32.mul_add(-y_scale, offset[1]).abs() <= 0.000_1);
            assert!(world_z.abs() <= 0.000_1);
        }
        other => panic!("expected sprite-backed quad, got {other:?}"),
    }
}

#[test]
fn song_lua_quad_applies_rainbow_tint_at_runtime() {
    let overlay = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::Quad,
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let actor = build_song_lua_overlay_actor(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            size: Some([100.0, 50.0]),
            rainbow: true,
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        779,
        640.0,
        480.0,
        0.5,
        0.0,
        0.5,
    )
    .expect_actor("rainbow quad should render");

    match actor {
        Actor::Sprite { tint, z, .. } => {
            assert_eq!(z, 779);
            assert_eq!(tint, [0.0, 1.0, 1.0, 1.0]);
        }
        other => panic!("expected rainbow sprite-backed quad, got {other:?}"),
    }
}

#[test]
fn song_lua_bitmaptext_applies_rainbow_scroll_at_runtime() {
    let overlay = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::BitmapText {
            font_name: "miso",
            font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
            text: Arc::<str>::from("ABC"),
            text_changes: Arc::from([]),
            stroke_color: None,
            attributes: empty_text_attributes(),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let mut scratch = song_lua_projected_mesh_scratch_for(std::slice::from_ref(&overlay));
    let actor = build_song_lua_overlay_actor_with_scratch(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            rainbow_scroll: true,
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        780,
        640.0,
        480.0,
        0.0,
        0.0,
        0.4,
        scratch.first_mut(),
    )
    .expect_actor("rainbow-scroll bitmap text should render");

    match actor {
        Actor::Text { attributes, z, .. } => {
            assert_eq!(z, 780);
            assert!(matches!(attributes, TextAttributes::Shared(_)));
            assert_eq!(attributes.len(), 3);
            assert_eq!(attributes[0].color, [0.4, 0.3, 0.5, 1.0]);
            assert_eq!(attributes[1].color, [0.2, 0.6, 1.0, 1.0]);
            assert_eq!(attributes[2].color, [0.2, 0.8, 0.8, 1.0]);
        }
        other => panic!("expected rainbow-scroll bitmap text actor, got {other:?}"),
    }
}

#[test]
fn song_lua_countdown_renders_precompiled_text() {
    let overlay = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::BitmapText {
            font_name: "miso",
            font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
            text: Arc::from(""),
            text_changes: Arc::from([
                (104.0, Arc::from("3")),
                (112.0_f32.next_up(), Arc::from("2")),
            ]),
            stroke_color: None,
            attributes: empty_text_attributes(),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let mut scratch = song_lua_projected_mesh_scratch_for(std::slice::from_ref(&overlay));
    assert!(scratch[0].text_attribute_capacity >= 1);
    for uppercase in [false, true] {
        for (beat, expected) in [(111.0, "3"), (112.0, "3"), (112.01, "2"), (111.0, "3")] {
            let actor = build_song_lua_overlay_actor_with_scratch(
                &overlay,
                SongLuaOverlayState {
                    uppercase,
                    ..Default::default()
                },
                None,
                &AssetManager::new(),
                780,
                640.0,
                480.0,
                0.0,
                beat,
                0.0,
                scratch.first_mut(),
            )
            .expect_actor("countdown should render");
            let Actor::Text { content, .. } = actor else {
                panic!("countdown must be text");
            };
            assert_eq!(content.as_str(), expected);
        }
    }
}

#[test]
fn long_song_lua_rainbow_text_uses_prewarmed_current_phase_buffer() {
    let text = "R".repeat(SONG_LUA_RAINBOW_TEXT_PREWARM_MAX_CHARS + 17);
    let overlay = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::BitmapText {
            font_name: "miso",
            font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
            text: Arc::from(text.as_str()),
            text_changes: Arc::from([]),
            stroke_color: None,
            attributes: empty_text_attributes(),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let mut scratch = song_lua_projected_mesh_scratch_for(std::slice::from_ref(&overlay));
    assert!(scratch[0].rainbow_text_attributes.is_none());
    assert!(scratch[0].text_attribute_capacity >= text.chars().count());

    let actor = build_song_lua_overlay_actor_with_scratch(
        &overlay,
        SongLuaOverlayState {
            rainbow_scroll: true,
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        780,
        640.0,
        480.0,
        0.0,
        0.0,
        0.4,
        scratch.first_mut(),
    )
    .expect_actor("long rainbow-scroll bitmap text should render");

    let Actor::Text {
        attributes: TextAttributes::Reusable(attributes),
        ..
    } = actor
    else {
        panic!("expected text with reusable rainbow attributes");
    };
    assert_eq!(attributes.len(), text.chars().count());
    assert_eq!(attributes[0].color, [0.4, 0.3, 0.5, 1.0]);
    assert_eq!(attributes[1].color, [0.2, 0.6, 1.0, 1.0]);
    assert_eq!(scratch[0].replacements, 0);
}

#[test]
fn song_lua_bitmaptext_shares_compiled_attributes() {
    let compiled: Arc<[TextAttribute]> = Arc::from([TextAttribute {
        start: 1,
        length: 2,
        color: [0.2, 0.4, 0.6, 0.8],
        vertex_colors: None,
        glow: None,
    }]);
    let overlay = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::BitmapText {
            font_name: "miso",
            font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
            text: Arc::<str>::from("ATTR"),
            text_changes: Arc::from([]),
            stroke_color: None,
            attributes: Arc::clone(&compiled),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let mut scratch = song_lua_projected_mesh_scratch_for(std::slice::from_ref(&overlay));
    let actor = build_song_lua_overlay_actor_with_scratch(
        &overlay,
        SongLuaOverlayState::default(),
        None,
        &AssetManager::new(),
        781,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
        scratch.first_mut(),
    )
    .expect_actor("compiled text attributes should render");

    let Actor::Text {
        attributes: TextAttributes::Shared(rendered),
        ..
    } = actor
    else {
        panic!("expected text with shared attributes");
    };
    assert!(Arc::ptr_eq(&compiled, &rendered));
}

#[test]
fn song_lua_bitmaptext_respects_text_glow_mode_at_runtime() {
    let overlay = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::BitmapText {
            font_name: "miso",
            font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
            text: Arc::<str>::from("GLOW"),
            text_changes: Arc::from([]),
            stroke_color: Some([0.0, 0.0, 0.0, 0.5]),
            attributes: empty_text_attributes(),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let mut scratch = song_lua_projected_mesh_scratch_for(std::slice::from_ref(&overlay));
    let actors = build_song_lua_overlay_actor_with_scratch(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            glow: [0.2, 0.3, 0.4, 0.5],
            text_glow_mode: SongLuaTextGlowMode::Stroke,
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        781,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
        scratch.first_mut(),
    )
    .expect_actors("text glow bitmap text should render");

    let first_ptr = match actors.as_slice() {
        [
            _,
            Actor::Text {
                color,
                stroke_color,
                attributes: TextAttributes::Reusable(attributes),
                blend,
                ..
            },
        ] => {
            assert_eq!(color, &[1.0, 1.0, 1.0, 1.0]);
            assert_eq!(stroke_color, &Some([0.2, 0.3, 0.4, 0.5]));
            assert_eq!(blend, &BlendMode::Add);
            assert_eq!(attributes.len(), 1);
            assert_eq!(attributes[0].color, [1.0, 1.0, 1.0, 0.0]);
            Arc::as_ptr(attributes)
        }
        other => panic!("expected text plus stroke-only glow actors, got {other:?}"),
    };
    drop(actors);

    let actors = build_song_lua_overlay_actor_with_scratch(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            glow: [0.2, 0.3, 0.4, 0.5],
            text_glow_mode: SongLuaTextGlowMode::Stroke,
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        781,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
        scratch.first_mut(),
    )
    .expect_actors("prewarmed text glow should render again");
    let [
        _,
        Actor::Text {
            attributes: TextAttributes::Reusable(attributes),
            ..
        },
    ] = actors.as_slice()
    else {
        panic!("expected reusable stroke-only glow attributes");
    };
    assert_eq!(Arc::as_ptr(attributes), first_ptr);
    assert_eq!(scratch[0].replacements, 0);
}

#[test]
fn song_lua_bitmaptext_attribute_glow_adds_runtime_glow_pass() {
    let overlay = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::BitmapText {
            font_name: "miso",
            font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
            text: Arc::<str>::from("GLOW"),
            text_changes: Arc::from([]),
            stroke_color: None,
            attributes: Arc::from([TextAttribute {
                start: 1,
                length: 2,
                color: [1.0, 1.0, 1.0, 1.0],
                vertex_colors: None,
                glow: Some([0.7, 0.3, 0.9, 0.5]),
            }]),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let mut scratch = song_lua_projected_mesh_scratch_for(std::slice::from_ref(&overlay));
    let actors = build_song_lua_overlay_actor_with_scratch(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        783,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
        scratch.first_mut(),
    )
    .expect_actors("attribute glow bitmap text should render");

    let first_ptr = match actors.as_slice() {
        [
            _,
            Actor::Text {
                color,
                stroke_color,
                attributes: TextAttributes::Reusable(attributes),
                blend,
                ..
            },
        ] => {
            assert_eq!(color, &[1.0, 1.0, 1.0, 1.0]);
            assert_eq!(stroke_color, &None);
            assert_eq!(blend, &BlendMode::Add);
            assert_eq!(attributes.len(), 1);
            assert_eq!(attributes[0].start, 1);
            assert_eq!(attributes[0].length, 2);
            assert_eq!(attributes[0].color, [0.7, 0.3, 0.9, 0.5]);
            Arc::as_ptr(attributes)
        }
        other => panic!("expected text plus attribute glow actors, got {other:?}"),
    };
    drop(actors);

    let actors = build_song_lua_overlay_actor_with_scratch(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        783,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
        scratch.first_mut(),
    )
    .expect_actors("prewarmed attribute glow should render again");
    let [
        _,
        Actor::Text {
            attributes: TextAttributes::Reusable(attributes),
            ..
        },
    ] = actors.as_slice()
    else {
        panic!("expected reusable attribute glow attributes");
    };
    assert_eq!(Arc::as_ptr(attributes), first_ptr);
    assert_eq!(scratch[0].replacements, 0);
}

#[test]
fn song_lua_ddr_explosions_honor_resolution_hints() {
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        640.0, 480.0,
    ));
    for skin in ["ddr-note", "ddr-vivid", "ddr-rainbow"] {
        for (file, expected) in [
            ("Down Tap Explosion Dim (doubleres).png", 64.0),
            ("Down Tap Explosion Bright.png", 96.0),
        ] {
            let path = workspace_root()
                .join("assets/noteskins/dance")
                .join(skin)
                .join(file);
            let key = path.to_string_lossy().into_owned();
            let mut assets = AssetManager::new();
            assets.queue_texture_upload(key.clone(), image::open(&path).unwrap().into_rgba8());
            let overlay = SongLuaOverlayActor {
                kind: test_sprite_kind(&key),
                name: None,
                parent_index: None,
                initial_state: SongLuaOverlayState::default(),
                message_commands: Vec::new(),
            };
            let mut scratch = SongLuaProjectedMeshScratch::textured(PROJECTED_MESH_VERTEX_CAPACITY);
            // Cover both the immediate and retained bindings, and the skin's zoom tween.
            for retained in [false, true, true] {
                for zoom in [1.0, 1.1] {
                    let actor = build_song_lua_overlay_actor_with_scratch(
                        &overlay,
                        SongLuaOverlayState {
                            x: 320.0,
                            y: 240.0,
                            zoom,
                            ..Default::default()
                        },
                        None,
                        &assets,
                        1,
                        screen_width(),
                        screen_height(),
                        0.0,
                        0.0,
                        0.0,
                        retained.then_some(&mut scratch),
                    )
                    .expect_actor("DDR explosion");
                    let Actor::Sprite {
                        size: [SizeSpec::Px(w), SizeSpec::Px(h)],
                        ..
                    } = actor
                    else {
                        panic!("expected explosion sprite");
                    };
                    assert!((w - expected * zoom).abs() < 0.001, "{skin}/{file}: {w}");
                    assert!((h - expected * zoom).abs() < 0.001, "{skin}/{file}: {h}");
                }
            }
        }
    }
}

#[test]
fn song_lua_sprite_binding_tracks_availability_key_changes_and_reload() {
    let mut textures = deadlib_assets::TextureStore::<()>::new();
    let key: Arc<str> = Arc::from("lua-binding 4x2.png");
    let other: Arc<str> = Arc::from("lua-binding-other 2x1.png");
    let mut scratch = SongLuaProjectedMeshScratch::default();
    let state = SongLuaOverlayState {
        sprite_state_index: Some(5),
        ..Default::default()
    };
    assert!(scratch.bind_sprite(&key, &textures).is_none());
    let original = textures.reserve_texture_handle(key.to_string());
    let unmeasured = scratch.bind_sprite(&key, &textures).unwrap();
    assert_eq!(song_lua_overlay_sprite_size(state, unmeasured), None);
    assert_eq!(
        song_lua_overlay_sprite_size(
            SongLuaOverlayState {
                size: Some([7.0, 9.0]),
                ..state
            },
            unmeasured,
        ),
        Some([7.0, 9.0])
    );
    textures.queue_texture_upload(key.to_string(), image::RgbaImage::new(40, 20));
    let bound = scratch.bind_sprite(&key, &textures).unwrap();
    assert_eq!(bound.handle, original);
    assert_eq!(
        song_lua_overlay_sprite_size(state, bound),
        Some([10.0, 10.0])
    );
    assert_eq!(
        song_lua_overlay_uv_rect(state, Some(bound.sheet), &[], 0.0),
        Some([0.25, 0.5, 0.5, 1.0])
    );
    textures.queue_texture_upload(key.to_string(), image::RgbaImage::new(80, 40));
    let resized = scratch.bind_sprite(&key, &textures).unwrap();
    assert_eq!(resized.handle, original);
    assert_eq!(
        song_lua_overlay_sprite_size(state, resized),
        Some([20.0, 20.0])
    );
    textures.insert_texture(other.to_string(), (), 60, 10);
    let other_bound = scratch.bind_sprite(&other, &textures).unwrap();
    assert_eq!(other_bound.sheet, (2, 1));
    assert_ne!(other_bound.handle, original);
    assert_eq!(
        scratch.bind_sprite(&key, &textures).unwrap().handle,
        original
    );
    textures.take_textures();
    assert!(scratch.bind_sprite(&key, &textures).is_none());
    textures.insert_texture(key.to_string(), (), 120, 60);
    let reloaded = scratch.bind_sprite(&key, &textures).unwrap();
    assert_ne!(reloaded.handle, original);
    assert_eq!(
        song_lua_overlay_sprite_size(state, reloaded),
        Some([30.0, 30.0])
    );
}

#[test]
fn song_lua_bound_sprite_draw_refreshes_identity_size_and_uv() {
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    let key = "lua-bound-draw 4x2.png";
    let mut assets = AssetManager::new();
    let overlay = SongLuaOverlayActor {
        kind: test_sprite_kind(key),
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let mut scratch = SongLuaProjectedMeshScratch::textured(PROJECTED_MESH_VERTEX_CAPACITY);
    let mut draw = |assets: &AssetManager| {
        build_song_lua_overlay_actor_with_scratch(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                sprite_state_index: Some(5),
                ..Default::default()
            },
            None,
            assets,
            1,
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            0.0,
            Some(&mut scratch),
        )
    };
    assert!(draw(&assets).is_none());
    for width in [40, 80] {
        assets.queue_texture_upload(key.into(), image::RgbaImage::new(width, 20));
        for _ in 0..3 {
            match draw(&assets).expect_actor("prepared sprite must render") {
                Actor::Sprite {
                    source:
                        SpriteSource::TextureHandle {
                            handle, generation, ..
                        },
                    size,
                    uv_rect,
                    ..
                } => {
                    assert_eq!(handle, assets.texture_context().texture_handle(key));
                    assert_eq!(generation, assets.texture_context().revision());
                    assert!(
                        matches!(size, [SizeSpec::Px(w), SizeSpec::Px(h)] if w == width as f32 / 4.0 && h == 10.0)
                    );
                    assert_eq!(uv_rect, Some([0.25, 0.5, 0.5, 1.0]));
                }
                actor => panic!("expected bound sprite, got {actor:?}"),
            }
        }
    }
    assets.remove_texture(key);
    assert!(draw(&assets).is_none());
}

#[test]
#[ignore = "manual Lua sprite binding benchmark; run in release mode"]
fn song_lua_sprite_binding_hot_paths() {
    use std::{hint::black_box, time::Instant};
    let key: Arc<str> = Arc::from("benchmark-lua-native-sprite 4x2.png");
    let missing: Arc<str> = Arc::from("benchmark-lua-missing.png");
    let mut assets = AssetManager::new();
    assets.queue_texture_upload(key.to_string(), image::RgbaImage::new(40, 20));
    let textures = assets.texture_context();
    let overlay = SongLuaOverlayActor {
        kind: test_sprite_kind(&key),
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let state = SongLuaOverlayState {
        x: 320.0,
        y: 240.0,
        sprite_state_index: Some(5),
        ..Default::default()
    };
    let mut scratch = SongLuaProjectedMeshScratch::textured(PROJECTED_MESH_VERTEX_CAPACITY);
    let bound = scratch.bind_sprite(&key, textures).unwrap();
    let width = screen_width();
    let height = screen_height();
    const REPEATS: usize = 100_000;
    for mode in ["bound", "size", "uv", "draw", "resolve", "missing"] {
        let mut samples = [0.0_f64; 7];
        if mode == "missing" {
            scratch.bind_sprite(&missing, textures);
        }
        for sample in &mut samples {
            let started = Instant::now();
            for _ in 0..REPEATS {
                match mode {
                    "bound" => {
                        black_box(scratch.bind_sprite(black_box(&key), black_box(textures)));
                    }
                    "size" => {
                        black_box(song_lua_overlay_sprite_size(
                            black_box(state),
                            black_box(bound),
                        ));
                    }
                    "uv" => {
                        black_box(song_lua_overlay_uv_rect(
                            black_box(state),
                            Some(black_box(bound.sheet)),
                            &[],
                            0.0,
                        ));
                    }
                    "draw" => {
                        black_box(build_song_lua_overlay_actor_with_scratch(
                            black_box(&overlay),
                            state,
                            None,
                            &assets,
                            1,
                            width,
                            height,
                            0.0,
                            0.0,
                            0.0,
                            Some(&mut scratch),
                        ));
                    }
                    "resolve" => {
                        black_box(textures.bind_texture(black_box(&key)));
                    }
                    "missing" => {
                        black_box(scratch.bind_sprite(black_box(&missing), black_box(textures)));
                    }
                    _ => unreachable!(),
                }
            }
            *sample = started.elapsed().as_nanos() as f64 / REPEATS as f64;
        }
        samples.sort_by(f64::total_cmp);
        eprintln!(
            "lua sprite {mode}: median {:.1} ns, worst batch {:.1} ns/op",
            samples[3], samples[6]
        );
    }
}

#[test]
fn song_lua_sprite_setstate_uses_sheet_cell_size_at_runtime() {
    let key = "song-lua-test 4x3.png".to_string();
    let mut asset_manager = AssetManager::new();
    asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(40, 30));
    let overlay = SongLuaOverlayActor {
        kind: test_sprite_kind(&key),
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let actor = build_song_lua_overlay_actor(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            sprite_state_index: Some(5),
            ..SongLuaOverlayState::default()
        },
        None,
        &asset_manager,
        778,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("setstate sprite should render");

    match actor {
        Actor::Sprite {
            size, uv_rect, z, ..
        } => {
            let expected_w = 10.0 * screen_width() / 640.0;
            let expected_h = 10.0 * screen_height() / 480.0;
            assert_eq!(z, 778);
            assert_eq!(uv_rect, Some([0.25, 1.0 / 3.0, 0.5, 2.0 / 3.0]));
            match size {
                [SizeSpec::Px(w), SizeSpec::Px(h)] => {
                    assert!((w - expected_w).abs() <= 0.000_1);
                    assert!((h - expected_h).abs() <= 0.000_1);
                }
                other => panic!("expected explicit sprite size, got {other:?}"),
            }
        }
        other => panic!("expected sprite overlay, got {other:?}"),
    }
}

#[test]
fn song_lua_sprite_setstate_restarts_custom_animation() {
    let key = "song-lua-animation-reset 4x1.png".to_string();
    let mut asset_manager = AssetManager::new();
    asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(40, 10));
    let overlay = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::Sprite {
            texture_path: key.clone().into(),
            texture_key: Arc::from(key.as_str()),
            states: Arc::from([
                deadsync_song_lua::SongLuaSpriteState {
                    frame: 0,
                    delay: 0.07,
                },
                deadsync_song_lua::SongLuaSpriteState {
                    frame: 1,
                    delay: 0.07,
                },
                deadsync_song_lua::SongLuaSpriteState {
                    frame: 2,
                    delay: 0.07,
                },
                deadsync_song_lua::SongLuaSpriteState {
                    frame: 3,
                    delay: 9_999.0,
                },
            ]),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let actor = build_song_lua_overlay_actor(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            sprite_animate: true,
            sprite_loop: false,
            sprite_state_index: Some(0),
            sprite_animation_epoch: Some(10.0),
            ..SongLuaOverlayState::default()
        },
        None,
        &asset_manager,
        778,
        640.0,
        480.0,
        10.08,
        0.0,
        42.0,
    )
    .expect_actor("reset animated sprite should render");

    let Actor::Sprite { uv_rect, .. } = actor else {
        panic!("expected sprite overlay");
    };
    assert_eq!(uv_rect, Some([0.25, 0.0, 0.5, 1.0]));
}

#[test]
fn song_lua_update_setstate_records_animation_epoch() {
    let tracks = [deadsync_song_lua::SongLuaOverlayRuntimeUpdateTrack {
        overlay_index: 0,
        target: deadsync_song_lua::SongLuaOverlayUpdateTarget::SpriteStateIndex,
        samples: vec![
            deadsync_song_lua::SongLuaOverlayRuntimeUpdateSample {
                second: 1.0,
                value: deadsync_song_lua::SongLuaOverlayUpdateValue::U32(0),
            },
            deadsync_song_lua::SongLuaOverlayRuntimeUpdateSample {
                second: 2.0,
                value: deadsync_song_lua::SongLuaOverlayUpdateValue::U32(2),
            },
        ],
    }];
    let mut cursors = [0];
    let mut state = SongLuaOverlayState::default();
    apply_song_lua_overlay_runtime_updates_for(1.1, &tracks, 0..1, &mut cursors, None, &mut state);
    assert_eq!(state.sprite_state_index, Some(0));
    assert_eq!(state.sprite_animation_epoch, Some(1.0));

    apply_song_lua_overlay_runtime_updates_for(2.0, &tracks, 0..1, &mut cursors, None, &mut state);
    assert_eq!(state.sprite_state_index, Some(2));
    assert_eq!(state.sprite_animation_epoch, Some(2.0));
}

#[test]
fn spooky_door_vertices_match_native_tween() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let native: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("itgmania-actors/spooky-doors.json")).unwrap(),
    )
    .unwrap();
    let mut context = deadsync_assets::song_lua::SongLuaCompileContext::new(
        &root.join("song_lua"),
        "Spooky doors",
    );
    context.screen_width = 854.0;
    context.music_length_seconds = 1.0;
    let compiled = deadsync_assets::song_lua::compile_song_lua(
        &root.join("song_lua/spooky-door.lua"),
        &context,
    )
    .unwrap();
    let mut assets = AssetManager::new();
    assets.queue_texture_upload("spooky-door.png".into(), image::RgbaImage::new(854, 480));
    let mut corners = 0;
    for mut door in compiled
        .overlays
        .into_iter()
        .filter(|o| o.name.as_deref().is_some_and(|n| n.starts_with("door")))
    {
        door.kind = test_sprite_kind("spooky-door.png");
        let command = door
            .message_commands
            .iter()
            .find(|c| c.message == "SlideDoor")
            .unwrap();
        for sample in native["samples"].as_array().unwrap() {
            let elapsed = sample["time"].as_f64().unwrap() as f32;
            let state = deadsync_song_lua::overlay_state_after_blocks(
                door.initial_state,
                &command.blocks,
                elapsed,
            );
            let actor = build_song_lua_overlay_actor(
                &door, state, None, &assets, 0, 854.0, 480.0, elapsed, 0.0, elapsed,
            )
            .expect_actor("door sprite");
            let Actor::Sprite {
                offset,
                size,
                align,
                cropleft,
                cropright,
                croptop,
                cropbottom,
                ..
            } = actor
            else {
                panic!("door sprite");
            };
            let [SizeSpec::Px(width), SizeSpec::Px(height)] = size else {
                panic!("door size");
            };
            assert_eq!(align, [0.0, 0.0]);
            let left = (offset[0] + width * cropleft) * 854.0 / screen_width();
            let right = (offset[0] + width * (1.0 - cropright)) * 854.0 / screen_width();
            let top = (offset[1] + height * croptop) * 480.0 / screen_height();
            let bottom = (offset[1] + height * (1.0 - cropbottom)) * 480.0 / screen_height();
            let oracle = sample["actors"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["name"].as_str() == door.name.as_deref())
                .unwrap();
            for (point, vertex) in [[left, top], [left, bottom], [right, bottom], [right, top]]
                .iter()
                .zip(oracle["draws"][0]["vertices"].as_array().unwrap())
            {
                for axis in 0..2 {
                    assert!(
                        (point[axis] - vertex["screen"][axis].as_f64().unwrap() as f32).abs()
                            < 0.001,
                        "{} at {elapsed}: {point:?}, native={vertex}",
                        door.name.as_deref().unwrap()
                    );
                }
                corners += 1;
            }
        }
    }
    assert_eq!(corners, 264);
}

#[test]
fn song_lua_target_sheet_uses_filename_grid_for_physical_size() {
    let key = r"C:\songs\Botanic Panic\lua\ayaze\target 4x2.png".to_string();
    let mut asset_manager = AssetManager::new();
    asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(256, 128));
    let overlay = SongLuaOverlayActor {
        kind: test_sprite_kind(&key),
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let actor = build_song_lua_overlay_actor(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            zoom: 0.75,
            zoom_x: 0.75,
            zoom_y: 0.75,
            zoom_z: 0.75,
            basezoom_x: -1.0,
            sprite_animate: true,
            ..SongLuaOverlayState::default()
        },
        None,
        &asset_manager,
        778,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("target sheet sprite should render");

    let Actor::Sprite { size, uv_rect, .. } = actor else {
        panic!("expected target sprite overlay");
    };
    let [SizeSpec::Px(width), SizeSpec::Px(height)] = size else {
        panic!("expected explicit target sprite size");
    };
    assert!((width - 48.0 * screen_width() / 640.0).abs() <= 0.000_1);
    assert!((height - 48.0 * screen_height() / 480.0).abs() <= 0.000_1);
    assert_eq!(uv_rect, Some([0.0, 0.0, 0.25, 0.5]));
}

#[test]
fn song_lua_zero_zoom_sheet_does_not_fall_back_to_native_size() {
    let key = r"C:\songs\Botanic Panic\lua\ayaze\target 4x2.png".to_string();
    let mut asset_manager = AssetManager::new();
    asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(256, 128));
    let overlay = SongLuaOverlayActor {
        kind: test_sprite_kind(&key),
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };

    assert!(
        build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                zoom: 0.0,
                zoom_x: 0.0,
                zoom_y: 0.0,
                zoom_z: 0.0,
                basezoom_x: -1.0,
                sprite_animate: true,
                ..SongLuaOverlayState::default()
            },
            None,
            &asset_manager,
            778,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .is_none()
    );
}

#[test]
fn song_lua_paused_sheet_keeps_first_frame_uv_at_runtime() {
    let key = "song-lua-paused 4x3.png".to_string();
    let mut asset_manager = AssetManager::new();
    asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(40, 30));
    let overlay = SongLuaOverlayActor {
        kind: test_sprite_kind(&key),
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let actor = build_song_lua_overlay_actor(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            sprite_animate: false,
            ..SongLuaOverlayState::default()
        },
        None,
        &asset_manager,
        778,
        640.0,
        480.0,
        0.0,
        0.0,
        2.0,
    )
    .expect_actor("paused sheet sprite should render");

    let Actor::Sprite { uv_rect, .. } = actor else {
        panic!("expected paused sprite overlay");
    };
    assert_eq!(uv_rect, Some([0.0, 0.0, 0.25, 1.0 / 3.0]));
}

#[test]
fn song_lua_custom_sprite_state_maps_to_declared_frame_at_runtime() {
    let key = "song-lua-custom-state 4x3.png".to_string();
    let mut asset_manager = AssetManager::new();
    asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(40, 30));
    let overlay = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::Sprite {
            texture_path: key.clone().into(),
            texture_key: Arc::from(key.as_str()),
            states: Arc::from([
                deadsync_song_lua::SongLuaSpriteState {
                    frame: 2,
                    delay: 999.0,
                },
                deadsync_song_lua::SongLuaSpriteState {
                    frame: 5,
                    delay: 0.1,
                },
            ]),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let actor = build_song_lua_overlay_actor(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            sprite_animate: false,
            sprite_state_index: Some(1),
            ..SongLuaOverlayState::default()
        },
        None,
        &asset_manager,
        778,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("custom-state sprite should render");

    let Actor::Sprite { uv_rect, .. } = actor else {
        panic!("expected custom-state sprite overlay");
    };
    assert_eq!(uv_rect, Some([0.25, 1.0 / 3.0, 0.5, 2.0 / 3.0]));
}

#[test]
fn song_lua_sprite_animation_advances_sheet_frames_at_runtime() {
    let key = "song-lua-animate 4x3.png".to_string();
    let mut asset_manager = AssetManager::new();
    asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(40, 30));
    let overlay = SongLuaOverlayActor {
        kind: test_sprite_kind(&key),
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let actor = build_song_lua_overlay_actor(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            sprite_state_index: Some(1),
            sprite_animate: true,
            sprite_loop: true,
            sprite_playback_rate: 1.0,
            sprite_state_delay: 0.5,
            ..SongLuaOverlayState::default()
        },
        None,
        &asset_manager,
        779,
        640.0,
        480.0,
        0.0,
        0.0,
        1.1,
    )
    .expect_actor("animated sprite should render");

    match actor {
        Actor::Sprite { uv_rect, z, .. } => {
            assert_eq!(z, 779);
            assert_eq!(uv_rect, Some([0.75, 0.0, 1.0, 1.0 / 3.0]));
        }
        other => panic!("expected animated sprite overlay, got {other:?}"),
    }
}

#[test]
fn song_lua_sprite_animation_applies_rate_and_loop_controls_at_runtime() {
    let key = "song-lua-animate-rate 4x3.png".to_string();
    let mut asset_manager = AssetManager::new();
    asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(40, 30));
    let overlay = SongLuaOverlayActor {
        kind: test_sprite_kind(&key),
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let actor = build_song_lua_overlay_actor(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            sprite_state_index: Some(1),
            sprite_animate: true,
            sprite_loop: false,
            sprite_playback_rate: 2.0,
            sprite_state_delay: 0.5,
            ..SongLuaOverlayState::default()
        },
        None,
        &asset_manager,
        780,
        640.0,
        480.0,
        0.0,
        0.0,
        10.0,
    )
    .expect_actor("rate-controlled sprite should render");

    match actor {
        Actor::Sprite { uv_rect, z, .. } => {
            assert_eq!(z, 780);
            assert_eq!(uv_rect, Some([0.75, 2.0 / 3.0, 1.0, 1.0]));
        }
        other => panic!("expected animated sprite overlay, got {other:?}"),
    }
}

#[test]
fn song_lua_sprite_applies_texture_translate_to_uv_rect() {
    let key = "song-lua-translate.png".to_string();
    let mut asset_manager = AssetManager::new();
    asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(40, 30));
    let overlay = SongLuaOverlayActor {
        kind: test_sprite_kind(&key),
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let actor = build_song_lua_overlay_actor(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            texture_wrapping: true,
            texcoord_offset: Some([0.25, -0.5]),
            ..SongLuaOverlayState::default()
        },
        None,
        &asset_manager,
        781,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("translated sprite should render");

    match actor {
        Actor::Sprite { uv_rect, z, .. } => {
            assert_eq!(z, 781);
            assert_eq!(uv_rect, Some([0.25, -0.5, 1.25, 0.5]));
        }
        other => panic!("expected translated sprite overlay, got {other:?}"),
    }
}

#[test]
fn song_lua_sprite_renders_vertex_diffuse_as_mesh() {
    let key = "song-lua-vertex-diffuse.png".to_string();
    let mut asset_manager = AssetManager::new();
    asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(40, 30));
    let overlay = SongLuaOverlayActor {
        kind: test_sprite_kind(&key),
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let actor = build_song_lua_overlay_actor(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            vertex_colors: Some([
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0, 1.0],
                [1.0, 1.0, 0.0, 1.0],
                [0.0, 0.0, 1.0, 1.0],
            ]),
            ..SongLuaOverlayState::default()
        },
        None,
        &asset_manager,
        782,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("vertex-diffuse sprite should render");

    match actor {
        Actor::TexturedMesh { vertices, z, .. } => {
            assert_eq!(z, 782);
            assert_eq!(vertices.len(), 6);
            assert_eq!(vertices[0].color, [1.0, 0.0, 0.0, 1.0]);
            assert_eq!(vertices[1].color, [0.0, 1.0, 0.0, 1.0]);
            assert_eq!(vertices[2].color, [0.0, 0.0, 1.0, 1.0]);
            assert_eq!(vertices[5].color, [1.0, 1.0, 0.0, 1.0]);
        }
        other => panic!("expected textured mesh-backed vertex diffuse, got {other:?}"),
    }
}

#[test]
fn song_lua_sprite_applies_fade_edges_at_runtime() {
    let key = "song-lua-fade-edges.png".to_string();
    let mut asset_manager = AssetManager::new();
    asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(40, 30));
    let overlay = SongLuaOverlayActor {
        kind: test_sprite_kind(&key),
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let actor = build_song_lua_overlay_actor(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            fadeleft: 0.1,
            faderight: 0.2,
            fadetop: 0.3,
            fadebottom: 0.4,
            ..SongLuaOverlayState::default()
        },
        None,
        &asset_manager,
        782,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("faded sprite should render");

    match actor {
        Actor::Sprite {
            fadeleft,
            faderight,
            fadetop,
            fadebottom,
            z,
            ..
        } => {
            assert_eq!(z, 782);
            assert!((fadeleft - 0.1).abs() <= 0.000_1);
            assert!((faderight - 0.2).abs() <= 0.000_1);
            assert!((fadetop - 0.3).abs() <= 0.000_1);
            assert!((fadebottom - 0.4).abs() <= 0.000_1);
        }
        other => panic!("expected faded sprite overlay, got {other:?}"),
    }
}

#[test]
fn song_lua_overlay_applies_skew_at_runtime() {
    let key = "song-lua-skew.png".to_string();
    let mut asset_manager = AssetManager::new();
    asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(40, 30));
    let overlay = SongLuaOverlayActor {
        kind: test_sprite_kind(&key),
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let actor = build_song_lua_overlay_actor(
        &overlay,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            skew_x: 0.5,
            skew_y: 0.25,
            ..SongLuaOverlayState::default()
        },
        None,
        &asset_manager,
        783,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("skewed sprite should render");

    match actor {
        Actor::TexturedMesh { vertices, z, .. } => {
            assert_eq!(z, 783);
            assert_eq!(vertices.len(), 6);
            let x_scale = screen_width() / 640.0;
            let y_scale = screen_height() / 480.0;
            let center = [320.0 * x_scale, 240.0 * y_scale];
            let half = [20.0 * x_scale, 15.0 * y_scale];
            let top_left = test_skewed_overlay_point(center, [-half[0], -half[1]], 0.5, 0.25);
            let bottom_right = test_skewed_overlay_point(center, [half[0], half[1]], 0.5, 0.25);
            assert!((vertices[0].pos[0] - top_left[0]).abs() <= 0.001);
            assert!((vertices[0].pos[1] - top_left[1]).abs() <= 0.001);
            assert!((vertices[2].pos[0] - bottom_right[0]).abs() <= 0.001);
            assert!((vertices[2].pos[1] - bottom_right[1]).abs() <= 0.001);
        }
        other => panic!("expected skewed textured mesh overlay, got {other:?}"),
    }
}

#[test]
fn song_lua_overlay_applies_mask_flags_at_runtime() {
    let quad = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::Quad,
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let quad_actor = build_song_lua_overlay_actor(
        &quad,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            size: Some([100.0, 50.0]),
            mask_source: true,
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        783,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("masked quad should render");

    match quad_actor {
        Actor::Sprite {
            mask_source,
            mask_dest,
            z,
            ..
        } => {
            assert_eq!(z, 783);
            assert!(mask_source);
            assert!(!mask_dest);
        }
        other => panic!("expected masked quad sprite, got {other:?}"),
    }

    let text = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::BitmapText {
            font_name: "miso",
            font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
            text: Arc::<str>::from("MASK"),
            text_changes: Arc::from([]),
            stroke_color: None,
            attributes: empty_text_attributes(),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let text_actor = build_song_lua_overlay_actor(
        &text,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            mask_dest: true,
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        784,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("masked text should render");

    match text_actor {
        Actor::Text { mask_dest, z, .. } => {
            assert_eq!(z, 784);
            assert!(mask_dest);
        }
        other => panic!("expected masked text actor, got {other:?}"),
    }
}

#[test]
fn song_lua_overlay_applies_alignment_at_runtime() {
    let quad = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::Quad,
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let quad_actor = build_song_lua_overlay_actor(
        &quad,
        SongLuaOverlayState {
            x: 100.0,
            y: 200.0,
            size: Some([80.0, 40.0]),
            halign: 0.0,
            valign: 1.0,
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        785,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("aligned quad should render");

    match quad_actor {
        Actor::Sprite { align, z, .. } => {
            assert_eq!(z, 785);
            assert_eq!(align, [0.0, 1.0]);
        }
        other => panic!("expected aligned quad sprite, got {other:?}"),
    }

    let text = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::BitmapText {
            font_name: "miso",
            font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
            text: Arc::<str>::from("ALIGN"),
            text_changes: Arc::from([]),
            stroke_color: None,
            attributes: empty_text_attributes(),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let text_actor = build_song_lua_overlay_actor(
        &text,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            halign: 1.0,
            valign: 0.0,
            text_align: TextAlign::Right,
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        786,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("aligned text should render");

    match text_actor {
        Actor::Text {
            align,
            align_text,
            z,
            ..
        } => {
            assert_eq!(z, 786);
            assert_eq!(align, [1.0, 0.0]);
            assert_eq!(align_text, TextAlign::Right);
        }
        other => panic!("expected aligned text actor, got {other:?}"),
    }
}

#[test]
fn song_lua_overlay_applies_runtime_actor_shadow() {
    let quad = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::Quad,
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let quad_actor = build_song_lua_overlay_actor(
        &quad,
        SongLuaOverlayState {
            x: 100.0,
            y: 200.0,
            size: Some([80.0, 40.0]),
            shadow_len: [3.0, -4.0],
            shadow_color: [0.1, 0.2, 0.3, 0.4],
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        787,
        screen_width(),
        screen_height(),
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("shadowed quad should render");

    match quad_actor {
        Actor::Sprite {
            z,
            shadow_len,
            shadow_color,
            ..
        } => {
            assert_eq!(z, 787);
            assert_eq!(shadow_len, [3.0, -4.0]);
            assert_eq!(shadow_color, [0.1, 0.2, 0.3, 0.4]);
        }
        other => panic!("expected shadowed quad sprite, got {other:?}"),
    }
}

#[test]
fn song_lua_overlay_applies_extra_blend_modes_at_runtime() {
    let sprite_key = "song-lua-multiply.png".to_string();
    let mut asset_manager = AssetManager::new();
    asset_manager.queue_texture_upload(sprite_key.clone(), image::RgbaImage::new(40, 30));

    let sprite = SongLuaOverlayActor {
        kind: test_sprite_kind(&sprite_key),
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let sprite_actor = build_song_lua_overlay_actor(
        &sprite,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            blend: SongLuaOverlayBlendMode::Multiply,
            ..SongLuaOverlayState::default()
        },
        None,
        &asset_manager,
        788,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("multiply sprite should render");

    match sprite_actor {
        Actor::Sprite { blend, z, .. } => {
            assert_eq!(z, 788);
            assert_eq!(blend, BlendMode::Multiply);
        }
        other => panic!("expected multiply sprite actor, got {other:?}"),
    }

    let quad = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::Quad,
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let quad_actor = build_song_lua_overlay_actor(
        &quad,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            size: Some([100.0, 50.0]),
            blend: SongLuaOverlayBlendMode::Subtract,
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        789,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("subtract quad should render");

    match quad_actor {
        Actor::Sprite { blend, z, .. } => {
            assert_eq!(z, 789);
            assert_eq!(blend, BlendMode::Subtract);
        }
        other => panic!("expected subtract quad actor, got {other:?}"),
    }
}

#[test]
fn song_lua_sprite_glow_draws_once_with_native_blend() {
    use deadlib_render_core::DrawOp;
    let key = "song-lua-cyber-glow.png";
    let mut assets = AssetManager::new();
    assets.queue_texture_upload(key.to_owned(), image::RgbaImage::new(64, 64));
    let overlay = SongLuaOverlayActor {
        kind: test_sprite_kind(key),
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let metrics = deadlib_present::space::Metrics::centered(640.0, 480.0);
    for camera in [
        None,
        Some(SongLuaOverlayState {
            fov: Some(45.0),
            vanishpoint: Some([320.0, 240.0]),
            ..Default::default()
        }),
    ] {
        for cached in [false, true] {
            for blend in [SongLuaOverlayBlendMode::Alpha, SongLuaOverlayBlendMode::Add] {
                let mut scratch = SongLuaProjectedMeshScratch::default();
                let actors = build_song_lua_overlay_actor_with_scratch(
                    &overlay,
                    SongLuaOverlayState {
                        x: 320.0,
                        y: 240.0,
                        diffuse: [1.0, 1.0, 1.0, 0.8],
                        glow: [1.0, 1.0, 1.0, 0.4],
                        blend,
                        ..Default::default()
                    },
                    camera,
                    &assets,
                    0,
                    640.0,
                    480.0,
                    0.0,
                    0.0,
                    0.0,
                    cached.then_some(&mut scratch),
                )
                .expect("visible glowing Sprite");
                let frame = deadlib_present::compose::build_screen_with_texture_context(
                    &actors,
                    [0.0; 4],
                    &metrics,
                    &font::FontMap::default(),
                    0.0,
                    assets.texture_context(),
                );
                assert_eq!(
                    frame.sprite_instances.len() + frame.tmesh_instances.len(),
                    2,
                    "Sprite::DrawTexture emits one diffuse and one glow: camera={camera:?}, cached={cached}"
                );
                for op in &frame.ops {
                    let actual = match op {
                        DrawOp::Sprite(run) => run.blend,
                        DrawOp::TexturedMesh(run) => run.blend,
                        other => panic!("unexpected Sprite draw: {other:?}"),
                    };
                    assert_eq!(actual, song_lua_overlay_blend(blend));
                }
                let masks = frame
                    .sprite_instances
                    .iter()
                    .map(|i| i.texture_mask)
                    .chain(frame.tmesh_instances.iter().map(|i| i.texture_mask))
                    .collect::<Vec<_>>();
                assert_eq!(masks, [0.0, 1.0]);
            }
        }
    }
}

#[test]
fn song_lua_overlay_wraps_runtime_actors_with_glow() {
    let sprite_key = "song-lua-glow.png".to_string();
    let mut asset_manager = AssetManager::new();
    asset_manager.queue_texture_upload(sprite_key.clone(), image::RgbaImage::new(32, 24));

    let sprite = SongLuaOverlayActor {
        kind: test_sprite_kind(&sprite_key),
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let sprite_actors = build_song_lua_overlay_actor(
        &sprite,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            glow: [0.1, 0.2, 0.3, 0.4],
            ..SongLuaOverlayState::default()
        },
        None,
        &asset_manager,
        790,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
    )
    .expect_actors("glowing sprite should render");

    match sprite_actors.as_slice() {
        [
            Actor::Sprite {
                tint,
                glow,
                blend,
                z,
                ..
            },
        ] => {
            assert_eq!(blend, &BlendMode::Alpha);
            assert_eq!(z, &790);
            assert_eq!(tint, &[1.0; 4]);
            assert_eq!(glow, &[0.1, 0.2, 0.3, 0.4]);
        }
        other => panic!("expected one sprite with its native glow pass, got {other:?}"),
    }

    let quad = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::Quad,
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let quad_actors = build_song_lua_overlay_actor(
        &quad,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            size: Some([100.0, 50.0]),
            diffuse: [1.0, 1.0, 1.0, 0.0],
            effect_mode: deadlib_present::anim::EffectMode::GlowShift,
            effect_color1: [0.3, 0.4, 0.5, 0.6],
            effect_color2: [0.1, 0.2, 0.3, 0.1],
            effect_period: 1.0,
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        791,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
    );
    assert!(
        quad_actors.is_none(),
        "transparent glowshift must not emit a glow pass"
    );
}

#[test]
fn song_lua_projected_overlay_applies_fade_edges_at_runtime() {
    let sprite_key = "song-lua-projected-fade.png".to_string();
    let mut asset_manager = AssetManager::new();
    asset_manager.queue_texture_upload(sprite_key.clone(), image::RgbaImage::new(64, 32));

    let sprite = SongLuaOverlayActor {
        kind: test_sprite_kind(&sprite_key),
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let state = SongLuaOverlayState {
        x: 320.0,
        y: 240.0,
        diffuse: [0.8, 0.7, 0.6, 0.5],
        fadeleft: 0.25,
        faderight: 0.25,
        ..SongLuaOverlayState::default()
    };
    let camera = Some(SongLuaOverlayState {
        fov: Some(45.0),
        ..SongLuaOverlayState::default()
    });
    let actor = build_song_lua_overlay_actor(
        &sprite,
        state,
        camera,
        &asset_manager,
        792,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("projected fading sprite should render");

    let expected_vertices = match actor {
        Actor::TexturedMesh {
            tint, vertices, z, ..
        } => {
            assert_eq!(z, 792);
            assert_eq!(tint, [0.8, 0.7, 0.6, 0.5]);
            assert_eq!(vertices.len(), 18);
            assert!(vertices.iter().all(|vertex| {
                (vertex.color[0] - 1.0).abs() <= 0.000_1
                    && (vertex.color[1] - 1.0).abs() <= 0.000_1
                    && (vertex.color[2] - 1.0).abs() <= 0.000_1
            }));
            assert!(vertices.iter().any(|vertex| vertex.color[3] <= 0.000_1));
            assert!(
                vertices
                    .iter()
                    .any(|vertex| (vertex.color[3] - 1.0).abs() <= 0.000_1)
            );
            vertices
        }
        other => panic!("expected projected textured mesh, got {other:?}"),
    };

    let mut scratch = SongLuaProjectedMeshScratch::textured(PROJECTED_MESH_VERTEX_CAPACITY);
    let build_reused = |scratch: &mut SongLuaProjectedMeshScratch| {
        build_song_lua_overlay_actor_with_scratch(
            &sprite,
            state,
            camera,
            &asset_manager,
            792,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
            Some(scratch),
        )
        .expect_actor("projected fading sprite should reuse its mesh")
    };
    let Actor::ReusableTexturedMesh {
        vertices: reused_vertices,
        ..
    } = build_reused(&mut scratch)
    else {
        panic!("expected reusable projected textured mesh");
    };
    assert_eq!(expected_vertices.as_ref(), reused_vertices.as_slice());
    let buffer_ptr = Arc::as_ptr(&reused_vertices);
    drop(reused_vertices);
    let Actor::ReusableTexturedMesh {
        vertices: next_vertices,
        ..
    } = build_reused(&mut scratch)
    else {
        panic!("expected reusable projected textured mesh");
    };
    assert_eq!(Arc::as_ptr(&next_vertices), buffer_ptr);
    assert_eq!(scratch.replacements, 0);
}

#[test]
fn song_lua_overlay_applies_bitmaptext_layout_at_runtime() {
    let text = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::BitmapText {
            font_name: "miso",
            font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
            text: Arc::<str>::from("WRAP"),
            text_changes: Arc::from([]),
            stroke_color: None,
            attributes: empty_text_attributes(),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let text_actor = build_song_lua_overlay_actor(
        &text,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            wrap_width_pixels: Some(64),
            max_width: Some(80.0),
            max_height: Some(40.0),
            max_w_pre_zoom: true,
            max_h_pre_zoom: false,
            text_jitter: true,
            text_distortion: 0.5,
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        787,
        screen_width(),
        screen_height(),
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("bitmap text layout should render");

    match text_actor {
        Actor::Text {
            wrap_width_pixels,
            max_width,
            max_height,
            max_w_pre_zoom,
            max_h_pre_zoom,
            jitter,
            distortion,
            z,
            ..
        } => {
            assert_eq!(z, 787);
            assert_eq!(wrap_width_pixels, Some(64));
            assert_eq!(max_width, Some(80.0));
            assert_eq!(max_height, Some(40.0));
            assert!(max_w_pre_zoom);
            assert!(!max_h_pre_zoom);
            assert!(jitter);
            assert_eq!(distortion, 0.5);
        }
        other => panic!("expected bitmap text actor with layout settings, got {other:?}"),
    }
}

#[test]
fn song_lua_bitmaptext_max_dimension_use_zoom_reaches_runtime() {
    let text = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::BitmapText {
            font_name: "miso",
            font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
            text: Arc::<str>::from("USEZOOM"),
            text_changes: Arc::from([]),
            stroke_color: None,
            attributes: empty_text_attributes(),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let text_actor = build_song_lua_overlay_actor(
        &text,
        SongLuaOverlayState {
            max_width: Some(80.0),
            max_height: Some(40.0),
            max_w_pre_zoom: true,
            max_h_pre_zoom: true,
            max_dimension_uses_zoom: true,
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        0,
        screen_width(),
        screen_height(),
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("bitmap text max dimension zoom should render");

    match text_actor {
        Actor::Text {
            max_w_pre_zoom,
            max_h_pre_zoom,
            ..
        } => {
            assert!(!max_w_pre_zoom);
            assert!(!max_h_pre_zoom);
        }
        other => panic!("expected bitmap text actor with max-dimension zoom, got {other:?}"),
    }
}

#[test]
fn song_lua_overlay_applies_bitmaptext_attributes_at_runtime() {
    let text = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::BitmapText {
            font_name: "miso",
            font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
            text: Arc::<str>::from("ATTR"),
            text_changes: Arc::from([]),
            stroke_color: None,
            attributes: Arc::from([TextAttribute {
                start: 1,
                length: 2,
                color: [0.2, 0.4, 0.6, 0.8],
                vertex_colors: None,
                glow: None,
            }]),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let text_actor = build_song_lua_overlay_actor(
        &text,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        791,
        screen_width(),
        screen_height(),
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("bitmap text with attributes should render");

    match text_actor {
        Actor::Text { attributes, z, .. } => {
            assert_eq!(z, 791);
            assert_eq!(attributes.len(), 1);
            assert_eq!(attributes[0].start, 1);
            assert_eq!(attributes[0].length, 2);
            assert_eq!(attributes[0].color, [0.2, 0.4, 0.6, 0.8]);
        }
        other => panic!("expected bitmap text actor with attributes, got {other:?}"),
    }
}

#[test]
fn song_lua_bitmaptext_attributes_can_ignore_actor_diffuse_at_runtime() {
    let text = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::BitmapText {
            font_name: "miso",
            font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
            text: Arc::<str>::from("ATTR"),
            text_changes: Arc::from([]),
            stroke_color: None,
            attributes: Arc::from([TextAttribute {
                start: 1,
                length: 2,
                color: [0.2, 0.4, 0.6, 0.8],
                vertex_colors: None,
                glow: None,
            }]),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let mut scratch = song_lua_projected_mesh_scratch_for(std::slice::from_ref(&text));
    let text_actor = build_song_lua_overlay_actor_with_scratch(
        &text,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            diffuse: [0.5, 0.6, 0.7, 0.9],
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        792,
        screen_width(),
        screen_height(),
        0.0,
        0.0,
        0.0,
        scratch.first_mut(),
    )
    .expect_actor("bitmap text with non-multiplied attributes should render");

    let first_ptr = match text_actor {
        Actor::Text {
            color,
            attributes: TextAttributes::Reusable(attributes),
            z,
            ..
        } => {
            assert_eq!(z, 792);
            assert_eq!(color, [1.0, 1.0, 1.0, 1.0]);
            assert_eq!(attributes.len(), 2);
            assert_eq!(attributes[0].start, 0);
            assert_eq!(attributes[0].length, 4);
            assert_eq!(attributes[0].color, [0.5, 0.6, 0.7, 0.9]);
            assert_eq!(attributes[1].start, 1);
            assert_eq!(attributes[1].length, 2);
            assert_eq!(attributes[1].color, [0.2, 0.4, 0.6, 0.8]);
            Arc::as_ptr(&attributes)
        }
        other => {
            panic!("expected bitmap text actor with non-multiplied attributes, got {other:?}")
        }
    };

    let text_actor = build_song_lua_overlay_actor_with_scratch(
        &text,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            diffuse: [0.5, 0.6, 0.7, 0.9],
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        792,
        screen_width(),
        screen_height(),
        0.0,
        0.0,
        0.0,
        scratch.first_mut(),
    )
    .expect_actor("prewarmed diffuse attributes should render again");
    let Actor::Text {
        attributes: TextAttributes::Reusable(attributes),
        ..
    } = text_actor
    else {
        panic!("expected reusable diffuse attributes");
    };
    assert_eq!(Arc::as_ptr(&attributes), first_ptr);
    assert_eq!(scratch[0].replacements, 0);
}

#[test]
fn song_lua_overlay_applies_bitmaptext_uppercase_and_vertspacing_at_runtime() {
    let text = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::BitmapText {
            font_name: "miso",
            font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
            text: Arc::<str>::from("Mixed Straße"),
            text_changes: Arc::from([]),
            stroke_color: None,
            attributes: empty_text_attributes(),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let mut scratch = song_lua_projected_mesh_scratch_for(std::slice::from_ref(&text))
        .pop()
        .expect("bitmap text should have overlay scratch");
    let cached = Arc::clone(
        scratch
            .uppercase_text
            .as_ref()
            .expect("uppercase text should be prewarmed"),
    );
    let build = |scratch: &mut SongLuaProjectedMeshScratch| {
        build_song_lua_overlay_actor_with_scratch(
            &text,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                uppercase: true,
                vert_spacing: Some(18),
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            788,
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            0.0,
            Some(scratch),
        )
        .expect_actor("bitmap text uppercase and vertspacing should render")
    };
    let text_actor = build(&mut scratch);

    match text_actor {
        Actor::Text {
            content,
            line_spacing,
            z,
            ..
        } => {
            assert_eq!(z, 788);
            assert_eq!(content.as_str(), "MIXED STRASSE");
            let TextContent::Shared(shared) = content else {
                panic!("prewarmed uppercase text should stay shared");
            };
            assert!(Arc::ptr_eq(&shared, &cached));
            assert_eq!(line_spacing, Some(18));
        }
        other => {
            panic!("expected bitmap text actor with uppercase and vertspacing, got {other:?}")
        }
    }
    let Actor::Text { content, .. } = build(&mut scratch) else {
        panic!("second uppercase frame should remain text");
    };
    let TextContent::Shared(shared) = content else {
        panic!("second uppercase frame should stay shared");
    };
    assert!(Arc::ptr_eq(&shared, &cached));
}

#[test]
fn song_lua_overlay_applies_bitmaptext_skew_at_runtime() {
    let text = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::BitmapText {
            font_name: "miso",
            font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
            text: Arc::<str>::from("SKEW"),
            text_changes: Arc::from([]),
            stroke_color: None,
            attributes: empty_text_attributes(),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let text_actor = build_song_lua_overlay_actor(
        &text,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            skew_x: 0.15,
            skew_y: -0.35,
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        789,
        screen_width(),
        screen_height(),
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("bitmap text skew should render");

    match text_actor {
        Actor::Text {
            local_transform, z, ..
        } => {
            let actual = local_transform.to_cols_array();
            let expected =
                song_lua_overlay_local_transform([0.0, 0.0, 0.0], 0.15, -0.35).to_cols_array();
            assert_eq!(z, 789);
            assert!(
                actual
                    .iter()
                    .zip(expected.iter())
                    .all(|(a, b)| (a - b).abs() <= 0.000_1)
            );
        }
        other => panic!("expected skewed bitmap text actor, got {other:?}"),
    }
}

#[test]
fn song_lua_overlay_applies_bitmaptext_fit_size_at_runtime() {
    let text = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::BitmapText {
            font_name: "miso",
            font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
            text: Arc::<str>::from("FIT"),
            text_changes: Arc::from([]),
            stroke_color: None,
            attributes: empty_text_attributes(),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let text_actor = build_song_lua_overlay_actor(
        &text,
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            size: Some([120.0, 30.0]),
            ..SongLuaOverlayState::default()
        },
        None,
        &AssetManager::new(),
        790,
        screen_width(),
        screen_height(),
        0.0,
        0.0,
        0.0,
    )
    .expect_actor("bitmap text fit size should render");

    match text_actor {
        Actor::Text {
            fit_width,
            fit_height,
            z,
            ..
        } => {
            assert_eq!(z, 790);
            assert_eq!(fit_width, Some(120.0));
            assert_eq!(fit_height, Some(30.0));
        }
        other => panic!("expected bitmap text actor with fit size, got {other:?}"),
    }
}

#[test]
fn song_lua_foreground_owner_index_matches_visibility_and_layer_start() {
    let path = std::path::PathBuf::from("badapple.avi");
    let layer = |start_second: f32, overlay: SongLuaOverlayActor| {
        deadsync_gameplay::SongLuaVisualLayerRuntime {
            start_second,
            screen_width: 640.0,
            screen_height: 480.0,
            overlays: vec![overlay],
            overlay_eases: Vec::new(),
            overlay_ease_ranges: vec![0..0],
            overlay_events: vec![Vec::new()],
            song_foreground: SongLuaCapturedActor::default(),
            song_foreground_events: Vec::new(),
        }
    };
    let overlay = || SongLuaOverlayActor {
        kind: test_sprite_path_kind(path.clone()),
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let visuals = SongLuaRuntimeVisuals {
        overlays: Vec::new(),
        overlay_eases: Vec::new(),
        overlay_ease_ranges: Vec::new(),
        overlay_events: Vec::new(),
        background_visual_layers: vec![layer(5.0, overlay())],
        foreground_visual_layers: vec![layer(10.0, overlay())],
        player_actors: std::array::from_fn(|_| SongLuaCapturedActor::default()),
        player_events: std::array::from_fn(|_| Vec::new()),
        player_judgment_events: std::array::from_fn(|_| Vec::new()),
        player_combo_events: std::array::from_fn(|_| Vec::new()),
        song_foreground: SongLuaCapturedActor::default(),
        song_foreground_events: Vec::new(),
        hidden_players: [false; MAX_PLAYERS],
        hidden_screen_layers: [false; 2],
        note_hides: std::array::from_fn(|_| deadsync_gameplay::SongLuaNoteHideWindows::default()),
        column_offsets: std::array::from_fn(|_| Vec::new()),
        screen_width: 640.0,
        screen_height: 480.0,
    };
    let mut index = SongLuaForegroundOwnerIndex::new(&visuals);
    index.select(Some(path.as_path()));
    let root_states = Vec::new();
    let mut background_states = vec![vec![SongLuaOverlayState::default()]];
    let mut foreground_states = vec![vec![SongLuaOverlayState {
        visible: false,
        ..SongLuaOverlayState::default()
    }]];

    assert!(!index.owns(
        4.99,
        &visuals,
        &root_states,
        &background_states,
        &foreground_states,
    ));
    assert!(index.owns(
        5.0,
        &visuals,
        &root_states,
        &background_states,
        &foreground_states,
    ));
    background_states[0][0].visible = false;
    assert!(!index.owns(
        9.99,
        &visuals,
        &root_states,
        &background_states,
        &foreground_states,
    ));
    foreground_states[0][0].visible = true;
    assert!(index.owns(
        10.0,
        &visuals,
        &root_states,
        &background_states,
        &foreground_states,
    ));

    index.select(Some(Path::new("not-owned.avi")));
    assert!(!index.owns(
        10.0,
        &visuals,
        &root_states,
        &background_states,
        &foreground_states,
    ));
}

#[test]
fn song_lua_overlay_order_sorts_siblings_by_draworder() {
    let overlays = vec![
        test_order_overlay(SongLuaOverlayKind::ActorFrame, None, 20),
        test_order_overlay(SongLuaOverlayKind::Quad, Some(0), 10),
        test_order_overlay(SongLuaOverlayKind::Quad, Some(0), -5),
        test_order_overlay(SongLuaOverlayKind::Quad, None, -10),
    ];
    let states = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();

    assert_eq!(
        song_lua_overlay_order(&overlays, &states, None),
        [3, 0, 2, 1]
    );
}

#[test]
fn song_lua_overlay_order_sorts_children_by_z_when_enabled() {
    let overlays = vec![
        SongLuaOverlayActor {
            kind: SongLuaOverlayKind::ActorFrame,
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState {
                draw_by_z_position: true,
                ..SongLuaOverlayState::default()
            },
            message_commands: Vec::new(),
        },
        test_order_overlay(SongLuaOverlayKind::Quad, Some(0), 100),
        test_order_overlay(SongLuaOverlayKind::Quad, Some(0), -100),
        test_order_overlay(SongLuaOverlayKind::Quad, Some(0), 0),
    ];
    let mut states = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    states[1].z = -20.0;
    states[2].z = 5.0;
    states[3].z = 0.0;

    assert_eq!(
        song_lua_overlay_order(&overlays, &states, None),
        [0, 1, 3, 2]
    );
}

#[test]
fn song_lua_layers_keep_background_player_foreground_order() {
    let highest_background = SONG_LUA_BACKGROUND_DEPTH
        .shifted(f32::MAX)
        .draw_z(usize::MAX);
    let player_layer = song_lua_player_layer_z(
        true,
        &SongLuaCapturedActor::default(),
        SongLuaOverlayState::default(),
        0.0,
    );
    let highest_notefield_layer = song_lua_add_z(player_layer, 200);
    let foreground_layer = song_lua_add_z(SONG_LUA_OVERLAY_LAYER_Z_BASE, 0);

    assert!(
        highest_background < player_layer,
        "background Lua must stay below the isolated player/notefield subtree"
    );
    assert!(
        highest_notefield_layer <= foreground_layer,
        "foreground Lua should draw over the isolated player/notefield subtree"
    );
    // flip69 has more than 100 foreground actors, and its final arrow used
    // to cross the transition's z=1200 solely because of its draw index.
    for draw_index in [0, 100, 357, usize::MAX] {
        assert!(
            SONG_LUA_FOREGROUND_DEPTH
                .shifted(f32::MAX)
                .draw_z(draw_index)
                <= LUA_FOREGROUND_Z_MAX
        );
    }
}

#[test]
fn song_lua_overlay_delta_applies_depth_filtering_and_draw_by_z() {
    let mut state = SongLuaOverlayState::default();
    apply_overlay_delta(
        &mut state,
        &SongLuaOverlayStateDelta {
            depth_test: Some(true),
            draw_by_z_position: Some(true),
            texture_filtering: Some(false),
            ..SongLuaOverlayStateDelta::default()
        },
    );

    assert!(state.depth_test);
    assert!(state.draw_by_z_position);
    assert!(!state.texture_filtering);
}

#[test]
fn gameplay_presentation_scratch_presizes_only_active_players() {
    let actors = player_scratch::<Actor>(1, 384);
    let draws = player_scratch::<FlatDraw>(1, NOTEFIELD_ACTOR_SCRATCH_CAPACITY);

    assert!(actors.iter().all(Vec::is_empty));
    assert!(actors[0].capacity() >= 384);
    assert_eq!(actors[1].capacity(), 0);
    assert!(draws.iter().all(Vec::is_empty));
    assert!(draws[0].capacity() >= NOTEFIELD_ACTOR_SCRATCH_CAPACITY);
    assert_eq!(draws[1].capacity(), 0);
}

#[test]
fn compositor_runs_without_a_theme_and_preserves_lua_across_hud_styles() {
    crate::tests::init_paths();
    let simfile = workspace_root().join("tests/fixtures/song_lua/display-size.ssc");
    let song = Arc::new(deadsync_simfile::app_runtime::parse_song_for_test(&simfile, 0.0).unwrap());
    let chart = Arc::new(song.charts[0].clone());
    let charts = [chart.clone(), chart];
    let gameplay_chart = Arc::new(
        deadsync_simfile::app_runtime::load_gameplay_charts(&song, &[0], 0.0)
            .unwrap()
            .remove(0),
    );
    let gameplay_charts = [gameplay_chart.clone(), gameplay_chart];
    let profiles = std::array::from_fn(|_| profile_data::Profile::default());
    let scroll = [ScrollSpeedSetting::XMod(1.0); MAX_PLAYERS];
    let viewport = GameplayViewport::design();
    let session = GameplaySession::default();
    let config = GameplayConfig::default();
    let prepared = prepare_fixture(
        &song,
        &charts,
        std::array::from_fn(|p| &gameplay_charts[p].timing),
        &profiles,
        &scroll,
        1.0,
        viewport,
        (854, 480),
        &session,
        &config,
        BackendType::VulkanWgpu,
    );
    assert!(
        prepared.primary.is_some(),
        "the fixture must exercise real Lua compilation"
    );
    let sounds = song_lua_sound_paths(&prepared);
    let mut state: GameplayCoreState = deadsync_gameplay::init_gameplay_runtime(
        song,
        charts,
        gameplay_charts,
        viewport,
        session,
        config,
        deadsync_chart::SyncPref::Default,
        Default::default(),
        Default::default(),
        prepared,
        |_, _, _, _, _| Vec::new(),
        5,
        1.0,
        scroll,
        profiles.map(deadsync_profile_gameplay::GameplayProfile::from),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        [deadsync_gameplay::CourseLifeConfig::Bar; MAX_PLAYERS],
        false,
        [0; MAX_PLAYERS],
    );
    let mut media = SongMedia::new(&state, sounds);
    let assets = AssetManager::new();
    let mut scratches = [FrameScratch::new(&state), FrameScratch::new(&state)];
    let options = FrameOptions {
        play_style: deadsync_gameplay::GameplayInputPlayStyle::Single,
        player_side: deadsync_gameplay::GameplayInputPlayerSide::P1,
        hide_song_bg: [false; MAX_PLAYERS],
        notefield: ViewOverride::default(),
        hide_gameplay_hud: false,
        apply_attacks: true,
        show_song_visuals: true,
    };
    // Include a backward seek to exercise the reusable runtime state caches.
    for now in [0.0, 0.5, 1.0, 0.25] {
        state.boundary.total_elapsed_in_screen = 10.0 + now;
        state.set_current_music_time_ns(deadsync_core::song_time::song_time_ns_from_seconds(now));
        media.refresh_foreground(&state);
        let mut field_requests = [Vec::new(), Vec::new()];
        for (style, scratch) in scratches.iter_mut().enumerate() {
            let mut actors = Vec::new();
            let mut hud_count = 0;
            let segments = compose_frame(
                &mut actors,
                &state,
                &media,
                scratch,
                &assets,
                options,
                |layer, actors, _| {
                    if matches!(layer, ScreenLayer::Hud) {
                        let mut actor = test_source_actor();
                        if let Actor::Frame { background, .. } = &mut actor {
                            *background =
                                Some(deadlib_present::actors::Background::Color(if style == 0 {
                                    [1.0, 0.0, 0.0, 1.0]
                                } else {
                                    [0.0, 1.0, 0.0, 1.0]
                                }));
                        }
                        actors.push(actor);
                        hud_count += 1;
                    }
                },
                |request, _, _, _, _, _| {
                    field_requests[style].push((
                        request.player,
                        request.judgment_visible,
                        request.combo_visible,
                    ));
                    deadsync_notefield::BuiltNotefield {
                        layout_center_x: 200.0,
                        field_camera: None,
                        field_camera_generation: 0,
                        field_actors: None,
                        field_draw_range: None,
                        judgment_actors: None,
                        judgment_draw_range: None,
                        combo_actors: None,
                        combo_draw_range: None,
                    }
                },
            );
            assert_eq!(hud_count, 1, "each caller supplies its own HUD");
            assert!(segments.segments(scratch, &actors).count() > 0);
            assert_eq!(
                scratch.render_targets().len(),
                3,
                "all authored AFT passes survive"
            );
        }
        assert_eq!(field_requests[0], field_requests[1]);
        assert!(!field_requests[0].is_empty());
        assert_eq!(
            scratches[0].song_lua_overlay_state_scratch,
            scratches[1].song_lua_overlay_state_scratch
        );
        let target_sizes = |scratch: &FrameScratch| {
            scratch
                .render_targets()
                .iter()
                .map(|target| (target.size, target.logical_size, target.children.len()))
                .collect::<Vec<_>>()
        };
        assert_eq!(target_sizes(&scratches[0]), target_sizes(&scratches[1]));
    }
}

fn prepare_fixture(
    song: &SongData,
    charts: &[Arc<ChartData>; MAX_PLAYERS],
    timing: [&TimingData; MAX_PLAYERS],
    player_profiles: &[profile_data::Profile; MAX_PLAYERS],
    scroll_speed: &[ScrollSpeedSetting; MAX_PLAYERS],
    music_rate: f32,
    viewport: GameplayViewport,
    display_size: (u32, u32),
    session: &GameplaySession,
    config: &GameplayConfig,
    video_renderer: BackendType,
) -> super::PreparedGameplaySongLua<SpriteSlot> {
    let context = deadsync_profile_gameplay::song_lua_play_context(
        song,
        charts,
        timing,
        player_profiles,
        scroll_speed,
        music_rate,
        viewport,
        display_size,
        session,
        config,
        &video_renderer.to_string(),
    );
    super::prepare_song_lua(
        song,
        &context,
        compile_song_lua,
        deadsync_assets::song_lua::compile_song_lua_layers,
    )
}
