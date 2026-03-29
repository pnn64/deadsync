use super::{
    DevdEvent, DevdWatch, GpSystemEvent, PadBackend, PadCode, PadEvent, PadId, emit_dir_edges,
    event_time, receipt_time, uuid_from_bytes,
};
use crate::engine::input::RawKeyboardEvent;
use log::{debug, warn};
use std::ffi::c_void;
use std::fs;
use std::mem::{MaybeUninit, size_of};
use std::os::unix::io::AsRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::platform::scancode::PhysicalKeyExtScancode;

const POLLIN: i16 = 0x0001;
const POLLERR: i16 = 0x0008;
const POLLHUP: i16 = 0x0010;
const POLLNVAL: i16 = 0x0020;

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;

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

const EV_MAX: u16 = 0x1f;
const KEY_MAX: u16 = 0x2ff;
const ABS_MAX: u16 = 0x3f;

const EV_BITS_LEN: usize = (EV_MAX as usize + 8) / 8;
const KEY_BITS_LEN: usize = (KEY_MAX as usize + 8) / 8;
const ABS_BITS_LEN: usize = (ABS_MAX as usize + 8) / 8;

const IOC_NRBITS: u32 = 8;
const IOC_TYPEBITS: u32 = 8;
const IOC_SIZEBITS: u32 = 14;
const IOC_NRSHIFT: u32 = 0;
const IOC_TYPESHIFT: u32 = IOC_NRSHIFT + IOC_NRBITS;
const IOC_SIZESHIFT: u32 = IOC_TYPESHIFT + IOC_TYPEBITS;
const IOC_DIRSHIFT: u32 = IOC_SIZESHIFT + IOC_SIZEBITS;
const IOC_READ: u32 = 2;

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

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct InputId {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
}

