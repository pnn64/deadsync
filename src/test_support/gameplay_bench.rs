use crate::assets::AssetManager;
use crate::engine::gfx::MeshVertex;
use crate::engine::present::actors::Actor;
use crate::game::{gameplay, profile};
use crate::screens::gameplay as gameplay_screen;
use crate::test_support::{compose_scenarios, notefield_bench};
use std::sync::Arc;

pub const SCENARIO_NAME: &str = "gameplay";

pub struct GameplayBenchFixture {
    state: gameplay_screen::State,
    asset_manager: AssetManager,
}

impl GameplayBenchFixture {
    pub fn build(&mut self, retained: bool) -> Vec<Actor> {
        if !retained {
            for cache in &self.state.notefield_model_cache {
                cache.borrow_mut().clear();
            }
        }
        let mut actors = Vec::new();
        gameplay_screen::push_actors(
            &mut actors,
            &mut self.state,
            &self.asset_manager,
            gameplay_screen::ActorViewOverride::default(),
        );
        actors
    }
}

pub fn fixture() -> GameplayBenchFixture {
    profile::set_session_play_style(profile::PlayStyle::Single);
    profile::set_session_player_side(profile::PlayerSide::P1);
    profile::set_session_joined(true, false);

    let mut base = notefield_bench::fixture();
    {
        let state = base.state_mut();
        state.song_full_title = Arc::from("Gameplay Screen Benchmark");
        state.stage_intro_text = Arc::from("STAGE 1");
        state.background_texture_key = "bench/gameplay_bg.png".to_string();
        state.autoplay_enabled = true;
        state.replay_status_text = Some(Arc::from("REPLAY BENCH"));
        state.sync_overlay_message = Some(Arc::from("Clock drift stable"));
        state.autosync_mode = gameplay::AutosyncMode::Machine;
        state.initial_global_offset_seconds = -0.021;
        state.global_offset_seconds = -0.012;
        state.autosync_standard_deviation = 0.004;
        state.autosync_offset_sample_count = 11;
        state.music_rate = 1.15;
        state.current_music_time_display = 48.25;
        state.current_music_time_visible_ns[0] = 48_250_000_000;
        state.current_music_time_visible[0] = 48.25;
        state.density_graph_first_second = 0.0;
        state.density_graph_last_second = 120.0;
        state.density_graph_top_h = 30.0;
        state.density_graph_top_w[0] = 214.0;
        state.density_graph_top_scale_y[0] = 0.85;
        state.players[0].life = 0.734;
        state.player_profiles[0].nps_graph_at_top = true;
        state.player_profiles[0].show_ex_score = true;
        state.player_profiles[0].show_hard_ex_score = true;
        state.player_profiles[0].show_life_percent = true;
        state.player_profiles[0].hide_score = false;
        state.player_profiles[0].hide_lifebar = false;
        state.player_profiles[0].hide_song_bg = false;
        state.player_profiles[0].data_visualizations = profile::DataVisualizations::None;
    }
    let (state, _) = base.into_parts();
    let mut state = gameplay_screen::State::from_gameplay(state);
    state.density_graph.top_mesh[0] = Some(top_graph_mesh());

    let mut asset_manager = AssetManager::new();
    for (name, font) in compose_scenarios::bench_fonts() {
        asset_manager.register_font(name, font);
    }

    GameplayBenchFixture {
        state,
        asset_manager,
    }
}

fn top_graph_mesh() -> Arc<[MeshVertex]> {
    Arc::from(vec![
        MeshVertex {
            pos: [0.0, 25.5],
            color: [0.12, 0.82, 0.93, 0.95],
        },
        MeshVertex {
            pos: [18.0, 22.0],
            color: [0.12, 0.82, 0.93, 0.95],
        },
        MeshVertex {
            pos: [36.0, 14.0],
            color: [0.12, 0.82, 0.93, 0.95],
        },
        MeshVertex {
            pos: [54.0, 17.0],
            color: [0.12, 0.82, 0.93, 0.95],
        },
        MeshVertex {
            pos: [72.0, 9.0],
            color: [0.12, 0.82, 0.93, 0.95],
        },
        MeshVertex {
            pos: [90.0, 12.0],
            color: [0.12, 0.82, 0.93, 0.95],
        },
        MeshVertex {
            pos: [108.0, 6.0],
            color: [0.12, 0.82, 0.93, 0.95],
        },
        MeshVertex {
            pos: [126.0, 9.0],
            color: [0.12, 0.82, 0.93, 0.95],
        },
        MeshVertex {
            pos: [144.0, 3.0],
            color: [0.12, 0.82, 0.93, 0.95],
        },
        MeshVertex {
            pos: [162.0, 10.0],
            color: [0.12, 0.82, 0.93, 0.95],
        },
        MeshVertex {
            pos: [180.0, 6.0],
            color: [0.12, 0.82, 0.93, 0.95],
        },
        MeshVertex {
            pos: [198.0, 13.0],
            color: [0.12, 0.82, 0.93, 0.95],
        },
        MeshVertex {
            pos: [214.0, 11.0],
            color: [0.12, 0.82, 0.93, 0.95],
        },
        MeshVertex {
            pos: [0.0, 25.5],
            color: [0.12, 0.82, 0.93, 0.0],
        },
        MeshVertex {
            pos: [214.0, 25.5],
            color: [0.12, 0.82, 0.93, 0.0],
        },
    ])
}
