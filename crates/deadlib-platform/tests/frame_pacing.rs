use deadlib_platform::frame_pacing::*;
use std::time::{Duration, Instant};

// Instants use an arbitrary origin; assertions depend only on supplied offsets,
// never on wall-clock progress or the duration of test execution.

#[test]
fn window_frame_interval_uses_background_when_occluded() {
    let state = window_frame_interval_state(true, None, true, true, true, false);
    assert_eq!(
        state,
        FrameIntervalState {
            interval: Some(BACKGROUND_REDRAW_INTERVAL),
            reason: FrameIntervalReason::Background,
        }
    );
}
#[test]
fn window_frame_interval_combines_max_fps_and_background() {
    let max_fps = Some(Duration::from_millis(5));
    let state = window_frame_interval_state(false, max_fps, false, true, false, true);
    assert_eq!(
        state,
        FrameIntervalState {
            interval: Some(BACKGROUND_REDRAW_INTERVAL),
            reason: FrameIntervalReason::MaxFpsBackground,
        }
    );
}
#[test]
fn window_frame_interval_respects_unfocused_no_throttle() {
    let max_fps = Some(Duration::from_millis(5));
    let state = window_frame_interval_state(false, max_fps, false, true, false, false);
    assert_eq!(
        state,
        FrameIntervalState {
            interval: max_fps,
            reason: FrameIntervalReason::MaxFps,
        }
    );
}
#[test]
fn foreground_input_requires_focus_and_surface() {
    assert!(foreground_input_active(true, true));
    assert!(!foreground_input_active(false, true));
    assert!(!foreground_input_active(true, false));
    assert!(!foreground_input_active(false, false));
}
#[test]
fn compose_draw_skips_occluded_or_inactive_surface() {
    assert!(!should_skip_compose_and_draw(false, true));
    assert!(should_skip_compose_and_draw(true, true));
    assert!(should_skip_compose_and_draw(false, false));
}
#[test]
fn max_fps_zero_has_no_interval() {
    assert_eq!(frame_interval_for_max_fps(0), None);
}
#[test]
fn max_fps_interval_uses_fps_period() {
    assert_eq!(
        frame_interval_for_max_fps(60),
        Some(Duration::from_secs_f64(1.0 / 60.0))
    );
}
#[test]
fn redraw_deadline_keeps_future_deadline() {
    let now = Instant::now();
    let deadline = now + Duration::from_millis(16);
    assert_eq!(
        advance_redraw_deadline(deadline, now, Duration::from_millis(16)),
        deadline
    );
}
#[test]
fn redraw_deadline_advances_past_now() {
    let now = Instant::now();
    let deadline = now - Duration::from_millis(33);
    let next = advance_redraw_deadline(deadline, now, Duration::from_millis(16));
    assert_eq!(next, now + Duration::from_millis(15));
}
#[test]
fn redraw_request_state_latches_first_reason_until_taken() {
    let now = Instant::now();
    let mut state = RedrawRequestState::new();

    state.note_requested(now, "input");
    state.note_requested(now + Duration::from_micros(100), "chain");
    assert!(state.pending());

    let timing = state.take_timing(now + Duration::from_micros(250));

    assert_eq!(timing.request_to_redraw_us, 250);
    assert_eq!(timing.reason, "input");
    assert!(!state.pending());
}
#[test]
fn redraw_request_state_marks_external_when_unrequested() {
    let mut state = RedrawRequestState::new();
    let timing = state.take_timing(Instant::now());

    assert_eq!(timing.request_to_redraw_us, 0);
    assert_eq!(timing.reason, "external");
}
#[test]
fn frame_loop_mode_tracker_reports_only_changes() {
    let mut tracker = FrameLoopModeTracker::new();

    assert!(tracker.note(FrameLoopMode::Poll));
    assert!(!tracker.note(FrameLoopMode::Poll));
    assert!(tracker.note(FrameLoopMode::WaitPending));
    tracker.reset();
    assert!(tracker.note(FrameLoopMode::WaitPending));
}
#[test]
fn state_changes_and_redraw_deadlines_are_latched() {
    let now = Instant::now();
    let mut state = FrameLoopState::new(120, true, now);
    assert!(state.set_window_focus(true));
    assert!(!state.set_window_focus(true));
    state.note_redraw_requested(now, "test");
    assert!(state.redraw_pending());
    assert_eq!(state.take_redraw_request_timing(now).1, "test");
    let plan = state.plan_wait(now, state.interval_state(false, false));
    assert_eq!(plan.redraw_reason, Some("scheduled_maxfps"));
    assert_eq!(
        plan.control,
        FrameWaitControl::WaitUntil(now + Duration::from_nanos(7_333_333)),
    );
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
            control: FrameWaitControl::WaitUntil(now + Duration::from_millis(15)),
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

    assert_eq!(state.take_redraw_request_timing(now), (0, "external"));
    let plan = state.plan_wait(now, uncapped);
    assert_eq!(plan.control, FrameWaitControl::Poll);
    assert_eq!(plan.redraw_reason, Some("poll"));
}

