use crate::screens::components::heart_bg;
use crate::ui::actors::Actor;

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
