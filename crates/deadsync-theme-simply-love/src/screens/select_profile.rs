use crate::assets::AssetManager;
use crate::screens::ThemeEffect;
use crate::screens::components::shared::profile_boxes;
use deadlib_present::actors::Actor;
use deadsync_input::InputEvent;
use deadsync_profile as profile_data;

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
pub fn enter_late_join(state: &mut State, joining_side: profile_data::PlayerSide) {
    profile_boxes::enter_late_join(state, joining_side);
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
pub fn handle_input(state: &mut State, ev: &InputEvent) -> ThemeEffect {
    profile_boxes::handle_input(state, ev)
}

#[inline(always)]
pub fn push_actors(
    actors: &mut Vec<Actor>,
    state: &State,
    asset_manager: &AssetManager,
    alpha_multiplier: f32,
) {
    profile_boxes::push_actors(actors, state, asset_manager, alpha_multiplier);
}

#[inline(always)]
pub fn get_actors(
    state: &State,
    asset_manager: &AssetManager,
    alpha_multiplier: f32,
) -> Vec<Actor> {
    profile_boxes::get_actors(state, asset_manager, alpha_multiplier)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SimplyLoveProfileRequest, SimplyLoveRuntimeRequest};
    use deadsync_core::input::InputSource;
    use deadsync_theme::AudioRequest;
    use std::time::Instant;

    fn press(action: deadsync_input::VirtualAction) -> InputEvent {
        let now = Instant::now();
        InputEvent {
            action,
            input_slot: 0,
            pressed: true,
            source: InputSource::Keyboard,
            timestamp: now,
            timestamp_host_nanos: 0,
            stored_at: now,
            emitted_at: now,
        }
    }

    #[test]
    fn standalone_consumer_preserves_start_sound_before_profile_selection() {
        let mut state = init();
        set_joined(&mut state, true, false);

        let ThemeEffect::Batch(effects) =
            handle_input(&mut state, &press(deadsync_input::VirtualAction::p1_start))
        else {
            panic!("profile confirmation should return an ordered effect batch");
        };
        assert_eq!(effects.len(), 2);
        assert!(matches!(
            &effects[0],
            ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Audio(AudioRequest::PlaySfx(path)))
                if path == "assets/sounds/start.ogg"
        ));
        assert!(matches!(
            &effects[1],
            ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Profile(
                SimplyLoveProfileRequest::Select {
                    p1_joined: true,
                    p2_joined: false,
                    ..
                }
            ))
        ));
    }
}