#[test]
fn missed_frames_keep_cadence_and_the_first_pending_request() {
    let start = Instant::now();
    let mut state = FrameLoopState::new(50, true, start);
    let interval = state.interval_state(false, false);
    let first = state.plan_wait(start, interval);
    assert_eq!(first.redraw_reason, Some("scheduled_maxfps"));
    assert!(!state.redraw_pending());
    state.note_redraw_requested(start, "input");

    let late = start + Duration::from_millis(73);
    let plan = state.plan_wait(late, interval);
    assert_eq!(plan.redraw_reason, Some("scheduled_maxfps"));
    assert_eq!(
        plan.control,
        FrameWaitControl::WaitUntil(start + Duration::from_millis(79))
    );
    state.note_redraw_requested(late, "scheduled_maxfps");
    assert_eq!(state.take_redraw_request_timing(late), (73_000, "input"));
    assert!(!state.redraw_pending());
    assert_eq!(state.plan_wait(late, interval).redraw_reason, None);
}

#[test]
fn scheduled_wait_polls_at_guard_boundary_and_redraws_only_at_deadline() {
    let start = Instant::now();
    let mut state = FrameLoopState::new(50, true, start);
    let interval = state.interval_state(false, false);
    state.plan_wait(start, interval);

    let before_guard = state.plan_wait(start + Duration::from_micros(18_999), interval);
    assert_eq!(
        before_guard.control,
        FrameWaitControl::WaitUntil(start + Duration::from_millis(19))
    );
    assert_eq!(before_guard.redraw_reason, None);
    for offset in [19_000, 19_001, 19_999] {
        let plan = state.plan_wait(start + Duration::from_micros(offset), interval);
        assert_eq!(plan.control, FrameWaitControl::Poll);
        assert_eq!(plan.redraw_reason, None);
    }
    let due = state.plan_wait(start + Duration::from_millis(20), interval);
    assert_eq!(due.redraw_reason, Some("scheduled_maxfps"));
    assert_eq!(
        due.control,
        FrameWaitControl::WaitUntil(start + Duration::from_millis(39))
    );
}

#[test]
fn redraw_deadline_handles_zero_intervals_and_excessive_catch_up() {
    let start = Instant::now();
    let now = start + Duration::from_secs(5);
    assert_eq!(advance_redraw_deadline(start, now, Duration::ZERO), now);
    assert_eq!(
        advance_redraw_deadline(start, now, Duration::from_nanos(1)),
        now + Duration::from_nanos(1)
    );
}

#[test]
fn request_latency_clamps_reversed_and_overflowing_timestamps() {
    let start = Instant::now();
    let mut state = RedrawRequestState::new();
    state.note_requested(start + Duration::from_micros(1), "input");
    assert_eq!(state.take_timing(start).request_to_redraw_us, 0);
    state.note_requested(start, "input");
    let later = start + Duration::from_micros(u64::from(u32::MAX) + 1);
    assert_eq!(state.take_timing(later).request_to_redraw_us, u32::MAX);
}

#[test]
fn changing_frame_cap_resets_deadline_requests_and_mode_only() {
    let start = Instant::now();
    let mut state = FrameLoopState::new(50, true, start);
    state.set_window_focus(true);
    state.set_window_occluded(true);
    state.note_redraw_requested(start, "input");
    assert!(state.note_mode(FrameLoopMode::Poll));

    let reset = start + Duration::from_secs(1);
    state.set_max_fps(100, reset);
    assert!(!state.redraw_pending());
    assert!(state.note_mode(FrameLoopMode::Poll));
    assert!(state.window_focused());
    assert!(state.window_occluded());
    assert!(state.surface_active());
    assert_eq!(state.frame_interval(), Some(Duration::from_millis(10)));

    state.set_window_occluded(false);
    let interval = state.interval_state(false, false);
    let plan = state.plan_wait(reset, interval);
    assert_eq!(plan.redraw_reason, Some("scheduled_maxfps"));
    assert_eq!(
        plan.control,
        FrameWaitControl::WaitUntil(reset + Duration::from_millis(9))
    );
}

#[test]
fn window_policy_preserves_stricter_caps_and_surface_throttling() {
    let start = Instant::now();
    let mut state = FrameLoopState::new(10, true, start);
    assert_eq!(
        state.interval_state(false, true),
        FrameIntervalState {
            interval: Some(Duration::from_millis(100)),
            reason: FrameIntervalReason::MaxFpsBackground,
        }
    );
    assert_eq!(state.interval_state(true, false).interval, None);

    state.set_surface_active(false);
    assert_eq!(
        state.interval_state(true, false),
        FrameIntervalState {
            interval: Some(Duration::from_millis(67)),
            reason: FrameIntervalReason::Background,
        }
    );
    assert!(state.should_skip_compose_and_draw());
}
