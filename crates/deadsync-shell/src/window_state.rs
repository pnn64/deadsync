use std::time::Instant;

use deadlib_platform::display::FullscreenType;
use deadsync_config::app_config::DisplayMode;

use crate::ShellState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShellWindowEventPlan {
    pub changed: bool,
    pub sync_gameplay_capture: bool,
    pub clear_live_input: bool,
    pub redraw_reason: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowMinimizePlan {
    None,
    Minimize,
    Restore,
}

const fn unchanged_window_event() -> ShellWindowEventPlan {
    ShellWindowEventPlan {
        changed: false,
        sync_gameplay_capture: false,
        clear_live_input: false,
        redraw_reason: None,
    }
}

pub fn apply_shell_window_focus(
    shell: &mut ShellState,
    focused: bool,
    now: Instant,
) -> ShellWindowEventPlan {
    if !shell.set_window_focus(focused, now) {
        return unchanged_window_event();
    }
    ShellWindowEventPlan {
        changed: true,
        sync_gameplay_capture: true,
        clear_live_input: !focused,
        redraw_reason: focused.then_some("focus"),
    }
}

pub fn apply_shell_surface_active(
    shell: &mut ShellState,
    active: bool,
    now: Instant,
) -> ShellWindowEventPlan {
    if !shell.set_surface_active(active, now) {
        return unchanged_window_event();
    }
    ShellWindowEventPlan {
        changed: true,
        sync_gameplay_capture: true,
        clear_live_input: false,
        redraw_reason: active.then_some("surface_active"),
    }
}

pub fn apply_shell_window_occlusion(
    shell: &mut ShellState,
    occluded: bool,
    now: Instant,
) -> ShellWindowEventPlan {
    if !shell.set_window_occluded(occluded, now) {
        return unchanged_window_event();
    }
    ShellWindowEventPlan {
        changed: true,
        sync_gameplay_capture: true,
        clear_live_input: false,
        redraw_reason: (!occluded && shell.frame_loop.surface_active()).then_some("occluded"),
    }
}

pub const fn exclusive_fullscreen_focus_plan(
    display_mode: DisplayMode,
    focused: bool,
    minimized: bool,
) -> WindowMinimizePlan {
    match display_mode {
        DisplayMode::Fullscreen(FullscreenType::Exclusive) if !focused => {
            WindowMinimizePlan::Minimize
        }
        DisplayMode::Fullscreen(FullscreenType::Exclusive) if minimized => {
            WindowMinimizePlan::Restore
        }
        _ => WindowMinimizePlan::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_config::app_config::Config;

    #[test]
    fn focus_plan_clears_input_only_on_focus_loss() {
        let mut shell = ShellState::new(&Config::default(), 0);
        let now = Instant::now();

        let focused = apply_shell_window_focus(&mut shell, true, now);
        assert_eq!(
            focused,
            ShellWindowEventPlan {
                changed: true,
                sync_gameplay_capture: true,
                clear_live_input: false,
                redraw_reason: Some("focus"),
            }
        );

        assert_eq!(
            apply_shell_window_focus(&mut shell, true, now),
            unchanged_window_event()
        );

        let unfocused = apply_shell_window_focus(&mut shell, false, now);
        assert_eq!(
            unfocused,
            ShellWindowEventPlan {
                changed: true,
                sync_gameplay_capture: true,
                clear_live_input: true,
                redraw_reason: None,
            }
        );
    }

    #[test]
    fn surface_and_occlusion_plan_redraw_only_when_visible() {
        let mut shell = ShellState::new(&Config::default(), 0);
        let now = Instant::now();

        assert_eq!(
            apply_shell_surface_active(&mut shell, false, now),
            ShellWindowEventPlan {
                changed: true,
                sync_gameplay_capture: true,
                clear_live_input: false,
                redraw_reason: None,
            }
        );
        assert_eq!(
            apply_shell_surface_active(&mut shell, true, now),
            ShellWindowEventPlan {
                changed: true,
                sync_gameplay_capture: true,
                clear_live_input: false,
                redraw_reason: Some("surface_active"),
            }
        );
        assert_eq!(
            apply_shell_window_occlusion(&mut shell, true, now),
            ShellWindowEventPlan {
                changed: true,
                sync_gameplay_capture: true,
                clear_live_input: false,
                redraw_reason: None,
            }
        );
        assert_eq!(
            apply_shell_window_occlusion(&mut shell, false, now),
            ShellWindowEventPlan {
                changed: true,
                sync_gameplay_capture: true,
                clear_live_input: false,
                redraw_reason: Some("occluded"),
            }
        );
    }

    #[test]
    fn exclusive_fullscreen_focus_plan_minimizes_and_restores() {
        let mode = DisplayMode::Fullscreen(FullscreenType::Exclusive);
        assert_eq!(
            exclusive_fullscreen_focus_plan(mode, false, false),
            WindowMinimizePlan::Minimize
        );
        assert_eq!(
            exclusive_fullscreen_focus_plan(mode, true, true),
            WindowMinimizePlan::Restore
        );
        assert_eq!(
            exclusive_fullscreen_focus_plan(mode, true, false),
            WindowMinimizePlan::None
        );
        assert_eq!(
            exclusive_fullscreen_focus_plan(DisplayMode::Windowed, false, false),
            WindowMinimizePlan::None
        );
    }
}
