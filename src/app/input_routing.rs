use super::{App, CurrentScreen, TransitionState};
use crate::config;
use deadsync_input as logical_input;
use deadsync_input::InputEvent;
use deadsync_input::RawKeyboardEvent;
use std::error::Error;
use std::time::Instant;
use winit::event_loop::ActiveEventLoop;

#[derive(Clone, Copy, Debug)]
pub(super) struct GameplayRawKeyEvent {
    pub(super) code: winit::keyboard::KeyCode,
    pub(super) pressed: bool,
    pub(super) timestamp: Instant,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum GameplayQueuedEvent {
    Input(InputEvent),
    RawKey(GameplayRawKeyEvent),
}

#[inline(always)]
pub(super) fn screen_accepts_queued_input(
    screen: CurrentScreen,
    transition: &TransitionState,
) -> bool {
    matches!(transition, TransitionState::Idle)
        || (screen == CurrentScreen::Gameplay
            && matches!(transition, TransitionState::FadingIn { .. }))
}

#[inline(always)]
pub(super) fn gameplay_raw_key_event(raw_key: &RawKeyboardEvent) -> Option<GameplayQueuedEvent> {
    use winit::keyboard::KeyCode;

    if raw_key.repeat {
        return None;
    }
    match raw_key.code {
        KeyCode::ShiftLeft
        | KeyCode::ShiftRight
        | KeyCode::ControlLeft
        | KeyCode::ControlRight
        | KeyCode::KeyR
        | KeyCode::F6
        | KeyCode::F7
        | KeyCode::F8
        | KeyCode::F11
        | KeyCode::F12 => {}
        _ => return None,
    }
    Some(GameplayQueuedEvent::RawKey(GameplayRawKeyEvent {
        code: raw_key.code,
        pressed: raw_key.pressed,
        timestamp: raw_key.timestamp,
    }))
}

impl App {
    #[inline(always)]
    pub(super) const fn raw_keyboard_restart_screen(screen: CurrentScreen) -> bool {
        matches!(screen, CurrentScreen::Gameplay | CurrentScreen::Evaluation)
    }

    pub(super) fn flush_due_input_events(
        &mut self,
        event_loop: &ActiveEventLoop,
    ) -> Result<bool, Box<dyn Error>> {
        if !screen_accepts_queued_input(
            self.state.screens.current_screen,
            &self.state.shell.transition,
        ) {
            logical_input::clear_debounce_state();
            self.lights.clear_button_pressed();
            return Ok(false);
        }
        let mut flushed = false;
        let mut err: Option<Box<dyn Error>> = None;
        let gameplay_screen = self.state.screens.current_screen == CurrentScreen::Gameplay;
        let start_screen = self.state.screens.current_screen;
        let mut discard_gameplay_batch = false;
        logical_input::drain_debounced_input_events_with(|ev| {
            flushed = true;
            if gameplay_screen {
                if discard_gameplay_batch || err.is_some() {
                    return;
                }
                if let Err(e) =
                    self.route_gameplay_event(event_loop, GameplayQueuedEvent::Input(ev))
                {
                    err = Some(e);
                    return;
                }
                if !self.gameplay_dispatch_continues(start_screen) {
                    discard_gameplay_batch = true;
                }
            } else if err.is_none()
                && let Err(e) = self.route_input_event(event_loop, ev)
            {
                err = Some(e);
            }
        });
        if let Some(e) = err {
            return Err(e);
        }
        Ok(flushed)
    }

    #[inline(always)]
    pub(super) fn gameplay_dispatch_continues(&self, start_screen: CurrentScreen) -> bool {
        self.state.screens.current_screen == start_screen
            && screen_accepts_queued_input(
                self.state.screens.current_screen,
                &self.state.shell.transition,
            )
    }

    #[inline(always)]
    pub(super) fn route_gameplay_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        ev: GameplayQueuedEvent,
    ) -> Result<(), Box<dyn Error>> {
        self.state.shell.note_gameplay_queued_input();
        match ev {
            GameplayQueuedEvent::Input(ev) => self.route_input_event(event_loop, ev),
            GameplayQueuedEvent::RawKey(ev) => {
                self.route_gameplay_raw_key_event(event_loop, ev);
                Ok(())
            }
        }
    }

    fn route_gameplay_raw_key_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        ev: GameplayRawKeyEvent,
    ) {
        if self.state.screens.current_screen != CurrentScreen::Gameplay {
            return;
        }
        let Some(gs) = self.state.screens.gameplay_state.as_mut() else {
            return;
        };
        let allow_commands = self.state.gameplay_offset_save_prompt.is_none();
        crate::game::gameplay::sync_queued_raw_modifiers(
            gs,
            self.state.shell.shift_held,
            self.state.shell.ctrl_held,
        );
        let action = crate::game::gameplay::handle_queued_raw_key(
            gs,
            ev.code,
            ev.pressed,
            ev.timestamp,
            allow_commands,
        );
        if matches!(action, crate::game::gameplay::RawKeyAction::Restart)
            && config::get().keyboard_features
            && self.state.session.course_run.is_none()
        {
            self.try_gameplay_restart(event_loop, "Ctrl+R");
        }
    }

    #[inline(always)]
    pub(super) fn sync_gameplay_input_capture(&self) {
        let capture_enabled = self.accepts_live_input();
        #[cfg(windows)]
        let capture_enabled = capture_enabled
            && Self::raw_keyboard_restart_screen(self.state.screens.current_screen)
            && screen_accepts_queued_input(
                self.state.screens.current_screen,
                &self.state.shell.transition,
            );
        deadsync_input_native::set_raw_keyboard_capture_enabled(capture_enabled);
    }

    #[inline(always)]
    pub(super) fn clear_gameplay_input_events(&self) {
        deadsync_input_native::set_raw_keyboard_capture_enabled(false);
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

        assert!(screen_accepts_queued_input(
            CurrentScreen::Gameplay,
            &transition
        ));
    }

    #[test]
    fn queued_input_stays_blocked_for_non_gameplay_fades() {
        let transition = TransitionState::FadingIn {
            elapsed: 0.0,
            duration: 1.0,
        };

        assert!(!screen_accepts_queued_input(
            CurrentScreen::SelectMusic,
            &transition
        ));
    }

    #[test]
    fn queued_input_stays_blocked_during_gameplay_fade_out() {
        let transition = TransitionState::FadingOut {
            elapsed: 0.0,
            duration: 0.5,
            target: CurrentScreen::Evaluation,
        };

        assert!(!screen_accepts_queued_input(
            CurrentScreen::Gameplay,
            &transition
        ));
    }
}
