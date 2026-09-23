//! Optional validation against the original song, without installing its skin.
use deadsync_assets::{init_paths, noteskin, song_lua};
use deadsync_config::dirs::AppDirs;
use deadsync_song_lua::SongLuaCompileContext;
use std::path::PathBuf;

#[test]
#[ignore = "requires DEADSYNC_ROSEWOOD pointing to the VANITY ANGEL song directory"]
fn rosewood_loads_skin_and_compiles_chart() {
    let song = PathBuf::from(std::env::var_os("DEADSYNC_ROSEWOOD").expect("set DEADSYNC_ROSEWOOD"))
        .canonicalize()
        .unwrap();
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let dirs = AppDirs {
        data_dir: workspace.clone(),
        exe_dir: workspace.clone(),
        cache_dir: workspace.join("target/rosewood-check"),
        portable: true,
    };
    init_paths(dirs.asset_paths(Some(&workspace))).unwrap();
    let skin = noteskin::load_song_skin(
        &noteskin::Style {
            num_cols: 4,
            num_players: 1,
        },
        &song,
        "SCH-CLASSIC-RAINBOW",
        "",
    )
    .expect("chart-local noteskin loads");
    assert_eq!(skin.notes.len(), 4 * noteskin::NUM_QUANTIZATIONS);
    assert!(skin.notes.iter().all(|slot| slot.model.is_some()));
    assert!(skin.notes.iter().any(|slot| slot.uv_velocity[1] == -1.0));
    for slot in &skin.receptor_off {
        assert_eq!(slot.beat_receptor_start, Some(-8.0));
        assert_eq!(slot.frame_index(0.0, -9.0), 2);
        for (beat, frame) in [(0.0, 0), (0.1, 1), (0.5, 1), (0.9, 0), (1.0, 0)] {
            assert_eq!(slot.frame_index(0.0, beat), frame);
        }
    }
    for (column, (frame, rotation)) in [(0, 90.0), (5, 0.0), (8, 180.0), (13, 90.0)]
        .into_iter()
        .enumerate()
    {
        let spark = skin.mine_frames[column]
            .as_ref()
            .expect("animated mine spark");
        assert_eq!(
            spark.def.src,
            [
                (frame % 4) * spark.def.size[0],
                (frame / 4) * spark.def.size[1]
            ]
        );
        assert_eq!(spark.model_draw.rot[2], rotation);
    }
    let reverse = noteskin::load_song_skin(
        &noteskin::Style {
            num_cols: 4,
            num_players: 1,
        },
        &song,
        "SCH-CLASSIC-RAINBOW",
        "Reverse",
    )
    .unwrap();
    for (column, direction) in [(1, "Up"), (2, "Down")] {
        let body = reverse.hold_columns[column].body_active.as_ref().unwrap();
        assert!(
            body.texture_key()
                .contains(&format!("{direction} Hold Body Active"))
        );
    }
    assert!(skin.mines.iter().all(Option::is_some));
    let mut context = SongLuaCompileContext::new(&song, "VANITY ANGEL");
    context.song_music_rate = 1.0;
    context.song_timing_bpms = vec![(0.0, 140.0)];
    context.music_length_seconds = 96.0;
    let compiled = song_lua::compile_song_lua(&song.join("lua/default.lua"), &context)
        .expect("original chart compiles");
    assert_eq!(
        compiled.startup.noteskins,
        [
            Some("SCH-CLASSIC-RAINBOW".into()),
            Some("SCH-CLASSIC-RAINBOW".into())
        ]
    );
    assert_eq!(compiled.startup.min_seconds_to_music, Some(6.01));
    assert!(compiled.startup.hide_in);
    assert_eq!(compiled.info.unsupported_perframes, 0);
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert!(compiled.info.skipped_message_command_captures.is_empty());
    for (message, seconds) in [
        ("SchIntroLine", -5.76),
        ("SchIntroCard", -5.58),
        ("SchIntroReady", -1.50),
        ("SchIntroGo", 0.0),
    ] {
        let event = compiled
            .messages
            .iter()
            .find(|event| event.message == message)
            .unwrap();
        assert!((event.beat - (seconds + 0.001) * 140.0 / 60.0).abs() < 0.0001);
    }
    assert!(!compiled.overlays.is_empty());
    assert_eq!(compiled.sound_paths.len(), 6);
    assert!(compiled.sound_paths.iter().all(|path| path.is_file()));
    assert!(!noteskin::song_lua_noteskin_exists("SCH-CLASSIC-RAINBOW"));
}
