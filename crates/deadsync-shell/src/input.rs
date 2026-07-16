use crate::TransitionState;
use deadsync_gameplay::{
    GameplayOffsetAdjustKey, GameplayRawKeyInput, GameplayRawModifierKey, RawKeyAction,
};
use deadsync_input::{PadEvent, RawKeyboardEvent, VirtualAction};
use deadsync_input_native::{GpSystemEvent, PadBackend};
use deadsync_profile::PlayerSide;
use deadsync_theme::views::GamepadSystemView;
use deadsync_theme_simply_love::screens::SimplyLoveScreen as Screen;
use std::time::Instant;
use winit::keyboard::KeyCode;

/// Events forwarded from platform input backends into the application loop.
#[derive(Debug, Clone)]
pub enum UserEvent {
    Pad(PadEvent),
    Key(RawKeyboardEvent),
    GamepadSystem(GpSystemEvent),
}

#[derive(Clone, Copy, Debug)]
pub struct GameplayRawKeyEvent {
    pub code: KeyCode,
    pub pressed: bool,
    pub timestamp: Instant,
}

#[derive(Clone, Copy, Debug)]
pub enum GameplayQueuedEvent {
    Input(deadsync_input::InputEvent),
    RawKey(GameplayRawKeyEvent),
}

#[inline(always)]
pub fn gameplay_raw_key_event(raw_key: &RawKeyboardEvent) -> Option<GameplayQueuedEvent> {
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

#[inline(always)]
const fn gameplay_raw_modifier_key(code: KeyCode) -> Option<GameplayRawModifierKey> {
    match code {
        KeyCode::ShiftLeft | KeyCode::ShiftRight => Some(GameplayRawModifierKey::Shift),
        KeyCode::ControlLeft | KeyCode::ControlRight => Some(GameplayRawModifierKey::Ctrl),
        _ => None,
    }
}

#[inline(always)]
const fn gameplay_raw_key_input(code: KeyCode) -> GameplayRawKeyInput {
    match code {
        KeyCode::KeyR => GameplayRawKeyInput::Restart,
        KeyCode::F6 => GameplayRawKeyInput::Autosync,
        KeyCode::F7 => GameplayRawKeyInput::TimingTick,
        KeyCode::F8 => GameplayRawKeyInput::Autoplay,
        KeyCode::F11 => GameplayRawKeyInput::OffsetAdjust(GameplayOffsetAdjustKey::Decrease),
        KeyCode::F12 => GameplayRawKeyInput::OffsetAdjust(GameplayOffsetAdjustKey::Increase),
        _ => GameplayRawKeyInput::Other,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GamepadSystemEventPlan {
    pub forward_to_sandbox: bool,
    pub refresh_smx_underglow: bool,
    pub log_message: Option<String>,
    pub user_message: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreScreenInputContext {
    pub screen: Screen,
    pub only_dedicated_menu_buttons: bool,
    pub evaluation_test_input_active: bool,
    pub gameplay_offset_prompt_active: bool,
    pub course_active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreScreenInputRoute {
    Dispatch,
    Consume,
    RequestScreenshot(Option<PlayerSide>),
    Restart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawKeyTextRoute {
    Ignore,
    ManageLocalProfiles,
    Options,
    SelectMusic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawKeyScreenRoute {
    None,
    Sandbox,
    Menu,
    Mappings,
    ManageLocalProfiles,
    OverscanAdjustment,
    Input,
    Options,
    SelectMusic,
    PlayerOptions,
    Practice,
    Evaluation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawPadScreenRoute {
    None,
    Sandbox,
    Mappings,
    Input,
    SelectMusic,
    Evaluation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvaluationRawKeyShortcut {
    GameplayRestart,
    GameplayReload,
    PracticeFromEvaluation,
    RetrySubmissions,
    StepCourseEvalPage(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppRawKeyShortcut {
    FrameStatsCycleAnchor,
    FrameStatsToggleStyle,
    FrameStatsToggle,
    CycleOverlayMode,
    ToggleTranslatedTitles,
}

pub fn gamepad_system_event_plan(screen: Screen, ev: &GpSystemEvent) -> GamepadSystemEventPlan {
    let forward_to_sandbox = matches!(screen, Screen::Sandbox);
    match ev {
        GpSystemEvent::StartupComplete => GamepadSystemEventPlan {
            forward_to_sandbox,
            refresh_smx_underglow: false,
            log_message: None,
            user_message: None,
        },
        GpSystemEvent::Connected {
            name,
            id,
            backend,
            initial,
            ..
        } => {
            let message = format!(
                "Connected: {} (ID: {}) via {:?}",
                name,
                usize::from(*id),
                backend
            );
            GamepadSystemEventPlan {
                forward_to_sandbox,
                refresh_smx_underglow: *backend == PadBackend::Smx,
                log_message: Some(format!(
                    "Gamepad connected: {} (ID: {}) via {:?}",
                    name,
                    usize::from(*id),
                    backend
                )),
                user_message: (!*initial).then_some(message),
            }
        }
        GpSystemEvent::Disconnected {
            name,
            id,
            backend,
            initial,
            ..
        } => {
            let message = format!(
                "Disconnected: {} (ID: {}) via {:?}",
                name,
                usize::from(*id),
                backend
            );
            GamepadSystemEventPlan {
                forward_to_sandbox,
                refresh_smx_underglow: *backend == PadBackend::Smx,
                log_message: Some(format!(
                    "Gamepad disconnected: {} (ID: {}) via {:?}",
                    name,
                    usize::from(*id),
                    backend
                )),
                user_message: (!*initial).then_some(message),
            }
        }
    }
}

const fn pad_backend_name(backend: PadBackend) -> &'static str {
    match backend {
        #[cfg(windows)]
        PadBackend::WindowsRawInput => "Windows Raw Input",
        #[cfg(windows)]
        PadBackend::WindowsWgi => "Windows Gaming Input",
        #[cfg(target_os = "linux")]
        PadBackend::LinuxEvdev => "Linux evdev",
        #[cfg(target_os = "freebsd")]
        PadBackend::FreeBsdHidraw => "FreeBSD hidraw",
        #[cfg(target_os = "freebsd")]
        PadBackend::FreeBsdEvdev => "FreeBSD evdev",
        #[cfg(target_os = "macos")]
        PadBackend::MacOsIohid => "macOS IOHID",
        PadBackend::Smx => "StepManiaX",
    }
}

pub fn gamepad_system_view(ev: &GpSystemEvent) -> GamepadSystemView {
    match ev {
        GpSystemEvent::Connected {
            name,
            id,
            vendor_id,
            product_id,
            backend,
            initial,
        } => GamepadSystemView::Connected {
            name: name.clone(),
            id: usize::from(*id),
            vendor_id: *vendor_id,
            product_id: *product_id,
            backend: pad_backend_name(*backend),
            initial: *initial,
        },
        GpSystemEvent::Disconnected {
            name,
            id,
            backend,
            initial,
        } => GamepadSystemView::Disconnected {
            name: name.clone(),
            id: usize::from(*id),
            backend: pad_backend_name(*backend),
            initial: *initial,
        },
        GpSystemEvent::StartupComplete => GamepadSystemView::StartupComplete,
    }
}

pub fn pre_screen_input_route(
    pressed: bool,
    action: VirtualAction,
    context: PreScreenInputContext,
) -> PreScreenInputRoute {
    if context.only_dedicated_menu_buttons && action.is_gameplay_arrow() {
        let allow_gameplay_arrow = matches!(
            context.screen,
            Screen::Gameplay | Screen::Practice | Screen::Input | Screen::SelectMusic
        ) || (context.screen == Screen::Evaluation
            && context.evaluation_test_input_active);
        if !allow_gameplay_arrow {
            return PreScreenInputRoute::Consume;
        }
    }

    if pressed
        && matches!(
            context.screen,
            Screen::Evaluation | Screen::EvaluationSummary
        )
        && matches!(action, VirtualAction::p1_select | VirtualAction::p2_select)
    {
        let side = match action {
            VirtualAction::p1_select => Some(PlayerSide::P1),
            VirtualAction::p2_select => Some(PlayerSide::P2),
            _ => None,
        };
        return PreScreenInputRoute::RequestScreenshot(side);
    }

    if pressed
        && matches!(context.screen, Screen::Gameplay | Screen::Evaluation)
        && !context.gameplay_offset_prompt_active
        && !context.course_active
        && matches!(
            action,
            VirtualAction::p1_restart | VirtualAction::p2_restart
        )
    {
        return PreScreenInputRoute::Restart;
    }

    PreScreenInputRoute::Dispatch
}

#[inline(always)]
pub const fn raw_key_text_route(screen: Screen) -> RawKeyTextRoute {
    match screen {
        Screen::ManageLocalProfiles => RawKeyTextRoute::ManageLocalProfiles,
        Screen::Options => RawKeyTextRoute::Options,
        Screen::SelectMusic => RawKeyTextRoute::SelectMusic,
        _ => RawKeyTextRoute::Ignore,
    }
}

#[inline(always)]
pub const fn raw_key_screen_route(screen: Screen) -> RawKeyScreenRoute {
    match screen {
        Screen::Sandbox => RawKeyScreenRoute::Sandbox,
        Screen::Menu => RawKeyScreenRoute::Menu,
        Screen::Mappings => RawKeyScreenRoute::Mappings,
        Screen::ManageLocalProfiles => RawKeyScreenRoute::ManageLocalProfiles,
        Screen::OverscanAdjustment => RawKeyScreenRoute::OverscanAdjustment,
        Screen::Input => RawKeyScreenRoute::Input,
        Screen::Options => RawKeyScreenRoute::Options,
        Screen::SelectMusic => RawKeyScreenRoute::SelectMusic,
        Screen::PlayerOptions => RawKeyScreenRoute::PlayerOptions,
        Screen::Practice => RawKeyScreenRoute::Practice,
        Screen::Evaluation => RawKeyScreenRoute::Evaluation,
        _ => RawKeyScreenRoute::None,
    }
}

#[inline(always)]
pub const fn raw_pad_screen_route(screen: Screen) -> RawPadScreenRoute {
    match screen {
        Screen::Sandbox => RawPadScreenRoute::Sandbox,
        Screen::Mappings => RawPadScreenRoute::Mappings,
        Screen::Input => RawPadScreenRoute::Input,
        Screen::SelectMusic => RawPadScreenRoute::SelectMusic,
        Screen::Evaluation => RawPadScreenRoute::Evaluation,
        _ => RawPadScreenRoute::None,
    }
}

#[inline(always)]
pub const fn raw_key_alt_f4_quit(pressed: bool, code: KeyCode, alt_held: bool) -> bool {
    pressed && matches!(code, KeyCode::F4) && alt_held
}

#[inline(always)]
pub const fn practice_reload_shortcut(
    pressed: bool,
    repeat: bool,
    code: KeyCode,
    ctrl_held: bool,
    shift_held: bool,
    keyboard_features: bool,
) -> bool {
    pressed
        && !repeat
        && matches!(code, KeyCode::KeyR)
        && ctrl_held
        && shift_held
        && keyboard_features
}

#[inline(always)]
pub const fn evaluation_raw_key_shortcut(
    pressed: bool,
    repeat: bool,
    code: KeyCode,
    ctrl_held: bool,
    shift_held: bool,
    keyboard_features: bool,
    course_active: bool,
    retry_submissions_available: bool,
    course_eval_pages_active: bool,
) -> Option<EvaluationRawKeyShortcut> {
    if pressed
        && !repeat
        && matches!(code, KeyCode::KeyR)
        && ctrl_held
        && keyboard_features
        && !course_active
    {
        return Some(if shift_held {
            EvaluationRawKeyShortcut::GameplayReload
        } else {
            EvaluationRawKeyShortcut::GameplayRestart
        });
    }

    if pressed
        && !repeat
        && matches!(code, KeyCode::KeyP)
        && ctrl_held
        && keyboard_features
        && !course_active
    {
        return Some(EvaluationRawKeyShortcut::PracticeFromEvaluation);
    }

    if pressed && !repeat && matches!(code, KeyCode::F5) && retry_submissions_available {
        return Some(EvaluationRawKeyShortcut::RetrySubmissions);
    }

    if pressed && course_eval_pages_active {
        match code {
            KeyCode::KeyN => return Some(EvaluationRawKeyShortcut::StepCourseEvalPage(1)),
            KeyCode::KeyP => return Some(EvaluationRawKeyShortcut::StepCourseEvalPage(-1)),
            _ => {}
        }
    }

    None
}

#[inline(always)]
pub const fn app_raw_key_shortcut(
    pressed: bool,
    repeat: bool,
    code: KeyCode,
    ctrl_held: bool,
    shift_held: bool,
    alt_held: bool,
    frame_stats_enabled: bool,
) -> Option<AppRawKeyShortcut> {
    if pressed && matches!(code, KeyCode::F3) {
        if ctrl_held && shift_held {
            if !repeat && frame_stats_enabled {
                return Some(AppRawKeyShortcut::FrameStatsCycleAnchor);
            }
        } else if ctrl_held && alt_held {
            if !repeat && frame_stats_enabled {
                return Some(AppRawKeyShortcut::FrameStatsToggleStyle);
            }
        } else if ctrl_held {
            if !repeat {
                return Some(AppRawKeyShortcut::FrameStatsToggle);
            }
        } else {
            return Some(AppRawKeyShortcut::CycleOverlayMode);
        }
    }

    if pressed && !repeat && matches!(code, KeyCode::F9) {
        return Some(AppRawKeyShortcut::ToggleTranslatedTitles);
    }

    None
}

#[inline(always)]
pub fn screen_accepts_queued_input(screen: Screen, transition: &TransitionState) -> bool {
    deadsync_config::frame_pacing::queued_input_allowed(
        screen == Screen::Gameplay,
        matches!(transition, TransitionState::Idle),
        matches!(transition, TransitionState::FadingIn { .. }),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueuedInputFlushPlan {
    pub gameplay_screen: bool,
    pub start_screen: Screen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueuedInputEventRoute {
    Skip,
    Gameplay,
    Screen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmxPanelPressFeedback {
    pub pad_slot: usize,
    pub panel: usize,
    pub pressed: bool,
}

pub fn queued_input_flush_plan(
    screen: Screen,
    transition: &TransitionState,
) -> Option<QueuedInputFlushPlan> {
    screen_accepts_queued_input(screen, transition).then_some(QueuedInputFlushPlan {
        gameplay_screen: screen == Screen::Gameplay,
        start_screen: screen,
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueuedInputBatchState {
    pub flushed: bool,
    pub discard_gameplay_batch: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameplayRawKeyRouteContext {
    pub screen: Screen,
    pub gameplay_state_active: bool,
    pub offset_prompt_active: bool,
    pub shift_held: bool,
    pub ctrl_held: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameplayRawKeyRoutePlan {
    pub input: GameplayRawKeyInput,
    pub modifier_key: Option<GameplayRawModifierKey>,
    pub pressed: bool,
    pub timestamp: Instant,
    pub allow_commands: bool,
    pub shift_held: bool,
    pub ctrl_held: bool,
}

impl QueuedInputFlushPlan {
    #[inline(always)]
    pub fn route_drained_event(
        self,
        batch: &mut QueuedInputBatchState,
        error_pending: bool,
    ) -> QueuedInputEventRoute {
        batch.note_event();
        self.route_mapped_event(batch, error_pending)
    }

    #[inline(always)]
    pub const fn route_mapped_event(
        self,
        batch: &QueuedInputBatchState,
        error_pending: bool,
    ) -> QueuedInputEventRoute {
        if batch.should_skip_event(error_pending) {
            QueuedInputEventRoute::Skip
        } else if self.gameplay_screen {
            QueuedInputEventRoute::Gameplay
        } else {
            QueuedInputEventRoute::Screen
        }
    }

    #[inline(always)]
    pub fn note_dispatched_event(
        self,
        batch: &mut QueuedInputBatchState,
        current_screen: Screen,
        transition: &TransitionState,
    ) {
        if self.gameplay_screen {
            batch.note_gameplay_dispatch(self.start_screen, current_screen, transition);
        }
    }
}

impl QueuedInputBatchState {
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            flushed: false,
            discard_gameplay_batch: false,
        }
    }

    #[inline(always)]
    pub fn note_event(&mut self) {
        self.flushed = true;
    }

    #[inline(always)]
    pub const fn should_skip_event(self, error_pending: bool) -> bool {
        self.discard_gameplay_batch || error_pending
    }

    #[inline(always)]
    pub fn note_gameplay_dispatch(
        &mut self,
        start_screen: Screen,
        current_screen: Screen,
        transition: &TransitionState,
    ) {
        if !gameplay_dispatch_continues(start_screen, current_screen, transition) {
            self.discard_gameplay_batch = true;
        }
    }
}

pub fn smx_panel_press_feedback_plan(
    smx_input: bool,
    smx_panel_lights: bool,
    screen: Screen,
    smx_blackout_synced: &[bool],
    ev: &PadEvent,
) -> Option<SmxPanelPressFeedback> {
    if !smx_input || !smx_panel_lights {
        return None;
    }
    let PadEvent::RawButton {
        id, code, pressed, ..
    } = ev
    else {
        return None;
    };
    let pad_slot = id.0 as usize;
    let panel = code.0 as usize;
    let gameplay = matches!(screen, Screen::Gameplay | Screen::Practice);
    let blacked_out = smx_blackout_synced.get(pad_slot).copied().unwrap_or(false);
    ((!gameplay || blacked_out) && panel != deadsync_smx::CENTER_PANEL).then_some(
        SmxPanelPressFeedback {
            pad_slot,
            panel,
            pressed: *pressed,
        },
    )
}

#[inline(always)]
pub const fn raw_keyboard_restart_screen(screen: Screen) -> bool {
    matches!(screen, Screen::Gameplay | Screen::Evaluation)
}

pub fn gameplay_raw_key_route_plan(
    ev: GameplayRawKeyEvent,
    context: GameplayRawKeyRouteContext,
) -> Option<GameplayRawKeyRoutePlan> {
    if context.screen != Screen::Gameplay || !context.gameplay_state_active {
        return None;
    }
    Some(GameplayRawKeyRoutePlan {
        input: gameplay_raw_key_input(ev.code),
        modifier_key: gameplay_raw_modifier_key(ev.code),
        pressed: ev.pressed,
        timestamp: ev.timestamp,
        allow_commands: !context.offset_prompt_active,
        shift_held: context.shift_held,
        ctrl_held: context.ctrl_held,
    })
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
    use deadsync_input::{PadCode, PadId};
    use winit::keyboard::KeyCode;

    #[test]
    fn gameplay_raw_key_event_ignores_repeats_and_unhandled_keys() {
        let mut event = RawKeyboardEvent {
            code: KeyCode::KeyA,
            pressed: true,
            repeat: false,
            timestamp: Instant::now(),
            host_nanos: 0,
        };

        assert!(gameplay_raw_key_event(&event).is_none());
        event.code = KeyCode::KeyR;
        event.repeat = true;
        assert!(gameplay_raw_key_event(&event).is_none());
    }

    #[test]
    fn gameplay_raw_key_event_accepts_gameplay_control_keys() {
        let event = RawKeyboardEvent {
            code: KeyCode::F12,
            pressed: true,
            repeat: false,
            timestamp: Instant::now(),
            host_nanos: 0,
        };

        match gameplay_raw_key_event(&event) {
            Some(GameplayQueuedEvent::RawKey(ev)) => {
                assert_eq!(ev.code, KeyCode::F12);
                assert!(ev.pressed);
            }
            _ => panic!("expected raw gameplay key event"),
        }
    }

    #[test]
    fn gameplay_raw_key_mapping_preserves_offset_keys() {
        assert_eq!(
            gameplay_raw_key_input(KeyCode::F11),
            GameplayRawKeyInput::OffsetAdjust(GameplayOffsetAdjustKey::Decrease)
        );
        assert_eq!(
            gameplay_raw_key_input(KeyCode::F12),
            GameplayRawKeyInput::OffsetAdjust(GameplayOffsetAdjustKey::Increase)
        );
        assert_eq!(
            gameplay_raw_modifier_key(KeyCode::ControlLeft),
            Some(GameplayRawModifierKey::Ctrl)
        );
    }

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
    fn queued_input_plan_captures_starting_screen() {
        assert_eq!(
            queued_input_flush_plan(Screen::Gameplay, &TransitionState::Idle),
            Some(QueuedInputFlushPlan {
                gameplay_screen: true,
                start_screen: Screen::Gameplay,
            }),
        );

        assert_eq!(
            queued_input_flush_plan(
                Screen::SelectMusic,
                &TransitionState::FadingIn {
                    elapsed: 0.0,
                    duration: 0.2,
                },
            ),
            None,
        );
    }

    #[test]
    fn queued_input_batch_discards_after_gameplay_route_leaves_screen() {
        let mut batch = QueuedInputBatchState::new();
        batch.note_event();
        assert!(!batch.should_skip_event(false));

        batch.note_gameplay_dispatch(Screen::Gameplay, Screen::Evaluation, &TransitionState::Idle);

        assert!(batch.flushed);
        assert!(batch.should_skip_event(false));
    }

    #[test]
    fn queued_input_plan_routes_drained_events_and_marks_flush() {
        let plan = QueuedInputFlushPlan {
            gameplay_screen: false,
            start_screen: Screen::SelectMusic,
        };
        let mut batch = QueuedInputBatchState::new();

        assert_eq!(
            plan.route_drained_event(&mut batch, false),
            QueuedInputEventRoute::Screen
        );
        assert!(batch.flushed);
        assert_eq!(
            plan.route_drained_event(&mut batch, true),
            QueuedInputEventRoute::Skip
        );
    }

    #[test]
    fn queued_input_plan_routes_gameplay_until_screen_changes() {
        let plan = QueuedInputFlushPlan {
            gameplay_screen: true,
            start_screen: Screen::Gameplay,
        };
        let mut batch = QueuedInputBatchState::new();

        assert_eq!(
            plan.route_mapped_event(&batch, false),
            QueuedInputEventRoute::Gameplay
        );
        plan.note_dispatched_event(&mut batch, Screen::Evaluation, &TransitionState::Idle);
        assert_eq!(
            plan.route_mapped_event(&batch, false),
            QueuedInputEventRoute::Skip
        );
    }

    #[test]
    fn smx_panel_feedback_requires_enabled_raw_button() {
        let ev = PadEvent::RawButton {
            id: PadId(1),
            timestamp: Instant::now(),
            host_nanos: 0,
            code: PadCode(2),
            uuid: [0; 16],
            value: 1.0,
            pressed: true,
        };

        assert_eq!(
            smx_panel_press_feedback_plan(true, true, Screen::SelectMusic, &[], &ev),
            Some(SmxPanelPressFeedback {
                pad_slot: 1,
                panel: 2,
                pressed: true
            })
        );
        assert_eq!(
            smx_panel_press_feedback_plan(false, true, Screen::SelectMusic, &[], &ev),
            None
        );
    }

    #[test]
    fn smx_panel_feedback_is_limited_during_gameplay() {
        let ev = PadEvent::RawButton {
            id: PadId(0),
            timestamp: Instant::now(),
            host_nanos: 0,
            code: PadCode(1),
            uuid: [0; 16],
            value: 1.0,
            pressed: false,
        };

        assert_eq!(
            smx_panel_press_feedback_plan(true, true, Screen::Gameplay, &[false], &ev),
            None
        );
        assert_eq!(
            smx_panel_press_feedback_plan(true, true, Screen::Gameplay, &[true], &ev),
            Some(SmxPanelPressFeedback {
                pad_slot: 0,
                panel: 1,
                pressed: false
            })
        );
    }

    #[test]
    fn smx_panel_feedback_skips_center_panel_and_non_raw_events() {
        let center = PadEvent::RawButton {
            id: PadId(0),
            timestamp: Instant::now(),
            host_nanos: 0,
            code: PadCode(deadsync_smx::CENTER_PANEL as u32),
            uuid: [0; 16],
            value: 1.0,
            pressed: true,
        };
        let dir = PadEvent::Dir {
            id: PadId(0),
            timestamp: Instant::now(),
            host_nanos: 0,
            dir: deadsync_input::PadDir::Up,
            pressed: true,
        };

        assert_eq!(
            smx_panel_press_feedback_plan(true, true, Screen::SelectMusic, &[], &center),
            None
        );
        assert_eq!(
            smx_panel_press_feedback_plan(true, true, Screen::SelectMusic, &[], &dir),
            None
        );
    }

    #[test]
    fn raw_key_text_routes_only_text_entry_screens() {
        assert_eq!(
            raw_key_text_route(Screen::ManageLocalProfiles),
            RawKeyTextRoute::ManageLocalProfiles
        );
        assert_eq!(
            raw_key_text_route(Screen::Options),
            RawKeyTextRoute::Options
        );
        assert_eq!(
            raw_key_text_route(Screen::SelectMusic),
            RawKeyTextRoute::SelectMusic
        );
        assert_eq!(
            raw_key_text_route(Screen::Gameplay),
            RawKeyTextRoute::Ignore
        );
    }

    #[test]
    fn raw_key_screen_route_marks_screen_owned_raw_handlers() {
        assert_eq!(
            raw_key_screen_route(Screen::Sandbox),
            RawKeyScreenRoute::Sandbox
        );
        assert_eq!(
            raw_key_screen_route(Screen::Mappings),
            RawKeyScreenRoute::Mappings
        );
        assert_eq!(
            raw_key_screen_route(Screen::Options),
            RawKeyScreenRoute::Options
        );
        assert_eq!(
            raw_key_screen_route(Screen::Evaluation),
            RawKeyScreenRoute::Evaluation
        );
        assert_eq!(
            raw_key_screen_route(Screen::PlayerOptions),
            RawKeyScreenRoute::PlayerOptions
        );
        assert_eq!(
            raw_key_screen_route(Screen::Gameplay),
            RawKeyScreenRoute::None
        );
    }

    #[test]
    fn raw_pad_screen_route_marks_screen_owned_raw_handlers() {
        assert_eq!(
            raw_pad_screen_route(Screen::Sandbox),
            RawPadScreenRoute::Sandbox
        );
        assert_eq!(
            raw_pad_screen_route(Screen::Mappings),
            RawPadScreenRoute::Mappings
        );
        assert_eq!(
            raw_pad_screen_route(Screen::Input),
            RawPadScreenRoute::Input
        );
        assert_eq!(
            raw_pad_screen_route(Screen::SelectMusic),
            RawPadScreenRoute::SelectMusic
        );
        assert_eq!(
            raw_pad_screen_route(Screen::Evaluation),
            RawPadScreenRoute::Evaluation
        );
        assert_eq!(
            raw_pad_screen_route(Screen::Gameplay),
            RawPadScreenRoute::None
        );
    }

    #[test]
    fn raw_key_alt_f4_requires_alt_press() {
        assert!(raw_key_alt_f4_quit(true, KeyCode::F4, true));
        assert!(!raw_key_alt_f4_quit(false, KeyCode::F4, true));
        assert!(!raw_key_alt_f4_quit(true, KeyCode::F4, false));
        assert!(!raw_key_alt_f4_quit(true, KeyCode::F3, true));
    }

    #[test]
    fn practice_reload_shortcut_requires_ctrl_shift_r() {
        assert!(practice_reload_shortcut(
            true,
            false,
            KeyCode::KeyR,
            true,
            true,
            true
        ));
        assert!(!practice_reload_shortcut(
            true,
            true,
            KeyCode::KeyR,
            true,
            true,
            true
        ));
        assert!(!practice_reload_shortcut(
            true,
            false,
            KeyCode::KeyR,
            true,
            false,
            true
        ));
        assert!(!practice_reload_shortcut(
            true,
            false,
            KeyCode::KeyR,
            true,
            true,
            false
        ));
    }

    #[test]
    fn evaluation_shortcuts_preserve_restart_order() {
        assert_eq!(
            evaluation_raw_key_shortcut(
                true,
                false,
                KeyCode::KeyR,
                true,
                false,
                true,
                false,
                false,
                false,
            ),
            Some(EvaluationRawKeyShortcut::GameplayRestart)
        );
        assert_eq!(
            evaluation_raw_key_shortcut(
                true,
                false,
                KeyCode::KeyR,
                true,
                true,
                true,
                false,
                false,
                false,
            ),
            Some(EvaluationRawKeyShortcut::GameplayReload)
        );
        assert_eq!(
            evaluation_raw_key_shortcut(
                true,
                false,
                KeyCode::KeyP,
                true,
                false,
                true,
                false,
                false,
                true,
            ),
            Some(EvaluationRawKeyShortcut::PracticeFromEvaluation)
        );
    }

    #[test]
    fn evaluation_shortcuts_gate_course_and_retry_actions() {
        assert_eq!(
            evaluation_raw_key_shortcut(
                true,
                false,
                KeyCode::KeyR,
                true,
                false,
                true,
                true,
                false,
                false,
            ),
            None
        );
        assert_eq!(
            evaluation_raw_key_shortcut(
                true,
                false,
                KeyCode::F5,
                false,
                false,
                false,
                true,
                true,
                false,
            ),
            Some(EvaluationRawKeyShortcut::RetrySubmissions)
        );
        assert_eq!(
            evaluation_raw_key_shortcut(
                true,
                false,
                KeyCode::KeyN,
                false,
                false,
                false,
                true,
                false,
                true,
            ),
            Some(EvaluationRawKeyShortcut::StepCourseEvalPage(1))
        );
        assert_eq!(
            evaluation_raw_key_shortcut(
                true,
                false,
                KeyCode::KeyP,
                false,
                false,
                false,
                true,
                false,
                true,
            ),
            Some(EvaluationRawKeyShortcut::StepCourseEvalPage(-1))
        );
    }

    #[test]
    fn app_raw_key_shortcuts_preserve_modifier_precedence() {
        assert_eq!(
            app_raw_key_shortcut(true, false, KeyCode::F3, true, true, false, true),
            Some(AppRawKeyShortcut::FrameStatsCycleAnchor)
        );
        assert_eq!(
            app_raw_key_shortcut(true, false, KeyCode::F3, true, false, true, true),
            Some(AppRawKeyShortcut::FrameStatsToggleStyle)
        );
        assert_eq!(
            app_raw_key_shortcut(true, false, KeyCode::F3, true, false, false, false),
            Some(AppRawKeyShortcut::FrameStatsToggle)
        );
        assert_eq!(
            app_raw_key_shortcut(true, false, KeyCode::F3, false, false, false, false),
            Some(AppRawKeyShortcut::CycleOverlayMode)
        );
        assert_eq!(
            app_raw_key_shortcut(true, false, KeyCode::F9, false, false, false, false),
            Some(AppRawKeyShortcut::ToggleTranslatedTitles)
        );
    }

    #[test]
    fn app_raw_key_shortcuts_ignore_repeated_gated_toggles() {
        assert_eq!(
            app_raw_key_shortcut(true, true, KeyCode::F3, true, true, false, true),
            None
        );
        assert_eq!(
            app_raw_key_shortcut(true, true, KeyCode::F3, true, false, true, true),
            None
        );
        assert_eq!(
            app_raw_key_shortcut(true, true, KeyCode::F3, true, false, false, true),
            None
        );
        assert_eq!(
            app_raw_key_shortcut(true, true, KeyCode::F9, false, false, false, false),
            None
        );
    }

    #[test]
    fn gameplay_raw_key_route_requires_live_gameplay_state() {
        let ev = GameplayRawKeyEvent {
            code: KeyCode::KeyR,
            pressed: true,
            timestamp: Instant::now(),
        };
        let context = GameplayRawKeyRouteContext {
            screen: Screen::Gameplay,
            gameplay_state_active: false,
            offset_prompt_active: false,
            shift_held: false,
            ctrl_held: true,
        };
        assert!(gameplay_raw_key_route_plan(ev, context).is_none());

        let context = GameplayRawKeyRouteContext {
            screen: Screen::Evaluation,
            gameplay_state_active: true,
            ..context
        };
        assert!(gameplay_raw_key_route_plan(ev, context).is_none());
    }

    #[test]
    fn gameplay_raw_key_route_maps_event_and_modifier_snapshot() {
        let timestamp = Instant::now();
        let ev = GameplayRawKeyEvent {
            code: KeyCode::KeyR,
            pressed: true,
            timestamp,
        };
        let plan = gameplay_raw_key_route_plan(
            ev,
            GameplayRawKeyRouteContext {
                screen: Screen::Gameplay,
                gameplay_state_active: true,
                offset_prompt_active: true,
                shift_held: true,
                ctrl_held: true,
            },
        )
        .expect("gameplay state accepts raw key");

        assert_eq!(plan.input, GameplayRawKeyInput::Restart);
        assert_eq!(plan.modifier_key, None);
        assert!(plan.pressed);
        assert_eq!(plan.timestamp, timestamp);
        assert!(!plan.allow_commands);
        assert!(plan.shift_held);
        assert!(plan.ctrl_held);

        let plan = gameplay_raw_key_route_plan(
            GameplayRawKeyEvent {
                code: KeyCode::ShiftLeft,
                pressed: false,
                timestamp,
            },
            GameplayRawKeyRouteContext {
                screen: Screen::Gameplay,
                gameplay_state_active: true,
                offset_prompt_active: false,
                shift_held: false,
                ctrl_held: true,
            },
        )
        .expect("modifier keys route to gameplay");
        assert_eq!(plan.input, GameplayRawKeyInput::Other);
        assert_eq!(plan.modifier_key, Some(GameplayRawModifierKey::Shift));
        assert!(plan.allow_commands);
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

    #[test]
    fn gamepad_system_startup_only_forwards_to_sandbox() {
        assert_eq!(
            gamepad_system_event_plan(Screen::Menu, &GpSystemEvent::StartupComplete),
            GamepadSystemEventPlan {
                forward_to_sandbox: false,
                refresh_smx_underglow: false,
                log_message: None,
                user_message: None,
            }
        );
        assert!(
            gamepad_system_event_plan(Screen::Sandbox, &GpSystemEvent::StartupComplete)
                .forward_to_sandbox
        );
    }

    #[test]
    fn gamepad_connect_plan_refreshes_smx_and_hides_initial_overlay() {
        let initial = gamepad_system_event_plan(
            Screen::Menu,
            &GpSystemEvent::Connected {
                name: "Pad".to_string(),
                id: PadId(7),
                vendor_id: None,
                product_id: None,
                backend: PadBackend::Smx,
                initial: true,
            },
        );
        assert!(initial.refresh_smx_underglow);
        assert!(initial.log_message.as_deref().is_some_and(|msg| {
            msg.contains("Gamepad connected") && msg.contains("Pad") && msg.contains("7")
        }));
        assert_eq!(initial.user_message, None);

        let live = gamepad_system_event_plan(
            Screen::Menu,
            &GpSystemEvent::Connected {
                name: "Pad".to_string(),
                id: PadId(7),
                vendor_id: None,
                product_id: None,
                backend: PadBackend::Smx,
                initial: false,
            },
        );
        assert!(live.user_message.as_deref().is_some_and(|msg| {
            msg.contains("Connected") && msg.contains("Pad") && msg.contains("7")
        }));
    }

    #[test]
    fn gamepad_disconnect_plan_matches_connection_policy() {
        let plan = gamepad_system_event_plan(
            Screen::Sandbox,
            &GpSystemEvent::Disconnected {
                name: "Gone".to_string(),
                id: PadId(3),
                backend: PadBackend::Smx,
                initial: false,
            },
        );
        assert!(plan.forward_to_sandbox);
        assert!(plan.refresh_smx_underglow);
        assert!(plan.log_message.as_deref().is_some_and(|msg| {
            msg.contains("Gamepad disconnected") && msg.contains("Gone") && msg.contains("3")
        }));
        assert!(plan.user_message.as_deref().is_some_and(|msg| {
            msg.contains("Disconnected") && msg.contains("Gone") && msg.contains("3")
        }));
    }

    fn pre_screen_context(screen: Screen) -> PreScreenInputContext {
        PreScreenInputContext {
            screen,
            only_dedicated_menu_buttons: false,
            evaluation_test_input_active: false,
            gameplay_offset_prompt_active: false,
            course_active: false,
        }
    }

    #[test]
    fn dedicated_mode_filters_arrows_before_shortcuts() {
        let context = PreScreenInputContext {
            only_dedicated_menu_buttons: true,
            ..pre_screen_context(Screen::Menu)
        };
        assert_eq!(
            pre_screen_input_route(true, VirtualAction::p1_left, context),
            PreScreenInputRoute::Consume,
        );
        assert_eq!(
            pre_screen_input_route(
                true,
                VirtualAction::p1_left,
                PreScreenInputContext {
                    screen: Screen::Evaluation,
                    evaluation_test_input_active: true,
                    ..context
                },
            ),
            PreScreenInputRoute::Dispatch,
        );
    }

    #[test]
    fn evaluation_select_requests_side_scoped_screenshot() {
        assert_eq!(
            pre_screen_input_route(
                true,
                VirtualAction::p2_select,
                pre_screen_context(Screen::EvaluationSummary),
            ),
            PreScreenInputRoute::RequestScreenshot(Some(PlayerSide::P2)),
        );
        assert_eq!(
            pre_screen_input_route(
                false,
                VirtualAction::p2_select,
                pre_screen_context(Screen::EvaluationSummary),
            ),
            PreScreenInputRoute::Dispatch,
        );
    }

    #[test]
    fn restart_requires_gameplay_or_evaluation_without_prompt_or_course() {
        assert_eq!(
            pre_screen_input_route(
                true,
                VirtualAction::p1_restart,
                pre_screen_context(Screen::Gameplay),
            ),
            PreScreenInputRoute::Restart,
        );
        for context in [
            PreScreenInputContext {
                gameplay_offset_prompt_active: true,
                ..pre_screen_context(Screen::Gameplay)
            },
            PreScreenInputContext {
                course_active: true,
                ..pre_screen_context(Screen::Evaluation)
            },
            pre_screen_context(Screen::SelectMusic),
        ] {
            assert_eq!(
                pre_screen_input_route(true, VirtualAction::p1_restart, context),
                PreScreenInputRoute::Dispatch,
            );
        }
    }
}
