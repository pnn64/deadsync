use super::{
    GpSystemEvent, PadBackend, PadCode, PadEvent, PadId, emit_dir_edges, event_time, receipt_time,
    uuid_from_bytes,
};
use crate::engine::input::RawKeyboardEvent;
use log::{debug, warn};
use std::ffi::{CStr, c_char, c_void};
use std::fs;
use std::io::ErrorKind;
use std::mem::{MaybeUninit, size_of};
use std::os::unix::io::AsRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::platform::scancode::PhysicalKeyExtScancode;

const POLLIN: i16 = 0x0001;
const POLLERR: i16 = 0x0008;
const POLLHUP: i16 = 0x0010;
const POLLNVAL: i16 = 0x0020;

const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;
const EV_SYN: u16 = 0x00;

const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const ABS_RX: u16 = 0x03;
const ABS_RY: u16 = 0x04;
const ABS_HAT0X: u16 = 0x10;
const ABS_HAT0Y: u16 = 0x11;

const BTN_TRIGGER: u16 = 0x120;
const BTN_JOYSTICK_LAST: u16 = 0x12f;
const BTN_GAMEPAD: u16 = 0x130;
const BTN_GAMEPAD_LAST: u16 = 0x13f;
const BTN_SELECT: u16 = 0x13a;
const BTN_START: u16 = 0x13b;
const BTN_DPAD_UP: u16 = 0x220;
const BTN_DPAD_RIGHT: u16 = 0x223;
const BTN_TRIGGER_HAPPY1: u16 = 0x2c0;
const BTN_TRIGGER_HAPPY4: u16 = 0x2c3;

const KEY_ESC: u16 = 1;
const KEY_ENTER: u16 = 28;
const KEY_A: u16 = 30;
const KEY_Z: u16 = 44;
const KEY_SPACE: u16 = 57;

const INPUT_PATH: &[u8] = b"/dev/input\0";
const INPUT_SUBSYSTEM: &[u8] = b"input\0";
const UDEV_NETLINK: &[u8] = b"udev\0";
const ID_INPUT_JOYSTICK: &[u8] = b"ID_INPUT_JOYSTICK\0";
const VALUE_ONE: &[u8] = b"1\0";
const ACTION_ADD: &[u8] = b"add\0";
const ACTION_REMOVE: &[u8] = b"remove\0";
const ID_INPUT_KEYBOARD: &[u8] = b"ID_INPUT_KEYBOARD\0";
const INOTIFY_MASK: u32 =
    libc::IN_CREATE | libc::IN_DELETE | libc::IN_ATTRIB | libc::IN_MOVED_TO | libc::IN_MOVED_FROM;
const IOC_NRBITS: u32 = 8;
const IOC_TYPEBITS: u32 = 8;
const IOC_SIZEBITS: u32 = 14;
const IOC_NRSHIFT: u32 = 0;
const IOC_TYPESHIFT: u32 = IOC_NRSHIFT + IOC_NRBITS;
const IOC_SIZESHIFT: u32 = IOC_TYPESHIFT + IOC_TYPEBITS;
const IOC_DIRSHIFT: u32 = IOC_SIZESHIFT + IOC_SIZEBITS;
const IOC_WRITE: u32 = 1;

type UdevCtxHandle = c_void;
type UdevEnumHandle = c_void;
type UdevListHandle = c_void;
type UdevDeviceHandle = c_void;
type UdevMonitorHandle = c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct InputEventRaw {
    tv_sec: i64,
    tv_usec: i64,
    type_: u16,
    code: u16,
    value: i32,
}

#[link(name = "udev")]
// SAFETY: These are direct libudev FFI declarations. Callers must pass live libudev handles,
// valid NUL-terminated strings, and only dereference returned pointers according to libudev's
// ownership/lifetime rules.
unsafe extern "C" {
    fn udev_new() -> *mut UdevCtxHandle;
    fn udev_unref(udev: *mut UdevCtxHandle) -> *mut UdevCtxHandle;

    fn udev_enumerate_new(udev: *mut UdevCtxHandle) -> *mut UdevEnumHandle;
    fn udev_enumerate_unref(enumerate: *mut UdevEnumHandle) -> *mut UdevEnumHandle;
    fn udev_enumerate_add_match_subsystem(
        enumerate: *mut UdevEnumHandle,
        subsystem: *const c_char,
    ) -> i32;
    fn udev_enumerate_scan_devices(enumerate: *mut UdevEnumHandle) -> i32;
    fn udev_enumerate_get_list_entry(enumerate: *mut UdevEnumHandle) -> *mut UdevListHandle;

    fn udev_list_entry_get_next(list_entry: *mut UdevListHandle) -> *mut UdevListHandle;
    fn udev_list_entry_get_name(list_entry: *mut UdevListHandle) -> *const c_char;

    fn udev_device_new_from_syspath(
        udev: *mut UdevCtxHandle,
        syspath: *const c_char,
    ) -> *mut UdevDeviceHandle;
    fn udev_device_unref(device: *mut UdevDeviceHandle) -> *mut UdevDeviceHandle;
    fn udev_device_get_devnode(device: *mut UdevDeviceHandle) -> *const c_char;
    fn udev_device_get_action(device: *mut UdevDeviceHandle) -> *const c_char;
    fn udev_device_get_property_value(
        device: *mut UdevDeviceHandle,
        key: *const c_char,
    ) -> *const c_char;

    fn udev_monitor_new_from_netlink(
        udev: *mut UdevCtxHandle,
        name: *const c_char,
    ) -> *mut UdevMonitorHandle;
    fn udev_monitor_unref(monitor: *mut UdevMonitorHandle) -> *mut UdevMonitorHandle;
    fn udev_monitor_filter_add_match_subsystem_devtype(
        monitor: *mut UdevMonitorHandle,
        subsystem: *const c_char,
        devtype: *const c_char,
    ) -> i32;
    fn udev_monitor_enable_receiving(monitor: *mut UdevMonitorHandle) -> i32;
    fn udev_monitor_get_fd(monitor: *mut UdevMonitorHandle) -> i32;
    fn udev_monitor_receive_device(monitor: *mut UdevMonitorHandle) -> *mut UdevDeviceHandle;

    fn poll(fds: *mut PollFd, nfds: usize, timeout: i32) -> i32;
    fn read(fd: i32, buf: *mut c_void, count: usize) -> isize;
}

