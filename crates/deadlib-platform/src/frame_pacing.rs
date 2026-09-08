//! Frame deadlines, redraw tracking, and window-aware event-loop scheduling.
//!
//! [`FrameLoopState`] is owned by the event-loop thread for the window's lifetime.
//! Callers supply timestamps and policy, including whether an unfocused window may
//! be throttled, then apply the returned [`FrameWaitPlan`] to their event loop.
//! Record actual redraw requests with [`FrameLoopState::note_redraw_requested`]
//! and consume their timing when the redraw arrives. Planning does not mark a
//! redraw pending; repeated requests preserve the first timestamp and reason.
//!
//! Scheduling uses fixed-size state without allocation, locking, or I/O. Missed
//! deadlines advance past the supplied time in constant work, preserving cadence.
//! Scheduled waits wake one millisecond early and poll through the deadline guard.

use std::time::{Duration, Instant};

/// Background redraw period for occluded, inactive, or safely throttled unfocused windows.
pub const BACKGROUND_REDRAW_INTERVAL: Duration = Duration::from_millis(67);

/// Converts a frame-rate cap to its period; zero means uncapped.
#[inline(always)]
#[must_use]
pub fn frame_interval_for_max_fps(max_fps: u16) -> Option<Duration> {
    if max_fps == 0 {
        None
    } else {
        Some(Duration::from_secs_f64(1.0 / f64::from(max_fps)))
    }
}

/// Advances a due deadline past `now` while retaining its interval cadence.
///
/// Future deadlines stay unchanged. A zero interval returns `now`; unrepresentable
/// catch-up steps fall back to `now + interval`, or `now` if that also overflows.
#[inline(always)]
#[must_use]
pub fn advance_redraw_deadline(deadline: Instant, now: Instant, interval: Duration) -> Instant {
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

/// Combines frame-rate policy with window visibility, surface availability, and focus.
///
/// Vsync suppresses the explicit frame-rate cap. Background throttling takes the
/// slower period when both caps apply. `throttle_unfocused` only affects focus;
/// occlusion and inactive surfaces always enable background throttling.
#[inline(always)]
#[must_use]
pub fn window_frame_interval_state(
    vsync_enabled: bool,
    max_fps_interval: Option<Duration>,
    window_occluded: bool,
    surface_active: bool,
    window_focused: bool,
    throttle_unfocused: bool,
) -> FrameIntervalState {
    let base = (!vsync_enabled).then_some(max_fps_interval).flatten();
    let background =
        (window_occluded || !surface_active || (!window_focused && throttle_unfocused))
            .then_some(BACKGROUND_REDRAW_INTERVAL);
    match (base, background) {
        (Some(base), Some(background)) => FrameIntervalState {
            interval: Some(base.max(background)),
            reason: FrameIntervalReason::MaxFpsBackground,
        },
        (Some(interval), None) => FrameIntervalState {
            interval: Some(interval),
            reason: FrameIntervalReason::MaxFps,
        },
        (None, Some(interval)) => FrameIntervalState {
            interval: Some(interval),
            reason: FrameIntervalReason::Background,
        },
        (None, None) => FrameIntervalState {
            interval: None,
            reason: FrameIntervalReason::None,
        },
    }
}

/// Reports whether the window can receive foreground input.
#[inline(always)]
#[must_use]
pub const fn foreground_input_active(window_focused: bool, surface_active: bool) -> bool {
    window_focused && surface_active
}

/// Reports whether occlusion or an inactive surface makes drawing unnecessary.
#[inline(always)]
#[must_use]
pub const fn should_skip_compose_and_draw(window_occluded: bool, surface_active: bool) -> bool {
    window_occluded || !surface_active
}

/// Reads the monotonic clock for diagnostic elapsed microseconds, saturating at `u32::MAX`.
#[inline(always)]
#[must_use]
pub fn elapsed_us_since(started: Instant) -> u32 {
    micros_to_u32(started.elapsed().as_micros())
}

/// Measures supplied timestamps in microseconds, clamping negative and overflowing durations.
#[inline(always)]
#[must_use]
pub fn elapsed_us_between(later: Instant, earlier: Instant) -> u32 {
    micros_to_u32(
        later
            .checked_duration_since(earlier)
            .unwrap_or(Duration::ZERO)
            .as_micros(),
    )
}

#[inline(always)]
const fn micros_to_u32(micros: u128) -> u32 {
    if micros > u32::MAX as u128 {
        u32::MAX
    } else {
        micros as u32
    }
}

/// Policy responsible for the selected frame interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameIntervalReason {
    /// No scheduler cap applies.
    None,
    /// The explicit frame-rate cap applies.
    MaxFps,
    /// Window background throttling applies.
    Background,
    /// Both caps apply, using the slower interval.
    MaxFpsBackground,
}

