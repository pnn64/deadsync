use deadsync_gameplay::{SongLuaEaseMaskTarget, song_lua_ease_window_value};
use deadsync_rules::timing::{TimingData, TimingSegments};
use deadsync_song_lua::{
    CompiledSongLua, SongLuaColumnOffsetWindow, SongLuaColumnTransformTarget,
    SongLuaCompileContext, SongLuaDifficulty, SongLuaEaseTarget, SongLuaEaseWindow,
    SongLuaMessageEvent, SongLuaModWindow, SongLuaNoteskinResolver, SongLuaOverlayActor,
    SongLuaOverlayCommandBlock, SongLuaOverlayEase, SongLuaOverlayMessageCommand,
    SongLuaOverlayModelLayer, SongLuaOverlayState, SongLuaOverlayUpdateSample,
    SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateTrack, SongLuaOverlayUpdateValue,
    SongLuaPlayerContext, SongLuaSpanMode, SongLuaSpeedMod, SongLuaTimeUnit,
    compile_song_lua_with_default_host,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

type TestCompiledSongLua = CompiledSongLua<SongLuaOverlayActor<()>>;

#[test]
fn catalog_note_bounds_include_pack_sync_before_global_offset() {
    let dir = std::env::temp_dir().join(format!("deadsync-catalog-sync-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let _ = deadsync_simfile::app_runtime::init_paths(deadsync_simfile::app_runtime::ScanPaths {
        song_cache: dir.join("cache"), extra_songs: vec![], extra_courses: vec![],
        autogen_courses: dir.join("autogen"), song_movies: vec![], random_movies: vec![], bg_animations: vec![],
    });
    let path = dir.join("control.sm");
    std::fs::write(&path, "#TITLE:Catalog sync;#BPMS:0=80;#LASTSECONDHINT:103.502998;#NOTES:dance-single::Challenge:1:0,0,0,0,0:\n1000\n0000\n0000\n0001\n;").unwrap();
    let mut song = deadsync_simfile::app_runtime::parse_song_for_test(&path, 0.0).unwrap();
    song.first_second = 9.0;
    song.last_second_hint = 103.502998;
    song.precise_note_last_second_seconds = Some(94.5);
    let chart = Arc::new(song.charts[0].clone());
    let charts = [chart.clone(), chart];
    let timing = TimingData::from_segments(0.0, 0.0, &TimingSegments::default(), &[]);
    let profiles = std::array::from_fn(|_| deadsync_profile::Profile::default());
    let scroll = profiles.each_ref().map(|profile| profile.scroll_speed);
    let config = deadsync_gameplay::GameplayConfig { global_offset_seconds: -0.008,
        machine_pack_ini_offsets: true, machine_default_sync_pref: deadsync_chart::SyncPref::Itg,
        ..Default::default() };
    let context = super::song_lua_play_context(&song, &charts, [&timing, &timing], &profiles,
        &scroll, 1.0, deadsync_gameplay::GameplayViewport::design(), (854, 480), 16.0/9.0,
        1.0, &deadsync_gameplay::GameplaySession::default(), &config, "null");
    assert_eq!(context.song_note_bounds, Some([9.017, 103.502998]),
        "native catalog timing applies the ITG pack offset before resolving the cabinet offset; hints stay absolute");
}

fn deadsync_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn song_lua_player_positions_apply_native_integer_margins() {
    use deadsync_gameplay::{GameplayInputPlayStyle as Style, GameplayInputPlayerSide as Side, GameplayViewport};
    let profile = deadsync_profile::Profile::default();
    for (width, p1, p2) in [(640.0, 160.0, 480.0), (852.0, 213.0, 639.0),
        (854.0, 213.25, 640.75), (1280.0, 426.25, 853.75)] {
        let viewport = GameplayViewport::new(width, 480.0);
        for (count, player, style, side, expected) in [
            (1, 0, Style::Single, Side::P1, p1),
            (1, 0, Style::Single, Side::P2, p2),
            (2, 0, Style::Versus, Side::P1, p1),
            (2, 1, Style::Versus, Side::P1, p2),
            (1, 0, Style::Double, Side::P1, width * 0.5),
        ] {
            assert_eq!(super::song_lua_compile_player_screen_x(count, player, &profile,
                viewport, style, side, false), expected);
        }
        assert_eq!(super::song_lua_compile_player_screen_x(1, 0, &profile, viewport,
            Style::Single, Side::P1, true), width * 0.5);
    }
    let profile = deadsync_profile::Profile { note_field_offset_x: 10, ..profile };
    assert_eq!(super::song_lua_compile_player_screen_x(1, 0, &profile,
        GameplayViewport::design(), Style::Single, Side::P1, false), 203.25);
}

#[test]
fn null_renderer_reaches_song_lua_unchanged() {
    let mut song = deadsync_simfile::song::parse_song_meta_file(
        &deadsync_root().join("tests/fixtures/song_lua/display-size.ssc"),
        &deadsync_simfile::song::ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
        0.0,
        |_| 2.0,
    ).expect("small context chart");
    song.banner_path = Some(deadsync_root().join("crates/deadsync-song-lua/assets/itgmania/_blank.png"));
    let charts = std::array::from_fn(|_| Arc::new(song.charts[0].clone()));
    let timing = TimingData::default();
    let mut profiles = std::array::from_fn(|_| deadsync_profile::Profile::default());
    profiles[0].background_filter = deadsync_profile::BackgroundFilter::from_percent(37);
    profiles[0].hide_lifebar = true;
    profiles[0].hide_danger = true;
    profiles[0].lifemeter_type = deadsync_profile::LifeMeterType::Vertical;
    profiles[0].nps_graph_at_top = true;
    profiles[0].step_statistics = deadsync_profile::StepStatisticsMask::empty();
    profiles[0].timing_windows = deadsync_profile::TimingWindowsOption::WayOffs;
    profiles[1].timing_windows = deadsync_profile::TimingWindowsOption::FantasticsAndExcellents;
    let context = super::song_lua_play_context(
        &song, &charts, [&timing; 2],
        &profiles,
        &[deadsync_rules::scroll::ScrollSpeedSetting::XMod(1.0); 2],
        1.0, deadsync_gameplay::GameplayViewport::new(854.0, 480.0), (0, 0), 16.0 / 9.0, 1.0,
        &Default::default(), &Default::default(), "null",
    );
    assert_eq!((context.screen_width, context.screen_height), (854.0, 480.0));
    assert_eq!((context.display_width, context.display_height), (0.0, 0.0));
    assert_eq!(context.display_aspect_ratio, 16.0 / 9.0);
    assert_eq!(context.song_banner_path, song.banner_path);
    assert_eq!(context.players[0].autoplay, 1.0);
    assert_eq!(context.players[1].autoplay, 0.0);
    assert_eq!(context.players[0].background_filter_percent, 37);
    assert_eq!(context.players[1].background_filter_percent, 0);
    assert!(context.players[0].hide_lifebar && context.players[1].hide_lifebar);
    assert!(context.players[0].hide_danger && !context.players[1].hide_danger);
    assert_eq!(context.players[0].life_meter_type, "Vertical");
    assert!(context.players[0].nps_graph_at_top && !context.players[1].nps_graph_at_top);
    assert!(!context.players[0].step_statistics_enabled);
    assert_eq!(context.players[1].step_statistics_enabled, context.players[1].enabled);
    assert_eq!(context.players[0].timing_windows, [true, true, true, true, false]);
    assert_eq!(context.players[1].timing_windows, [false, false, true, true, true]);
    assert_eq!(context.video_renderers, "null");
    assert!(!context.render_to_texture);
}

#[test]
fn saved_perspective_reaches_lua_for_each_player_side() {
    use deadsync_gameplay::{GameplayInputPlayerSide, GameplaySession, GameplayViewport};
    use deadsync_profile::{Perspective, Profile};
    let song = deadsync_simfile::song::parse_song_meta_file(
        &deadsync_root().join("tests/fixtures/song_lua/display-size.ssc"),
        &deadsync_simfile::song::ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
        0.0,
        |_| 2.0,
    )
    .expect("small context chart");
    let charts = [
        Arc::new(song.charts[0].clone()),
        Arc::new(song.charts[0].clone()),
    ];
    let timing = TimingData::default();
    let profiles = [
        Profile {
            perspective: Perspective::Incoming,
            ..Default::default()
        },
        Profile {
            perspective: Perspective::Space,
            ..Default::default()
        },
    ];
    let speeds = [deadsync_rules::scroll::ScrollSpeedSetting::XMod(1.0); 2];
    for (side, expected) in [
        (GameplayInputPlayerSide::P1, (-1.0, 1.0)),
        (GameplayInputPlayerSide::P2, (1.0, 1.0)),
    ] {
        let context = super::song_lua_play_context(
            &song,
            &charts,
            [&timing; 2],
            &profiles,
            &speeds,
            1.0,
            GameplayViewport::new(854.0, 480.0),
            (854, 480),
            4.0 / 3.0,
            0.0,
            &GameplaySession {
                player_side: side,
                ..Default::default()
            },
            &Default::default(),
            "opengl",
        );
        let perspective = context.players[0].perspective;
        assert_eq!(context.display_aspect_ratio, 4.0 / 3.0);
        assert_eq!((perspective.tilt, perspective.skew), expected);
        assert!(!context.players[1].enabled);
        assert_eq!(context.players[1].perspective, Default::default());
    }
}

#[test]
fn saved_mini_reaches_song_lua_constructor_and_reverse_path() {
    use deadsync_gameplay::{GameplayInputPlayerSide as Side, GameplaySession, GameplayViewport};
    let root = deadsync_root().join("tests/fixtures/song_lua");
    let song = deadsync_simfile::song::parse_song_meta_file(
        &root.join("display-size.ssc"),
        &deadsync_simfile::song::ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
        0.0,
        |_| 2.0,
    ).expect("small context chart");
    let charts = std::array::from_fn(|_| Arc::new(song.charts[0].clone()));
    let timing = TimingData::default();
    let profiles = [60, -40].map(|mini_percent| deadsync_profile::Profile {
        mini_percent,
        scroll_speed: deadsync_rules::scroll::ScrollSpeedSetting::MMod(730.0),
        ..Default::default()
    });
    let speeds = profiles.each_ref().map(|profile| profile.scroll_speed);
    let native: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/itgmania-song-lua-micro/profile-mini/splines-native.json"
    )).expect("native CubicSpline reference");
    for ((side, mini), native) in [(Side::P1, 0.6_f32), (Side::P2, -0.4)]
        .into_iter().zip(native["splines"].as_array().expect("native paths")) {
        let context = super::song_lua_play_context(
            &song, &charts, [&timing; 2], &profiles, &speeds, 1.0,
            GameplayViewport::design(), (854, 480), 16.0 / 9.0, 1.0,
            &GameplaySession { player_side: side, ..Default::default() },
            &Default::default(), "opengl",
        );
        let compiled = compile_song_lua_with_default_host(
            &root.join("profile-mini.lua"), &context, SongLuaNoteskinResolver::default(),
            test_read_model_slots, test_model_layer_from_slot, |_, _| None,
        ).expect("compile saved Mini reverse path");
        let observed = compiled.overlays.iter().find(|actor| actor.name.as_deref() == Some("InitialMini"))
            .expect("constructor observation").initial_state.x;
        assert!((observed - mini).abs() < 0.000001,
            "{side:?}: song constructor saw Mini={observed}, profile requires {mini}");
        let path = compiled.column_splines.iter().find(|track| track.player == 0 && track.column == 0)
            .and_then(|track| track.frames.first()).and_then(|frame| frame.position.as_ref())
            .expect("compiled reverse path");
        assert_eq!(path.coefficients.len(), native["points"].as_array().unwrap().len(),
            "path length must use the saved Mini");
        for sample in native["samples"].as_array().expect("native samples") {
            let (position, _) = path.view().sample(
                sample["song_beat"].as_f64().unwrap() as f32,
                sample["note_beat"].as_f64().unwrap() as f32,
            );
            for axis in 0..3 {
                let expected = sample["position"][axis].as_f64().unwrap() as f32;
                assert!((position[axis] - expected).abs() < 0.001,
                    "{side:?}, note beat {}, axis {axis}: {} != native {expected}",
                    sample["note_beat"], position[axis]);
            }
        }
        let constants = deadsync_song_lua::gameplay::build_song_lua_constant_windows_for_player(
            &compiled, &timing, 0, 0.0,
        );
        let (eases, unsupported) = deadsync_song_lua::gameplay::build_song_lua_ease_windows_for_player(
            &compiled, &timing, 0, 0.0, &constants,
        );
        assert_eq!(unsupported, 0);
        assert!(eases.iter().any(|window| window.target == SongLuaEaseMaskTarget::MiniPercent),
            "the regression must exercise the compiled Mini write");
        let mut runtime = deadsync_gameplay::GameplayAttackRuntimeState::new(
            [constants, vec![]], [eases, vec![]],
        );
        for tick in 0..=60 {
            runtime.refresh_player(0, tick as f32 / 60.0, 1.0 / 60.0,
                Default::default(), || deadsync_gameplay::AttackBaseEffects {
                    mini_percent: mini * 100.0, ..Default::default()
                }, Default::default());
        }
        let current_mini = runtime.mini_percent[0].expect("compiled Mini reaches gameplay");
        assert!((current_mini - mini * 100.0).abs() < 0.001,
            "{side:?}: runtime Mini={current_mini}; native setter replaces saved Mini");
        let zoom = 1.0 - current_mini / 200.0;
        let receptor_y = 240.0 + path.view().receptor(0.0)[1] * zoom;
        assert!((receptor_y - 375.0).abs() < 0.001, "reversed receptor: {receptor_y}");
        let endpoint_y = 240.0 + path.view().sample(0.0, 8.0).0[1] * zoom;
        assert!(endpoint_y < -20.0, "future notes clamped onto the score area at y={endpoint_y}");
    }
}