struct Dev {
    id: PadId,
    uuid: [u8; 16],
    name: String,
    path: String,
    file: std::fs::File,
    hat_x: i32,
    hat_y: i32,
    dir: [bool; 4],
}

struct KeyDev {
    path: String,
    file: std::fs::File,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DevClass {
    Pad,
    Keyboard,
}

#[derive(Clone)]
struct DevSpec {
    class: DevClass,
    path: String,
    name: String,
    uuid: [u8; 16],
    vendor_id: Option<u16>,
    product_id: Option<u16>,
}

#[derive(Default)]
struct FallbackScratch {
    dev_seen: Vec<bool>,
    key_seen: Vec<bool>,
    specs: Vec<DevSpec>,
}

#[derive(Default)]
struct CapabilityBits(Vec<u64>);

struct InotifyWatch {
    fd: i32,
    wd: i32,
}

struct UdevContext(*mut UdevCtxHandle);
struct UdevEnumerate(*mut UdevEnumHandle);
struct UdevDevice(*mut UdevDeviceHandle);
struct UdevMonitor(*mut UdevMonitorHandle);

struct UdevState {
    _ctx: UdevContext,
    monitor: UdevMonitor,
}

const fn iow(type_: u8, nr: u8, size: usize) -> libc::c_ulong {
    ((IOC_WRITE << IOC_DIRSHIFT)
        | ((type_ as u32) << IOC_TYPESHIFT)
        | ((nr as u32) << IOC_NRSHIFT)
        | ((size as u32) << IOC_SIZESHIFT)) as libc::c_ulong
}

const EVIOCSCLOCKID: libc::c_ulong = iow(b'E', 0xa0, size_of::<libc::c_int>());

enum Discovery {
    Udev(UdevState),
    Inotify(InotifyWatch),
    None,
}

enum HotplugEvent {
    Add(DevSpec),
    Remove(String),
}

static KEYBOARD_WINDOW_FOCUSED: AtomicBool = AtomicBool::new(true);
static KEYBOARD_CAPTURE_ENABLED: AtomicBool = AtomicBool::new(true);
static KEYBOARD_BACKEND_ACTIVE: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
struct KeyboardStartupStats {
    candidates: u32,
    opened: u32,
    permission_denied: u32,
    open_errors: u32,
}

#[inline(always)]
pub fn set_keyboard_window_focused(focused: bool) {
    KEYBOARD_WINDOW_FOCUSED.store(focused, Ordering::Relaxed);
}

#[inline(always)]
pub fn set_keyboard_capture_enabled(enabled: bool) {
    KEYBOARD_CAPTURE_ENABLED.store(enabled, Ordering::Relaxed);
}

#[inline(always)]
pub fn keyboard_backend_active() -> bool {
    KEYBOARD_BACKEND_ACTIVE.load(Ordering::Relaxed)
}

#[inline(always)]
fn keyboard_capture_active() -> bool {
    KEYBOARD_WINDOW_FOCUSED.load(Ordering::Relaxed)
        && KEYBOARD_CAPTURE_ENABLED.load(Ordering::Relaxed)
}

#[inline(always)]
fn publish_keyboard_backend_state(key_devs: &[KeyDev]) {
    KEYBOARD_BACKEND_ACTIVE.store(!key_devs.is_empty(), Ordering::Relaxed);
}

fn warn_keyboard_startup(stats: &KeyboardStartupStats) {
    if stats.candidates == 0 || stats.opened != 0 {
        return;
    }
    if stats.permission_denied == stats.candidates {
        warn!(
            "linux evdev could not open any of the {} keyboard candidates due to permissions; using focused-window keyboard fallback until /dev/input/event* is readable",
            stats.candidates
        );
        return;
    }
    if stats.permission_denied != 0 {
        warn!(
            "linux evdev could not open {} of {} keyboard candidates due to permissions",
            stats.permission_denied, stats.candidates
        );
    }
    if stats.open_errors != 0 {
        warn!(
            "linux evdev hit open errors on {} of {} keyboard candidates during startup; using focused-window keyboard fallback",
            stats.open_errors, stats.candidates
        );
    }
}

impl CapabilityBits {
    fn parse(text: &str) -> Self {
        let mut words = Vec::new();
        for chunk in text.split_whitespace().rev() {
            let Ok(word) = u64::from_str_radix(chunk, 16) else {
                return Self::default();
            };
            words.push(word);
        }
        Self(words)
    }

    #[inline(always)]
    fn has(&self, bit: u16) -> bool {
        let word = (bit as usize) >> 6;
        word < self.0.len() && ((self.0[word] >> (bit as usize & 63)) & 1) != 0
    }

    fn has_range(&self, start: u16, end: u16) -> bool {
        let mut bit = start;
        while bit <= end {
            if self.has(bit) {
                return true;
            }
            bit = bit.saturating_add(1);
        }
        false
    }
}

impl InotifyWatch {
    fn new() -> Option<Self> {
        // SAFETY: `inotify_init1` takes only flag bits and returns a new owned fd
        // or a negative errno result.
        let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if fd < 0 {
            warn!(
                "linux evdev could not create inotify fallback for /dev/input: {}",
                std::io::Error::last_os_error()
            );
            return None;
        }
        // SAFETY: `fd` is a live inotify descriptor owned by this constructor, and
        // `INPUT_PATH` is a static NUL-terminated byte string.
        let wd = unsafe { libc::inotify_add_watch(fd, INPUT_PATH.as_ptr().cast(), INOTIFY_MASK) };
        if wd < 0 {
            warn!(
                "linux evdev could not watch /dev/input for hotplug fallback: {}",
                std::io::Error::last_os_error()
            );
            // SAFETY: `fd` is still uniquely owned on this error path and must be
            // closed before returning.
            unsafe {
                libc::close(fd);
            }
            return None;
        }
        Some(Self { fd, wd })
    }