impl FrameIntervalReason {
    /// Returns the diagnostic label for this interval policy.
    #[inline(always)]
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::MaxFps => "max_fps",
            Self::Background => "background",
            Self::MaxFpsBackground => "max_fps+background",
        }
    }

    /// Returns the diagnostic reason attached to a scheduled redraw request.
    #[inline(always)]
    #[must_use]
    pub const fn redraw_reason(self) -> &'static str {
        match self {
            Self::None => "scheduled",
            Self::MaxFps => "scheduled_maxfps",
            Self::Background => "scheduled_background",
            Self::MaxFpsBackground => "scheduled_maxfps_background",
        }
    }
}

/// Selected scheduler interval and the policy that produced it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameIntervalState {
    /// Minimum period between scheduled redraws, or `None` for uncapped operation.
    pub interval: Option<Duration>,
    /// Policy label used for mode changes and redraw diagnostics.
    pub reason: FrameIntervalReason,
}

/// Event-loop scheduling mode used for change detection and diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameLoopMode {
    /// Request uncapped redraws while polling.
    Poll,
    /// Wait for an outstanding uncapped redraw.
    WaitPending,
    /// Use a deadline determined by the supplied policy and interval.
    Scheduled(FrameIntervalReason, Duration),
}

/// Latency and reason of a consumed redraw request.
#[derive(Clone, Copy, Debug)]
pub struct RedrawRequestTiming {
    /// Saturating microseconds between the first request and its redraw.
    pub request_to_redraw_us: u32,
    /// First request reason, or `"external"` when no request was recorded.
    pub reason: &'static str,
}

/// First outstanding redraw request, retained until consumed or reset.
#[derive(Clone, Copy, Debug)]
pub struct RedrawRequestState {
    requested_at: Option<Instant>,
    reason: &'static str,
}

impl RedrawRequestState {
    /// Creates an empty request tracker.
    #[inline(always)]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            requested_at: None,
            reason: "none",
        }
    }

    /// Clears the pending request and its reason.
    #[inline(always)]
    pub const fn reset(&mut self) {
        *self = Self::new();
    }

    /// Records the first pending request without replacing it on repeated requests.
    #[inline(always)]
    pub const fn note_requested(&mut self, now: Instant, reason: &'static str) {
        if self.requested_at.is_none() {
            self.requested_at = Some(now);
            self.reason = reason;
        }
    }

    /// Consumes the pending request, or reports an external redraw when none exists.
    #[inline(always)]
    pub fn take_timing(&mut self, now: Instant) -> RedrawRequestTiming {
        let requested_at = self.requested_at.take();
        let reason = if requested_at.is_some() {
            self.reason
        } else {
            "external"
        };
        self.reason = "none";
        RedrawRequestTiming {
            request_to_redraw_us: requested_at
                .map(|at| elapsed_us_between(now, at))
                .unwrap_or_default(),
            reason,
        }
    }

    /// Reports whether a recorded request is awaiting its redraw.
    #[inline(always)]
    #[must_use]
    pub const fn pending(&self) -> bool {
        self.requested_at.is_some()
    }
}

impl Default for RedrawRequestState {
    fn default() -> Self {
        Self::new()
    }
}

/// Tracks scheduling mode changes without emitting or allocating diagnostic messages.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameLoopModeTracker {
    last: Option<FrameLoopMode>,
}

impl FrameLoopModeTracker {
    /// Creates a tracker with no previously observed mode.
    #[inline(always)]
    #[must_use]
    pub const fn new() -> Self {
        Self { last: None }
    }

    /// Clears the previous mode so the next observation reports a change.
    #[inline(always)]
    pub const fn reset(&mut self) {
        self.last = None;
    }

    /// Records a mode and reports whether it differs from the previous observation.
    #[inline(always)]
    pub fn note(&mut self, mode: FrameLoopMode) -> bool {
        if self.last == Some(mode) {
            return false;
        }
        self.last = Some(mode);
        true
    }
}

// Wake early to poll through the last millisecond before the redraw deadline.
const SCHEDULED_REDRAW_POLL_GUARD: Duration = Duration::from_micros(1_000);

