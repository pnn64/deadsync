//! Original VM-free data/helper unit fixtures against the public library.
use super::*;
use deadlib_present::{
    actors::{TextAlign, TextAttribute},
    anim::{EffectClock, EffectMode},
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};
type TestOverlayKind = SongLuaOverlayKind<(), (), TextAttribute>;
type TestOverlayActor = SongLuaOverlayActor<TestOverlayKind>;
type TestCompiledSongLua = CompiledSongLua<TestOverlayActor>;

fn test_sprite_overlay(path: PathBuf, decode_movie: bool) -> TestOverlayActor {
    TestOverlayActor {
        kind: TestOverlayKind::Sprite {
            texture_key: Arc::from(path.to_string_lossy().into_owned()),
            texture_path: path,
            states: Arc::from([]),
            textures: Arc::from([]),
        },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState {
            decode_movie,
            ..SongLuaOverlayState::default()
        },
        message_commands: Vec::new(),
    }
}

fn test_sound_overlay(path: PathBuf) -> TestOverlayActor {
    TestOverlayActor {
        kind: TestOverlayKind::Sound { sound_path: path },
        name: None,
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    }
}

fn mod_window(start: f32, limit: f32, mods: &str) -> SongLuaModWindow {
    SongLuaModWindow {
        unit: SongLuaTimeUnit::Beat,
        start,
        limit,
        span_mode: SongLuaSpanMode::Len,
        mods: mods.to_string(),
        player: None,
    }
}

fn ease_window(start: f32, limit: f32) -> SongLuaEaseWindow {
    SongLuaEaseWindow {
        approach_speed: None,
        unit: SongLuaTimeUnit::Beat,
        start,
        limit,
        span_mode: SongLuaSpanMode::Len,
        from: 0.0,
        to: 1.0,
        target: SongLuaEaseTarget::PlayerX,
        easing: None,
        player: None,
        sustain: None,
        opt1: None,
        opt2: None,
    }
}

fn overlay_ease(overlay_index: usize, start: f32, limit: f32) -> SongLuaOverlayEase {
    SongLuaOverlayEase {
        overlay_index,
        unit: SongLuaTimeUnit::Beat,
        start,
        limit,
        span_mode: SongLuaSpanMode::Len,
        from: SongLuaOverlayStateDelta::default(),
        to: SongLuaOverlayStateDelta::default(),
        easing: None,
        sustain: None,
        opt1: None,
        opt2: None,
    }
}

#[test]
fn song_lua_video_paths_filter_and_dedupe_video_sprites() {
    let movie = PathBuf::from("badapple.AVI");
    let overlays = vec![
        test_sprite_overlay(movie.clone(), true),
        test_sprite_overlay(movie.clone(), true),
        test_sprite_overlay(PathBuf::from("panel.png"), true),
        TestOverlayActor {
            kind: TestOverlayKind::Quad,
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        },
    ];

    assert_eq!(song_lua_video_paths(&overlays), vec![movie]);
}

#[test]
fn song_lua_video_paths_skip_disabled_video_decode() {
    let movie = PathBuf::from("badapple.AVI");
    let overlays = vec![test_sprite_overlay(movie, false)];

    assert!(song_lua_video_paths(&overlays).is_empty());
}

#[test]
fn push_song_lua_video_paths_reuses_seen_keys() {
    let movie = PathBuf::from("badapple.AVI");
    let overlays = vec![test_sprite_overlay(movie.clone(), true)];
    let mut paths = vec![movie.clone()];
    let mut seen = HashSet::from(["badapple.AVI"]);

    push_song_lua_video_paths(&overlays, &mut seen, &mut paths);

    assert_eq!(paths, vec![movie]);
}

#[test]
fn push_song_lua_overlay_sound_paths_filters_and_dedupes_sounds() {
    let sound = PathBuf::from("sound.ogg");
    let overlays = vec![
        test_sound_overlay(sound.clone()),
        test_sound_overlay(sound.clone()),
        test_sprite_overlay(PathBuf::from("movie.avi"), true),
    ];
    let mut paths = Vec::new();
    let mut seen = HashSet::new();

    push_song_lua_overlay_sound_paths(&overlays, &mut seen, &mut paths);

    assert_eq!(paths, vec![sound]);
}

#[test]
fn compiled_song_lua_sound_paths_preserves_first_seen_order() {
    let mut first = TestCompiledSongLua::default();
    first.sound_paths = vec![PathBuf::from("a.ogg"), PathBuf::from("b.ogg")];
    let mut second = TestCompiledSongLua::default();
    second.sound_paths = vec![PathBuf::from("b.ogg"), PathBuf::from("c.ogg")];

    assert_eq!(
        compiled_song_lua_sound_paths([&first, &second]),
        vec![
            PathBuf::from("a.ogg"),
            PathBuf::from("b.ogg"),
            PathBuf::from("c.ogg")
        ]
    );
}

