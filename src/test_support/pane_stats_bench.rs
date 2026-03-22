use crate::assets::AssetManager;
use crate::game::chart::{ChartData, StaminaCounts};
use crate::game::judgment::JudgeGrade;
use crate::game::profile;
use crate::game::scores::{Grade, GrooveStatsEvalState, ItlEvalState};
use crate::game::scroll::ScrollSpeedSetting;
use crate::game::song::SongData;
use crate::game::timing::{HistogramMs, TimingStats, WindowCounts};
use crate::screens::components::evaluation::pane_stats;
use crate::screens::evaluation::{EvalPane, ScoreInfo};
use crate::test_support::compose_scenarios;
use crate::ui::actors::Actor;
use rssp::TechCounts;
use rssp::stats::ArrowStats;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

pub const SCENARIO_NAME: &str = "pane-stats";

pub struct PaneStatsBenchFixture {
    score_info: ScoreInfo,
    pane: EvalPane,
    controller: profile::PlayerSide,
    asset_manager: AssetManager,
    elapsed_s: f32,
}

impl PaneStatsBenchFixture {
    pub fn build(&self) -> Vec<Actor> {
        pane_stats::build_stats_pane(
            &self.score_info,
            self.pane,
            self.controller,
            &self.asset_manager,
            self.elapsed_s,
        )
    }
}

pub fn fixture() -> PaneStatsBenchFixture {
    let mut asset_manager = AssetManager::new();
    for (name, font) in compose_scenarios::bench_fonts() {
        asset_manager.register_font(name, font);
    }

    PaneStatsBenchFixture {
        score_info: bench_score_info(),
        pane: EvalPane::HardEx,
        controller: profile::PlayerSide::P1,
        asset_manager,
        elapsed_s: 0.41,
    }
}

fn bench_score_info() -> ScoreInfo {
    let song = Arc::new(bench_song());
    let chart = Arc::new(song.charts[0].clone());
    let judgment_counts = HashMap::from([
        (JudgeGrade::Fantastic, 28_904),
        (JudgeGrade::Excellent, 2_318),
        (JudgeGrade::Great, 481),
        (JudgeGrade::Decent, 53),
        (JudgeGrade::WayOff, 7),
        (JudgeGrade::Miss, 1),
    ]);

    ScoreInfo {
        song,
        chart,
        profile_name: "BenchPlayer".to_string(),
        score_valid: true,
        disqualified: false,
        groovestats: GrooveStatsEvalState {
            valid: true,
            reason_lines: Vec::new(),
        },
        itl: ItlEvalState::default(),
        judgment_counts,
        score_percent: 0.9765,
        grade: Grade::Tier02,
        speed_mod: ScrollSpeedSetting::CMod(700.0),
        hands_achieved: 237,
        hands_total: 288,
        holds_held: 452,
        holds_total: 487,
        rolls_held: 73,
        rolls_total: 81,
        mines_avoided: 905,
        mines_total: 999,
        timing: TimingStats {
            mean_abs_ms: 11.4,
            mean_ms: -1.8,
            stddev_ms: 13.2,
            max_abs_ms: 42.5,
        },
        scatter: Vec::new(),
        scatter_worst_window_ms: 180.0,
        histogram: HistogramMs::default(),
        graph_first_second: 0.0,
        graph_last_second: 128.0,
        music_rate: 1.0,
        scroll_option: profile::ScrollOption::Normal,
        life_history: Vec::new(),
        fail_time: None,
        window_counts: WindowCounts {
            w0: 10_322,
            w1: 18_582,
            w2: 2_318,
            w3: 481,
            w4: 53,
            w5: 7,
            miss: 1,
        },
        window_counts_10ms: WindowCounts {
            w0: 8_764,
            w1: 20_140,
            w2: 2_318,
            w3: 481,
            w4: 53,
            w5: 7,
            miss: 1,
        },
        ex_score_percent: 99.14,
        hard_ex_score_percent: 98.43,
        column_judgments: Vec::new(),
        noteskin: None,
        show_fa_plus_window: true,
        show_ex_score: true,
        show_hard_ex_score: true,
        show_fa_plus_pane: true,
        track_early_judgments: true,
        machine_records: Vec::new(),
        machine_record_highlight_rank: None,
        personal_records: Vec::new(),
        personal_record_highlight_rank: None,
        show_machine_personal_split: false,
    }
}

fn bench_song() -> SongData {
    SongData {
        simfile_path: PathBuf::from("Songs/Bench/Pane Stats/pane-stats.ssc"),
        title: "Pane Stats Benchmark".to_string(),
        subtitle: "Optimization Pass".to_string(),
        translit_title: String::new(),
        translit_subtitle: String::new(),
        artist: "Bench Artist".to_string(),
        banner_path: None,
        background_path: None,
        background_changes: Vec::new(),
        has_lua: false,
        cdtitle_path: None,
        music_path: None,
        display_bpm: "180".to_string(),
        offset: 0.0,
        sample_start: None,
        sample_length: None,
        min_bpm: 180.0,
        max_bpm: 180.0,
        normalized_bpms: "0.000=180.000".to_string(),
        music_length_seconds: 128.0,
        total_length_seconds: 128,
        precise_last_second_seconds: 128.0,
        charts: vec![bench_chart()],
    }
}

fn bench_chart() -> ChartData {
    ChartData {
        chart_type: "dance-single".to_string(),
        difficulty: "challenge".to_string(),
        description: String::new(),
        chart_name: String::new(),
        meter: 15,
        step_artist: String::new(),
        short_hash: "pane-stats-bench".to_string(),
        stats: ArrowStats {
            total_arrows: 0,
            left: 0,
            down: 0,
            up: 0,
            right: 0,
            total_steps: 0,
            jumps: 0,
            hands: 288,
            mines: 999,
            holds: 487,
            rolls: 81,
            lifts: 0,
            fakes: 0,
            holding: 0,
        },
        tech_counts: TechCounts {
            crossovers: 0,
            half_crossovers: 0,
            full_crossovers: 0,
            footswitches: 0,
            up_footswitches: 0,
            down_footswitches: 0,
            sideswitches: 0,
            jacks: 0,
            brackets: 0,
            doublesteps: 0,
        },
        mines_nonfake: 999,
        stamina_counts: StaminaCounts::default(),
        total_streams: 0,
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
        has_significant_timing_changes: false,
        possible_grade_points: 0,
        holds_total: 487,
        rolls_total: 81,
        mines_total: 999,
    }
}