fn test_song_lua_double_context(root: &Path, title: &str) -> SongLuaCompileContext {
    let mut context = SongLuaCompileContext::new(root, title);
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
            speedmod: SongLuaSpeedMod::X(2.0),
            ..SongLuaPlayerContext::default()
        },
    ];
    context
}

fn test_read_model_slots(_: &Path, _: &Path, _: &Path) -> Result<Arc<[()]>, String> {
    Ok(Arc::from(Vec::<()>::new().into_boxed_slice()))
}

fn test_model_layer_from_slot(_: &()) -> Option<SongLuaOverlayModelLayer<()>> {
    None
}

fn test_row_to_beat(last_row: usize) -> Vec<f32> {
    (0..=last_row)
        .map(|row| row as f32 / deadsync_core::timing::ROWS_PER_BEAT as f32)
        .collect()
}

fn test_timing(last_row: usize) -> TimingData {
    let timing_segments = TimingSegments {
        bpms: vec![(0.0, 60.0)],
        ..TimingSegments::default()
    };
    TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(last_row))
}

fn runtime_window_bridge_fixture() -> TestCompiledSongLua {
    TestCompiledSongLua {
        time_mods: vec![
            SongLuaModWindow {
                unit: SongLuaTimeUnit::Second,
                start: 1.0,
                limit: 0.5,
                span_mode: SongLuaSpanMode::Len,
                mods: "*100 50 drunk".to_string(),
                player: None,
            },
            SongLuaModWindow {
                unit: SongLuaTimeUnit::Second,
                start: 2.0,
                limit: 0.5,
                span_mode: SongLuaSpanMode::Len,
                mods: "*100 25 tipsy".to_string(),
                player: Some(1),
            },
            SongLuaModWindow {
                unit: SongLuaTimeUnit::Second,
                start: 3.0,
                limit: 0.5,
                span_mode: SongLuaSpanMode::Len,
                mods: "*100 -25 flip".to_string(),
                player: Some(2),
            },
        ],
        beat_mods: vec![SongLuaModWindow {
            unit: SongLuaTimeUnit::Beat,
            start: 8.0,
            limit: 1.0,
            span_mode: SongLuaSpanMode::Len,
            mods: "*100 75 dark".to_string(),
            player: None,
        }],
        eases: vec![
            SongLuaEaseWindow {
                approach_speed: None,
                player: None,
                unit: SongLuaTimeUnit::Second,
                start: 1.0,
                limit: 0.5,
                span_mode: SongLuaSpanMode::Len,
                target: SongLuaEaseTarget::Mod("dark".to_string()),
                from: 0.0,
                to: 100.0,
                easing: Some("inOutQuad".to_string()),
                sustain: Some(0.25),
                opt1: None,
                opt2: None,
            },
            SongLuaEaseWindow {
                approach_speed: None,
                player: Some(1),
                unit: SongLuaTimeUnit::Beat,
                start: 4.0,
                limit: 1.0,
                span_mode: SongLuaSpanMode::Len,
                target: SongLuaEaseTarget::PlayerX,
                from: 320.0,
                to: 360.0,
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            },
            SongLuaEaseWindow {
                approach_speed: None,
                player: Some(2),
                unit: SongLuaTimeUnit::Second,
                start: 3.0,
                limit: 0.5,
                span_mode: SongLuaSpanMode::Len,
                target: SongLuaEaseTarget::Mod("tipsy".to_string()),
                from: 20.0,
                to: 80.0,
                easing: Some("outSine".to_string()),
                sustain: None,
                opt1: Some(0.5),
                opt2: None,
            },
            SongLuaEaseWindow {
                approach_speed: None,
                player: Some(1),
                unit: SongLuaTimeUnit::Second,
                start: 5.0,
                limit: 0.5,
                span_mode: SongLuaSpanMode::Len,
                target: SongLuaEaseTarget::Mod("unsupported-window-target".to_string()),
                from: 0.0,
                to: 1.0,
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            },
            SongLuaEaseWindow {
                approach_speed: None,
                player: None,
                unit: SongLuaTimeUnit::Second,
                start: 6.0,
                limit: 0.5,
                span_mode: SongLuaSpanMode::Len,
                target: SongLuaEaseTarget::Function,
                from: 0.0,
                to: 1.0,
                easing: None,
                sustain: None,
                opt1: None,
                opt2: None,
            },
        ],
        column_offsets: vec![
            SongLuaColumnOffsetWindow {
                player: 0,
                column: 0,
                target: SongLuaColumnTransformTarget::OffsetX,
                unit: SongLuaTimeUnit::Second,
                start: 1.0,
                limit: 0.5,
                span_mode: SongLuaSpanMode::Len,
                from_y: -16.0,
                to_y: 16.0,
                easing: Some("inOutSine".to_string()),
                sustain: Some(0.25),
                opt1: None,
                opt2: None,
            },
            SongLuaColumnOffsetWindow {
                player: 0,
                column: 3,
                target: SongLuaColumnTransformTarget::RotationZ,
                unit: SongLuaTimeUnit::Beat,
                start: 4.0,
                limit: 1.0,
                span_mode: SongLuaSpanMode::Len,
                from_y: 0.0,
                to_y: 45.0,
                easing: Some("outQuad".to_string()),
                sustain: None,
                opt1: Some(0.25),
                opt2: Some(0.75),
            },
            SongLuaColumnOffsetWindow {
                player: 1,
                column: 7,
                target: SongLuaColumnTransformTarget::Zoom,
                unit: SongLuaTimeUnit::Second,
                start: 3.0,
                limit: 0.5,
                span_mode: SongLuaSpanMode::Len,
                from_y: 1.0,
                to_y: 0.5,
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            },
        ],
        ..Default::default()
    }
}

