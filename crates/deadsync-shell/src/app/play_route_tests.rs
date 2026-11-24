use super::course_transition_tests::{finish_pending_stage, on_app_thread};
use super::*;
use crate::gameplay_regression_tests::tests::{actor_tree_has_text, fixture_assets};
use deadsync_config::theme::ThemeFlag;
use deadsync_core::input::InputSource;
use deadsync_gameplay::{GameplayAudioSnapshot, GameplayStreamClockSnapshot};
use deadsync_input::VirtualAction;
use std::process::Command as ProcessCommand;
use winit::platform::windows::EventLoopBuilderExtWindows;
use winit::window::WindowId;

const TEST_NAME: &str = "app::play_route_tests::configured_themes_complete_song_route";
const CHILD_THEME: &str = "DEADSYNC_TEST_PLAY_ROUTE_THEME";
const CHILD_COMPLETED: &str = "configured play route completed";
const SONG_TITLE: &str = "Route verification";
const STEP_SECONDS: f32 = 0.125;

#[test]
fn configured_themes_complete_song_route() {
    // The runtime configuration and winit event-loop registration are both
    // process-global. Give each configured launch its own isolated process.
    if let Ok(theme) = std::env::var(CHILD_THEME) {
        on_app_thread(move || run_child(&theme));
        println!("{CHILD_COMPLETED}");
        return;
    }
    let executable = std::env::current_exe().unwrap();
    for theme in ThemeFlag::CHOICES.map(ThemeFlag::as_str) {
        let child = ProcessCommand::new(&executable)
            .args(["--exact", TEST_NAME, "--nocapture"])
            .env(CHILD_THEME, theme)
            .output()
            .expect("start the isolated play route");
        let output = String::from_utf8_lossy(&child.stdout);
        assert!(
            child.status.success() && output.contains(CHILD_COMPLETED),
            "{theme} play route failed\n{output}\n{}",
            String::from_utf8_lossy(&child.stderr),
        );
    }
}

fn run_child(theme: &str) {
    let root = tempfile::tempdir().unwrap();
    let dirs = AppDirs {
        data_dir: root.path().join("data"),
        cache_dir: root.path().join("cache"),
        exe_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
        portable: false,
    };
    dirs.ensure_dirs_exist();
    init_paths(&dirs).unwrap();
    std::fs::write(
        dirs.config_path(),
        format!(
            "[Options]\nTheme={theme}\nVideoRenderer=Software\nLogToFile=0\nCoinMode=Home\n\
             EnableArrowCloud=0\nEnableGrooveStats=0\n\
             [Theme]\nMachineShowSelectProfile=1\nMachineShowSelectColor=0\n\
             MachineShowSelectStyle=1\nMachineShowSelectPlayMode=1\n",
        ),
    )
    .unwrap();
    config::runtime_load::load();
    config::runtime::flush_pending_saves();
    deadsync_theme_simply_love::i18n::init_for_tests();
    seed_library(&dirs);
    let expected_theme: ThemeFlag = theme.parse().unwrap();
    let (generation, cfg) = config::runtime::snapshot();
    assert_eq!(cfg.theme_flag, expected_theme);
    let paths = dirs.asset_paths(None);
    let mut app = App::new(
        dirs,
        0,
        cfg.theme_color,
        generation,
        cfg,
        profile::get(),
        deadsync_audio_stream::AudioControl::without_audio(),
        deadsync_audio_stream::MusicClock::without_audio(),
        None,
        crate::theme_registry::mount(cfg.theme_flag),
    );
    app.asset_manager = fixture_assets();
    let fonts = deadlib_assets::parse_font_asset_specs(
        app.theme_resources.fonts.iter().copied(),
        &paths.texture_roots,
        |path| paths.resolve_asset_path(path),
    ).expect("bundled theme fonts load");
    app.asset_manager
        .register_fonts(fonts.into_iter().map(|asset| (asset.name, asset.font)));

    let mut route = RouteTest { app: Some(app), theme: expected_theme };
    let event_loop = EventLoop::builder().with_any_thread(true).build().unwrap();
    event_loop.run_app(&mut route).unwrap();
    assert!(route.app.is_none(), "winit did not run the play route");
    config::runtime::flush_pending_saves();
}

