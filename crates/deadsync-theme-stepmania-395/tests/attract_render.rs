//! Manual GPU captures of native presentation, without opening a visible window.
#![cfg(all(
    target_os = "windows",
    not(target_pointer_width = "32"),
    not(target_vendor = "win7")
))]

use std::{path::PathBuf, sync::Arc};
mod support;

use deadlib_present::{
    actors::Actor,
    compose,
    space::{self, Metrics},
};
use deadlib_render::{BackendType, PresentModePolicy, create_backend};
use deadsync_theme::startup::StartupPresentation;
use deadsync_theme::{
    ThemeResources,
    attract::AttractPresentation,
    views::{AttractRecord, AttractRecords},
};
use deadsync_theme_stepmania_395::StepManiaStartup;
use deadsync_theme_stepmania_395::{StepManiaIntro, StepManiaTheme};
use winit::{
    dpi::PhysicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};

type RenderAssets = deadlib_assets::AssetManager<deadlib_render::Texture>;

#[test]
#[ignore = "requires a graphics device; writes reference captures under target"]
fn native_attract_captures() {
    let events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[expect(deprecated, reason = "hidden renderer fixture needs no event dispatch")]
    let window = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(640, 480)),
            )
            .unwrap(),
    );
    let metrics = Metrics::centered(640.0, 480.0);
    space::set_current_metrics(metrics);
    space::set_current_window_px(640, 480);
    let mut backend = create_backend(
        BackendType::VulkanWgpu,
        window,
        metrics.projection(),
        false,
        PresentModePolicy::Immediate,
        false,
        true,
    )
    .unwrap();
    let mut assets = RenderAssets::new();
    assets.load_textures(&mut backend, Vec::new()).unwrap();
    let manifest = StepManiaTheme::asset_manifest();
    for &png in manifest.embedded_pngs {
        assets.load_embedded_png(&mut backend, png).unwrap();
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let roots = [root.join("assets")];
    for parsed in
        deadlib_assets::parse_font_asset_specs(manifest.fonts.iter().copied(), &roots, |path| {
            root.join(path)
        })
        .unwrap()
    {
        for texture in deadlib_assets::prepare_required_font_textures(
            &parsed.font,
            &parsed.required_textures,
            &roots,
            |key| assets.has_texture_key(key),
        )
        .unwrap()
        {
            assets
                .update_texture_for_key_with_sampler(
                    &mut backend,
                    &texture.key,
                    &texture.image,
                    texture.hints.sampler_desc(),
                )
                .unwrap();
        }
        assets.register_font(parsed.name, parsed.font);
    }
    let output = root.join("target/native-395-attract-captures");
    std::fs::create_dir_all(&output).unwrap();
    for (name, actors) in [
        ("select-style", selection_actors(false, 0)),
        ("select-difficulty", selection_actors(true, 2)),
        ("select-nonstop", selection_actors(true, 4)),
        ("select-course", course_actors()),
        ("select-profile", profile_actors(false)),
        ("select-profile-overlay", profile_actors(true)),
        ("stage-intro", stage_actors(false, 0.35)),
        ("stage-nonstop", stage_actors(true, 1.0)),
        ("system-options", options_actors(&assets, None)),
        (
            "player-options",
            options_actors(&assets, Some([true, true])),
        ),
        (
            "player-options-p2",
            options_actors(&assets, Some([false, true])),
        ),
        ("gameplay-hud", gameplay_hud_actors()),
        (
            "gameplay-entry-wipe",
            gameplay_hud_motion_actors(deadsync_theme::views::GameplayHudMotion::Entering {
                elapsed_seconds: 1.0,
            }),
        ),
        (
            "gameplay-ready",
            gameplay_hud_motion_actors(deadsync_theme::views::GameplayHudMotion::Entering {
                elapsed_seconds: 2.25,
            }),
        ),
        (
            "gameplay-go",
            gameplay_hud_motion_actors(deadsync_theme::views::GameplayHudMotion::Entering {
                elapsed_seconds: 3.75,
            }),
        ),
        ("gameplay-cleared-wipe", gameplay_outro_actors(false, 0.75)),
        ("gameplay-cleared", gameplay_outro_actors(false, 2.5)),
        ("gameplay-failed-wipe", gameplay_outro_actors(true, 0.25)),
        ("gameplay-failed", gameplay_outro_actors(true, 2.5)),
        ("gameplay-danger", gameplay_warning_actors(0.25)),
        ("gameplay-danger-blink-off", gameplay_warning_actors(0.75)),
    ] {
        capture_actors(name, &actors, 2.0, &output, &metrics, &mut backend, &assets);
    }
    let progress = deadsync_assets::progress::LibraryLoadView {
        phase: deadsync_assets::progress::ContentReloadPhase::Songs,
        done: 47,
        total: 128,
        line2: "Dance Pack".into(),
        line3: "Quartz".into(),
        finished: false,
        can_skip_replaygain: false,
        started_at: std::time::Instant::now(),
        phase_started_at: std::time::Instant::now(),
    };
    let mut loading = Vec::new();
    StepManiaStartup::default().push_loading(&mut loading, Some(&progress), &());
    capture_actors(
        "loading",
        &loading,
        0.0,
        &output,
        &metrics,
        &mut backend,
        &assets,
    );
    let mut intro = StepManiaIntro::default();
    let p2_double = deadsync_theme::select_music::SessionStyleView {
        double: true,
        versus: false,
        primary_side: 1,
    };
    for (name, actors, elapsed) in [
        (
            "select-difficulty-p2-double",
            selection_actors_with_style(true, 3, p2_double),
            2.0,
        ),
        (
            "evaluation-double-p2",
            support::evaluation_actors_with_style(false, true, 5.0, p2_double),
            5.0,
        ),
        (
            "evaluation-header-entering",
            support::evaluation_actors(false, false, 0.25),
            0.25,
        ),
    ] {
        capture_actors(
            name,
            &actors,
            elapsed,
            &output,
            &metrics,
            &mut backend,
            &assets,
        );
    }
    for (name, screen, elapsed, p2) in [
        (
            "session-summary",
            deadsync_theme::Screen::EvaluationSummary,
            5.0,
            false,
        ),
        (
            "session-summary-p2",
            deadsync_theme::Screen::EvaluationSummary,
            5.0,
            true,
        ),
        ("name-entry", deadsync_theme::Screen::Initials, 2.0, false),
        ("name-entry-p2", deadsync_theme::Screen::Initials, 2.0, true),
        ("game-over", deadsync_theme::Screen::GameOver, 1.0, false),
    ] {
        capture_actors(
            name,
            &support::session_actors(screen, elapsed, p2),
            elapsed,
            &output,
            &metrics,
            &mut backend,
            &assets,
        );
    }
    for (name, elapsed) in [("title-delayed-logo", 0.25), ("title-revealed-logo", 1.0)] {
        capture_actors(
            name,
            &support::title_actors(elapsed),
            elapsed,
            &output,
            &metrics,
            &mut backend,
            &assets,
        );
    }
    for (name, mode, credits, remainder, price) in [
        ("title-free", deadsync_core::session_flow::CoinMode::Free, 0, 0, 1),
        ("title-pay-partial", deadsync_core::session_flow::CoinMode::Pay, 0, 1, 3),
        ("title-pay-credit", deadsync_core::session_flow::CoinMode::Pay, 2, 1, 3),
    ] {
        let actors = support::title_actors_with_runtime(1.5,
            deadsync_theme::main_menu::MainMenuRuntimeView {
                coin_mode: mode,
                credits,
                credit_remainder: remainder,
                coins_per_credit: price,
                can_start: mode != deadsync_core::session_flow::CoinMode::Pay || credits > 0,
                song_count: 37,
                pack_count: 4,
                course_count: 3,
                ..Default::default()
            });
        capture_actors(name, &actors, 1.5, &output, &metrics, &mut backend, &assets);
    }
    capture_actors(
        "select-music",
        &support::music_actors(),
        3.0,
        &output,
        &metrics,
        &mut backend,
        &assets,
    );
    for (name, range, style, elapsed) in [
        ("select-music-variable-bpm", Some((135.0, 175.0)), true, 1.5),
        ("select-music-solo-p2", Some((120.0, 120.0)), false, 3.0),
    ] {
        let session_style = deadsync_theme::select_music::SessionStyleView {
            double: false,
            versus: style,
            primary_side: 1,
        };
        capture_actors(
            name,
            &support::music_actors_for(session_style, range, elapsed),
            elapsed,
            &output,
            &metrics,
            &mut backend,
            &assets,
        );
    }
    for (name, delta) in [("company", 3.0), ("logo", 9.0)] {
        intro.advance(delta);
        capture(name, &intro, &output, &metrics, &mut backend, &assets);
    }
    for (name, arriving, elapsed) in [
        ("music-transition-in", true, 0.25),
        ("music-transition-out", false, 0.15),
    ] {
        let mut actors = support::music_actors();
        actors.extend(support::transition_actors(arriving, elapsed).0);
        capture_actors(
            name,
            &actors,
            elapsed,
            &output,
            &metrics,
            &mut backend,
            &assets,
        );
    }
    for (name, course, p2_only, elapsed) in [
        ("evaluation-stage", false, false, 5.0),
        ("evaluation-stage-entering", false, false, 0.9),
        ("evaluation-solo-p2", false, true, 5.0),
        ("evaluation-nonstop", true, false, 5.0),
    ] {
        capture_actors(
            name,
            &support::evaluation_actors(course, p2_only, elapsed),
            elapsed,
            &output,
            &metrics,
            &mut backend,
            &assets,
        );
    }
    intro.start_idle(AttractRecords::default());
    intro.advance(18.0);
    for (name, delta) in [
        ("tutorial-title", 0.4),
        ("tutorial-pad", 6.6),
        ("tutorial-down", 3.0),
        ("tutorial-jump", 9.5),
        ("tutorial-miss", 4.0),
    ] {
        intro.advance(delta);
        capture(name, &intro, &output, &metrics, &mut backend, &assets);
    }
    intro.start_idle(records());
    intro.advance(43.0);
    capture(
        "demonstration-overlay",
        &intro,
        &output,
        &metrics,
        &mut backend,
        &assets,
    );
    intro.finish_demonstration();
    intro.advance(2.0);
    capture("ranking", &intro, &output, &metrics, &mut backend, &assets);
    intro.advance(3.0);
    capture("warning", &intro, &output, &metrics, &mut backend, &assets);
}

