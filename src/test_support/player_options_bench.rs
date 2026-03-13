use crate::assets::AssetManager;
use crate::game::profile;
use crate::screens::{Screen, player_options};
use crate::test_support::{compose_scenarios, notefield_bench};
use crate::ui::actors::Actor;

pub const SCENARIO_NAME: &str = "player-options";

pub struct PlayerOptionsBenchFixture {
    state: player_options::State,
    asset_manager: AssetManager,
}

impl PlayerOptionsBenchFixture {
    pub fn build(&self, retained: bool) -> Vec<Actor> {
        if !retained {
            player_options::clear_inline_choice_layout_cache(&self.state);
            player_options::clear_row_text_cache(&self.state);
        }
        let mut actors = player_options::get_actors(&self.state, &self.asset_manager);
        actors.retain(|actor| actor_z(actor) >= 0);
        actors
    }
}

pub fn fixture() -> PlayerOptionsBenchFixture {
    let base = notefield_bench::fixture();
    let song = base.state().song.clone();

    profile::set_session_play_style(profile::PlayStyle::Versus);
    profile::set_session_player_side(profile::PlayerSide::P1);
    profile::set_session_joined(true, true);

    let mut asset_manager = AssetManager::new();
    for (name, font) in compose_scenarios::bench_fonts() {
        asset_manager.register_font(name, font);
    }

    let mut state = player_options::init(song, [0; 2], [0; 2], 1, Screen::SelectMusic, None);

    let perspective_row = state
        .rows
        .iter()
        .position(|row| row.name == "Perspective")
        .unwrap_or(0);
    let background_filter_row = state
        .rows
        .iter()
        .position(|row| row.name == "Background Filter")
        .unwrap_or(perspective_row);
    state.selected_row = [perspective_row, background_filter_row];
    state.prev_selected_row = state.selected_row;
    player_options::update(&mut state, 1.0, &asset_manager);
    player_options::update(&mut state, 1.0, &asset_manager);

    PlayerOptionsBenchFixture {
        state,
        asset_manager,
    }
}

fn actor_z(actor: &Actor) -> i16 {
    match actor {
        Actor::Sprite { z, .. }
        | Actor::Text { z, .. }
        | Actor::Mesh { z, .. }
        | Actor::TexturedMesh { z, .. }
        | Actor::Frame { z, .. } => *z,
        Actor::Camera { .. } => 0,
        Actor::Shadow { child, .. } => actor_z(child),
    }
}
