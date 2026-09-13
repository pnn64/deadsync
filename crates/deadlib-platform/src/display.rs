#[cfg(not(target_os = "windows"))]
use log::info;
use log::warn;
use std::collections::HashMap;
use std::str::FromStr;
use winit::{
    dpi::PhysicalPosition, event_loop::ActiveEventLoop, monitor::MonitorHandle, window::Fullscreen,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullscreenType {
    Exclusive,
    Borderless,
}

impl FullscreenType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exclusive => "Exclusive",
            Self::Borderless => "Borderless",
        }
    }
}

#[must_use]
pub const fn fullscreen_type_choice_index(fullscreen_type: FullscreenType) -> usize {
    match fullscreen_type {
        FullscreenType::Exclusive => 0,
        FullscreenType::Borderless => 1,
    }
}

#[must_use]
pub const fn fullscreen_type_from_choice(idx: usize) -> FullscreenType {
    match idx {
        1 => FullscreenType::Borderless,
        _ => FullscreenType::Exclusive,
    }
}

pub const DISPLAY_ASPECT_RATIO_LABELS: [&str; 4] = ["16:9", "16:10", "4:3", "1:1"];

pub const DEFAULT_RESOLUTION_CHOICES: &[(u32, u32)] = &[
    (1920, 1080),
    (1600, 900),
    (1280, 720),
    (1024, 768),
    (800, 600),
];

#[must_use]
pub fn display_aspect_choice_index(width: u32, height: u32) -> usize {
    if height == 0 {
        return 0;
    }

    if let Some(idx) = DISPLAY_ASPECT_RATIO_LABELS
        .iter()
        .position(|label| aspect_matches(width, height, label))
    {
        return idx;
    }

    let ratio = width as f32 / height as f32;
    let mut best_idx = 0;
    let mut best_delta = f32::INFINITY;
    for (idx, label) in DISPLAY_ASPECT_RATIO_LABELS.iter().enumerate() {
        let target = match *label {
            "16:9" => 16.0 / 9.0,
            "16:10" => 16.0 / 10.0,
            "4:3" => 4.0 / 3.0,
            "1:1" => 1.0,
            _ => continue,
        };
        let delta = (ratio - target).abs();
        if delta < best_delta {
            best_delta = delta;
            best_idx = idx;
        }
    }
    best_idx
}

#[must_use]
pub fn display_aspect_label_from_choice(idx: usize) -> &'static str {
    DISPLAY_ASPECT_RATIO_LABELS
        .get(idx)
        .copied()
        .unwrap_or("16:9")
}

pub fn push_unique_resolution(target: &mut Vec<(u32, u32)>, width: u32, height: u32) {
    if !target.iter().any(|&(w, h)| w == width && h == height) {
        target.push((width, height));
    }
}

#[must_use]
pub fn preset_resolutions_for_aspect(label: &str) -> Vec<(u32, u32)> {
    match label.to_ascii_lowercase().as_str() {
        "16:9" => vec![(1280, 720), (1600, 900), (1920, 1080)],
        "16:10" => vec![(1280, 800), (1440, 900), (1680, 1050), (1920, 1200)],
        "4:3" => vec![
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 960),
            (1600, 1200),
        ],
        "1:1" => vec![(342, 342), (456, 456), (608, 608), (810, 810), (1080, 1080)],
        _ => DEFAULT_RESOLUTION_CHOICES.to_vec(),
    }
}

#[must_use]
pub fn aspect_matches(width: u32, height: u32, label: &str) -> bool {
    let ratio = width as f32 / height as f32;
    match label {
        "16:9" => (ratio - 1.7777).abs() < 0.05,
        "16:10" => (ratio - 1.6).abs() < 0.05,
        "4:3" => (ratio - 1.3333).abs() < 0.05,
        "1:1" => (ratio - 1.0).abs() < 0.05,
        _ => true,
    }
}

impl FromStr for FullscreenType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "exclusive" => Ok(Self::Exclusive),
            "borderless" => Ok(Self::Borderless),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fullscreen_type_choices_match_options_order() {
        assert_eq!(fullscreen_type_choice_index(FullscreenType::Exclusive), 0);
        assert_eq!(fullscreen_type_choice_index(FullscreenType::Borderless), 1);
        assert_eq!(fullscreen_type_from_choice(0), FullscreenType::Exclusive);
        assert_eq!(fullscreen_type_from_choice(1), FullscreenType::Borderless);
        assert_eq!(fullscreen_type_from_choice(99), FullscreenType::Exclusive);
    }

    #[test]
    fn display_aspect_choice_matches_common_resolutions() {
        assert_eq!(display_aspect_choice_index(1920, 1080), 0);
        assert_eq!(display_aspect_choice_index(1680, 1050), 1);
        assert_eq!(display_aspect_choice_index(1024, 768), 2);
        assert_eq!(display_aspect_choice_index(810, 810), 3);
        assert_eq!(display_aspect_choice_index(1920, 0), 0);
    }

    #[test]
    fn display_aspect_labels_fallback_to_wide() {
        assert_eq!(display_aspect_label_from_choice(0), "16:9");
        assert_eq!(display_aspect_label_from_choice(2), "4:3");
        assert_eq!(display_aspect_label_from_choice(99), "16:9");
    }

    #[test]
    fn preset_resolutions_match_aspect_policy() {
        assert_eq!(
            preset_resolutions_for_aspect("16:9"),
            vec![(1280, 720), (1600, 900), (1920, 1080)]
        );
        assert!(preset_resolutions_for_aspect("4:3").contains(&(1024, 768)));
        assert_eq!(
            preset_resolutions_for_aspect("unknown"),
            DEFAULT_RESOLUTION_CHOICES
        );
    }

    #[test]
    fn push_unique_resolution_deduplicates() {
        let mut resolutions = vec![(800, 600)];
        push_unique_resolution(&mut resolutions, 800, 600);
        push_unique_resolution(&mut resolutions, 1024, 768);
        assert_eq!(resolutions, vec![(800, 600), (1024, 768)]);
    }
}

