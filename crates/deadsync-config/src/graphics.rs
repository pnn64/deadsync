//! Display setting choices derived from the host monitor's advertised modes.

use deadlib_platform::display;
use deadsync_core::display::VideoMode;

pub use deadlib_platform::display::{
    DISPLAY_ASPECT_RATIO_LABELS, display_aspect_choice_for_ratio, display_aspect_ratio_from_choice,
};

use crate::options::clamped_max_fps;

/// Rebuild the resolution list and return the index of the current size.
/// A nonzero current size stays selectable even outside the configured aspect.
pub fn rebuild_resolution_choices(
    resolutions: &mut Vec<(u32, u32)>,
    modes: &[VideoMode],
    aspect_choice: usize,
    current: (u32, u32),
) -> usize {
    let aspect = display::display_aspect_label_from_choice(aspect_choice);
    resolutions.clear();
    resolutions.extend(
        modes
            .iter()
            .map(|mode| (mode.width, mode.height))
            .filter(|&(w, h)| h > 0 && display::aspect_matches(w, h, aspect)),
    );
    if resolutions.is_empty() {
        resolutions.extend_from_slice(display::resolution_presets_for_aspect(aspect));
    }
    if current.0 > 0 && current.1 > 0 {
        resolutions.push(current);
    }
    resolutions.sort_unstable();
    resolutions.dedup();
    resolutions
        .iter()
        .position(|&size| size == current)
        .unwrap_or(0)
}

/// Rebuild the refresh rates for the selected fullscreen mode and return the
/// nearest selection less than 10 Hz from the requested rate. An empty mode
/// list or a rate outside that tolerance selects Default. Windowed mode
/// passes an empty slice.
pub fn rebuild_refresh_rate_choices(
    rates: &mut Vec<u32>,
    modes: &[VideoMode],
    (width, height): (u32, u32),
    requested_rate: u32,
) -> usize {
    rates.clear();
    rates.extend(
        modes
            .iter()
            .filter(|mode| mode.width == width && mode.height == height)
            .map(|mode| mode.refresh_rate_millihertz),
    );
    rates.sort_unstable();
    rates.dedup();
    // Default keeps its own position, separate from the hardware list.
    rates.insert(0, 0);
    rates
        .iter()
        .enumerate()
        .min_by_key(|(_, rate)| rate.abs_diff(requested_rate))
        .filter(|(_, rate)| rate.abs_diff(requested_rate) < 10_000)
        .map_or(0, |(index, _)| index)
}

/// Return the FPS cap to show when entering its editor. With no explicit cap,
/// seed from three times the rounded refresh rate for the selected display.
#[must_use]
pub fn max_fps_seed(
    configured_cap: u16,
    selected_refresh: u32,
    modes: &[VideoMode],
    fullscreen_resolution: Option<(u32, u32)>,
) -> u16 {
    if configured_cap != 0 {
        return clamped_max_fps(configured_cap);
    }
    let refresh = if selected_refresh != 0 {
        selected_refresh
    } else {
        fullscreen_resolution
            .and_then(|(width, height)| {
                modes
                    .iter()
                    .filter(|mode| mode.width == width && mode.height == height)
                    .map(|mode| mode.refresh_rate_millihertz)
                    .max()
            })
            .or_else(|| modes.iter().map(|mode| mode.refresh_rate_millihertz).max())
            .unwrap_or(60_000)
    };
    let recommended = (refresh.saturating_add(500) / 1_000).saturating_mul(3);
    clamped_max_fps(recommended.min(u32::from(u16::MAX)) as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modes() -> [VideoMode; 4] {
        [
            (1280, 60_000),
            (1920, 144_000),
            (1280, 60_000),
            (1280, 75_000),
        ]
        .map(|(width, refresh_rate_millihertz)| VideoMode {
            width,
            height: 720,
            refresh_rate_millihertz,
        })
    }

    #[test]
    fn refresh_choices_match_the_resolution_and_use_a_strict_tolerance() {
        let modes = modes();
        let mut rates = Vec::new();
        let resolution = (1280, 720);
        let chosen = rebuild_refresh_rate_choices(&mut rates, &modes, resolution, 59_940);
        assert_eq!(rates, [0, 60_000, 75_000]);
        assert_eq!(rates[chosen], 60_000);
        assert_eq!(
            rebuild_refresh_rate_choices(&mut rates, &modes, resolution, 85_000),
            0
        );
        assert_eq!(
            rebuild_refresh_rate_choices(&mut rates, &[], resolution, 60_000),
            0
        );
        assert_eq!(rates, [0]);
    }

    #[test]
    fn uncapped_fps_seed_prefers_the_active_mode_then_the_monitor() {
        let modes = modes();
        assert_eq!(max_fps_seed(0, 0, &modes, Some((1280, 720))), 225);
        assert_eq!(max_fps_seed(0, 0, &modes, Some((640, 480))), 432);
        assert_eq!(max_fps_seed(0, 59_940, &modes, None), 180);
        assert_eq!(max_fps_seed(120, 0, &modes, None), 120);
        assert_eq!(
            max_fps_seed(0, u32::MAX, &[], None),
            crate::options::MAX_FPS_MAX
        );
    }
}