    #[inline(always)]
    const fn pollfd(&self) -> PollFd {
        PollFd {
            fd: self.fd,
            events: POLLIN,
            revents: 0,
        }
    }

    fn drain(&self) {
        let mut buf = [0u8; 1024];
        loop {
            // SAFETY: `self.fd` is the live inotify descriptor owned by this
            // watcher, and `buf` is writable stack storage for the read call.
            let n = unsafe { read(self.fd, buf.as_mut_ptr().cast(), buf.len()) };
            if n > 0 {
                continue;
            }
            if n == 0 {
                return;
            }
            let err = std::io::Error::last_os_error();
            let raw = err.raw_os_error();
            if raw == Some(libc::EAGAIN) || raw == Some(libc::EWOULDBLOCK) {
                return;
            }
            warn!("linux evdev inotify fallback read failed: {err}");
            return;
        }
    }
}

impl Drop for InotifyWatch {
    fn drop(&mut self) {
        // SAFETY: `wd` belongs to `fd`, and both are owned by this watcher. We
        // remove the watch and close the descriptor exactly once on drop.
        unsafe {
            libc::inotify_rm_watch(self.fd, self.wd);
            libc::close(self.fd);
        }
    }
}

impl UdevContext {
    fn new() -> Option<Self> {
        // SAFETY: `udev_new` returns either a new owned context pointer or null.
        let ptr = unsafe { udev_new() };
        (!ptr.is_null()).then_some(Self(ptr))
    }

    fn enumerate(&self) -> Option<UdevEnumerate> {
        // SAFETY: `self.0` is a live udev context owned by this wrapper.
        let ptr = unsafe { udev_enumerate_new(self.0) };
        (!ptr.is_null()).then_some(UdevEnumerate(ptr))
    }

    fn device_from_syspath(&self, syspath: &CStr) -> Option<UdevDevice> {
        // SAFETY: `self.0` is a live udev context and `syspath` is a valid
        // NUL-terminated C string that lives for the duration of the call.
        let ptr = unsafe { udev_device_new_from_syspath(self.0, syspath.as_ptr()) };
        (!ptr.is_null()).then_some(UdevDevice(ptr))
    }

    fn monitor(&self) -> Option<UdevMonitor> {
        // SAFETY: `self.0` is a live udev context and `UDEV_NETLINK` is a static
        // NUL-terminated string.
        let ptr = unsafe { udev_monitor_new_from_netlink(self.0, UDEV_NETLINK.as_ptr().cast()) };
        if ptr.is_null() {
            return None;
        }
        // SAFETY: `ptr` is the live monitor handle returned above, and the
        // subsystem string is a static NUL-terminated byte string.
        if unsafe {
            udev_monitor_filter_add_match_subsystem_devtype(
                ptr,
                INPUT_SUBSYSTEM.as_ptr().cast(),
                std::ptr::null(),
            )
        } != 0
        {
            // SAFETY: `ptr` is still uniquely owned on this error path and must be
            // unref'd before returning.
            unsafe {
                udev_monitor_unref(ptr);
            }
            return None;
        }
        // SAFETY: `ptr` remains a live monitor handle owned by this constructor.
        if unsafe { udev_monitor_enable_receiving(ptr) } != 0 {
            // SAFETY: `ptr` is still uniquely owned on this error path and must be
            // unref'd before returning.
            unsafe {
                udev_monitor_unref(ptr);
            }
            return None;
        }
        Some(UdevMonitor(ptr))
    }
}

impl Drop for UdevContext {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns the udev context pointer and releases it once
        // here on drop.
        unsafe {
            udev_unref(self.0);
        }
    }
}

impl UdevEnumerate {
    fn configure_for_input_subsystem(&self) -> bool {
        // SAFETY: `self.0` is a live enumerate handle and all string pointers are
        // static NUL-terminated byte strings.
        unsafe {
            udev_enumerate_add_match_subsystem(self.0, INPUT_SUBSYSTEM.as_ptr().cast()) == 0
                && udev_enumerate_scan_devices(self.0) == 0
        }
    }

    fn syspaths(&self) -> Vec<String> {
        let mut out = Vec::new();
        // SAFETY: `self.0` is a live enumerate handle, and libudev returns a linked
        // list whose nodes remain valid while the enumerate object is alive.
        let mut entry = unsafe { udev_enumerate_get_list_entry(self.0) };
        while !entry.is_null() {
            // SAFETY: `entry` is a live node from the enumerate list.
            let name = unsafe { udev_list_entry_get_name(entry) };
            if !name.is_null() {
                out.push(
                    // SAFETY: `name` was checked for null above and points to a
                    // valid NUL-terminated string owned by libudev.
                    unsafe { CStr::from_ptr(name) }
                        .to_string_lossy()
                        .into_owned(),
                );
            }
            // SAFETY: `entry` is a live node from the enumerate list.
            entry = unsafe { udev_list_entry_get_next(entry) };
        }
        out
    }
}

impl Drop for UdevEnumerate {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns the enumerate pointer and releases it once
        // here on drop.
        unsafe {
            udev_enumerate_unref(self.0);
        }
    }
}

impl UdevDevice {
    fn devnode(&self) -> Option<String> {
        // SAFETY: `self.0` is a live udev device handle and the returned pointer,
        // if non-null, remains valid while the handle is alive.
        ptr_to_string(unsafe { udev_device_get_devnode(self.0) })
    }