#[test]
fn song_lua_clock_uses_native_bpm_segments() {
    assert_eq!(
        crate::parse_song_timing_bpms("0.000=120.000,4.000=60.000"),
        vec![(0.0, 120.0), (4.0, 60.0)]
    );
    let mut context = SongLuaCompileContext::new(".", "Variable BPM");
    context.song_timing_bpms = vec![(0.0, 120.0), (4.0, 60.0)];
    assert!((crate::song_elapsed_seconds_at(4.0, &context) - 2.0).abs() < 1e-6);
    assert!((crate::song_elapsed_seconds_at(6.0, &context) - 4.0).abs() < 1e-6);
    assert!((crate::song_beat_at_elapsed_seconds(4.0, &context) - 6.0).abs() < 1e-6);
    assert!((crate::song_bps_at_beat(5.0, &context) - 1.0).abs() < 1e-6);
}

#[test]
fn multitap_sample_eases_step_visibility_edges() {
    let baseline = SongLuaOverlayState {
        visible: false,
        ..SongLuaOverlayState::default()
    };
    let hidden = baseline;
    let visible_a = SongLuaOverlayState {
        visible: true,
        x: 100.0,
        y: -20.0,
        rot_z_deg: 90.0,
        ..SongLuaOverlayState::default()
    };
    let visible_b = SongLuaOverlayState {
        visible: true,
        x: 120.0,
        y: -10.0,
        rot_z_deg: 90.0,
        ..SongLuaOverlayState::default()
    };
    let samples = [
        (0.0, hidden),
        (0.125, visible_a),
        (0.25, visible_b),
        (0.375, hidden),
    ];
    let mut eases = Vec::new();

    push_overlay_sample_eases(&mut eases, 7, baseline, &samples);

    assert!(eases.iter().any(|ease| {
        ease.overlay_index == 7
            && ease.start == 0.125
            && ease.limit == 0.0
            && ease.to.visible == Some(true)
            && ease.to.x == Some(100.0)
            && ease.to.y == Some(-20.0)
    }));
    assert!(eases.iter().any(|ease| {
        ease.overlay_index == 7
            && ease.start == 0.125
            && ease.limit == 0.125
            && ease.from.x == Some(100.0)
            && ease.to.x == Some(120.0)
            && ease.to.visible == Some(true)
    }));
    let hide = eases
        .iter()
        .find(|ease| {
            ease.overlay_index == 7
                && ease.start == 0.375
                && ease.limit == 0.0
                && ease.to.visible == Some(false)
        })
        .expect("visibility should step off instead of tweening to the baseline");
    assert_eq!(hide.to.x, None);
    assert_eq!(hide.to.y, None);
    assert!(!eases.iter().any(|ease| {
        ease.to.visible == Some(false)
            && ease.limit > 0.0
            && (ease.to.x.is_some() || ease.to.y.is_some())
    }));
}

#[test]
fn multitap_deco_state_rotates_yinyang_during_bounce() {
    let state = multitap_deco_state(
        SongLuaOverlayState::default(),
        SongLuaNoteskinResolver::default(),
        "ddr-note",
        MultitapPhase {
            pos: 0.0,
            squish: 0.0,
            lin: 0.75,
            qtc: 1,
            visible: true,
        },
    );

    assert!(state.visible);
    assert_eq!(state.rot_z_deg, 135.0);
}

#[test]
fn multitap_arrow_sampler_does_not_emit_inactive_baseline() {
    let baseline = SongLuaOverlayState {
        visible: true,
        rot_z_deg: 0.0,
        ..SongLuaOverlayState::default()
    };
    let mut samples = Vec::new();

    push_multitap_arrow_sample(
        &mut samples,
        10.0,
        baseline,
        SongLuaNoteskinResolver::default(),
        "ddr-note",
        1,
        MultitapPhase {
            pos: 0.0,
            squish: 0.0,
            lin: 1.0,
            qtc: 1,
            visible: true,
        },
    );
    push_multitap_arrow_sample(
        &mut samples,
        10.001,
        baseline,
        SongLuaNoteskinResolver::default(),
        "ddr-note",
        1,
        MultitapPhase {
            pos: 0.0,
            squish: 0.0,
            lin: 0.0,
            qtc: 0,
            visible: false,
        },
    );

    assert_eq!(samples.len(), 1);
    assert!(samples[0].1.visible);
    assert_eq!(samples[0].1.rot_z_deg, 90.0);

    let mut eases = Vec::new();
    push_overlay_sample_eases(&mut eases, 3, baseline, &samples);

    assert!(!eases.iter().any(|ease| {
        ease.overlay_index == 3
            && ease.limit > 0.0
            && ease.from.rot_z_deg == Some(90.0)
            && ease.to.rot_z_deg == Some(0.0)
    }));
    assert!(
        !eases
            .iter()
            .any(|ease| ease.overlay_index == 3 && ease.to.visible == Some(false))
    );
}

#[test]
fn actor_indices_for_pointers_matches_known_actor_tables() {
    let actor_ptrs = HashSet::from([20_usize, 40]);

    assert_eq!(
        actor_indices_for_pointers(5, |index| (index + 1) * 10, &actor_ptrs),
        vec![1, 3]
    );
}

