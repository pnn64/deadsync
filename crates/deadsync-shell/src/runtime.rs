use crate::{
    FrameLoopState, FrameStatsController, FrameStatsSample, GameplayInputTrace,
    GameplayPacingTrace, ShellInteractionState, StutterDiagRecorder, TransitionState,
};
use deadlib_present::space::{self, Metrics};
use deadlib_render::{DrawStats, PresentModePolicy, PresentStats};
use deadsync_assets::screenshot::ScreenshotRuntimeState;
use deadsync_config::app_config::{Config, DisplayMode};
use deadsync_config::frame_pacing::{
    FrameIntervalState, FrameLoopMode, OverlayMode, StutterSampleRing, seconds_to_us_u32,
    stutter_severity,
};
use deadsync_profile::PlayerSide;
use deadsync_theme_simply_love::screens::SimplyLoveScreen as Screen;
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

    #[inline(always)]
    pub fn expected_frame_seconds(&self, screen: Screen) -> f32 {
        if self.last_fps > 0.0 {
            return 1.0 / self.last_fps;
        }
        if let Some(interval) = self.background_frame_interval(screen) {
            return interval.as_secs_f32();
        }
        if self.vsync_enabled {
            return 1.0 / 60.0;
        }
        0.0
    }

    #[inline(always)]
    pub fn update_stutter_samples(
        &mut self,
        screen: Screen,
        frame_seconds: f32,
        total_elapsed: f32,
    ) {
        if !self.overlay_mode.shows_stutter() {
            return;
        }
        let expected = self.expected_frame_seconds(screen);
        let severity = stutter_severity(frame_seconds, expected);
        if severity != 0 {
            self.push_stutter_sample(total_elapsed, frame_seconds, expected, severity);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_frame_stats_sample(
        &mut self,
        frame_host_nanos: u64,
        frame_seconds: f32,
        input_us: u32,
        update_us: u32,
        compose_us: u32,
        upload_us: u32,
        draw_us: u32,
        draw_stats: DrawStats,
        display_error_seconds: f32,
        display_catching_up: bool,
    ) {
        if !self.frame_stats.enabled() {
            return;
        }
        let display_error_us = (f64::from(display_error_seconds) * 1_000_000.0)
            .round()
            .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
        self.frame_stats.record(FrameStatsSample {
            host_nanos: frame_host_nanos.max(1),
            frame_us: seconds_to_us_u32(frame_seconds),
            input_us,
            update_us,
            compose_us,
            upload_us,
            draw_us,
            gpu_wait_us: draw_stats.gpu_wait_us,
            display_error_us,
            catching_up: display_catching_up,
        });
    }

    #[inline(always)]
    pub fn update_fps_stats(&mut self, now: Instant) {
        self.frame_count += 1;
        let elapsed = now.duration_since(self.last_title_update);
        if elapsed.as_secs_f32() >= 1.0 {
            self.last_fps = self.frame_count as f32 / elapsed.as_secs_f32();
            self.last_vpf = self.current_frame_vpf;
            self.frame_count = 0;
            self.last_title_update = now;
        }
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

    #[test]
    fn measured_fps_drives_expected_frame_time_and_stutter_sampling() {
        let mut state = ShellState::new(&Config::default(), 2);
        state.last_fps = 100.0;
        assert!((state.expected_frame_seconds(Screen::Gameplay) - 0.01).abs() < f32::EPSILON);

        state.update_stutter_samples(Screen::Gameplay, 0.03, 5.0);
        let samples = state.stutter_samples.visible(5.0);
        assert_eq!(samples.len(), 1);
        assert!((samples[0].frame_multiple - 3.0).abs() < 0.001);
    }

    #[test]
    fn fps_stats_roll_over_after_one_second() {
        let mut state = ShellState::new(&Config::default(), 0);
        let now = state.last_title_update + Duration::from_secs(2);
        state.frame_count = 119;
        state.current_frame_vpf = 3;

        state.update_fps_stats(now);

        assert_eq!(state.last_fps, 60.0);
        assert_eq!(state.last_vpf, 3);
        assert_eq!(state.frame_count, 0);
        assert_eq!(state.last_title_update, now);
    }
}
