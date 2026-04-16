use super::{GpSystemEvent, PadBackend, PadCode, PadEvent, PadId, emit_dir_edges, uuid_from_bytes};
use crate::engine::host_time::now_nanos;
use crate::engine::input::RawKeyboardEvent;
use log::debug;
use mach2::mach_time::{mach_absolute_time, mach_timebase_info, mach_timebase_info_data_t};
use std::collections::hash_map::Entry;
use std::ffi::{c_char, c_void};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::time::Instant;
use winit::keyboard::KeyCode;

type CFAllocatorRef = *const c_void;
type CFRunLoopRef = *mut c_void;
type CFStringRef = *const c_void;
type CFTypeRef = *const c_void;
type CFIndex = isize;

type IOHIDManagerRef = *mut c_void;
type IOHIDDeviceRef = *mut c_void;
type IOHIDValueRef = *mut c_void;
type IOHIDElementRef = *mut c_void;
type IOReturn = i32;

#[link(name = "CoreFoundation", kind = "framework")]
// SAFETY: These are direct CoreFoundation FFI declarations. Callers must pass valid framework
// object handles/pointers and obey CoreFoundation ownership rules for returned objects.
unsafe extern "C" {
    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopRun();
    fn CFStringCreateWithCString(
        alloc: CFAllocatorRef,
        cstr: *const c_char,
        encoding: u32,
    ) -> CFStringRef;
    fn CFRelease(cf: CFTypeRef);
    fn CFSetGetCount(the_set: CFTypeRef) -> CFIndex;
    fn CFSetGetValues(the_set: CFTypeRef, values: *mut CFTypeRef);
    fn CFNumberGetValue(number: CFTypeRef, the_type: i32, value_ptr: *mut c_void) -> bool;

    static kCFRunLoopDefaultMode: CFStringRef;
}

#[link(name = "IOKit", kind = "framework")]
// SAFETY: These are direct IOKit HID FFI declarations. Callers must pass live manager/device/
// value handles and callback pointers that remain valid for the duration required by IOKit.
unsafe extern "C" {
    fn IOHIDManagerCreate(alloc: CFAllocatorRef, options: u32) -> IOHIDManagerRef;
    fn IOHIDManagerSetDeviceMatching(manager: IOHIDManagerRef, matching: CFTypeRef);
    fn IOHIDManagerRegisterDeviceMatchingCallback(
        manager: IOHIDManagerRef,
        callback: extern "C" fn(*mut c_void, IOReturn, *mut c_void, IOHIDDeviceRef),
        context: *mut c_void,
    );
    fn IOHIDManagerRegisterDeviceRemovalCallback(
        manager: IOHIDManagerRef,
        callback: extern "C" fn(*mut c_void, IOReturn, *mut c_void, IOHIDDeviceRef),
        context: *mut c_void,
    );
    fn IOHIDManagerRegisterInputValueCallback(
        manager: IOHIDManagerRef,
        callback: extern "C" fn(*mut c_void, IOReturn, *mut c_void, IOHIDValueRef),
        context: *mut c_void,
    );
    fn IOHIDManagerScheduleWithRunLoop(
        manager: IOHIDManagerRef,
        run_loop: CFRunLoopRef,
        mode: CFStringRef,
    );
    fn IOHIDManagerOpen(manager: IOHIDManagerRef, options: u32) -> IOReturn;
    fn IOHIDManagerCopyDevices(manager: IOHIDManagerRef) -> CFTypeRef;

    fn IOHIDDeviceGetProperty(device: IOHIDDeviceRef, key: CFStringRef) -> CFTypeRef;
    fn IOHIDValueGetElement(value: IOHIDValueRef) -> IOHIDElementRef;
    fn IOHIDValueGetIntegerValue(value: IOHIDValueRef) -> CFIndex;
    fn IOHIDValueGetTimeStamp(value: IOHIDValueRef) -> u64;
    fn IOHIDElementGetDevice(elem: IOHIDElementRef) -> IOHIDDeviceRef;
    fn IOHIDElementGetUsagePage(elem: IOHIDElementRef) -> u32;
    fn IOHIDElementGetUsage(elem: IOHIDElementRef) -> u32;
}