#[test]
fn update_function_sample_step_bounds_long_timelines() {
    let len = 512.0;
    let step = crate::update_function_sample_step(len);
    assert!((len / step).ceil() as usize <= SONG_LUA_UPDATE_FUNCTION_MAX_SAMPLES);
    assert_eq!(crate::update_function_sample_step(0.0), 0.0);
}

#[test]
fn theme_children_follow_screen_hierarchy() {
    assert!(!TOP_SCREEN_THEME_CHILD_NAMES.contains(&"BPMDisplay"));
    assert!(TOP_SCREEN_THEME_CHILD_NAMES.contains(&"StepsDisplayP2"));
    assert!(!UNDERLAY_THEME_CHILD_NAMES.contains(&"BPMDisplay"));
    assert!(UNDERLAY_THEME_CHILD_NAMES.contains(&"StepStatsPaneP1"));
    assert!(UNDERLAY_THEME_CHILD_NAMES.contains(&"SongMeter"));
}

#[test]
fn runtime_static_overlay_index_by_path_matches_static_texture_name() {
    let paths = [
        PathBuf::from("sprites/normal.png"),
        PathBuf::from("sprites/_STATIC 4x1.PNG"),
        PathBuf::from("sprites/other.png"),
    ];

    assert_eq!(
        runtime_static_overlay_index_by_path(paths.len(), |index| Some(paths[index].as_path())),
        Some(1)
    );
    assert_eq!(
        runtime_static_overlay_index_by_path(paths.len(), |index| (index == 0)
            .then_some(paths[index].as_path())),
        None
    );
}

#[test]
fn overlay_model_layer_new_preserves_fields() {
    let draw = SongLuaOverlayModelDraw::new(
        [1.0, 2.0, 3.0],
        [4.0, 5.0, 6.0],
        [7.0, 8.0, 9.0],
        [0.1, 0.2, 0.3, 0.4],
        0.5,
        true,
        false,
    );
    let layer = SongLuaOverlayModelLayer::new(
        Arc::from("model.png"),
        Arc::from([11_u32, 12_u32]),
        [13.0, 14.0],
        [15.0, 16.0],
        [17.0, 18.0],
        [19.0, 20.0],
        [21.0, 22.0],
        Some(23.0),
        draw,
    );

    assert_eq!(layer.texture_key.as_ref(), "model.png");
    assert_eq!(layer.vertices.as_ref(), &[11, 12]);
    assert_eq!(layer.model_size, [13.0, 14.0]);
    assert_eq!(layer.uv_scale, [15.0, 16.0]);
    assert_eq!(layer.uv_offset, [17.0, 18.0]);
    assert_eq!(layer.uv_tex_shift, [19.0, 20.0]);
    assert_eq!(layer.uv_velocity, [21.0, 22.0]);
    assert_eq!(layer.uv_cycle_seconds, Some(23.0));
    assert_eq!(layer.draw, draw);
}

#[test]
fn message_command_lists_have_listener_matches_command_name() {
    let first = vec![SongLuaOverlayMessageCommand {
        frame_advance: 0.0,
        message: "Alpha".to_string(),
        blocks: Vec::new(),
    }];
    let second = vec![SongLuaOverlayMessageCommand {
        frame_advance: 0.0,
        message: "Beta".to_string(),
        blocks: Vec::new(),
    }];

    assert!(message_command_lists_have_listener(
        [first.as_slice(), second.as_slice()],
        "Beta"
    ));
    assert!(!message_command_lists_have_listener(
        [first.as_slice(), second.as_slice()],
        "Gamma"
    ));
}

#[test]
fn push_startup_message_if_listened_adds_zero_beat_event() {
    let commands = vec![SongLuaOverlayMessageCommand {
        frame_advance: 0.0,
        message: SONG_LUA_STARTUP_MESSAGE.to_string(),
        blocks: Vec::new(),
    }];
    let mut messages = Vec::new();

    push_startup_message_if_listened(&mut messages, [commands.as_slice()]);

    assert_eq!(
        messages,
        vec![SongLuaMessageEvent {
            beat: 0.0,
            message: SONG_LUA_STARTUP_MESSAGE.to_string(),
            persists: false,
        }]
    );
}