fn records() -> AttractRecords {
    AttractRecords {
        songs: vec![AttractRecord {
            title: "Quartz".into(),
            chart_type: "dance-single".into(),
            difficulty: "Hard".into(),
            initials: "AAA".into(),
            percent: 98.76,
            leaderboard: ["AAA", "BBB", "CCC"]
                .into_iter()
                .enumerate()
                .map(|(rank, name)| deadsync_score::LeaderboardEntry {
                    rank: rank as u32 + 1,
                    name: name.into(),
                    machine_tag: None,
                    score: 9876.0 - rank as f64 * 120.0,
                    date: String::new(),
                    is_rival: false,
                    is_self: false,
                    is_fail: false,
                })
                .collect(),
        }],
        ..Default::default()
    }
}

fn capture(
    name: &str,
    intro: &StepManiaIntro,
    output: &std::path::Path,
    metrics: &Metrics,
    backend: &mut deadlib_render::Backend,
    assets: &RenderAssets,
) {
    let mut actors: Vec<Actor> = Vec::new();
    intro.push_actors(&mut actors, None);
    capture_actors(
        name,
        &actors,
        intro.playback_time(),
        output,
        metrics,
        backend,
        assets,
    );
}

fn capture_actors(
    name: &str,
    actors: &[Actor],
    elapsed: f32,
    output: &std::path::Path,
    metrics: &Metrics,
    backend: &mut deadlib_render::Backend,
    assets: &RenderAssets,
) {
    let frame = compose::build_screen_with_texture_context(
        actors,
        [0.0, 0.0, 0.0, 1.0],
        metrics,
        assets.fonts(),
        elapsed,
        assets.texture_context(),
    );
    assert!(!frame.ops.is_empty(), "{name} has no rendered actors");
    backend.request_screenshot();
    backend.draw(&frame, assets.textures(), false).unwrap();
    let image = backend.capture_frame().unwrap();
    assert_eq!(image.dimensions(), (640, 480));
    image.save(output.join(format!("{name}.png"))).unwrap();
}