const KCFSTRING_ENCODING_UTF8: u32 = 0x0800_0100;

fn cfstr(s: &str) -> CFStringRef {
    let c = std::ffi::CString::new(s).unwrap();
    // SAFETY: `c` is a valid NUL-terminated UTF-8 string that remains alive for
    // the duration of the call, and CoreFoundation copies the bytes into the new
    // `CFString`.
    unsafe { CFStringCreateWithCString(ptr::null(), c.as_ptr(), KCFSTRING_ENCODING_UTF8) }
}

fn cfnum_i32(v: CFTypeRef) -> Option<i32> {
    if v.is_null() {
        return None;
    }
    let mut out: i32 = 0;
    // SAFETY: `v` was checked for null, and `out` points to writable stack
    // storage for the requested CoreFoundation number conversion.
    let ok = unsafe { CFNumberGetValue(v, 9, ptr::addr_of_mut!(out).cast::<c_void>()) };
    ok.then_some(out)
}

#[derive(Clone, Copy)]
struct AxisState {
    code: u32,
    value: i64,
}

struct PadDev {
    id: PadId,
    name: String,
    uuid: [u8; 16],
    last_axis: Vec<AxisState>,
    dir: [bool; 4],
}

struct KeyDev;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DevClass {
    Pad,
    Keyboard,
}

#[derive(Clone, Copy)]
struct HostClock {
    numer: u32,
    denom: u32,
    offset_nanos: i128,
}

impl HostClock {
    fn calibrate() -> Option<Self> {
        let mut info = mach_timebase_info_data_t { numer: 0, denom: 0 };
        // SAFETY: `mach_timebase_info` writes into the provided stack local and
        // does not retain the pointer after returning.
        let status = unsafe { mach_timebase_info(&mut info) };
        if status != 0 || info.denom == 0 {
            return None;
        }
        let host_before = now_nanos();
        // SAFETY: `mach_absolute_time` reads the current monotonic host clock and
        // takes no pointers or borrowed Rust data.
        let mach_now = unsafe { mach_absolute_time() };
        let host_after = now_nanos();
        let host_mid =
            host_before / 2 + host_after / 2 + ((host_before & 1) + (host_after & 1)) / 2;
        let mach_nanos = scale_mach_time(mach_now, info.numer, info.denom);
        Some(Self {
            numer: info.numer,
            denom: info.denom,
            offset_nanos: i128::from(mach_nanos) - i128::from(host_mid),
        })
    }

    #[inline(always)]
    fn host_nanos(self, mach_time: u64) -> Option<u64> {
        if mach_time == 0 {
            return None;
        }
        let mach_nanos = scale_mach_time(mach_time, self.numer, self.denom);
        Some((i128::from(mach_nanos) - self.offset_nanos).clamp(0, i128::from(u64::MAX)) as u64)
    }
}

struct Ctx {
    emit_pad: Box<dyn FnMut(PadEvent) + Send>,
    emit_sys: Box<dyn FnMut(GpSystemEvent) + Send>,
    emit_key: Box<dyn FnMut(RawKeyboardEvent) + Send>,
    next_id: u32,
    id_by_uuid: HashMap<[u8; 16], PadId>,
    pad_devs: HashMap<usize, PadDev>,
    key_devs: HashMap<usize, KeyDev>,
    host_clock: Option<HostClock>,
    startup_complete_sent: bool,

    // CF strings (owned; we intentionally leak ctx).
    key_primary_usage_page: CFStringRef,
    key_primary_usage: CFStringRef,
    key_vendor_id: CFStringRef,
    key_product_id: CFStringRef,
    key_location_id: CFStringRef,
}

