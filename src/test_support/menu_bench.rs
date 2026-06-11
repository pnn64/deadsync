use crate::screens::menu;
use deadsync_present::actors::Actor;

pub const SCENARIO_NAME: &str = "menu";

pub struct MenuBenchFixture {
    state: menu::State,
}

impl MenuBenchFixture {
    pub fn push(&self, actors: &mut Vec<Actor>, retained: bool) {
        if !retained {
            menu::clear_render_cache(&self.state);
        }
        menu::push_actors(actors, &self.state, 1.0);
        actors.retain(|actor| actor_z(actor) >= 0);
    }

    pub fn build(&self, retained: bool) -> Vec<Actor> {
        let mut actors = Vec::with_capacity(96);
        self.push(&mut actors, retained);
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
        Actor::Camera { .. } | Actor::CameraPush { .. } | Actor::CameraPop => 0,
        Actor::Shadow { child, .. } => actor_z(child),
    }
}