#[test]
fn constant_window_bridge_filters_by_player() {
    let timing = test_timing(16 * 48);
    let compiled = runtime_window_bridge_fixture();
    for player in 0..2 {
        let actual = deadsync_song_lua::gameplay::build_song_lua_constant_windows_for_player(
            &compiled, &timing, player, 0.25,
        );
        assert_eq!(actual.len(), 3);
    }
    let windows = deadsync_song_lua::gameplay::build_song_lua_constant_windows_for_player(
        &compiled, &timing, 0, 0.25,
    );
    assert_eq!(windows[0].start_second, 0.75);
    assert_eq!(windows[0].end_second, 1.25);
    assert_eq!(windows[2].start_second, 8.0);
}

#[test]
fn ease_window_bridge_maps_supported_targets() {
    let timing = test_timing(16 * 48);
    let compiled = runtime_window_bridge_fixture();
    let (windows, unsupported) =
        deadsync_song_lua::gameplay::build_song_lua_ease_windows_for_player(
            &compiled,
            &timing,
            0,
            0.25,
            &[],
        );
    assert_eq!(windows.len(), 2);
    assert_eq!(unsupported, 1);
    assert!(matches!(
        windows[0].target,
        SongLuaEaseMaskTarget::VisibilityDark
    ));
    assert!(matches!(windows[1].target, SongLuaEaseMaskTarget::PlayerX));
}

