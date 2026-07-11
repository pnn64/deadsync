//! Shell interaction state shared across input, navigation, and overlays.

use std::time::Instant;

use deadsync_config::frame_pacing::apply_tab_acceleration;
use deadsync_theme_simply_love::screens::SimplyLoveScreen as Screen;
use winit::keyboard::KeyCode;

use crate::{
    Command,
    navigation::{TransitionState, menu_exit_uses_fade},
};

const MESSAGE_HOLD_SECONDS: f32 = 3.33;
const MESSAGE_FADE_SECONDS: f32 = 0.25;
const MESSAGE_TOTAL_SECONDS: f32 = MESSAGE_HOLD_SECONDS + MESSAGE_FADE_SECONDS;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExitIntent {
    #[default]
    None,
    Exit,
    Shutdown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessExitRequest {
    Exit,
    Shutdown,
}

pub enum ProcessExitPlan {
    BeginFade,
    Execute(Command),
}

/// Held modifier and non-gameplay time-control state.
#[derive(Clone, Copy, Debug)]
pub struct HeldControls {
    shift: bool,
    ctrl: bool,
    alt: bool,
    fast_forward: bool,
    slow_down: bool,
    tab_acceleration_enabled: bool,
}

impl HeldControls {
    pub const fn new(tab_acceleration_enabled: bool) -> Self {
        Self {
            shift: false,
            ctrl: false,
            alt: false,
            fast_forward: false,
            slow_down: false,
            tab_acceleration_enabled,
        }
    }

    pub fn update_modifier(&mut self, code: KeyCode, pressed: bool) {
        match code {
            KeyCode::ShiftLeft | KeyCode::ShiftRight => self.shift = pressed,
            KeyCode::ControlLeft | KeyCode::ControlRight => self.ctrl = pressed,
            KeyCode::AltLeft | KeyCode::AltRight => self.alt = pressed,
            _ => {}
        }
    }

    pub fn clear(&mut self) {
        self.shift = false;
        self.ctrl = false;
        self.alt = false;
        self.fast_forward = false;
        self.slow_down = false;
    }

    #[inline(always)]
    pub const fn shift(&self) -> bool {
        self.shift
    }

    #[inline(always)]
    pub const fn ctrl(&self) -> bool {
        self.ctrl
    }

    #[inline(always)]
    pub const fn alt(&self) -> bool {
        self.alt
    }

    #[inline(always)]
    pub fn set_fast_forward(&mut self, pressed: bool) {
        self.fast_forward = pressed;
    }

    #[inline(always)]
    pub fn set_slow_down(&mut self, pressed: bool) {
        self.slow_down = pressed;
    }

    pub fn logic_delta(&self, delta_time: f32, acceleration_allowed: bool) -> f32 {
        apply_tab_acceleration(
            delta_time,
            acceleration_allowed,
            self.fast_forward,
            self.slow_down,
            self.tab_acceleration_enabled,
        )
    }
}

/// Process-lifetime interaction state owned by the shell/game thread.
pub struct ShellInteractionState {
    controls: HeldControls,
    message: Option<(String, Instant)>,
    exit_intent: ExitIntent,
}

impl ShellInteractionState {
    pub const fn new(tab_acceleration_enabled: bool) -> Self {
        Self {
            controls: HeldControls::new(tab_acceleration_enabled),
            message: None,
            exit_intent: ExitIntent::None,
        }
    }

    #[inline(always)]
    pub fn controls(&self) -> &HeldControls {
        &self.controls
    }

    #[inline(always)]
    pub fn controls_mut(&mut self) -> &mut HeldControls {
        &mut self.controls
    }

    pub fn show_message(&mut self, message: String, now: Instant) {
        self.message = Some((message, now));
    }

    pub fn update_message(&mut self, now: Instant) {
        if self.message.as_ref().is_some_and(|(_, started)| {
            now.duration_since(*started).as_secs_f32() > MESSAGE_TOTAL_SECONDS
        }) {
            self.message = None;
        }
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_ref().map(|(message, _)| message.as_str())
    }

    #[inline(always)]
    pub fn clear_exit_intent(&mut self) {
        self.exit_intent = ExitIntent::None;
    }

    pub fn plan_process_exit(
        &mut self,
        request: ProcessExitRequest,
        screen: Screen,
        transition: &TransitionState,
    ) -> ProcessExitPlan {
        if menu_exit_uses_fade(screen, transition) {
            self.exit_intent = match request {
                ProcessExitRequest::Exit => ExitIntent::Exit,
                ProcessExitRequest::Shutdown => ExitIntent::Shutdown,
            };
            ProcessExitPlan::BeginFade
        } else {
            ProcessExitPlan::Execute(match request {
                ProcessExitRequest::Exit => Command::ExitNow,
                ProcessExitRequest::Shutdown => Command::Shutdown,
            })
        }
    }

    #[inline(always)]
    pub const fn exit_intent(&self) -> ExitIntent {
        self.exit_intent
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn held_controls_track_modifiers_and_clear_on_focus_loss() {
        let mut controls = HeldControls::new(true);
        controls.update_modifier(KeyCode::ShiftLeft, true);
        controls.update_modifier(KeyCode::ControlRight, true);
        controls.update_modifier(KeyCode::AltLeft, true);
        assert!(controls.shift() && controls.ctrl() && controls.alt());
        controls.clear();
        assert!(!controls.shift() && !controls.ctrl() && !controls.alt());
    }

    #[test]
    fn transient_message_expires_after_hold_and_fade() {
        let start = Instant::now();
        let mut state = ShellInteractionState::new(false);
        state.show_message("connected".to_string(), start);
        state.update_message(start + Duration::from_secs_f32(MESSAGE_TOTAL_SECONDS));
        assert_eq!(state.message(), Some("connected"));
        state.update_message(start + Duration::from_secs_f32(MESSAGE_TOTAL_SECONDS + 0.01));
        assert_eq!(state.message(), None);
    }

    #[test]
    fn menu_exit_begins_fade_and_latches_intent() {
        let mut state = ShellInteractionState::new(false);
        let plan = state.plan_process_exit(
            ProcessExitRequest::Exit,
            Screen::Menu,
            &TransitionState::Idle,
        );
        assert!(matches!(plan, ProcessExitPlan::BeginFade));
        assert_eq!(state.exit_intent(), ExitIntent::Exit);

        let plan = state.plan_process_exit(
            ProcessExitRequest::Shutdown,
            Screen::Menu,
            &TransitionState::Idle,
        );
        assert!(matches!(plan, ProcessExitPlan::BeginFade));
        assert_eq!(state.exit_intent(), ExitIntent::Shutdown);
        state.clear_exit_intent();
        assert_eq!(state.exit_intent(), ExitIntent::None);
    }

    #[test]
    fn immediate_exit_requests_return_effect_commands() {
        let mut state = ShellInteractionState::new(false);
        let exit = state.plan_process_exit(
            ProcessExitRequest::Exit,
            Screen::Gameplay,
            &TransitionState::Idle,
        );
        assert!(matches!(exit, ProcessExitPlan::Execute(Command::ExitNow)));

        let shutdown = state.plan_process_exit(
            ProcessExitRequest::Shutdown,
            Screen::Menu,
            &TransitionState::FadingIn {
                elapsed: 0.0,
                duration: 0.5,
            },
        );
        assert!(matches!(
            shutdown,
            ProcessExitPlan::Execute(Command::Shutdown)
        ));
        assert_eq!(state.exit_intent(), ExitIntent::None);
    }
}
