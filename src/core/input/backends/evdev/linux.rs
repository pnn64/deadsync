use super::{GpSystemEvent, PadBackend, PadCode, PadDir, PadEvent, PadId, uuid_from_bytes};
use crate::core::host_time::{instant_nanos, now_nanos};
use log::{debug, warn};
use std::collections::HashSet;
use std::ffi::{CStr, c_char, c_void};
use std::fs;
use std::mem::{MaybeUninit, size_of};
use std::os::unix::io::AsRawFd;
use std::time::{Duration, Instant};

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

const INPUT_PATH: &[u8] = b"/dev/input\0";
const INPUT_SUBSYSTEM: &[u8] = b"input\0";
const UDEV_NETLINK: &[u8] = b"udev\0";
const ID_INPUT_JOYSTICK: &[u8] = b"ID_INPUT_JOYSTICK\0";
const VALUE_ONE: &[u8] = b"1\0";
const ACTION_ADD: &[u8] = b"add\0";
const ACTION_REMOVE: &[u8] = b"remove\0";
const INOTIFY_MASK: u32 =
    libc::IN_CREATE | libc::IN_DELETE | libc::IN_ATTRIB | libc::IN_MOVED_TO | libc::IN_MOVED_FROM;

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
    fn udev_enumerate_add_match_property(
        enumerate: *mut UdevEnumHandle,
        key: *const c_char,
        value: *const c_char,
    ) -> i32;
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
    monotonic_timestamps: bool,
    clock_health: EvdevClockHealth,
    hat_x: i32,
    hat_y: i32,
    dir: [bool; 4],
}

#[derive(Clone)]
struct DevSpec {
    path: String,
    name: String,
    vendor_id: Option<u16>,
    product_id: Option<u16>,
}

#[derive(Clone, Copy)]
struct ClockSample {
    instant: Instant,
    monotonic_nanos: u64,
}

#[derive(Default)]
struct CapabilityBits(Vec<u64>);

struct EvdevClockHealth {
    kernel_trusted: bool,
    invalid_samples: u32,
    last_event_nanos: u64,
}

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

enum Discovery {
    Udev(UdevState),
    Inotify(InotifyWatch),
    None,
}

enum HotplugEvent {
    Add(DevSpec),
    Remove(String),
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

impl EvdevClockHealth {
    #[inline(always)]
    const fn new(kernel_trusted: bool) -> Self {
        Self {
            kernel_trusted,
            invalid_samples: 0,
            last_event_nanos: 0,
        }
    }

    #[inline(always)]
    const fn uses_kernel_timestamps(&self) -> bool {
        self.kernel_trusted
    }

    #[inline(always)]
    fn note_success(&mut self, event_nanos: u64) {
        self.last_event_nanos = event_nanos;
        self.invalid_samples = self.invalid_samples.saturating_sub(1);
    }