#[test]
fn column_offset_bridge_filters_by_player() {
    let timing = test_timing(16 * 48);
    let compiled = runtime_window_bridge_fixture();
    for (player, expected_len) in [(0, 2), (1, 1)] {
        let actual = deadsync_song_lua::gameplay::build_song_lua_column_offset_windows_for_player(
            &compiled, &timing, player, 0.25,
        );
        assert_eq!(actual.len(), expected_len);
    }
    let windows = deadsync_song_lua::gameplay::build_song_lua_column_offset_windows_for_player(
        &compiled, &timing, 1, 0.25,
    );
    assert_eq!(windows[0].column, 7);
    assert_eq!(windows[0].start_second, 2.75);
}

#[test]
fn song_lua_overlay_updates_keep_absolute_song_time() {
    let timing = test_timing(8 * 48);
    let mut compiled = TestCompiledSongLua {
        overlay_updates: vec![SongLuaOverlayUpdateTrack {
            overlay_index: 0,
            target: SongLuaOverlayUpdateTarget::Y,
            samples: vec![SongLuaOverlayUpdateSample {
                time: 4.0,
                value: SongLuaOverlayUpdateValue::F32(120.0),
            }],
        }],
        ..Default::default()
    };
    let global_offset_seconds = 0.25;

    let tracks = deadsync_song_lua::gameplay::build_song_lua_overlay_update_tracks(
        &compiled,
        &timing,
        global_offset_seconds,
    );

    let expected =
        deadsync_gameplay::song_lua_message_second(4.0, &timing, global_offset_seconds).unwrap();
    assert_eq!(tracks[0].samples[0].second, expected);

    compiled.overlay_update_unit = SongLuaTimeUnit::Second;
    let tracks = deadsync_song_lua::gameplay::build_song_lua_overlay_update_tracks(
        &compiled,
        &timing,
        global_offset_seconds,
    );
    assert_eq!(tracks[0].samples[0].second, 4.0 - global_offset_seconds);
}

#[test]
fn song_lua_overlay_eases_stop_after_later_message_blocks() {
    let timing = test_timing(8 * 48);
    let compiled = TestCompiledSongLua {
        overlays: vec![SongLuaOverlayActor {
            kind: (),
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: vec![SongLuaOverlayMessageCommand {
                frame_advance: 0.0,
                message: "ResetBlack".to_string(),
                blocks: vec![SongLuaOverlayCommandBlock {
                    progress: None,
                    queued: false,
                    start: 0.0,
                    duration: 0.0,
                    easing: None,
                    opt1: None,
                    opt2: None,
                    delta: deadsync_song_lua::SongLuaOverlayStateDelta {
                        diffuse: Some([1.0, 1.0, 1.0, 0.0]),
                        ..Default::default()
                    },
                }],
            }],
        }],
        overlay_eases: vec![SongLuaOverlayEase {
            overlay_index: 0,
            unit: SongLuaTimeUnit::Beat,
            start: 0.0,
            limit: 8.0,
            span_mode: SongLuaSpanMode::Len,
            from: deadsync_song_lua::SongLuaOverlayStateDelta {
                diffuse: Some([1.0, 1.0, 1.0, 0.0]),
                ..Default::default()
            },
            to: deadsync_song_lua::SongLuaOverlayStateDelta {
                diffuse: Some([1.0, 1.0, 1.0, 1.0]),
                ..Default::default()
            },
            easing: Some("linear".to_string()),
            sustain: None,
            opt1: None,
            opt2: None,
        }],
        messages: vec![SongLuaMessageEvent {
            beat: 4.0,
            message: "ResetBlack".to_string(),
            persists: true,
        }],
        ..Default::default()
    };

    let windows =
        deadsync_song_lua::gameplay::build_song_lua_overlay_ease_windows(&compiled, &timing, 0.0);

    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].cutoff_second, Some(4.0));
    assert_eq!(windows[0].end_second, 8.0);
}

