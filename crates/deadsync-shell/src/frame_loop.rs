use deadsync_config::frame_pacing::{
    FrameIntervalState, FrameLoopMode, FrameLoopModeTracker, RedrawRequestState,
    advance_redraw_deadline, frame_interval_for_max_fps, should_skip_compose_and_draw,
    window_frame_interval_state,
};
use deadsync_screens::Screen;
use std::time::{Duration, Instant};

const SCHEDULED_REDRAW_POLL_GUARD: Duration = Duration::from_micros(1_000);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameWaitControl {
    Poll,
    Wait,
    WaitUntil(Instant),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameWaitPlan {
    pub mode: FrameLoopMode,
    pub control: FrameWaitControl,
    pub redraw_reason: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameScreenStepContext {
    pub current_screen: Screen,
    pub transition_step_screen: bool,
    pub gameplay_offset_prompt_active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameScreenStepPlan {
    pub step_screen: bool,
}

pub struct FrameLoopState {
    frame_interval: Option<Duration>,
    next_redraw_at: Instant,
    redraw_request: RedrawRequestState,
    mode: FrameLoopModeTracker,
    window_focused: bool,
    window_occluded: bool,
    surface_active: bool,
}

#[inline(always)]
pub const fn frame_screen_step_plan(context: FrameScreenStepContext) -> FrameScreenStepPlan {
    FrameScreenStepPlan {
        step_screen: context.transition_step_screen
            && !(matches!(context.current_screen, Screen::Gameplay)
                && context.gameplay_offset_prompt_active),
    }
}

impl FrameLoopState {
    pub fn new(max_fps: u16, surface_active: bool, now: Instant) -> Self {
        Self {
            frame_interval: frame_interval_for_max_fps(max_fps),
            next_redraw_at: now,
            redraw_request: RedrawRequestState::new(),
            mode: FrameLoopModeTracker::new(),
            window_focused: false,
            window_occluded: false,
            surface_active,
        }
    }

    pub fn set_max_fps(&mut self, max_fps: u16) {
        self.frame_interval = frame_interval_for_max_fps(max_fps);
        self.reset_schedule(Instant::now());
    }

    pub fn reset_schedule(&mut self, now: Instant) {
        self.next_redraw_at = now;
        self.redraw_request.reset();
        self.mode.reset();
    }

    #[inline(always)]
    pub fn note_redraw_requested(&mut self, now: Instant, reason: &'static str) {
        self.redraw_request.note_requested(now, reason);
    }

    #[inline(always)]
    pub fn take_redraw_request_timing(&mut self, now: Instant) -> (u32, &'static str) {
        let timing = self.redraw_request.take_timing(now);
        (timing.request_to_redraw_us, timing.reason)
    }

    #[inline(always)]
    pub fn redraw_pending(&self) -> bool {
        self.redraw_request.pending()
    }

    pub fn interval_state(&self, vsync: bool, screen: Screen) -> FrameIntervalState {
        window_frame_interval_state(
            vsync,
            self.frame_interval,
            self.window_occluded,
            self.surface_active,
            self.window_focused,
            !matches!(screen, Screen::Gameplay | Screen::Practice),
        )
    }

    #[inline(always)]
    pub fn note_mode(&mut self, mode: FrameLoopMode) -> bool {
        self.mode.note(mode)
    }

    pub fn set_window_focus(&mut self, focused: bool) -> bool {
        if self.window_focused == focused {
            return false;
        }
        self.window_focused = focused;
        true
    }

    pub fn set_window_occluded(&mut self, occluded: bool) -> bool {
        if self.window_occluded == occluded {
            return false;
        }
        self.window_occluded = occluded;
        true
    }

    pub fn set_surface_active(&mut self, active: bool) -> bool {
        if self.surface_active == active {
            return false;
        }
        self.surface_active = active;
        true
    }

    #[inline(always)]
    pub fn should_skip_compose_and_draw(&self) -> bool {
        should_skip_compose_and_draw(self.window_occluded, self.surface_active)
    }

    fn advance_if_due(&mut self, now: Instant, interval: Duration) -> bool {
        if now < self.next_redraw_at {
            return false;
        }
        self.next_redraw_at = advance_redraw_deadline(self.next_redraw_at, now, interval);
        true
    }

    pub fn plan_wait(&mut self, now: Instant, interval_state: FrameIntervalState) -> FrameWaitPlan {
        let Some(interval) = interval_state.interval else {
            return if self.redraw_pending() {
                FrameWaitPlan {
                    mode: FrameLoopMode::WaitPending,
                    control: FrameWaitControl::Wait,
                    redraw_reason: None,
                }
            } else {
                FrameWaitPlan {
                    mode: FrameLoopMode::Poll,
                    control: FrameWaitControl::Poll,
                    redraw_reason: Some("poll"),
                }
            };
        };

        let redraw_reason = self
            .advance_if_due(now, interval)
            .then(|| interval_state.reason.redraw_reason());
        let time_until_deadline = self.next_redraw_at.saturating_duration_since(now);
        let control = if time_until_deadline <= SCHEDULED_REDRAW_POLL_GUARD {
            FrameWaitControl::Poll
        } else {
            FrameWaitControl::WaitUntil(self.next_redraw_at - SCHEDULED_REDRAW_POLL_GUARD)
        };
        FrameWaitPlan {
            mode: FrameLoopMode::Scheduled(interval_state.reason, interval),
            control,
            redraw_reason,
        }
    }

    #[inline(always)]
    pub const fn frame_interval(&self) -> Option<Duration> {
        self.frame_interval
    }

    #[inline(always)]
    pub const fn window_focused(&self) -> bool {
        self.window_focused
    }

    #[inline(always)]
    pub const fn window_occluded(&self) -> bool {
        self.window_occluded
    }

    #[inline(always)]
    pub const fn surface_active(&self) -> bool {
        self.surface_active
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_config::frame_pacing::FrameIntervalReason;

    #[test]
    fn state_changes_and_redraw_deadlines_are_latched() {
        let now = Instant::now();
        let mut state = FrameLoopState::new(120, true, now);
        assert!(state.set_window_focus(true));
        assert!(!state.set_window_focus(true));
        state.note_redraw_requested(now, "test");
        assert!(state.redraw_pending());
        assert_eq!(state.take_redraw_request_timing(now).1, "test");
        assert!(state.advance_if_due(now, Duration::from_millis(8)));
        assert!(state.next_redraw_at > now);
    }

    #[test]
    fn due_scheduled_frame_advances_and_requests_redraw() {
        let now = Instant::now();
        let interval = Duration::from_millis(16);
        let mut state = FrameLoopState::new(0, true, now);

        let plan = state.plan_wait(
            now,
            FrameIntervalState {
                interval: Some(interval),
                reason: FrameIntervalReason::MaxFps,
            },
        );

        assert_eq!(
            plan,
            FrameWaitPlan {
                mode: FrameLoopMode::Scheduled(FrameIntervalReason::MaxFps, interval),
                control: FrameWaitControl::WaitUntil(now + interval - SCHEDULED_REDRAW_POLL_GUARD),
                redraw_reason: Some("scheduled_maxfps"),
            }
        );
    }

    #[test]
    fn scheduled_frame_polls_inside_deadline_guard_without_duplicate_redraw() {
        let start = Instant::now();
        let interval = Duration::from_millis(16);
        let mut state = FrameLoopState::new(0, true, start);
        let interval_state = FrameIntervalState {
            interval: Some(interval),
            reason: FrameIntervalReason::Background,
        };
        let _ = state.plan_wait(start, interval_state);

        let plan = state.plan_wait(start + Duration::from_micros(15_500), interval_state);

        assert_eq!(plan.control, FrameWaitControl::Poll);
        assert_eq!(plan.redraw_reason, None);
    }

    #[test]
    fn uncapped_loop_waits_for_pending_redraw_or_polls_for_a_new_one() {
        let now = Instant::now();
        let uncapped = FrameIntervalState {
            interval: None,
            reason: FrameIntervalReason::None,
        };
        let mut state = FrameLoopState::new(0, true, now);

        assert_eq!(
            state.plan_wait(now, uncapped),
            FrameWaitPlan {
                mode: FrameLoopMode::Poll,
                control: FrameWaitControl::Poll,
                redraw_reason: Some("poll"),
            }
        );

        state.note_redraw_requested(now, "external");
        assert_eq!(
            state.plan_wait(now, uncapped),
            FrameWaitPlan {
                mode: FrameLoopMode::WaitPending,
                control: FrameWaitControl::Wait,
                redraw_reason: None,
            }
        );
    }

    #[test]
    fn screen_step_follows_transition_gate() {
        assert_eq!(
            frame_screen_step_plan(FrameScreenStepContext {
                current_screen: Screen::SelectMusic,
                transition_step_screen: false,
                gameplay_offset_prompt_active: false,
            }),
            FrameScreenStepPlan { step_screen: false }
        );
        assert_eq!(
            frame_screen_step_plan(FrameScreenStepContext {
                current_screen: Screen::SelectMusic,
                transition_step_screen: true,
                gameplay_offset_prompt_active: false,
            }),
            FrameScreenStepPlan { step_screen: true }
        );
    }

    #[test]
    fn gameplay_offset_prompt_blocks_only_gameplay_screen_step() {
        assert_eq!(
            frame_screen_step_plan(FrameScreenStepContext {
                current_screen: Screen::Gameplay,
                transition_step_screen: true,
                gameplay_offset_prompt_active: true,
            }),
            FrameScreenStepPlan { step_screen: false }
        );
        assert_eq!(
            frame_screen_step_plan(FrameScreenStepContext {
                current_screen: Screen::Evaluation,
                transition_step_screen: true,
                gameplay_offset_prompt_active: true,
            }),
            FrameScreenStepPlan { step_screen: true }
        );
    }
}
