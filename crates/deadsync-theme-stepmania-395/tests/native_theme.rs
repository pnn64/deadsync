use std::{path::Path, time::Instant};
mod support;

use deadlib_present::actors::{Actor, SizeSpec};
use deadsync_config::{
    coin::CoinMode,
    theme::{GameFlag, ThemeFlag},
};
use deadsync_core::input::InputSource;
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_theme::attract::AttractPresentation;
use deadsync_theme::main_menu::{MainMenuFrame, MainMenuPhase, MainMenuRuntimeView, ThemeMainMenu};
use deadsync_theme::stage::{StageCardMode, StageCardPresentation, StageCardView};
use deadsync_theme::{Screen, ThemeEffect, ThemeResources, TransitionState};
use deadsync_theme_stepmania_395::{StepManiaIntro, StepManiaStageCard, StepManiaTheme};

#[test]
fn wheel_radar_displays_both_players_from_parsed_chart_data() {
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        640.0, 480.0,
    ));
    let actors = support::music_actors();
    let fills: Vec<_> = actors
        .iter()
        .filter_map(|actor| match actor {
            Actor::Mesh { vertices, .. } if vertices.len() == 15 => Some(vertices),
            _ => None,
        })
        .collect();
    assert_eq!(fills.len(), 2);
    assert_eq!(fills[0][0].color, [0.4, 1.0, 0.8, 0.5]);
    assert_eq!(fills[1][0].color, [1.0, 0.5, 0.2, 0.5]);
    assert!(fills[0].iter().any(|vertex| vertex.pos[1] < 310.0));
}

#[test]
fn installed_native_title_uses_bundled_resources_and_shared_navigation() {
    assert_eq!(
        "StepMania 3.95".parse::<ThemeFlag>(),
        Ok(ThemeFlag::StepMania395)
    );
    let manifest = StepManiaTheme::asset_manifest();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for png in manifest.embedded_pngs {
        image::load_from_memory_with_format(png.bytes, image::ImageFormat::Png).unwrap();
    }
    for spec in manifest.fonts {
        let font =
            deadlib_present::font::parse(root.join(spec.ini_path).to_str().unwrap()).unwrap();
        assert!(font.font.glyph_map.contains_key(&'0'));
        assert!(font.font.line_spacing > 0);
        let up = deadlib_present::font::replace_markers("&UP;")
            .chars()
            .next()
            .unwrap();
        assert!(
            font.font.glyph_map.contains_key(&up),
            "{} lost the bundled button glyphs",
            spec.ini_path
        );
    }
    let mut state = StepManiaTheme::init_menu(2);
    StepManiaTheme::sync_menu_runtime(
        &mut state,
        MainMenuRuntimeView {
            game: GameFlag::Pump,
            song_count: 37,
            ..Default::default()
        },
    );
    StepManiaTheme::update_menu::<()>(&mut state, 1.0, MainMenuPhase::Active);
    let mut actors = Vec::new();
    StepManiaTheme::push_menu_actors(
        &mut actors,
        &state,
        MainMenuFrame {
            update_banner_tag: None,
            background_key: None,
            alpha_multiplier: 1.0,
            transition: &TransitionState::Idle,
            visual_policy: (),
        },
    );
    assert!(
        actors
            .iter()
            .any(|actor| matches!(actor, Actor::Sprite { source, .. }
        if source.texture_key() == Some("stepmania-395/title-pump")))
    );
    assert_eq!(
        actors
            .iter()
            .filter(|actor| matches!(
                actor,
                Actor::Text {
                    font: "stepmania-395/title",
                    ..
                }
            ))
            .count(),
        7
    );
    let now = Instant::now();
    let event = |action| InputEvent::new(action, 0, true, InputSource::Keyboard, now, 0, now, now);
    let mut effects = Vec::new();
    StepManiaTheme::handle_menu_input::<()>(&mut state, &event(VirtualAction::p2_start))
        .append_to(&mut effects);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ThemeEffect::Navigate(Screen::SelectProfile)))
    );
    StepManiaTheme::handle_menu_input::<()>(&mut state, &event(VirtualAction::p2_menu_down));
    effects.clear();
    StepManiaTheme::handle_menu_input::<()>(&mut state, &event(VirtualAction::p2_start))
        .append_to(&mut effects);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ThemeEffect::Navigate(Screen::Options)))
    );
    StepManiaTheme::reset_menu(&mut state);
    StepManiaTheme::sync_menu_runtime(
        &mut state,
        MainMenuRuntimeView {
            coin_mode: CoinMode::Pay,
            ..Default::default()
        },
    );
    effects.clear();
    StepManiaTheme::handle_menu_input::<()>(&mut state, &event(VirtualAction::p2_menu_down));
    StepManiaTheme::handle_menu_input::<()>(&mut state, &event(VirtualAction::p2_start))
        .append_to(&mut effects);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ThemeEffect::Navigate(Screen::SelectProfile)))
    );
}

