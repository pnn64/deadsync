use crate::engine::present::actors::Actor;
use crate::screens::menu;

pub const SCENARIO_NAME: &str = "menu";

pub struct MenuBenchFixture {
    state: menu::State,
}

impl MenuBenchFixture {
    pub fn build(&self, retained: bool) -> Vec<Actor> {
        if !retained {
            menu::clear_render_cache(&self.state);
        }
        let mut actors = menu::get_actors(&self.state, 1.0);
        actors.retain(|actor| actor_z(actor) >= 0);
        actors
    }
}

pub fn fixture() -> MenuBenchFixture {
    MenuBenchFixture {
        state: menu::init(),
    }
}

fn actor_z(actor: &Actor) -> i16 {
    match actor {
        Actor::Sprite { z, .. }
        | Actor::Text { z, .. }
        | Actor::Mesh { z, .. }
        | Actor::TexturedMesh { z, .. }
        | Actor::Frame { z, .. }
        | Actor::SharedFrame { z, .. } => *z,
        Actor::Camera { .. } => 0,
        Actor::Shadow { child, .. } => actor_z(child),
    }
}