fn seed_library(dirs: &AppDirs) {
    let directory = dirs.songs_dir().join("Route");
    let song_path = directory.join(SONG_TITLE).join("route.sm");
    std::fs::create_dir_all(song_path.parent().unwrap()).unwrap();
    std::fs::write(
        &song_path,
        format!(
            "#TITLE:{SONG_TITLE};\n#BPMS:0=120;\n#NOTES:dance-single::Hard:5:0,0,0,0,0:\n\
             0000\n,\n0000\n,\n0000\n1000\n0100\n0010\n,\n0001\n1000\n0000\n1000\n;\n",
        ),
    )
    .unwrap();
    let song = deadsync_simfile::app_runtime::parse_song_for_test(&song_path, 0.0).unwrap();
    deadsync_simfile::runtime_cache::set_song_cache(vec![deadsync_chart::SongPack {
        group_name: "Route".into(),
        name: "Route".into(),
        sort_title: "Route".into(),
        translit_title: String::new(),
        series: String::new(),
        folder_series: String::new(),
        year: 0,
        sync_pref: deadsync_chart::SyncPref::Default,
        directory,
        banner_path: None,
        songs: vec![Arc::new(song)],
    }]);
}

struct RouteTest {
    app: Option<App>,
    theme: ThemeFlag,
}