#[test]
fn native_intro_advances_from_company_to_logo_then_title() {
    let mut intro = StepManiaIntro::default();
    assert_eq!(
        intro.media().music,
        Some("assets/media/stepmania-395/company.ogg")
    );
    assert!(!intro.advance(8.0));
    assert_eq!(intro.media().music, None);
    let mut actors = Vec::new();
    intro.push_actors(&mut actors, None);
    assert!(matches!(&actors[0], Actor::Sprite { source, .. }
        if source.texture_key() == Some("stepmania-395/title-background")));
    assert!(intro.advance(10.0));
    intro.reset();
    assert_eq!(intro.playback_time(), 0.0);
}

#[test]
fn native_startup_presents_real_library_progress_and_keeps_the_original_intro() {
    use deadsync_assets::progress::{ContentReloadPhase, LibraryLoadView};
    use deadsync_theme::startup::StartupPresentation;
    use deadsync_theme_stepmania_395::StepManiaStartup;
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        640.0, 480.0,
    ));
    let now = Instant::now();
    let mut progress = LibraryLoadView {
        phase: ContentReloadPhase::Songs,
        done: 47,
        total: 128,
        line2: "Dance Pack".into(),
        line3: "Quartz".into(),
        finished: false,
        can_skip_replaygain: false,
        started_at: now,
        phase_started_at: now,
    };
    let mut startup = StepManiaStartup::default();
    let mut actors = Vec::new();
    startup.push_loading(&mut actors, Some(&progress), &Default::default());
    assert!(
        actors
            .iter()
            .any(|actor| matches!(actor, Actor::Sprite { source, .. }
        if source.texture_key() == Some("stepmania-395/loading-splash")))
    );
    for expected in ["Loading songs (47 of 128)...", "Dance Pack", "Quartz"] {
        assert!(
            actors
                .iter()
                .any(|actor| matches!(actor, Actor::Text { content, .. }
            if content.as_str() == expected)),
            "missing progress: {expected}"
        );
    }
    progress.phase = ContentReloadPhase::ReplayGain;
    progress.can_skip_replaygain = true;
    actors.clear();
    startup.push_loading(&mut actors, Some(&progress), &Default::default());
    assert!(
        actors
            .iter()
            .any(|actor| matches!(actor, Actor::Text { content, .. }
        if content.as_str() == "Press Start or Back to skip"))
    );
    assert!(matches!(startup.advance_intro(8.0), ThemeEffect::None));
    assert!(matches!(
        startup.advance_intro(10.0),
        ThemeEffect::NavigateNoFade(Screen::Menu)
    ));
    startup.reset_intro(0);
    actors.clear();
    startup.push_intro(&mut actors, &Default::default());
    assert!(
        actors
            .iter()
            .any(|actor| matches!(actor, Actor::Sprite { source, .. }
        if source.texture_key() == Some("stepmania-395/company")))
    );
}

