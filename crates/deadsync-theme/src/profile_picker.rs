//! Presentation interface for the profile selection screen.

use crate::{ThemeEffect, ThemeTransitions};
use deadsync_core::input::MAX_PLAYERS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfilePickerContext {
    pub color_index: i32,
    pub joined: Option<[bool; MAX_PLAYERS]>,
    pub fast_switch: bool,
}

pub trait ThemeProfilePicker: ThemeTransitions {
    type ProfilePickerState;
    type ProfilePickerView;
    type ProfilePickerInput;
    type ProfilePickerAssets;
    type ProfilePickerVisualPolicy;

    fn init_profile_picker(
        view: Self::ProfilePickerView,
        context: ProfilePickerContext,
    ) -> Self::ProfilePickerState;
    fn profile_picker_color_index(state: &Self::ProfilePickerState) -> i32;
    fn set_profile_picker_color_index(state: &mut Self::ProfilePickerState, color_index: i32);
    fn update_profile_picker(state: &mut Self::ProfilePickerState, delta_time: f32);
    fn handle_profile_picker_input(
        state: &mut Self::ProfilePickerState,
        input: &Self::ProfilePickerInput,
        effects: &mut Vec<ThemeEffect<Self::Screen, Self::RuntimeRequest>>,
    );
    fn push_profile_picker_actors(
        actors: &mut Vec<Self::Actor>,
        state: &Self::ProfilePickerState,
        assets: &Self::ProfilePickerAssets,
        alpha_multiplier: f32,
        visual_policy: Self::ProfilePickerVisualPolicy,
    );
}