#[derive(Clone, Debug)]
pub struct VideoModeSpec {
    pub width: u32,
    pub height: u32,
    pub refresh_rate_millihertz: u32,
}

#[derive(Clone, Debug)]
pub struct MonitorSpec {
    pub name: String,
    pub modes: Vec<VideoModeSpec>,
}

#[derive(Clone, Debug)]
struct DisplaySnapshot {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    name: String,
    friendly_name: String,
}

#[cfg(not(any(
    target_os = "windows",
    target_os = "macos",
    all(unix, not(target_os = "macos"))
)))]
mod platform {
    use super::DisplaySnapshot;

    pub fn displays() -> Result<Vec<DisplaySnapshot>, String> {
        Ok(Vec::new())
    }
}

fn snapshot_displays() -> Vec<DisplaySnapshot> {
    match platform::displays() {
        Ok(list) => list,
        Err(err) => {
            warn!("Falling back to default monitor names: {err}");
            Vec::new()
        }
    }
}

#[inline(always)]
const fn names_match(lhs: &str, rhs: &str) -> bool {
    lhs.eq_ignore_ascii_case(rhs)
}

/// Returns a best-effort friendly name for each monitor handle.
/// OS APIs are queried for friendly names; if we cannot match a monitor,
/// we fall back to winit's name or "Screen N".
#[must_use]
pub fn friendly_monitor_names(monitors: &[MonitorHandle]) -> Vec<String> {
    let snapshots = snapshot_displays();
    let mut used = vec![false; snapshots.len()];

    let mut seen_duplicates: HashMap<String, usize> = HashMap::new();

    monitors
        .iter()
        .enumerate()
        .map(|(idx, monitor)| {
            let pos = monitor.position();
            let size = monitor.size();
            let mon_name = monitor.name();

            let mut matched_idx = snapshots
                .iter()
                .enumerate()
                .find(|(snap_idx, snap)| {
                    !used[*snap_idx]
                        && snap.x == pos.x
                        && snap.y == pos.y
                        && snap.width == size.width
                        && snap.height == size.height
                })
                .map(|(i, _)| i);

            if matched_idx.is_none()
                && let Some(name) = &mon_name
            {
                matched_idx = snapshots
                    .iter()
                    .enumerate()
                    .find(|(snap_idx, snap)| {
                        !used[*snap_idx]
                            && (names_match(&snap.name, name)
                                || names_match(&snap.friendly_name, name))
                    })
                    .map(|(i, _)| i);
            }

            if matched_idx.is_none() && idx < snapshots.len() && !used[idx] {
                matched_idx = Some(idx);
            }

            let base_name = if let Some(i) = matched_idx {
                used[i] = true;
                snapshots[i].friendly_name.clone()
            } else {
                mon_name.unwrap_or_else(|| format!("Screen {}", idx + 1))
            };

            let entry = seen_duplicates.entry(base_name.clone()).or_insert(0);
            *entry += 1;
            if *entry == 1 {
                base_name
            } else {
                format!("{} ({})", base_name, *entry)
            }
        })
        .collect()
}

/// Return monitor specs with friendly names and all advertised video modes.
#[must_use]
pub fn monitor_specs(monitors: &[MonitorHandle]) -> Vec<MonitorSpec> {
    let friendly_names = friendly_monitor_names(monitors);
    monitors
        .iter()
        .cloned()
        .zip(friendly_names)
        .map(|(monitor, name)| {
            let modes = monitor
                .video_modes()
                .map(|vm| VideoModeSpec {
                    width: vm.size().width,
                    height: vm.size().height,
                    refresh_rate_millihertz: vm.refresh_rate_millihertz(),
                })
                .collect();
            MonitorSpec { name, modes }
        })
        .collect()
}

#[inline(always)]
fn sorted_dedup<T: Ord>(mut values: Vec<T>) -> Vec<T> {
    values.sort_unstable();
    values.dedup();
    values
}

/// Deduplicated list of resolutions supported by the provided monitor spec.
pub fn supported_resolutions(spec: Option<&MonitorSpec>) -> Vec<(u32, u32)> {
    spec.map_or_else(Vec::new, |spec| {
        let modes: Vec<(u32, u32)> = spec.modes.iter().map(|m| (m.width, m.height)).collect();
        sorted_dedup(modes)
    })
}

/// Deduplicated list of refresh rates (millihertz) for a given resolution.
pub fn supported_refresh_rates(spec: Option<&MonitorSpec>, width: u32, height: u32) -> Vec<u32> {
    spec.map_or_else(Vec::new, |spec| {
        let rates: Vec<u32> = spec
            .modes
            .iter()
            .filter(|m| m.width == width && m.height == height)
            .map(|m| m.refresh_rate_millihertz)
            .collect();
        sorted_dedup(rates)
    })
}