    fn action_matches(&self, value: &[u8]) -> bool {
        // SAFETY: `self.0` is a live udev device handle and the returned pointer,
        // if non-null, remains valid while the handle is alive.
        cstr_matches(unsafe { udev_device_get_action(self.0) }, value)
    }

    fn property_matches(&self, key: &[u8], value: &[u8]) -> bool {
        cstr_matches(
            // SAFETY: `self.0` is a live udev device handle, `key` is a valid
            // NUL-terminated byte string, and the returned pointer, if non-null,
            // remains valid while the handle is alive.
            unsafe { udev_device_get_property_value(self.0, key.as_ptr().cast()) },
            value,
        )
    }
}

impl Drop for UdevDevice {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns the device pointer and releases it once here
        // on drop.
        unsafe {
            udev_device_unref(self.0);
        }
    }
}

impl UdevMonitor {
    #[inline(always)]
    fn pollfd(&self) -> PollFd {
        PollFd {
            // SAFETY: `self.0` is a live monitor handle and libudev returns the
            // underlying pollable fd by value.
            fd: unsafe { udev_monitor_get_fd(self.0) },
            events: POLLIN,
            revents: 0,
        }
    }

    fn collect_hotplug(&self) -> Vec<HotplugEvent> {
        let mut out = Vec::new();
        loop {
            // SAFETY: `self.0` is a live monitor handle; libudev returns either a
            // newly owned device pointer or null when no event is pending.
            let ptr = unsafe { udev_monitor_receive_device(self.0) };
            if ptr.is_null() {
                break;
            }
            let dev = UdevDevice(ptr);
            if dev.action_matches(ACTION_ADD) {
                let class = if dev.property_matches(ID_INPUT_JOYSTICK, VALUE_ONE) {
                    Some(DevClass::Pad)
                } else if dev.property_matches(ID_INPUT_KEYBOARD, VALUE_ONE) {
                    Some(DevClass::Keyboard)
                } else {
                    None
                };
                if let Some(path) = dev.devnode()
                    && let Some(class) = class
                    && let Some(spec) = dev_spec_from_event_path(&path, class)
                {
                    out.push(HotplugEvent::Add(spec));
                }
                continue;
            }
            if dev.action_matches(ACTION_REMOVE)
                && let Some(path) = dev.devnode()
            {
                out.push(HotplugEvent::Remove(path));
            }
        }
        out
    }
}

impl Drop for UdevMonitor {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns the monitor pointer and releases it once here
        // on drop.
        unsafe {
            udev_monitor_unref(self.0);
        }
    }
}

impl UdevState {
    fn new() -> Option<Self> {
        let ctx = UdevContext::new()?;
        let monitor = ctx.monitor()?;
        Some(Self { _ctx: ctx, monitor })
    }

    fn enumerate_specs(&self) -> Vec<DevSpec> {
        let Some(enumerate) = self._ctx.enumerate() else {
            return Vec::new();
        };
        if !enumerate.configure_for_input_subsystem() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for syspath in enumerate.syspaths() {
            let Ok(syspath) = std::ffi::CString::new(syspath) else {
                continue;
            };
            let Some(dev) = self._ctx.device_from_syspath(syspath.as_c_str()) else {
                continue;
            };
            let class = if dev.property_matches(ID_INPUT_JOYSTICK, VALUE_ONE) {
                Some(DevClass::Pad)
            } else if dev.property_matches(ID_INPUT_KEYBOARD, VALUE_ONE) {
                Some(DevClass::Keyboard)
            } else {
                None
            };
            if let Some(path) = dev.devnode()
                && let Some(class) = class
                && let Some(spec) = dev_spec_from_event_path(&path, class)
            {
                out.push(spec);
            }
        }
        out.sort_unstable_by(|a, b| a.path.cmp(&b.path));
        out.dedup_by(|a, b| a.path == b.path);
        out
    }
}

#[inline(always)]
fn ptr_to_string(ptr: *const c_char) -> Option<String> {
    (!ptr.is_null()).then(|| {
        // SAFETY: callers only pass pointers returned by libudev, and the pointer
        // was checked for null above before converting it to `CStr`.
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned()
    })
}

#[inline(always)]
fn cstr_matches(ptr: *const c_char, expected: &[u8]) -> bool {
    // SAFETY: callers only pass pointers returned by libudev. The pointer is
    // checked for null before converting it to `CStr`.
    !ptr.is_null() && unsafe { CStr::from_ptr(ptr) }.to_bytes() == &expected[..expected.len() - 1]
}

#[inline(always)]
fn configure_evdev_clock(file: &std::fs::File) {
    let mut clock_id: libc::c_int = libc::CLOCK_MONOTONIC;
    // SAFETY: `file` is a live evdev descriptor, `EVIOCSCLOCKID` expects a
    // writable `int*`, and `CLOCK_MONOTONIC` is valid on Linux.
    let rc = unsafe { libc::ioctl(file.as_raw_fd(), EVIOCSCLOCKID, &mut clock_id) };
    if rc < 0 {
        debug!(
            "linux evdev could not switch '{}' to CLOCK_MONOTONIC timestamps",
            file.as_raw_fd()
        );
    }
}

#[inline(always)]
fn code_u32(type_: u16, code: u16) -> u32 {
    ((type_ as u32) << 16) | code as u32
}

#[inline(always)]
fn linux_key_code(code: u16) -> Option<KeyCode> {
    let PhysicalKey::Code(code) = PhysicalKey::from_scancode(u32::from(code)) else {
        return None;
    };
    Some(code)
}

#[inline(always)]
fn read_trimmed(path: &str) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let text = text.trim_matches(|ch: char| ch.is_ascii_whitespace() || ch == '\0');
    (!text.is_empty()).then(|| text.to_string())
}