#[inline(always)]
fn axis_changed(last_axis: &mut Vec<AxisState>, code: u32, value: i64) -> bool {
    for axis in last_axis.iter_mut() {
        if axis.code != code {
            continue;
        }
        if axis.value == value {
            return false;
        }
        axis.value = value;
        return true;
    }
    last_axis.push(AxisState { code, value });
    true
}

static KEYBOARD_WINDOW_FOCUSED: AtomicBool = AtomicBool::new(true);
static KEYBOARD_CAPTURE_ENABLED: AtomicBool = AtomicBool::new(true);

#[inline(always)]
pub fn set_keyboard_window_focused(focused: bool) {
    KEYBOARD_WINDOW_FOCUSED.store(focused, Ordering::Relaxed);
}

#[inline(always)]
pub fn set_keyboard_capture_enabled(enabled: bool) {
    KEYBOARD_CAPTURE_ENABLED.store(enabled, Ordering::Relaxed);
}

#[inline(always)]
fn keyboard_capture_active() -> bool {
    KEYBOARD_WINDOW_FOCUSED.load(Ordering::Relaxed)
        && KEYBOARD_CAPTURE_ENABLED.load(Ordering::Relaxed)
}

#[inline(always)]
fn timestamp_from_host_sample(
    target_host_nanos: u64,
    sample_host_nanos: u64,
    sample: Instant,
) -> Instant {
    if target_host_nanos >= sample_host_nanos {
        sample
            .checked_add(Duration::from_nanos(
                target_host_nanos.saturating_sub(sample_host_nanos),
            ))
            .unwrap_or(sample)
    } else {
        sample
            .checked_sub(Duration::from_nanos(
                sample_host_nanos.saturating_sub(target_host_nanos),
            ))
            .unwrap_or(sample)
    }
}

#[inline(always)]
fn event_time(host_clock: Option<HostClock>, value: IOHIDValueRef) -> (Instant, u64) {
    let sample = Instant::now();
    let sample_host_nanos = now_nanos();
    let host_nanos = host_clock
        .and_then(|clock| {
            // SAFETY: `value` is the live IOHID value delivered by the callback for
            // this call frame, so querying its timestamp is valid here.
            clock.host_nanos(unsafe { IOHIDValueGetTimeStamp(value) })
        })
        .unwrap_or(sample_host_nanos);
    (
        timestamp_from_host_sample(host_nanos, sample_host_nanos, sample),
        host_nanos,
    )
}

#[inline(always)]
fn debug_log_keyboard_value(
    device_key: usize,
    usage_page: u16,
    usage: u16,
    value: i64,
    host_nanos: u64,
    capture_active: bool,
) {
    let mapped = if usage_page == 0x07 {
        hid_key_code(usage)
    } else {
        None
    };
    debug!(
        "macos iohid keyboard raw: dev=0x{device_key:X} page=0x{usage_page:04X} usage=0x{usage:04X} value={value} host_ns={host_nanos} capture={} mapped={mapped:?}",
        capture_active
    );
}