#[test]
fn sort_compiled_song_lua_orders_timeline_dtos() {
    let mut compiled = CompiledSongLua::<()>::default();
    compiled.beat_mods = vec![
        mod_window(4.0, 1.0, "b"),
        mod_window(1.0, 2.0, "a"),
        mod_window(1.0, 1.0, "c"),
    ];
    compiled.time_mods = vec![mod_window(2.0, 1.0, "x"), mod_window(1.0, 1.0, "y")];
    compiled.eases = vec![ease_window(3.0, 1.0), ease_window(2.0, 2.0)];
    compiled.overlay_eases = vec![
        overlay_ease(2, 1.0, 1.0),
        overlay_ease(1, 1.0, 1.0),
        overlay_ease(0, 1.0, 0.5),
    ];
    compiled.messages = vec![
        SongLuaMessageEvent {
            beat: 2.0,
            message: "B".to_string(),
            persists: false,
        },
        SongLuaMessageEvent {
            beat: 1.0,
            message: "A".to_string(),
            persists: false,
        },
    ];

    sort_compiled_song_lua(&mut compiled);

    assert_eq!(
        compiled
            .beat_mods
            .iter()
            .map(|window| window.mods.as_str())
            .collect::<Vec<_>>(),
        vec!["c", "a", "b"]
    );
    assert_eq!(
        compiled
            .time_mods
            .iter()
            .map(|window| window.mods.as_str())
            .collect::<Vec<_>>(),
        vec!["y", "x"]
    );
    assert_eq!(
        compiled
            .eases
            .iter()
            .map(|window| window.start)
            .collect::<Vec<_>>(),
        vec![2.0, 3.0]
    );
    assert_eq!(
        compiled
            .overlay_eases
            .iter()
            .map(|window| window.overlay_index)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(
        compiled
            .messages
            .iter()
            .map(|event| event.beat)
            .collect::<Vec<_>>(),
        vec![1.0, 2.0]
    );
}

#[test]
fn easiest_steps_difficulty_ignores_disabled_players() {
    let mut players = std::array::from_fn(|_| SongLuaPlayerContext::default());
    players[0].difficulty = SongLuaDifficulty::Beginner;
    players[0].enabled = false;
    players[1].difficulty = SongLuaDifficulty::Hard;

    assert_eq!(
        easiest_steps_difficulty(&players),
        Some(SongLuaDifficulty::Hard)
    );
}

#[test]
fn human_player_count_counts_enabled_context_players() {
    let mut context = SongLuaCompileContext::new("songs/pack/song", "Song");
    context.players[1].enabled = false;

    assert_eq!(song_lua_human_player_count(&context), 1);
}

#[test]
fn graph_display_body_size_matches_player_count() {
    assert_eq!(graph_display_body_size(1), [610.0, 64.0]);
    assert_eq!(graph_display_body_size(2), [300.0, 64.0]);
    assert_eq!(graph_display_body_size(0), [300.0, 64.0]);
}

#[test]
fn scale_to_rect_plan_matches_actor_scale_policy() {
    let fit =
        scale_to_rect_plan([10.0, 20.0, 210.0, 120.0], [50.0, 100.0], [0.5, 0.0], false).unwrap();
    assert_eq!(fit.pos, [110.0, 20.0]);
    assert_eq!(fit.zoom, 1.0);
    assert!(!fit.flip_x);
    assert!(!fit.flip_y);

    let cover =
        scale_to_rect_plan([10.0, 20.0, 210.0, 120.0], [50.0, 100.0], [0.0, 0.5], true).unwrap();
    assert_eq!(cover.pos, [10.0, 70.0]);
    assert_eq!(cover.zoom, 4.0);

    let flipped =
        scale_to_rect_plan([210.0, 120.0, 10.0, 20.0], [50.0, 100.0], [0.5, 0.5], false).unwrap();
    assert_eq!(flipped.pos, [110.0, 70.0]);
    assert!(flipped.flip_x);
    assert!(flipped.flip_y);

    assert!(scale_to_rect_plan([0.0, 0.0, 1.0, 1.0], [0.0, 1.0], [0.0, 0.0], false).is_none());
}

#[test]
fn overlay_state_scale_uses_axis_fallbacks() {
    let state = SongLuaOverlayState {
        basezoom: 2.0,
        basezoom_x: 1.0,
        basezoom_y: 3.0,
        basezoom_z: 1.0,
        zoom: 4.0,
        zoom_x: 5.0,
        zoom_y: 1.0,
        zoom_z: 6.0,
        ..SongLuaOverlayState::default()
    };
    assert_eq!(overlay_state_axis_scale(state), [10.0, 12.0]);
    assert_eq!(overlay_state_z_scale(state), 12.0);
}

#[test]
fn texture_rect_offsets_preserve_actor_host_math() {
    assert_eq!(
        offset_texture_rect([0.25, 0.5, 0.75, 1.0], [0.1, -0.2]),
        [0.35, 0.3, 0.85, 0.8]
    );
    assert_eq!(
        texture_pixel_offset_rect([0.0, 0.0, 0.5, 0.5], [200.0, 100.0], [10.0, 5.0]),
        Some([0.05, 0.05, 0.55, 0.55])
    );
    assert_eq!(
        texture_pixel_offset_rect([0.0, 0.0, 1.0, 1.0], [0.0, 100.0], [10.0, 5.0]),
        None
    );
}

#[test]
fn sprite_texture_rect_prefers_custom_then_state_sheet() {
    assert_eq!(
        sprite_texture_rect(Some([0.1, 0.2, 0.3, 0.4]), Some(2), Some((4, 2))),
        [0.1, 0.2, 0.3, 0.4]
    );
    assert_eq!(
        sprite_texture_rect(None, Some(5), Some((4, 2))),
        [0.25, 0.5, 0.5, 1.0]
    );
    assert_eq!(
        sprite_texture_rect(None, Some(SONG_LUA_SPRITE_STATE_CLEAR), Some((4, 2))),
        [0.0, 0.0, 1.0, 1.0]
    );
    assert_eq!(
        sprite_texture_rect(None, Some(1), None),
        [0.0, 0.0, 1.0, 1.0]
    );
}

#[test]
fn sprite_texture_rect_with_offset_preserves_optional_policy() {
    assert_eq!(
        sprite_texture_rect_with_offset(None, None, Some((4, 2)), None),
        None
    );
    assert_eq!(
        sprite_texture_rect_with_offset(None, None, Some((4, 2)), Some([0.25, -0.5])),
        Some([0.25, -0.5, 1.25, 0.5])
    );
    assert_eq!(
        sprite_texture_rect_with_offset(None, Some(5), Some((4, 2)), Some([0.25, -0.5])),
        Some([0.5, 0.0, 0.75, 0.5])
    );
    assert_eq!(
        sprite_texture_rect_with_offset(
            Some([0.1, 0.2, 0.3, 0.4]),
            Some(5),
            Some((4, 2)),
            Some([0.25, -0.5]),
        ),
        Some([0.1 + 0.25, 0.2 - 0.5, 0.3 + 0.25, 0.4 - 0.5])
    );
}

#[test]
fn sprite_frame_count_saturates_empty_dims() {
    assert_eq!(sprite_frame_count(None), 1);
    assert_eq!(sprite_frame_count(Some((0, 0))), 1);
    assert_eq!(sprite_frame_count(Some((4, 3))), 12);
}

#[test]
fn sprite_image_frame_size_always_uses_one_sheet_frame() {
    assert_eq!(
        sprite_image_frame_size(Some((256.0, 128.0)), false, None, Some((4, 2))),
        Some((64.0, 64.0))
    );
    assert_eq!(
        sprite_image_frame_size(Some((256.0, 128.0)), true, None, Some((4, 2))),
        Some((64.0, 64.0))
    );
    assert_eq!(
        sprite_image_frame_size(Some((256.0, 128.0)), false, Some(3), Some((4, 2))),
        Some((64.0, 64.0))
    );
    assert_eq!(
        sprite_image_frame_size(
            Some((256.0, 128.0)),
            false,
            Some(SONG_LUA_SPRITE_STATE_CLEAR),
            Some((4, 2)),
        ),
        Some((64.0, 64.0))
    );
    assert_eq!(
        sprite_image_frame_size(Some((256.0, 128.0)), true, None, Some((0, 0))),
        Some((256.0, 128.0))
    );
    assert_eq!(
        sprite_image_frame_size(None, true, Some(3), Some((4, 2))),
        None
    );
}

#[test]
fn sprite_animation_state_at_matches_stepmania_timing() {
    assert_eq!(sprite_animation_state_at(-1.0, 0.1, 4), 0);
    assert_eq!(sprite_animation_state_at(0.39, 0.1, 4), 3);
    assert_eq!(sprite_animation_state_at(0.4, 0.1, 4), 0);
    assert_eq!(sprite_animation_state_at(5.0, 0.0, 4), 0);
    assert_eq!(sprite_animation_state_at(5.0, -1.0, 4), 0);
    assert_eq!(sprite_animation_state_at(5.0, 0.1, 0), 0);
}

#[test]
fn sprite_animation_state_from_offsets_and_clamps() {
    assert_eq!(sprite_animation_state_from(2, 0.29, 1.0, 0.1, 4, true), 0);
    assert_eq!(sprite_animation_state_from(2, 0.29, 1.0, 0.1, 4, false), 3);
    assert_eq!(sprite_animation_state_from(2, -0.29, 1.0, 0.1, 4, false), 0);
    assert_eq!(sprite_animation_state_from(7, 0.29, 1.0, 0.0, 4, true), 3);
}

#[test]
fn custom_sprite_animation_uses_declared_frames_and_delays() {
    let states = [
        SongLuaSpriteState {
            frame: 2,
            delay: 1.0,
        },
        SongLuaSpriteState {
            frame: 5,
            delay: 0.25,
        },
    ];
    assert_eq!(
        sprite_custom_animation_state_from(&states, 0, 0.99, 1.0, true),
        0
    );
    assert_eq!(
        sprite_custom_animation_state_from(&states, 0, 1.0, 1.0, true),
        1
    );
    assert_eq!(
        sprite_custom_animation_state_from(&states, 0, 1.25, 1.0, true),
        0
    );
    assert_eq!(
        sprite_custom_animation_state_from(&states, 1, 0.25, 1.0, false),
        1
    );
}

#[test]
fn note_column_zoom_hide_policy_filters_unsupported_handlers() {
    assert_eq!(
        note_column_zoom_hide_beats_per_t("NoteColumnSplineMode_Offset", false, 0.5),
        Some(0.5)
    );
    assert_eq!(
        note_column_zoom_hide_beats_per_t("NoteColumnSplineMode_Disabled", false, 0.5),
        None
    );
    assert_eq!(
        note_column_zoom_hide_beats_per_t("NoteColumnSplineMode_Offset", true, 0.5),
        None
    );
    assert_eq!(
        note_column_zoom_hide_beats_per_t("NoteColumnSplineMode_Offset", false, 0.0),
        None
    );
    assert_eq!(
        note_column_zoom_hide_beats_per_t("NoteColumnSplineMode_Offset", false, f32::NAN),
        None
    );
}

#[test]
fn sort_note_hide_windows_matches_actor_host_order() {
    let mut windows = vec![
        SongLuaNoteHideWindow {
            spline_beats_per_t: 0.25,
            spline_size: 1,
            player: 1,
            column: 0,
            start_beat: 1.0,
            end_beat: 1.5,
        },
        SongLuaNoteHideWindow {
            spline_beats_per_t: 0.25,
            spline_size: 1,
            player: 0,
            column: 1,
            start_beat: 2.0,
            end_beat: 2.5,
        },
        SongLuaNoteHideWindow {
            spline_beats_per_t: 0.25,
            spline_size: 1,
            player: 0,
            column: 1,
            start_beat: 1.0,
            end_beat: 2.0,
        },
        SongLuaNoteHideWindow {
            spline_beats_per_t: 0.25,
            spline_size: 1,
            player: 0,
            column: 0,
            start_beat: 4.0,
            end_beat: 4.5,
        },
    ];
    sort_note_hide_windows(&mut windows);
    assert_eq!(
        windows
            .iter()
            .map(|window| window.player)
            .collect::<Vec<_>>(),
        vec![0, 0, 0, 1]
    );
    assert_eq!(
        windows
            .iter()
            .map(|window| (window.column, window.start_beat))
            .collect::<Vec<_>>(),
        vec![(0, 4.0), (1, 1.0), (1, 2.0), (0, 1.0)]
    );
}

#[test]
fn note_column_pos_offset_y_from_points_matches_actor_host_policy() {
    assert_eq!(
        note_column_pos_offset_y_from_points("NoteColumnSplineMode_Disabled", &[]),
        Some(0.0)
    );
    assert_eq!(
        note_column_pos_offset_y_from_points("NoteColumnSplineMode_Rotation", &[[0.0, 2.0]]),
        None
    );
    assert_eq!(
        note_column_pos_offset_y_from_points("NoteColumnSplineMode_Offset", &[]),
        Some(0.0)
    );
    assert_eq!(
        note_column_pos_offset_y_from_points(
            "NoteColumnSplineMode_Offset",
            &[[0.0, 2.0], [0.0005, 2.0005]],
        ),
        Some(2.0)
    );
    assert_eq!(
        note_column_pos_offset_y_from_points(
            "NoteColumnSplineMode_Offset",
            &[[0.0, 2.0], [0.0, 2.01]],
        ),
        None
    );
    assert_eq!(
        note_column_pos_offset_y_from_points("NoteColumnSplineMode_Offset", &[[0.01, 2.0]]),
        Some(2.0)
    );
}

#[test]
fn column_offset_windows_from_samples_match_capture_policy() {
    let params = SongLuaColumnOffsetBuildParams {
        unit: SongLuaTimeUnit::Beat,
        start: 4.0,
        limit: 8.0,
        span_mode: SongLuaSpanMode::End,
        easing: Some("linear".to_string()),
        sustain: Some(1.0),
        opt1: Some(2.0),
        opt2: Some(3.0),
    };
    let from = [
        SongLuaColumnOffsetSample {
            player: 1,
            column: 2,
            y: 12.0,
        },
        SongLuaColumnOffsetSample {
            player: 0,
            column: 1,
            y: 0.0,
        },
        SongLuaColumnOffsetSample {
            player: 0,
            column: 0,
            y: 4.0,
        },
    ];
    let to = [
        SongLuaColumnOffsetSample {
            player: 0,
            column: 1,
            y: 0.0,
        },
        SongLuaColumnOffsetSample {
            player: 0,
            column: 0,
            y: 8.0,
        },
        SongLuaColumnOffsetSample {
            player: 1,
            column: 0,
            y: -2.0,
        },
    ];

    let out = column_offset_windows_from_samples(&from, &to, params);

    assert_eq!(out.len(), 3);
    assert_eq!(
        (out[0].player, out[0].column, out[0].from_y, out[0].to_y),
        (0, 0, 4.0, 8.0)
    );
    assert_eq!(
        (out[1].player, out[1].column, out[1].from_y, out[1].to_y),
        (1, 0, 0.0, -2.0)
    );
    assert_eq!(
        (out[2].player, out[2].column, out[2].from_y, out[2].to_y),
        (1, 2, 12.0, 0.0)
    );
    assert_eq!(out[0].easing.as_deref(), Some("linear"));
    assert_eq!(out[0].sustain, Some(1.0));
    assert_eq!(out[0].opt1, Some(2.0));
    assert_eq!(out[0].opt2, Some(3.0));
}

#[test]
fn note_hide_windows_from_flags_preserve_spline_run_policy() {
    let out =
        note_hide_windows_from_flags(1, 2, 0.5, &[false, true, true, false, true, true, true]);

    assert_eq!(out.len(), 2);
    assert_eq!((out[0].player, out[0].column), (1, 2));
    assert_eq!((out[0].start_beat, out[0].end_beat), (0.5, 1.0));
    assert_eq!((out[1].start_beat, out[1].end_beat), (2.0, 3.0));
}

#[test]
fn note_hide_window_index_policy_filters_invalid_ranges() {
    assert_eq!(note_hide_window_from_indices(0, 0, 1.0, 0, 1), None);
    assert_eq!(note_hide_window_from_indices(0, 0, 1.0, 3, 2), None);
    assert_eq!(
        note_hide_window_from_indices(0, 1, 0.25, 1, 1),
        Some(super::SongLuaNoteHideWindow {
            spline_beats_per_t: 0.25,
            spline_size: 1,
            player: 0,
            column: 1,
            start_beat: 0.0,
            end_beat: 0.0,
        })
    );
    assert!(note_hide_windows_from_flags(0, 0, 0.0, &[true]).is_empty());
    assert!(note_hide_windows_from_flags(0, 0, f32::NAN, &[true]).is_empty());
}

#[test]
fn overlay_eases_from_captures_intersects_common_delta_fields() {
    let params = SongLuaOverlayEaseBuildParams {
        unit: SongLuaTimeUnit::Beat,
        start: 2.0,
        limit: 4.0,
        span_mode: SongLuaSpanMode::Len,
        easing: Some("linear".to_string()),
        sustain: Some(0.5),
        opt1: Some(1.0),
        opt2: Some(2.0),
    };
    let block = |delta: SongLuaOverlayStateDelta| SongLuaOverlayCommandBlock {
        progress: None,
        queued: false,
        start: 0.0,
        duration: 0.0,
        easing: None,
        opt1: None,
        opt2: None,
        delta,
    };
    let from = vec![
        (
            2,
            vec![block(SongLuaOverlayStateDelta {
                x: Some(10.0),
                y: Some(20.0),
                ..SongLuaOverlayStateDelta::default()
            })],
        ),
        (
            0,
            vec![block(SongLuaOverlayStateDelta {
                x: Some(1.0),
                ..SongLuaOverlayStateDelta::default()
            })],
        ),
    ];
    let to = vec![
        (
            2,
            vec![block(SongLuaOverlayStateDelta {
                x: Some(30.0),
                ..SongLuaOverlayStateDelta::default()
            })],
        ),
        (
            1,
            vec![block(SongLuaOverlayStateDelta {
                x: Some(99.0),
                ..SongLuaOverlayStateDelta::default()
            })],
        ),
    ];

    let out = overlay_eases_from_captures(3, &from, &to, params);

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].overlay_index, 2);
    assert_eq!(out[0].from.x, Some(10.0));
    assert_eq!(out[0].from.y, None);
    assert_eq!(out[0].to.x, Some(30.0));
    assert_eq!(out[0].easing.as_deref(), Some("linear"));
    assert_eq!(out[0].sustain, Some(0.5));
    assert_eq!(out[0].opt1, Some(1.0));
    assert_eq!(out[0].opt2, Some(2.0));
}

