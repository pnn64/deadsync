use super::{GpSystemEvent, PadBackend, PadCode, PadDir, PadEvent, PadId, uuid_from_bytes};
use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;
use std::time::Instant;

use windows::Win32::Devices::HumanInterfaceDevice::{
    HIDP_CAPS, HIDP_STATUS_SUCCESS, HIDP_VALUE_CAPS, HidP_GetCaps, HidP_GetSpecificValueCaps,
    HidP_GetUsageValue, HidP_GetUsages, HidP_GetValueCaps, HidP_Input, HidP_MaxUsageListLength,
    PHIDP_PREPARSED_DATA,
};
use windows::Win32::Foundation::{HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::{
    GetRawInputData, GetRawInputDeviceInfoW, GetRawInputDeviceList, HRAWINPUT, RAWINPUTDEVICE,
    RAWINPUTDEVICELIST, RAWINPUTHEADER, RID_DEVICE_INFO, RID_DEVICE_INFO_HID, RID_INPUT,
    RIDEV_DEVNOTIFY, RIDEV_INPUTSINK, RIDI_DEVICEINFO, RIDI_DEVICENAME, RIDI_PREPARSEDDATA,
    RIM_TYPEHID, RegisterRawInputDevices,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetMessageW,
    GetWindowLongPtrW, HWND_MESSAGE, MSG, PostQuitMessage, RegisterClassExW, SetWindowLongPtrW,
    TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CREATE, WM_DESTROY, WM_INPUT,
    WM_INPUT_DEVICE_CHANGE, WNDCLASSEXW,
};
use windows::core::PCWSTR;

const USAGE_PAGE_GENERIC_DESKTOP: u16 = 0x01;
const USAGE_JOYSTICK: u16 = 0x04;
const USAGE_GAMEPAD: u16 = 0x05;
const USAGE_MULTI_AXIS: u16 = 0x08;

const USAGE_PAGE_BUTTON: u16 = 0x09;
const USAGE_HAT_SWITCH: u16 = 0x39;

const RIM_TYPEHID_U32: u32 = RIM_TYPEHID.0;
const GIDC_ARRIVAL_U32: usize = 1;
const GIDC_REMOVAL_U32: usize = 2;

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
    devices: HashMap<isize, Dev>,
    id_by_uuid: HashMap<[u8; 16], PadId>,
    refs_by_uuid: HashMap<[u8; 16], u32>,
    next_id: u32,
    buf: Vec<u8>,
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

#[inline(always)]
const fn is_controller_usage(usage_page: u16, usage: u16) -> bool {
    usage_page == USAGE_PAGE_GENERIC_DESKTOP
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
    unsafe {
        let pd = PHIDP_PREPARSED_DATA(preparsed.as_ptr() as isize);

        let mut cap = HIDP_VALUE_CAPS::default();
        let mut len: u16 = 1;
        let status = HidP_GetSpecificValueCaps(
            HidP_Input,
            Some(USAGE_PAGE_GENERIC_DESKTOP),
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
            if cap.UsagePage != USAGE_PAGE_GENERIC_DESKTOP {
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

    let info = get_device_info(h);
    let Some(hid) = info else {
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
        unsafe {
            HidP_MaxUsageListLength(
                HidP_Input,
                Some(USAGE_PAGE_BUTTON),
                PHIDP_PREPARSED_DATA(preparsed.as_ptr() as isize),
            )
        }
    };
    let dev = Dev {
        id,
        name,
        vendor_id: Some(hid.dwVendorId as u16),
        product_id: Some(hid.dwProductId as u16),
        uuid,
        preparsed,
        max_buttons,
        buttons_prev: Vec::new(),
        buttons_now: Vec::new(),
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
fn emit_button_diff(
    emit_pad: &mut (dyn FnMut(PadEvent) + Send),
    dev: &Dev,
    timestamp: Instant,
    now: &[u16],
) {
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
            // Released.
            (emit_pad)(PadEvent::RawButton {
                id: dev.id,
                timestamp,
                code: PadCode((u32::from(USAGE_PAGE_BUTTON) << 16) | u32::from(pa)),
                uuid: dev.uuid,
                value: 0.0,
                pressed: false,
            });
            a += 1;
        } else {
            // Pressed.
            (emit_pad)(PadEvent::RawButton {
                id: dev.id,
                timestamp,
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
            code: PadCode((u32::from(USAGE_PAGE_BUTTON) << 16) | u32::from(u)),
            uuid: dev.uuid,
            value: 1.0,
            pressed: true,
        });
        b += 1;
    }
}

fn process_hid_report(
    emit_pad: &mut (dyn FnMut(PadEvent) + Send),
    dev: &mut Dev,
    timestamp: Instant,
    report: &mut [u8],
) {
    if dev.max_buttons == 0 || dev.preparsed.is_empty() {
        return;
    }

    let want_cap = dev.max_buttons as usize;
    if dev.buttons_now.capacity() < want_cap {
        dev.buttons_now
            .reserve(want_cap - dev.buttons_now.capacity());
    }
    dev.buttons_now.clear();

    let mut len = dev.max_buttons;
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

    unsafe {
        dev.buttons_now.set_len(len as usize);
    }
    dev.buttons_now.sort_unstable();

    emit_button_diff(emit_pad, dev, timestamp, &dev.buttons_now);
    std::mem::swap(&mut dev.buttons_prev, &mut dev.buttons_now);
    dev.buttons_now.clear();

    // D-pad hat switch → PadDir edges (so dance pads / DPAD-only devices can bind directions).
    let mut hat: u32 = 0;
    let status = unsafe {
        HidP_GetUsageValue(
            HidP_Input,
            USAGE_PAGE_GENERIC_DESKTOP,
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

    // HID hat switches commonly come in two forms:
    // - logical 0..=7 with null state 8 (HasNull=true)
    // - logical 1..=8 with null state 0 (HasNull=true)
    // We use the descriptor-reported logical range to normalize to 0..=7, and treat values
    // outside the range as neutral.
    let hat = hat as i32;
    let mut hat0: i32 = -1;
    if hat >= dev.hat_min && hat <= dev.hat_max {
        let span = dev.hat_max - dev.hat_min + 1;
        let idx = hat - dev.hat_min;
        if span == 9 && dev.hat_min == 0 && dev.hat_max == 8 && idx == 8 {
            // Some devices include neutral as 8 inside the logical range.
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
    let want = [want_up, want_down, want_left, want_right];
    let dirs = [PadDir::Up, PadDir::Down, PadDir::Left, PadDir::Right];
    for i in 0..4 {
        if dev.dir[i] == want[i] {
            continue;
        }
        dev.dir[i] = want[i];
        (emit_pad)(PadEvent::Dir {
            id: dev.id,
            timestamp,
            dir: dirs[i],
            pressed: want[i],
        });
    }
}

fn handle_wm_input(ctx: &mut Ctx, hraw: HRAWINPUT) {
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

        // Parse header unaligned; buffer alignment is not guaranteed.
        if (size2 as usize) < size_of::<RAWINPUTHEADER>() {
            return;
        }
        let header: RAWINPUTHEADER = ptr::read_unaligned(ctx.buf.as_ptr().cast::<RAWINPUTHEADER>());
        if header.dwType != RIM_TYPEHID_U32 {
            return;
        }

        let dev_handle = header.hDevice;
        if !ctx.devices.contains_key(&hkey(dev_handle)) {
            add_device(ctx, dev_handle, false);
        }
        let Some(dev) = ctx.devices.get_mut(&hkey(dev_handle)) else {
            return;
        };

        // RAWHID starts immediately after RAWINPUTHEADER.
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
            let timestamp = Instant::now();
            process_hid_report(ctx.emit_pad.as_mut(), dev, timestamp, report);
            idx += 1;
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let ctx_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut Ctx;
    match msg {
        WM_CREATE => {
            let cs = lparam.0 as *const CREATESTRUCTW;
            if !cs.is_null() {
                let p = unsafe { (*cs).lpCreateParams }.cast::<Ctx>();
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, p as isize) };
            }
            LRESULT(0)
        }
        WM_INPUT => {
            if !ctx_ptr.is_null() {
                unsafe { handle_wm_input(&mut *ctx_ptr, HRAWINPUT(lparam.0 as *mut c_void)) };
            }
            LRESULT(0)
        }
        WM_INPUT_DEVICE_CHANGE => {
            if !ctx_ptr.is_null() {
                let ctx = unsafe { &mut *ctx_ptr };
                let h = HANDLE(lparam.0 as *mut c_void);
                match wparam.0 {
                    GIDC_ARRIVAL_U32 => add_device(ctx, h, false),
                    GIDC_REMOVAL_U32 => remove_device(ctx, h),
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

pub fn run(
    emit_pad: impl FnMut(PadEvent) + Send + 'static,
    emit_sys: impl FnMut(GpSystemEvent) + Send + 'static,
) {
    unsafe {
        let class_name: Vec<u16> = "deadsync_raw_input\0".encode_utf16().collect();
        let hinst: HINSTANCE = GetModuleHandleW(PCWSTR::null()).unwrap_or_default().into();

        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassExW(&raw const wc);

        let mut ctx = Box::new(Ctx {
            emit_pad: Box::new(emit_pad),
            emit_sys: Box::new(emit_sys),
            devices: HashMap::new(),
            id_by_uuid: HashMap::new(),
            refs_by_uuid: HashMap::new(),
            next_id: 0,
            buf: Vec::with_capacity(1024),
        });

        let hwnd = CreateWindowExW(
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
        );

        let hwnd = hwnd.unwrap_or_default();
        if hwnd.0.is_null() {
            loop {
                std::thread::park();
            }
        }

        // Register for HID controllers (joystick/gamepad/multiaxis), plus device notifications.
        let devices = [
            RAWINPUTDEVICE {
                usUsagePage: USAGE_PAGE_GENERIC_DESKTOP,
                usUsage: USAGE_JOYSTICK,
                dwFlags: RIDEV_DEVNOTIFY | RIDEV_INPUTSINK,
                hwndTarget: hwnd,
            },
            RAWINPUTDEVICE {
                usUsagePage: USAGE_PAGE_GENERIC_DESKTOP,
                usUsage: USAGE_GAMEPAD,
                dwFlags: RIDEV_DEVNOTIFY | RIDEV_INPUTSINK,
                hwndTarget: hwnd,
            },
            RAWINPUTDEVICE {
                usUsagePage: USAGE_PAGE_GENERIC_DESKTOP,
                usUsage: USAGE_MULTI_AXIS,
                dwFlags: RIDEV_DEVNOTIFY | RIDEV_INPUTSINK,
                hwndTarget: hwnd,
            },
        ];
        let _ = RegisterRawInputDevices(&devices, size_of::<RAWINPUTDEVICE>() as u32);

        enumerate_existing(&mut ctx);
        (ctx.emit_sys)(GpSystemEvent::StartupComplete);

        let mut msg = MSG::default();
        loop {
            let ok = GetMessageW(&raw mut msg, None, 0, 0);
            if ok.0 <= 0 {
                break;
            }
            let _ = TranslateMessage(&raw const msg);
            DispatchMessageW(&raw const msg);
        }

        // Keep ctx alive forever (message loop runs until process exit).
        std::mem::forget(ctx);
    }
}