#[test]
fn idle_attract_runs_the_tutorial_silent_versus_demo_and_real_chart_rankings() {
    use deadsync_theme::views::{AttractRecord, AttractRecords};
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        640.0, 480.0,
    ));
    let mut intro = StepManiaIntro::default();
    intro.start_idle(AttractRecords {
        songs: vec![AttractRecord {
            title: "Quartz".into(),
            chart_type: "dance-double".into(),
            difficulty: "Hard".into(),
            initials: "ZAP".into(),
            percent: 98.76,
            leaderboard: vec![deadsync_score::LeaderboardEntry {
                rank: 1,
                name: "ZAP".into(),
                machine_tag: None,
                score: 9876.0,
                date: String::new(),
                is_rival: false,
                is_self: false,
                is_fail: false,
            }],
        }],
        ..Default::default()
    });
    assert!(!intro.advance(18.0));
    assert_eq!(
        intro.media().music,
        Some("assets/media/stepmania-395/training.ogg")
    );
    let mut actors = Vec::new();
    intro.push_actors(&mut actors, None);
    assert_eq!(
        actors
            .iter()
            .filter_map(|actor| match actor {
                Actor::TexturedMesh { vertices, .. } => Some(vertices.len()),
                _ => None,
            })
            .sum::<usize>(),
        600,
        "the original pad mesh must be present"
    );
    assert!(!intro.advance(25.0));
    let demo = intro.demonstration().unwrap();
    assert!(demo.two_players);
    assert!(!demo.play_music);
    assert!(!demo.hide_gameplay_hud);
    assert_eq!(demo.chart_type, "dance-single");
    intro.finish_demonstration();
    assert_eq!(
        intro.media().music,
        Some("assets/media/stepmania-395/ranking.ogg")
    );
    assert!(!intro.advance(2.0));
    actors.clear();
    intro.push_actors(&mut actors, None);
    for expected in ["Quartz", "Double", "ZAP", "098.76%", "STEP", "000000000"] {
        assert!(
            actors
                .iter()
                .any(|actor| matches!(actor, Actor::Text { content, .. }
            if content.as_str() == expected)),
            "rankings are missing {expected}"
        );
    }
    assert!(!intro.advance(3.0));
    assert_eq!(
        intro.media().music,
        Some("assets/media/stepmania-395/company.ogg")
    );
    assert!(!intro.advance(73.0));
    assert_eq!(
        intro.media().music,
        Some("assets/media/stepmania-395/ranking.ogg")
    );
    intro.reset();
    assert!(intro.advance(18.0));
}

#[test]
fn stage_card_uses_the_session_mode_and_native_exit_clock() {
    let card = StepManiaStageCard;
    assert_eq!(card.timing().hold.as_secs_f32(), 3.0);
    let mut actors = Vec::new();
    card.push_actors(
        &mut actors,
        StageCardView {
            stage_number: 2,
            mode: StageCardMode::Event,
            elapsed: 1.0,
            exit_elapsed: Some(0.15),
        },
    );
    assert!(actors.iter().any(|actor| matches!(actor, Actor::Sprite { source, tint, .. }
        if source.texture_key() == Some("stepmania-395/stage-event") && (tint[3] - 0.5).abs() < 1e-6)));
    assert!(
        !actors
            .iter()
            .any(|actor| matches!(actor, Actor::Sprite { source, .. }
        if source.texture_key() == Some("stepmania-395/stage-2")))
    );
}

