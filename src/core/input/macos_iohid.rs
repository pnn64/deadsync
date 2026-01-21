use super::{PadDir, uuid_from_bytes, GpSystemEvent, PadBackend, PadCode, PadEvent, PadId};
use std::collections::HashMap;
use std::ffi::{c_char, c_void};
use std::ptr;
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
unsafe extern "C" {
    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopRun();
    fn CFStringCreateWithCString(alloc: CFAllocatorRef, cstr: *const c_char, encoding: u32) -> CFStringRef;
    fn CFRelease(cf: CFTypeRef);
    fn CFSetGetCount(the_set: CFTypeRef) -> CFIndex;
    fn CFSetGetValues(the_set: CFTypeRef, values: *mut CFTypeRef);
    fn CFNumberGetValue(number: CFTypeRef, the_type: i32, value_ptr: *mut c_void) -> bool;

    static kCFRunLoopDefaultMode: CFStringRef;
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOHIDManagerCreate(alloc: CFAllocatorRef, options: u32) -> IOHIDManagerRef;
    fn IOHIDManagerSetDeviceMatching(manager: IOHIDManagerRef, matching: CFTypeRef);
    fn IOHIDManagerRegisterDeviceMatchingCallback(manager: IOHIDManagerRef, callback: extern "C" fn(*mut c_void, IOReturn, *mut c_void, IOHIDDeviceRef), context: *mut c_void);
    fn IOHIDManagerRegisterDeviceRemovalCallback(manager: IOHIDManagerRef, callback: extern "C" fn(*mut c_void, IOReturn, *mut c_void, IOHIDDeviceRef), context: *mut c_void);
    fn IOHIDManagerRegisterInputValueCallback(manager: IOHIDManagerRef, callback: extern "C" fn(*mut c_void, IOReturn, *mut c_void, IOHIDValueRef), context: *mut c_void);
    fn IOHIDManagerScheduleWithRunLoop(manager: IOHIDManagerRef, run_loop: CFRunLoopRef, mode: CFStringRef);
    fn IOHIDManagerOpen(manager: IOHIDManagerRef, options: u32) -> IOReturn;
    fn IOHIDManagerCopyDevices(manager: IOHIDManagerRef) -> CFTypeRef;

    fn IOHIDDeviceGetProperty(device: IOHIDDeviceRef, key: CFStringRef) -> CFTypeRef;
    fn IOHIDValueGetElement(value: IOHIDValueRef) -> IOHIDElementRef;
    fn IOHIDValueGetIntegerValue(value: IOHIDValueRef) -> CFIndex;
    fn IOHIDElementGetDevice(elem: IOHIDElementRef) -> IOHIDDeviceRef;
    fn IOHIDElementGetUsagePage(elem: IOHIDElementRef) -> u32;
    fn IOHIDElementGetUsage(elem: IOHIDElementRef) -> u32;
}

const KCFSTRING_ENCODING_UTF8: u32 = 0x0800_0100;

fn cfstr(s: &str) -> CFStringRef {
    let c = std::ffi::CString::new(s).unwrap();
    unsafe { CFStringCreateWithCString(ptr::null(), c.as_ptr(), KCFSTRING_ENCODING_UTF8) }
}

fn cfnum_i32(v: CFTypeRef) -> Option<i32> {
    if v.is_null() {
        return None;
    }
    let mut out: i32 = 0;
    let ok = unsafe { CFNumberGetValue(v, 9, ptr::addr_of_mut!(out).cast::<c_void>()) };
    ok.then_some(out)
}

struct Dev {
    id: PadId,
    name: String,
    uuid: [u8; 16],
    vendor_id: Option<u16>,
    product_id: Option<u16>,
    last: HashMap<u32, i64>,
    dir: [bool; 4],
}

struct Ctx {
    emit_pad: Box<dyn FnMut(PadEvent) + Send>,
    emit_sys: Box<dyn FnMut(GpSystemEvent) + Send>,
    next_id: u32,
    id_by_uuid: HashMap<[u8; 16], PadId>,
    devs: HashMap<usize, Dev>,

    // CF strings (owned; we intentionally leak ctx).
    key_primary_usage_page: CFStringRef,
    key_primary_usage: CFStringRef,
    key_product: CFStringRef,
    key_vendor_id: CFStringRef,
    key_product_id: CFStringRef,
    key_location_id: CFStringRef,
}

extern "C" fn on_match(_ctx: *mut c_void, _res: IOReturn, _sender: *mut c_void, device: IOHIDDeviceRef) {
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

        let vendor_id = cfnum_i32(IOHIDDeviceGetProperty(device, ctx.key_vendor_id)).map(|x| x as u16);
        let product_id = cfnum_i32(IOHIDDeviceGetProperty(device, ctx.key_product_id)).map(|x| x as u16);
        let location_id = cfnum_i32(IOHIDDeviceGetProperty(device, ctx.key_location_id));

        let name = format!(
            "iohid:vid={:04X} pid={:04X} loc={}",
            vendor_id.unwrap_or(0),
            product_id.unwrap_or(0),
            location_id.unwrap_or(-1),
        );
        let uuid = uuid_from_bytes(name.as_bytes());
        let id = ctx
            .id_by_uuid
            .get(&uuid)
            .copied()
            .unwrap_or_else(|| {
                let id = PadId(ctx.next_id);
                ctx.next_id += 1;
                ctx.id_by_uuid.insert(uuid, id);
                id
            });

        let dev = Dev {
            id,
            name: name.clone(),
            uuid,
            vendor_id,
            product_id,
            last: HashMap::new(),
            dir: [false; 4],
        };

        (ctx.emit_sys)(GpSystemEvent::Connected {
            name: name.clone(),
            id,
            vendor_id,
            product_id,
            backend: PadBackend::MacOsIohid,
        });

        ctx.devs.insert(key, dev);
    }
}

extern "C" fn on_remove(_ctx: *mut c_void, _res: IOReturn, _sender: *mut c_void, device: IOHIDDeviceRef) {
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
        });
    }
}

extern "C" fn on_input(_ctx: *mut c_void, _res: IOReturn, _sender: *mut c_void, value: IOHIDValueRef) {
    unsafe {
        let ctx = &mut *(_ctx as *mut Ctx);
        let timestamp = Instant::now();
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
            let want = [want_up, want_down, want_left, want_right];
            let dirs = [PadDir::Up, PadDir::Down, PadDir::Left, PadDir::Right];
            for i in 0..4 {
                if dev.dir[i] == want[i] {
                    continue;
                }
                dev.dir[i] = want[i];
                (ctx.emit_pad)(PadEvent::Dir {
                    id: dev.id,
                    timestamp,
                    dir: dirs[i],
                    pressed: want[i],
                });
            }
            return;
        }

        if usage_page == 0x09 {
            let pressed = v != 0;
            (ctx.emit_pad)(PadEvent::RawButton {
                id: dev.id,
                timestamp,
                code: PadCode(code),
                uuid: dev.uuid,
                value: if pressed { 1.0 } else { 0.0 },
                pressed,
            });
        } else {
            (ctx.emit_pad)(PadEvent::RawAxis {
                id: dev.id,
                timestamp,
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
            key_primary_usage_page: cfstr("PrimaryUsagePage"),
            key_primary_usage: cfstr("PrimaryUsage"),
            key_product: cfstr("Product"),
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

        CFRunLoopRun();

        // Keep ctx alive forever (run loop runs until process exit).
        std::mem::forget(ctx);
    }
}
