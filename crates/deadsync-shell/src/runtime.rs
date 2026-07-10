use crate::{
    FrameLoopState, FrameStatsController, GameplayInputTrace, GameplayPacingTrace,
    ShellInteractionState, StutterDiagRecorder, TransitionState,
};
use deadlib_present::space::{self, Metrics};
use deadlib_render::{PresentModePolicy, PresentStats};
use deadsync_assets::screenshot::ScreenshotRuntimeState;
use deadsync_config::app_config::{Config, DisplayMode};
use deadsync_config::frame_pacing::{
    FrameIntervalState, FrameLoopMode, OverlayMode, StutterSampleRing,
};
use deadsync_profile::PlayerSide;
use deadsync_screens::Screen;
use std::time::{Duration, Instant};
use winit::dpi::PhysicalPosition;

/// Mutable process-shell state shared with the root runtime adapter.
pub struct ShellState {
    pub frame_count: u32,
    pub last_title_update: Instant,
    pub last_frame_time: Instant,
    pub last_frame_end_time: Instant,
    pub start_time: Instant,
    pub vsync_enabled: bool,
    pub present_mode_policy: PresentModePolicy,
    pub frame_loop: FrameLoopState,
    pub gameplay_pacing_trace: GameplayPacingTrace,
    pub gameplay_input_trace: GameplayInputTrace,
    pub display_mode: DisplayMode,
    pub display_monitor: usize,
    pub metrics: Metrics,
    pub last_fps: f32,
    pub last_vpf: u32,
    pub last_present_stats: PresentStats,
    pub current_frame_vpf: u32,
    pub overlay_mode: OverlayMode,
    pub stutter_samples: StutterSampleRing,
    pub stutter_diag: StutterDiagRecorder,
    pub frame_stats: FrameStatsController,
    pub transition: TransitionState,
    pub display_width: u32,
    pub display_height: u32,
    pub pending_window_position: Option<PhysicalPosition<i32>>,
    pub interaction: ShellInteractionState,
    pub screenshot: ScreenshotRuntimeState<PlayerSide>,
}

impl ShellState {
    pub fn new(cfg: &Config, overlay_mode: u8) -> Self {
        let metrics = space::metrics_for_window(cfg.display_width, cfg.display_height);
        let now = Instant::now();
        Self {
            frame_count: 0,
            last_title_update: now,
            last_frame_time: now,
            last_frame_end_time: now,
            start_time: now,
            vsync_enabled: cfg.vsync,
            present_mode_policy: cfg.present_mode_policy,
            frame_loop: FrameLoopState::new(
                cfg.max_fps,
                cfg.display_width > 0 && cfg.display_height > 0,
                now,
            ),
            gameplay_pacing_trace: GameplayPacingTrace::new(now),
            gameplay_input_trace: GameplayInputTrace::new(now),
            display_mode: cfg.display_mode(),
            metrics,
            last_fps: 0.0,
            last_vpf: 0,
            last_present_stats: PresentStats::default(),
            current_frame_vpf: 0,
            overlay_mode: OverlayMode::from_code(overlay_mode),
            stutter_samples: StutterSampleRing::new(),
            stutter_diag: StutterDiagRecorder::new(),
            frame_stats: FrameStatsController::new(
                cfg.frame_stats_overlay_anchor,
                cfg.frame_stats_overlay_style,
            ),
            transition: TransitionState::Idle,
            display_width: cfg.display_width,
            display_height: cfg.display_height,
            display_monitor: cfg.display_monitor,
            pending_window_position: None,
            interaction: ShellInteractionState::new(cfg.tab_acceleration),
            screenshot: ScreenshotRuntimeState::new(),
        }
    }

    #[inline(always)]
    pub fn set_max_fps(&mut self, max_fps: u16) {
        self.frame_loop.set_max_fps(max_fps);
    }

    #[inline(always)]
    pub fn set_present_mode_policy(&mut self, policy: PresentModePolicy) {
        self.present_mode_policy = policy;
        self.frame_loop.reset_schedule(Instant::now());
    }

    #[inline(always)]
    pub fn reset_frame_clock(&mut self, now: Instant) {
        self.last_frame_time = now;
        self.last_frame_end_time = now;
        self.frame_loop.reset_schedule(now);
        self.gameplay_pacing_trace.reset(now);
        self.gameplay_input_trace.reset(now);
        self.stutter_diag.reset_frame_clock();
    }