#[test]
fn native_stage_uses_the_captured_numeric_score_and_physical_player_side() {
    use deadlib_present::space::{Metrics, with_current_metrics};
    use deadsync_theme::stage::{StageOutroView, StagePresentation};
    use deadsync_theme::views::{GameplayDifficultyView, GameplayHudMotion, GameplayScoreView};
    use deadsync_theme_stepmania_395::StepManiaStagePresentation;

    let presentation = StepManiaStagePresentation::default();
    assert_eq!(presentation.gameplay_entry_seconds(false), Some(5.0));
    assert_eq!(presentation.gameplay_entry_seconds(true), None);
    let hud = presentation.gameplay_hud().unwrap();
    let mut entry = Vec::new();
    let empty = std::sync::Arc::from("");
    (hud.song_meter)(
        &mut entry,
        deadsync_theme::views::GameplaySongMeterView {
            title: &empty,
            rate: &empty,
            progress: 0.0,
            motion: GameplayHudMotion::Entering {
                elapsed_seconds: 4.0,
            },
        },
    );
    assert!(
        entry
            .iter()
            .any(|actor| matches!(actor, Actor::Sprite { source, tint, .. }
        if source.texture_key() == Some("stepmania-395/gameplay-go") && tint[3] > 0.0))
    );
    for width in [640.0, 854.0] {
        with_current_metrics(Metrics::centered(width, 480.0), || {
            let mut actors = Vec::new();
            (hud.percentage_score)(
                &mut actors,
                GameplayScoreView {
                    p2: true,
                    doubles: false,
                    numeric_score: 7_654_321,
                    percent: 23.45,
                    rgba: [1.0, 1.0, 1.0, 0.65],
                    ex: false,
                    motion: GameplayHudMotion::Settled,
                },
            );
            assert!(
                matches!(&actors[0], Actor::Text { content, font, color, offset, .. }
                if content.as_str() == "007654321" && *font == "stepmania-395/score"
                    && *color == [1.0, 0.5, 0.2, 0.65] && *offset == [534.0 + (width - 640.0) * 0.5, 444.0])
            );
            (hud.difficulty)(
                &mut actors,
                GameplayDifficultyView {
                    p2: true,
                    reverse: true,
                    difficulty: "Hard",
                    description: "",
                    meter: 8,
                    motion: GameplayHudMotion::Settled,
                },
            );
            assert!(
                matches!(&actors[1], Actor::Sprite { offset, uv_rect: Some([0.5, top, 1.0, bottom]), .. }
                if *offset == [580.0 + (width - 640.0) * 0.5, 58.0] && (*top - 0.5).abs() < 1e-6 && (*bottom - 4.0 / 6.0).abs() < 1e-6)
            );
        });
    }
    assert!(
        presentation
            .gameplay_outro(StageOutroView {
                failed: false,
                simfile: Path::new("fixture.ssc"),
                authored_transition: true
            })
            .is_none()
    );
    for failed in [false, true] {
        let outro = presentation
            .gameplay_outro(StageOutroView {
                failed,
                simfile: Path::new("fixture.ssc"),
                authored_transition: false,
            })
            .unwrap();
        assert_eq!(outro.duration(), 5.0);
        assert!(outro.covers_stage(if failed { 1.2 } else { 2.0 }));
        let mut actors = Vec::new();
        outro.push_actors(&mut actors, 2.5);
        let word = if failed {
            "stepmania-395/gameplay-failed"
        } else {
            "stepmania-395/gameplay-cleared"
        };
        assert!(actors.iter().any(|actor| matches!(actor, Actor::Sprite { source, tint, .. } if source.texture_key() == Some(word) && tint[3] > 0.0)));
    }
}

