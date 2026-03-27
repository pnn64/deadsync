use super::{
    GpSystemEvent, PadBackend, PadCode, PadEvent, PadId, RawKeyboardEvent, emit_dir_edges,
    uuid_from_bytes,
};
use crate::engine::windows_rt::{ThreadRole, boost_current_thread, current_host_nanos};
use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::time::Instant;

use windows::Win32::Devices::HumanInterfaceDevice::{
    HIDP_CAPS, HIDP_STATUS_SUCCESS, HIDP_VALUE_CAPS, HidP_GetCaps, HidP_GetSpecificValueCaps,
    HidP_GetUsageValue, HidP_GetUsages, HidP_GetValueCaps, HidP_Input, HidP_MaxUsageListLength,
    PHIDP_PREPARSED_DATA,
};
use windows::Win32::Foundation::{HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MAPVK_VK_TO_VSC_EX, MapVirtualKeyW, VK_NUMLOCK, VK_SHIFT,
};
use windows::Win32::UI::Input::{
    GetRawInputData, GetRawInputDeviceInfoW, GetRawInputDeviceList, HRAWINPUT, RAWINPUTDEVICE,
    RAWINPUTDEVICELIST, RAWINPUTHEADER, RAWKEYBOARD, RID_DEVICE_INFO, RID_DEVICE_INFO_HID,
    RID_INPUT, RIDEV_DEVNOTIFY, RIDEV_INPUTSINK, RIDEV_NOLEGACY, RIDI_DEVICEINFO, RIDI_DEVICENAME,
    RIDI_PREPARSEDDATA, RIM_TYPEHID, RIM_TYPEKEYBOARD, RegisterRawInputDevices,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetMessageW,
    GetWindowLongPtrW, HWND_MESSAGE, MSG, PostQuitMessage, RI_KEY_E0, RI_KEY_E1, RegisterClassExW,
    SetWindowLongPtrW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CREATE, WM_DESTROY,
    WM_INPUT, WM_INPUT_DEVICE_CHANGE, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    WNDCLASSEXW,
};
use windows::core::PCWSTR;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::platform::scancode::PhysicalKeyExtScancode;

const USAGE_PAGE_GENERIC: u16 = 0x01;
const USAGE_JOYSTICK: u16 = 0x04;
const USAGE_GAMEPAD: u16 = 0x05;
const USAGE_GENERIC_KEYBOARD: u16 = 0x06;
const USAGE_MULTI_AXIS: u16 = 0x08;

const USAGE_PAGE_BUTTON: u16 = 0x09;
const USAGE_HAT_SWITCH: u16 = 0x39;

const RIM_TYPEHID_U32: u32 = RIM_TYPEHID.0;
const RIM_TYPEKEYBOARD_U32: u32 = RIM_TYPEKEYBOARD.0;
const GIDC_ARRIVAL_U32: usize = 1;
const GIDC_REMOVAL_U32: usize = 2;
const RAW_KEY_HELD_SLOTS: usize = 3 * 256;
const WIN_SC_SHIFT: u16 = 0x0010;
const WIN_SC_NUMLOCK: u16 = 0x0045;
const WIN_SC_IGNORE_PAUSE_PREFIX: u16 = 0xe11d;
const WIN_SC_IGNORE_PRTSC_PREFIX: u16 = 0xe02a;

static WINDOW_FOCUSED: AtomicBool = AtomicBool::new(true);
static CAPTURE_ENABLED: AtomicBool = AtomicBool::new(false);
static RAW_INPUT_HWND: AtomicIsize = AtomicIsize::new(0);

struct Dev {
    id: PadId,
    name: String,
    vendor_id: Option<u16>,
    product_id: Option<u16>,
    uuid: [u8; 16],
    preparsed: Vec<u8>,
    max_buttons: u32,
    buttons_prev: Vec<u16>,
    buttons_now: Vec<u16>,
    hat_min: i32,
    hat_max: i32,
    dir: [bool; 4],
}

struct Ctx {
    emit_pad: Box<dyn FnMut(PadEvent) + Send>,
    emit_sys: Box<dyn FnMut(GpSystemEvent) + Send>,
    emit_key: Box<dyn FnMut(RawKeyboardEvent) + Send>,
    devices: HashMap<isize, Dev>,
    id_by_uuid: HashMap<[u8; 16], PadId>,
    refs_by_uuid: HashMap<[u8; 16], u32>,
    next_id: u32,
    buf: Vec<u8>,
    held: [bool; RAW_KEY_HELD_SLOTS],
    enable_pad: bool,
}

