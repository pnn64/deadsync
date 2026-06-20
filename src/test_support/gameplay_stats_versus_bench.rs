use crate::assets::AssetManager;
use crate::game::profile;
use crate::screens::components::gameplay::gameplay_stats;
use crate::screens::gameplay as gameplay_screen;
use crate::test_support::{compose_scenarios, notefield_bench};
use deadlib_present::actors::Actor;
use deadsync_profile as profile_data;
use deadsync_rules::timing::WindowCounts;
use std::sync::Arc;

pub const SCENARIO_NAME: &str = "gameplay-stats-versus";

pub struct GameplayStatsVersusBenchFixture {
    state: gameplay_screen::State,
    asset_manager: AssetManager,
}

impl GameplayStatsVersusBenchFixture {
    pub fn build(&self) -> Vec<Actor> {
        let mut actors = Vec::new();
        gameplay_stats::push_versus_step_stats(&mut actors, &self.state, &self.asset_manager);
        actors
    }
}

pub fn fixture() -> GameplayStatsVersusBenchFixture {
    profile::set_session_play_style(profile_data::PlayStyle::Versus);
    profile::set_session_player_side(profile_data::PlayerSide::P1);
    profile::set_session_joined(true, true);

    let mut base = notefield_bench::fixture();
    {
        let state = base.state_mut();
        state.set_num_players(2);
        state.set_num_cols(8);
        state.set_cols_per_player(4);
        let p1_range = state.note_range_for_player(0);
        state.set_note_range(1, p1_range);
        state.set_screen_elapsed(9.6);
        state.set_song_position_for_benchmark(
            state.current_beat(),
            state.current_music_time_ns(),
            state.current_beat_display(),
            64.25,
        );

        state.update_player(0, |player| {
            player.judgment_counts = [22_481, 2_118, 351, 49, 12, 3];
        });
        state.update_player(1, |player| {
            player.judgment_counts = [20_204, 1_804, 404, 88, 23, 7];
        });
        state.update_profile(0, |profile| {
            profile.step_statistics = profile_data::StepStatisticsMask::all_widgets();
            profile.show_fa_plus_window = true;
            profile.fa_plus_10ms_blue_window = true;
        });
        state.update_profile(1, |profile| {
            profile.step_statistics = profile_data::StepStatisticsMask::all_widgets();
            profile.show_fa_plus_window = false;
            profile.custom_fantastic_window = false;
        });

        let p1_canonical = WindowCounts {
            w0: 18_992,
            w1: 3_489,
            w2: 2_118,
            w3: 351,
            w4: 49,
            w5: 12,
            miss: 3,
        };
        let p1_ten_ms_blue = WindowCounts {
            w0: 19_704,
            w1: 2_777,
            w2: 2_118,
            w3: 351,
            w4: 49,
            w5: 12,
            miss: 3,
        };
        state.set_live_window_counts(0, p1_canonical, p1_ten_ms_blue, p1_ten_ms_blue);
        let p2_counts = WindowCounts {
            w0: 20_204,
            w1: 0,
            w2: 1_804,
            w3: 404,
            w4: 88,
            w5: 23,
            miss: 7,
        };
        state.set_live_window_counts(1, p2_counts, p2_counts, p2_counts);

        state.set_song_banner_path(Some("bench/banner.png".into()));
    }

    let (state, noteskin_assets, _) = base.into_parts();
    let mut state = gameplay_screen::State::from_gameplay(state, noteskin_assets);
    state.song_full_title = Arc::from("Gameplay Stats Versus Benchmark");
    gameplay_stats::refresh_density_graph_meshes(&mut state);

    let mut asset_manager = AssetManager::new();
    for (name, font) in compose_scenarios::bench_fonts() {
        asset_manager.register_font(name, font);
    }

    GameplayStatsVersusBenchFixture {
        state,
        asset_manager,
    }
}
