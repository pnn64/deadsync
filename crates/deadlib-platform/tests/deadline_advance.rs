use deadlib_platform::frame_pacing::{
    FrameIntervalReason, FrameIntervalState, FrameLoopMode, FrameLoopState, FrameWaitControl,
    advance_redraw_deadline,
};
use std::time::{Duration, Instant};

#[test]
fn wait_plans_preserve_deadlines_through_polling_and_stalls() {
    let start = Instant::now();
    for interval in [
        Duration::ZERO,
        Duration::from_nanos(1),
        Duration::from_millis(1),
        Duration::from_nanos(4_166_666),
        Duration::from_nanos(16_666_667),
        Duration::from_millis(67),
    ] {
        let mut scheduler = FrameLoopState::new(0, true, start);
        let policy = FrameIntervalState {
            interval: Some(interval),
            reason: FrameIntervalReason::MaxFps,
        };
        let mut deadline = start;
        let mut now = start;
        for step in 0..4096 {
            now += match step % 8 {
                0 => interval,
                1 => Duration::from_nanos(1),
                2 => Duration::from_millis(100),
                _ => interval / 4,
            };
            let due = now >= deadline;
            deadline = advance_redraw_deadline(deadline, now, interval);
            let plan = scheduler.plan_wait(now, policy);
            assert_eq!(plan.mode, FrameLoopMode::Scheduled(policy.reason, interval));
            assert_eq!(plan.redraw_reason, due.then_some("scheduled_maxfps"));
            let guard = Duration::from_millis(1);
            let control = if deadline.saturating_duration_since(now) <= guard {
                FrameWaitControl::Poll
            } else {
                FrameWaitControl::WaitUntil(deadline - guard)
            };
            assert_eq!(plan.control, control, "interval={interval:?}, step={step}");
            if due {
                scheduler.note_redraw_requested(now, "scheduled_maxfps");
            }
            if step % 3 == 0 {
                scheduler.take_redraw_request_timing(now);
            }
        }
    }
}

#[test]
fn deadlines_keep_future_times_and_advance_past_elapsed_intervals() {
    let start = Instant::now();
    let interval = Duration::from_millis(10);
    for (overdue, next) in [(0, 10), (1, 10), (9, 10), (10, 20), (31, 40)] {
        let now = start + Duration::from_millis(overdue);
        assert_eq!(
            advance_redraw_deadline(start, now, interval),
            start + Duration::from_millis(next)
        );
    }
    assert_eq!(
        advance_redraw_deadline(start + interval, start, interval),
        start + interval
    );
    assert_eq!(
        advance_redraw_deadline(start, start + interval, Duration::ZERO),
        start + interval
    );
}
