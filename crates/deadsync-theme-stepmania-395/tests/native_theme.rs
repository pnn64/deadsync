use std::{path::Path, time::Instant};

use deadlib_present::actors::Actor;
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
