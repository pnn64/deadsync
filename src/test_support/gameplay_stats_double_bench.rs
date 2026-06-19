use crate::assets::AssetManager;
use crate::game::gameplay;
use crate::game::profile;
use crate::screens::components::gameplay::gameplay_stats;
use crate::screens::gameplay as gameplay_screen;
use crate::test_support::{compose_scenarios, notefield_bench};
use deadlib_present::actors::Actor;
use deadlib_present::space::screen_center_x;
use deadsync_profile as profile_data;
use deadsync_rules::timing::WindowCounts;
use deadsync_score::{
    ArrowCloudPaneKind, CachedPlayerLeaderboardData, LeaderboardEntry, LeaderboardPane,
    PlayerLeaderboardData,
};
use std::path::PathBuf;
use std::sync::Arc;

pub const SCENARIO_NAME: &str = "gameplay-stats-double";

pub struct GameplayStatsDoubleBenchFixture {
    state: gameplay_screen::State,
    asset_manager: AssetManager,
}

impl GameplayStatsDoubleBenchFixture {
    pub fn build(&self) -> Vec<Actor> {
        let mut actors = Vec::new();
        gameplay_stats::push_double_step_stats(
            &mut actors,
            &self.state,
            &self.asset_manager,
            screen_center_x(),
        );
        actors
    }
}

pub fn fixture() -> GameplayStatsDoubleBenchFixture {
    profile::set_session_play_style(profile_data::PlayStyle::Double);
    profile::set_session_player_side(profile_data::PlayerSide::P1);
    profile::set_session_joined(true, false);

    let mut base = notefield_bench::fixture();
    let scorebox_side_snapshot;
    {
        let state = base.state_mut();
        gameplay::set_benchmark_song_banner_path(state, Some(PathBuf::from("bench/banner.png")));
        gameplay::set_benchmark_screen_elapsed(state, 9.6);
        gameplay::set_benchmark_song_position(
            state,
            gameplay::current_beat(state),
            gameplay::current_music_time_ns(state),
            gameplay::current_beat_display(state),
            64.25,
        );
        gameplay::set_benchmark_cols_per_player(state, 8);
        gameplay::set_benchmark_num_cols(state, 8);
        gameplay::update_benchmark_player(state, 0, |player| {
            player.judgment_counts = [22_481, 2_118, 351, 49, 12, 3];
            player.holds_held = 146;
            player.rolls_held = 31;
            player.mines_avoided = 503;
        });
        gameplay::update_benchmark_player_profile(state, 0, |profile| {
            profile.show_fa_plus_window = true;
            profile.fa_plus_10ms_blue_window = true;
        });
        let canonical = WindowCounts {
            w0: 18_992,
            w1: 3_489,
            w2: 2_118,
            w3: 351,
            w4: 49,
            w5: 12,
            miss: 3,
        };
        let ten_ms_blue = WindowCounts {
            w0: 19_704,
            w1: 2_777,
            w2: 2_118,
            w3: 351,
            w4: 49,
            w5: 12,
            miss: 3,
        };
        gameplay::set_benchmark_live_window_counts(state, 0, canonical, ten_ms_blue, ten_ms_blue);
        scorebox_side_snapshot = Some(CachedPlayerLeaderboardData {
            loading: false,
            error: None,
            data: Some(PlayerLeaderboardData {
                panes: vec![
                    LeaderboardPane {
                        name: "GrooveStats".to_string(),
                        is_ex: false,
                        disabled: false,
                        personalized: true,
                        arrowcloud_kind: None,
                        entries: vec![
                            leaderboard_entry(1, "WOLF", 9987.42, false, false),
                            leaderboard_entry(2, "YOU", 9975.13, false, true),
                            leaderboard_entry(3, "RIV1", 9968.56, true, false),
                            leaderboard_entry(4, "RIV2", 9961.04, true, false),
                            leaderboard_entry(5, "RIV3", 9958.44, true, false),
                        ],
                    },
                    LeaderboardPane {
                        name: "ArrowCloud".to_string(),
                        is_ex: false,
                        disabled: false,
                        personalized: true,
                        arrowcloud_kind: Some(ArrowCloudPaneKind::HardEx),
                        entries: vec![
                            leaderboard_entry(1, "AC01", 98.72, false, false),
                            leaderboard_entry(2, "YOU", 98.31, false, true),
                            leaderboard_entry(3, "RIV1", 97.95, true, false),
                            leaderboard_entry(4, "RIV2", 97.11, true, false),
                            leaderboard_entry(5, "RIV3", 96.80, true, false),
                        ],
                    },
                ],
                itl_self_score: None,
                itl_self_rank: None,
            }),
        });
    }

    let (state, noteskin_assets, _) = base.into_parts();
    let mut state = gameplay_screen::State::from_gameplay(state, noteskin_assets);
    state.song_full_title = Arc::from("Gameplay Stats Double Benchmark");
    state.scorebox_side_snapshot[0] = scorebox_side_snapshot;
    state.set_pack_display(
        Arc::from("Bench Pack"),
        Some(PathBuf::from("bench/banner.png")),
    );
    gameplay_stats::refresh_density_graph_meshes(&mut state);

    let mut asset_manager = AssetManager::new();
    for (name, font) in compose_scenarios::bench_fonts() {
        asset_manager.register_font(name, font);
    }

    GameplayStatsDoubleBenchFixture {
        state,
        asset_manager,
    }
}

fn leaderboard_entry(
    rank: u32,
    name: &str,
    score: f64,
    is_rival: bool,
    is_self: bool,
) -> LeaderboardEntry {
    LeaderboardEntry {
        rank,
        name: name.to_string(),
        machine_tag: Some(name.to_string()),
        score,
        date: "2026-03-12".to_string(),
        is_rival,
        is_self,
        is_fail: false,
    }
}