/// Resolve a monitor handle from the requested index, returning (handle, count, `clamped_index`).
#[must_use]
pub fn resolve_monitor(
    event_loop: &ActiveEventLoop,
    monitor_index: usize,
) -> (Option<MonitorHandle>, usize, usize) {
    let monitors: Vec<MonitorHandle> = event_loop.available_monitors().collect();
    let count = monitors.len();
    if monitors.is_empty() {
        return (event_loop.primary_monitor(), 0, 0);
    }
    let clamped = monitor_index.min(count.saturating_sub(1));
    let handle = monitors
        .get(clamped)
        .cloned()
        .or_else(|| monitors.first().cloned())
        .or_else(|| event_loop.primary_monitor());
    (handle, count, clamped)
}

/// Center the window on the given monitor, clamped to the monitor's bounds.
#[must_use]
pub fn default_window_position(
    width: u32,
    height: u32,
    monitor: Option<MonitorHandle>,
) -> Option<PhysicalPosition<i32>> {
    let mon = monitor?;
    let mon_pos = mon.position();
    let mon_size = mon.size();
    let mon_w = mon_size.width as i32;
    let mon_h = mon_size.height as i32;
    let win_w = width as i32;
    let win_h = height as i32;
    if mon_w <= 0 || mon_h <= 0 || win_w <= 0 || win_h <= 0 {
        return None;
    }

    let center_x = mon_pos.x + (mon_w.saturating_sub(win_w)) / 2;
    let center_y = mon_pos.y + (mon_h.saturating_sub(win_h)) / 2;
    let min_x = mon_pos.x;
    let min_y = mon_pos.y;
    let max_x = mon_pos.x + mon_w.saturating_sub(win_w).max(0);
    let max_y = mon_pos.y + mon_h.saturating_sub(win_h).max(0);

    let x = center_x.clamp(min_x, max_x);
    let y = center_y.clamp(min_y, max_y);
    Some(PhysicalPosition::new(x, y))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayError {
    MonitorQuery,
    ModeQuery,
    ModeChange(i32),
}

impl std::fmt::Display for DisplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MonitorQuery => f.write_str("could not query the selected monitor"),
            Self::ModeQuery => f.write_str("could not query the current display mode"),
            Self::ModeChange(code) => write!(f, "Windows rejected the display mode (code {code})"),
        }
    }
}

impl std::error::Error for DisplayError {}

/// Window-thread ownership of a temporary Windows display mode. Restores the
/// captured desktop timing on exit; no registry settings are changed.
#[derive(Default)]
pub struct FullscreenState {
    #[cfg(target_os = "windows")]
    mode: Option<platform::ScreenMode>,
}

impl FullscreenState {
    pub fn current_mode(&self) -> Option<VideoModeSpec> {
        #[cfg(target_os = "windows")]
        return self.mode.as_ref().map(platform::ScreenMode::spec);
        #[cfg(not(target_os = "windows"))]
        None
    }

    pub fn restore(&mut self) -> Result<(), DisplayError> {
        #[cfg(target_os = "windows")]
        if let Some(mode) = &mut self.mode {
            mode.set_active(false)?;
            self.mode = None;
        }
        Ok(())
    }

    pub fn set_focused(&mut self, focused: bool) -> Result<(), DisplayError> {
        #[cfg(target_os = "windows")]
        if let Some(mode) = &mut self.mode {
            mode.set_active(focused)?;
        }
        #[cfg(not(target_os = "windows"))]
        let _ = focused;
        Ok(())
    }

    pub fn mode(
        &mut self,
        fullscreen_type: FullscreenType,
        width: u32,
        height: u32,
        refresh_rate_millihertz: u32,
        monitor: Option<MonitorHandle>,
        event_loop: &ActiveEventLoop,
    ) -> Result<Option<Fullscreen>, DisplayError> {
        self.restore()?;
        let monitor = monitor.or_else(|| event_loop.primary_monitor());
        #[cfg(target_os = "windows")]
        {
            if fullscreen_type == FullscreenType::Exclusive {
                let monitor = monitor.as_ref().ok_or(DisplayError::MonitorQuery)?;
                self.mode = Some(platform::ScreenMode::capture(monitor)?);
                if let Err(error) = self.mode.as_mut().expect("mode was captured").apply(
                    width,
                    height,
                    refresh_rate_millihertz,
                ) {
                    self.restore()?;
                    return Err(error);
                }
            }
            // Windows mode switching is owned above. Winit only manages the
            // fullscreen window, so it cannot replace an interlaced mode with
            // a deduplicated VideoModeHandle or assert on a rejected request.
            Ok(Some(Fullscreen::Borderless(monitor)))
        }
        #[cfg(not(target_os = "windows"))]
        Ok(fullscreen_mode(
            fullscreen_type,
            width,
            height,
            refresh_rate_millihertz,
            monitor,
        ))
    }
}

impl Drop for FullscreenState {
    fn drop(&mut self) {
        if let Err(error) = self.restore() {
            warn!("Failed to restore desktop display mode: {error}");
        }
    }
}