impl Ctx {
    #[inline(always)]
    fn emit_connected(&mut self, dev: &Dev, initial: bool) {
        (self.emit_sys)(GpSystemEvent::Connected {
            name: dev.name.clone(),
            id: dev.id,
            vendor_id: dev.vendor_id,
            product_id: dev.product_id,
            backend: PadBackend::WindowsRawInput,
            initial,
        });
    }

    #[inline(always)]
    fn emit_disconnected(&mut self, dev: &Dev, initial: bool) {
        (self.emit_sys)(GpSystemEvent::Disconnected {
            name: dev.name.clone(),
            id: dev.id,
            backend: PadBackend::WindowsRawInput,
            initial,
        });
    }
}

#[cold]
fn warn_win32(label: &str, err: &windows::core::Error) {
    log::warn!("Windows Raw Input {label} failed ({err})");
}

#[cold]
fn emit_startup_complete(ctx: &mut Ctx) {
    if ctx.enable_pad {
        (ctx.emit_sys)(GpSystemEvent::StartupComplete);
    }
}

#[cold]
fn disable_backend(ctx: &mut Ctx, label: &str, err: windows::core::Error) {
    RAW_INPUT_HWND.store(0, Ordering::Release);
    log::warn!("Windows Raw Input {label} failed ({err}); backend disabled");
    emit_startup_complete(ctx);
}

#[inline(always)]
pub fn set_window_focused(focused: bool) {
    WINDOW_FOCUSED.store(focused, Ordering::Relaxed);
}

#[inline(always)]
pub fn set_capture_enabled(enabled: bool) {
    CAPTURE_ENABLED.store(enabled, Ordering::Relaxed);
    let hwnd = HWND(RAW_INPUT_HWND.load(Ordering::Acquire) as *mut c_void);
    if !hwnd.0.is_null() {
        register_keyboard(hwnd, enabled);
    }
}

#[inline(always)]
fn register_keyboard(hwnd: HWND, capture_enabled: bool) -> bool {
    let mut flags = RIDEV_INPUTSINK;
    if capture_enabled {
        flags |= RIDEV_NOLEGACY;
    }
    let devices = [RAWINPUTDEVICE {
        usUsagePage: USAGE_PAGE_GENERIC,
        usUsage: USAGE_GENERIC_KEYBOARD,
        dwFlags: flags,
        hwndTarget: hwnd,
    }];
    // SAFETY: `devices` points to a fixed stack array that lives for the duration
    // of the call, and `hwnd` is the message-only window created by this backend.
    unsafe {
        match RegisterRawInputDevices(&devices, size_of::<RAWINPUTDEVICE>() as u32) {
            Ok(()) => true,
            Err(err) => {
                warn_win32("keyboard registration", &err);
                false
            }
        }
    }
}

#[inline(always)]
fn register_controllers(hwnd: HWND) -> bool {
    let devices = [
        RAWINPUTDEVICE {
            usUsagePage: USAGE_PAGE_GENERIC,
            usUsage: USAGE_JOYSTICK,
            dwFlags: RIDEV_DEVNOTIFY | RIDEV_INPUTSINK,
            hwndTarget: hwnd,
        },
        RAWINPUTDEVICE {
            usUsagePage: USAGE_PAGE_GENERIC,
            usUsage: USAGE_GAMEPAD,
            dwFlags: RIDEV_DEVNOTIFY | RIDEV_INPUTSINK,
            hwndTarget: hwnd,
        },
        RAWINPUTDEVICE {
            usUsagePage: USAGE_PAGE_GENERIC,
            usUsage: USAGE_MULTI_AXIS,
            dwFlags: RIDEV_DEVNOTIFY | RIDEV_INPUTSINK,
            hwndTarget: hwnd,
        },
    ];
    // SAFETY: `devices` points to a fixed stack array that lives for the duration
    // of the call, and `hwnd` is the message-only window created by this backend.
    unsafe {
        match RegisterRawInputDevices(&devices, size_of::<RAWINPUTDEVICE>() as u32) {
            Ok(()) => true,
            Err(err) => {
                warn_win32("controller registration", &err);
                false
            }
        }
    }
}