fn selection_actors(mode: bool, selected: usize) -> Vec<Actor> {
    selection_actors_with_style(
        mode,
        selected,
        deadsync_theme::select_music::SessionStyleView {
            double: false,
            versus: true,
            primary_side: 0,
        },
    )
}

fn selection_actors_with_style(
    mode: bool,
    selected: usize,
    session_style: deadsync_theme::select_music::SessionStyleView,
) -> Vec<Actor> {
    use deadsync_theme::selection::SelectionPresentation;
    use deadsync_theme::views::{PlaySelectionChoice, PlaySelectionKind, PlaySelectionView};
    use deadsync_theme_stepmania_395::StepManiaMusicWheel;
    use std::sync::Arc;

    let mut selection = StepManiaMusicWheel::default();
    let title = Arc::from(if mode {
        "Select Difficulty"
    } else {
        "Select Style"
    });
    let prompt = Arc::from("Press Start");
    let labels: Vec<Arc<str>> = if mode {
        selection
            .play_mode_menu()
            .unwrap()
            .choices
            .iter()
            .map(|choice| choice.label.into())
            .collect()
    } else {
        ["Single", "Versus", "Double"].map(Arc::from).into()
    };
    let choices: Vec<_> = labels
        .iter()
        .enumerate()
        .map(|(index, label)| PlaySelectionChoice {
            label,
            description: None,
            enabled: !mode || index < 5,
        })
        .collect();
    let mut actors = Vec::new();
    for now in [0.0, 2.0] {
        actors.clear();
        selection.push_play_selection(
            &mut actors,
            PlaySelectionView {
                session_style,
                kind: if mode {
                    PlaySelectionKind::Mode
                } else {
                    PlaySelectionKind::Style { pump: false }
                },
                title: &title,
                choices: &choices,
                selected,
                player_names: std::array::from_fn(|side| {
                    (session_style.versus || side == session_style.primary_side)
                        .then_some(if side == 0 { "P1" } else { "P2" })
                }),
                join_prompt: &prompt,
                exit_progress: 0.0,
            },
            now,
        );
    }
    actors
}

