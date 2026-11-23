use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::profile_picker::ProfilePickerScreenView;
use crate::select_music::{
    SelectCourseView, SelectMusicView, WHEEL_OUTGOING_BANNERS, WheelBannerFade,
};
use crate::views::PlaySelectionView;

pub type ProfilePickerRenderer<A> = fn(&mut Vec<A>, ProfilePickerScreenView<'_>, bool);

pub struct WheelMediaFrame<'a> {
    pub now: Instant,
    pub outgoing: [Option<WheelBannerFade<'a>>; WHEEL_OUTGOING_BANNERS],
    pub exit_elapsed: f32,
}

pub trait SelectionPresentation<A> {
    fn show_random_entries(&self) -> bool;
    fn banner_crossfade(&self) -> Option<Duration>;
    fn player_options_exit_seconds(&self) -> Option<f32>;
    fn enter_wheel(&mut self, now: Instant);
    fn confirm_wheel(&mut self, now: Instant);
    fn cancel_wheel_confirmation(&mut self);
    fn enter_play_selection(&mut self);

    fn push_music(
        &self,
        actors: &mut Vec<A>,
        view: SelectMusicView<'_>,
        media: WheelMediaFrame<'_>,
    );
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