#[test]
fn parse_overlay_blend_mode_accepts_stepmania_add_name() {
    assert_eq!(
        parse_overlay_blend_mode("BlendMode_Add"),
        Some(SongLuaOverlayBlendMode::Add)
    );
    assert_eq!(
        parse_overlay_blend_mode("BlendMode_Multiply"),
        Some(SongLuaOverlayBlendMode::Multiply)
    );
    assert_eq!(
        parse_overlay_blend_mode("BlendMode_Subtract"),
        Some(SongLuaOverlayBlendMode::Subtract)
    );
}

#[test]
fn parse_overlay_effect_mode_accepts_song_lua_effect_names() {
    assert_eq!(
        parse_overlay_effect_mode("DiffuseRamp"),
        Some(EffectMode::DiffuseRamp)
    );
    assert_eq!(
        parse_overlay_effect_mode("glowshift"),
        Some(EffectMode::GlowShift)
    );
    assert_eq!(
        parse_overlay_effect_mode("bounce"),
        Some(EffectMode::Bounce)
    );
    assert_eq!(parse_overlay_effect_mode("wag"), Some(EffectMode::Wag));
}

#[test]
fn parse_overlay_effect_clock_accepts_music_and_bgm_aliases() {
    assert_eq!(parse_overlay_effect_clock("beat"), Some(EffectClock::Beat));
    assert_eq!(parse_overlay_effect_clock("bgm"), Some(EffectClock::Beat));
    assert_eq!(parse_overlay_effect_clock("music"), Some(EffectClock::Time));
}