#[test]
fn native_evaluation_keeps_real_scores_records_and_one_banner_for_solo_p2() {
    use deadlib_assets::AssetManager;
    use deadlib_present::space::{Metrics, with_current_metrics};
    use deadsync_theme::stage::StagePresentation;
    use deadsync_theme::views::{EvaluationOverview, EvaluationPlayerOverview, Grade};
    use deadsync_theme_stepmania_395::StepManiaStagePresentation;
    use std::sync::Arc;

    let mut presentation = StepManiaStagePresentation::default();
    let title = Arc::from("Native result fixture");
    let artist = Arc::from("Fixture artist");
    let mods = Arc::from("1.5x");
    let life = Default::default();
    with_current_metrics(Metrics::centered(640.0, 480.0), || {
        let mut actors = Vec::new();
        presentation.evaluation_overview().unwrap().push_overview(
            &mut actors,
            &AssetManager::new(),
            EvaluationOverview {
                title: &title,
                artist: &artist,
                banner_key: None,
                course_summary: false,
                stage_number: Some(2),
                double_side_style: false,
                elapsed: 5.0,
                departure_elapsed: None,
                players: [
                    Some(EvaluationPlayerOverview {
                        p2: true,
                        grade: Grade::Tier08,
                        percent: 97.65,
                        numeric_score: 12_345_678,
                        individual_percent: None,
                        ex_percent: None,
                        hard_ex_percent: None,
                        difficulty: "Hard",
                        meter: 8,
                        mods: &mods,
                        judgments: [321, 123, 17, 4, 2, 1],
                        row_window_counts: Default::default(),
                        score_valid: true,
                        dropped_hold_or_roll: false,
                        jumps: [0; 2],
                        holds: [7, 7],
                        mines: [0; 2],
                        hands: [0; 2],
                        rolls: [2, 2],
                        max_combo: 315,
                        combo_runs: &[],
                        graph_seconds: [0.0, 90.0],
                        course_graph_stages: &[],
                        steps_life: &life,
                        machine_rank: Some(1),
                        personal_rank: Some(2),
                        disqualified: false,
                    }),
                    None,
                ],
            },
        );
        let texture_count = |key| {
            actors.iter().filter(|actor| matches!(actor, Actor::Sprite { source, .. } if source.texture_key() == Some(key))).count()
        };
        assert_eq!(texture_count("stepmania-395/evaluation-banner-frame"), 1);
        assert_eq!(texture_count("stepmania-395/evaluation-grade-p1"), 0);
        assert_eq!(texture_count("stepmania-395/evaluation-grade-p2"), 1);
        assert_eq!(texture_count("stepmania-395/evaluation-machinerecord-1"), 1);
        assert_eq!(
            texture_count("stepmania-395/evaluation-personalrecord-2"),
            1
        );
        assert!(actors.iter().any(
            |actor| matches!(actor, Actor::Text { content, .. } if content.as_str() == "012345678")
        ));
        assert!(actors.iter().any(
            |actor| matches!(actor, Actor::Text { content, .. } if content.as_str() == "0009")
        ));
        assert!(actors.iter().any(|actor| matches!(actor, Actor::Sprite { source, uv_rect: Some([_, top, _, bottom]), .. }
            if source.texture_key() == Some("stepmania-395/evaluation-grades") && (*top - 0.25).abs() < 1e-5 && (*bottom - 0.375).abs() < 1e-5)));
    });
}

#[test]
fn native_wheel_centers_live_rows_and_crops_the_prepared_banner() {
    use deadlib_assets::register_texture_dims;
    use deadlib_present::space::{screen_height, screen_width};
    use deadsync_theme::select_music::{
        SELECT_MUSIC_WHEEL_ROWS, SelectMusicDifficultyRow, SelectMusicPlayerView,
        SelectMusicPrompt, SelectMusicView, SelectMusicWheelRow,
    };
    use deadsync_theme::selection::{MusicWheelPresentation, WheelMediaFrame};
    use deadsync_theme_stepmania_395::StepManiaMusicWheel;
    use std::{sync::Arc, time::Duration};

    let mut wheel = StepManiaMusicWheel::default();
    let now = Instant::now();
    wheel.enter_wheel(now);
    let mut rows = [None; SELECT_MUSIC_WHEEL_ROWS];
    rows[SELECT_MUSIC_WHEEL_ROWS / 2] = Some(SelectMusicWheelRow {
        title: "Native wheel fixture",
        subtitle: "",
        artist: "",
        song_count: None,
        expanded: false,
        action: None,
    });
    let banner: Arc<str> = "native-wheel-test-banner".into();
    register_texture_dims(&banner, 400, 400);
    let view = SelectMusicView {
        rows,
        wheel_offset: 0.0,
        wheel_locked: false,
        selection_beat: 0.0,
        song: None,
        banner_texture: Some(&banner),
        players: std::array::from_fn(|_| SelectMusicPlayerView {
            joined: false,
            name: "",
            chart: None,
            step_index: 0,
            local_records: Default::default(),
        }),
        difficulty_rows: std::array::from_fn(|index| SelectMusicDifficultyRow {
            step_index: index,
            chart: None,
        }),
        stage_number: None,
        music_rate: 1.0,
        prompt: SelectMusicPrompt::Hidden,
    };
    let mut actors = Vec::new();
    wheel.push_music(
        &mut actors,
        view,
        WheelMediaFrame {
            now: now + Duration::from_secs(1),
            outgoing: [None; 4],
            exit_elapsed: 0.0,
        },
    );
    let scale = (screen_width() / 640.0).min(screen_height() / 480.0);
    let center = [
        screen_width() * 0.5 + 40.0 * scale,
        screen_height() * 0.5 - 4.0 * scale,
    ];
    assert!(
        actors
            .iter()
            .any(|actor| matches!(actor, Actor::Text { content, offset, .. }
        if content.as_str() == "Native wheel fixture" && *offset == center))
    );
    assert!(actors.iter().any(|actor| matches!(
        actor,
        Actor::Sprite {
            mask_source: true,
            ..
        }
    )));
    assert!(actors.iter().any(|actor| matches!(actor, Actor::Sprite { source, mask_dest: true, uv_rect: Some([0.0, top, 1.0, bottom]), .. }
        if source.texture_key() == Some(banner.as_ref()) && (*top - (1.0 - 88.0 / 284.0) * 0.5).abs() < 1e-5
            && (*bottom - (1.0 + 88.0 / 284.0) * 0.5).abs() < 1e-5)));
}