#[inline(always)]
const fn scancode_slot(scancode: u16) -> usize {
    let page = match scancode & 0xff00 {
        0xe000 => 1,
        0xe100 => 2,
        _ => 0,
    };
    page * 256 + (scancode as usize & 0x00ff)
}

#[inline(always)]
fn keyboard_scancode(keyboard: RAWKEYBOARD) -> u16 {
    let flags = u32::from(keyboard.Flags);
    let extension = if flags & RI_KEY_E0 != 0 {
        0xe000
    } else if flags & RI_KEY_E1 != 0 {
        0xe100
    } else {
        0x0000
    };
    if keyboard.MakeCode == 0 {
        // SAFETY: `MapVirtualKeyW` takes only scalar arguments and returns the
        // current OS mapping for the provided virtual key code.
        unsafe { MapVirtualKeyW(u32::from(keyboard.VKey), MAPVK_VK_TO_VSC_EX) as u16 }
    } else {
        keyboard.MakeCode | extension
    }
}

#[inline(always)]
fn raw_keyboard_code(keyboard: RAWKEYBOARD) -> Option<(KeyCode, usize)> {
    let scancode = keyboard_scancode(keyboard);
    if matches!(
        scancode,
        WIN_SC_IGNORE_PAUSE_PREFIX | WIN_SC_IGNORE_PRTSC_PREFIX
    ) {
        return None;
    }

    let physical = if keyboard.VKey == VK_NUMLOCK.0 {
        PhysicalKey::Code(KeyCode::NumLock)
    } else {
        PhysicalKey::from_scancode(u32::from(scancode))
    };

    if keyboard.VKey == VK_SHIFT.0
        && matches!(
            physical,
            PhysicalKey::Code(
                KeyCode::NumpadDecimal
                    | KeyCode::Numpad0
                    | KeyCode::Numpad1
                    | KeyCode::Numpad2
                    | KeyCode::Numpad3
                    | KeyCode::Numpad4
                    | KeyCode::Numpad5
                    | KeyCode::Numpad6
                    | KeyCode::Numpad7
                    | KeyCode::Numpad8
                    | KeyCode::Numpad9
            )
        )
    {
        return None;
    }

    let PhysicalKey::Code(code) = physical else {
        return None;
    };
    let slot = scancode_slot(if keyboard.VKey == VK_NUMLOCK.0 {
        WIN_SC_NUMLOCK
    } else if keyboard.VKey == VK_SHIFT.0 && scancode == 0 {
        WIN_SC_SHIFT
    } else {
        scancode
    });
    Some((code, slot))
}

#[inline(always)]
const fn is_controller_usage(usage_page: u16, usage: u16) -> bool {
    usage_page == USAGE_PAGE_GENERIC
        && matches!(usage, USAGE_JOYSTICK | USAGE_GAMEPAD | USAGE_MULTI_AXIS)
}

#[inline(always)]
fn hkey(h: HANDLE) -> isize {
    h.0 as isize
}

fn wide_to_string(mut v: Vec<u16>) -> String {
    while matches!(v.last(), Some(0)) {
        v.pop();
    }
    String::from_utf16_lossy(&v)
}

fn get_device_name(h: HANDLE) -> Option<String> {
    // SAFETY: all pointers passed here reference local buffers that stay alive for
    // each Win32 call, and `h` is a device handle reported by Raw Input.
    unsafe {
        let mut size: u32 = 0;
        let _ = GetRawInputDeviceInfoW(Some(h), RIDI_DEVICENAME, None, &raw mut size);
        if size == 0 {
            return None;
        }
        let mut buf: Vec<u16> = vec![0u16; size as usize];
        let mut size2 = size;
        let rc = GetRawInputDeviceInfoW(
            Some(h),
            RIDI_DEVICENAME,
            Some(buf.as_mut_ptr().cast::<c_void>()),
            &raw mut size2,
        );
        if rc == u32::MAX {
            return None;
        }
        Some(wide_to_string(buf))
    }
}

fn get_device_info(h: HANDLE) -> Option<RID_DEVICE_INFO_HID> {
    // SAFETY: `info` and `size` are writable stack locals, and `h` is a device
    // handle reported by Raw Input.
    unsafe {
        let mut info = RID_DEVICE_INFO::default();
        info.cbSize = size_of::<RID_DEVICE_INFO>() as u32;
        let mut size = info.cbSize;
        let rc = GetRawInputDeviceInfoW(
            Some(h),
            RIDI_DEVICEINFO,
            Some(ptr::addr_of_mut!(info).cast::<c_void>()),
            &raw mut size,
        );
        if rc == u32::MAX {
            return None;
        }
        if info.dwType != RIM_TYPEHID {
            return None;
        }
        Some(info.Anonymous.hid)
    }
}

