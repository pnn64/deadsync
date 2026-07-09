use deadsync_gameplay::{SongLuaEaseMaskTarget, song_lua_ease_window_value};
use deadsync_rules::timing::{TimingData, TimingSegments};
use deadsync_song_lua::{
    CompiledSongLua, SongLuaColumnOffsetWindow, SongLuaCompileContext, SongLuaDifficulty,
    SongLuaEaseTarget, SongLuaEaseWindow, SongLuaMessageEvent, SongLuaModWindow,
    SongLuaNoteskinResolver, SongLuaOverlayActor, SongLuaOverlayCommandBlock, SongLuaOverlayEase,
    SongLuaOverlayMessageCommand, SongLuaOverlayModelLayer, SongLuaOverlayState,
    SongLuaPlayerContext, SongLuaSpanMode, SongLuaSpeedMod, SongLuaTimeUnit,
    compile_song_lua_with_default_host,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

type TestCompiledSongLua = CompiledSongLua<SongLuaOverlayActor<()>>;

fn deadsync_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
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

fn test_read_model_slots(_: &Path) -> Result<Arc<[()]>, String> {
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
                message: "ResetBlack".to_string(),
                blocks: vec![SongLuaOverlayCommandBlock {
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

    let windows = super::build_song_lua_overlay_ease_windows(&compiled, &timing, 0.0);

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
                message: "SetupZoom".to_string(),
                blocks: vec![SongLuaOverlayCommandBlock {
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

    let windows = super::build_song_lua_overlay_ease_windows(&compiled, &timing, 0.0);

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
                message: "ResetBlack".to_string(),
                blocks: vec![SongLuaOverlayCommandBlock {
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

    let windows = super::build_song_lua_overlay_ease_windows(&compiled, &timing, 0.0);

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
        super::build_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &[]);

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

    let constants = super::build_song_lua_constant_windows_for_player(&compiled, &timing, 0, 0.0);
    let (windows, unsupported) =
        super::build_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &constants);

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

    let windows =
        super::build_song_lua_column_offset_windows_for_player(&compiled, &timing, 0, 0.0);

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
        super::build_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &[]);

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
        super::build_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &[]);

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
        super::build_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &[]);

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
fn riddle_beat_70_confusion_offset_reaches_runtime_windows_if_present() {
    let root = deadsync_root();
    let Some(song_root) = [
        root.join("../lua-songs/Riddle"),
        root.join("songs/lua-songs/Riddle"),
    ]
    .into_iter()
    .find(|root| root.join("lua/default.lua").is_file()) else {
        return;
    };
    let entry = song_root.join("lua/default.lua");
    let context = test_song_lua_double_context(&song_root, "Riddle");
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
    let windows = super::build_song_lua_constant_windows_for_player(&compiled, &timing, 0, 0.0);

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
fn kenpo_flash_mods_reach_runtime_windows_if_present() {
    let root = deadsync_root();
    let Some(song_root) = [
        root.join("../lua-songs/[11] KENPO SAITO (DX) [Scrypts]"),
        root.join("songs/ITL Online 2026/[11] KENPO SAITO (DX) [Scrypts]"),
        root.join("songs/lua-songs/[11] KENPO SAITO (DX) [Scrypts]"),
    ]
    .into_iter()
    .find(|root| root.join("template/main.lua").is_file()) else {
        return;
    };
    let entry = song_root.join("template/main.lua");
    let context = test_song_lua_double_context(&song_root, "KENPO SAITO");
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
    let constants = super::build_song_lua_constant_windows_for_player(&compiled, &timing, 0, 0.0);
    let (windows, unsupported) =
        super::build_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &constants);

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