#[test]
fn theme_metric_number_exposes_numeric_compat_values() {
    assert_eq!(
        theme_metric_number("Player", "ReceptorArrowsYStandard"),
        Some(THEME_RECEPTOR_Y_STD)
    );
    assert_eq!(
        theme_metric_number("Player", "ReceptorArrowsYReverse"),
        Some(THEME_RECEPTOR_Y_REV)
    );
    assert_eq!(theme_metric_number("Combo", "ShowComboAt"), Some(4.0));
    assert_eq!(
        theme_metric_number("LifeMeterBar", "InitialValue"),
        Some(SONG_LUA_INITIAL_LIFE)
    );
    assert_eq!(
        theme_metric_number("MusicWheel", "NumWheelItems"),
        Some(15.0)
    );
    assert_eq!(
        theme_metric_number("PlayerStageStats", "NumGradeTiersUsed"),
        Some(7.0)
    );
}

#[test]
fn theme_string_names_include_difficulty_and_common_groups() {
    assert_eq!(
        theme_string_names("Difficulty"),
        vec![
            "Difficulty_Beginner".to_string(),
            "Difficulty_Easy".to_string(),
            "Difficulty_Medium".to_string(),
            "Difficulty_Hard".to_string(),
            "Difficulty_Challenge".to_string(),
            "Difficulty_Edit".to_string(),
        ]
    );

    let option_names = theme_string_names("OptionNames");
    assert!(option_names.contains(&"MusicRate".to_string()));
    assert!(option_names.contains(&"Difficulty_Hard".to_string()));
    assert!(theme_string_names("Unknown").is_empty());
}