impl ApplicationHandler for RouteTest {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Some(mut app) = self.app.take() else {
            return;
        };
        play_route(&mut app, event_loop, self.theme);
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn play_route(app: &mut App, event_loop: &ActiveEventLoop, theme: ThemeFlag) {
    use deadsync_gameplay::music_wheel::WheelRandomAction;

    app.on_fade_complete(CurrentScreen::Menu, event_loop);
    settle(app, event_loop, CurrentScreen::Menu);
    for next in [
        CurrentScreen::SelectProfile,
        CurrentScreen::SelectStyle,
        CurrentScreen::SelectPlayMode,
        CurrentScreen::SelectMusic,
    ] {
        press(app, event_loop, VirtualAction::p2_start);
        settle(app, event_loop, next);
    }
    let (actors, _, _) = app.get_current_actors(0.0, app.frame_policy);
    assert!(actor_tree_has_text(&actors, SONG_TITLE));
    let openitg = theme == ThemeFlag::OpenItg;
    let native_stage = theme == ThemeFlag::StepMania395;
    assert_eq!(actor_tree_has_text(&actors, "CHANCE"), openitg);
    assert_eq!(actor_tree_has_text(&actors, "RANDOM"), openitg || native_stage);
    if openitg {
        let width = deadlib_present::space::screen_width();
        let height = deadlib_present::space::screen_height();
        let scale = (width / 640.0).min(height / 480.0);
        let p2_steps = [width * 0.5 + 104.0 * scale, height * 0.5 + 121.0 * scale];
        let font = actors
            .iter()
            .find_map(|actor| match actor {
                Actor::Text {
                    content,
                    offset,
                    font,
                    ..
                } if content.as_str() == "6"
                    && (offset[0] - p2_steps[0]).abs() < 0.01
                    && (offset[1] - p2_steps[1]).abs() < 0.01 =>
                {
                    Some(*font)
                }
                _ => None,
            })
            .expect("the selected chart renders its tap count on P2's side");
        assert!(
            app.asset_manager
                .with_font(font, |font| {
                    "Steps 6"
                        .chars()
                        .all(|character| font.glyph_map.contains_key(&character))
                })
                .unwrap_or(false)
        );
        press(app, event_loop, VirtualAction::p2_right);
        let wheel = &app.state.screens.select_music_state;
        assert!(matches!(
            wheel.entries[wheel.selected_index],
            MusicWheelEntry::Random(WheelRandomAction::Reveal)
        ));
        press(app, event_loop, VirtualAction::p2_start);
        assert_eq!(
            app.state.screens.stage_launch.prompt(),
            deadsync_gameplay::stage_launch::StageLaunchPrompt::Hidden,
        );
        assert_eq!(app.state.screens.current_screen, CurrentScreen::SelectMusic);
        let wheel = &app.state.screens.select_music_state;
        let revealed_index = wheel.selected_index;
        assert_eq!(select_music::focused_song(wheel).unwrap().title, SONG_TITLE);
        press(app, event_loop, VirtualAction::p2_right);
        assert_eq!(
            app.state.screens.select_music_state.selected_index,
            revealed_index
        );
    }

    // The first Start confirms the chart and offers the options window.
    press(app, event_loop, VirtualAction::p2_start);
    menu_frame(app, event_loop);
    let (actors, _, _) = app.get_current_actors(0.0, app.frame_policy);
    assert_eq!(
        actors.iter().any(|actor| matches!(actor, Actor::Sprite {
            source: deadlib_present::actors::SpriteSource::TextureStatic("openitg/wheel-options-prompt.png"),
            ..
        })),
        openitg,
    );
    press(app, event_loop, VirtualAction::p2_start);
    settle(app, event_loop, CurrentScreen::PlayerOptions);
    let selection = app
        .state
        .screens
        .player_options_state
        .as_ref()
        .unwrap()
        .selection();
    assert_eq!(selection.song.title, SONG_TITLE);
    let (actors, _, _) = app.get_current_actors(0.0, app.frame_policy);
    assert_eq!(actor_tree_has_text(&actors, "PICK MODIFIERS"), openitg);
    if openitg {
        let (cover, duration) = app.get_in_transition_for_screen(CurrentScreen::PlayerOptions);
        assert!(cover.is_empty());
        assert_eq!(duration, 0.5);
    }
    let normal_entry = app.state.screens.transitions.entry;
    app.state.screens.transitions.entry = |screen| {
        assert_eq!(screen, CurrentScreen::PlayerOptions);
        (Vec::new(), 0.75)
    };
    let mounted_duration = match theme {
        ThemeFlag::SimplyLove => 0.75,
        ThemeFlag::OpenItg | ThemeFlag::StepMania395 => 0.5,
    };
    assert_eq!(
        app.get_in_transition_for_screen(CurrentScreen::PlayerOptions).1,
        mounted_duration
    );
    app.state.screens.transitions.entry = normal_entry;

    // The editor's first Start moves focus to Exit and the next starts play.
    press(app, event_loop, VirtualAction::p2_start);
    press(app, event_loop, VirtualAction::p2_start);
    settle(app, event_loop, CurrentScreen::Gameplay);
    let gameplay = app.state.screens.gameplay_state.as_ref().unwrap();
    assert_eq!(gameplay.song().title, SONG_TITLE);
    assert_eq!(gameplay.setup.session.joined_sides, [false, true]);
    if openitg {
        check_openitg_gameplay_hud(app);
    }

    complete_stage(app, event_loop);
    let outro = app.state.screens.gameplay_outro.as_ref();
    assert_eq!(outro.is_some(), openitg || native_stage);
    assert!(outro.is_none_or(|outro| {
        outro.cue().is_none() && outro.duration() == if native_stage { 5.0 } else { 0.8 }
    }));
    let outgoing_track = deadlib_audio_core::active_music_track_id();
    settle(app, event_loop, CurrentScreen::Evaluation);
    let evaluation_track = outgoing_track + u64::from(openitg);
    assert_eq!(deadlib_audio_core::active_music_track_id(), evaluation_track);
    if openitg {
        let relative_path = app.state.screens.stage_presentation.evaluation_audio().music_loop.unwrap();
        assert!(deadsync_assets::resolve_asset_path(relative_path).is_file());
    }
    assert_eq!(app.state.session.played_stages.len(), 1);
    let result = &app.state.session.played_stages[0];
    assert!(result.players[0].is_none());
    let player = result.players[1].as_ref().unwrap();
    assert_eq!(player.notes_hit, 6);
    assert_eq!(player.jumps, [0, 0]);
    assert_eq!(player.combo_runs.len(), 1);
    assert_eq!(player.combo_runs[0].count, 6);
    let numeric_score = format!("{:09}", player.numeric_score);
    assert!(app.state.screens.evaluation_stage.is_some());
    if openitg {
        press(app, event_loop, VirtualAction::p2_start);
        press(app, event_loop, VirtualAction::p2_back);
        assert!(matches!(app.state.shell.transition, TransitionState::Idle));
        assert!(app.state.screens.evaluation_departure.is_none());
    }
    // Allow the results intro to settle before accepting a normal Start.
    for _ in 0..48 {
        menu_frame(app, event_loop);
    }
    assert_eq!(deadlib_audio_core::active_music_track_id(), evaluation_track);
    let (actors, _, _) = app.get_current_actors(0.0, app.frame_policy);
    if native_stage {
        assert!(actors.iter().any(|actor| matches!(actor, Actor::Sprite { source, .. }
            if source.texture_key() == Some("stepmania-395/evaluation-grade-p2"))));
        assert!(!actors.iter().any(|actor| matches!(actor, Actor::Sprite { source, .. }
            if source.texture_key() == Some("stepmania-395/evaluation-grade-p1"))));
        assert!(actor_tree_has_text(&actors, &numeric_score));
    } else {
        assert!(actor_tree_has_text(&actors, SONG_TITLE));
    }
    assert_eq!(
        actors.iter().any(|actor| matches!(actor,
            Actor::Sprite {
                source: deadlib_present::actors::SpriteSource::TextureStatic("openitg/gameplay-stage-1.png"),
                ..
            }
        )),
        openitg,
    );
    press(app, event_loop, VirtualAction::p2_start);
    if openitg {
        let exit = app.state.screens.evaluation_departure.as_ref().unwrap();
        assert_eq!(exit.cue, Some(deadsync_theme::UiCue::Confirm));
        assert!(matches!(app.state.shell.transition, TransitionState::FadingOut {
            target: CurrentScreen::SelectMusic, elapsed: 0.0, duration: 0.5
        }));
        let (actors, _, _) = app.get_current_actors(0.0, app.frame_policy);
        assert!(!actor_tree_has_text(&actors, "Loading..."));
        for _ in 0..2 {
            menu_frame(app, event_loop);
        }
        let (actors, _, _) = app.get_current_actors(0.0, app.frame_policy);
        assert!(actor_tree_has_text(&actors, "Loading..."));
        assert_eq!(app.state.screens.current_screen, CurrentScreen::Evaluation);
        assert_eq!(deadlib_audio_core::active_music_track_id(), evaluation_track);
    } else if native_stage {
        let exit = app.state.screens.evaluation_departure.as_ref().unwrap();
        assert_eq!(exit.cue, None);
        assert_eq!(exit.duration, 1.3);
    } else {
        assert!(app.state.screens.evaluation_departure.is_none());
    }
    settle(app, event_loop, CurrentScreen::SelectMusic);
    assert!(app.state.screens.evaluation_departure.is_none());
    if openitg {
        for _ in 0..2 {
            press(app, event_loop, VirtualAction::p2_right);
        }
        for _ in 0..16 {
            menu_frame(app, event_loop);
        }
        let wheel = &app.state.screens.select_music_state;
        assert!(matches!(
            wheel.entries[wheel.selected_index],
            MusicWheelEntry::Random(WheelRandomAction::Portal)
        ));
        let preview = Arc::clone(select_music::focused_song(wheel).unwrap());
        press(app, event_loop, VirtualAction::p2_start);
        assert_eq!(
            app.state.screens.stage_launch.prompt(),
            deadsync_gameplay::stage_launch::StageLaunchPrompt::OfferOptions,
        );
        settle(app, event_loop, CurrentScreen::Gameplay);
        assert_eq!(
            app.state
                .screens
                .gameplay_state
                .as_ref()
                .unwrap()
                .song()
                .simfile_path,
            preview.simfile_path,
        );
        check_failed_stage_outro(app, event_loop);
    }
}

fn check_failed_stage_outro(app: &mut App, event_loop: &ActiveEventLoop) {
    use deadlib_present::actors::SpriteSource;
    use deadsync_gameplay::{GameplayAction, GameplayExit};

    let game = &mut app.state.screens.gameplay_state.as_mut().unwrap().stage.gameplay;
    assert!(game.force_fail_player(0));
    assert!(game.all_joined_players_failed());
    app.handle_gameplay_action(GameplayAction::Navigate(GameplayExit::Complete), event_loop);
    assert_eq!(
        app.state.screens.gameplay_outro.as_ref().and_then(|outro| outro.cue()),
        Some(deadsync_theme::UiCue::StageFailed),
    );
    assert!(matches!(app.state.shell.transition, TransitionState::FadingOut {
        duration: 7.5, target: CurrentScreen::Evaluation, ..
    }));

    for (frames, key) in [(6, "openitg/outro-life.png"), (22, "openitg/outro-round.png")] {
        for _ in 0..frames {
            menu_frame(app, event_loop);
        }
        let (actors, _, gameplay) = app.get_current_actors(0.0, app.frame_policy);
        assert_eq!(gameplay.is_some(), frames == 6);
        assert!(actors.iter().any(|actor| matches!(actor,
            Actor::Sprite { source: SpriteSource::TextureStatic(texture), .. } if *texture == key
        )));
    }
    settle(app, event_loop, CurrentScreen::Evaluation);
    assert!(app.state.screens.gameplay_outro.is_none());
    assert_eq!(app.state.session.played_stages.len(), 2);
    assert!(stage_stats::all_players_failed(&app.state.session.played_stages[1], [false, true]));
}

fn check_openitg_gameplay_hud(app: &mut App) {
    use deadlib_present::actors::SpriteSource;
    use deadlib_present::space::{screen_center_x, screen_height, screen_width};

    let previous_transition = std::mem::replace(
        &mut app.state.shell.transition,
        TransitionState::FadingIn {
            elapsed: 1.0,
            duration: 2.0,
        },
    );
    let (actors, _, _) = app.get_current_actors(0.0, app.frame_policy);
    let y = screen_height() * 0.5;
    assert!(actors.iter().any(|actor| matches!(
        actor,
        Actor::Sprite { source: SpriteSource::TextureStatic(key), offset, .. }
            if key.starts_with("openitg/gameplay-stage-") && *offset == [screen_center_x(), y]
    )));
    app.state.shell.transition = previous_transition;

    let game = &mut app.state.screens.gameplay_state.as_mut().unwrap().stage.gameplay;
    let full_life = game.players()[0].life;
    game.update_player(0, |player| player.life = 0.25);
    let (actors, _, _) = app.get_current_actors(0.0, app.frame_policy);
    let scale = (screen_width() / 640.0).min(screen_height() / 480.0);
    let p2_x = screen_center_x() + 160.0 * scale;
    assert!(actors.iter().any(|actor| matches!(
        actor,
        Actor::Sprite {
            source: SpriteSource::TextureStatic("openitg/gameplay-danger-text.png"),
            offset, ..
        } if *offset == [p2_x, y]
    )));
    let game = &mut app.state.screens.gameplay_state.as_mut().unwrap().stage.gameplay;
    let was_hidden = game.profiles()[0].hide_danger;
    game.update_profile(0, |profile| profile.hide_danger = true);
    let (actors, _, _) = app.get_current_actors(0.0, app.frame_policy);
    assert!(!actors.iter().any(|actor| matches!(
        actor,
        Actor::Sprite {
            source: SpriteSource::TextureStatic("openitg/gameplay-danger-text.png"),
            ..
        }
    )));
    let game = &mut app.state.screens.gameplay_state.as_mut().unwrap().stage.gameplay;
    game.update_player(0, |player| player.life = full_life);
    game.update_profile(0, |profile| profile.hide_danger = was_hidden);
}

fn press(app: &mut App, event_loop: &ActiveEventLoop, action: VirtualAction) {
    for pressed in [true, false] {
        let now = Instant::now();
        let event = InputEvent::new(action, 0, pressed, InputSource::Keyboard, now, 0, now, now);
        app.route_input_event(event_loop, event).unwrap();
    }
}

fn settle(app: &mut App, event_loop: &ActiveEventLoop, expected: CurrentScreen) {
    let mut frames = 0;
    let deadline = Instant::now() + Duration::from_secs(5);
    while frames < 100 && Instant::now() < deadline {
        if app.state.screens.current_screen == expected
            && matches!(app.state.shell.transition, TransitionState::Idle)
        {
            return;
        }
        menu_frame(app, event_loop);
        if app.state.screens.current_screen == CurrentScreen::ProfileLoad {
            // Let the asynchronous catalog worker run during simulated frames.
            std::thread::sleep(Duration::from_millis(2));
        } else {
            frames += 1;
        }
    }
    panic!(
        "expected {expected:?}, found {:?} with {:?}",
        app.state.screens.current_screen, app.state.shell.transition
    );
}

fn menu_frame(app: &mut App, event_loop: &ActiveEventLoop) {
    if app.state.screens.current_screen == CurrentScreen::ProfileLoad {
        app.poll_profile_load();
    }
    finish_pending_stage(app);
    deadlib_present::runtime::tick(STEP_SECONDS);
    let transition = app.state.shell.transition.advance_frame(STEP_SECONDS);
    if transition.step_screen && app.state.screens.current_screen != CurrentScreen::Gameplay {
        app.step_idle_screen(STEP_SECONDS, Instant::now(), event_loop);
    }
    match transition.completion {
        Some(TransitionCompletion::ActorFadeOut(target)) => {
            app.finish_actor_fade_out(target, event_loop)
        }
        Some(TransitionCompletion::GlobalFadeOut(target)) => {
            app.on_fade_complete(target, event_loop)
        }
        None => {}
    }
    app.get_current_actors(0.0, app.frame_policy);
}

fn complete_stage(app: &mut App, event_loop: &ActiveEventLoop) {
    let state = app.state.screens.gameplay_state.as_mut().unwrap();
    let columns = [
        VirtualAction::p2_left,
        VirtualAction::p2_down,
        VirtualAction::p2_up,
        VirtualAction::p2_right,
    ];
    let epoch = Instant::now() - Duration::from_secs(30);
    let host_epoch = 1_000_000_000;
    let mut edges = Vec::new();
    for (note, &ns) in state.notes().iter().zip(state.note_time_cache_ns()) {
        edges.push((ns, columns[note.column], true));
        edges.push((ns + 40_000_000, columns[note.column], false));
    }
    edges.sort_by_key(|(ns, _, _)| *ns);
    let mut edges = edges.into_iter().peekable();
    let mut effects = Vec::new();
    for frame in 0..800 {
        let music_nanos = frame * 20_000_000;
        let host = host_epoch + music_nanos as u64;
        while let Some((ns, action, pressed)) = edges.next_if(|(ns, _, _)| *ns <= music_nanos) {
            let instant = epoch + Duration::from_nanos(ns as u64);
            let event = InputEvent::new(
                action,
                0,
                pressed,
                InputSource::Keyboard,
                instant,
                host_epoch + ns as u64,
                instant,
                instant,
            );
            deadsync_theme_simply_love::screens::gameplay::handle_input(
                state,
                &event,
                &mut effects,
            );
        }
        deadsync_theme_simply_love::screens::gameplay::update(
            state,
            0.020,
            GameplayAudioSnapshot {
                stream_clock: GameplayStreamClockSnapshot {
                    music_nanos,
                    music_seconds_per_second: 1.0,
                    valid_at: epoch + Duration::from_nanos(music_nanos as u64),
                    valid_at_host_nanos: host,
                    has_music_mapping: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            || host,
            &mut effects,
        );
        if state.song_completed_naturally() {
            assert_eq!(state.players()[0].judgment_counts.iter().sum::<u32>(), 6);
            let complete = effects
                .iter()
                .find(|effect| {
                    matches!(
                        gameplay_action_for_effect(effect),
                        Some(deadsync_gameplay::GameplayAction::Navigate(
                            deadsync_gameplay::GameplayExit::Complete
                        ))
                    )
                })
                .expect("completed stage queues navigation");
            let action = gameplay_action_for_effect(complete).unwrap();
            app.handle_gameplay_action(action, event_loop);
            return;
        }
        effects.clear();
    }
    panic!("the chart did not finish on the deterministic audio clock");
}