fn options_actors(assets: &RenderAssets, players: Option<[bool; 2]>) -> Vec<Actor> {
    use deadlib_present::actors::TextContent;
    use deadsync_theme::player_options::{
        PlayerOptionsInline, PlayerOptionsLayout, PlayerOptionsPage, PlayerOptionsRow,
    };
    use deadsync_theme::system_options::SystemOptionsPresentation;
    use deadsync_theme::views::{
        SystemOptionsPage, SystemOptionsPageKind, SystemOptionsPhase, SystemOptionsRow,
    };
    use deadsync_theme_stepmania_395::{StepManiaPlayerOptions, StepManiaSystemOptions};
    use std::sync::Arc;

    let mut actors = Vec::new();
    if let Some(active) = players {
        let labels = [
            "Speed",
            "Accel",
            "Effect",
            "Appearance",
            "Turn",
            "Scroll",
            "NoteSkin",
            "Holds",
            "EXIT",
        ]
        .map(TextContent::from);
        let choices: Vec<Vec<TextContent>> = [
            &["1x", "1.5x", "2x", "3x"][..],
            &["Off", "Boost", "Brake"],
            &["Off", "Drunk", "Dizzy"],
            &["Visible", "Hidden", "Sudden"],
            &["None", "Mirror", "Left", "Right"],
            &["Normal", "Reverse"],
            &["default"],
            &["On", "Off"],
            &[],
        ]
        .into_iter()
        .map(|choices| {
            choices
                .iter()
                .map(|value| TextContent::from(*value))
                .collect()
        })
        .collect();
        let row = |index: usize| {
            Some(PlayerOptionsRow {
                label: &labels[index],
                detail: None,
                slot: index as f32,
                slots: 1,
                alpha: 1.0,
                exit: index == labels.len() - 1,
                values: [None; 2],
                next_row: [false; 2],
                inline: Some(PlayerOptionsInline {
                    choices: &choices[index],
                    focused: [Some(0), Some(0)],
                    selected: [0; 2],
                    enabled: [None; 2],
                }),
                has_preview: false,
            })
        };
        let mut presentation = StepManiaPlayerOptions::default();
        for now in [0.0, 2.0] {
            actors.clear();
            presentation.push_page(
                &mut actors,
                PlayerOptionsPage {
                    title: "Player Options",
                    active,
                    selected_rows: [0, 1],
                    scroll_position: [0.0, 1.0],
                    row_count: labels.len(),
                    visible_slots: 10,
                    alpha: 1.0,
                    row: &row,
                    help: &|_, _| None,
                    draw_preview: &|_, _, _, _| {},
                },
                assets,
                now,
                &deadsync_theme::TransitionState::Idle,
            );
        }
    } else {
        let rows = [
            (
                "Theme",
                &["Simply Love", "OpenITG", "StepMania 3.95"][..],
                2,
            ),
            ("Display Mode", &["Windowed", "Fullscreen"], 0),
            ("Resolution", &["1600x900"], 0),
            ("Vsync", &["Off", "On"], 0),
            ("Renderer", &["Vulkan", "OpenGL", "Software"], 0),
            ("Sound Device", &["Auto"], 0),
        ];
        let row = |index: usize| {
            rows.get(index)
                .map(|(label, values, selected)| SystemOptionsRow {
                    label: Arc::from(*label),
                    choices: Some(values.iter().map(|value| Arc::from(*value)).collect()),
                    selected_choice: *selected,
                    inline: true,
                    disabled: false,
                    enabled_choices: None,
                    choice_tint: None,
                })
        };
        StepManiaSystemOptions.with_renderer(
            SystemOptionsPhase::Steady,
            2.0,
            assets,
            &mut |renderer| {
                (renderer.background)(&mut actors);
                (renderer.page)(
                    &mut actors,
                    SystemOptionsPage {
                        id: 0,
                        title: "System Options",
                        kind: SystemOptionsPageKind::Settings,
                        selected: 0,
                        row_count: rows.len(),
                        row: &row,
                        description: Some("Select the installed presentation theme.".into()),
                        detail: None,
                        phase: SystemOptionsPhase::Steady,
                    },
                );
            },
        );
    }
    actors
}

