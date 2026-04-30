use crate::assets::AssetManager;
use crate::engine::input::{InputEvent, InputSource, VirtualAction};
use crate::engine::present::actors::Actor;
use crate::screens::options;
use std::time::Instant;

use crate::test_support::compose_scenarios;

pub const SCENARIO_NAME: &str = "options";

pub struct OptionsBenchFixture {
    state: options::State,
    asset_manager: AssetManager,
}

impl OptionsBenchFixture {
    pub fn build(&self, retained: bool) -> Vec<Actor> {
        if !retained {
            options::clear_render_cache(&self.state);
        }
        let mut actors = options::get_actors(&self.state, &self.asset_manager, 1.0);
        actors.retain(|actor| actor_z(actor) >= 0);
        actors
    }
}

pub fn fixture() -> OptionsBenchFixture {
    let mut asset_manager = AssetManager::new();
    for (name, font) in compose_scenarios::bench_fonts() {
        asset_manager.register_font(name, font);
    }

    let mut state = options::init();
    press(&mut state, &asset_manager, VirtualAction::p1_down);
    press(&mut state, &asset_manager, VirtualAction::p1_start);
    options::update(&mut state, 1.0, &asset_manager);
    options::update(&mut state, 1.0, &asset_manager);
    press(&mut state, &asset_manager, VirtualAction::p1_down);
    press(&mut state, &asset_manager, VirtualAction::p1_down);
    options::update(&mut state, 0.016, &asset_manager);

    OptionsBenchFixture {
        state,
        asset_manager,
    }
}

fn press(state: &mut options::State, asset_manager: &AssetManager, action: VirtualAction) {
    let now = Instant::now();
    let ev = InputEvent {
        action,
        input_slot: 0,
        pressed: true,
        source: InputSource::Keyboard,
        timestamp: now,
        timestamp_host_nanos: 0,
        stored_at: now,
        emitted_at: now,
    };
    let _ = options::handle_input(state, asset_manager, &ev);
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
