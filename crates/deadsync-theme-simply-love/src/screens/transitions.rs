use deadlib_assets::AssetView;
use deadlib_present::actors::Actor;
use deadsync_theme::{ThemeTransitions, TransitionKind, TransitionState};

use super::{
    Screen, arrowcloud_login, content_browser, credits, evaluation, evaluation_summary, gameover,
    gameplay, groovestats_login, init, initials, input as input_screen, manage_local_profiles,
    mappings, menu, options, overscan_adjustment, pad_config, player_options, profile_load,
    sandbox, select_color, select_course, select_mode, select_music, select_profile, select_style,
    smx_assign, test_lights, uses_actor_fade, uses_actor_only_transition,
};
use crate::SimplyLoveTheme;
use crate::views::SimplyLoveVisualPolicyView;

const ACTOR_ENTRY_DURATION: f32 = 0.65;
const DEFAULT_ACTOR_EXIT_DURATION: f32 = 0.4;

pub struct PlayEntryTransition<'a> {
    pub gameplay: Option<&'a gameplay::State>,
    pub assets: &'a dyn AssetView,
    pub restart: bool,
    pub policy: SimplyLoveVisualPolicyView,
}

impl ThemeTransitions for SimplyLoveTheme {
    type Actor = Actor;
    type EntryContext<'a> = Option<PlayEntryTransition<'a>>;

    fn transition_kind(from: Screen, to: Screen) -> TransitionKind {
        if (from, to) == (Screen::Init, Screen::Menu) {
            TransitionKind::Direct
        } else if uses_actor_only_transition(from, to) {
            TransitionKind::Actors
        } else {
            TransitionKind::Global
        }
    }

    fn actor_entry_duration(screen: Screen) -> Option<f32> {
        uses_actor_fade(screen).then_some(ACTOR_ENTRY_DURATION)
    }

    fn actor_exit_duration(screen: Screen) -> f32 {
        match screen {
            Screen::Menu => menu::out_transition().1,
            Screen::SelectColor => select_color::exit_anim_duration(),
            Screen::SelectProfile => select_profile::exit_anim_duration(),
            _ => DEFAULT_ACTOR_EXIT_DURATION,
        }
    }

    fn skip_global_entry_fade(from: Screen, to: Screen) -> bool {
        matches!(
            (from, to),
            (Screen::Options, Screen::Credits) | (Screen::Credits, Screen::Options)
        )
    }

    fn out_transition(screen: Screen, target: Screen) -> (Vec<Actor>, f32) {
        match screen {
            Screen::Menu if target == Screen::Init => menu::cancel_transition(),
            Screen::Menu => menu::out_transition(),
            Screen::Gameplay => gameplay::out_transition(),
            Screen::Practice => gameplay::out_transition(),
            Screen::Options => options::out_transition(),
            Screen::ContentBrowser => content_browser::out_transition(),
            Screen::Credits => credits::out_transition(),
            Screen::ManageLocalProfiles => manage_local_profiles::out_transition(),
            Screen::Mappings => mappings::out_transition(),
            Screen::TestLights => test_lights::out_transition(),
            Screen::OverscanAdjustment => overscan_adjustment::out_transition(),
            Screen::PlayerOptions => player_options::out_transition(),
            Screen::SelectProfile => select_profile::out_transition(),
            Screen::SelectColor => select_color::out_transition(),
            Screen::ArrowCloudLogin => arrowcloud_login::out_transition(),
            Screen::GrooveStatsLogin => groovestats_login::out_transition(),
            Screen::SelectStyle => select_style::out_transition(),
            Screen::SelectPlayMode => select_mode::out_transition(),
            Screen::ProfileLoad => profile_load::out_transition(),
            Screen::SelectMusic => select_music::out_transition(),
            Screen::SelectCourse => select_course::out_transition(),
            Screen::Sandbox => sandbox::out_transition(),
            Screen::Init => init::out_transition(),
            Screen::Evaluation => evaluation::out_transition(),
            Screen::EvaluationSummary => evaluation_summary::out_transition(),
            Screen::Initials => initials::out_transition(),
            Screen::GameOver => gameover::out_transition(),
            Screen::Input => input_screen::out_transition(),
            Screen::ConfigurePads => pad_config::out_transition(),
            Screen::SmxAssignPads => smx_assign::out_transition(),
        }
    }