fn hid_key_code(usage: u16) -> Option<KeyCode> {
    Some(match usage {
        0x04 => KeyCode::KeyA,
        0x05 => KeyCode::KeyB,
        0x06 => KeyCode::KeyC,
        0x07 => KeyCode::KeyD,
        0x08 => KeyCode::KeyE,
        0x09 => KeyCode::KeyF,
        0x0A => KeyCode::KeyG,
        0x0B => KeyCode::KeyH,
        0x0C => KeyCode::KeyI,
        0x0D => KeyCode::KeyJ,
        0x0E => KeyCode::KeyK,
        0x0F => KeyCode::KeyL,
        0x10 => KeyCode::KeyM,
        0x11 => KeyCode::KeyN,
        0x12 => KeyCode::KeyO,
        0x13 => KeyCode::KeyP,
        0x14 => KeyCode::KeyQ,
        0x15 => KeyCode::KeyR,
        0x16 => KeyCode::KeyS,
        0x17 => KeyCode::KeyT,
        0x18 => KeyCode::KeyU,
        0x19 => KeyCode::KeyV,
        0x1A => KeyCode::KeyW,
        0x1B => KeyCode::KeyX,
        0x1C => KeyCode::KeyY,
        0x1D => KeyCode::KeyZ,
        0x1E => KeyCode::Digit1,
        0x1F => KeyCode::Digit2,
        0x20 => KeyCode::Digit3,
        0x21 => KeyCode::Digit4,
        0x22 => KeyCode::Digit5,
        0x23 => KeyCode::Digit6,
        0x24 => KeyCode::Digit7,
        0x25 => KeyCode::Digit8,
        0x26 => KeyCode::Digit9,
        0x27 => KeyCode::Digit0,
        0x28 => KeyCode::Enter,
        0x29 => KeyCode::Escape,
        0x2A => KeyCode::Backspace,
        0x2B => KeyCode::Tab,
        0x2C => KeyCode::Space,
        0x2D => KeyCode::Minus,
        0x2E => KeyCode::Equal,
        0x2F => KeyCode::BracketLeft,
        0x30 => KeyCode::BracketRight,
        0x31 => KeyCode::Backslash,
        0x32 => KeyCode::IntlBackslash,
        0x33 => KeyCode::Semicolon,
        0x34 => KeyCode::Quote,
        0x35 => KeyCode::Backquote,
        0x36 => KeyCode::Comma,
        0x37 => KeyCode::Period,
        0x38 => KeyCode::Slash,
        0x39 => KeyCode::CapsLock,
        0x3A => KeyCode::F1,
        0x3B => KeyCode::F2,
        0x3C => KeyCode::F3,
        0x3D => KeyCode::F4,
        0x3E => KeyCode::F5,
        0x3F => KeyCode::F6,
        0x40 => KeyCode::F7,
        0x41 => KeyCode::F8,
        0x42 => KeyCode::F9,
        0x43 => KeyCode::F10,
        0x44 => KeyCode::F11,
        0x45 => KeyCode::F12,
        0x46 => KeyCode::PrintScreen,
        0x47 => KeyCode::ScrollLock,
        0x48 => KeyCode::Pause,
        0x49 => KeyCode::Insert,
        0x4A => KeyCode::Home,
        0x4B => KeyCode::PageUp,
        0x4C => KeyCode::Delete,
        0x4D => KeyCode::End,
        0x4E => KeyCode::PageDown,
        0x4F => KeyCode::ArrowRight,
        0x50 => KeyCode::ArrowLeft,
        0x51 => KeyCode::ArrowDown,
        0x52 => KeyCode::ArrowUp,
        0x53 => KeyCode::NumLock,
        0x54 => KeyCode::NumpadDivide,
        0x55 => KeyCode::NumpadMultiply,
        0x56 => KeyCode::NumpadSubtract,
        0x57 => KeyCode::NumpadAdd,
        0x58 => KeyCode::NumpadEnter,
        0x59 => KeyCode::Numpad1,
        0x5A => KeyCode::Numpad2,
        0x5B => KeyCode::Numpad3,
        0x5C => KeyCode::Numpad4,
        0x5D => KeyCode::Numpad5,
        0x5E => KeyCode::Numpad6,
        0x5F => KeyCode::Numpad7,
        0x60 => KeyCode::Numpad8,
        0x61 => KeyCode::Numpad9,
        0x62 => KeyCode::Numpad0,
        0x63 => KeyCode::NumpadDecimal,
        0x64 => KeyCode::IntlBackslash,
        0x65 => KeyCode::ContextMenu,
        0x66 => KeyCode::Power,
        0x67 => KeyCode::NumpadEqual,
        0x68 => KeyCode::F13,
        0x69 => KeyCode::F14,
        0x6A => KeyCode::F15,
        0x6B => KeyCode::F16,
        0x6C => KeyCode::F17,
        0x6D => KeyCode::F18,
        0x6E => KeyCode::F19,
        0x6F => KeyCode::F20,
        0x70 => KeyCode::F21,
        0x71 => KeyCode::F22,
        0x72 => KeyCode::F23,
        0x73 => KeyCode::F24,
        0x75 => KeyCode::Help,
        0x77 => KeyCode::Select,
        0x79 => KeyCode::Again,
        0x7A => KeyCode::Undo,
        0x7B => KeyCode::Cut,
        0x7C => KeyCode::Copy,
        0x7D => KeyCode::Paste,
        0x7E => KeyCode::Find,
        0x7F => KeyCode::AudioVolumeMute,
        0x80 => KeyCode::AudioVolumeUp,
        0x81 => KeyCode::AudioVolumeDown,
        0x85 => KeyCode::NumpadComma,
        0x87 => KeyCode::IntlRo,
        0x88 => KeyCode::KanaMode,
        0x89 => KeyCode::IntlYen,
        0x8A => KeyCode::Convert,
        0x8B => KeyCode::NonConvert,
        0x90 => KeyCode::Lang1,
        0x91 => KeyCode::Lang2,
        0x92 => KeyCode::Lang3,
        0x93 => KeyCode::Lang4,
        0x94 => KeyCode::Lang5,
        0xE0 => KeyCode::ControlLeft,
        0xE1 => KeyCode::ShiftLeft,
        0xE2 => KeyCode::AltLeft,
        0xE3 => KeyCode::SuperLeft,
        0xE4 => KeyCode::ControlRight,
        0xE5 => KeyCode::ShiftRight,
        0xE6 => KeyCode::AltRight,
        0xE7 => KeyCode::SuperRight,
        _ => return None,
    })
}

