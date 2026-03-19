use super::{GpSystemEvent, PadBackend, PadCode, PadEvent, PadId, emit_dir_edges, uuid_from_bytes};
use crate::core::host_time::now_nanos;
use mach2::mach_time::{mach_absolute_time, mach_timebase_info, mach_timebase_info_data_t};
use std::collections::HashMap;
use std::ffi::{c_char, c_void};
use std::ptr;
use std::time::Duration;
use std::time::Instant;

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

struct Dev {
    id: PadId,
    name: String,
    uuid: [u8; 16],
    last: HashMap<u32, i64>,
    dir: [bool; 4],
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
    next_id: u32,
    id_by_uuid: HashMap<[u8; 16], PadId>,
    devs: HashMap<usize, Dev>,
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
        if ctx.devs.contains_key(&key) {
            return;
        }

        let up = cfnum_i32(IOHIDDeviceGetProperty(device, ctx.key_primary_usage_page))
            .map(|x| x as u16)
            .unwrap_or(0);
        let u = cfnum_i32(IOHIDDeviceGetProperty(device, ctx.key_primary_usage))
            .map(|x| x as u16)
            .unwrap_or(0);
        let is_controller = up == 0x01 && matches!(u, 0x04 | 0x05 | 0x08);
        if !is_controller {
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
        let id = ctx.id_by_uuid.get(&uuid).copied().unwrap_or_else(|| {
            let id = PadId(ctx.next_id);
            ctx.next_id += 1;
            ctx.id_by_uuid.insert(uuid, id);
            id
        });

        let dev = Dev {
            id,
            name: name.clone(),
            uuid,
            last: HashMap::new(),
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

        ctx.devs.insert(key, dev);
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
        let Some(dev) = ctx.devs.remove(&key) else {
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
        let Some(dev) = ctx.devs.get_mut(&(device as usize)) else {
            return;
        };
        let usage_page = IOHIDElementGetUsagePage(elem) as u16;
        let usage = IOHIDElementGetUsage(elem) as u16;
        let code = ((usage_page as u32) << 16) | (usage as u32);
        let v = IOHIDValueGetIntegerValue(value) as i64;

        if dev.last.get(&code).copied() == Some(v) {
            return;
        }
        dev.last.insert(code, v);

        // Hat switch → PadDir edges (match common HID D-pads/pads).
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
            (ctx.emit_pad)(PadEvent::RawAxis {
                id: dev.id,
                timestamp,
                host_nanos,
                code: PadCode(code),
                uuid: dev.uuid,
                value: v as f32,
            });
        }
    }
}

pub fn run(
    emit_pad: impl FnMut(PadEvent) + Send + 'static,
    emit_sys: impl FnMut(GpSystemEvent) + Send + 'static,
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
            next_id: 0,
            id_by_uuid: HashMap::new(),
            devs: HashMap::new(),
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