    fn in_transition(screen: Screen, context: Self::EntryContext<'_>) -> (Vec<Actor>, f32) {
        match screen {
            Screen::Menu => menu::in_transition(),
            Screen::Gameplay | Screen::Practice => {
                let play = context.expect("play transition requires gameplay presentation inputs");
                gameplay::in_transition(play.gameplay, play.assets, play.restart, play.policy)
            }
            Screen::Options => options::in_transition(),
            Screen::ContentBrowser => content_browser::in_transition(),
            Screen::Credits => credits::in_transition(),
            Screen::ManageLocalProfiles => manage_local_profiles::in_transition(),
            Screen::Mappings => mappings::in_transition(),
            Screen::TestLights => test_lights::in_transition(),
            Screen::OverscanAdjustment => overscan_adjustment::in_transition(),
            Screen::PlayerOptions => player_options::in_transition(),
            Screen::SelectProfile => select_profile::in_transition(),
            Screen::SelectColor => select_color::in_transition(),
            Screen::ArrowCloudLogin => arrowcloud_login::in_transition(),
            Screen::GrooveStatsLogin => groovestats_login::in_transition(),
            Screen::SelectStyle => select_style::in_transition(),
            Screen::SelectPlayMode => select_mode::in_transition(),
            Screen::ProfileLoad => profile_load::in_transition(),
            Screen::SelectMusic => select_music::in_transition(),
            Screen::SelectCourse => select_course::in_transition(),
            Screen::Sandbox => sandbox::in_transition(),
            Screen::Evaluation => evaluation::in_transition(),
            Screen::EvaluationSummary => evaluation_summary::in_transition(),
            Screen::Initials => initials::in_transition(),
            Screen::GameOver => gameover::in_transition(),
            Screen::Input => input_screen::in_transition(),
            Screen::ConfigurePads => pad_config::in_transition(),
            Screen::SmxAssignPads => smx_assign::in_transition(),
            Screen::Init => (vec![], 0.0),
        }
    }
}

pub struct ScreenFadeView {
    pub screen_alpha: f32,
    pub menu_entry_elapsed: Option<f32>,
    pub menu_exit_elapsed: Option<f32>,
}

#[inline]
pub fn screen_fade_view(screen: Screen, transition: &TransitionState<Screen>) -> ScreenFadeView {
    let mut view = ScreenFadeView {
        screen_alpha: 1.0,
        menu_entry_elapsed: None,
        menu_exit_elapsed: None,
    };
    if screen == Screen::Menu {
        match transition {
            TransitionState::ActorsFadeIn { elapsed, .. }
            | TransitionState::FadingIn { elapsed, .. } => {
                view.menu_entry_elapsed = Some(*elapsed);
            }
            TransitionState::ActorsFadeOut { elapsed, .. } => {
                view.menu_exit_elapsed = Some(*elapsed)
            }
            TransitionState::FadingOut {
                elapsed, target, ..
            } if *target != Screen::Init => {
                view.menu_exit_elapsed = Some(*elapsed);
            }
            _ => {}
        }
    } else if uses_actor_fade(screen) {
        match transition {
            TransitionState::ActorsFadeIn { elapsed, duration } => {
                view.screen_alpha = (elapsed / duration).clamp(0.0, 1.0);
            }
            TransitionState::ActorsFadeOut {
                elapsed, duration, ..
            } => {
                view.screen_alpha = 1.0 - (elapsed / duration).clamp(0.0, 1.0);
            }
            _ => {}
        }
    }
    view
}

#[inline]
pub fn gameplay_smx_overlay_alpha(
    hide_song_intro: bool,
    transition: &TransitionState<Screen>,
) -> f32 {
    match *transition {
        TransitionState::FadingIn { .. } if hide_song_intro => 1.0,
        TransitionState::FadingIn { elapsed, duration } => {
            if duration <= gameplay::TRANSITION_IN_RESTART_DURATION + 0.01 {
                (elapsed / duration).clamp(0.0, 1.0)
            } else {
                let fade_start = duration - gameplay::TRANSITION_IN_BLACK_FADE_DURATION;
                ((elapsed - fade_start) / gameplay::TRANSITION_IN_BLACK_FADE_DURATION)
                    .clamp(0.0, 1.0)
            }
        }
        TransitionState::FadingOut { elapsed, .. } => {
            1.0 - ((elapsed - gameplay::TRANSITION_OUT_DELAY)
                / gameplay::TRANSITION_OUT_FADE_DURATION)
                .clamp(0.0, 1.0)
        }
        _ => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_cancel_uses_the_global_cover_without_fading_menu_actors() {
        let fade = |target| {
            screen_fade_view(
                Screen::Menu,
                &TransitionState::FadingOut {
                    elapsed: 0.5,
                    duration: 1.0,
                    target,
                },
            )
        };
        assert_eq!(fade(Screen::Init).menu_exit_elapsed, None);
        assert_eq!(fade(Screen::Menu).menu_exit_elapsed, Some(0.5));
    }

    #[test]
    fn actor_fade_alpha_uses_the_clock_duration() {
        let transition = TransitionState::ActorsFadeIn {
            elapsed: 0.2,
            duration: 0.8,
        };
        let fade = screen_fade_view(Screen::Options, &transition);
        assert_eq!(fade.screen_alpha, 0.25);
        let menu = screen_fade_view(Screen::Menu, &transition);
        assert_eq!(menu.screen_alpha, 1.0);
        assert_eq!(menu.menu_entry_elapsed, Some(0.2));
    }
}
