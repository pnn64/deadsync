use crate::config::FullscreenType;
use log::{info, warn};
use std::collections::HashMap;
use winit::{
    dpi::PhysicalPosition, event_loop::ActiveEventLoop, monitor::MonitorHandle, window::Fullscreen,
};

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

/// Pick a fullscreen mode (exclusive if available) for the given monitor and resolution.
pub fn fullscreen_mode(
    fullscreen_type: FullscreenType,
    width: u32,
    height: u32,
    monitor: Option<MonitorHandle>,
    event_loop: &ActiveEventLoop,
) -> Option<Fullscreen> {
    let primary = event_loop.primary_monitor();
    let mon = monitor.or(primary);
    match fullscreen_type {
        FullscreenType::Exclusive => {
            if let Some(mon) = mon {
                let best_mode = mon
                    .video_modes()
                    .filter(|m| {
                        let sz = m.size();
                        sz.width == width && sz.height == height
                    })
                    .max_by_key(winit::monitor::VideoModeHandle::refresh_rate_millihertz);
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
    use super::DisplaySnapshot;
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
        EnumDisplayDevicesW, EnumDisplayMonitors, EnumDisplaySettingsW, GetMonitorInfoW, HDC,
        HMONITOR, MONITORINFO, MONITORINFOEXW,
    };
    use windows::core::{BOOL, PCWSTR};

    unsafe extern "system" fn monitor_enum_proc(
        h_monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        state: LPARAM,
    ) -> BOOL {
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
        unsafe {
            GetMonitorInfoW(h_monitor, ptr)
                .ok()
                .map_err(|e| format!("GetMonitorInfoW failed: {e:?}"))?;
        }

        let mut dev_mode = windows::Win32::Graphics::Gdi::DEVMODEW {
            dmSize: mem::size_of::<windows::Win32::Graphics::Gdi::DEVMODEW>() as u16,
            ..Default::default()
        };
        unsafe {
            EnumDisplaySettingsW(
                PCWSTR(info_ex.szDevice.as_ptr()),
                windows::Win32::Graphics::Gdi::ENUM_CURRENT_SETTINGS,
                &raw mut dev_mode,
            )
            .ok()
            .map_err(|e| format!("EnumDisplaySettingsW failed: {e:?}"))?;
        }

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
        CGDirectDisplayID, CGDisplayBounds, CGDisplayCopyDisplayMode, CGDisplayMode, CGError,
        CGGetActiveDisplayList, CGGetDisplaysWithPoint,
    };
    use objc2_foundation::{NSNumber, NSString};

    fn friendly_name(display_id: CGDirectDisplayID) -> Option<String> {
        let screens = NSScreen::screens(unsafe { MainThreadMarker::new_unchecked() });
        for screen in screens {
            let device_description = screen.deviceDescription();
            let screen_number =
                device_description.objectForKey(&NSString::from_str("NSScreenNumber"))?;
            let screen_id = screen_number
                .downcast::<NSNumber>()
                .ok()?
                .unsignedIntValue();
            if screen_id == display_id {
                unsafe { return Some(screen.localizedName().to_string()) };
            }
        }
        None
    }

    fn snapshot(display_id: CGDirectDisplayID) -> Result<DisplaySnapshot, String> {
        unsafe {
            let bounds = CGDisplayBounds(display_id);
            let mode = CGDisplayCopyDisplayMode(display_id);
            let pixel_width = CGDisplayMode::pixel_width(mode.as_deref());
            let scale = if bounds.size.width > 0.0 {
                pixel_width as f32 / bounds.size.width as f32
            } else {
                1.0
            };

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
    }

    pub fn displays() -> Result<Vec<DisplaySnapshot>, String> {
        let max_displays: u32 = 16;
        let mut ids: Vec<CGDirectDisplayID> = vec![0; max_displays as usize];
        let mut count: u32 = 0;

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
        use smithay_client_toolkit::{delegate_output, delegate_registry, registry_handlers};

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

        delegate_output!(ListOutputs);
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