#[inline(always)]
fn read_hex_u16(path: &str) -> Option<u16> {
    let text = read_trimmed(path)?;
    u16::from_str_radix(text.trim_start_matches("0x"), 16).ok()
}

#[inline(always)]
fn event_name_from_path(path: &str) -> Option<&str> {
    let name = path.rsplit('/').next()?;
    (name.starts_with("event")
        && name.len() > 5
        && name[5..].bytes().all(|byte| byte.is_ascii_digit()))
    .then_some(name)
}

fn uuid_key_from_parts(
    uniq: Option<&str>,
    phys: Option<&str>,
    name: &str,
    vendor_id: Option<u16>,
    product_id: Option<u16>,
    path: &str,
) -> String {
    if let Some(uniq) = uniq {
        return format!("linux-evdev:uniq:{uniq}");
    }
    if let Some(phys) = phys {
        return format!("linux-evdev:phys:{phys}");
    }
    if !name.is_empty() || vendor_id.is_some() || product_id.is_some() {
        return format!(
            "linux-evdev:name:{name}|vid:{:04x}|pid:{:04x}",
            vendor_id.unwrap_or(0),
            product_id.unwrap_or(0),
        );
    }
    format!("linux-evdev:path:{path}")
}

#[inline(always)]
fn uuid_from_sys_device(
    sys: &str,
    name: &str,
    vendor_id: Option<u16>,
    product_id: Option<u16>,
    path: &str,
) -> [u8; 16] {
    let uniq = read_trimmed(&format!("{sys}/uniq"));
    let phys = read_trimmed(&format!("{sys}/phys"));
    let key = uuid_key_from_parts(
        uniq.as_deref(),
        phys.as_deref(),
        name,
        vendor_id,
        product_id,
        path,
    );
    uuid_from_bytes(key.as_bytes())
}

fn dev_spec_from_event_path(path: &str, class: DevClass) -> Option<DevSpec> {
    let event_name = event_name_from_path(path)?;
    let sys = format!("/sys/class/input/{event_name}/device");
    let name = read_trimmed(&format!("{sys}/name")).unwrap_or_else(|| format!("evdev:{path}"));
    let vendor_id = read_hex_u16(&format!("{sys}/id/vendor"));
    let product_id = read_hex_u16(&format!("{sys}/id/product"));
    Some(DevSpec {
        class,
        path: path.to_string(),
        uuid: uuid_from_sys_device(&sys, &name, vendor_id, product_id, path),
        name,
        vendor_id,
        product_id,
    })
}

#[inline(always)]
fn looks_like_controller(ev: &CapabilityBits, key: &CapabilityBits, abs: &CapabilityBits) -> bool {
    if !ev.has(EV_KEY) {
        return false;
    }
    let face = key.has_range(BTN_GAMEPAD, BTN_GAMEPAD_LAST);
    let joystick = key.has_range(BTN_TRIGGER, BTN_JOYSTICK_LAST);
    let dpad = key.has_range(BTN_DPAD_UP, BTN_DPAD_RIGHT)
        || key.has_range(BTN_TRIGGER_HAPPY1, BTN_TRIGGER_HAPPY4);
    let menu = key.has(BTN_START) || key.has(BTN_SELECT);
    let sticks = abs.has(ABS_X) || abs.has(ABS_Y) || abs.has(ABS_RX) || abs.has(ABS_RY);
    let hats = abs.has(ABS_HAT0X) || abs.has(ABS_HAT0Y);
    let primaries = face || joystick || dpad || hats;
    primaries && (dpad || menu || sticks || hats)
}

#[inline(always)]
fn looks_like_keyboard(ev: &CapabilityBits, key: &CapabilityBits, abs: &CapabilityBits) -> bool {
    ev.has(EV_KEY)
        && !abs.has(ABS_X)
        && !abs.has(ABS_Y)
        && key.has(KEY_ESC)
        && key.has(KEY_ENTER)
        && key.has(KEY_A)
        && key.has(KEY_Z)
        && key.has(KEY_SPACE)
}

fn fallback_spec_from_event_path(path: &str) -> Option<DevSpec> {
    let event_name = event_name_from_path(path)?;
    let sys = format!("/sys/class/input/{event_name}/device");
    let ev = read_trimmed(&format!("{sys}/capabilities/ev"))
        .map_or_else(CapabilityBits::default, |text| CapabilityBits::parse(&text));
    let key = read_trimmed(&format!("{sys}/capabilities/key"))
        .map_or_else(CapabilityBits::default, |text| CapabilityBits::parse(&text));
    let abs = read_trimmed(&format!("{sys}/capabilities/abs"))
        .map_or_else(CapabilityBits::default, |text| CapabilityBits::parse(&text));
    let class = if looks_like_controller(&ev, &key, &abs) {
        Some(DevClass::Pad)
    } else if looks_like_keyboard(&ev, &key, &abs) {
        Some(DevClass::Keyboard)
    } else {
        None
    };
    class.and_then(|class| dev_spec_from_event_path(path, class))
}

#[inline(always)]
fn clear_seen(seen: &mut Vec<bool>, len: usize) {
    seen.resize(len, false);
    seen.fill(false);
}

fn scan_fallback(scratch: &mut FallbackScratch, devs: &[Dev], key_devs: &[KeyDev]) {
    clear_seen(&mut scratch.dev_seen, devs.len());
    clear_seen(&mut scratch.key_seen, key_devs.len());
    scratch.specs.clear();

    if let Ok(entries) = fs::read_dir("/dev/input") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if !name.to_string_lossy().starts_with("event") {
                continue;
            }
            let path_buf = entry.path();
            let path = path_buf.to_string_lossy();
            if let Some(idx) = devs.iter().position(|dev| dev.path == path.as_ref()) {
                scratch.dev_seen[idx] = true;
                continue;
            }
            if let Some(idx) = key_devs.iter().position(|dev| dev.path == path.as_ref()) {
                scratch.key_seen[idx] = true;
                continue;
            }
            let Some(spec) = fallback_spec_from_event_path(path.as_ref()) else {
                continue;
            };
            scratch.specs.push(spec);
        }
    }
    scratch.specs.sort_unstable_by(|a, b| a.path.cmp(&b.path));
}