extern "C" fn on_match(
    _ctx: *mut c_void,
    _res: IOReturn,
    _sender: *mut c_void,
    device: IOHIDDeviceRef,
) {
    // SAFETY: `_ctx` was registered from a leaked `Box<Ctx>` in `run`, so it
    // stays valid for the lifetime of the CFRunLoop. `device` is the callback's
    // live IOHID device handle for this invocation.
    unsafe {
        let ctx = &mut *(_ctx as *mut Ctx);
        let key = device as usize;
        if ctx.pad_devs.contains_key(&key) || ctx.key_devs.contains_key(&key) {
            return;
        }

        let up = cfnum_i32(IOHIDDeviceGetProperty(device, ctx.key_primary_usage_page))
            .map(|x| x as u16)
            .unwrap_or(0);
        let u = cfnum_i32(IOHIDDeviceGetProperty(device, ctx.key_primary_usage))
            .map(|x| x as u16)
            .unwrap_or(0);
        let class = if up == 0x01 && matches!(u, 0x04 | 0x05 | 0x08) {
            DevClass::Pad
        } else if up == 0x01 && u == 0x06 {
            DevClass::Keyboard
        } else {
            return;
        };

        if class == DevClass::Keyboard {
            ctx.key_devs.insert(key, KeyDev);
            return;
        }

        let vendor_id =
            cfnum_i32(IOHIDDeviceGetProperty(device, ctx.key_vendor_id)).map(|x| x as u16);
        let product_id =
            cfnum_i32(IOHIDDeviceGetProperty(device, ctx.key_product_id)).map(|x| x as u16);
        let location_id = cfnum_i32(IOHIDDeviceGetProperty(device, ctx.key_location_id));

        let name = format!(
            "iohid:vid={:04X} pid={:04X} loc={}",
            vendor_id.unwrap_or(0),
            product_id.unwrap_or(0),
            location_id.unwrap_or(-1),
        );
        let uuid = uuid_from_bytes(name.as_bytes());
        let id = match ctx.id_by_uuid.entry(uuid) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let id = PadId(ctx.next_id);
                ctx.next_id += 1;
                *entry.insert(id)
            }
        };

        let dev = PadDev {
            id,
            name: name.clone(),
            uuid,
            // Pads usually expose only a handful of analog elements, so a tiny
            // linear cache is cheaper here than hashing in the input callback.
            last_axis: Vec::with_capacity(8),
            dir: [false; 4],
        };

        (ctx.emit_sys)(GpSystemEvent::Connected {
            name: name.clone(),
            id,
            vendor_id,
            product_id,
            backend: PadBackend::MacOsIohid,
            initial: !ctx.startup_complete_sent,
        });

        ctx.pad_devs.insert(key, dev);
    }
}

