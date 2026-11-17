use super::*;

type Prepared = PreparedGameplaySongLua<deadsync_assets::noteskin::SpriteSlot>;

fn fixture() -> (tempfile::TempDir, SongData, crate::SongLuaCompileContext) {
    crate::tests::init_paths();
    let dir = tempfile::tempdir().unwrap();
    for name in ["background.lua", "foreground.lua"] {
        std::fs::write(dir.path().join(name), "return Def.ActorFrame{}").unwrap();
    }
    let simfile = dir.path().join("song.ssc");
    std::fs::write(&simfile, "#TITLE:Lua submission;\n#BPMS:0=120;\n#BGCHANGES:0=background.lua=1=0=0=1;\n#FGCHANGES:0=foreground.lua=1=0=0=1;\n#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Challenge;\n#METER:1;\n#NOTES:\n1000\n0000\n0100\n0000\n;\n").unwrap();
    let song = deadsync_simfile::app_runtime::parse_song_for_test(&simfile, 0.0).unwrap();
    assert_eq!(song.background_lua_changes.len(), 1);
    assert_eq!(song.foreground_lua_changes.len(), 1);
    let mut context = crate::SongLuaCompileContext::new(dir.path(), &song.title);
    context.music_length_seconds = 0.25;
    (dir, song, context)
}

fn prepare(song: &SongData, context: &crate::SongLuaCompileContext) -> Prepared {
    prepare_song_lua_with_runtime_fallback(
        song,
        context,
        deadsync_assets::song_lua::compile_song_lua,
        deadsync_assets::song_lua::compile_song_lua_layers,
    )
}

#[test]
fn lua_submission_approval_requires_the_current_complete_compilation() {
    let (dir, song, context) = fixture();
    assert!(prepare(&song, &context).compile_succeeded());

    // Lua is outside the chart hash. Reusing the chart must not reuse approval.
    std::fs::write(dir.path().join("foreground.lua"), "return ???").unwrap();
    let failed = prepare(&song, &context);
    assert!(!failed.compile_succeeded());
    assert_eq!(
        failed.background_layers.len(),
        1,
        "visual fallback still works"
    );
    std::fs::write(dir.path().join("foreground.lua"), "return Def.ActorFrame{}").unwrap();
    assert!(prepare(&song, &context).compile_succeeded());

    std::fs::remove_file(dir.path().join("foreground.lua")).unwrap();
    assert!(!prepare(&song, &context).compile_succeeded());
    assert!(!Prepared::default().compile_succeeded());
}

#[test]
fn lua_submission_rejects_captured_callback_errors() {
    let (dir, song, context) = fixture();
    std::fs::write(
        dir.path().join("foreground.lua"),
        r#"
return Def.ActorFrame{
    OnCommand=function(self)
        self:SetUpdateFunction(function() error("broken authored update") end)
    end
}
"#,
    )
    .unwrap();
    let data = prepare(&song, &context);
    assert!(
        data.primary.is_some(),
        "compiler can retain a partial scene"
    );
    assert!(
        !data.compile_succeeded(),
        "a partial scene cannot approve scores"
    );
}

#[test]
fn lua_submission_allows_a_successful_runtime_compilation() {
    let (dir, song, context) = fixture();
    std::fs::write(
        dir.path().join("foreground.lua"),
        r#"
return Def.ActorFrame{
    OnCommand=function(self)
        SCREENMAN:GetTopScreen():AddInputCallback(function() return false end)
    end
}
"#,
    )
    .unwrap();
    let mut data = prepare(&song, &context);
    assert!(data.compile_succeeded());
    assert!(data.take_live_scene().unwrap().is_some());
    assert!(data.compile_succeeded());
}

#[test]
fn lua_submission_rejects_a_failed_shared_session_even_if_layers_recover() {
    let (_dir, song, context) = fixture();
    let data: Prepared = prepare_song_lua(
        &song,
        &context,
        deadsync_assets::song_lua::compile_song_lua,
        |_, _, _| Err("shared scene failed".into()),
    );
    assert!(data.primary.is_some());
    assert_eq!(data.background_layers.len(), 1);
    assert!(!data.compile_succeeded());
}

#[test]
fn captured_screen_and_draw_clocks_rebase_only_the_calibration_difference() {
    use crate::{
        SongLuaOverlayUpdateSample, SongLuaOverlayUpdateTarget as Target,
        SongLuaOverlayUpdateTrack, SongLuaOverlayUpdateValue as Value, SongLuaTimeUnit,
    };
    let timing = deadsync_rules::timing::TimingData::from_segments(
        0.042,
        -0.008,
        &deadsync_rules::timing::TimingSegments {
            bpms: vec![(0.0, 161.0)],
            ..Default::default()
        },
        &[],
    );
    let raw = 38.080_566_f32;
    let compiled: CompiledSongLua<deadsync_assets::noteskin::SpriteSlot> = CompiledSongLua {
        screen_width: 854.0,
        screen_height: 480.0,
        screen_overlay_index: Some(0),
        overlays: vec![SongLuaOverlayActor {
            kind: SongLuaOverlayKind::ActorFrame,
            name: Some("ScreenGameplay".into()),
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        }],
        overlay_update_unit: SongLuaTimeUnit::Second,
        overlay_capture_calibration_seconds: -0.008 * timing.global_offset_rate(),
        overlay_updates: vec![SongLuaOverlayUpdateTrack {
            overlay_index: 0,
            target: Target::X,
            samples: vec![
                SongLuaOverlayUpdateSample {
                    time: raw - 0.016,
                    value: Value::F32(0.0),
                },
                SongLuaOverlayUpdateSample {
                    time: raw,
                    value: Value::F32(427.0),
                },
            ],
        }],
        draw_frames: vec![SongLuaDrawFrame {
            second: raw,
            immediate: true,
            owners: vec![0],
            owner_ends: vec![0],
            ops: Vec::new(),
        }],
        ..Default::default()
    };
    for runtime_offset in [-0.008, 0.242] {
        let expected = if runtime_offset < 0.0 {
            raw
        } else {
            raw - 0.25 * timing.global_offset_rate()
        };
        for shared in [false, true] {
            let screen;
            let source = if shared {
                screen = song_lua_shared_screen(&compiled);
                &screen
            } else {
                &compiled
            };
            let overlays = song_lua_runtime_overlays(source, &timing, runtime_offset);
            let SongLuaOverlayKind::UpdateTracks {
                tracks,
                draw_frames,
            } = &overlays.last().unwrap().kind
            else {
                panic!("retained capture tracks");
            };
            assert_eq!(tracks[0].samples[1].second.to_bits(), expected.to_bits());
            if !shared {
                assert_eq!(draw_frames[0].second.to_bits(), expected.to_bits());
            }
            let mut current = SongLuaOverlayState::default();
            let mut cursors = vec![0; tracks.len()];
            apply_song_lua_overlay_runtime_updates_for(
                expected,
                tracks,
                0..tracks.len(),
                &mut cursors,
                None,
                &mut current,
            );
            assert_eq!(
                current.x, 427.0,
                "the screen jump is complete on its capture frame"
            );
        }
    }
}