fn remove_dev_by_path(
    path: &str,
    devs: &mut Vec<Dev>,
    key_devs: &mut Vec<KeyDev>,
    emit_sys: &mut impl FnMut(GpSystemEvent),
) {
    if let Some(idx) = devs.iter().position(|dev| dev.path == path) {
        let dev = devs.swap_remove(idx);
        emit_sys(GpSystemEvent::Disconnected {
            name: dev.name,
            id: dev.id,
            backend: PadBackend::LinuxEvdev,
            initial: false,
        });
        return;
    }
    if let Some(idx) = key_devs.iter().position(|dev| dev.path == path) {
        key_devs.swap_remove(idx);
    }
}

fn open_dev(
    spec: DevSpec,
    id: PadId,
    initial: bool,
    emit_sys: &mut impl FnMut(GpSystemEvent),
) -> Option<Dev> {
    let file = match std::fs::File::open(&spec.path) {
        Ok(file) => file,
        Err(err) => {
            warn!(
                "linux evdev could not open controller candidate '{}' at '{}': {err}",
                spec.name, spec.path
            );
            return None;
        }
    };
    configure_evdev_clock(&file);
    emit_sys(GpSystemEvent::Connected {
        name: spec.name.clone(),
        id,
        vendor_id: spec.vendor_id,
        product_id: spec.product_id,
        backend: PadBackend::LinuxEvdev,
        initial,
    });
    Some(Dev {
        id,
        uuid: spec.uuid,
        name: spec.name,
        path: spec.path,
        file,
        hat_x: 0,
        hat_y: 0,
        dir: [false; 4],
    })
}

fn open_key_dev(spec: DevSpec, mut stats: Option<&mut KeyboardStartupStats>) -> Option<KeyDev> {
    if let Some(stats) = &mut stats {
        stats.candidates = stats.candidates.saturating_add(1);
    }
    let file = match std::fs::File::open(&spec.path) {
        Ok(file) => file,
        Err(err) => {
            if let Some(stats) = &mut stats {
                if err.kind() == ErrorKind::PermissionDenied {
                    stats.permission_denied = stats.permission_denied.saturating_add(1);
                } else {
                    stats.open_errors = stats.open_errors.saturating_add(1);
                }
            }
            warn!(
                "linux evdev could not open keyboard candidate '{}' at '{}': {err}",
                spec.name, spec.path
            );
            return None;
        }
    };
    if let Some(stats) = &mut stats {
        stats.opened = stats.opened.saturating_add(1);
    }
    configure_evdev_clock(&file);
    Some(KeyDev {
        path: spec.path,
        file,
    })
}

fn add_dev_if_new(
    spec: DevSpec,
    devs: &mut Vec<Dev>,
    next_id: &mut u32,
    initial: bool,
    emit_sys: &mut impl FnMut(GpSystemEvent),
) {
    if devs.iter().any(|dev| dev.path == spec.path) {
        return;
    }
    let id = PadId(*next_id);
    if let Some(dev) = open_dev(spec, id, initial, emit_sys) {
        *next_id = next_id.saturating_add(1);
        devs.push(dev);
    }
}

fn add_key_dev_if_new(
    spec: DevSpec,
    key_devs: &mut Vec<KeyDev>,
    stats: Option<&mut KeyboardStartupStats>,
) {
    if key_devs.iter().any(|dev| dev.path == spec.path) {
        return;
    }
    if let Some(dev) = open_key_dev(spec, stats) {
        key_devs.push(dev);
        publish_keyboard_backend_state(key_devs);
    }
}

fn refresh_fallback(
    devs: &mut Vec<Dev>,
    key_devs: &mut Vec<KeyDev>,
    next_id: &mut u32,
    scratch: &mut FallbackScratch,
    scan_keyboards: bool,
    emit_sys: &mut impl FnMut(GpSystemEvent),
) {
    scan_fallback(scratch, devs, key_devs);

    for idx in (0..devs.len()).rev() {
        if scratch.dev_seen[idx] {
            continue;
        }
        let dev = devs.swap_remove(idx);
        emit_sys(GpSystemEvent::Disconnected {
            name: dev.name,
            id: dev.id,
            backend: PadBackend::LinuxEvdev,
            initial: false,
        });
    }
    for idx in (0..key_devs.len()).rev() {
        if scratch.key_seen[idx] {
            continue;
        }
        key_devs.swap_remove(idx);
    }

    publish_keyboard_backend_state(key_devs);
    for spec in scratch.specs.drain(..) {
        match spec.class {
            DevClass::Pad => add_dev_if_new(spec, devs, next_id, false, emit_sys),
            DevClass::Keyboard if scan_keyboards => add_key_dev_if_new(spec, key_devs, None),
            DevClass::Keyboard => {}
        }
    }
    publish_keyboard_backend_state(key_devs);
}

fn init_discovery() -> Discovery {
    if let Some(state) = UdevState::new() {
        return Discovery::Udev(state);
    }
    if let Some(watch) = InotifyWatch::new() {
        warn!("linux evdev falling back to inotify gamepad discovery; udev unavailable");
        return Discovery::Inotify(watch);
    }
    warn!("linux evdev has no hotplug discovery backend; startup enumeration only");
    Discovery::None
}

pub fn run(
    emit_pad: impl FnMut(PadEvent),
    emit_sys: impl FnMut(GpSystemEvent),
    emit_key: impl FnMut(RawKeyboardEvent),
) {
    run_inner(true, emit_pad, emit_sys, emit_key);
}

pub fn run_pad_only(emit_pad: impl FnMut(PadEvent), emit_sys: impl FnMut(GpSystemEvent)) {
    run_inner(false, emit_pad, emit_sys, |_| {});
}