extern "C" fn on_remove(
    _ctx: *mut c_void,
    _res: IOReturn,
    _sender: *mut c_void,
    device: IOHIDDeviceRef,
) {
    // SAFETY: `_ctx` was registered from a leaked `Box<Ctx>` in `run`, so it
    // stays valid for the lifetime of the CFRunLoop. `device` is the callback's
    // live IOHID device handle for this invocation.
    unsafe {
        let ctx = &mut *(_ctx as *mut Ctx);
        let key = device as usize;
        if ctx.key_devs.remove(&key).is_some() {
            return;
        }
        let Some(dev) = ctx.pad_devs.remove(&key) else {
            return;
        };
        (ctx.emit_sys)(GpSystemEvent::Disconnected {
            name: dev.name,
            id: dev.id,
            backend: PadBackend::MacOsIohid,
            initial: !ctx.startup_complete_sent,
        });
    }
}

extern "C" fn on_input(
    _ctx: *mut c_void,
    _res: IOReturn,
    _sender: *mut c_void,
    value: IOHIDValueRef,
) {
    // SAFETY: `_ctx` was registered from a leaked `Box<Ctx>` in `run`, so it
    // stays valid for the lifetime of the CFRunLoop. `value` and any derived HID
    // element/device handles are only used during this callback.
    unsafe {
        let ctx = &mut *(_ctx as *mut Ctx);
        let (timestamp, host_nanos) = event_time(ctx.host_clock, value);
        let elem = IOHIDValueGetElement(value);
        if elem.is_null() {
            return;
        }
        let device = IOHIDElementGetDevice(elem);
        if device.is_null() {
            return;
        }
        let key = device as usize;
        let usage_page = IOHIDElementGetUsagePage(elem) as u16;
        let usage = IOHIDElementGetUsage(elem) as u16;
        let code = ((usage_page as u32) << 16) | (usage as u32);
        let v = IOHIDValueGetIntegerValue(value) as i64;

        if let Some(dev) = ctx.pad_devs.get_mut(&key) {
            // Hat switch → PadDir edges (match common HID D-pads/pads).
            // `emit_dir_edges` already filters unchanged logical directions, so
            // coalescing raw hat values here is redundant and risks hiding
            // distinct edges on devices that reuse HID usages across elements.
            if usage_page == 0x01 && usage == 0x39 {
                let hat = v as u32;
                let want_up = matches!(hat, 0 | 1 | 7);
                let want_right = matches!(hat, 1 | 2 | 3);
                let want_down = matches!(hat, 3 | 4 | 5);
                let want_left = matches!(hat, 5 | 6 | 7);
                emit_dir_edges(
                    &mut ctx.emit_pad,
                    dev.id,
                    &mut dev.dir,
                    timestamp,
                    host_nanos,
                    [want_up, want_down, want_left, want_right],
                );
                return;
            }

            if usage_page == 0x09 {
                let pressed = v != 0;
                // The shared debounce path already tracks repeated button
                // states by binding. Coalescing button values in the backend can
                // mask real press/release transitions on some IOHID devices.
                (ctx.emit_pad)(PadEvent::RawButton {
                    id: dev.id,
                    timestamp,
                    host_nanos,
                    code: PadCode(code),
                    uuid: dev.uuid,
                    value: if pressed { 1.0 } else { 0.0 },
                    pressed,
                });
            } else {
                if !axis_changed(&mut dev.last_axis, code, v) {
                    return;
                }
                (ctx.emit_pad)(PadEvent::RawAxis {
                    id: dev.id,
                    timestamp,
                    host_nanos,
                    code: PadCode(code),
                    uuid: dev.uuid,
                    value: v as f32,
                });
            }
            return;
        }

        let Some(_dev) = ctx.key_devs.get(&key) else {
            return;
        };
        let capture_active = keyboard_capture_active();
        debug_log_keyboard_value(key, usage_page, usage, v, host_nanos, capture_active);
        if usage_page != 0x07 {
            return;
        }
        if !capture_active {
            return;
        }
        let Some(code) = hid_key_code(usage) else {
            return;
        };
        // Like pad buttons, repeated raw keyboard values are filtered later by
        // the shared debounce store, which preserves the true edge semantics.
        (ctx.emit_key)(RawKeyboardEvent {
            code,
            pressed: v != 0,
            repeat: false,
            timestamp,
            host_nanos,
        });
    }
}

