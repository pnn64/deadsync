use super::{GpSystemEvent, PadBackend, PadCode, PadDir, PadEvent, PadId, uuid_from_bytes};
use crate::core::host_time::{instant_nanos, now_nanos};
use log::{debug, warn};
use std::collections::HashSet;
use std::ffi::c_void;
use std::fs;
use std::mem::{MaybeUninit, size_of};
use std::os::unix::io::AsRawFd;
use std::time::{Duration, Instant};

const POLLIN: i16 = 0x0001;

const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;
const EV_SYN: u16 = 0x00;

// EV_ABS hats (most d-pads and many dance pads expose these).
const ABS_HAT0X: u16 = 0x10;
const ABS_HAT0Y: u16 = 0x11;

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

unsafe extern "C" {
    fn poll(fds: *mut PollFd, nfds: usize, timeout: i32) -> i32;
    fn read(fd: i32, buf: *mut c_void, count: usize) -> isize;
}

struct Dev {
    id: PadId,
    uuid: [u8; 16],
    path: String,
    file: std::fs::File,
    monotonic_timestamps: bool,
    clock_health: EvdevClockHealth,
    hat_x: i32,
    hat_y: i32,
    dir: [bool; 4],
}

#[derive(Clone, Copy)]
struct ClockSample {
    instant: Instant,
    monotonic_nanos: u64,
}

struct EvdevClockHealth {
    kernel_trusted: bool,
    invalid_samples: u32,
    last_event_nanos: u64,
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

fn wanted_event_nodes() -> HashSet<String> {
    // Best-effort filter to avoid opening keyboards/mice:
    // include only `eventX` devices that also expose a `jsY` handler.
    //
    // Source: /proc/bus/input/devices, which is stable across distros and doesn't require ioctls.
    let Ok(text) = fs::read_to_string("/proc/bus/input/devices") else {
        return HashSet::new();
    };

    let mut out = HashSet::new();
    let mut has_js = false;
    let mut events: Vec<String> = Vec::new();

    for line in text.lines().chain(std::iter::once("")) {
        if line.trim().is_empty() {
            if has_js {
                out.extend(events.drain(..));
            } else {
                events.clear();
            }
            has_js = false;
            continue;
        }

        let Some(rest) = line.strip_prefix("H:") else {
            continue;
        };
        let rest = rest.trim();
        let Some(handlers) = rest.strip_prefix("Handlers=") else {
            continue;
        };
        for tok in handlers.split_whitespace() {
            if tok.starts_with("js") {
                has_js = true;
            } else if tok.starts_with("event") {
                events.push(tok.to_string());
            }
        }
    }

    out
}

pub fn run(mut emit_pad: impl FnMut(PadEvent), mut emit_sys: impl FnMut(GpSystemEvent)) {
    let mut devs: Vec<Dev> = Vec::new();

    let wanted = wanted_event_nodes();
    if let Ok(entries) = fs::read_dir("/dev/input") {
        for e in entries.flatten() {
            let name_os = e.file_name();
            let name = name_os.to_string_lossy();
            if !name.starts_with("event") {
                continue;
            }
            if !wanted.is_empty() && !wanted.contains(name.as_ref()) {
                continue;
            }
            let path = e.path();
            let Ok(file) = std::fs::File::open(&path) else {
                continue;
            };
            let monotonic_timestamps = enable_monotonic_timestamps(file.as_raw_fd());
            let path_s = path.to_string_lossy().to_string();
            if monotonic_timestamps {
                debug!("linux evdev '{path_s}' enabled CLOCK_MONOTONIC event timestamps");
            } else {
                warn!(
                    "linux evdev '{path_s}' could not enable CLOCK_MONOTONIC event timestamps; using receipt-time fallback"
                );
            }
            let uuid = uuid_from_bytes(path_s.as_bytes());
            let id = PadId(devs.len() as u32);
            let dev_name = format!("evdev:{path_s}");

            emit_sys(GpSystemEvent::Connected {
                name: dev_name.clone(),
                id,
                vendor_id: None,
                product_id: None,
                backend: PadBackend::LinuxEvdev,
                initial: true,
            });

            devs.push(Dev {
                id,
                uuid,
                path: path_s,
                file,
                monotonic_timestamps,
                clock_health: EvdevClockHealth::new(monotonic_timestamps),
                hat_x: 0,
                hat_y: 0,
                dir: [false; 4],
            });
        }
    }

    emit_sys(GpSystemEvent::StartupComplete);
    if devs.is_empty() {
        loop {
            std::thread::park();
        }
    }

    let mut pollfds: Vec<PollFd> = devs
        .iter()
        .map(|d| PollFd {
            fd: d.file.as_raw_fd(),
            events: POLLIN,
            revents: 0,
        })
        .collect();

    let mut buf: [MaybeUninit<InputEventRaw>; 64] = [MaybeUninit::uninit(); 64];
    let buf_bytes = unsafe {
        std::slice::from_raw_parts_mut(
            buf.as_mut_ptr().cast::<u8>(),
            buf.len() * size_of::<InputEventRaw>(),
        )
    };

    loop {
        for p in &mut pollfds {
            p.revents = 0;
        }

        let rc = unsafe { poll(pollfds.as_mut_ptr(), pollfds.len(), -1) };
        if rc <= 0 {
            continue;
        }

        for i in 0..pollfds.len() {
            if (pollfds[i].revents & POLLIN) == 0 {
                continue;
            }

            let fd = pollfds[i].fd;
            let dev = &mut devs[i];
            let n = unsafe { read(fd, buf_bytes.as_mut_ptr().cast::<c_void>(), buf_bytes.len()) };
            if n <= 0 {
                continue;
            }

            let count = (n as usize) / size_of::<InputEventRaw>();
            let count = count.min(buf.len());
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
                    // 0 = release, 1 = press, 2 = autorepeat
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

                if ev.type_ == EV_ABS {
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

                    let want_up = dev.hat_y < 0;
                    let want_down = dev.hat_y > 0;
                    let want_left = dev.hat_x < 0;
                    let want_right = dev.hat_x > 0;
                    let want = [want_up, want_down, want_left, want_right];
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
        }
    }
}