fn run_inner(
    scan_keyboards: bool,
    mut emit_pad: impl FnMut(PadEvent),
    mut emit_sys: impl FnMut(GpSystemEvent),
    mut emit_key: impl FnMut(RawKeyboardEvent),
) {
    let discovery = init_discovery();
    let mut devs: Vec<Dev> = Vec::new();
    let mut key_devs: Vec<KeyDev> = Vec::new();
    let mut fallback = FallbackScratch::default();
    let mut keyboard_startup = KeyboardStartupStats::default();
    let mut next_id = 0u32;

    match &discovery {
        Discovery::Udev(state) => {
            for spec in state.enumerate_specs() {
                match spec.class {
                    DevClass::Pad => {
                        add_dev_if_new(spec, &mut devs, &mut next_id, true, &mut emit_sys)
                    }
                    DevClass::Keyboard if scan_keyboards => {
                        add_key_dev_if_new(spec, &mut key_devs, Some(&mut keyboard_startup))
                    }
                    DevClass::Keyboard => {}
                }
            }
        }
        Discovery::Inotify(_) | Discovery::None => {
            scan_fallback(&mut fallback, &devs, &key_devs);
            for spec in fallback.specs.drain(..) {
                match spec.class {
                    DevClass::Pad => {
                        add_dev_if_new(spec, &mut devs, &mut next_id, true, &mut emit_sys)
                    }
                    DevClass::Keyboard if scan_keyboards => {
                        add_key_dev_if_new(spec, &mut key_devs, Some(&mut keyboard_startup))
                    }
                    DevClass::Keyboard => {}
                }
            }
        }
    }
    publish_keyboard_backend_state(&key_devs);
    if scan_keyboards {
        warn_keyboard_startup(&keyboard_startup);
    }

    emit_sys(GpSystemEvent::StartupComplete);

    let mut buf: [MaybeUninit<InputEventRaw>; 64] = [MaybeUninit::uninit(); 64];
    // SAFETY: `buf` is an array of `MaybeUninit<InputEventRaw>`, so viewing its
    // backing storage as a mutable byte slice of the same size is valid for reads
    // into the uninitialized buffer.
    let buf_bytes = unsafe {
        std::slice::from_raw_parts_mut(
            buf.as_mut_ptr().cast::<u8>(),
            buf.len() * size_of::<InputEventRaw>(),
        )
    };
    let mut pollfds = Vec::with_capacity(17);

    loop {
        pollfds.clear();
        let dev_offset = match &discovery {
            Discovery::Udev(state) => {
                pollfds.push(state.monitor.pollfd());
                1usize
            }
            Discovery::Inotify(watch) => {
                pollfds.push(watch.pollfd());
                1usize
            }
            Discovery::None => 0usize,
        };
        pollfds.extend(devs.iter().map(|dev| PollFd {
            fd: dev.file.as_raw_fd(),
            events: POLLIN,
            revents: 0,
        }));
        let key_offset = dev_offset + devs.len();
        pollfds.extend(key_devs.iter().map(|dev| PollFd {
            fd: dev.file.as_raw_fd(),
            events: POLLIN,
            revents: 0,
        }));

        let poll_ptr = if pollfds.is_empty() {
            std::ptr::null_mut()
        } else {
            pollfds.as_mut_ptr()
        };
        // SAFETY: `poll_ptr` is either null for an empty set or points to the
        // first element of `pollfds`, which remains allocated for the duration of
        // the call.
        let rc = unsafe { poll(poll_ptr, pollfds.len(), -1) };
        if rc < 0 {
            continue;
        }
        let receipt = receipt_time();

        let mut hotplug = Vec::new();
        let mut fallback_refresh = false;
        if dev_offset == 1 {
            let revents = pollfds[0].revents;
            if (revents & (POLLERR | POLLHUP | POLLNVAL)) != 0 {
                warn!("linux evdev discovery fd reported poll error");
            }
            if (revents & POLLIN) != 0 {
                match &discovery {
                    Discovery::Udev(state) => hotplug = state.monitor.collect_hotplug(),
                    Discovery::Inotify(watch) => {
                        watch.drain();
                        fallback_refresh = true;
                    }
                    Discovery::None => {}
                }
            }
        }

        let mut remove = Vec::new();
        for i in 0..devs.len() {
            let revents = pollfds[i + dev_offset].revents;
            if (revents & (POLLERR | POLLHUP | POLLNVAL)) != 0 {
                remove.push(i);
                continue;
            }
            if (revents & POLLIN) == 0 {
                continue;
            }

            // SAFETY: `buf_bytes` is writable storage for `InputEventRaw` values,
            // and the fd comes from the matching open evdev device.
            let n = unsafe {
                read(
                    pollfds[i + dev_offset].fd,
                    buf_bytes.as_mut_ptr().cast(),
                    buf_bytes.len(),
                )
            };
            if n <= 0 {
                remove.push(i);
                continue;
            }

            let dev = &mut devs[i];
            let count = (n as usize / size_of::<InputEventRaw>()).min(buf.len());

            for event in buf.iter().take(count) {
                // SAFETY: `count` is derived from the number of bytes returned by
                // `read`, so the first `count` entries of `buf` were initialized by
                // the kernel in this iteration.
                let ev = unsafe { event.assume_init() };
                let (event_timestamp, event_host_nanos) =
                    event_time(receipt, ev.tv_sec, ev.tv_usec);
                if ev.type_ == EV_SYN {
                    continue;
                }
                if ev.type_ == EV_KEY {
                    if ev.value == 2 {
                        continue;
                    }
                    let pressed = ev.value != 0;
                    emit_pad(PadEvent::RawButton {
                        id: dev.id,
                        timestamp: event_timestamp,
                        host_nanos: event_host_nanos,
                        code: PadCode(code_u32(ev.type_, ev.code)),
                        uuid: dev.uuid,
                        value: if pressed { 1.0 } else { 0.0 },
                        pressed,
                    });
                    continue;
                }
                if ev.type_ != EV_ABS {
                    continue;
                }

                emit_pad(PadEvent::RawAxis {
                    id: dev.id,
                    timestamp: event_timestamp,
                    host_nanos: event_host_nanos,
                    code: PadCode(code_u32(ev.type_, ev.code)),
                    uuid: dev.uuid,
                    value: ev.value as f32,
                });

                if ev.code == ABS_HAT0X {
                    dev.hat_x = ev.value;
                } else if ev.code == ABS_HAT0Y {
                    dev.hat_y = ev.value;
                } else {
                    continue;
                }

                let want = [dev.hat_y < 0, dev.hat_y > 0, dev.hat_x < 0, dev.hat_x > 0];
                emit_dir_edges(
                    &mut emit_pad,
                    dev.id,
                    &mut dev.dir,
                    event_timestamp,
                    event_host_nanos,
                    want,
                );
            }
        }

        let mut key_remove = Vec::new();
        for i in 0..key_devs.len() {
            let revents = pollfds[i + key_offset].revents;
            if (revents & (POLLERR | POLLHUP | POLLNVAL)) != 0 {
                key_remove.push(i);
                continue;
            }
            if (revents & POLLIN) == 0 {
                continue;
            }

            // SAFETY: `buf_bytes` is writable storage for `InputEventRaw` values,
            // and the fd comes from the matching open evdev device.
            let n = unsafe {
                read(
                    pollfds[i + key_offset].fd,
                    buf_bytes.as_mut_ptr().cast(),
                    buf_bytes.len(),
                )
            };
            if n <= 0 {
                key_remove.push(i);
                continue;
            }

            let count = (n as usize / size_of::<InputEventRaw>()).min(buf.len());

            for event in buf.iter().take(count) {
                // SAFETY: `count` is derived from the number of bytes returned by
                // `read`, so the first `count` entries of `buf` were initialized by
                // the kernel in this iteration.
                let ev = unsafe { event.assume_init() };
                let (event_timestamp, event_host_nanos) =
                    event_time(receipt, ev.tv_sec, ev.tv_usec);
                if ev.type_ != EV_KEY {
                    continue;
                }
                let Some(code) = linux_key_code(ev.code) else {
                    continue;
                };
                let repeat = ev.value == 2;
                let pressed = ev.value != 0;
                if keyboard_capture_active() {
                    emit_key(RawKeyboardEvent {
                        code,
                        pressed,
                        repeat,
                        timestamp: event_timestamp,
                        host_nanos: event_host_nanos,
                    });
                }
            }
        }

        remove.sort_unstable();
        remove.dedup();
        for &idx in remove.iter().rev() {
            let dev = devs.swap_remove(idx);
            emit_sys(GpSystemEvent::Disconnected {
                name: dev.name,
                id: dev.id,
                backend: PadBackend::LinuxEvdev,
                initial: false,
            });
        }
        key_remove.sort_unstable();
        key_remove.dedup();
        for &idx in key_remove.iter().rev() {
            key_devs.swap_remove(idx);
        }
        publish_keyboard_backend_state(&key_devs);

        for event in hotplug {
            match event {
                HotplugEvent::Add(spec) => match spec.class {
                    DevClass::Pad => {
                        add_dev_if_new(spec, &mut devs, &mut next_id, false, &mut emit_sys)
                    }
                    DevClass::Keyboard if scan_keyboards => {
                        add_key_dev_if_new(spec, &mut key_devs, None)
                    }
                    DevClass::Keyboard => {}
                },
                HotplugEvent::Remove(path) => {
                    remove_dev_by_path(&path, &mut devs, &mut key_devs, &mut emit_sys);
                    publish_keyboard_backend_state(&key_devs);
                }
            }
        }
        if fallback_refresh {
            refresh_fallback(
                &mut devs,
                &mut key_devs,
                &mut next_id,
                &mut fallback,
                scan_keyboards,
                &mut emit_sys,
            );
        }
        publish_keyboard_backend_state(&key_devs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(bits: &[u16]) -> CapabilityBits {
        let words_len = bits
            .iter()
            .copied()
            .max()
            .map_or(0usize, |bit| (bit as usize >> 6) + 1);
        let mut words = vec![0u64; words_len];
        for bit in bits {
            words[*bit as usize >> 6] |= 1u64 << (*bit as usize & 63);
        }
        CapabilityBits(words)
    }

    #[test]
    fn detects_dpad_only_controller() {
        let ev = caps(&[EV_KEY]);
        let key = caps(&[BTN_DPAD_UP, BTN_DPAD_RIGHT, BTN_START]);
        let abs = CapabilityBits::default();
        assert!(looks_like_controller(&ev, &key, &abs));
    }

    #[test]
    fn uuid_key_prefers_uniq() {
        assert_eq!(
            uuid_key_from_parts(
                Some("serial-123"),
                Some("usb-0000:00:14.0-1/input0"),
                "Pad",
                Some(0x1234),
                Some(0xabcd),
                "/dev/input/event4",
            ),
            "linux-evdev:uniq:serial-123"
        );
    }

    #[test]
    fn uuid_key_falls_back_to_phys_then_ids() {
        assert_eq!(
            uuid_key_from_parts(
                None,
                Some("usb-0000:00:14.0-1/input0"),
                "Pad",
                Some(0x1234),
                Some(0xabcd),
                "/dev/input/event4",
            ),
            "linux-evdev:phys:usb-0000:00:14.0-1/input0"
        );
        assert_eq!(
            uuid_key_from_parts(
                None,
                None,
                "Pad",
                Some(0x1234),
                Some(0xabcd),
                "/dev/input/event4"
            ),
            "linux-evdev:name:Pad|vid:1234|pid:abcd"
        );
    }
}