    #[inline(always)]
    fn note_failure(&mut self, path: &str, reason: &str, event_nanos: u64, sample_nanos: u64) {
        self.invalid_samples = self.invalid_samples.saturating_add(1);
        warn!(
            "linux evdev '{}' rejected kernel timestamp ({reason}) event={} sample={} failure={}/{}",
            path, event_nanos, sample_nanos, self.invalid_samples, EVDEV_MAX_INVALID_SAMPLES
        );
        if self.invalid_samples >= EVDEV_MAX_INVALID_SAMPLES && self.kernel_trusted {
            self.kernel_trusted = false;
            warn!(
                "linux evdev '{}' disabling kernel timestamp use for the rest of the session; falling back to receipt-time timestamps.",
                path
            );
        }
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
    fn configure_for_joysticks(&self) -> bool {
        // SAFETY: `self.0` is a live enumerate handle and all string pointers are
        // static NUL-terminated byte strings.
        unsafe {
            udev_enumerate_add_match_property(
                self.0,
                ID_INPUT_JOYSTICK.as_ptr().cast(),
                VALUE_ONE.as_ptr().cast(),
            ) == 0
                && udev_enumerate_add_match_subsystem(self.0, INPUT_SUBSYSTEM.as_ptr().cast()) == 0
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
            if !dev.property_matches(ID_INPUT_JOYSTICK, VALUE_ONE) {
                continue;
            }
            if dev.action_matches(ACTION_ADD) {
                if let Some(path) = dev.devnode()
                    && let Some(spec) = dev_spec_from_event_path(&path)
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
        if !enumerate.configure_for_joysticks() {
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
            if let Some(path) = dev.devnode()
                && let Some(spec) = dev_spec_from_event_path(&path)
            {
                out.push(spec);
            }
        }
        out.sort_unstable_by(|a, b| a.path.cmp(&b.path));
        out.dedup_by(|a, b| a.path == b.path);
        out
    }
}

const EVDEV_FUTURE_TOLERANCE_NS: u64 = 5_000_000;
const EVDEV_REGRESSION_TOLERANCE_NS: u64 = 1_000_000;
const EVDEV_MAX_INVALID_SAMPLES: u32 = 4;

const IOC_NRBITS: u32 = 8;
const IOC_TYPEBITS: u32 = 8;
const IOC_SIZEBITS: u32 = 14;
const IOC_NRSHIFT: u32 = 0;
const IOC_TYPESHIFT: u32 = IOC_NRSHIFT + IOC_NRBITS;
const IOC_SIZESHIFT: u32 = IOC_TYPESHIFT + IOC_TYPEBITS;
const IOC_DIRSHIFT: u32 = IOC_SIZESHIFT + IOC_SIZEBITS;
const IOC_WRITE: u32 = 1;

const fn ioc(dir: u32, type_: u32, nr: u32, size: u32) -> libc::c_ulong {
    ((dir << IOC_DIRSHIFT)
        | (type_ << IOC_TYPESHIFT)
        | (nr << IOC_NRSHIFT)
        | (size << IOC_SIZESHIFT)) as libc::c_ulong
}

const fn iow(type_: u8, nr: u8, size: usize) -> libc::c_ulong {
    ioc(IOC_WRITE, type_ as u32, nr as u32, size as u32)
}

const EVIOCSCLOCKID: libc::c_ulong = iow(b'E', 0xA0, size_of::<libc::c_int>());

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
fn current_monotonic_nanos() -> Option<u64> {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `clock_gettime` writes into the provided stack local and
    // `CLOCK_MONOTONIC` is a valid kernel clock id.
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    if rc != 0 || ts.tv_sec < 0 || ts.tv_nsec < 0 {
        return None;
    }
    Some((ts.tv_sec as u64).saturating_mul(1_000_000_000) + ts.tv_nsec as u64)
}

#[inline(always)]
fn evdev_event_nanos(ev: InputEventRaw) -> Option<u64> {
    if ev.tv_sec < 0 || ev.tv_usec < 0 {
        return None;
    }
    Some(
        (ev.tv_sec as u64).saturating_mul(1_000_000_000)
            + (ev.tv_usec as u64).saturating_mul(1_000),
    )
}

#[inline(always)]
fn sample_monotonic_clock() -> Option<ClockSample> {
    Some(ClockSample {
        instant: Instant::now(),
        monotonic_nanos: current_monotonic_nanos()?,
    })
}

#[inline(always)]
fn instant_from_clock_sample(target_nanos: u64, sample: ClockSample) -> Instant {
    if target_nanos >= sample.monotonic_nanos {
        sample
            .instant
            .checked_add(Duration::from_nanos(
                target_nanos.saturating_sub(sample.monotonic_nanos),
            ))
            .unwrap_or(sample.instant)
    } else {
        sample
            .instant
            .checked_sub(Duration::from_nanos(
                sample.monotonic_nanos.saturating_sub(target_nanos),
            ))
            .unwrap_or(sample.instant)
    }
}

#[inline(always)]
fn event_time(dev: &mut Dev, ev: InputEventRaw, sample: Option<ClockSample>) -> (Instant, u64) {
    if dev.monotonic_timestamps
        && dev.clock_health.uses_kernel_timestamps()
        && let Some(sample) = sample
        && let Some(target_nanos) = evdev_event_nanos(ev)
    {
        if target_nanos
            > sample
                .monotonic_nanos
                .saturating_add(EVDEV_FUTURE_TOLERANCE_NS)
        {
            dev.clock_health.note_failure(
                &dev.path,
                "future",
                target_nanos,
                sample.monotonic_nanos,
            );
        } else if dev.clock_health.last_event_nanos != 0
            && target_nanos.saturating_add(EVDEV_REGRESSION_TOLERANCE_NS)
                < dev.clock_health.last_event_nanos
        {
            dev.clock_health.note_failure(
                &dev.path,
                "regression",
                target_nanos,
                sample.monotonic_nanos,
            );
        } else {
            dev.clock_health.note_success(target_nanos);
            let timestamp = instant_from_clock_sample(target_nanos, sample);
            return (timestamp, instant_nanos(timestamp));
        }
    }
    let timestamp = Instant::now();
    (timestamp, now_nanos())
}

#[inline(always)]
fn enable_monotonic_timestamps(fd: i32) -> bool {
    let mut clock_id = libc::CLOCK_MONOTONIC;
    // SAFETY: `fd` is the live evdev device descriptor and `clock_id` points to
    // writable stack storage for the ioctl.
    unsafe { libc::ioctl(fd, EVIOCSCLOCKID, &mut clock_id) == 0 }
}

#[inline(always)]
fn code_u32(type_: u16, code: u16) -> u32 {
    ((type_ as u32) << 16) | code as u32
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

fn dev_spec_from_event_path(path: &str) -> Option<DevSpec> {
    let event_name = event_name_from_path(path)?;
    let sys = format!("/sys/class/input/{event_name}/device");
    let name = read_trimmed(&format!("{sys}/name")).unwrap_or_else(|| format!("evdev:{path}"));
    Some(DevSpec {
        path: path.to_string(),
        name,
        vendor_id: read_hex_u16(&format!("{sys}/id/vendor")),
        product_id: read_hex_u16(&format!("{sys}/id/product")),
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

fn fallback_spec_from_event_path(path: &str) -> Option<DevSpec> {
    let event_name = event_name_from_path(path)?;
    let sys = format!("/sys/class/input/{event_name}/device");
    let ev = read_trimmed(&format!("{sys}/capabilities/ev"))
        .map_or_else(CapabilityBits::default, |text| CapabilityBits::parse(&text));
    let key = read_trimmed(&format!("{sys}/capabilities/key"))
        .map_or_else(CapabilityBits::default, |text| CapabilityBits::parse(&text));
    let abs = read_trimmed(&format!("{sys}/capabilities/abs"))
        .map_or_else(CapabilityBits::default, |text| CapabilityBits::parse(&text));
    looks_like_controller(&ev, &key, &abs)
        .then(|| dev_spec_from_event_path(path))
        .flatten()
}

fn scan_live_event_paths() -> HashSet<String> {
    let mut out = HashSet::new();
    if let Ok(entries) = fs::read_dir("/dev/input") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("event") {
                out.insert(entry.path().to_string_lossy().to_string());
            }
        }
    }
    out
}

fn scan_fallback_specs(open_paths: &HashSet<String>) -> Vec<DevSpec> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir("/dev/input") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if !name.to_string_lossy().starts_with("event") {
                continue;
            }
            let path = entry.path().to_string_lossy().to_string();
            if open_paths.contains(&path) {
                continue;
            }
            if let Some(spec) = fallback_spec_from_event_path(&path) {
                out.push(spec);
            }
        }
    }
    out.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    out
}

fn remove_dev_by_path(path: &str, devs: &mut Vec<Dev>, emit_sys: &mut impl FnMut(GpSystemEvent)) {
    if let Some(idx) = devs.iter().position(|dev| dev.path == path) {
        let dev = devs.swap_remove(idx);
        emit_sys(GpSystemEvent::Disconnected {
            name: dev.name,
            id: dev.id,
            backend: PadBackend::LinuxEvdev,
            initial: false,
        });
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
    let monotonic_timestamps = enable_monotonic_timestamps(file.as_raw_fd());
    if monotonic_timestamps {
        debug!(
            "linux evdev '{}' enabled CLOCK_MONOTONIC event timestamps",
            spec.path
        );
    } else {
        warn!(
            "linux evdev '{}' could not enable CLOCK_MONOTONIC event timestamps; using receipt-time fallback",
            spec.path
        );
    }
    let uuid = uuid_from_bytes(spec.path.as_bytes());
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
        uuid,
        name: spec.name,
        path: spec.path,
        file,
        monotonic_timestamps,
        clock_health: EvdevClockHealth::new(monotonic_timestamps),
        hat_x: 0,
        hat_y: 0,
        dir: [false; 4],
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

fn refresh_fallback(
    devs: &mut Vec<Dev>,
    next_id: &mut u32,
    emit_sys: &mut impl FnMut(GpSystemEvent),
) {
    let live_paths = scan_live_event_paths();
    let mut remove = Vec::new();
    for (idx, dev) in devs.iter().enumerate() {
        if !live_paths.contains(&dev.path) {
            remove.push(idx);
        }
    }
    for &idx in remove.iter().rev() {
        let dev = devs.swap_remove(idx);
        emit_sys(GpSystemEvent::Disconnected {
            name: dev.name,
            id: dev.id,
            backend: PadBackend::LinuxEvdev,
            initial: false,
        });
    }
    let open_paths = devs
        .iter()
        .map(|dev| dev.path.clone())
        .collect::<HashSet<_>>();
    for spec in scan_fallback_specs(&open_paths) {
        add_dev_if_new(spec, devs, next_id, false, emit_sys);
    }
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

pub fn run(mut emit_pad: impl FnMut(PadEvent), mut emit_sys: impl FnMut(GpSystemEvent)) {
    let discovery = init_discovery();
    let mut devs: Vec<Dev> = Vec::new();
    let mut next_id = 0u32;

    match &discovery {
        Discovery::Udev(state) => {
            for spec in state.enumerate_specs() {
                add_dev_if_new(spec, &mut devs, &mut next_id, true, &mut emit_sys);
            }
        }
        Discovery::Inotify(_) | Discovery::None => {
            for spec in scan_fallback_specs(&HashSet::new()) {
                add_dev_if_new(spec, &mut devs, &mut next_id, true, &mut emit_sys);
            }
        }
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
    let mut pollfds = Vec::with_capacity(9);

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
            let clock_sample =
                if dev.monotonic_timestamps && dev.clock_health.uses_kernel_timestamps() {
                    sample_monotonic_clock()
                } else {
                    None
                };

            for j in 0..count {
                // SAFETY: `count` is derived from the number of bytes returned by
                // `read`, so the first `count` entries of `buf` were initialized by
                // the kernel in this iteration.
                let ev = unsafe { buf[j].assume_init() };
                if ev.type_ == EV_SYN {
                    continue;
                }
                if ev.type_ == EV_KEY {
                    if ev.value == 2 {
                        continue;
                    }
                    let (timestamp, host_nanos) = event_time(dev, ev, clock_sample);
                    let pressed = ev.value != 0;
                    emit_pad(PadEvent::RawButton {
                        id: dev.id,
                        timestamp,
                        host_nanos,
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

                let (timestamp, host_nanos) = event_time(dev, ev, clock_sample);
                emit_pad(PadEvent::RawAxis {
                    id: dev.id,
                    timestamp,
                    host_nanos,
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
                let dirs = [PadDir::Up, PadDir::Down, PadDir::Left, PadDir::Right];
                for k in 0..4 {
                    if dev.dir[k] == want[k] {
                        continue;
                    }
                    dev.dir[k] = want[k];
                    emit_pad(PadEvent::Dir {
                        id: dev.id,
                        timestamp,
                        host_nanos,
                        dir: dirs[k],
                        pressed: want[k],
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

        for event in hotplug {
            match event {
                HotplugEvent::Add(spec) => {
                    add_dev_if_new(spec, &mut devs, &mut next_id, false, &mut emit_sys)
                }
                HotplugEvent::Remove(path) => remove_dev_by_path(&path, &mut devs, &mut emit_sys),
            }
        }
        if fallback_refresh {
            refresh_fallback(&mut devs, &mut next_id, &mut emit_sys);
        }
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
}
