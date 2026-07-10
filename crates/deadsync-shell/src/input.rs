use crate::TransitionState;
use deadsync_gameplay::RawKeyAction;
use deadsync_input::{PadEvent, RawKeyboardEvent};
use deadsync_input_native::GpSystemEvent;
use deadsync_screens::Screen;

/// Events forwarded from platform input backends into the application loop.
#[derive(Debug, Clone)]
pub enum UserEvent {
    Pad(PadEvent),
    Key(RawKeyboardEvent),
    GamepadSystem(GpSystemEvent),
}

#[inline(always)]
pub fn screen_accepts_queued_input(screen: Screen, transition: &TransitionState) -> bool {
    deadsync_config::frame_pacing::queued_input_allowed(
        screen == Screen::Gameplay,
        matches!(transition, TransitionState::Idle),
        matches!(transition, TransitionState::FadingIn { .. }),
    )
}

#[inline(always)]
pub const fn raw_keyboard_restart_screen(screen: Screen) -> bool {
    matches!(screen, Screen::Gameplay | Screen::Evaluation)
}

#[inline(always)]
pub fn gameplay_dispatch_continues(
    start_screen: Screen,
    current_screen: Screen,
    transition: &TransitionState,
) -> bool {
    current_screen == start_screen && screen_accepts_queued_input(current_screen, transition)
}

#[inline(always)]
pub fn raw_keyboard_capture_enabled(
    accepts_live_input: bool,
    screen: Screen,
    transition: &TransitionState,
    gameplay_only: bool,
) -> bool {
    accepts_live_input
        && (!gameplay_only
            || (raw_keyboard_restart_screen(screen)
                && screen_accepts_queued_input(screen, transition)))
}

#[inline(always)]
pub const fn allowed_gameplay_raw_action(
    action: RawKeyAction,
    keyboard_features: bool,
    course_active: bool,
) -> Option<RawKeyAction> {
    if keyboard_features
        && !course_active
        && matches!(action, RawKeyAction::Restart | RawKeyAction::Reload)
    {
        Some(action)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_input_routes_during_gameplay_fade_in() {
        let transition = TransitionState::FadingIn {
            elapsed: 0.0,
            duration: 2.0,
        };

        assert!(screen_accepts_queued_input(Screen::Gameplay, &transition));
        assert!(!screen_accepts_queued_input(
            Screen::SelectMusic,
            &transition
        ));
    }

    #[test]
    fn queued_input_stays_blocked_during_gameplay_fade_out() {
        let transition = TransitionState::FadingOut {
            elapsed: 0.0,
            duration: 0.5,
            target: Screen::Evaluation,
        };

        assert!(!screen_accepts_queued_input(Screen::Gameplay, &transition));
    }

    #[test]
    fn dispatch_requires_the_same_input_accepting_screen() {
        assert!(gameplay_dispatch_continues(
            Screen::Gameplay,
            Screen::Gameplay,
            &TransitionState::Idle,
        ));
        assert!(!gameplay_dispatch_continues(
            Screen::Gameplay,
            Screen::Evaluation,
            &TransitionState::Idle,
        ));
    }

    #[test]
    fn capture_policy_preserves_platform_scope() {
        assert!(raw_keyboard_capture_enabled(
            true,
            Screen::SelectMusic,
            &TransitionState::Idle,
            false,
        ));
        assert!(!raw_keyboard_capture_enabled(
            true,
            Screen::SelectMusic,
            &TransitionState::Idle,
            true,
        ));
        assert!(raw_keyboard_capture_enabled(
            true,
            Screen::Gameplay,
            &TransitionState::Idle,
            true,
        ));
    }

    #[test]
    fn gameplay_shortcuts_require_features_and_non_course_play() {
        assert_eq!(
            allowed_gameplay_raw_action(RawKeyAction::Restart, true, false),
            Some(RawKeyAction::Restart),
        );
        assert_eq!(
            allowed_gameplay_raw_action(RawKeyAction::Reload, false, false),
            None,
        );
        assert_eq!(
            allowed_gameplay_raw_action(RawKeyAction::Restart, true, true),
            None,
        );
    }
}
