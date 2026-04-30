use crate::engine::present::actors::Actor;
use crate::screens::components::shared::visual_style_bg;

pub const SCENARIO_NAME: &str = "visual-style-bg";

const ELAPSED_S: f32 = 12.345;

pub struct VisualStyleBgBenchFixture {
    state: visual_style_bg::State,
}

impl VisualStyleBgBenchFixture {
    pub fn build(&self) -> Vec<Actor> {
        self.state.build_at_elapsed(
            visual_style_bg::Params {
                active_color_index: 3,
                backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
                alpha_mul: 1.0,
            },
            ELAPSED_S,
        )
    }
}

pub fn fixture() -> VisualStyleBgBenchFixture {
    VisualStyleBgBenchFixture {
        state: visual_style_bg::State::new(),
    }
}