fn get_preparsed(h: HANDLE) -> Option<Vec<u8>> {
    // SAFETY: the queried size comes from the first Raw Input call, and the second
    // call writes into the owned `buf` allocation that stays alive for the duration
    // of the call.
    unsafe {
        let mut size: u32 = 0;
        let _ = GetRawInputDeviceInfoW(Some(h), RIDI_PREPARSEDDATA, None, &raw mut size);
        if size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        let mut size2 = size;
        let rc = GetRawInputDeviceInfoW(
            Some(h),
            RIDI_PREPARSEDDATA,
            Some(buf.as_mut_ptr().cast::<c_void>()),
            &raw mut size2,
        );
        if rc == u32::MAX {
            return None;
        }
        Some(buf)
    }
}

fn hat_logical_range(preparsed: &[u8]) -> Option<(i32, i32)> {
    if preparsed.is_empty() {
        return None;
    }
    // SAFETY: `preparsed` holds the exact bytes returned by Raw Input for HID
    // preparsed data, and all HID parser calls treat that buffer as read-only for
    // the duration of this function.
    unsafe {
        let pd = PHIDP_PREPARSED_DATA(preparsed.as_ptr() as isize);

        let mut cap = HIDP_VALUE_CAPS::default();
        let mut len: u16 = 1;
        let status = HidP_GetSpecificValueCaps(
            HidP_Input,
            Some(USAGE_PAGE_GENERIC),
            None,
            Some(USAGE_HAT_SWITCH),
            &raw mut cap,
            &raw mut len,
            pd,
        );
        if status == HIDP_STATUS_SUCCESS && len != 0 {
            return Some((cap.LogicalMin, cap.LogicalMax));
        }

        let mut hid_caps = HIDP_CAPS::default();
        let status = HidP_GetCaps(pd, &raw mut hid_caps);
        if status != HIDP_STATUS_SUCCESS || hid_caps.NumberInputValueCaps == 0 {
            return None;
        }

        let mut value_caps: Vec<HIDP_VALUE_CAPS> =
            vec![HIDP_VALUE_CAPS::default(); hid_caps.NumberInputValueCaps as usize];
        let mut value_len = hid_caps.NumberInputValueCaps;
        let status = HidP_GetValueCaps(HidP_Input, value_caps.as_mut_ptr(), &raw mut value_len, pd);
        if status != HIDP_STATUS_SUCCESS || value_len == 0 {
            return None;
        }

        for cap in value_caps.iter().take(value_len as usize) {
            if cap.UsagePage != USAGE_PAGE_GENERIC {
                continue;
            }
            let has_hat = if cap.IsRange {
                let r = { cap.Anonymous.Range };
                r.UsageMin <= USAGE_HAT_SWITCH && USAGE_HAT_SWITCH <= r.UsageMax
            } else {
                let nr = { cap.Anonymous.NotRange };
                nr.Usage == USAGE_HAT_SWITCH
            };
            if has_hat {
                return Some((cap.LogicalMin, cap.LogicalMax));
            }
        }
        None
    }
}

fn add_device(ctx: &mut Ctx, h: HANDLE, initial: bool) {
    if ctx.devices.contains_key(&hkey(h)) {
        return;
    }

    let Some(hid) = get_device_info(h) else {
        return;
    };
    if !is_controller_usage(hid.usUsagePage, hid.usUsage) {
        return;
    }

    let name = get_device_name(h).unwrap_or_else(|| format!("RawInput:{h:?}"));
    let uuid = uuid_from_bytes(name.as_bytes());

    let id = ctx.id_by_uuid.get(&uuid).copied().unwrap_or_else(|| {
        let id = PadId(ctx.next_id);
        ctx.next_id += 1;
        ctx.id_by_uuid.insert(uuid, id);
        id
    });

    let refs = ctx.refs_by_uuid.get(&uuid).copied().unwrap_or(0);
    ctx.refs_by_uuid.insert(uuid, refs + 1);

    let preparsed = get_preparsed(h).unwrap_or_default();
    let (hat_min, hat_max) = hat_logical_range(&preparsed).unwrap_or((0, 7));
    let max_buttons = if preparsed.is_empty() {
        0
    } else {
        // SAFETY: `preparsed` contains Raw Input preparsed data for this device,
        // and `HidP_MaxUsageListLength` only reads that buffer.
        unsafe {
            HidP_MaxUsageListLength(
                HidP_Input,
                Some(USAGE_PAGE_BUTTON),
                PHIDP_PREPARSED_DATA(preparsed.as_ptr() as isize),
            )
        }
    };
    let button_cap = max_buttons as usize;

    let dev = Dev {
        id,
        name,
        vendor_id: Some(hid.dwVendorId as u16),
        product_id: Some(hid.dwProductId as u16),
        uuid,
        preparsed,
        max_buttons,
        buttons_prev: Vec::with_capacity(button_cap),
        buttons_now: Vec::with_capacity(button_cap),
        hat_min,
        hat_max,
        dir: [false; 4],
    };

    if refs == 0 {
        ctx.emit_connected(&dev, initial);
    }
    ctx.devices.insert(hkey(h), dev);
}