pub fn run(
    emit_pad: impl FnMut(PadEvent) + Send + 'static,
    emit_sys: impl FnMut(GpSystemEvent) + Send + 'static,
    emit_key: impl FnMut(RawKeyboardEvent) + Send + 'static,
) {
    // SAFETY: the manager, run loop, and callback registrations all stay within
    // this thread; `ctx` is intentionally leaked so the callback context pointer
    // remains valid for the life of the run loop.
    unsafe {
        let manager = IOHIDManagerCreate(ptr::null(), 0);
        if manager.is_null() {
            loop {
                std::thread::park();
            }
        }

        let mut ctx = Box::new(Ctx {
            emit_pad: Box::new(emit_pad),
            emit_sys: Box::new(emit_sys),
            emit_key: Box::new(emit_key),
            next_id: 0,
            id_by_uuid: HashMap::new(),
            pad_devs: HashMap::new(),
            key_devs: HashMap::new(),
            host_clock: HostClock::calibrate(),
            startup_complete_sent: false,
            key_primary_usage_page: cfstr("PrimaryUsagePage"),
            key_primary_usage: cfstr("PrimaryUsage"),
            key_vendor_id: cfstr("VendorID"),
            key_product_id: cfstr("ProductID"),
            key_location_id: cfstr("LocationID"),
        });

        let ctx_ptr = ptr::addr_of_mut!(*ctx).cast::<c_void>();
        IOHIDManagerSetDeviceMatching(manager, ptr::null());
        IOHIDManagerRegisterDeviceMatchingCallback(manager, on_match, ctx_ptr);
        IOHIDManagerRegisterDeviceRemovalCallback(manager, on_remove, ctx_ptr);
        IOHIDManagerRegisterInputValueCallback(manager, on_input, ctx_ptr);

        let rl = CFRunLoopGetCurrent();
        IOHIDManagerScheduleWithRunLoop(manager, rl, kCFRunLoopDefaultMode);
        let _ = IOHIDManagerOpen(manager, 0);

        let set = IOHIDManagerCopyDevices(manager);
        if !set.is_null() {
            let n = CFSetGetCount(set);
            if n > 0 {
                let mut vals: Vec<CFTypeRef> = vec![ptr::null(); n as usize];
                CFSetGetValues(set, vals.as_mut_ptr());
                for &v in &vals {
                    let dev = v as IOHIDDeviceRef;
                    if dev.is_null() {
                        continue;
                    }
                    on_match(ctx_ptr, 0, ptr::null_mut(), dev);
                }
            }
            CFRelease(set);
        }
        (ctx.emit_sys)(GpSystemEvent::StartupComplete);
        ctx.startup_complete_sent = true;

        CFRunLoopRun();

        // Keep ctx alive forever (run loop runs until process exit).
        std::mem::forget(ctx);
    }
}

#[inline(always)]
fn scale_mach_time(mach_time: u64, numer: u32, denom: u32) -> u64 {
    ((u128::from(mach_time) * u128::from(numer)) / u128::from(denom)).min(u128::from(u64::MAX))
        as u64
}