// SAFETY: These are direct libc FFI declarations. Callers must pass valid poll arrays, file
// descriptors, and writable/readable buffers matching the requested byte counts.
unsafe extern "C" {
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

struct DevSpec {
    class: DevClass,
    path: String,
    name: String,
    vendor_id: Option<u16>,
    product_id: Option<u16>,
}

#[derive(Default)]
struct ScanStats {
    event_nodes: u32,
    permission_denied: u32,
    open_errors: u32,
    non_input: u32,
}

enum ProbeResult {
    Skip,
    PermissionDenied,
    OpenError,
    NotInput,
    Device(DevSpec),
}

#[derive(Default)]
struct CapabilityBits(Vec<u64>);

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

impl CapabilityBits {
    fn from_bytes(bytes: &[u8]) -> Self {
        let mut words = Vec::with_capacity(bytes.len().div_ceil(8));
        for chunk in bytes.chunks(8) {
            let mut word = 0u64;
            for (shift, byte) in chunk.iter().enumerate() {
                word |= u64::from(*byte) << (shift * 8);
            }
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

const fn ioc(dir: u32, type_: u32, nr: u32, size: u32) -> libc::c_ulong {
    ((dir << IOC_DIRSHIFT)
        | (type_ << IOC_TYPESHIFT)
        | (nr << IOC_NRSHIFT)
        | (size << IOC_SIZESHIFT)) as libc::c_ulong
}

const fn ior(type_: u8, nr: u8, size: usize) -> libc::c_ulong {
    ioc(IOC_READ, type_ as u32, nr as u32, size as u32)
}

const EVIOCGID: libc::c_ulong = ior(b'E', 0x02, size_of::<InputId>());

#[inline(always)]
const fn eviocgname(len: usize) -> libc::c_ulong {
    ioc(IOC_READ, b'E' as u32, 0x06, len as u32)
}

#[inline(always)]
const fn eviocgbit(ev: u8, len: usize) -> libc::c_ulong {
    ioc(IOC_READ, b'E' as u32, 0x20 + ev as u32, len as u32)
}

#[inline(always)]
fn code_u32(type_: u16, code: u16) -> u32 {
    ((type_ as u32) << 16) | (code as u32)
}

#[inline(always)]
fn freebsd_key_code(code: u16) -> Option<KeyCode> {
    let PhysicalKey::Code(code) = PhysicalKey::from_scancode(u32::from(code)) else {
        return None;
    };
    Some(code)
}

#[inline(always)]
fn event_name_from_path(path: &str) -> Option<&str> {
    let name = path.rsplit('/').next()?;
    (name.starts_with("event")
        && name.len() > 5
        && name[5..].bytes().all(|byte| byte.is_ascii_digit()))
    .then_some(name)
}

#[inline(always)]
fn is_event_path(path: &str) -> bool {
    event_name_from_path(path).is_some()
}

#[inline(always)]
fn is_permission_error(err: &std::io::Error) -> bool {
    err.raw_os_error()
        .is_some_and(|code| code == libc::EACCES || code == libc::EPERM)
}

#[inline(always)]
fn ioctl_read_buffer(fd: i32, request: libc::c_ulong, buf: &mut [u8]) -> bool {
    // SAFETY: `fd` is a live device descriptor, `request` is an evdev read ioctl,
    // and `buf` provides writable storage for the kernel to fill.
    unsafe { libc::ioctl(fd, request, buf.as_mut_ptr()) >= 0 }
}

fn raw_dev_name(file: &std::fs::File) -> Option<String> {
    let mut buf = [0u8; 256];
    // SAFETY: `file` is an open evdev device, and `buf` is writable stack storage
    // for the kernel to fill with the NUL-terminated device name.
    let rc = unsafe { libc::ioctl(file.as_raw_fd(), eviocgname(buf.len()), buf.as_mut_ptr()) };
    if rc < 0 {
        return None;
    }
    let len = buf.iter().position(|byte| *byte == 0).unwrap_or(buf.len());
    std::str::from_utf8(&buf[..len]).ok().map(str::to_owned)
}

fn raw_dev_info(file: &std::fs::File) -> (Option<u16>, Option<u16>) {
    let mut info = InputId::default();
    // SAFETY: `file` is an open evdev device, and `info` is writable stack
    // storage for the kernel to fill.
    let rc = unsafe { libc::ioctl(file.as_raw_fd(), EVIOCGID, &mut info) };
    if rc < 0 {
        return (None, None);
    }
    (Some(info.vendor), Some(info.product))
}

fn evdev_bits(fd: i32, ev: u8, len: usize) -> CapabilityBits {
    let mut buf = vec![0u8; len];
    if !ioctl_read_buffer(fd, eviocgbit(ev, len), &mut buf) {
        return CapabilityBits::default();
    }
    CapabilityBits::from_bytes(&buf)
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
    (face || joystick) && (dpad || menu || sticks || hats)
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

fn probe_path(path: &str, noisy: bool) -> ProbeResult {
    if !is_event_path(path) {
        return ProbeResult::Skip;
    }
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(err) => {
            let msg = format!("freebsd evdev could not open candidate '{path}': {err}");
            if noisy {
                warn!("{msg}");
            } else {
                debug!("{msg}");
            }
            return if is_permission_error(&err) {
                ProbeResult::PermissionDenied
            } else {
                ProbeResult::OpenError
            };
        }
    };
    let ev = evdev_bits(file.as_raw_fd(), 0, EV_BITS_LEN);
    let key = evdev_bits(file.as_raw_fd(), EV_KEY as u8, KEY_BITS_LEN);
    let abs = evdev_bits(file.as_raw_fd(), EV_ABS as u8, ABS_BITS_LEN);
    let class = if looks_like_controller(&ev, &key, &abs) {
        Some(DevClass::Pad)
    } else if looks_like_keyboard(&ev, &key, &abs) {
        Some(DevClass::Keyboard)
    } else {
        None
    };
    let Some(class) = class else {
        return ProbeResult::NotInput;
    };
    let name = raw_dev_name(&file).unwrap_or_else(|| format!("evdev:{path}"));
    let (vendor_id, product_id) = raw_dev_info(&file);
    ProbeResult::Device(DevSpec {
        class,
        path: path.to_owned(),
        name,
        vendor_id,
        product_id,
    })
}

fn scan_event_specs() -> (Vec<DevSpec>, ScanStats) {
    let mut out = Vec::new();
    let mut stats = ScanStats::default();
    let Ok(entries) = fs::read_dir("/dev/input") else {
        return (out, stats);
    };
    for entry in entries.flatten() {
        let path = entry.path().to_string_lossy().to_string();
        match probe_path(&path, false) {
            ProbeResult::Skip => {}
            ProbeResult::PermissionDenied => {
                stats.event_nodes += 1;
                stats.permission_denied += 1;
            }
            ProbeResult::OpenError => {
                stats.event_nodes += 1;
                stats.open_errors += 1;
            }
            ProbeResult::NotInput => {
                stats.event_nodes += 1;
                stats.non_input += 1;
            }
            ProbeResult::Device(spec) => {
                stats.event_nodes += 1;
                out.push(spec);
            }
        }
    }
    out.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    (out, stats)
}

fn warn_startup_scan(stats: &ScanStats) {
    if stats.event_nodes == 0 {
        warn!("freebsd evdev found no /dev/input/event* nodes at startup");
        return;
    }
    if stats.permission_denied == stats.event_nodes {
        warn!(
            "freebsd evdev could not inspect any of the {} /dev/input/event* nodes due to permissions; grant read access to /dev/input/event* for non-root gamepad input",
            stats.event_nodes
        );
        return;
    }
    if stats.permission_denied != 0 {
        warn!(
            "freebsd evdev could not inspect {} of {} /dev/input/event* nodes due to permissions",
            stats.permission_denied, stats.event_nodes
        );
    }
    if stats.open_errors != 0 {
        warn!(
            "freebsd evdev hit open errors on {} /dev/input/event* nodes during startup",
            stats.open_errors
        );
    }
    let readable = stats
        .event_nodes
        .saturating_sub(stats.permission_denied + stats.open_errors);
    if readable != 0 {
        warn!(
            "freebsd evdev saw {} readable /dev/input/event* nodes at startup, but none looked like supported input devices",
            readable
        );
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
                "freebsd evdev could not reopen controller candidate '{}' at '{}': {err}",
                spec.name, spec.path
            );
            return None;
        }
    };
    emit_sys(GpSystemEvent::Connected {
        name: spec.name.clone(),
        id,
        vendor_id: spec.vendor_id,
        product_id: spec.product_id,
        backend: PadBackend::FreeBsdEvdev,
        initial,
    });
    Some(Dev {
        id,
        uuid: uuid_from_bytes(spec.path.as_bytes()),
        name: spec.name,
        path: spec.path,
        file,
        hat_x: 0,
        hat_y: 0,
        dir: [false; 4],
    })
}

fn open_key_dev(spec: DevSpec) -> Option<KeyDev> {
    let file = match std::fs::File::open(&spec.path) {
        Ok(file) => file,
        Err(err) => {
            warn!(
                "freebsd evdev could not reopen keyboard candidate '{}' at '{}': {err}",
                spec.name, spec.path
            );
            return None;
        }
    };
    Some(KeyDev {
        path: spec.path,
        file,
    })
}

fn add_dev_if_new(
    path: String,
    devs: &mut Vec<Dev>,
    next_id: &mut u32,
    initial: bool,
    emit_sys: &mut impl FnMut(GpSystemEvent),
) {
    if devs.iter().any(|dev| dev.path == path) {
        return;
    }
    let ProbeResult::Device(spec) = probe_path(&path, !initial) else {
        return;
    };
    if spec.class != DevClass::Pad {
        return;
    };
    let id = PadId(*next_id);
    if let Some(dev) = open_dev(spec, id, initial, emit_sys) {
        *next_id = next_id.saturating_add(1);
        devs.push(dev);
    }
}

fn add_key_dev_if_new(path: String, key_devs: &mut Vec<KeyDev>, initial: bool) {
    if key_devs.iter().any(|dev| dev.path == path) {
        return;
    }
    let ProbeResult::Device(spec) = probe_path(&path, !initial) else {
        return;
    };
    if spec.class != DevClass::Keyboard {
        return;
    }
    if let Some(dev) = open_key_dev(spec) {
        key_devs.push(dev);
    }
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
            backend: PadBackend::FreeBsdEvdev,
            initial: false,
        });
        return;
    }
    if let Some(idx) = key_devs.iter().position(|dev| dev.path == path) {
        key_devs.swap_remove(idx);
    }
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
    let watch = DevdWatch::new();
    let mut devs = Vec::new();
    let mut key_devs = Vec::new();
    let mut next_id = 0u32;
    let (startup_specs, startup_stats) = scan_event_specs();
    for spec in startup_specs {
        let path = spec.path.clone();
        match spec.class {
            DevClass::Pad => {
                let id = PadId(next_id);
                if let Some(dev) = open_dev(spec, id, true, &mut emit_sys) {
                    next_id = next_id.saturating_add(1);
                    devs.push(dev);
                } else {
                    debug!("freebsd evdev skipped '{path}' during startup");
                }
            }
            DevClass::Keyboard if scan_keyboards => {
                if let Some(dev) = open_key_dev(spec) {
                    key_devs.push(dev);
                } else {
                    debug!("freebsd evdev skipped '{path}' during startup");
                }
            }
            DevClass::Keyboard => {}
        }
    }
    if devs.is_empty() && key_devs.is_empty() {
        warn_startup_scan(&startup_stats);
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
        let watch_offset = if let Some(watch) = &watch {
            pollfds.push(PollFd {
                fd: watch.fd(),
                events: POLLIN,
                revents: 0,
            });
            1usize
        } else {
            0usize
        };
        pollfds.extend(devs.iter().map(|dev| PollFd {
            fd: dev.file.as_raw_fd(),
            events: POLLIN,
            revents: 0,
        }));
        let key_offset = watch_offset + devs.len();
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
        if watch_offset == 1 {
            let revents = pollfds[0].revents;
            if (revents & (POLLERR | POLLHUP | POLLNVAL)) != 0 {
                warn!("freebsd evdev devd watcher reported poll error");
            }
            if (revents & POLLIN) != 0
                && let Some(watch) = &watch
            {
                hotplug = watch.collect_events();
            }
        }

        let mut remove = Vec::new();
        for i in 0..devs.len() {
            let revents = pollfds[i + watch_offset].revents;
            if (revents & (POLLERR | POLLHUP | POLLNVAL)) != 0 {
                remove.push(i);
                continue;
            }
            if (revents & POLLIN) == 0 {
                continue;
            }

            let fd = pollfds[i + watch_offset].fd;
            let dev = &mut devs[i];
            // SAFETY: `buf_bytes` is writable storage for `InputEventRaw` values,
            // and the fd comes from the matching open evdev device.
            let n = unsafe { read(fd, buf_bytes.as_mut_ptr().cast::<c_void>(), buf_bytes.len()) };
            if n <= 0 {
                remove.push(i);
                continue;
            }

            let count = (n as usize / size_of::<InputEventRaw>()).min(buf.len());

            for j in 0..count {
                // SAFETY: `count` is derived from the number of bytes returned by
                // `read`, so the first `count` entries of `buf` were initialized by
                // the kernel in this iteration.
                let ev = unsafe { buf[j].assume_init() };
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

            let fd = pollfds[i + key_offset].fd;
            let dev = &mut key_devs[i];
            // SAFETY: `buf_bytes` is writable storage for `InputEventRaw` values,
            // and the fd comes from the matching open evdev device.
            let n = unsafe { read(fd, buf_bytes.as_mut_ptr().cast::<c_void>(), buf_bytes.len()) };
            if n <= 0 {
                key_remove.push(i);
                continue;
            }

            let count = (n as usize / size_of::<InputEventRaw>()).min(buf.len());

            for j in 0..count {
                // SAFETY: `count` is derived from the number of bytes returned by
                // `read`, so the first `count` entries of `buf` were initialized by
                // the kernel in this iteration.
                let ev = unsafe { buf[j].assume_init() };
                let (event_timestamp, event_host_nanos) =
                    event_time(receipt, ev.tv_sec, ev.tv_usec);
                if ev.type_ != EV_KEY {
                    continue;
                }
                let Some(code) = freebsd_key_code(ev.code) else {
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
                backend: PadBackend::FreeBsdEvdev,
                initial: false,
            });
        }
        key_remove.sort_unstable();
        key_remove.dedup();
        for &idx in key_remove.iter().rev() {
            key_devs.swap_remove(idx);
        }
        for event in hotplug {
            match event {
                DevdEvent::Create(path) => {
                    add_dev_if_new(path.clone(), &mut devs, &mut next_id, false, &mut emit_sys);
                    if scan_keyboards {
                        add_key_dev_if_new(path, &mut key_devs, false);
                    }
                }
                DevdEvent::Destroy(path) => {
                    remove_dev_by_path(&path, &mut devs, &mut key_devs, &mut emit_sys)
                }
            }
        }
    }
}
