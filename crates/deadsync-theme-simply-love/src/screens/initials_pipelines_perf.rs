use super::*;
use crate::perf::measure;
use crate::pipelines_support::compare;
use deadsync_chart::{ArrowStats, ChartData, SongData, StaminaCounts, TechCounts};
use deadsync_rules::timing::{
    ArrowTimingStats, HistogramMs, ScatterPoint, TimingStats, WindowCounts,
};
use score_data::{ColumnJudgmentList, Grade, GrooveStatsEvalState, ItlEvalState};
use stage_stats::{PlayerStageSummary, StageSummary};
use std::hint::black_box;
use std::path::PathBuf;

type Leaderboards = HashMap<String, Vec<score_data::LeaderboardEntry>>;

fn fixture(rows: usize, count: usize, distinct: bool) -> (Vec<StageSummary>, Leaderboards) {
    let song = test_song("Songs/Pack/test.ssc", "Test", 60.0);
    let mut boards = HashMap::new();
    let stages = (0..count)
        .map(|i| {
            let hash = format!("chart-{}", if distinct { i } else { 0 });
            boards.entry(hash.clone()).or_insert_with(|| {
                (0..rows)
                    .map(|j| score_data::LeaderboardEntry {
                        rank: j as u32 + 1,
                        name: "AAA".into(),
                        score: 10_000.0,
                        date: "2026-10-10".into(),
                        machine_tag: Some("Cabinet Å猫".into()),
                        is_rival: j % 2 == 0,
                        is_self: j % 3 == 0,
                        is_fail: false,
                    })
                    .collect()
            });
            let player = test_player_summary(test_chart(&hash), Grade::Tier01, 500, 500);
            StageSummary {
                song: Arc::clone(&song),
                music_rate: 1.0,
                duration_seconds: 60.0,
                players: [Some(player.clone()), Some(player)],
            }
        })
        .collect();
    (stages, boards)
}

#[test]
fn borrowed_scores_preserve_both_sides_repeated_charts_and_missing_tables() {
    for rows in [0, 1, 5, 64] {
        for count in [0, 1, 6] {
            for distinct in [false, true] {
                let (mut stages, mut boards) = fixture(rows, count, distinct);
                if stages.len() > 1 {
                    stages[1].players[0] = None;
                }
                for entries in boards.values_mut() {
                    if let Some(entry) = entries.get_mut(1) {
                        entry.name = " Å猫 ".into();
                        entry.score = 9999.5;
                    }
                    if let Some(entry) = entries.get_mut(2) {
                        entry.rank = 0;
                        entry.score = f64::NAN;
                    }
                }
                for initials in ["AAA", "", "  ", " Å猫 ", "missing"] {
                    for side in [profile_data::PlayerSide::P1, profile_data::PlayerSide::P2] {
                        let old = pipelines_original::build_side_highscore_lists(
                            side, initials, &stages, &boards,
                        );
                        let new = build_side_highscore_lists(side, initials, &stages, &boards);
                        assert_eq!(format!("{old:?}"), format!("{new:?}"));
                    }
                }
                boards.clear();
                let old = pipelines_original::build_side_highscore_lists(
                    profile_data::PlayerSide::P1,
                    "AAA",
                    &stages,
                    &boards,
                );
                let new = build_side_highscore_lists(
                    profile_data::PlayerSide::P1,
                    "AAA",
                    &stages,
                    &boards,
                );
                assert_eq!(format!("{old:?}"), format!("{new:?}"));
            }
        }
    }
}

#[test]
fn repeated_scores_are_consumed_once_per_side_in_reverse_stage_order() {
    let (stages, boards) = fixture(3, 3, false);
    for side in [profile_data::PlayerSide::P1, profile_data::PlayerSide::P2] {
        let lists = build_side_highscore_lists(side, "AAA", &stages, &boards);
        let ranks: Vec<_> = lists
            .iter()
            .map(|list| {
                list.as_ref()
                    .unwrap()
                    .rows
                    .iter()
                    .find(|row| row.is_highlight)
                    .unwrap()
                    .rank
                    .as_ref()
            })
            .collect();
        assert_eq!(ranks, ["3. ", "2. ", "1. "]);
    }
}

