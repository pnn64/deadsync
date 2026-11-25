//! Presentation interface for the profile selection screen.

use crate::{ThemeEffect, ThemeTransitions};
use deadsync_core::input::MAX_PLAYERS;
use std::sync::Arc;

pub const PROFILE_PICKER_VISIBLE_ROWS: usize = 5;

/// Borrowed profile names and status for one player panel.
pub struct ProfilePickerPanel<'a> {
    pub joined: bool,
    pub ready: bool,
    pub names: [Option<&'a Arc<str>>; PROFILE_PICKER_VISIBLE_ROWS],
    pub avatar: Option<&'a Arc<str>>,
    pub songs_played: Option<&'a Arc<str>>,
    pub invalid_choice: bool,
}

/// Presentation data for the standalone picker and the music wheel overlay.
pub struct ProfilePickerScreenView<'a> {
    pub title: &'a Arc<str>,
    pub join_text: &'a Arc<str>,
    pub waiting_text: &'a Arc<str>,
    pub panels: [ProfilePickerPanel<'a>; MAX_PLAYERS],
    pub alpha: f32,
}

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
    type ProfilePickerAssets: ?Sized;
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
    fn with_profile_picker_view(
        state: &Self::ProfilePickerState,
        alpha: f32,
        draw: impl FnOnce(ProfilePickerScreenView<'_>),
    );
    fn push_profile_picker_actors(
        actors: &mut Vec<Self::Actor>,
        state: &Self::ProfilePickerState,
        assets: &Self::ProfilePickerAssets,
        alpha_multiplier: f32,
        visual_policy: Self::ProfilePickerVisualPolicy,
    );
}

/// Shell-prepared local profile catalog used by Manage Local Profiles.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ManageLocalProfilesView {
    pub profiles: Vec<LocalProfileView>,
    pub default_profile_ids: [Option<String>; 2],
    pub dedicated_three_key_nav: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalProfileView {
    pub id: String,
    pub display_name: String,
}