    #[inline(always)]
    pub fn note_redraw_requested(&mut self, now: Instant, reason: &'static str) {
        self.frame_loop.note_redraw_requested(now, reason);
    }

    #[inline(always)]
    pub fn take_redraw_request_timing(&mut self, now: Instant) -> (u32, &'static str) {
        self.frame_loop.take_redraw_request_timing(now)
    }

    #[inline(always)]
    pub fn redraw_pending(&self) -> bool {
        self.frame_loop.redraw_pending()
    }

    #[inline(always)]
    pub fn frame_interval_state(&self, screen: Screen) -> FrameIntervalState {
        self.frame_loop.interval_state(self.vsync_enabled, screen)
    }

    #[inline(always)]
    pub fn note_frame_loop_mode(&mut self, mode: FrameLoopMode) -> bool {
        self.frame_loop.note_mode(mode)
    }

    #[inline(always)]
    pub fn set_window_focus(&mut self, focused: bool, now: Instant) -> bool {
        if !self.frame_loop.set_window_focus(focused) {
            return false;
        }
        self.reset_frame_clock(now);
        true
    }

    #[inline(always)]
    pub fn set_window_occluded(&mut self, occluded: bool, now: Instant) -> bool {
        if !self.frame_loop.set_window_occluded(occluded) {
            return false;
        }
        self.reset_frame_clock(now);
        true
    }

    #[inline(always)]
    pub fn set_surface_active(&mut self, active: bool, now: Instant) -> bool {
        if !self.frame_loop.set_surface_active(active) {
            return false;
        }
        self.reset_frame_clock(now);
        true
    }

    #[inline(always)]
    pub fn background_frame_interval(&self, screen: Screen) -> Option<Duration> {
        self.frame_interval_state(screen).interval
    }

    #[inline(always)]
    pub fn should_skip_compose_and_draw(&self) -> bool {
        self.frame_loop.should_skip_compose_and_draw()
    }

    #[inline(always)]
    pub fn set_overlay_mode(&mut self, mode: u8) {
        let next = OverlayMode::from_code(mode);
        if self.overlay_mode.shows_stutter() && !next.shows_stutter() {
            self.clear_stutter_samples();
        }
        self.overlay_mode = next;
    }

    #[inline(always)]
    pub fn cycle_overlay_mode(&mut self) -> u8 {
        let prev = self.overlay_mode;
        self.overlay_mode = self.overlay_mode.next();
        if prev.shows_stutter() && !self.overlay_mode.shows_stutter() {
            self.clear_stutter_samples();
        }
        self.overlay_mode.code()
    }

    #[inline(always)]
    pub fn push_stutter_sample(
        &mut self,
        at_seconds: f32,
        frame_seconds: f32,
        expected_seconds: f32,
        severity: u8,
    ) {
        self.stutter_samples
            .push(at_seconds, frame_seconds, expected_seconds, severity);
    }

    #[inline(always)]
    pub fn clear_stutter_samples(&mut self) {
        self.stutter_samples.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_copies_runtime_config() {
        let mut cfg = Config::default();
        cfg.display_width = 1600;
        cfg.display_height = 900;
        cfg.display_monitor = 2;
        cfg.vsync = false;
        cfg.tab_acceleration = false;

        let state = ShellState::new(&cfg, 2);

        assert_eq!(state.display_width, 1600);
        assert_eq!(state.display_height, 900);
        assert_eq!(state.display_monitor, 2);
        assert!(!state.vsync_enabled);
        assert_eq!(state.overlay_mode, OverlayMode::FpsAndStutter);
        assert!(matches!(state.transition, TransitionState::Idle));
        assert_eq!(state.interaction.controls().logic_delta(1.0, true), 1.0);
    }

    #[test]
    fn hiding_stutter_overlay_clears_samples() {
        let mut state = ShellState::new(&Config::default(), 2);
        state.push_stutter_sample(1.0, 0.05, 0.01, 2);
        assert_eq!(state.stutter_samples.visible(1.0).len(), 1);

        state.set_overlay_mode(1);

        assert!(state.stutter_samples.visible(1.0).is_empty());
        assert_eq!(state.overlay_mode, OverlayMode::Fps);
    }
}