fn remove_device(ctx: &mut Ctx, h: HANDLE) {
    let Some(dev) = ctx.devices.remove(&hkey(h)) else {
        return;
    };

    let refs = ctx.refs_by_uuid.get(&dev.uuid).copied().unwrap_or(0);
    if refs <= 1 {
        ctx.refs_by_uuid.remove(&dev.uuid);
        ctx.emit_disconnected(&dev, false);
    } else {
        ctx.refs_by_uuid.insert(dev.uuid, refs - 1);
    }
}

fn enumerate_existing(ctx: &mut Ctx) {
    // SAFETY: the first call queries the device count, the second fills the owned
    // `list` buffer sized from that count, and all handles come directly from Raw
    // Input.
    unsafe {
        let mut count: u32 = 0;
        let _ = GetRawInputDeviceList(None, &raw mut count, size_of::<RAWINPUTDEVICELIST>() as u32);
        if count == 0 {
            return;
        }
        let mut list = vec![RAWINPUTDEVICELIST::default(); count as usize];
        let rc = GetRawInputDeviceList(
            Some(list.as_mut_ptr()),
            &raw mut count,
            size_of::<RAWINPUTDEVICELIST>() as u32,
        );
        if rc == u32::MAX {
            return;
        }
        for item in list {
            if item.dwType != RIM_TYPEHID {
                continue;
            }
            add_device(ctx, item.hDevice, true);
        }
    }
}

#[inline(always)]
fn emit_button_diff<F>(
    emit_pad: &mut F,
    dev: &Dev,
    timestamp: Instant,
    host_nanos: u64,
    now: &[u16],
) where
    F: FnMut(PadEvent),
{
    let mut a = 0usize;
    let mut b = 0usize;
    while a < dev.buttons_prev.len() && b < now.len() {
        let pa = dev.buttons_prev[a];
        let nb = now[b];
        if pa == nb {
            a += 1;
            b += 1;
            continue;
        }
        if pa < nb {
            (emit_pad)(PadEvent::RawButton {
                id: dev.id,
                timestamp,
                host_nanos,
                code: PadCode((u32::from(USAGE_PAGE_BUTTON) << 16) | u32::from(pa)),
                uuid: dev.uuid,
                value: 0.0,
                pressed: false,
            });
            a += 1;
        } else {
            (emit_pad)(PadEvent::RawButton {
                id: dev.id,
                timestamp,
                host_nanos,
                code: PadCode((u32::from(USAGE_PAGE_BUTTON) << 16) | u32::from(nb)),
                uuid: dev.uuid,
                value: 1.0,
                pressed: true,
            });
            b += 1;
        }
    }
    while a < dev.buttons_prev.len() {
        let u = dev.buttons_prev[a];
        (emit_pad)(PadEvent::RawButton {
            id: dev.id,
            timestamp,
            host_nanos,
            code: PadCode((u32::from(USAGE_PAGE_BUTTON) << 16) | u32::from(u)),
            uuid: dev.uuid,
            value: 0.0,
            pressed: false,
        });
        a += 1;
    }
    while b < now.len() {
        let u = now[b];
        (emit_pad)(PadEvent::RawButton {
            id: dev.id,
            timestamp,
            host_nanos,
            code: PadCode((u32::from(USAGE_PAGE_BUTTON) << 16) | u32::from(u)),
            uuid: dev.uuid,
            value: 1.0,
            pressed: true,
        });
        b += 1;
    }
}

