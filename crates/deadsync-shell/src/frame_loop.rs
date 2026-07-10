use deadsync_config::frame_pacing::{
    FrameIntervalState, FrameLoopMode, FrameLoopModeTracker, RedrawRequestState,
    advance_redraw_deadline, frame_interval_for_max_fps, should_skip_compose_and_draw,
    window_frame_interval_state,
};
use deadsync_screens::Screen;
use std::time::{Duration, Instant};

pub struct FrameLoopState {
    frame_interval: Option<Duration>,
    next_redraw_at: Instant,
    redraw_request: RedrawRequestState,
    mode: FrameLoopModeTracker,
    window_focused: bool,
    window_occluded: bool,
    surface_active: bool,
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

    pub fn advance_if_due(&mut self, now: Instant, interval: Duration) -> bool {
        if now < self.next_redraw_at {
            return false;
        }
        self.next_redraw_at = advance_redraw_deadline(self.next_redraw_at, now, interval);
        true
    }

    #[inline(always)]
    pub const fn next_redraw_at(&self) -> Instant {
        self.next_redraw_at
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
        assert!(state.next_redraw_at() > now);
    }
}
