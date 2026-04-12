use crate::assets::AssetManager;
use crate::engine::present::actors::Actor;
use crate::engine::space::screen_center_x;
use crate::game::profile;
use crate::game::scores::{
    CachedPlayerLeaderboardData, LeaderboardEntry, LeaderboardPane, PlayerLeaderboardData,
};
use crate::game::timing::WindowCounts;
use crate::screens::components::gameplay::gameplay_stats;
use crate::screens::gameplay as gameplay_screen;
use crate::test_support::{compose_scenarios, notefield_bench};
use std::path::PathBuf;
use std::sync::Arc;

pub const SCENARIO_NAME: &str = "gameplay-stats-double";

pub struct GameplayStatsDoubleBenchFixture {
    state: gameplay_screen::State,
    asset_manager: AssetManager,
}

impl GameplayStatsDoubleBenchFixture {
    pub fn build(&self) -> Vec<Actor> {
        gameplay_stats::build_double_step_stats(&self.state, &self.asset_manager, screen_center_x())
    }
}

pub fn fixture() -> GameplayStatsDoubleBenchFixture {
    profile::set_session_play_style(profile::PlayStyle::Double);
    profile::set_session_player_side(profile::PlayerSide::P1);
    profile::set_session_joined(true, false);

    let mut base = notefield_bench::fixture();
    {
        let state = base.state_mut();
        let song = Arc::make_mut(&mut state.song);
        song.banner_path = Some(PathBuf::from("bench/banner.png"));
        state.pack_banner_path = Some(PathBuf::from("bench/banner.png"));
        state.pack_group = Arc::from("Bench Pack");
        state.song_full_title = Arc::from("Gameplay Stats Double Benchmark");
        state.total_elapsed_in_screen = 9.6;
        state.current_music_time_display = 64.25;
        state.cols_per_player = 8;
        state.num_cols = 8;
        state.players[0].judgment_counts = [22_481, 2_118, 351, 49, 12, 3];
        state.players[0].holds_held = 146;
        state.players[0].rolls_held = 31;
        state.players[0].mines_avoided = 503;
        state.player_profiles[0].show_fa_plus_window = true;
        state.player_profiles[0].fa_plus_10ms_blue_window = true;
        state.live_window_counts[0] = WindowCounts {
            w0: 18_992,
            w1: 3_489,
            w2: 2_118,
            w3: 351,
            w4: 49,
            w5: 12,
            miss: 3,
        };
        state.live_window_counts_10ms_blue[0] = WindowCounts {
            w0: 19_704,
            w1: 2_777,
            w2: 2_118,
            w3: 351,
            w4: 49,
            w5: 12,
            miss: 3,
        };
        state.live_window_counts_display_blue[0] = state.live_window_counts_10ms_blue[0];
        state.scorebox_side_snapshot[0] = Some(CachedPlayerLeaderboardData {
            loading: false,
            error: None,
            data: Some(PlayerLeaderboardData {
                panes: vec![
                    LeaderboardPane {
                        name: "GrooveStats".to_string(),
                        is_ex: false,
                        disabled: false,
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
            }),
        });
    }

    let (state, _) = base.into_parts();
    let mut state = gameplay_screen::State::from_gameplay(state);
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