#[test]
fn score_cache_borrows_entries_and_keeps_consumed_flags_on_hits() {
    let (_, boards) = fixture(100, 1, false);
    let hash = String::from("chart-0");
    let mut cache = Vec::with_capacity(1);
    let (entry, churn) = measure(|| find_chart_score_cache(&mut cache, &hash, &boards));
    assert_eq!(entry.entries.as_ptr(), boards[&hash].as_ptr());
    assert_eq!(entry.chart_hash.as_ptr(), hash.as_ptr());
    assert_eq!(churn.allocs, 1); // Only the used-score flags.
    entry.used[5] = true;
    crate::perf::assert_no_churn(|| {
        let entry = find_chart_score_cache(&mut cache, &hash, &boards);
        assert!(entry.used[5]);
    });
}

#[test]
fn complete_highscore_setup_avoids_deep_table_copies() {
    let (stages, boards) = fixture(100, 3, true);
    let (old, before) = measure(|| {
        pipelines_original::build_side_highscore_lists(
            profile_data::PlayerSide::P1,
            "AAA",
            &stages,
            &boards,
        )
    });
    let (new, after) = measure(|| {
        build_side_highscore_lists(profile_data::PlayerSide::P1, "AAA", &stages, &boards)
    });
    assert_eq!(format!("{old:?}"), format!("{new:?}"));
    assert!(before.allocs - after.allocs >= 900);
    assert!(after.allocated_bytes < before.allocated_bytes);
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_string_pipelines_initials() {
    for (label, rows, count, distinct) in [
        ("empty-table", 0, 1, false),
        ("five", 5, 1, false),
        ("hundred", 100, 1, false),
        ("thousand", 1000, 1, false),
        ("repeated", 100, 12, false),
        ("distinct", 100, 12, true),
    ] {
        let (stages, boards) = fixture(rows, count, distinct);
        compare(
            &format!("initials/{label}"),
            || {
                black_box(pipelines_original::build_side_highscore_lists(
                    profile_data::PlayerSide::P1,
                    black_box("AAA"),
                    black_box(&stages),
                    black_box(&boards),
                ));
            },
            || {
                black_box(build_side_highscore_lists(
                    profile_data::PlayerSide::P1,
                    black_box("AAA"),
                    black_box(&stages),
                    black_box(&boards),
                ));
            },
        );
    }
}

fn test_chart(hash: &str) -> Arc<ChartData> {
    Arc::new(ChartData {
        chart_type: "dance-single".to_string(),
        difficulty: "Hard".to_string(),
        description: "Stage Description".to_string(),
        chart_name: "Stage Chart Name".to_string(),
        meter: 9,
        step_artist: "Stage Artist".to_string(),
        music_path: None,
        short_hash: hash.to_string(),
        stats: ArrowStats {
            total_steps: 20,
            ..ArrowStats::default()
        },
        tech_counts: TechCounts::default(),
        mines_nonfake: 0,
        stamina_counts: StaminaCounts::default(),
        total_streams: 0,
        matrix_rating: 0.0,
        matrix_profile: Box::default(),
        max_nps: 0.0,
        sn_detailed_breakdown: String::new(),
        sn_partial_breakdown: String::new(),
        sn_simple_breakdown: String::new(),
        detailed_breakdown: String::new(),
        partial_breakdown: String::new(),
        simple_breakdown: String::new(),
        total_measures: 0,
        measure_nps_vec: Vec::new(),
        measure_seconds_vec: Vec::new(),
        first_second: 0.0,
        has_note_data: true,
        has_chart_attacks: false,
        possible_grade_points: 500,
        holds_total: 2,
        rolls_total: 1,
        mines_total: 3,
        display_bpm: None,
        min_bpm: 0.0,
        max_bpm: 0.0,
    })
}

fn test_song(path: &str, title: &str, seconds: f32) -> Arc<SongData> {
    Arc::new(SongData {
        simfile_path: PathBuf::from(path),
        title: title.to_string(),
        subtitle: String::new(),
        translit_title: title.to_string(),
        translit_subtitle: String::new(),
        artist: String::new(),
        translit_artist: String::new(),
        genre: String::new(),
        banner_path: None,
        background_path: None,
        background_changes: Vec::new(),
        background_layer2_changes: Vec::new(),
        foreground_changes: Vec::new(),
        background_lua_changes: Vec::new(),
        foreground_lua_changes: Vec::new(),
        has_lua: false,
        cdtitle_path: None,
        music_path: None,
        display_bpm: String::new(),
        offset: 0.0,
        sample_start: None,
        sample_length: None,
        min_bpm: 0.0,
        max_bpm: 0.0,
        normalized_bpms: String::new(),
        song_timing: None,
        music_length_seconds: seconds,
        first_second: 0.0,
        total_length_seconds: seconds.round() as i32,
        precise_last_second_seconds: seconds,
        last_second_hint: 0.0,
        charts: Vec::new(),
    })
}

fn test_player_summary(
    chart: Arc<ChartData>,
    grade: Grade,
    earned_grade_points: i32,
    possible_grade_points: i32,
) -> PlayerStageSummary {
    PlayerStageSummary {
        judgment_counts: [0; 6],
        column_judgments: ColumnJudgmentList::new(),
        fail_stream_progress: None,
        profile_name: "P1".to_string(),
        chart,
        score_valid: true,
        disqualified: false,
        groovestats: GrooveStatsEvalState::default(),
        itl: ItlEvalState::default(),
        grade,
        score_percent: 1.0,
        earned_grade_points,
        possible_grade_points,
        ex_score_percent: 100.0,
        hard_ex_score_percent: 100.0,
        hands_achieved: 0,
        hands_total: 0,
        holds_held: 2,
        holds_held_for_score: 2,
        holds_total: 2,
        rolls_held: 1,
        rolls_held_for_score: 1,
        rolls_total: 1,
        mines_hit_for_score: 0,
        mines_avoided: 3,
        mines_total: 3,
        notes_hit: 20,
        calories_burned: 1.25,
        window_counts: WindowCounts {
            w1: 20,
            ..WindowCounts::default()
        },
        window_counts_10ms: WindowCounts {
            w1: 20,
            ..WindowCounts::default()
        },
        timing: TimingStats {
            mean_ms: 10.0,
            mean_abs_ms: 10.0,
            max_abs_ms: 10.0,
            ..TimingStats::default()
        },
        arrow_timing: ArrowTimingStats::default(),
        scatter: vec![ScatterPoint {
            time_sec: 5.0,
            offset_ms: Some(10.0),
            direction_code: 1,
            miss_because_held: false,
            row_index: 0,
            quantization_idx: 0,
            parity_foot: deadsync_rules::timing::ScatterFoot::Unknown,
        }],
        scatter_worst_window_ms: 30.0,
        histogram: HistogramMs {
            bins: vec![(10, 1)],
            smoothed: vec![(10, 1.0)],
            max_count: 1,
            worst_observed_ms: 10.0,
            worst_window_ms: 30.0,
        },
        graph_first_second: 0.0,
        graph_last_second: 60.0,
        life_history: vec![(1.0, 1.0)],
        fail_time: Some(40.0),
        show_w0: true,
        show_ex_score: true,
        show_hard_ex_score: true,
        show_fa_plus_pane: true,
        track_early_judgments: true,
        dim_post_fail_scatter: true,
    }
}
