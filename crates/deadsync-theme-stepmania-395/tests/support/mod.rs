use std::time::{Duration, Instant};

use deadlib_present::actors::Actor;
use deadsync_theme::select_music::{
    SelectMusicDifficultyRow, SelectMusicPlayerView, SelectMusicPrompt, SelectMusicSongView,
    SelectMusicView, SelectMusicWheelRow,
};
use deadsync_theme::selection::{MusicWheelPresentation, WheelMediaFrame};
use deadsync_theme_stepmania_395::StepManiaMusicWheel;

pub fn music_actors() -> Vec<Actor> {
    // A real parsed chart keeps the capture independent of fabricated radar values.
    let source = b"#TITLE:Radar Fixture;\n#ARTIST:Fixture Artist;\n#BPMS:0=120;\n\
        #NOTES:dance-single::Hard:8:0,0,0,0,0:\n\
        1200\n0010\n0300\n0001\n1100\n0010\n0101\n0001\n,\n\
        1000\n0100\n0010\n0001\n1000\n0100\n0010\n0001\n\
        1000\n0100\n0010\n0001\n1000\n0100\n0010\n0001\n;\n";
    let mut parsed = deadsync_simfile::song::parse_song_bytes(
        source,
        "sm",
        std::path::Path::new("tests/radar-fixture.sm"),
        &deadsync_simfile::song::ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
        |_| 8.0,
    )
    .unwrap();
    parsed.music_length_seconds = 8.0;
    let song = deadsync_simfile::cache::build_song_meta(parsed, 0.0);
    let chart: &deadsync_chart::ChartData = &song.charts[0];
    assert!(
        chart
            .radar_values(song.music_length_seconds)
            .iter()
            .all(|value| *value > 0.0)
    );
    let now = Instant::now();
    let mut wheel = StepManiaMusicWheel::default();
    wheel.enter_wheel(now - Duration::from_secs(3));
    let mut actors = Vec::new();
    wheel.push_music(
        &mut actors,
        SelectMusicView {
            rows: std::array::from_fn(|index| {
                Some(SelectMusicWheelRow {
                    title: if index.is_multiple_of(3) {
                        "Another Song"
                    } else if index % 3 == 1 {
                        "Third Song"
                    } else {
                        &song.title
                    },
                    subtitle: "",
                    artist: "Fixture Artist",
                    song_count: None,
                    expanded: false,
                    action: None,
                })
            }),
            wheel_offset: 0.0,
            wheel_locked: false,
            sort: deadsync_theme::select_music::SelectMusicSortView::Title,
            selection_beat: 1.0,
            song: Some(SelectMusicSongView {
                title: &song.title,
                subtitle: "",
                artist: &song.artist,
                display_bpm: "120",
                music_length_seconds: song.music_length_seconds,
            }),
            banner_texture: None,
            players: std::array::from_fn(|side| SelectMusicPlayerView {
                joined: true,
                name: if side == 0 { "P1" } else { "P2" },
                chart: Some(chart),
                step_index: 3,
                local_records: Default::default(),
            }),
            difficulty_rows: std::array::from_fn(|index| SelectMusicDifficultyRow {
                step_index: index,
                chart: (index == 3).then_some(chart),
            }),
            stage_number: Some(1),
            music_rate: 1.0,
            prompt: SelectMusicPrompt::Hidden,
        },
        WheelMediaFrame {
            now,
            outgoing: [None; 4],
            exit_elapsed: 0.0,
        },
    );
    actors
}

pub fn evaluation_actors(course: bool, p2_only: bool, elapsed: f32) -> Vec<Actor> {
    use deadlib_assets::AssetManager;
    use deadsync_theme::stage::StagePresentation;
    use deadsync_theme::views::{EvaluationOverview, EvaluationPlayerOverview, Grade};
    use deadsync_theme_stepmania_395::StepManiaStagePresentation;
    use std::sync::Arc;

    let title = Arc::from("Native result fixture");
    let artist = Arc::from("Fixture artist");
    let mods = Arc::from("1.5x");
    let life = Default::default();
    let mut actors = Vec::new();
    StepManiaStagePresentation::default()
        .evaluation_overview()
        .unwrap()
        .push_overview(
            &mut actors,
            &AssetManager::new(),
            EvaluationOverview {
                title: &title,
                artist: &artist,
                banner_key: None,
                course_summary: course,
                stage_number: (!course).then_some(2),
                double_side_style: false,
                elapsed,
                departure_elapsed: None,
                players: std::array::from_fn(|side| {
                    if p2_only && side == 0 {
                        return None;
                    }
                    Some(EvaluationPlayerOverview {
                        p2: side == 1,
                        grade: Grade::Tier08,
                        percent: 97.65,
                        numeric_score: 12_345_678,
                        radar_possible: [0.8, 0.7, 0.6, 0.5, 0.4],
                        radar_achievement: (!course).then_some([0.9, 0.8, 0.7, 1.0, 0.95]),
                        stages_passed: if course { 3 } else { 1 },
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
                    })
                }),
            },
        );
    actors
}

pub fn transition_actors(arriving: bool, elapsed: f32) -> (Vec<Actor>, f32) {
    use deadsync_theme::{Screen, ThemeTransitions};
    use deadsync_theme_stepmania_395::StepManiaTheme;
    let actors = || {
        if arriving {
            StepManiaTheme::in_transition(Screen::SelectMusic, ())
        } else {
            StepManiaTheme::out_transition(Screen::SelectMusic, Screen::Gameplay)
        }
    };
    deadlib_present::runtime::clear_all();
    let _ = actors();
    deadlib_present::runtime::tick(elapsed);
    actors()
}

pub fn title_actors(elapsed: f32) -> Vec<Actor> {
    use deadsync_theme::main_menu::{MainMenuFrame, MainMenuPhase, ThemeMainMenu};
    use deadsync_theme::{ThemeEffect, TransitionState};
    use deadsync_theme_stepmania_395::StepManiaTheme;
    let mut state = StepManiaTheme::init_menu(0);
    let _: ThemeEffect<_, ()> =
        StepManiaTheme::update_menu(&mut state, elapsed, MainMenuPhase::Active);
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
    actors
}
