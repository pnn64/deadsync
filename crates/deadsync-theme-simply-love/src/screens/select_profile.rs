use crate::SimplyLoveEffect as ThemeEffect;
use crate::screens::components::shared::profile_boxes;
use crate::views::ProfilePickerView;
use deadlib_assets::AssetManager;
use deadlib_present::actors::Actor;
use deadsync_input::InputEvent;
use deadsync_profile as profile_data;
use deadsync_theme::profile_picker::{
    ProfilePickerContext, ProfilePickerScreenView, ThemeProfilePicker,
};

pub type State = profile_boxes::State;

#[inline(always)]
#[must_use]
pub const fn exit_anim_duration() -> f32 {
    profile_boxes::exit_anim_duration()
}

#[inline(always)]
#[must_use]
pub fn init(view: ProfilePickerView) -> State {
    profile_boxes::init(view)
}

#[inline(always)]
pub fn set_joined(state: &mut State, p1_joined: bool, p2_joined: bool) {
    profile_boxes::set_joined(state, p1_joined, p2_joined);
}

#[inline(always)]
pub const fn set_fast_switch(state: &mut State, enabled: bool) {
    profile_boxes::set_fast_switch(state, enabled);
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
#[must_use]
pub fn in_transition() -> (Vec<Actor>, f32) {
    profile_boxes::in_transition()
}

#[inline(always)]
#[must_use]
pub fn out_transition() -> (Vec<Actor>, f32) {
    profile_boxes::out_transition()
}

pub fn handle_input(state: &mut State, ev: &InputEvent, effects: &mut Vec<ThemeEffect>) {
    if let profile_boxes::InputOutcome::Cancelled(screen) =
        profile_boxes::handle_input(state, ev, effects)
    {
        effects.push(ThemeEffect::Navigate(screen));
    }
}

#[inline(always)]
pub fn push_actors(
    actors: &mut Vec<Actor>,
    state: &State,
    asset_manager: &AssetManager,
    alpha_multiplier: f32,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) {
    profile_boxes::push_actors(
        actors,
        state,
        asset_manager,
        alpha_multiplier,
        visual_policy,
    );
}

#[inline(always)]
pub fn get_actors(
    state: &State,
    asset_manager: &AssetManager,
    alpha_multiplier: f32,
) -> Vec<Actor> {
    profile_boxes::get_actors(state, asset_manager, alpha_multiplier)
}

impl ThemeProfilePicker for crate::SimplyLoveTheme {
    type ProfilePickerState = State;
    type ProfilePickerView = ProfilePickerView;
    type ProfilePickerInput = InputEvent;
    type ProfilePickerAssets = AssetManager;
    type ProfilePickerVisualPolicy = crate::views::SimplyLoveVisualPolicyView;

    fn init_profile_picker(view: ProfilePickerView, context: ProfilePickerContext) -> State {
        let mut state = init(view);
        state.active_color_index = context.color_index;
        set_fast_switch(&mut state, context.fast_switch);
        if let Some([p1_joined, p2_joined]) = context.joined {
            set_joined(&mut state, p1_joined, p2_joined);
        }
        state
    }

    fn profile_picker_color_index(state: &State) -> i32 {
        state.active_color_index
    }

    fn set_profile_picker_color_index(state: &mut State, color_index: i32) {
        state.active_color_index = color_index;
    }

    fn update_profile_picker(state: &mut State, delta_time: f32) {
        update(state, delta_time);
    }

    fn handle_profile_picker_input(
        state: &mut State,
        input: &InputEvent,
        effects: &mut Vec<ThemeEffect>,
    ) {
        handle_input(state, input, effects);
    }

    fn with_profile_picker_view(
        state: &State,
        alpha: f32,
        draw: impl FnOnce(ProfilePickerScreenView<'_>),
    ) {
        profile_boxes::with_screen_view(state, alpha, draw);
    }

    fn push_profile_picker_actors(
        actors: &mut Vec<Actor>,
        state: &State,
        assets: &AssetManager,
        alpha_multiplier: f32,
        visual_policy: Self::ProfilePickerVisualPolicy,
    ) {
        push_actors(actors, state, assets, alpha_multiplier, visual_policy);
    }
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
        use crate::SimplyLoveTheme as Theme;

        let mut state = Theme::init_profile_picker(
            ProfilePickerView::default(),
            ProfilePickerContext {
                color_index: 7,
                joined: Some([false, true]),
                fast_switch: false,
            },
        );
        assert_eq!(Theme::profile_picker_color_index(&state), 7);

        let mut effects = Vec::new();
        Theme::handle_profile_picker_input(
            &mut state,
            &press(deadsync_input::VirtualAction::p2_start),
            &mut effects,
        );
        assert_eq!(effects.len(), 2);
        assert!(matches!(
            &effects[0],
            ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Audio(AudioRequest::PlaySfx(path)))
                if *path == "assets/sounds/start.ogg"
        ));
        assert!(matches!(
            &effects[1],
            ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Profile(
                SimplyLoveProfileRequest::Select {
                    p1_joined: false,
                    p2_joined: true,
                    fast_switch: false,
                    ..
                }
            ))
        ));
    }

    #[test]
    fn fast_switch_is_explicit_in_selection_and_cancel_effects() {
        let mut state = init(ProfilePickerView::default());
        set_fast_switch(&mut state, true);
        set_joined(&mut state, true, false);

        let mut effects = Vec::new();
        handle_input(
            &mut state,
            &press(deadsync_input::VirtualAction::p1_start),
            &mut effects,
        );
        assert!(matches!(
            &effects[1],
            ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Profile(
                SimplyLoveProfileRequest::Select {
                    fast_switch: true,
                    ..
                }
            ))
        ));

        let mut state = init(ProfilePickerView::default());
        set_fast_switch(&mut state, true);
        set_joined(&mut state, false, false);
        let mut effects = Vec::new();
        handle_input(
            &mut state,
            &press(deadsync_input::VirtualAction::p1_back),
            &mut effects,
        );
        assert!(matches!(
            effects.as_slice(),
            [ThemeEffect::Navigate(crate::screens::Screen::SelectMusic)]
        ));

        let mut state = init(ProfilePickerView::default());
        set_joined(&mut state, false, false);
        let mut effects = Vec::new();
        handle_input(
            &mut state,
            &press(deadsync_input::VirtualAction::p1_back),
            &mut effects,
        );
        assert!(matches!(
            effects.as_slice(),
            [ThemeEffect::Navigate(crate::screens::Screen::Menu)]
        ));
    }
}