#[test]
fn native_options_accept_shared_rows_and_preserve_second_player_focus() {
    use deadlib_assets::AssetManager;
    use deadlib_present::actors::TextContent;
    use deadsync_theme::player_options::{
        PlayerOptionsLayout, PlayerOptionsPage, PlayerOptionsRow,
    };
    use deadsync_theme::system_options::SystemOptionsPresentation;
    use deadsync_theme::views::{
        SystemOptionsPage, SystemOptionsPageKind, SystemOptionsPhase, SystemOptionsRow,
    };
    use deadsync_theme_stepmania_395::{StepManiaPlayerOptions, StepManiaSystemOptions};
    use std::sync::Arc;

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut assets = AssetManager::new();
    for spec in StepManiaTheme::asset_manifest().fonts {
        let parsed =
            deadlib_present::font::parse(root.join(spec.ini_path).to_str().unwrap()).unwrap();
        assets.register_font(spec.name, parsed.font);
    }
    let choices: Arc<[Arc<str>]> = Arc::from([
        "Simply Love".into(),
        "OpenITG".into(),
        "StepMania 3.95".into(),
    ]);
    let row = |_| {
        Some(SystemOptionsRow {
            label: "Theme".into(),
            choices: Some(Arc::clone(&choices)),
            selected_choice: 2,
            inline: true,
            disabled: false,
            enabled_choices: None,
            choice_tint: None,
        })
    };
    let mut actors = Vec::new();
    let mut options = StepManiaSystemOptions;
    options.with_renderer(SystemOptionsPhase::Steady, 1.0, &assets, &mut |renderer| {
        (renderer.background)(&mut actors);
        (renderer.page)(
            &mut actors,
            SystemOptionsPage {
                id: 0,
                title: "System Options",
                kind: SystemOptionsPageKind::Settings,
                selected: 0,
                row_count: 1,
                row: &row,
                description: None,
                detail: None,
                phase: SystemOptionsPhase::Steady,
            },
        );
    });
    assert!(actors.iter().any(
        |actor| matches!(actor, Actor::Text { content, .. } if content.as_str() == "StepMania 3.95")
    ));

    let mut player_options = StepManiaPlayerOptions::default();
    let value = TextContent::from("1.5x");
    let label = TextContent::from("Speed");
    let player_row = |_| {
        Some(PlayerOptionsRow {
            label: &label,
            detail: None,
            slot: 0.0,
            slots: 1,
            alpha: 1.0,
            exit: false,
            values: [None, Some(&value)],
            next_row: [false; 2],
            inline: None,
            has_preview: false,
        })
    };
    actors.clear();
    player_options.push_page(
        &mut actors,
        PlayerOptionsPage {
            title: "Player Options",
            active: [false, true],
            selected_rows: [0; 2],
            scroll_position: [0.0; 2],
            row_count: 1,
            visible_slots: 10,
            alpha: 1.0,
            row: &player_row,
            help: &|_, _| None,
            draw_preview: &|_, _, _, _| {},
        },
        &assets,
        1.0,
        &TransitionState::Idle,
    );
    assert!(actors.iter().any(
        |actor| matches!(actor, Actor::Sprite { source, uv_rect: Some([_, 0.5, _, 1.0]), .. }
        if source.texture_key() == Some("stepmania-395/options-cursor"))
    ));
    assert!(!actors.iter().any(
        |actor| matches!(actor, Actor::Sprite { source, uv_rect: Some([_, 0.0, _, 0.5]), .. }
        if source.texture_key() == Some("stepmania-395/options-cursor"))
    ));
}

