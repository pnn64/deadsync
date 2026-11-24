use std::sync::Arc;
use std::time::{Duration, Instant};

use deadlib_present::actors::Actor;
use deadsync_theme::select_music::{SelectCourseView, SelectMusicView};
use deadsync_theme::selection::{ProfilePickerRenderer, SelectionPresentation, WheelMediaFrame};
use deadsync_theme::views::PlaySelectionView;

use crate::play_selection::{PlaySelectionCarousel, push_profile_load_actors};
use crate::profile_picker::push_profile_picker_actors;
use crate::select_music::{
    WHEEL_BANNER_FADE, WHEEL_TO_PLAYER_OPTIONS_SECONDS, WheelBannerMotion,
    push_select_course_actors, push_select_music_actors,
};

#[derive(Default)]
pub struct OpenItgSelectionPresentation {
    banner: WheelBannerMotion,
    carousel: PlaySelectionCarousel,
}

impl SelectionPresentation<Actor> for OpenItgSelectionPresentation {
    fn show_random_entries(&self) -> bool {
        true
    }

    fn banner_crossfade(&self) -> Option<Duration> {
        Some(WHEEL_BANNER_FADE)
    }

    fn player_options_exit_seconds(&self) -> Option<f32> {
        Some(WHEEL_TO_PLAYER_OPTIONS_SECONDS)
    }

    fn enter_wheel(&mut self, now: Instant) {
        self.banner.enter(now);
    }

    fn confirm_wheel(&mut self, now: Instant) {
        self.banner.confirm(now);
    }

    fn cancel_wheel_confirmation(&mut self) {
        self.banner.cancel_confirmation();
    }

    fn enter_play_selection(&mut self) {
        self.carousel = PlaySelectionCarousel::default();
    }

    fn push_music(
        &self,
        actors: &mut Vec<Actor>,
        view: SelectMusicView<'_>,
        media: WheelMediaFrame<'_>,
    ) {
        let banner = self
            .banner
            .frame(media.now, media.outgoing, media.exit_elapsed);
        push_select_music_actors(actors, view, banner);
    }

    fn push_course(
        &self,
        actors: &mut Vec<Actor>,
        view: SelectCourseView<'_>,
        media: WheelMediaFrame<'_>,
    ) {
        let banner = self
            .banner
            .frame(media.now, media.outgoing, media.exit_elapsed);
        push_select_course_actors(actors, view, banner);
    }

    fn profile_picker(&self) -> ProfilePickerRenderer<Actor> {
        push_profile_picker_actors
    }

    fn push_play_selection(
        &mut self,
        actors: &mut Vec<Actor>,
        view: PlaySelectionView<'_>,
        now: f64,
    ) {
        self.carousel.push_actors(actors, view, now);
    }

    fn push_profile_load(&self, actors: &mut Vec<Actor>, label: &Arc<str>, elapsed: f32) {
        push_profile_load_actors(actors, label, elapsed);
    }
}