fn process_hid_report<F>(
    emit_pad: &mut F,
    dev: &mut Dev,
    timestamp: Instant,
    host_nanos: u64,
    report: &mut [u8],
) where
    F: FnMut(PadEvent),
{
    if dev.max_buttons == 0 || dev.preparsed.is_empty() {
        return;
    }

    dev.buttons_now.clear();

    let mut len = dev.max_buttons;
    // SAFETY: `dev.preparsed` is the live preparsed-data blob for this HID, the
    // report buffer is borrowed mutably for the duration of the call, and
    // `buttons_now` was pre-sized from `max_buttons` when the device was added.
    let status = unsafe {
        HidP_GetUsages(
            HidP_Input,
            USAGE_PAGE_BUTTON,
            None,
            dev.buttons_now.as_mut_ptr(),
            &raw mut len,
            PHIDP_PREPARSED_DATA(dev.preparsed.as_ptr() as isize),
            report,
        )
    };
    if status != HIDP_STATUS_SUCCESS {
        return;
    }

    // SAFETY: `HidP_GetUsages` wrote exactly `len` initialized entries to the
    // front of `buttons_now`, and `len <= dev.max_buttons <= capacity`.
    unsafe {
        dev.buttons_now.set_len(len as usize);
    }
    dev.buttons_now.sort_unstable();

    emit_button_diff(emit_pad, dev, timestamp, host_nanos, &dev.buttons_now);
    std::mem::swap(&mut dev.buttons_prev, &mut dev.buttons_now);
    dev.buttons_now.clear();

    let mut hat: u32 = 0;
    // SAFETY: `dev.preparsed` is the live preparsed-data blob for this HID, and
    // `hat` is a writable stack local for the requested usage value.
    let status = unsafe {
        HidP_GetUsageValue(
            HidP_Input,
            USAGE_PAGE_GENERIC,
            None,
            USAGE_HAT_SWITCH,
            &raw mut hat,
            PHIDP_PREPARSED_DATA(dev.preparsed.as_ptr() as isize),
            report,
        )
    };
    if status != HIDP_STATUS_SUCCESS {
        return;
    }

    let hat = hat as i32;
    let mut hat0: i32 = -1;
    if hat >= dev.hat_min && hat <= dev.hat_max {
        let span = dev.hat_max - dev.hat_min + 1;
        let idx = hat - dev.hat_min;
        if span == 9 && dev.hat_min == 0 && dev.hat_max == 8 && idx == 8 {
            hat0 = -1;
        } else if span == 8 {
            hat0 = idx;
        } else if span == 9 && dev.hat_min == 0 && dev.hat_max == 8 && (0..=7).contains(&idx) {
            hat0 = idx;
        } else if span == 4 {
            hat0 = idx * 2;
        }
    }

    let want_up = matches!(hat0, 0 | 1 | 7);
    let want_right = matches!(hat0, 1..=3);
    let want_down = matches!(hat0, 3..=5);
    let want_left = matches!(hat0, 5..=7);
    emit_dir_edges(
        emit_pad,
        dev.id,
        &mut dev.dir,
        timestamp,
        host_nanos,
        [want_up, want_down, want_left, want_right],
    );
}

#[inline(always)]
fn handle_keyboard_input(ctx: &mut Ctx, timestamp: Instant, host_nanos: u64) {
    if !WINDOW_FOCUSED.load(Ordering::Relaxed) {
        return;
    }
    if ctx.buf.len() < size_of::<RAWINPUTHEADER>() + size_of::<RAWKEYBOARD>() {
        return;
    }

    // SAFETY: the size check above guarantees `ctx.buf` contains at least a raw
    // input header plus one `RAWKEYBOARD`, so the unaligned read from the payload
    // is within bounds for this message.
    unsafe {
        let keyboard = ptr::read_unaligned(
            ctx.buf
                .as_ptr()
                .add(size_of::<RAWINPUTHEADER>())
                .cast::<RAWKEYBOARD>(),
        );
        let pressed = matches!(keyboard.Message, WM_KEYDOWN | WM_SYSKEYDOWN);
        let released = matches!(keyboard.Message, WM_KEYUP | WM_SYSKEYUP);
        if !pressed && !released {
            return;
        }

        let Some((code, slot)) = raw_keyboard_code(keyboard) else {
            return;
        };
        if ctx.held[slot] == pressed {
            return;
        }
        ctx.held[slot] = pressed;
        (ctx.emit_key)(RawKeyboardEvent {
            code,
            pressed,
            repeat: false,
            timestamp,
            host_nanos,
        });
    }
}