#[test]
fn native_selection_uses_the_original_style_and_difficulty_cards() {
    use deadlib_present::space::{Metrics, with_current_metrics};
    use deadsync_theme::selection::{PlaySelectionMode, SelectionPresentation};
    use deadsync_theme::views::{PlaySelectionChoice, PlaySelectionKind, PlaySelectionView};
    use deadsync_theme_stepmania_395::StepManiaMusicWheel;
    use std::sync::Arc;

    let mut selection = StepManiaMusicWheel::default();
    let title = Arc::from("Select Style");
    let prompt = Arc::from("Press Start");
    let names = ["Single", "Versus", "Double"].map(Arc::<str>::from);
    let choices = names.each_ref().map(|label| PlaySelectionChoice {
        label,
        description: None,
        enabled: true,
    });
    for width in [640.0, 854.0] {
        selection.enter_play_selection();
        with_current_metrics(Metrics::centered(width, 480.0), || {
            let mut actors = Vec::new();
            for now in [0.0, 1.0] {
                actors.clear();
                selection.push_play_selection(
                    &mut actors,
                    PlaySelectionView {
                        kind: PlaySelectionKind::Style { pump: false },
                        title: &title,
                        choices: &choices,
                        selected: 2,
                        player_names: [None, Some("P2")],
                        join_prompt: &prompt,
                        exit_progress: 0.0,
                    },
                    now,
                );
            }
            assert!(actors.iter().any(
                |actor| matches!(actor, Actor::Sprite { source, offset, size: [SizeSpec::Px(w), SizeSpec::Px(h)], .. }
                if source.texture_key() == Some("stepmania-395/style-picture-4")
                    && *offset == [160.0 + (width - 640.0) * 0.5, 260.0] && *w == 336.0 && *h == 352.0)
            ));
            assert!(actors.iter().any(|actor| matches!(actor, Actor::Sprite { source, tint, .. }
                if source.texture_key() == Some("stepmania-395/style-icon-3") && *tint == [0.2, 0.2, 0.2, 1.0])));
        });
    }

    let menu = selection.play_mode_menu().unwrap();
    assert_eq!(menu.choices.len(), 8);
    assert_eq!(menu.choices[3].mode, Some(PlaySelectionMode::Regular));
    assert_eq!(menu.choices[3].difficulty_index, Some(3));
    assert_eq!(menu.choices[4].mode, Some(PlaySelectionMode::Course));
    assert!(menu.choices[5..].iter().all(|choice| choice.mode.is_none()));
    let labels: Vec<Arc<str>> = menu
        .choices
        .iter()
        .map(|choice| Arc::from(choice.label))
        .collect();
    let choices: Vec<_> = menu
        .choices
        .iter()
        .zip(&labels)
        .map(|(choice, label)| PlaySelectionChoice {
            label,
            description: None,
            enabled: choice.mode.is_some(),
        })
        .collect();
    selection.enter_play_selection();
    let mut actors = Vec::new();
    for (selected, now) in [(2, 0.0), (4, 2.0)] {
        actors.clear();
        selection.push_play_selection(
            &mut actors,
            PlaySelectionView {
                kind: PlaySelectionKind::Mode,
                title: &title,
                choices: &choices,
                selected,
                player_names: [None, Some("P2")],
                join_prompt: &prompt,
                exit_progress: 0.0,
            },
            now,
        );
    }
    assert!(
        actors
            .iter()
            .any(|actor| matches!(actor, Actor::Sprite { source, .. }
        if source.texture_key() == Some("stepmania-395/mode-picture-5")))
    );
    assert!(actors.iter().any(|actor| matches!(actor, Actor::Sprite { source, tint, .. }
        if source.texture_key() == Some("stepmania-395/mode-picture-6") && *tint == [0.2, 0.2, 0.2, 1.0])));
    assert!(actors.iter().any(
        |actor| matches!(actor, Actor::Sprite { source, uv_rect: Some([0.5, 0.0, 1.0, 1.0]), .. }
        if source.texture_key() == Some("stepmania-395/mode-cursor"))
    ));
}