#[test]
fn song_lua_overlay_eases_ignore_same_timestamp_setup_blocks() {
    let timing = test_timing(8 * 48);
    let compiled = TestCompiledSongLua {
        overlays: vec![SongLuaOverlayActor {
            kind: (),
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: vec![SongLuaOverlayMessageCommand {
                frame_advance: 0.0,
                message: "SetupZoom".to_string(),
                blocks: vec![SongLuaOverlayCommandBlock {
                    progress: None,
                    queued: false,
                    start: 0.0,
                    duration: 0.0,
                    easing: None,
                    opt1: None,
                    opt2: None,
                    delta: deadsync_song_lua::SongLuaOverlayStateDelta {
                        zoom: Some(1.5),
                        ..Default::default()
                    },
                }],
            }],
        }],
        overlay_eases: vec![SongLuaOverlayEase {
            overlay_index: 0,
            unit: SongLuaTimeUnit::Beat,
            start: 0.0,
            limit: 8.0,
            span_mode: SongLuaSpanMode::Len,
            from: deadsync_song_lua::SongLuaOverlayStateDelta {
                zoom: Some(1.5),
                ..Default::default()
            },
            to: deadsync_song_lua::SongLuaOverlayStateDelta {
                zoom: Some(1.0),
                ..Default::default()
            },
            easing: Some("linear".to_string()),
            sustain: None,
            opt1: None,
            opt2: None,
        }],
        messages: vec![SongLuaMessageEvent {
            beat: 0.0,
            message: "SetupZoom".to_string(),
            persists: true,
        }],
        ..Default::default()
    };

    let windows =
        deadsync_song_lua::gameplay::build_song_lua_overlay_ease_windows(&compiled, &timing, 0.0);

    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].cutoff_second, None);
    assert_eq!(windows[0].end_second, 8.0);
}

#[test]
fn song_lua_overlay_eases_stop_persisting_after_later_reset_messages() {
    let timing = test_timing(8 * 48);
    let compiled = TestCompiledSongLua {
        overlays: vec![SongLuaOverlayActor {
            kind: (),
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: vec![SongLuaOverlayMessageCommand {
                frame_advance: 0.0,
                message: "ResetBlack".to_string(),
                blocks: vec![SongLuaOverlayCommandBlock {
                    progress: None,
                    queued: false,
                    start: 0.0,
                    duration: 0.0,
                    easing: None,
                    opt1: None,
                    opt2: None,
                    delta: deadsync_song_lua::SongLuaOverlayStateDelta {
                        diffuse: Some([0.0, 0.0, 0.0, 0.0]),
                        ..Default::default()
                    },
                }],
            }],
        }],
        overlay_eases: vec![SongLuaOverlayEase {
            overlay_index: 0,
            unit: SongLuaTimeUnit::Beat,
            start: 0.0,
            limit: 2.0,
            span_mode: SongLuaSpanMode::Len,
            from: deadsync_song_lua::SongLuaOverlayStateDelta {
                diffuse: Some([0.0, 0.0, 0.0, 0.0]),
                ..Default::default()
            },
            to: deadsync_song_lua::SongLuaOverlayStateDelta {
                diffuse: Some([0.0, 0.0, 0.0, 1.0]),
                ..Default::default()
            },
            easing: Some("linear".to_string()),
            sustain: None,
            opt1: None,
            opt2: None,
        }],
        messages: vec![SongLuaMessageEvent {
            beat: 4.0,
            message: "ResetBlack".to_string(),
            persists: true,
        }],
        ..Default::default()
    };

    let windows =
        deadsync_song_lua::gameplay::build_song_lua_overlay_ease_windows(&compiled, &timing, 0.0);

    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].end_second, 2.0);
    assert_eq!(windows[0].cutoff_second, Some(4.0));
}

#[test]
fn song_lua_eases_persist_until_later_override() {
    let timing = test_timing(16 * 48);
    let compiled = TestCompiledSongLua {
        eases: vec![
            SongLuaEaseWindow {
                approach_speed: None,
                player: Some(1),
                unit: SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 4.0,
                span_mode: SongLuaSpanMode::Len,
                target: SongLuaEaseTarget::PlayerZoomY,
                from: 1.0,
                to: 0.0,
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            },
            SongLuaEaseWindow {
                approach_speed: None,
                player: Some(1),
                unit: SongLuaTimeUnit::Beat,
                start: 8.0,
                limit: 4.0,
                span_mode: SongLuaSpanMode::Len,
                target: SongLuaEaseTarget::PlayerZoomY,
                from: 0.0,
                to: 1.0,
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            },
            SongLuaEaseWindow {
                approach_speed: None,
                player: Some(1),
                unit: SongLuaTimeUnit::Beat,
                start: 1.0,
                limit: 0.25,
                span_mode: SongLuaSpanMode::Len,
                target: SongLuaEaseTarget::Mod("dark".to_string()),
                from: 0.0,
                to: 100.0,
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            },
            SongLuaEaseWindow {
                approach_speed: None,
                player: Some(1),
                unit: SongLuaTimeUnit::Beat,
                start: 4.0,
                limit: 2.0,
                span_mode: SongLuaSpanMode::Len,
                target: SongLuaEaseTarget::Mod("dark".to_string()),
                from: 100.0,
                to: 0.0,
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            },
        ],
        ..Default::default()
    };

    let (windows, unsupported) =
        deadsync_song_lua::gameplay::build_song_lua_ease_windows_for_player(
            &compiled,
            &timing,
            0,
            0.0,
            &[],
        );

    assert_eq!(unsupported, 0);
    assert_eq!(windows.len(), 4);
    assert_eq!(windows[0].sustain_end_second, 8.0);
    assert!(
        song_lua_ease_window_value(&windows[0], 6.0)
            .is_some_and(|value| (value - 0.0).abs() <= 0.000_1)
    );
    assert_eq!(windows[1].sustain_end_second, f32::MAX);
    assert!(
        song_lua_ease_window_value(&windows[1], 20.0)
            .is_some_and(|value| (value - 1.0).abs() <= 0.000_1)
    );
    assert_eq!(windows[2].sustain_end_second, 4.0);
    assert!(
        song_lua_ease_window_value(&windows[2], 3.0)
            .is_some_and(|value| (value - 1.0).abs() <= 0.000_1)
    );
    assert_eq!(windows[3].sustain_end_second, f32::MAX);
    assert!(
        song_lua_ease_window_value(&windows[3], 7.0).is_some_and(|value| value.abs() <= 0.000_1)
    );
}

