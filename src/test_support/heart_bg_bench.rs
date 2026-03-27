use crate::engine::present::actors::Actor;
use crate::screens::components::shared::heart_bg;

pub const SCENARIO_NAME: &str = "heart-bg";

const ELAPSED_S: f32 = 12.345;

pub struct HeartBgBenchFixture {
    state: heart_bg::State,
}

impl HeartBgBenchFixture {
    pub fn build(&self) -> Vec<Actor> {
        self.state.build_at_elapsed(
            heart_bg::Params {
                active_color_index: 3,
                backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
                alpha_mul: 1.0,
            },
            ELAPSED_S,
        )
    }
}

pub fn fixture() -> HeartBgBenchFixture {
    HeartBgBenchFixture {
        state: heart_bg::State::new(),
    }
}