#[test]
fn native_profile_overlay_and_course_use_shared_live_data() {
    use deadsync_theme::profile_picker::{ProfilePickerPanel, ProfilePickerScreenView};
    use deadsync_theme::select_music::{
        SELECT_MUSIC_WHEEL_ROWS, SelectCourseItem, SelectCourseTrack, SelectCourseView,
        SelectMusicPrompt,
    };
    use deadsync_theme::selection::{
        MusicWheelPresentation, SelectionPresentation, WheelMediaFrame,
    };
    use deadsync_theme_stepmania_395::StepManiaMusicWheel;
    use std::{sync::Arc, time::Duration};
    deadlib_present::space::with_current_metrics(
        deadlib_present::space::Metrics::centered(640.0, 480.0),
        || {
            let mut selection = StepManiaMusicWheel::default();
            let title = Arc::from("Select Profile");
            let join = Arc::from("Press Start");
            let wait = Arc::from("Waiting");
            let name = Arc::from("Ready player");
            let mut actors = Vec::new();
            selection.profile_picker()(
                &mut actors,
                ProfilePickerScreenView {
                    title: &title,
                    join_text: &join,
                    waiting_text: &wait,
                    alpha: 1.0,
                    panels: std::array::from_fn(|seat| ProfilePickerPanel {
                        joined: seat == 0,
                        ready: seat == 0,
                        names: [None, None, Some(&name), None, None],
                        avatar: None,
                        songs_played: None,
                        invalid_choice: false,
                    }),
                },
                true,
            );
            for value in ["Ready player", "Press Start", "Waiting"] {
                assert!(actors.iter().any(
            |actor| matches!(actor, Actor::Text { content, .. } if content.as_str() == value)
        ));
            }
            assert!(actors.iter().all(|actor| match actor {
                Actor::Sprite { z, .. } | Actor::Text { z, .. } => *z >= 1450,
                _ => true,
            }));

            let now = Instant::now();
            selection.enter_wheel(now);
            let meter = Arc::from("8");
            let description = Arc::from("Course fixture");
            actors.clear();
            selection.push_course(
                &mut actors,
                SelectCourseView {
                    rows: [None; SELECT_MUSIC_WHEEL_ROWS],
                    wheel_offset: 0.0,
                    selection_beat: 0.0,
                    banner_texture: None,
                    player_names: [None, Some("P2")],
                    music_rate: 1.0,
                    prompt: SelectMusicPrompt::Hidden,
                    item: Some(SelectCourseItem {
                        title: "Course fixture",
                        author: "",
                        description: &description,
                        songs: "1",
                        bpm: "120",
                        length: "1:30",
                        rating: None,
                        can_change_rating: false,
                        rating_focused: false,
                        tracks: [
                            Some(SelectCourseTrack {
                                number: 7,
                                title: &description,
                                meter: &meter,
                                difficulty: "Hard",
                            }),
                            None,
                            None,
                            None,
                            None,
                            None,
                        ],
                        machine_record: None,
                        player_record: None,
                    }),
                },
                WheelMediaFrame {
                    now: now + Duration::from_secs(1),
                    outgoing: [None; 4],
                    exit_elapsed: 0.0,
                },
            );
            assert!(
        actors
            .iter()
            .any(|actor| matches!(actor, Actor::Sprite { source, size: [SizeSpec::Px(w), SizeSpec::Px(h)], .. }
        if source.texture_key() == Some("stepmania-395/course-track") && *w == 270.0 && *h == 44.0))
    );
            assert!(
                !actors
                    .iter()
                    .any(|actor| matches!(actor, Actor::Sprite { source, .. }
        if source.texture_key() == Some("stepmania-395/music-radar")))
            );
        },
    );
}