fn handle_wm_input(ctx: &mut Ctx, hraw: HRAWINPUT) {
    // SAFETY: the first call queries the required byte count, the second fills the
    // owned `ctx.buf` allocation sized from that count, and all subsequent raw
    // pointer reads are guarded by explicit size checks before use.
    unsafe {
        let mut size: u32 = 0;
        let _ = GetRawInputData(
            hraw,
            RID_INPUT,
            None,
            &raw mut size,
            size_of::<RAWINPUTHEADER>() as u32,
        );
        if size == 0 {
            return;
        }
        if ctx.buf.len() < size as usize {
            ctx.buf.resize(size as usize, 0);
        }
        let mut size2 = size;
        let rc = GetRawInputData(
            hraw,
            RID_INPUT,
            Some(ctx.buf.as_mut_ptr().cast::<c_void>()),
            &raw mut size2,
            size_of::<RAWINPUTHEADER>() as u32,
        );
        if rc == u32::MAX {
            return;
        }
        if (size2 as usize) < size_of::<RAWINPUTHEADER>() {
            return;
        }

        let header: RAWINPUTHEADER = ptr::read_unaligned(ctx.buf.as_ptr().cast::<RAWINPUTHEADER>());
        let timestamp = Instant::now();
        let host_nanos = current_host_nanos();
        if header.dwType == RIM_TYPEKEYBOARD_U32 {
            handle_keyboard_input(ctx, timestamp, host_nanos);
            return;
        }
        if header.dwType != RIM_TYPEHID_U32 || !ctx.enable_pad {
            return;
        }

        let dev_handle = header.hDevice;
        if !ctx.devices.contains_key(&hkey(dev_handle)) {
            add_device(ctx, dev_handle, false);
        }
        let Some(dev) = ctx.devices.get_mut(&hkey(dev_handle)) else {
            return;
        };

        let base = ctx.buf.as_mut_ptr().add(size_of::<RAWINPUTHEADER>());
        if (size2 as usize) < size_of::<RAWINPUTHEADER>() + 8 {
            return;
        }
        let dw_size_hid = ptr::read_unaligned(base.cast::<u32>()) as usize;
        let dw_count = ptr::read_unaligned(base.add(4).cast::<u32>()) as usize;
        let data = base.add(8);
        let total = dw_size_hid.saturating_mul(dw_count);
        if (size2 as usize) < size_of::<RAWINPUTHEADER>() + 8 + total {
            return;
        }
        let reports = std::slice::from_raw_parts_mut(data, total);

        let mut idx = 0;
        while idx < dw_count {
            let start = idx * dw_size_hid;
            let end = start + dw_size_hid;
            if end > reports.len() {
                break;
            }
            let report = &mut reports[start..end];
            process_hid_report(&mut ctx.emit_pad, dev, timestamp, host_nanos, report);
            idx += 1;
        }
    }
}

