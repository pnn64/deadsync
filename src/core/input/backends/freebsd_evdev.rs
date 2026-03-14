use super::freebsd_devd::{DevdEvent, DevdWatch};
use super::{GpSystemEvent, PadBackend, PadCode, PadDir, PadEvent, PadId, uuid_from_bytes};
use crate::core::host_time::{instant_nanos, now_nanos};
use log::{debug, warn};
use std::ffi::c_void;
use std::fs;
use std::mem::{MaybeUninit, size_of};
use std::os::unix::io::AsRawFd;
use std::time::{Duration, Instant};

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

const EV_MAX: u16 = 0x1f;
const KEY_MAX: u16 = 0x2ff;
const ABS_MAX: u16 = 0x3f;

const EV_BITS_LEN: usize = (EV_MAX as usize + 8) / 8;
const KEY_BITS_LEN: usize = (KEY_MAX as usize + 8) / 8;
const ABS_BITS_LEN: usize = (ABS_MAX as usize + 8) / 8;

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
    monotonic_timestamps: bool,
    clock_health: EvdevClockHealth,
    hat_x: i32,
    hat_y: i32,
    dir: [bool; 4],
}

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
            "freebsd evdev '{}' rejected kernel timestamp ({reason}) event={} sample={} failure={}/{}",
            path, event_nanos, sample_nanos, self.invalid_samples, EVDEV_MAX_INVALID_SAMPLES
        );
        if self.invalid_samples >= EVDEV_MAX_INVALID_SAMPLES && self.kernel_trusted {
            self.kernel_trusted = false;
            warn!(
                "freebsd evdev '{}' disabling kernel timestamp use for the rest of the session; falling back to receipt-time timestamps.",
                path
            );
        }
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

const fn iow(type_: u8, nr: u8, size: usize) -> libc::c_ulong {
    ioc(IOC_WRITE, type_ as u32, nr as u32, size as u32)
}

const EVIOCSCLOCKID: libc::c_ulong = iow(b'E', 0xA0, size_of::<libc::c_int>());
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
fn current_monotonic_nanos() -> Option<u64> {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
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
    unsafe { libc::ioctl(fd, EVIOCSCLOCKID, &mut clock_id) == 0 }
}

#[inline(always)]
fn code_u32(type_: u16, code: u16) -> u32 {
    ((type_ as u32) << 16) | (code as u32)
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
fn ioctl_read_buffer(fd: i32, request: libc::c_ulong, buf: &mut [u8]) -> bool {
    unsafe { libc::ioctl(fd, request, buf.as_mut_ptr()) >= 0 }
}

fn raw_dev_name(file: &std::fs::File) -> Option<String> {
    let mut buf = [0u8; 256];
    let rc = unsafe { libc::ioctl(file.as_raw_fd(), eviocgname(buf.len()), buf.as_mut_ptr()) };
    if rc < 0 {
        return None;
    }
    let len = buf.iter().position(|byte| *byte == 0).unwrap_or(buf.len());
    std::str::from_utf8(&buf[..len]).ok().map(str::to_owned)
}

fn raw_dev_info(file: &std::fs::File) -> (Option<u16>, Option<u16>) {
    let mut info = InputId::default();
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

fn spec_from_path(path: &str, noisy: bool) -> Option<DevSpec> {
    if !is_event_path(path) {
        return None;
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
            return None;
        }
    };
    let ev = evdev_bits(file.as_raw_fd(), 0, EV_BITS_LEN);
    let key = evdev_bits(file.as_raw_fd(), EV_KEY as u8, KEY_BITS_LEN);
    let abs = evdev_bits(file.as_raw_fd(), EV_ABS as u8, ABS_BITS_LEN);
    if !looks_like_controller(&ev, &key, &abs) {
        return None;
    }
    let name = raw_dev_name(&file).unwrap_or_else(|| format!("evdev:{path}"));
    let (vendor_id, product_id) = raw_dev_info(&file);
    Some(DevSpec {
        path: path.to_owned(),
        name,
        vendor_id,
        product_id,
    })
}

fn scan_event_specs() -> Vec<DevSpec> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/dev/input") else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path().to_string_lossy().to_string();
        if let Some(spec) = spec_from_path(&path, false) {
            out.push(spec);
        }
    }
    out.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    out
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
    let monotonic_timestamps = enable_monotonic_timestamps(file.as_raw_fd());
    if monotonic_timestamps {
        debug!(
            "freebsd evdev '{}' enabled CLOCK_MONOTONIC event timestamps",
            spec.path
        );
    } else {
        warn!(
            "freebsd evdev '{}' could not enable CLOCK_MONOTONIC event timestamps; using receipt-time fallback",
            spec.path
        );
    }
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
        monotonic_timestamps,
        clock_health: EvdevClockHealth::new(monotonic_timestamps),
        hat_x: 0,
        hat_y: 0,
        dir: [false; 4],
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
    let Some(spec) = spec_from_path(&path, !initial) else {
        return;
    };
    let id = PadId(*next_id);
    if let Some(dev) = open_dev(spec, id, initial, emit_sys) {
        *next_id = next_id.saturating_add(1);
        devs.push(dev);
    }
}