fn gameplay_hud_actors() -> Vec<Actor> {
    gameplay_hud_motion_actors(deadsync_theme::views::GameplayHudMotion::Settled)
}

fn gameplay_hud_motion_actors(motion: deadsync_theme::views::GameplayHudMotion) -> Vec<Actor> {
    use deadsync_theme::stage::StagePresentation;
    use deadsync_theme::views::{
        GameplayDifficultyView, GameplayLifeView, GameplayScoreView, GameplaySongMeterView,
        GameplayStageBadgeView,
    };
    use deadsync_theme_stepmania_395::StepManiaStagePresentation;
    use std::sync::Arc;

    let hud = StepManiaStagePresentation::default()
        .gameplay_hud()
        .unwrap();
    let mut actors = Vec::new();
    (hud.fallback_background)(&mut actors, 1.0);
    let title = Arc::from("Quartz");
    let rate = Arc::from("");
    (hud.song_meter)(
        &mut actors,
        GameplaySongMeterView {
            title: &title,
            rate: &rate,
            progress: 0.3,
            motion,
        },
    );
    (hud.stage_badge)(
        &mut actors,
        GameplayStageBadgeView {
            text: &rate,
            round: Some(1),
            motion,
        },
    );
    for p2 in [false, true] {
        (hud.standard_life)(
            &mut actors,
            GameplayLifeView {
                p2,
                level: if p2 { 0.6 } else { 0.8 },
                failed: false,
                hot: false,
                song_beat: 20.0,
                percent_label: None,
                motion,
            },
        );
        (hud.percentage_score)(
            &mut actors,
            GameplayScoreView {
                p2,
                doubles: false,
                numeric_score: 12_345_678,
                percent: 97.65,
                rgba: [1.0; 4],
                ex: false,
                motion,
            },
        );
        (hud.difficulty)(
            &mut actors,
            GameplayDifficultyView {
                p2,
                reverse: false,
                difficulty: "Hard",
                description: "",
                meter: 8,
                motion,
            },
        );
    }
    actors
}

fn gameplay_outro_actors(failed: bool, elapsed: f32) -> Vec<Actor> {
    use deadsync_theme::stage::{StageOutroView, StagePresentation};
    use deadsync_theme::views::GameplayHudMotion;
    use deadsync_theme_stepmania_395::StepManiaStagePresentation;
    let outro = StepManiaStagePresentation::default()
        .gameplay_outro(StageOutroView {
            failed,
            simfile: std::path::Path::new("fixture.ssc"),
            authored_transition: false,
        })
        .unwrap();
    let mut actors = if outro.covers_stage(elapsed) {
        Vec::new()
    } else {
        gameplay_hud_motion_actors(GameplayHudMotion::Leaving {
            elapsed_seconds: elapsed,
        })
    };
    outro.push_actors(&mut actors, elapsed);
    actors
}

fn gameplay_warning_actors(elapsed: f32) -> Vec<Actor> {
    use deadsync_theme::{stage::StagePresentation, views::GameplayBackgroundWarningView};
    use deadsync_theme_stepmania_395::StepManiaStagePresentation;
    let mut actors = gameplay_hud_actors();
    let hud = StepManiaStagePresentation::default()
        .gameplay_hud()
        .unwrap();
    (hud.background_warning)(
        &mut actors,
        GameplayBackgroundWarningView {
            p2: false,
            both_sides: true,
            failed: false,
            danger_seconds: elapsed,
            brightness: 1.0,
        },
    );
    actors
}

