use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::profile_picker::ProfilePickerScreenView;
use crate::select_music::{
    SelectCourseView, SelectMusicView, WHEEL_OUTGOING_BANNERS, WheelBannerFade,
};
use crate::views::PlaySelectionView;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaySelectionMode {
    Regular,
    Course,
    Free,
}

/// An authored entry into the shared play flow. An unavailable outcome remains
/// visible so a presentation can retain its layout without offering a false action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayModeMenuChoice {
    pub label: &'static str,
    pub mode: Option<PlaySelectionMode>,
    pub difficulty_index: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayModeMenu {
    pub choices: &'static [PlayModeMenuChoice],
}

pub type ProfilePickerRenderer<A> = fn(&mut Vec<A>, ProfilePickerScreenView<'_>, bool);

pub struct WheelMediaFrame<'a> {
    pub now: Instant,
    pub outgoing: [Option<WheelBannerFade<'a>>; WHEEL_OUTGOING_BANNERS],
    pub exit_elapsed: f32,
}

pub trait MusicWheelPresentation<A> {
    fn show_random_entries(&self) -> bool;
    fn banner_crossfade(&self) -> Option<Duration>;
    fn player_options_exit_seconds(&self) -> Option<f32>;
    fn enter_wheel(&mut self, now: Instant);
    fn confirm_wheel(&mut self, now: Instant);
    fn cancel_wheel_confirmation(&mut self);
    fn push_music(
        &self,
        actors: &mut Vec<A>,
        view: SelectMusicView<'_>,
        media: WheelMediaFrame<'_>,
    );
}

pub trait SelectionPresentation<A>: MusicWheelPresentation<A> {
    fn play_mode_menu(&self) -> Option<&'static PlayModeMenu> {
        None
    }
    fn enter_play_selection(&mut self);
    fn push_course(
        &self,
        actors: &mut Vec<A>,
        view: SelectCourseView<'_>,
        media: WheelMediaFrame<'_>,
    );
    fn profile_picker(&self) -> ProfilePickerRenderer<A>;
    fn push_play_selection(&mut self, actors: &mut Vec<A>, view: PlaySelectionView<'_>, now: f64);
    fn push_profile_load(&self, actors: &mut Vec<A>, label: &Arc<str>, elapsed: f32);
}