fn remove_dev_by_path(path: &str, devs: &mut Vec<Dev>, emit_sys: &mut impl FnMut(GpSystemEvent)) {
    if let Some(idx) = devs.iter().position(|dev| dev.path == path) {
        let dev = devs.swap_remove(idx);
        emit_sys(GpSystemEvent::Disconnected {
            name: dev.name,
            id: dev.id,
            backend: PadBackend::FreeBsdEvdev,
            initial: false,
        });
    }
}

pub fn run(mut emit_pad: impl FnMut(PadEvent), mut emit_sys: impl FnMut(GpSystemEvent)) {
    let watch = DevdWatch::new();
    let mut devs = Vec::new();
    let mut next_id = 0u32;
    for spec in scan_event_specs() {
        let path = spec.path.clone();
        let id = PadId(next_id);
        if let Some(dev) = open_dev(spec, id, true, &mut emit_sys) {
            next_id = next_id.saturating_add(1);
            devs.push(dev);
        } else {
            debug!("freebsd evdev skipped '{path}' during startup");
        }
    }
    emit_sys(GpSystemEvent::StartupComplete);

    let mut buf: [MaybeUninit<InputEventRaw>; 64] = [MaybeUninit::uninit(); 64];
    let buf_bytes = unsafe {
        std::slice::from_raw_parts_mut(
            buf.as_mut_ptr().cast::<u8>(),
            buf.len() * size_of::<InputEventRaw>(),
        )
    };
    let mut pollfds = Vec::with_capacity(9);

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

        let poll_ptr = if pollfds.is_empty() {
            std::ptr::null_mut()
        } else {
            pollfds.as_mut_ptr()
        };
        let rc = unsafe { poll(poll_ptr, pollfds.len(), -1) };
        if rc < 0 {
            continue;
        }

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
            let n = unsafe { read(fd, buf_bytes.as_mut_ptr().cast::<c_void>(), buf_bytes.len()) };
            if n <= 0 {
                remove.push(i);
                continue;
            }

            let count = (n as usize / size_of::<InputEventRaw>()).min(buf.len());
            let clock_sample =
                if dev.monotonic_timestamps && dev.clock_health.uses_kernel_timestamps() {
                    sample_monotonic_clock()
                } else {
                    None
                };

            for j in 0..count {
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
                backend: PadBackend::FreeBsdEvdev,
                initial: false,
            });
        }
        for event in hotplug {
            match event {
                DevdEvent::Create(path) => {
                    add_dev_if_new(path, &mut devs, &mut next_id, false, &mut emit_sys)
                }
                DevdEvent::Destroy(path) => remove_dev_by_path(&path, &mut devs, &mut emit_sys),
            }
        }
    }
}