fn course_actors() -> Vec<Actor> {
    use deadsync_theme::select_music::{
        SELECT_MUSIC_WHEEL_ROWS, SelectCourseItem, SelectCourseTrack, SelectCourseView,
        SelectMusicPrompt, SelectMusicWheelRow,
    };
    use deadsync_theme::selection::{
        MusicWheelPresentation, SelectionPresentation, WheelMediaFrame,
    };
    use deadsync_theme_stepmania_395::StepManiaMusicWheel;
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut selection = StepManiaMusicWheel::default();
    selection.enter_wheel(now - Duration::from_secs(3));
    let titles: [Arc<str>; 5] = [
        "BRILLIANT 2U",
        "B4U",
        "BURNIN' THE FLOOR",
        "DYNAMITE RAVE",
        "CELEBRATE NITE",
    ]
    .map(Arc::from);
    let meters = ["6", "7", "7", "8", "8"].map(Arc::from);
    let subtitles: [Arc<str>; 5] = ["ORCHESTRA GROOVE", "", "", "", ""].map(Arc::from);
    let artist = Arc::from("NAOKI");
    let description = Arc::from("Five song Nonstop course");
    let mut actors = Vec::new();
    selection.push_course(
        &mut actors,
        SelectCourseView {
            session_style: deadsync_theme::select_music::SessionStyleView {
                double: false,
                versus: true,
                primary_side: 0,
            },
            rows: std::array::from_fn::<_, SELECT_MUSIC_WHEEL_ROWS, _>(|index| {
                Some(SelectMusicWheelRow {
                    title: ["NAOKI STANDARD", "PARANOIA BROTHERS", "PLAYER'S BEST"][index % 3],
                    subtitle: "",
                    artist: "",
                    song_count: None,
                    expanded: false,
                    action: None,
                })
            }),
            wheel_offset: 0.0,
            selection_beat: 1.0,
            banner_texture: None,
            player_names: [None, Some("P2")],
            music_rate: 1.0,
            prompt: SelectMusicPrompt::Hidden,
            item: Some(SelectCourseItem {
                title: "NAOKI STANDARD",
                author: "",
                description: &description,
                songs: "5",
                bpm: "125–160",
                length: "7:30",
                rating: None,
                can_change_rating: false,
                rating_focused: false,
                tracks: std::array::from_fn(|index| {
                    (index < 5).then(|| SelectCourseTrack {
                        number: index + 1,
                        title: &titles[index],
                        banner_title: &titles[index],
                        subtitle: &subtitles[index],
                        artist: &artist,
                        meter: &meters[index],
                        difficulty: "Hard",
                    })
                }),
                machine_record: None,
                player_record: None,
            }),
        },
        WheelMediaFrame {
            now,
            outgoing: [None; 4],
            exit_elapsed: 0.0,
        },
    );
    actors
}

fn profile_actors(overlay: bool) -> Vec<Actor> {
    use deadsync_theme::profile_picker::{ProfilePickerPanel, ProfilePickerScreenView};
    use deadsync_theme::selection::SelectionPresentation;
    use deadsync_theme_stepmania_395::StepManiaMusicWheel;
    let name = Arc::from("Ready Player");
    let title = Arc::from("Select Profile");
    let join = Arc::from("Press Start");
    let waiting = Arc::from("Waiting");
    let mut actors = if overlay {
        support::music_actors()
    } else {
        Vec::new()
    };
    StepManiaMusicWheel::default().profile_picker()(
        &mut actors,
        ProfilePickerScreenView {
            title: &title,
            join_text: &join,
            waiting_text: &waiting,
            alpha: 1.0,
            panels: std::array::from_fn(|side| ProfilePickerPanel {
                joined: side == 1,
                ready: side == 1,
                names: [None, None, Some(&name), None, None],
                avatar: None,
                songs_played: None,
                invalid_choice: false,
            }),
        },
        overlay,
    );
    actors
}

fn stage_actors(course: bool, elapsed: f32) -> Vec<Actor> {
    use deadsync_theme::stage::{StageCardMode, StageCardPresentation, StageCardView};
    use deadsync_theme_stepmania_395::StepManiaStageCard;
    let mut actors = Vec::new();
    StepManiaStageCard.push_actors(
        &mut actors,
        StageCardView {
            stage_number: 1,
            mode: if course {
                StageCardMode::Nonstop
            } else {
                StageCardMode::Regular
            },
            elapsed,
            exit_elapsed: None,
        },
    );
    actors
}