#[test]
fn song_lua_constant_mod_cuts_prior_ease_tail() {
    let timing = test_timing(16 * 48);
    let compiled = TestCompiledSongLua {
        eases: vec![SongLuaEaseWindow {
            approach_speed: None,
            player: Some(1),
            unit: SongLuaTimeUnit::Beat,
            start: 0.0,
            limit: 4.0,
            span_mode: SongLuaSpanMode::Len,
            target: SongLuaEaseTarget::Mod("flip".to_string()),
            from: 0.0,
            to: -400.0,
            easing: Some("linear".to_string()),
            sustain: None,
            opt1: None,
            opt2: None,
        }],
        beat_mods: vec![SongLuaModWindow {
            unit: SongLuaTimeUnit::Beat,
            start: 4.0,
            limit: 1.0,
            span_mode: SongLuaSpanMode::Len,
            mods: "*100 0 flip".to_string(),
            player: Some(1),
        }],
        ..Default::default()
    };

    let constants = deadsync_song_lua::gameplay::build_song_lua_constant_windows_for_player(
        &compiled, &timing, 0, 0.0,
    );
    let (windows, unsupported) =
        deadsync_song_lua::gameplay::build_song_lua_ease_windows_for_player(
            &compiled, &timing, 0, 0.0, &constants,
        );

    assert_eq!(unsupported, 0);
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].sustain_end_second, 4.0);
    assert!(
        song_lua_ease_window_value(&windows[0], 3.5)
            .is_some_and(|value| (value + 3.5).abs() <= 0.000_1)
    );
    assert!(song_lua_ease_window_value(&windows[0], 4.25).is_none());
}

#[test]
fn song_lua_column_offsets_persist_until_next_column_offset() {
    let timing = test_timing(16 * 48);
    let compiled = TestCompiledSongLua {
        column_offsets: vec![
            SongLuaColumnOffsetWindow {
                player: 0,
                column: 2,
                target: SongLuaColumnTransformTarget::OffsetY,
                unit: SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 0.5,
                span_mode: SongLuaSpanMode::Len,
                from_y: 33.75,
                to_y: 0.0,
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            },
            SongLuaColumnOffsetWindow {
                player: 0,
                column: 2,
                target: SongLuaColumnTransformTarget::OffsetY,
                unit: SongLuaTimeUnit::Beat,
                start: 2.0,
                limit: 0.5,
                span_mode: SongLuaSpanMode::Len,
                from_y: 0.0,
                to_y: 33.75,
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            },
        ],
        ..Default::default()
    };

    let windows = deadsync_song_lua::gameplay::build_song_lua_column_offset_windows_for_player(
        &compiled, &timing, 0, 0.0,
    );

    assert_eq!(windows.len(), 2);
    assert_eq!(windows[0].sustain_end_second, 2.0);
    assert_eq!(windows[1].sustain_end_second, f32::MAX);
}

#[test]
fn song_lua_builds_playerxy_playerz_rotationx_skewy_zoom_and_zoomz_runtime_targets() {
    let timing = test_timing(16 * 48);
    let compiled = TestCompiledSongLua {
        eases: vec![
            SongLuaEaseWindow {
                approach_speed: None,
                player: Some(1),
                unit: SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 1.0,
                span_mode: SongLuaSpanMode::Len,
                target: SongLuaEaseTarget::PlayerX,
                from: 320.0,
                to: 360.0,
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            },
            SongLuaEaseWindow {
                approach_speed: None,
                player: Some(1),
                unit: SongLuaTimeUnit::Beat,
                start: 1.0,
                limit: 1.0,
                span_mode: SongLuaSpanMode::Len,
                target: SongLuaEaseTarget::PlayerY,
                from: 240.0,
                to: 210.0,
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            },
            SongLuaEaseWindow {
                approach_speed: None,
                player: Some(1),
                unit: SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 2.0,
                span_mode: SongLuaSpanMode::Len,
                target: SongLuaEaseTarget::PlayerZ,
                from: 0.0,
                to: -120.0,
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            },
            SongLuaEaseWindow {
                approach_speed: None,
                player: Some(1),
                unit: SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 4.0,
                span_mode: SongLuaSpanMode::Len,
                target: SongLuaEaseTarget::PlayerRotationX,
                from: 0.0,
                to: 20.0,
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            },
            SongLuaEaseWindow {
                approach_speed: None,
                player: Some(1),
                unit: SongLuaTimeUnit::Beat,
                start: 4.0,
                limit: 2.0,
                span_mode: SongLuaSpanMode::Len,
                target: SongLuaEaseTarget::PlayerSkewY,
                from: 0.0,
                to: 0.25,
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            },
            SongLuaEaseWindow {
                approach_speed: None,
                player: Some(1),
                unit: SongLuaTimeUnit::Beat,
                start: 6.0,
                limit: 2.0,
                span_mode: SongLuaSpanMode::Len,
                target: SongLuaEaseTarget::PlayerZoom,
                from: 1.0,
                to: 0.75,
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            },
            SongLuaEaseWindow {
                approach_speed: None,
                player: Some(1),
                unit: SongLuaTimeUnit::Beat,
                start: 8.0,
                limit: 2.0,
                span_mode: SongLuaSpanMode::Len,
                target: SongLuaEaseTarget::PlayerZoomZ,
                from: 1.0,
                to: 1.25,
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            },
        ],
        ..Default::default()
    };

    let (windows, unsupported) =
        deadsync_song_lua::gameplay::build_song_lua_ease_windows_for_player(
            &compiled,
            &timing,
            0,
            0.0,
            &[],
        );

    assert_eq!(unsupported, 0);
    assert_eq!(windows.len(), 7);
    assert!(matches!(windows[0].target, SongLuaEaseMaskTarget::PlayerX));
    assert!(matches!(windows[1].target, SongLuaEaseMaskTarget::PlayerY));
    assert!(matches!(windows[2].target, SongLuaEaseMaskTarget::PlayerZ));
    assert!(matches!(
        windows[3].target,
        SongLuaEaseMaskTarget::PlayerRotationX
    ));
    assert!(matches!(
        windows[4].target,
        SongLuaEaseMaskTarget::PlayerSkewY
    ));
    assert!(matches!(
        windows[5].target,
        SongLuaEaseMaskTarget::PlayerZoom
    ));
    assert!(matches!(
        windows[6].target,
        SongLuaEaseMaskTarget::PlayerZoomZ
    ));
    assert!(
        song_lua_ease_window_value(&windows[0], 0.5)
            .is_some_and(|value| (value - 340.0).abs() <= 0.000_1)
    );
    assert!(
        song_lua_ease_window_value(&windows[1], 1.5)
            .is_some_and(|value| (value - 225.0).abs() <= 0.000_1)
    );
    assert!(
        song_lua_ease_window_value(&windows[2], 1.0)
            .is_some_and(|value| (value + 60.0).abs() <= 0.000_1)
    );
    assert!(
        song_lua_ease_window_value(&windows[3], 2.0)
            .is_some_and(|value| (value - 10.0).abs() <= 0.000_1)
    );
    assert!(
        song_lua_ease_window_value(&windows[4], 5.0)
            .is_some_and(|value| (value - 0.125).abs() <= 0.000_1)
    );
    assert!(
        song_lua_ease_window_value(&windows[5], 7.0)
            .is_some_and(|value| (value - 0.875).abs() <= 0.000_1)
    );
    assert!(
        song_lua_ease_window_value(&windows[6], 9.0)
            .is_some_and(|value| (value - 1.125).abs() <= 0.000_1)
    );
}

