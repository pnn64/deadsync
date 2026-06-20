use crate::assets::AssetManager;
use crate::game::{gameplay, profile};
use crate::screens::gameplay as gameplay_screen;
use crate::test_support::{compose_scenarios, notefield_bench};
use deadlib_present::actors::Actor;
use deadlib_render::MeshVertex;
use deadsync_profile as profile_data;
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
    profile::set_session_play_style(profile_data::PlayStyle::Single);
    profile::set_session_player_side(profile_data::PlayerSide::P1);
    profile::set_session_joined(true, false);

    let mut base = notefield_bench::fixture();
    {
        let state = base.state_mut();
        state.set_autoplay_enabled_for_benchmark(true);
        state.set_global_offsets(-0.021, -0.012);
        state.set_autosync_state_for_benchmark(gameplay::AutosyncMode::Machine, 0.004, 11);
        gameplay::set_music_rate(state, 1.15);
        state.set_song_position_for_benchmark(
            state.current_beat(),
            state.current_music_time_ns(),
            state.current_beat_display(),
            48.25,
        );
        state.set_visible_time(0, 48_250_000_000, 48.25, state.visible_beat(0));
        state.set_density_graph_top_for_benchmark(0.0, 120.0, 0, 214.0, 30.0, 0.85);
        state.update_player(0, |player| {
            player.life = 0.734;
        });
        state.update_profile(0, |profile| {
            profile.nps_graph_at_top = true;
            profile.show_ex_score = true;
            profile.show_hard_ex_score = true;
            profile.show_life_percent = true;
            profile.hide_score = false;
            profile.hide_lifebar = false;
            profile.hide_song_bg = false;
            profile.step_statistics = profile_data::StepStatisticsMask::empty();
        });
    }
    let (state, noteskin_assets, _) = base.into_parts();
    let mut state = gameplay_screen::State::from_gameplay(state, noteskin_assets);
    state.song_full_title = Arc::from("Gameplay Screen Benchmark");
    state.stage_intro_text = Arc::from("STAGE 1");
    state.replay_status_text = Some(Arc::from("REPLAY BENCH"));
    state.background_texture_key = Arc::from("bench/gameplay_bg.png");
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
