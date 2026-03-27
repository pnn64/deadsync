use crate::assets::AssetManager;
use crate::engine::present::actors::Actor;
use crate::game::profile;
use crate::game::timing::WindowCounts;
use crate::screens::components::gameplay::gameplay_stats;
use crate::test_support::{compose_scenarios, notefield_bench};
use std::sync::Arc;

pub const SCENARIO_NAME: &str = "gameplay-stats-versus";

pub struct GameplayStatsVersusBenchFixture {
    base: notefield_bench::NotefieldBenchFixture,
    asset_manager: AssetManager,
}

impl GameplayStatsVersusBenchFixture {
    pub fn build(&self) -> Vec<Actor> {
        gameplay_stats::build_versus_step_stats(self.base.state(), &self.asset_manager)
    }
}

pub fn fixture() -> GameplayStatsVersusBenchFixture {
    profile::set_session_play_style(profile::PlayStyle::Versus);
    profile::set_session_player_side(profile::PlayerSide::P1);
    profile::set_session_joined(true, true);

    let mut base = notefield_bench::fixture();
    {
        let state = base.state_mut();
        state.num_players = 2;
        state.num_cols = 8;
        state.cols_per_player = 4;
        state.note_ranges[1] = state.note_ranges[0];
        state.total_elapsed_in_screen = 9.6;
        state.current_music_time_display = 64.25;

        state.players[0].judgment_counts = [22_481, 2_118, 351, 49, 12, 3];
        state.players[1].judgment_counts = [20_204, 1_804, 404, 88, 23, 7];
        state.player_profiles[0].data_visualizations = profile::DataVisualizations::StepStatistics;
        state.player_profiles[1].data_visualizations = profile::DataVisualizations::StepStatistics;
        state.player_profiles[0].show_fa_plus_window = true;
        state.player_profiles[0].fa_plus_10ms_blue_window = true;
        state.player_profiles[1].show_fa_plus_window = false;
        state.player_profiles[1].custom_fantastic_window = false;

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
        state.live_window_counts[1] = WindowCounts {
            w0: 20_204,
            w1: 0,
            w2: 1_804,
            w3: 404,
            w4: 88,
            w5: 23,
            miss: 7,
        };
        state.live_window_counts_10ms_blue[1] = state.live_window_counts[1];
        state.live_window_counts_display_blue[1] = state.live_window_counts[1];

        let song = Arc::make_mut(&mut state.song);
        song.banner_path = Some("bench/banner.png".into());
        state.song_full_title = Arc::from("Gameplay Stats Versus Benchmark");
    }

    let mut asset_manager = AssetManager::new();
    for (name, font) in compose_scenarios::bench_fonts() {
        asset_manager.register_font(name, font);
    }

    GameplayStatsVersusBenchFixture {
        base,
        asset_manager,
    }
}
