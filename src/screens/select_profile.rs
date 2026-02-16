use crate::assets::AssetManager;
use crate::core::input::InputEvent;
use crate::screens::ScreenAction;
use crate::screens::components::profile_boxes;
use crate::ui::actors::Actor;

pub type State = profile_boxes::State;

#[inline(always)]
pub const fn exit_anim_duration() -> f32 {
    profile_boxes::exit_anim_duration()
}

#[inline(always)]
pub fn init() -> State {
    profile_boxes::init()
}

#[inline(always)]
pub fn set_joined(state: &mut State, p1_joined: bool, p2_joined: bool) {
    profile_boxes::set_joined(state, p1_joined, p2_joined);
}

#[inline(always)]
pub fn update(state: &mut State, dt: f32) {
    profile_boxes::update(state, dt);
}

#[inline(always)]
pub fn in_transition() -> (Vec<Actor>, f32) {
    profile_boxes::in_transition()
}

#[inline(always)]
pub fn out_transition() -> (Vec<Actor>, f32) {
    profile_boxes::out_transition()
}

#[inline(always)]
pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    profile_boxes::handle_input(state, ev)
}

#[inline(always)]
pub fn get_actors(
    state: &State,
    asset_manager: &AssetManager,
    alpha_multiplier: f32,
) -> Vec<Actor> {
    profile_boxes::get_actors(state, asset_manager, alpha_multiplier)
}
