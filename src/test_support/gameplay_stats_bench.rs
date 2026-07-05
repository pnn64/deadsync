use crate::assets::AssetManager;
use crate::screens::components::gameplay::{gameplay_stats, notefield};
use crate::screens::gameplay as gameplay_screen;
use crate::test_support::{compose_scenarios, notefield_bench};
use deadlib_present::actors::Actor;
use deadsync_notefield::{FieldPlacement, ProxyCaptureRequests, ViewOverride};
use deadsync_profile as profile_data;
use deadsync_rules::timing::WindowCounts;
use deadsync_score::{
    ArrowCloudPaneKind, CachedPlayerLeaderboardData, LeaderboardEntry, LeaderboardPane,
    PlayerLeaderboardData,
};
use std::path::PathBuf;
use std::sync::Arc;

pub const SCENARIO_NAME: &str = "gameplay-stats";

pub struct GameplayStatsBenchFixture {
    state: gameplay_screen::State,
    asset_manager: AssetManager,
    playfield_center_x: f32,
}

impl GameplayStatsBenchFixture {
    pub fn build(&self) -> Vec<Actor> {
        let mut actors = Vec::new();
        gameplay_stats::push_step_stats(
            &mut actors,
            &self.state,
            &self.asset_manager,
            self.playfield_center_x,
            profile_data::PlayerSide::P1,
        );
        actors
    }
}

pub fn fixture() -> GameplayStatsBenchFixture {
    let mut base = notefield_bench::fixture();
    let scorebox_side_snapshot;
    {
        let state = base.state_mut();
        state.set_song_banner_path(Some(PathBuf::from("bench/banner.png")));
        state.set_screen_elapsed(9.6);
        state.set_song_position_for_benchmark(
            state.current_beat(),
            state.current_music_time_ns(),
            state.current_beat_display(),
            64.25,
        );
        state.update_player(0, |player| {
            player.judgment_counts = [22_481, 2_118, 351, 49, 12, 3];
            player.holds_held = 146;
            player.rolls_held = 31;
            player.mines_avoided = 503;
        });
        state.update_profile(0, |profile| {
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
        state.set_live_window_counts(0, canonical, ten_ms_blue, ten_ms_blue);
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
                srpg_self_score: None,
                itl_self_score: None,
                itl_self_rank: None,
            }),
        });
    }
    let (state, noteskin_assets, bench_profile) = base.into_parts();
    let mut state = gameplay_screen::State::from_gameplay(state, noteskin_assets);
    state.song_full_title = Arc::from("Gameplay Stats Benchmark");
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

    let mut notefield_actors = Vec::new();
    let mut notefield_hud_actors = Vec::new();
    let playfield_center_x = notefield::build_bundles(
        &state,
        &state.noteskin_assets,
        &state.notefield_model_cache,
        &bench_profile,
        FieldPlacement::P1,
        profile_data::PlayStyle::Single,
        false,
        ProxyCaptureRequests::default(),
        false,
        ViewOverride::default(),
        &mut notefield_actors,
        &mut notefield_hud_actors,
    )
    .layout_center_x;

    GameplayStatsBenchFixture {
        state,
        asset_manager,
        playfield_center_x,
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