// SAFETY: Windows invokes this window procedure with the ABI and userdata contract established by
// `CreateWindowExW`; `GWLP_USERDATA` is either null or the boxed `Ctx` pointer stored in WM_CREATE.
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: `GWLP_USERDATA` stores either null or the `Ctx` pointer we place in
    // `WM_CREATE`. We only dereference it after null checks below.
    let ctx_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut Ctx;
    match msg {
        WM_CREATE => {
            let cs = lparam.0 as *const CREATESTRUCTW;
            if !cs.is_null() {
                // SAFETY: `cs` comes directly from `WM_CREATE`, so reading its
                // `lpCreateParams` field is valid for this call frame.
                let p = unsafe { (*cs).lpCreateParams }.cast::<Ctx>();
                // SAFETY: `p` is the boxed context pointer passed to
                // `CreateWindowExW`, and storing it in `GWLP_USERDATA` is how the
                // window procedure recovers that context on later messages.
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, p as _) };
            }
            LRESULT(0)
        }
        WM_INPUT => {
            if !ctx_ptr.is_null() {
                // SAFETY: `ctx_ptr` was checked for null and points to the live
                // boxed `Ctx` for this window. `lparam` carries the raw-input handle
                // for the current message.
                unsafe { handle_wm_input(&mut *ctx_ptr, HRAWINPUT(lparam.0 as *mut c_void)) };
            }
            LRESULT(0)
        }
        WM_INPUT_DEVICE_CHANGE => {
            if !ctx_ptr.is_null() {
                // SAFETY: `ctx_ptr` was checked for null and points to the live
                // boxed `Ctx` for this window.
                let ctx = unsafe { &mut *ctx_ptr };
                if ctx.enable_pad {
                    let h = HANDLE(lparam.0 as *mut c_void);
                    match wparam.0 {
                        GIDC_ARRIVAL_U32 => add_device(ctx, h, false),
                        GIDC_REMOVAL_U32 => remove_device(ctx, h),
                        _ => {}
                    }
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            // SAFETY: posting quit is side-effect-only and takes no borrowed Rust
            // memory.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        // SAFETY: forwarding all unhandled messages to `DefWindowProcW` is the
        // required default Win32 behavior for this window procedure.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn run_inner(mut ctx: Box<Ctx>) {
    let _thread_policy = boost_current_thread(ThreadRole::Input);
    // SAFETY: this thread owns the Win32 class registration, message-only window,
    // and message loop. The boxed `ctx` pointer is passed to `CreateWindowExW` and
    // intentionally leaked at shutdown so `GWLP_USERDATA` never dangles while the
    // window can still receive messages.
    unsafe {
        let class_name: Vec<u16> = "deadsync_raw_input\0".encode_utf16().collect();
        let hinst: HINSTANCE = match GetModuleHandleW(PCWSTR::null()) {
            Ok(hinst) => hinst.into(),
            Err(err) => {
                disable_backend(&mut ctx, "module handle lookup", err);
                return;
            }
        };

        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassExW(&raw const wc);

        let hwnd = match CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            wc.lpszClassName,
            PCWSTR::null(),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinst),
            Some(ptr::addr_of_mut!(*ctx).cast::<c_void>().cast_const()),
        ) {
            Ok(hwnd) if !hwnd.0.is_null() => hwnd,
            Ok(_) => {
                disable_backend(
                    &mut ctx,
                    "window creation",
                    windows::core::Error::from_thread(),
                );
                return;
            }
            Err(err) => {
                disable_backend(&mut ctx, "window creation", err);
                return;
            }
        };

        RAW_INPUT_HWND.store(hwnd.0 as isize, Ordering::Release);
        let _ = register_keyboard(hwnd, CAPTURE_ENABLED.load(Ordering::Relaxed));

        if ctx.enable_pad {
            if register_controllers(hwnd) {
                enumerate_existing(&mut ctx);
            }
            emit_startup_complete(&mut ctx);
        }

        let mut msg = MSG::default();
        loop {
            let ok = GetMessageW(&raw mut msg, None, 0, 0);
            if ok.0 <= 0 {
                break;
            }
            let _ = TranslateMessage(&raw const msg);
            DispatchMessageW(&raw const msg);
        }

        RAW_INPUT_HWND.store(0, Ordering::Release);
        std::mem::forget(ctx);
    }
}

pub fn run(
    emit_pad: impl FnMut(PadEvent) + Send + 'static,
    emit_sys: impl FnMut(GpSystemEvent) + Send + 'static,
    emit_key: impl FnMut(RawKeyboardEvent) + Send + 'static,
) {
    run_inner(Box::new(Ctx {
        emit_pad: Box::new(emit_pad),
        emit_sys: Box::new(emit_sys),
        emit_key: Box::new(emit_key),
        devices: HashMap::new(),
        id_by_uuid: HashMap::new(),
        refs_by_uuid: HashMap::new(),
        next_id: 0,
        buf: Vec::with_capacity(1024),
        held: [false; RAW_KEY_HELD_SLOTS],
        enable_pad: true,
    }));
}

pub fn run_keyboard_only(emit_key: impl FnMut(RawKeyboardEvent) + Send + 'static) {
    run_inner(Box::new(Ctx {
        emit_pad: Box::new(|_| {}),
        emit_sys: Box::new(|_| {}),
        emit_key: Box::new(emit_key),
        devices: HashMap::new(),
        id_by_uuid: HashMap::new(),
        refs_by_uuid: HashMap::new(),
        next_id: 0,
        buf: Vec::with_capacity(1024),
        held: [false; RAW_KEY_HELD_SLOTS],
        enable_pad: false,
    }));
}