/// X11's default policy follows ITGmania: highest rate, or nearest explicit rate.
#[cfg(not(target_os = "windows"))]
fn fullscreen_mode(
    fullscreen_type: FullscreenType,
    width: u32,
    height: u32,
    refresh_rate_millihertz: u32,
    monitor: Option<MonitorHandle>,
) -> Option<Fullscreen> {
    let mon = monitor;
    match fullscreen_type {
        FullscreenType::Exclusive => {
            if let Some(mon) = mon {
                let best_mode = mon
                    .video_modes()
                    .filter(|m| {
                        let sz = m.size();
                        sz.width == width && sz.height == height
                    })
                    .min_by_key(|mode| {
                        let rate = mode.refresh_rate_millihertz();
                        if refresh_rate_millihertz == 0 {
                            u32::MAX - rate
                        } else {
                            rate.abs_diff(refresh_rate_millihertz)
                        }
                    });
                if let Some(mode) = best_mode {
                    info!(
                        "Fullscreen: using EXCLUSIVE {}x{} @ {} mHz",
                        width,
                        height,
                        mode.refresh_rate_millihertz()
                    );
                    Some(Fullscreen::Exclusive(mode))
                } else {
                    warn!("No exact EXCLUSIVE mode {width}x{height}; using BORDERLESS.");
                    Some(Fullscreen::Borderless(Some(mon)))
                }
            } else {
                warn!("No primary monitor reported; using BORDERLESS fullscreen.");
                Some(Fullscreen::Borderless(None))
            }
        }
        FullscreenType::Borderless => Some(Fullscreen::Borderless(mon)),
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{DisplayError, DisplaySnapshot};
    use std::mem;
    use windows::Win32::Devices::Display::{
        DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
        DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
        DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME,
        DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS,
        QueryDisplayConfig,
    };
    use windows::Win32::Foundation::{LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{
        CDS_FULLSCREEN, ChangeDisplaySettingsExW, DEVMODEW, DISP_CHANGE, DISP_CHANGE_SUCCESSFUL,
        DM_BITSPERPEL, DM_DISPLAYFREQUENCY, DM_PELSHEIGHT, DM_PELSWIDTH, ENUM_CURRENT_SETTINGS,
        EnumDisplayDevicesW, EnumDisplayMonitors, EnumDisplaySettingsW, GetMonitorInfoW, HDC,
        HMONITOR, MONITORINFO, MONITORINFOEXW,
    };
    use windows::core::{BOOL, PCWSTR};

    pub(super) struct ScreenMode {
        device: [u16; 32],
        desktop: DEVMODEW,
        fullscreen: DEVMODEW,
        active: bool,
    }

    impl ScreenMode {
        pub(super) fn spec(&self) -> super::VideoModeSpec {
            super::VideoModeSpec {
                width: self.fullscreen.dmPelsWidth,
                height: self.fullscreen.dmPelsHeight,
                refresh_rate_millihertz: self.fullscreen.dmDisplayFrequency.saturating_mul(1000),
            }
        }

        pub(super) fn capture(
            monitor: &winit::monitor::MonitorHandle,
        ) -> Result<Self, DisplayError> {
            use winit::platform::windows::MonitorHandleExtWindows;
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = mem::size_of::<MONITORINFOEXW>() as u32;
            // SAFETY: The monitor handle comes from winit; info is a properly
            // sized MONITORINFOEXW and remains writable throughout the call.
            if !unsafe {
                GetMonitorInfoW(
                    HMONITOR(monitor.hmonitor() as *mut _),
                    &mut info.monitorInfo,
                )
            }
            .as_bool()
            {
                return Err(DisplayError::MonitorQuery);
            }
            let desktop = current_mode(&info.szDevice)?;
            Ok(Self {
                device: info.szDevice,
                desktop,
                fullscreen: desktop,
                active: false,
            })
        }

        pub(super) fn apply(
            &mut self,
            width: u32,
            height: u32,
            rate: u32,
        ) -> Result<(), DisplayError> {
            let request = mode_request(width, height, rate);
            let selected = try_modes(request, self.desktop, |mode| {
                change_mode(&self.device, mode)
            })?;
            self.fullscreen = selected;
            self.active = true;
            // Capture the resolved native mode, including scan flags, for focus
            // restoration. Do not reconstruct it from winit's reduced mode list.
            self.fullscreen = current_mode(&self.device)?;
            let mode = &self.fullscreen;
            // SAFETY: For a display DEVMODEW the second union contains dmDisplayFlags.
            let flags = unsafe { mode.Anonymous2.dmDisplayFlags };
            log::info!(
                "Fullscreen: using EXCLUSIVE {}x{} @ {} Hz, display_flags=0x{:x} (requested {}x{} @ {} mHz)",
                mode.dmPelsWidth,
                mode.dmPelsHeight,
                mode.dmDisplayFrequency,
                flags,
                width,
                height,
                rate
            );
            Ok(())
        }

        pub(super) fn set_active(&mut self, active: bool) -> Result<(), DisplayError> {
            if self.active != active {
                let mode = if active {
                    &self.fullscreen
                } else {
                    &self.desktop
                };
                let result = change_mode(&self.device, mode);
                if result != DISP_CHANGE_SUCCESSFUL {
                    return Err(DisplayError::ModeChange(result.0));
                }
                self.active = active;
            }
            Ok(())
        }
    }

    fn current_mode(device: &[u16; 32]) -> Result<DEVMODEW, DisplayError> {
        let mut mode = DEVMODEW {
            dmSize: mem::size_of::<DEVMODEW>() as u16,
            ..Default::default()
        };
        // SAFETY: device is the terminated name returned by GetMonitorInfoW;
        // mode is correctly sized writable storage, with no driver-extra bytes.
        if unsafe {
            EnumDisplaySettingsW(PCWSTR(device.as_ptr()), ENUM_CURRENT_SETTINGS, &mut mode)
        }
        .as_bool()
        {
            Ok(mode)
        } else {
            Err(DisplayError::ModeQuery)
        }
    }

    fn change_mode(device: &[u16; 32], mode: &DEVMODEW) -> DISP_CHANGE {
        // SAFETY: device is terminated; mode is initialized and has the correct
        // size. The API only borrows both and does not update registry settings.
        unsafe {
            ChangeDisplaySettingsExW(
                PCWSTR(device.as_ptr()),
                Some(mode),
                None,
                CDS_FULLSCREEN,
                None,
            )
        }
    }

    fn mode_request(width: u32, height: u32, rate: u32) -> DEVMODEW {
        let mut mode = DEVMODEW {
            dmSize: mem::size_of::<DEVMODEW>() as u16,
            dmPelsWidth: width,
            dmPelsHeight: height,
            dmBitsPerPel: 32,
            dmFields: DM_PELSWIDTH | DM_PELSHEIGHT | DM_BITSPERPEL,
            ..Default::default()
        };
        if rate != 0 {
            mode.dmDisplayFrequency = ((u64::from(rate) + 500) / 1000) as u32;
            mode.dmFields |= DM_DISPLAYFREQUENCY;
        }
        mode
    }

    /// Match ITGmania's Windows request/retry order, then recover the exact
    /// captured desktop timing if the requested resolution is rejected too.
    fn try_modes(
        mut request: DEVMODEW,
        desktop: DEVMODEW,
        mut apply: impl FnMut(&DEVMODEW) -> DISP_CHANGE,
    ) -> Result<DEVMODEW, DisplayError> {
        let mut result = apply(&request);
        if result != DISP_CHANGE_SUCCESSFUL && request.dmFields.contains(DM_DISPLAYFREQUENCY) {
            log::warn!(
                "Fullscreen refresh {} Hz rejected ({}); retrying driver default",
                request.dmDisplayFrequency,
                result.0
            );
            request.dmFields &= !DM_DISPLAYFREQUENCY;
            request.dmDisplayFrequency = 0;
            result = apply(&request);
        }
        if result == DISP_CHANGE_SUCCESSFUL {
            return Ok(request);
        }
        log::warn!(
            "Fullscreen {}x{} rejected ({}); restoring current desktop mode",
            request.dmPelsWidth,
            request.dmPelsHeight,
            result.0
        );
        result = apply(&desktop);
        if result == DISP_CHANGE_SUCCESSFUL {
            Ok(desktop)
        } else {
            Err(DisplayError::ModeChange(result.0))
        }
    }

    #[cfg(test)]
    mod mode_tests {
        use super::*;
        use windows::Win32::Graphics::Gdi::{DISP_CHANGE_BADMODE, DM_DISPLAYFLAGS, DM_INTERLACED};

        #[test]
        fn default_leaves_refresh_and_scan_choice_to_driver() {
            let request = mode_request(640, 480, 0);
            let mut calls = 0;
            let result = try_modes(request, DEVMODEW::default(), |mode| {
                calls += 1;
                assert_eq!((mode.dmPelsWidth, mode.dmPelsHeight), (640, 480));
                assert!(!mode.dmFields.contains(DM_DISPLAYFREQUENCY));
                assert!(!mode.dmFields.contains(DM_DISPLAYFLAGS));
                DISP_CHANGE_SUCCESSFUL
            });
            assert!(result.is_ok());
            assert_eq!(calls, 1);
        }

        #[test]
        fn rejected_explicit_rate_retries_default() {
            let mut calls = 0;
            let result = try_modes(
                mode_request(640, 480, 30_000),
                DEVMODEW::default(),
                |mode| {
                    calls += 1;
                    assert_eq!((mode.dmPelsWidth, mode.dmPelsHeight), (640, 480));
                    if calls == 1 {
                        assert!(mode.dmFields.contains(DM_DISPLAYFREQUENCY));
                        assert_eq!(mode.dmDisplayFrequency, 30);
                        DISP_CHANGE_BADMODE
                    } else {
                        assert!(!mode.dmFields.contains(DM_DISPLAYFREQUENCY));
                        DISP_CHANGE_SUCCESSFUL
                    }
                },
            );
            assert!(result.is_ok());
            assert_eq!(calls, 2);
        }

        #[test]
        fn rejected_resolution_recovers_exact_interlaced_desktop() {
            let mut desktop = mode_request(640, 480, 30_000);
            desktop.dmFields |= DM_DISPLAYFLAGS;
            desktop.Anonymous2.dmDisplayFlags = DM_INTERLACED.0;
            let mut calls = 0;
            let result = try_modes(mode_request(1920, 1080, 60_000), desktop, |mode| {
                calls += 1;
                if calls < 3 {
                    DISP_CHANGE_BADMODE
                } else {
                    assert_eq!((mode.dmPelsWidth, mode.dmPelsHeight), (640, 480));
                    assert_eq!(mode.dmDisplayFrequency, 30);
                    assert!(mode.dmFields.contains(DM_DISPLAYFLAGS));
                    // SAFETY: This test initialized the display-flags union member above.
                    assert_eq!(unsafe { mode.Anonymous2.dmDisplayFlags }, DM_INTERLACED.0);
                    DISP_CHANGE_SUCCESSFUL
                }
            });
            assert!(result.is_ok());
            assert_eq!(calls, 3);
        }

        #[test]
        fn failed_default_and_desktop_return_error() {
            let mut calls = 0;
            let result = try_modes(mode_request(640, 480, 0), DEVMODEW::default(), |_| {
                calls += 1;
                DISP_CHANGE_BADMODE
            });
            assert!(matches!(result, Err(DisplayError::ModeChange(-2))));
            assert_eq!(calls, 2);
        }
    }

    // SAFETY: Windows calls this callback synchronously during `EnumDisplayMonitors`; `state` is
    // the exact `Vec<HMONITOR>` pointer passed from `displays()` and remains valid for that call.
    unsafe extern "system" fn monitor_enum_proc(
        h_monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        state: LPARAM,
    ) -> BOOL {
        // SAFETY: `state` is created from `&mut handles as isize` in `displays()`,
        // and `EnumDisplayMonitors` does not outlive that call. The callback is
        // therefore allowed to cast it back to the original `Vec<HMONITOR>`.
        let monitors = unsafe { &mut *(state.0 as *mut Vec<HMONITOR>) };
        monitors.push(h_monitor);
        BOOL(1)
    }

    fn utf16_to_string(buf: &[u16]) -> String {
        let len = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len]).trim().to_string()
    }

    fn friendly_name_from_config(monitor_info_ex: &MONITORINFOEXW) -> Option<String> {
        let mut path_count = 0;
        let mut mode_count = 0;
        // SAFETY: the Win32 API writes the path/mode counts into valid stack
        // locals passed by mutable pointer and does not retain those pointers.
        unsafe {
            if GetDisplayConfigBufferSizes(
                QDC_ONLY_ACTIVE_PATHS,
                &raw mut path_count,
                &raw mut mode_count,
            )
            .is_err()
            {
                return None;
            }
        }

        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];

        // SAFETY: `paths` and `modes` are preallocated to the sizes requested by
        // `GetDisplayConfigBufferSizes`, and the API only writes into the provided
        // buffers for the duration of this call.
        unsafe {
            if QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &raw mut path_count,
                paths.as_mut_ptr(),
                &raw mut mode_count,
                modes.as_mut_ptr(),
                None,
            )
            .is_err()
            {
                return None;
            }
        }

        for path in paths {
            let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
                header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                    size: mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                    adapterId: path.sourceInfo.adapterId,
                    id: path.sourceInfo.id,
                },
                ..Default::default()
            };

            // SAFETY: `source.header` is fully initialized with the struct size and
            // identifiers required by `DisplayConfigGetDeviceInfo`, and the API
            // writes only into this stack-allocated structure.
            unsafe {
                if DisplayConfigGetDeviceInfo(&raw mut source.header) != 0 {
                    continue;
                }
            }

            if source.viewGdiDeviceName != monitor_info_ex.szDevice {
                continue;
            }

            let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
                header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                    size: mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
                    adapterId: path.sourceInfo.adapterId,
                    id: path.targetInfo.id,
                },
                ..Default::default()
            };

            // SAFETY: `target.header` is fully initialized with the struct size and
            // identifiers required by `DisplayConfigGetDeviceInfo`, and the API
            // writes only into this stack-allocated structure.
            unsafe {
                if DisplayConfigGetDeviceInfo(&raw mut target.header) != 0 {
                    continue;
                }
            }

            let name = utf16_to_string(&target.monitorFriendlyDeviceName);
            if !name.is_empty() {
                return Some(name);
            }
        }

        None
    }

    fn device_string(monitor_info_ex: &MONITORINFOEXW) -> Option<String> {
        // SAFETY: `display_device` is initialized with its required `cb` size and
        // passed by mutable reference for the duration of the Win32 call only.
        unsafe {
            let mut display_device = windows::Win32::Graphics::Gdi::DISPLAY_DEVICEW {
                cb: mem::size_of::<windows::Win32::Graphics::Gdi::DISPLAY_DEVICEW>() as u32,
                ..Default::default()
            };
            if !EnumDisplayDevicesW(
                PCWSTR(monitor_info_ex.szDevice.as_ptr()),
                0,
                &raw mut display_device,
                0,
            )
            .as_bool()
            {
                return None;
            }
            let name = utf16_to_string(&display_device.DeviceString);
            if name.is_empty() { None } else { Some(name) }
        }
    }

    fn monitor_snapshot(h_monitor: HMONITOR) -> Result<DisplaySnapshot, String> {
        let mut info_ex = MONITORINFOEXW::default();
        info_ex.monitorInfo.cbSize = mem::size_of::<MONITORINFOEXW>() as u32;
        let ptr = &raw mut info_ex as *mut MONITORINFO;
        // SAFETY: `MONITORINFOEXW` begins with `MONITORINFO`, `cbSize` is set to
        // the concrete struct size, and `ptr` stays valid for the duration of the
        // API call.
        unsafe {
            GetMonitorInfoW(h_monitor, ptr)
                .ok()
                .map_err(|e| format!("GetMonitorInfoW failed: {e:?}"))?;
        }

        let mut dev_mode = windows::Win32::Graphics::Gdi::DEVMODEW {
            dmSize: mem::size_of::<windows::Win32::Graphics::Gdi::DEVMODEW>() as u16,
            ..Default::default()
        };
        // SAFETY: `dev_mode` has its required size field set and is passed by
        // mutable reference for the duration of the Win32 call only.
        unsafe {
            EnumDisplaySettingsW(
                PCWSTR(info_ex.szDevice.as_ptr()),
                windows::Win32::Graphics::Gdi::ENUM_CURRENT_SETTINGS,
                &raw mut dev_mode,
            )
            .ok()
            .map_err(|e| format!("EnumDisplaySettingsW failed: {e:?}"))?;
        }

        // SAFETY: `EnumDisplaySettingsW` initialized `dev_mode`, so reading the
        // active union arm containing `dmPosition` is valid here.
        let pos = unsafe { dev_mode.Anonymous1.Anonymous2.dmPosition };
        let width = dev_mode.dmPelsWidth;
        let height = dev_mode.dmPelsHeight;

        let name = utf16_to_string(&info_ex.szDevice);
        let friendly_name = friendly_name_from_config(&info_ex)
            .or_else(|| device_string(&info_ex))
            .unwrap_or_else(|| format!("Unknown Display {h_monitor:?}"));

        Ok(DisplaySnapshot {
            x: pos.x,
            y: pos.y,
            width,
            height,
            name,
            friendly_name,
        })
    }

    pub fn displays() -> Result<Vec<DisplaySnapshot>, String> {
        let mut handles: Vec<HMONITOR> = Vec::new();
        // SAFETY: the callback receives a pointer back to `handles`, which lives
        // until `EnumDisplayMonitors` returns. The API does not retain that state.
        unsafe {
            EnumDisplayMonitors(
                None,
                None,
                Some(monitor_enum_proc),
                LPARAM(&raw mut handles as isize),
            )
            .ok()
            .map_err(|e| format!("EnumDisplayMonitors failed: {e:?}"))?;
        }

        let mut out = Vec::with_capacity(handles.len());
        for h in handles {
            if let Ok(snapshot) = monitor_snapshot(h) {
                out.push(snapshot);
            }
        }

        Ok(out)
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::DisplaySnapshot;
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSScreen;
    use objc2_core_foundation::CGPoint;
    use objc2_core_graphics::{
        CGDirectDisplayID, CGDisplayBounds, CGError, CGGetActiveDisplayList, CGGetDisplaysWithPoint,
    };
    use objc2_foundation::{NSNumber, NSString};

    fn friendly_name(display_id: CGDirectDisplayID) -> Option<String> {
        let mtm =
            MainThreadMarker::new().expect("AppKit display enumeration requires the main thread");
        let screens = NSScreen::screens(mtm);
        for screen in screens {
            let device_description = screen.deviceDescription();
            let screen_number =
                device_description.objectForKey(&NSString::from_str("NSScreenNumber"))?;
            let screen_id = screen_number
                .downcast::<NSNumber>()
                .ok()?
                .unsignedIntValue();
            if screen_id == display_id {
                return Some(screen.localizedName().to_string());
            }
        }
        None
    }

    fn snapshot(display_id: CGDirectDisplayID) -> Result<DisplaySnapshot, String> {
        let bounds = CGDisplayBounds(display_id);
        Ok(DisplaySnapshot {
            x: bounds.origin.x as i32,
            y: bounds.origin.y as i32,
            width: bounds.size.width as u32,
            height: bounds.size.height as u32,
            name: format!("Display {display_id}"),
            friendly_name: friendly_name(display_id)
                .unwrap_or_else(|| format!("Unknown Display {display_id}")),
        })
    }

    pub fn displays() -> Result<Vec<DisplaySnapshot>, String> {
        let max_displays: u32 = 16;
        let mut ids: Vec<CGDirectDisplayID> = vec![0; max_displays as usize];
        let mut count: u32 = 0;

        // SAFETY: `ids` is preallocated for `max_displays` entries and `count`
        // points to a valid stack local that CoreGraphics fills before returning.
        let err = unsafe { CGGetActiveDisplayList(max_displays, ids.as_mut_ptr(), &mut count) };
        if err != CGError::Success {
            return Err(format!("CGGetActiveDisplayList failed: {:?}", err));
        }

        ids.truncate(count as usize);
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Ok(s) = snapshot(id) {
                out.push(s);
            }
        }
        Ok(out)
    }

    #[allow(dead_code)]
    pub fn display_from_point(x: i32, y: i32) -> Result<DisplaySnapshot, String> {
        let point = CGPoint {
            x: x as f64,
            y: y as f64,
        };
        let max_displays: u32 = 16;
        let mut ids: Vec<CGDirectDisplayID> = vec![0; max_displays as usize];
        let mut count: u32 = 0;
        // SAFETY: `ids` is preallocated for `max_displays` entries and `count`
        // points to a valid stack local that CoreGraphics fills before returning.
        let err =
            unsafe { CGGetDisplaysWithPoint(point, max_displays, ids.as_mut_ptr(), &mut count) };
        if err != CGError::Success {
            return Err(format!("CGGetDisplaysWithPoint failed: {:?}", err));
        }
        ids.first()
            .and_then(|id| snapshot(*id).ok())
            .ok_or_else(|| "Display not found".to_string())
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod platform {
    use super::DisplaySnapshot;
    use std::env::var_os;

    fn is_wayland() -> bool {
        var_os("WAYLAND_DISPLAY")
            .or(var_os("XDG_SESSION_TYPE"))
            .is_some_and(|v| {
                v.to_str()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains("wayland")
            })
    }

    pub fn displays() -> Result<Vec<DisplaySnapshot>, String> {
        if is_wayland() {
            wayland::displays()
        } else {
            xorg::displays()
        }
    }

    mod xorg {
        use super::DisplaySnapshot;
        use std::str;
        use xcb::x::{Atom, GetAtomName};
        use xcb::{
            Connection,
            randr::{GetCrtcInfo, GetMonitors, GetOutputInfo, Output},
            x::{ATOM_RESOURCE_MANAGER, ATOM_STRING, GetProperty, Screen},
        };

        fn get_name(conn: &Connection, atom: Atom) -> Result<String, String> {
            let cookie = conn.send_request(&GetAtomName { atom });
            let reply = conn
                .wait_for_reply(cookie)
                .map_err(|e| format!("{:?}", e))?;
            Ok(reply.name().to_string())
        }

        fn get_scale_factor(conn: &Connection, screen: &Screen) -> Result<f32, String> {
            let prefix = "Xft.dpi:\t";
            let cookie = conn.send_request(&GetProperty {
                delete: false,
                window: screen.root(),
                property: ATOM_RESOURCE_MANAGER,
                r#type: ATOM_STRING,
                long_offset: 0,
                long_length: 60,
            });
            let reply = conn
                .wait_for_reply(cookie)
                .map_err(|e| format!("{:?}", e))?;
            let resource_manager = str::from_utf8(reply.value()).map_err(|e| e.to_string())?;
            let dpi_str = resource_manager
                .split('\n')
                .find(|s| s.starts_with(prefix))
                .and_then(|s| s.strip_prefix(prefix))
                .ok_or_else(|| "Xft.dpi parse failed".to_string())?;
            let dpi = dpi_str.parse::<f32>().map_err(|e| e.to_string())?;
            Ok(dpi / 96.0)
        }

        fn get_rotation(
            conn: &Connection,
            output: &Output,
        ) -> Result<(i32, i32, u32, u32), String> {
            let output_info = conn
                .wait_for_reply(conn.send_request(&GetOutputInfo {
                    output: *output,
                    config_timestamp: 0,
                }))
                .map_err(|e| format!("{:?}", e))?;

            let crtc_info = conn
                .wait_for_reply(conn.send_request(&GetCrtcInfo {
                    crtc: output_info.crtc(),
                    config_timestamp: 0,
                }))
                .map_err(|e| format!("{:?}", e))?;

            Ok((
                crtc_info.x().into(),
                crtc_info.y().into(),
                crtc_info.width().into(),
                crtc_info.height().into(),
            ))
        }

        pub fn displays() -> Result<Vec<DisplaySnapshot>, String> {
            let (conn, index) = Connection::connect(None).map_err(|e| format!("{:?}", e))?;
            let setup = conn.get_setup();
            let screen = setup
                .roots()
                .nth(index as usize)
                .ok_or_else(|| "No screen".to_string())?;

            let scale_factor = get_scale_factor(&conn, screen).unwrap_or(1.0);

            let monitors_reply = conn
                .wait_for_reply(conn.send_request(&GetMonitors {
                    window: screen.root(),
                    get_active: true,
                }))
                .map_err(|e| format!("{:?}", e))?;

            let mut out = Vec::new();
            for monitor in monitors_reply.monitors() {
                let output = monitor
                    .outputs()
                    .first()
                    .cloned()
                    .ok_or_else(|| "No output".to_string())?;
                let name = get_name(&conn, monitor.name())?;
                let (x, y, w, h) = get_rotation(&conn, &output)?;
                out.push(DisplaySnapshot {
                    x: ((x as f32) / scale_factor) as i32,
                    y: ((y as f32) / scale_factor) as i32,
                    width: ((w as f32) / scale_factor) as u32,
                    height: ((h as f32) / scale_factor) as u32,
                    name: name.clone(),
                    friendly_name: name,
                });
            }

            Ok(out)
        }
    }

    mod wayland {
        use super::DisplaySnapshot;
        use smithay_client_toolkit::output::{OutputHandler, OutputInfo, OutputState};
        use smithay_client_toolkit::reexports::client::globals::registry_queue_init;
        use smithay_client_toolkit::reexports::client::protocol::wl_output;
        use smithay_client_toolkit::reexports::client::{Connection, QueueHandle};
        use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
        use smithay_client_toolkit::{delegate_dispatch2, delegate_registry, registry_handlers};

        fn snapshot_from_info(info: OutputInfo) -> DisplaySnapshot {
            let OutputInfo {
                scale_factor,
                logical_position,
                location,
                logical_size,
                physical_size,
                name,
                id,
                ..
            } = info;
            let scale = scale_factor as f32;
            let (x, y) = logical_position.unwrap_or(location);
            let (w, h) = logical_size.unwrap_or(physical_size);
            let name = name.unwrap_or_else(|| format!("Unknown Display {}", id));

            DisplaySnapshot {
                x: ((x as f32) / scale) as i32,
                y: ((y as f32) / scale) as i32,
                width: ((w as f32) / scale) as u32,
                height: ((h as f32) / scale) as u32,
                name: name.clone(),
                friendly_name: name,
            }
        }

        struct ListOutputs {
            registry_state: RegistryState,
            output_state: OutputState,
        }

        impl OutputHandler for ListOutputs {
            fn output_state(&mut self) -> &mut OutputState {
                &mut self.output_state
            }

            fn new_output(
                &mut self,
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
                _output: wl_output::WlOutput,
            ) {
            }

            fn update_output(
                &mut self,
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
                _output: wl_output::WlOutput,
            ) {
            }

            fn output_destroyed(
                &mut self,
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
                _output: wl_output::WlOutput,
            ) {
            }
        }

        delegate_dispatch2!(ListOutputs);
        delegate_registry!(ListOutputs);

        impl ProvidesRegistryState for ListOutputs {
            fn registry(&mut self) -> &mut RegistryState {
                &mut self.registry_state
            }

            registry_handlers! {
                OutputState,
            }
        }

        pub fn displays() -> Result<Vec<DisplaySnapshot>, String> {
            let conn = Connection::connect_to_env().map_err(|e| format!("{:?}", e))?;
            let (globals, mut event_queue) =
                registry_queue_init(&conn).map_err(|e| format!("{:?}", e))?;
            let qh = event_queue.handle();

            let registry_state = RegistryState::new(&globals);
            let output_delegate = OutputState::new(&globals, &qh);

            let mut list_outputs = ListOutputs {
                registry_state,
                output_state: output_delegate,
            };

            event_queue
                .roundtrip(&mut list_outputs)
                .map_err(|e| format!("{:?}", e))?;

            list_outputs
                .output_state
                .outputs()
                .map(|output| {
                    list_outputs
                        .output_state
                        .info(&output)
                        .map(snapshot_from_info)
                        .ok_or_else(|| "Cannot read output info".to_string())
                })
                .collect()
        }
    }
}
