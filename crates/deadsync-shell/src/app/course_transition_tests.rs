use super::*;
use crate::gameplay_regression_tests::tests::{build_compiled_test_state, with_session};
use deadsync_chart::{SongPack, SyncPref};
use deadsync_gameplay::{GameplayInputPlayStyle, GameplaySession, GameplayViewport};
use deadsync_simfile::{
    cache::build_song_meta,
    song::{ParseSongOptions, parse_song_bytes},
};
use deadsync_theme_simply_love::views::{SelectCourseContextView, SelectCourseInitView};

// App owns the complete screen/runtime state. As with the compositor
// integration tests, give debug builds enough stack for construction/moves.
fn on_app_thread(test: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .name("endless-transition".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(test)
        .unwrap()
        .join()
        .unwrap();
}

fn finish_pending_stage(app: &mut App) {
    if let Some(pending) = app.pending_gameplay_init.take() {
        let state = pending
            .receiver
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap();
        app.finish_gameplay_init(state, &pending.finish);
    }
}

fn fixture(root: &std::path::Path, name: &str, pump: bool) -> Arc<SongData> {
    let path = root.join("Songs/PR845").join(name).join("partner.sm");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let kind = if pump {
        "pump-routine"
    } else {
        "dance-routine"
    };
    let first = if pump { "1000000000" } else { "10000000" };
    let second = if pump { "0000000001" } else { "00000001" };
    let source = format!(
        "#TITLE:{name};\n#BPMS:0=120;\n#NOTES:{kind}::Hard:3:0,0,0,0,0:\n{first}\n&\n{second}\n;\n"
    );
    std::fs::write(&path, &source).unwrap();
    let parsed = parse_song_bytes(
        source.as_bytes(),
        "sm",
        &path,
        &ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
        |_| 2.0,
    )
    .unwrap();
    Arc::new(build_song_meta(parsed, 0.0))
}

fn test_app(root: &std::path::Path) -> App {
    let dirs = AppDirs {
        data_dir: root.join("data"),
        cache_dir: root.join("cache"),
        exe_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap(),
        portable: false,
    };
    dirs.ensure_dirs_exist();
    let cfg = config::app_config::Config {
        video_renderer: BackendType::Software,
        ..Default::default()
    };
    App::new(
        dirs,
        0,
        0,
        0,
        cfg,
        profile_data::Profile::default(),
        deadsync_audio_stream::AudioControl::without_audio(),
        deadsync_audio_stream::MusicClock::without_audio(),
        None,
    )
}

fn prepare_boundary(app: &mut App, root: &std::path::Path, pump: bool) -> Arc<SongData> {
    let a = fixture(root, "A", pump);
    let b = fixture(root, "B", pump);
    let course_path = root.join("endless.crs");
    std::fs::write(
        &course_path,
        "#COURSE:PR845 transition;\n#REPEAT:YES;\n#SONG:PR845/A:Hard;\n#SONG:PR845/B:Hard;\n",
    )
    .unwrap();
    let course = deadsync_simfile::course::parse_course_file(&course_path).unwrap();
    let style = if pump {
        profile_data::PlayStyle::PumpRoutine
    } else {
        profile_data::PlayStyle::Routine
    };
    app.state.screens.select_course_state = select_course::init(SelectCourseInitView {
        song_packs: vec![SongPack {
            group_name: "PR845".into(),
            name: "PR845".into(),
            sort_title: "PR845".into(),
            translit_title: String::new(),
            series: String::new(),
            folder_series: String::new(),
            year: 0,
            sync_pref: SyncPref::Default,
            directory: root.join("Songs/PR845"),
            banner_path: None,
            songs: vec![a.clone(), b.clone()],
        }],
        courses: vec![(course_path.clone(), course)],
        last_course_path: Some(course_path.clone()),
        last_course_difficulty: Some("Hard".into()),
        context: SelectCourseContextView {
            play_style: style,
            joined: [true, true],
            ..Default::default()
        },
        ..Default::default()
    });
    select_course::restore_selection_for_course(
        &mut app.state.screens.select_course_state,
        &course_path,
        Some("Hard"),
    );
    let mut plan = select_course::selected_course_plan(&app.state.screens.select_course_state)
        .expect("real selected course plan");
    assert_eq!(plan.stages.len(), 2);
    // The previous cycle contains A. The fresh cycle must append A then B.
    plan.stages.truncate(1);
    let mut run = build_course_run_from_selection(plan, style.chart_type()).unwrap();
    run.next_stage_index = 1;
    app.state.session.begin_play_session(Instant::now());
    app.state.session.last_course_wheel_path = Some(course_path);
    app.state.session.course_run = Some(run);
    let mut stage = build_compiled_test_state(
        &a.simfile_path,
        GameplayViewport::new(640.0, 480.0),
        GameplaySession {
            play_style: if pump {
                GameplayInputPlayStyle::PumpRoutine
            } else {
                GameplayInputPlayStyle::Routine
            },
            joined_sides: [true, true],
            ..Default::default()
        },
        std::array::from_fn(|_| profile_data::Profile::default()),
    );
    stage.progress.stage.song_completed_naturally = true;
    app.state.screens.gameplay_state = Some(stage);
    app.state.screens.current_screen = CurrentScreen::Gameplay;
    b
}

#[test]
fn endless_failed_renewal_finishes_with_course_evaluation_and_completed_song() {
    on_app_thread(|| {
        for pump in [false, true] {
            for failure in ["payload", "plan", "cursor"] {
                let style = if pump {
                    profile_data::PlayStyle::PumpRoutine
                } else {
                    profile_data::PlayStyle::Routine
                };
                with_session(style, profile_data::PlayerSide::P1, true, true, || {
                    let root = tempfile::Builder::new()
                        .prefix(&format!("pr845-transition-failure-{pump}-{failure}-"))
                        .tempdir()
                        .unwrap();
                    let mut app = test_app(root.path());
                    let b = prepare_boundary(&mut app, root.path(), pump);
                    match failure {
                        // Selection still has B's metadata, but its payload load fails.
                        "payload" => std::fs::remove_file(&b.simfile_path).unwrap(),
                        "plan" => {
                            app.state.screens.select_course_state =
                                select_course::init(SelectCourseInitView::default())
                        }
                        "cursor" => {
                            app.state
                                .session
                                .course_run
                                .as_mut()
                                .unwrap()
                                .next_stage_index = 2
                        }
                        _ => unreachable!(),
                    }
                    let (target, _) = app.prepare_global_fade_entry(CurrentScreen::Gameplay);
                    assert_eq!(target, CurrentScreen::Evaluation);
                    assert_eq!(
                        app.state.shell.interaction.message(),
                        Some("Unable to prepare next course stage")
                    );
                    assert_eq!(app.state.screens.current_screen, CurrentScreen::Evaluation);
                    assert!(app.state.session.course_run.is_none());
                    assert!(app.state.screens.player_options_state.is_none());
                    assert!(app.pending_gameplay_init.is_none());
                    assert!(app.state.screens.gameplay_state.is_none());
                    assert_eq!(app.state.session.played_stages.len(), 2);
                    assert_eq!(app.state.session.played_stages[0].song.title, "A");
                    assert_eq!(
                        app.state.session.played_stages[1].song.title,
                        "PR845 transition"
                    );
                    assert!(app.state.session.course_stage_eval_pages.is_empty());
                    assert_eq!(app.state.session.course_eval_pages.len(), 2);
                    assert!(app.state.screens.evaluation_state.return_to_course);
                    assert_eq!(
                        app.state.session.last_course_wheel_path.as_deref(),
                        Some(root.path().join("endless.crs").as_path())
                    );
                });
            }
        }
    });
}

#[test]
fn endless_successful_renewal_starts_next_song_with_extended_totals() {
    on_app_thread(|| {
        with_session(
            profile_data::PlayStyle::Routine,
            profile_data::PlayerSide::P1,
            true,
            true,
            || {
                let root = tempfile::Builder::new()
                    .prefix("pr845-transition-success-")
                    .tempdir()
                    .unwrap();
                let mut app = test_app(root.path());
                prepare_boundary(&mut app, root.path(), false);
                let before = app
                    .state
                    .session
                    .course_run
                    .as_ref()
                    .unwrap()
                    .course_display_totals;
                let (target, _) = app.prepare_global_fade_entry(CurrentScreen::Gameplay);
                assert_eq!(target, CurrentScreen::Gameplay);
                let run = app.state.session.course_run.as_ref().unwrap();
                assert_eq!(run.stages.len(), 3);
                for (after, before) in run.course_display_totals.iter().zip(before) {
                    assert_eq!(
                        after.possible_grade_points,
                        3 * before.possible_grade_points
                    );
                    assert_eq!(after.total_steps, 3 * before.total_steps);
                }
                assert_eq!(app.state.session.played_stages.len(), 1);
                finish_pending_stage(&mut app);
                assert_eq!(
                    app.state
                        .session
                        .course_run
                        .as_ref()
                        .unwrap()
                        .next_stage_index,
                    2
                );
                assert_eq!(
                    app.state
                        .screens
                        .gameplay_state
                        .as_ref()
                        .unwrap()
                        .song()
                        .title,
                    "A"
                );
                // Complete A and then B: B is inside the prepared cycle, so it
                // must not renew. The following boundary must renew again.
                for expected_cursor in [3, 4] {
                    app.state
                        .screens
                        .gameplay_state
                        .as_mut()
                        .unwrap()
                        .progress
                        .stage
                        .song_completed_naturally = true;
                    let (target, _) = app.prepare_global_fade_entry(CurrentScreen::Gameplay);
                    assert_eq!(target, CurrentScreen::Gameplay);
                    finish_pending_stage(&mut app);
                    assert_eq!(
                        app.state
                            .session
                            .course_run
                            .as_ref()
                            .unwrap()
                            .next_stage_index,
                        expected_cursor
                    );
                }
                let run = app.state.session.course_run.as_ref().unwrap();
                assert_eq!(run.stages.len(), 5);
                for (after, before) in run.course_display_totals.iter().zip(before) {
                    assert_eq!(
                        after.possible_grade_points,
                        5 * before.possible_grade_points
                    );
                }
                assert_eq!(
                    app.state
                        .session
                        .played_stages
                        .iter()
                        .map(|stage| stage.song.title.as_str())
                        .collect::<Vec<_>>(),
                    ["A", "A", "B"]
                );
            },
        );
    });
}

/// Profiles live under the test services' root and are removed after restoring
/// the active identities, including when an assertion unwinds.
struct ScoreProfiles {
    previous: [profile_data::ActiveProfile; MAX_PLAYERS],
    directories: [tempfile::TempDir; MAX_PLAYERS],
}
impl ScoreProfiles {
    fn new() -> Self {
        let root = profile_data::app_runtime::profiles_root();
        std::fs::create_dir_all(&root).unwrap();
        let directories = std::array::from_fn(|_| {
            tempfile::Builder::new()
                .prefix("pr858-score-profile-")
                .tempdir_in(&root)
                .unwrap()
        });
        let previous = profile_data::runtime_active_profiles();
        let profiles = std::array::from_fn(|side| profile_data::ActiveProfile::Local {
            // Non-GUID ids resolve to these exact temporary profile folders.
            id: directories[side]
                .path()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned(),
        });
        let _ = profile_data::runtime_set_active_profiles(profiles);
        Self {
            previous,
            directories,
        }
    }
}
impl Drop for ScoreProfiles {
    fn drop(&mut self) {
        let _ = profile_data::runtime_set_active_profiles(self.previous.clone());
    }
}

#[test]
fn interrupted_endless_saves_accumulated_routine_score_and_all_evaluation_pages() {
    on_app_thread(|| {
        for pump in [false, true] {
            let style = if pump {
                profile_data::PlayStyle::PumpRoutine
            } else {
                profile_data::PlayStyle::Routine
            };
            with_session(style, profile_data::PlayerSide::P1, true, true, || {
                let root = tempfile::Builder::new()
                    .prefix("pr858-endless-result-")
                    .tempdir()
                    .unwrap();
                let mut app = test_app(root.path());
                let b = prepare_boundary(&mut app, root.path(), pump);
                let profiles = ScoreProfiles::new();
                let score_hash = app
                    .state
                    .session
                    .course_run
                    .as_ref()
                    .unwrap()
                    .score_hash
                    .clone();
                // Finish the previous cycle and both songs of the next cycle.
                for _ in 0..2 {
                    let (target, _) = app.prepare_global_fade_entry(CurrentScreen::Gameplay);
                    assert_eq!(target, CurrentScreen::Gameplay);
                    finish_pending_stage(&mut app);
                    app.state
                        .screens
                        .gameplay_state
                        .as_mut()
                        .unwrap()
                        .progress
                        .stage
                        .song_completed_naturally = true;
                }
                assert_eq!(app.state.session.played_stages.len(), 2);
                assert_eq!(app.state.session.course_stage_eval_pages.len(), 2);
                std::fs::remove_file(&b.simfile_path).unwrap();
                let (target, _) = app.prepare_global_fade_entry(CurrentScreen::Gameplay);
                assert_eq!(target, CurrentScreen::Evaluation);
                assert!(app.state.session.course_run.is_none());
                assert!(app.state.screens.gameplay_state.is_none());
                assert!(app.pending_gameplay_init.is_none());
                assert_eq!(app.state.session.played_stages.len(), 4);
                assert_eq!(app.state.session.course_eval_pages.len(), 4);
                assert!(app.state.session.course_stage_eval_pages.is_empty());
                assert_eq!(app.state.session.played_stages[0].song.title, "A");
                assert_eq!(app.state.session.played_stages[1].song.title, "A");
                assert_eq!(app.state.session.played_stages[2].song.title, "B");
                assert_eq!(
                    app.state.session.played_stages[3].song.title,
                    "PR845 transition"
                );
                assert!(app.state.screens.evaluation_state.return_to_course);
                for (side, directory) in profiles.directories.iter().enumerate() {
                    let index = score_data::load_local_score_index_from_root(
                        &directory.path().join("scores/local"),
                    );
                    assert_eq!(
                        index.best_itg.contains_key(&score_hash),
                        side == 0,
                        "Routine saves the accumulated course result for its master only"
                    );
                }
            });
        }
    });
}
