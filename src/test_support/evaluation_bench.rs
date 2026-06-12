use crate::assets::AssetManager;
use crate::game::profile;
use crate::screens::evaluation::{self, ScoreInfo, State};
use crate::test_support::{compose_scenarios, pane_stats_bench};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_present::actors::Actor;
use deadsync_profile as profile_data;
use deadsync_rules::timing::{HistogramMs, ScatterPoint};
use deadsync_score::LeaderboardEntry;

pub const SCENARIO_NAME: &str = "evaluation";
pub const SCENARIO_NAME_VERSUS: &str = "evaluation-versus";

const STAGE_DURATION_SECONDS: f32 = 90.0;
const SCATTER_POINTS: usize = 1_536;
const LIFE_SAMPLES: usize = 240;
const LEADERBOARD_ROWS: usize = 10;

/// Benchmark fixture for the full evaluation screen: it owns a fully-built
/// `State` (via `init_from_score_info`) plus a font-seeded `AssetManager`, and
/// `build()` produces the per-frame actor tree through `evaluation::get_actors`.
pub struct EvaluationBenchFixture {
    state: State,
    asset_manager: AssetManager,
}

impl EvaluationBenchFixture {
    pub fn build(&self) -> Vec<Actor> {
        evaluation::get_actors(&self.state, &self.asset_manager)
    }
}

/// Single-player evaluation scenario (default Standard pane), the common hot path.
pub fn fixture() -> EvaluationBenchFixture {
    profile::set_session_play_style(profile_data::PlayStyle::Single);
    profile::set_session_player_side(profile_data::PlayerSide::P1);
    profile::set_session_joined(true, false);

    let mut score_info: [Option<ScoreInfo>; MAX_PLAYERS] = std::array::from_fn(|_| None);
    score_info[0] = Some(heavy_score_info(profile_data::PlayerSide::P1));

    let state = evaluation::init_from_score_info(score_info, STAGE_DURATION_SECONDS);
    EvaluationBenchFixture {
        state,
        asset_manager: bench_asset_manager(),
    }
}

/// Versus evaluation scenario (two players), the heaviest configuration.
pub fn fixture_versus() -> EvaluationBenchFixture {
    profile::set_session_play_style(profile_data::PlayStyle::Versus);
    profile::set_session_player_side(profile_data::PlayerSide::P1);
    profile::set_session_joined(true, true);

    let mut score_info: [Option<ScoreInfo>; MAX_PLAYERS] = std::array::from_fn(|_| None);
    score_info[0] = Some(heavy_score_info(profile_data::PlayerSide::P1));
    score_info[1] = Some(heavy_score_info(profile_data::PlayerSide::P2));

    let state = evaluation::init_from_score_info(score_info, STAGE_DURATION_SECONDS);
    EvaluationBenchFixture {
        state,
        asset_manager: bench_asset_manager(),
    }
}

fn bench_asset_manager() -> AssetManager {
    let mut asset_manager = AssetManager::new();
    for (name, font) in compose_scenarios::bench_fonts() {
        asset_manager.register_font(name, font);
    }
    asset_manager
}

/// Realistic, heavy `ScoreInfo`: starts from the shared pane-stats fixture and
/// fills in the scatter plot, timing histogram, life graph, and machine/personal
/// leaderboards so the whole screen renders representative content.
fn heavy_score_info(side: profile_data::PlayerSide) -> ScoreInfo {
    let mut si = pane_stats_bench::bench_score_info();
    si.side = side;
    si.profile_name = match side {
        profile_data::PlayerSide::P1 => "BenchPlayer1",
        profile_data::PlayerSide::P2 => "BenchPlayer2",
    }
    .to_string();

    si.scatter = bench_scatter();
    si.scatter_worst_window_ms = 90.0;
    si.histogram = bench_histogram();
    si.life_history = bench_life_history();

    si.machine_records = bench_leaderboard(side, true);
    si.machine_record_highlight_rank = Some(3);
    si.personal_records = bench_leaderboard(side, false);
    si.personal_record_highlight_rank = Some(1);
    si.show_machine_personal_split = true;

    si
}

fn bench_scatter() -> Vec<ScatterPoint> {
    (0..SCATTER_POINTS)
        .map(|i| {
            let t = i as f32 / SCATTER_POINTS as f32;
            // Deterministic pseudo-random offset spread in roughly [-30, 30] ms.
            let phase = (i as f32 * 0.61803398875).fract();
            let offset = (phase - 0.5) * 60.0;
            let is_miss = i % 97 == 0;
            ScatterPoint {
                time_sec: t * 128.0,
                offset_ms: if is_miss { None } else { Some(offset) },
                direction_code: (i % 4 + 1) as u8,
                is_stream: i % 3 == 0,
                is_left_foot: i % 2 == 0,
                miss_because_held: false,
            }
        })
        .collect()
}

fn bench_histogram() -> HistogramMs {
    let mut bins: Vec<(i32, u32)> = Vec::with_capacity(91);
    let mut smoothed: Vec<(i32, f32)> = Vec::with_capacity(91);
    let mut max_count = 0u32;
    for bin in -45i32..=45 {
        // Triangular-ish distribution peaking at 0ms.
        let count = (46 - bin.unsigned_abs() as i32).max(0) as u32 * 12;
        max_count = max_count.max(count);
        bins.push((bin, count));
        smoothed.push((bin, count as f32 * 0.95));
    }
    HistogramMs {
        bins,
        smoothed,
        max_count,
        worst_observed_ms: 44.0,
        worst_window_ms: 45.0,
    }
}

fn bench_life_history() -> Vec<(f32, f32)> {
    (0..LIFE_SAMPLES)
        .map(|i| {
            let t = i as f32 / LIFE_SAMPLES as f32 * 128.0;
            let life = 0.5 + 0.5 * (i as f32 * 0.13).sin();
            (t, life.clamp(0.0, 1.0))
        })
        .collect()
}

fn bench_leaderboard(side: profile_data::PlayerSide, machine: bool) -> Vec<LeaderboardEntry> {
    let self_rank = if machine { 3 } else { 1 };
    let tag_prefix = match side {
        profile_data::PlayerSide::P1 => "AAA",
        profile_data::PlayerSide::P2 => "BBB",
    };
    (1..=LEADERBOARD_ROWS as u32)
        .map(|rank| LeaderboardEntry {
            rank,
            name: format!("{tag_prefix}{rank:02}"),
            machine_tag: Some(format!("{tag_prefix}")),
            score: 99.5 - rank as f64 * 0.37,
            date: "2025-01-01".to_string(),
            is_rival: rank % 4 == 0,
            is_self: rank == self_rank,
            is_fail: false,
        })
        .collect()
}