#[test]
fn theme_string_formats_compat_values() {
    assert_eq!(
        theme_string("Difficulty", "Difficulty_Challenge"),
        "Challenge"
    );
    assert_eq!(theme_string("OptionNames", "Music_Rate"), "Music Rate");
    assert_eq!(theme_string("", "Cancel"), "Cancel");
    assert_eq!(theme_string("", "CustomValue"), "CustomValue");
}

#[test]
fn theme_has_string_matches_compat_groups_and_common_values() {
    assert!(theme_has_string("CustomDifficulty", "Difficulty_Edit"));
    assert!(theme_has_string("OptionTitles", "MusicRate"));
    assert!(theme_has_string("", "Yes"));
    assert!(!theme_has_string("Unknown", "Maybe"));
}

#[test]
fn arch_name_reports_stepmania_style_platform_name() {
    assert!(matches!(
        song_lua_arch_name(),
        "Windows" | "Mac OS X" | "Linux" | "FreeBSD" | "Unknown"
    ));
}

#[test]
fn custom_multi_modifier_key_prefixes_hide_choices() {
    assert_eq!(custom_multi_modifier_key("Hide", "Targets"), "HideTargets");
    assert_eq!(
        custom_multi_modifier_key("hide", "ComboExplosions"),
        "HideComboExplosions"
    );
    assert_eq!(
        custom_multi_modifier_key("GameplayExtras", "ColumnCues"),
        "ColumnCues"
    );
}