/// Event-loop wait action, independent of the windowing backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameWaitControl {
    /// Poll immediately.
    Poll,
    /// Wait for an event without a scheduler deadline.
    Wait,
    /// Wait until the supplied instant or an earlier event.
    WaitUntil(Instant),
}

/// Scheduler decision applied by the event-loop owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameWaitPlan {
    /// Scheduling mode for diagnostics.
    pub mode: FrameLoopMode,
    /// Wait action to apply after handling the redraw decision.
    pub control: FrameWaitControl,
    /// Reason to request a redraw if none is already pending.
    pub redraw_reason: Option<&'static str>,
}

/// Window state, redraw deadlines, and request tracking owned by one event loop.
#[derive(Debug)]
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
    /// Starts scheduling at `now` with an unfocused, non-occluded window.
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

    /// Updates the frame-rate cap and resets scheduling at the supplied time.
    pub fn set_max_fps(&mut self, max_fps: u16, now: Instant) {
        self.frame_interval = frame_interval_for_max_fps(max_fps);
        self.reset_schedule(now);
    }

    /// Makes the next redraw due now and clears request and mode tracking.
    pub const fn reset_schedule(&mut self, now: Instant) {
        self.next_redraw_at = now;
        self.redraw_request.reset();
        self.mode.reset();
    }

    /// Records an actual request, retaining the first pending timestamp and reason.
    #[inline(always)]
    pub const fn note_redraw_requested(&mut self, now: Instant, reason: &'static str) {
        self.redraw_request.note_requested(now, reason);
    }

    /// Consumes the pending request and returns latency in microseconds and its reason.
    #[inline(always)]
    pub fn take_redraw_request_timing(&mut self, now: Instant) -> (u32, &'static str) {
        let timing = self.redraw_request.take_timing(now);
        (timing.request_to_redraw_us, timing.reason)
    }

    /// Reports whether an actual redraw request is outstanding.
    #[inline(always)]
    pub const fn redraw_pending(&self) -> bool {
        self.redraw_request.pending()
    }

    /// Selects an interval using window state and explicit caller policy.
    pub fn interval_state(&self, vsync: bool, throttle_unfocused: bool) -> FrameIntervalState {
        window_frame_interval_state(
            vsync,
            self.frame_interval,
            self.window_occluded,
            self.surface_active,
            self.window_focused,
            throttle_unfocused,
        )
    }

    /// Records the applied mode and reports whether it changed.
    #[inline(always)]
    pub fn note_mode(&mut self, mode: FrameLoopMode) -> bool {
        self.mode.note(mode)
    }

    /// Updates focus and reports a change; the caller decides when to reset scheduling.
    pub const fn set_window_focus(&mut self, focused: bool) -> bool {
        if self.window_focused == focused {
            return false;
        }
        self.window_focused = focused;
        true
    }

    /// Updates occlusion and reports a change without resetting scheduling.
    pub const fn set_window_occluded(&mut self, occluded: bool) -> bool {
        if self.window_occluded == occluded {
            return false;
        }
        self.window_occluded = occluded;
        true
    }

    /// Updates surface availability and reports a change without resetting scheduling.
    pub const fn set_surface_active(&mut self, active: bool) -> bool {
        if self.surface_active == active {
            return false;
        }
        self.surface_active = active;
        true
    }

    /// Reports whether the current window state makes drawing unnecessary.
    #[inline(always)]
    pub const fn should_skip_compose_and_draw(&self) -> bool {
        should_skip_compose_and_draw(self.window_occluded, self.surface_active)
    }

    fn advance_if_due(&mut self, now: Instant, interval: Duration) -> bool {
        if now < self.next_redraw_at {
            return false;
        }
        self.next_redraw_at = advance_redraw_deadline(self.next_redraw_at, now, interval);
        true
    }

    /// Advances due deadlines and plans the next wait without recording a redraw request.
    ///
    /// Uncapped loops wait while a request is pending. Scheduled loops retain their
    /// cadence even with a pending request; callers coalesce actual redraw requests.
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

    /// Returns the configured frame-rate period before vsync and background policy.
    #[inline(always)]
    pub const fn frame_interval(&self) -> Option<Duration> {
        self.frame_interval
    }

    /// Returns the last supplied focus state.
    #[inline(always)]
    pub const fn window_focused(&self) -> bool {
        self.window_focused
    }

    /// Returns the last supplied occlusion state.
    #[inline(always)]
    pub const fn window_occluded(&self) -> bool {
        self.window_occluded
    }

    /// Returns the last supplied surface availability.
    #[inline(always)]
    pub const fn surface_active(&self) -> bool {
        self.surface_active
    }
}
