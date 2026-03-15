use crate::screens::init;
use crate::ui::actors::Actor;

pub const SCENARIO_NAME: &str = "init";

const LOADING_ELAPSED_S: f32 = 7.25;

pub struct InitBenchFixture {
    state: init::State,
}

impl InitBenchFixture {
    pub fn build(&self, retained: bool) -> Vec<Actor> {
        if !retained {
            init::clear_render_cache(&self.state);
        }
        init::get_actors_at_loading_elapsed(&self.state, LOADING_ELAPSED_S)
    }
}

pub fn fixture() -> InitBenchFixture {
    InitBenchFixture {
        state: init::bench_loading_state(),
    }
}