#[test]
fn song_lua_skew_mod_eases_scale_to_player_skews() {
    let timing = test_timing(16 * 48);
    let compiled = TestCompiledSongLua {
        eases: vec![
            SongLuaEaseWindow {
                approach_speed: None,
                player: Some(1),
                unit: SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 1.0,
                span_mode: SongLuaSpanMode::Len,
                target: SongLuaEaseTarget::Mod("skewx".to_string()),
                from: 0.0,
                to: 3.0,
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            },
            SongLuaEaseWindow {
                approach_speed: None,
                player: Some(1),
                unit: SongLuaTimeUnit::Beat,
                start: 1.0,
                limit: 1.0,
                span_mode: SongLuaSpanMode::Len,
                target: SongLuaEaseTarget::Mod("skewy".to_string()),
                from: 0.0,
                to: -4.0,
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            },
        ],
        ..Default::default()
    };

    let (windows, unsupported) =
        deadsync_song_lua::gameplay::build_song_lua_ease_windows_for_player(
            &compiled,
            &timing,
            0,
            0.0,
            &[],
        );

    assert_eq!(unsupported, 0);
    assert_eq!(windows.len(), 2);
    assert!(matches!(
        windows[0].target,
        SongLuaEaseMaskTarget::PlayerSkewX
    ));
    assert!(matches!(
        windows[1].target,
        SongLuaEaseMaskTarget::PlayerSkewY
    ));
    assert!((windows[0].to - 0.03).abs() <= 0.000_1);
    assert!((windows[1].to + 0.04).abs() <= 0.000_1);
}

#[test]
fn song_lua_confusion_offset_ease_scales_like_itgmania() {
    let timing = test_timing(16 * 48);
    let compiled = TestCompiledSongLua {
        eases: vec![SongLuaEaseWindow {
            approach_speed: None,
            player: Some(1),
            unit: SongLuaTimeUnit::Beat,
            start: 0.0,
            limit: 4.0,
            span_mode: SongLuaSpanMode::Len,
            target: SongLuaEaseTarget::Mod("confusionoffset".to_string()),
            from: -85.0,
            to: 0.0,
            easing: Some("outQuad".to_string()),
            sustain: None,
            opt1: None,
            opt2: None,
        }],
        ..Default::default()
    };

    let (windows, unsupported) =
        deadsync_song_lua::gameplay::build_song_lua_ease_windows_for_player(
            &compiled,
            &timing,
            0,
            0.0,
            &[],
        );

    assert_eq!(unsupported, 0);
    assert_eq!(windows.len(), 1);
    assert!(matches!(
        windows[0].target,
        SongLuaEaseMaskTarget::VisualConfusionOffset
    ));
    assert!((windows[0].from + 0.85).abs() <= 0.000_1);
    assert!(windows[0].to.abs() <= 0.000_1);
}

#[test]
fn confusion_offset_fixture_reaches_runtime_windows() {
    let root = deadsync_root();
    let song_root = root.join("tests/fixtures/song_lua");
    let entry = song_root.join("runtime-constants.lua");
    assert!(entry.is_file(), "missing fixture: {}", entry.display());
    let context = test_song_lua_double_context(&song_root, "Constant Mod Fixture");
    let compiled = compile_song_lua_with_default_host(
        &entry,
        &context,
        SongLuaNoteskinResolver::default(),
        test_read_model_slots,
        test_model_layer_from_slot,
        |_context, _noteskin| None,
    )
    .unwrap();

    assert!(compiled.beat_mods.iter().any(|window| {
        (window.start - 70.5).abs() <= 0.001 && window.mods.contains("80% confusionoffset")
    }));

    let timing_segments = TimingSegments {
        bpms: vec![(0.0, 128.0)],
        ..TimingSegments::default()
    };
    let timing =
        TimingData::from_segments(0.036, 0.0, &timing_segments, &test_row_to_beat(72 * 48));
    let windows = deadsync_song_lua::gameplay::build_song_lua_constant_windows_for_player(
        &compiled, &timing, 0, 0.0,
    );

    assert!(windows.iter().any(|window| {
        (window.start_second - timing.get_time_for_beat(70.5)).abs() <= 0.001
            && (window.end_second - timing.get_time_for_beat(71.0)).abs() <= 0.001
            && window
                .visual
                .confusion_offset
                .is_some_and(|value| (value - 0.8).abs() <= 0.000_1)
    }));
}

