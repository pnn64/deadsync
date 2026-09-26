use deadlib_platform::frame_pacing::{
    FrameIntervalReason, FrameIntervalState, FrameLoopMode, FrameLoopState, FrameWaitControl,
    advance_redraw_deadline,
};
use std::hint::black_box;
use std::time::{Duration, Instant};

#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

// Frozen deadline calculation from 1d0d58cbf, including its overflow fallbacks.
#[inline(always)]
fn advance_before(deadline: Instant, now: Instant, interval: Duration) -> Instant {
    if deadline > now {
        return deadline;
    }
    let step_ns = interval.as_nanos();
    if step_ns == 0 {
        return now;
    }
    let overdue_ns = now.duration_since(deadline).as_nanos();
    let steps = overdue_ns / step_ns + 1;
    if steps <= u128::from(u32::MAX)
        && let Some(delta) = interval.checked_mul(steps as u32)
        && let Some(next) = deadline.checked_add(delta)
    {
        return next;
    }
    now.checked_add(interval).unwrap_or(now)
}

fn duration(nanos: u128) -> Duration {
    Duration::new(
        (nanos / 1_000_000_000) as u64,
        (nanos % 1_000_000_000) as u32,
    )
}

fn last_representable_instant(start: Instant) -> Instant {
    let mut low = 0;
    let mut high = Duration::MAX.as_nanos();
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if start.checked_add(duration(middle)).is_some() {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    start.checked_add(duration(low)).unwrap()
}

#[test]
fn deadlines_match_baseline_at_boundaries_and_overflow() {
    let start = Instant::now();
    let last = last_representable_instant(start);
    for deadline in [start, last - Duration::from_nanos(10)] {
        for interval in [
            Duration::ZERO,
            Duration::from_nanos(1),
            Duration::from_nanos(8),
            Duration::from_nanos(16_666_667),
            Duration::from_millis(67),
            Duration::from_secs(1),
            Duration::MAX,
        ] {
            let nanos = interval.as_nanos();
            let mut offsets = vec![0, 1, 7, 8, 9, 10];
            for multiple in [1, 2, 3, u128::from(u32::MAX), u128::from(u32::MAX) + 1] {
                let boundary = nanos * multiple;
                for offset in [boundary.saturating_sub(1), boundary, boundary + 1] {
                    if offset <= Duration::MAX.as_nanos() {
                        offsets.push(offset);
                    }
                }
            }
            for offset in offsets {
                if let Some(now) = deadline.checked_add(duration(offset)) {
                    assert_eq!(
                        advance_redraw_deadline(deadline, now, interval),
                        advance_before(deadline, now, interval),
                        "interval={interval:?}, overdue={offset} ns"
                    );
                }
            }
            let now = deadline - Duration::from_nanos(1);
            assert_eq!(advance_redraw_deadline(deadline, now, interval), deadline);
        }
    }
}

#[test]
fn deadlines_match_baseline_for_randomized_offsets() {
    let deadline = Instant::now();
    let mut seed = 0x1234_5678_9abc_def0u64;
    for case in 0..20_000 {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let interval = Duration::from_nanos(seed % 70_000_001);
        let overdue = match case % 4 {
            0 => Duration::ZERO,
            1 => interval / 2,
            2 => interval,
            _ => Duration::from_nanos(seed % 10_000_000_000),
        };
        let now = deadline + overdue;
        assert_eq!(
            advance_redraw_deadline(deadline, now, interval),
            advance_before(deadline, now, interval),
            "case={case}, interval={interval:?}, overdue={overdue:?}"
        );
    }
}

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
            deadline = advance_before(deadline, now, interval);
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
#[ignore = "manual release benchmark; --ignored --nocapture --test-threads=1"]
fn benchmark_deadline_advance() {
    type Advance = fn(Instant, Instant, Duration) -> Instant;
    let variants: [(&str, Advance); 2] = [
        ("before", advance_before),
        ("after", advance_redraw_deadline),
    ];
    let start = Instant::now();
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (name, period, offsets) in [
        ("240fps-due", 4_166_667, [0; 8]),
        (
            "240fps-jitter",
            4_166_667,
            [0, 1, 1000, 5000, 50_000, 100_000, 500_000, 1_000_000],
        ),
        (
            "60fps-jitter",
            16_666_667,
            [0, 1, 1000, 5000, 50_000, 100_000, 500_000, 1_000_000],
        ),
        (
            "background-jitter",
            67_000_000,
            [0, 1, 1000, 5000, 50_000, 100_000, 500_000, 1_000_000],
        ),
        (
            "240fps-missed",
            4_166_667,
            [
                4_166_667,
                5_000_000,
                10_000_000,
                16_666_668,
                20_000_000,
                50_000_000,
                100_000_000,
                1_000_000_000,
            ],
        ),
        (
            "240fps-mixed",
            4_166_667,
            [
                -1_000_000, -1000, 0, 1000, 50_000, 100_000, 10_000_000, 50_000_000,
            ],
        ),
        ("future", 4_166_667, [-1_000_000; 8]),
        (
            "zero",
            0,
            [0, 1, 1000, 5000, 50_000, 100_000, 500_000, 1_000_000],
        ),
    ] {
        let interval = Duration::from_nanos(period);
        let times: [Instant; 128] = std::array::from_fn(|index| {
            let offset: i64 = offsets[index % offsets.len()];
            if offset < 0 {
                start - Duration::from_nanos(offset.unsigned_abs())
            } else {
                start + Duration::from_nanos(offset as u64)
            }
        });
        for index in if reverse { [1, 0] } else { [0, 1] } {
            let (label, advance) = variants[index];
            let advance = black_box(advance);
            perf::measure_sampled(&format!("{name}/{label}"), 32768, times.len(), || {
                for &now in black_box(&times) {
                    black_box(advance(black_box(start), now, black_box(interval)));
                }
            });
        }
    }
}
