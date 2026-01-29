use super::{GpSystemEvent, PadBackend, PadCode, PadDir, PadEvent, PadId, uuid_from_bytes};
use std::collections::HashSet;
use std::ffi::c_void;
use std::fs;
use std::mem::{MaybeUninit, size_of};
use std::os::unix::io::AsRawFd;
use std::time::Instant;

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
    file: std::fs::File,
    hat_x: i32,
    hat_y: i32,
    dir: [bool; 4],
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
            let path_s = path.to_string_lossy().to_string();
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
                file,
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
                    let timestamp = Instant::now();
                    let pressed = ev.value != 0;
                    emit_pad(PadEvent::RawButton {
                        id: dev.id,
                        timestamp,
                        code: PadCode(code_u32(ev.type_, ev.code)),
                        uuid: dev.uuid,
                        value: if pressed { 1.0 } else { 0.0 },
                        pressed,
                    });
                    continue;
                }

                if ev.type_ == EV_ABS {
                    let timestamp = Instant::now();
                    emit_pad(PadEvent::RawAxis {
                        id: dev.id,
                        timestamp,
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
                            dir: dirs[k],
                            pressed: want[k],
                        });
                    }
                }
            }
        }
    }
}