#[test]
fn transform_ease_fixture_reaches_runtime_windows() {
    let root = deadsync_root();
    let song_root = root.join("tests/fixtures/song_lua");
    let entry = song_root.join("runtime-eases.lua");
    assert!(entry.is_file(), "missing fixture: {}", entry.display());
    let context = test_song_lua_double_context(&song_root, "Transform Ease Fixture");
    let compiled = compile_song_lua_with_default_host(
        &entry,
        &context,
        SongLuaNoteskinResolver::default(),
        test_read_model_slots,
        test_model_layer_from_slot,
        |_context, _noteskin| None,
    )
    .unwrap();

    assert!(compiled.eases.iter().any(|window| {
        matches!(
            window.target,
            SongLuaEaseTarget::Mod(ref name)
                if name == "tiny"
        ) && (window.start - 26.5).abs() <= 0.001
            && (window.to + 200.0).abs() <= 0.001
    }));
    assert!(compiled.eases.iter().any(|window| {
        matches!(
            window.target,
            SongLuaEaseTarget::Mod(ref name)
                if name == "flip"
        ) && (window.start - 26.5).abs() <= 0.001
            && (window.to - 50.0).abs() <= 0.001
    }));
    assert!(compiled.eases.iter().any(|window| {
        matches!(
            window.target,
            SongLuaEaseTarget::Mod(ref name)
                if name == "dark"
        ) && (window.start - 28.0).abs() <= 0.001
            && (window.to - 100.0).abs() <= 0.001
    }));
    assert!(compiled.eases.iter().any(|window| {
        matches!(
            window.target,
            SongLuaEaseTarget::Mod(ref name)
                if name == "skewx"
        ) && (window.start - 166.0).abs() <= 0.001
            && (window.to.abs() - 3.0).abs() <= 0.001
    }));
    assert!(compiled.eases.iter().any(|window| {
        matches!(
            window.target,
            SongLuaEaseTarget::Mod(ref name)
                if name == "skewx"
        ) && (window.start - 182.0).abs() <= 0.001
            && (window.to.abs() - 3.0).abs() <= 0.001
    }));
    assert!(compiled.eases.iter().any(|window| {
        matches!(window.target, SongLuaEaseTarget::PlayerRotationX)
            && (window.start - 189.0).abs() <= 0.001
            && (window.to - 20.0).abs() <= 0.001
    }));

    let timing_segments = TimingSegments {
        bpms: vec![(0.0, 77.0)],
        ..TimingSegments::default()
    };
    let timing = TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(200 * 48));
    let constants = deadsync_song_lua::gameplay::build_song_lua_constant_windows_for_player(
        &compiled, &timing, 0, 0.0,
    );
    let (windows, unsupported) =
        deadsync_song_lua::gameplay::build_song_lua_ease_windows_for_player(
            &compiled, &timing, 0, 0.0, &constants,
        );

    assert_eq!(unsupported, 0);
    assert!(windows.iter().any(|window| {
        matches!(window.target, SongLuaEaseMaskTarget::PlayerSkewX)
            && (window.start_second - timing.get_time_for_beat(166.0)).abs() <= 0.001
            && (window.to.abs() - 0.03).abs() <= 0.000_1
    }));
    assert!(windows.iter().any(|window| {
        matches!(window.target, SongLuaEaseMaskTarget::PlayerSkewX)
            && (window.start_second - timing.get_time_for_beat(182.0)).abs() <= 0.001
            && (window.to.abs() - 0.03).abs() <= 0.000_1
    }));
    assert!(windows.iter().any(|window| {
        matches!(window.target, SongLuaEaseMaskTarget::PlayerRotationX)
            && (window.start_second - timing.get_time_for_beat(189.0)).abs() <= 0.001
            && (window.to - 20.0).abs() <= 0.000_1
    }));
    assert!(windows.iter().any(|window| {
        matches!(window.target, SongLuaEaseMaskTarget::VisualTiny)
            && (window.start_second - timing.get_time_for_beat(26.5)).abs() <= 0.001
            && (window.to + 2.0).abs() <= 0.000_1
    }));
    assert!(windows.iter().any(|window| {
        matches!(window.target, SongLuaEaseMaskTarget::VisualFlip)
            && (window.start_second - timing.get_time_for_beat(26.5)).abs() <= 0.001
            && (window.to - 0.5).abs() <= 0.000_1
    }));
    assert!(windows.iter().any(|window| {
        matches!(window.target, SongLuaEaseMaskTarget::VisibilityDark)
            && (window.start_second - timing.get_time_for_beat(28.0)).abs() <= 0.001
            && (window.to - 1.0).abs() <= 0.000_1
    }));
}

#[test]
fn riddle_option_writes_keep_swaps_discrete_and_merge_gradual() {
    use deadsync_gameplay::{
        AppearanceEffects, AttackBaseEffects, GameplayAttackRuntimeState, SongLuaPlayerTransform,
    };
    let root = deadsync_root().join("tests/fixtures/song_lua");
    let mut context = test_song_lua_double_context(&root, "Riddle approach");
    context.song_display_bpms = [128.0; 2];
    context.music_length_seconds = 2.0;
    let compiled = compile_song_lua_with_default_host(
        &root.join("riddle-approach.lua"),
        &context,
        SongLuaNoteskinResolver::default(),
        test_read_model_slots,
        test_model_layer_from_slot,
        |_, _| None,
    )
    .expect("compile recurring option writes");
    let timing = TimingData::from_segments(
        0.0,
        0.0,
        &TimingSegments {
            bpms: vec![(0.0, 128.0)],
            ..TimingSegments::default()
        },
        &test_row_to_beat(8 * 48),
    );
    let constants = deadsync_song_lua::gameplay::build_song_lua_constant_windows_for_player(
        &compiled, &timing, 0, 0.0,
    );
    let (windows, unsupported) =
        deadsync_song_lua::gameplay::build_song_lua_ease_windows_for_player(
            &compiled, &timing, 0, 0.0, &constants,
        );
    assert_eq!(unsupported, 0);
    let invert = windows
        .iter()
        .find(|w| w.target == SongLuaEaseMaskTarget::VisualInvert && w.to == 1.0)
        .expect("fast invert target");
    assert_eq!(invert.approach_speed, Some(1000.0));
    let swap_at = invert.start_second;
    let merge_at = windows
        .iter()
        .find(|w| w.target == SongLuaEaseMaskTarget::VisualFlip && w.to == 0.5)
        .expect("slow merge target")
        .start_second;
    let mut runtime = GameplayAttackRuntimeState::new([constants, vec![]], [windows, vec![]]);
    let mut transform = SongLuaPlayerTransform::default();
    let mut advance = |runtime: &mut GameplayAttackRuntimeState, now, dt| {
        if let Some(next) = runtime.refresh_player(
            0,
            now,
            dt,
            AppearanceEffects::default(),
            AttackBaseEffects::default,
            transform,
        ) {
            transform = next;
        }
    };
    let dt = 1.0 / 240.0;
    advance(&mut runtime, swap_at - dt, dt);
    assert_eq!(
        runtime.visual[0].invert,
        Some(0.0),
        "no interpolation toward a future swap"
    );
    advance(&mut runtime, swap_at, dt);
    assert_eq!(
        runtime.visual[0].invert,
        Some(1.0),
        "1000 units/s swaps within one frame"
    );
    advance(&mut runtime, merge_at - dt, 1.0);
    for frame in 0..24 {
        advance(&mut runtime, merge_at + frame as f32 * dt, dt);
        // PlayerOptions::Approach uses speed * seconds, in option units.
        let amount = 10.0 * (frame + 1) as f32 * dt;
        assert!((runtime.visual[0].flip.expect("flip") - amount.min(0.5)).abs() < 0.00001);
        assert!(
            (runtime.mini_percent[0].expect("mini") + amount.min(1.0) * 100.0).abs() < 0.0001,
            "frame {frame}: mini={:?}, amount={amount}",
            runtime.mini_percent[0]
        );
        assert!((runtime.scroll[0].reverse.expect("reverse") - amount.min(0.1)).abs() < 0.00001);
    }
}