#[test]
fn updates_hold_infinity() {
    use SongLuaOverlayUpdateValue as Value;
    let finite = Value::Vec4([0.2, 0.4, 0.6, 0.3]);
    for alpha in [f32::INFINITY, f32::NEG_INFINITY] {
        let infinite = Value::Vec4([0.2, 0.4, 0.6, alpha]);
        for t in [0.0, 0.5, 1.0 - f32::EPSILON] {
            assert_eq!(finite.lerp(&infinite, t), finite);
            assert_eq!(infinite.lerp(&finite, t), infinite);
            assert_eq!(infinite.lerp(&infinite, t), infinite);
        }
        assert_eq!(finite.lerp(&infinite, 1.0), infinite);
        assert_eq!(infinite.lerp(&finite, 1.0), finite);
    }
    // Finite scalar/vector interpolation and discrete values retain their
    // existing behavior, including the endpoint threshold for flags.
    for (from, to, expected) in [
        (Value::F32(0.0), Value::F32(1.0), Value::F32(0.5)),
        (
            Value::Vec2([0.0; 2]),
            Value::Vec2([1.0; 2]),
            Value::Vec2([0.5; 2]),
        ),
        (
            Value::Vec3([0.0; 3]),
            Value::Vec3([1.0; 3]),
            Value::Vec3([0.5; 3]),
        ),
        (
            Value::Vec4([0.0; 4]),
            Value::Vec4([1.0; 4]),
            Value::Vec4([0.5; 4]),
        ),
        (
            Value::Vec5([0.0; 5]),
            Value::Vec5([1.0; 5]),
            Value::Vec5([0.5; 5]),
        ),
    ] {
        assert_eq!(from.lerp(&to, 0.5), expected);
    }
    assert_eq!(
        Value::Bool(false).lerp(&Value::Bool(true), 0.5),
        Value::Bool(false)
    );
    assert_eq!(
        Value::Bool(false).lerp(&Value::Bool(true), 1.0 - f32::EPSILON),
        Value::Bool(true)
    );
}
