use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Instant;

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/* ------------------------ Pad types + backend ------------------------ */

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PadId(pub u32);

impl From<PadId> for usize {
    #[inline(always)]
    fn from(value: PadId) -> Self {
        value.0 as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PadCode(pub u32);

impl PadCode {
    #[inline(always)]
    pub const fn into_u32(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadBackend {
    WindowsRawInput,
    LinuxEvdev,
    MacOsIohid,
}

#[inline(always)]
fn uuid_from_bytes(bytes: &[u8]) -> [u8; 16] {
    // Deterministic, fast, and tiny (no deps): two FNV-1a 64-bit passes with different offsets.
    const OFF0: u64 = 0xcbf29ce484222325;
    const OFF1: u64 = 0xaf63dc4c8601ec8c;
    const PRIME: u64 = 0x00000100000001b3;

    #[inline(always)]
    fn fnv64(mut h: u64, bytes: &[u8]) -> u64 {
        let mut i = 0;
        while i < bytes.len() {
            h ^= bytes[i] as u64;
            h = h.wrapping_mul(PRIME);
            i += 1;
        }
        h
    }

    let a = fnv64(OFF0, bytes);
    let b = fnv64(OFF1, bytes);
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&a.to_le_bytes());
    out[8..].copy_from_slice(&b.to_le_bytes());
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PadDir {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PadButton {
    Confirm,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FaceBtn {
    SouthA,
    EastB,
    WestX,
    NorthY,
}

#[derive(Clone, Copy, Debug)]
pub enum PadEvent {
    Dir {
        id: PadId,
        dir: PadDir,
        pressed: bool,
    },
    Button {
        id: PadId,
        btn: PadButton,
        pressed: bool,
    },
    Face {
        id: PadId,
        btn: FaceBtn,
        pressed: bool,
    },
    /// Raw low-level button event with platform-specific code and device UUID.
    RawButton {
        id: PadId,
        code: PadCode,
        uuid: [u8; 16],
        value: f32,
        pressed: bool,
    },
    /// Raw low-level axis event with platform-specific code and device UUID.
    RawAxis {
        id: PadId,
        code: PadCode,
        uuid: [u8; 16],
        value: f32,
    },
}

#[derive(Clone, Debug)]
pub enum GpSystemEvent {
    Connected {
        name: String,
        id: PadId,
        vendor_id: Option<u16>,
        product_id: Option<u16>,
        backend: PadBackend,
    },
    Disconnected {
        name: String,
        id: PadId,
        backend: PadBackend,
    },
    StartupComplete,
}

/// Run the platform pad backend on the current thread.
///
/// This is intended to be called from a dedicated thread which forwards `PadEvent` and
/// `GpSystemEvent` into the winit `EventLoopProxy` (see `deadsync/src/app.rs`).
pub fn run_pad_backend(
    emit_pad: impl FnMut(PadEvent) + Send + 'static,
    emit_sys: impl FnMut(GpSystemEvent) + Send + 'static,
) {
    #[cfg(windows)]
    return windows_raw_input::run(emit_pad, emit_sys);
    #[cfg(all(unix, not(target_os = "macos")))]
    return linux_evdev::run(emit_pad, emit_sys);
    #[cfg(target_os = "macos")]
    return macos_iohid::run(emit_pad, emit_sys);

    #[cfg(not(any(windows, unix)))]
    {
        let _ = emit_pad;
        let _ = emit_sys;
        loop {
            std::thread::park();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Lane {
    Left = 0,
    Down = 1,
    Up = 2,
    Right = 3,
    P2Left = 4,
    P2Down = 5,
    P2Up = 6,
    P2Right = 7,
}

impl Lane {
    #[inline(always)]
    pub const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputSource {
    Keyboard,
    Gamepad,
}

#[derive(Clone, Copy, Debug)]
pub struct InputEdge {
    pub lane: Lane,
    pub pressed: bool,
    pub source: InputSource,
    // Music time (seconds) at which this edge occurred, in the gameplay
    // screen's timebase (includes music rate and global offset). Filled in
    // by the gameplay code using the audio device clock.
    pub event_music_time: f32,
}

// Removed legacy per-key state helpers in favor of virtual action mapping.

/* ------------------------ Virtual Keymap system ------------------------ */

#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VirtualAction {
    p1_up,
    p1_down,
    p1_left,
    p1_right,
    p1_start,
    p1_back,
    p1_menu_up,
    p1_menu_down,
    p1_menu_left,
    p1_menu_right,
    p1_select,
    p1_operator,
    p1_restart,
    // Player 2 virtual actions (mirroring P1 for future 2P support).
    p2_up,
    p2_down,
    p2_left,
    p2_right,
    p2_start,
    p2_back,
    p2_menu_up,
    p2_menu_down,
    p2_menu_left,
    p2_menu_right,
    p2_select,
    p2_operator,
    p2_restart,
}

/// Low-level gamepad binding to a platform-specific element code.
///
/// - `code_u32` is the emitted `PadCode(u32)` (see `PadEvent::RawButton`).
/// - `device` is an optional runtime `PadId` index (from `usize::from(id)`).
/// - `uuid` is an optional per-device stable identifier (backend-derived).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GamepadCodeBinding {
    pub code_u32: u32,
    pub device: Option<usize>,
    pub uuid: Option<[u8; 16]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InputBinding {
    Key(KeyCode),
    PadDir(PadDir),
    PadButton(PadButton),
    Face(FaceBtn),
    PadDirOn { device: usize, dir: PadDir },
    PadButtonOn { device: usize, btn: PadButton },
    FaceOn { device: usize, btn: FaceBtn },
    GamepadCode(GamepadCodeBinding),
}

#[derive(Clone, Debug, Default)]
pub struct Keymap {
    map: HashMap<VirtualAction, Vec<InputBinding>>,
}

static KEYMAP: Lazy<RwLock<Keymap>> = Lazy::new(|| RwLock::new(Keymap::default()));

#[inline(always)]
fn with_keymap<R>(f: impl FnOnce(&Keymap) -> R) -> R {
    f(&KEYMAP.read().unwrap())
}

#[inline(always)]
pub fn get_keymap() -> Keymap {
    KEYMAP.read().unwrap().clone()
}

#[inline(always)]
pub fn set_keymap(new_map: Keymap) {
    *KEYMAP.write().unwrap() = new_map;
}

// Defaults are provided by config.rs; keep this module free of config.

impl Keymap {
    #[inline(always)]
    pub fn bind(&mut self, action: VirtualAction, inputs: &[InputBinding]) {
        self.map.insert(action, inputs.to_vec());
    }

    /// Returns the first keyboard key bound to this virtual action, if any.
    /// This reflects the first `KeyCode::...` token listed for the action
    /// in `deadsync.ini` (or the hardcoded default keymap).
    #[inline(always)]
    pub fn first_key_binding(&self, action: VirtualAction) -> Option<KeyCode> {
        self.map.get(&action).and_then(|bindings| {
            bindings.iter().find_map(|b| {
                if let InputBinding::Key(code) = b {
                    Some(*code)
                } else {
                    None
                }
            })
        })
    }

    /// Returns the raw binding at the given index for this virtual action,
    /// preserving the order parsed from deadsync.ini.
    #[inline(always)]
    pub fn binding_at(&self, action: VirtualAction, index: usize) -> Option<InputBinding> {
        self.map
            .get(&action)
            .and_then(|bindings| bindings.get(index))
            .copied()
    }

    #[inline(always)]
    pub fn actions_for_key_event(&self, ev: &KeyEvent) -> Vec<(VirtualAction, bool)> {
        let mut out = Vec::with_capacity(2);
        let pressed = ev.state == ElementState::Pressed;
        let PhysicalKey::Code(code) = ev.physical_key else {
            return out;
        };
        for (act, binds) in &self.map {
            for b in binds {
                if *b == InputBinding::Key(code) {
                    out.push((*act, pressed));
                    break;
                }
            }
        }
        out
    }

    #[inline(always)]
    pub fn actions_for_pad_event(&self, ev: &PadEvent) -> Vec<(VirtualAction, bool)> {
        let mut out = Vec::with_capacity(2);
        match *ev {
            PadEvent::Dir { id, dir, pressed } => {
                let dev = usize::from(id);
                for (act, binds) in &self.map {
                    for b in binds {
                        match *b {
                            InputBinding::PadDir(d) if d == dir => {
                                out.push((*act, pressed));
                                break;
                            }
                            InputBinding::PadDirOn { device, dir: d }
                                if d == dir && device == dev =>
                            {
                                out.push((*act, pressed));
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
            PadEvent::Button { id, btn, pressed } => {
                let dev = usize::from(id);
                for (act, binds) in &self.map {
                    for b in binds {
                        match *b {
                            InputBinding::PadButton(b0) if b0 == btn => {
                                out.push((*act, pressed));
                                break;
                            }
                            InputBinding::PadButtonOn { device, btn: b0 }
                                if b0 == btn && device == dev =>
                            {
                                out.push((*act, pressed));
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
            PadEvent::Face { id, btn, pressed } => {
                let dev = usize::from(id);
                for (act, binds) in &self.map {
                    for b in binds {
                        match *b {
                            InputBinding::Face(b0) if b0 == btn => {
                                out.push((*act, pressed));
                                break;
                            }
                            InputBinding::FaceOn { device, btn: b0 }
                                if b0 == btn && device == dev =>
                            {
                                out.push((*act, pressed));
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
            PadEvent::RawButton {
                id,
                code,
                uuid,
                pressed,
                ..
            } => {
                let dev = usize::from(id);
                let code_u32 = code.into_u32();
                for (act, binds) in &self.map {
                    for b in binds {
                        match *b {
                            InputBinding::GamepadCode(binding) => {
                                if binding.code_u32 != code_u32 {
                                    continue;
                                }
                                if let Some(d_expected) = binding.device {
                                    if d_expected != dev {
                                        continue;
                                    }
                                }
                                if let Some(u_expected) = binding.uuid {
                                    if u_expected != uuid {
                                        continue;
                                    }
                                }
                                out.push((*act, pressed));
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
            PadEvent::RawAxis { .. } => {
                // Axis events are exposed for debugging but are not yet
                // mapped directly to virtual actions.
            }
        }
        out
    }
}

// INI parsing and default emission moved to config.rs

/* ------------------------- Normalized input events ------------------------- */

#[derive(Clone, Copy, Debug)]
pub struct InputEvent {
    pub action: VirtualAction,
    pub pressed: bool,
    pub source: InputSource,
    pub timestamp: Instant,
}

#[inline(always)]
pub fn map_key_event(ev: &KeyEvent) -> Vec<InputEvent> {
    let mut out = Vec::with_capacity(2);
    // Ignore OS key auto-repeat for pressed events (prevents resetting hold timers)
    if ev.state == ElementState::Pressed && ev.repeat {
        return out;
    }
    let mut actions = with_keymap(|km| km.actions_for_key_event(ev));
    dedup_menu_variants(&mut actions);
    if actions.is_empty() {
        return out;
    }
    let timestamp = Instant::now();
    for (act, pressed) in actions {
        out.push(InputEvent {
            action: act,
            pressed,
            source: InputSource::Keyboard,
            timestamp,
        });
    }
    out
}

#[inline(always)]
pub fn map_pad_event(ev: &PadEvent) -> Vec<InputEvent> {
    let mut out = Vec::with_capacity(2);
    let mut actions = with_keymap(|km| km.actions_for_pad_event(ev));
    dedup_menu_variants(&mut actions);
    if actions.is_empty() {
        return out;
    }
    let timestamp = Instant::now();
    for (act, pressed) in actions {
        out.push(InputEvent {
            action: act,
            pressed,
            source: InputSource::Gamepad,
            timestamp,
        });
    }
    out
}

#[inline(always)]
pub fn lane_from_action(act: VirtualAction) -> Option<Lane> {
    match act {
        VirtualAction::p1_left => Some(Lane::Left),
        VirtualAction::p1_down => Some(Lane::Down),
        VirtualAction::p1_up => Some(Lane::Up),
        VirtualAction::p1_right => Some(Lane::Right),
        VirtualAction::p2_left => Some(Lane::P2Left),
        VirtualAction::p2_down => Some(Lane::P2Down),
        VirtualAction::p2_up => Some(Lane::P2Up),
        VirtualAction::p2_right => Some(Lane::P2Right),
        _ => None,
    }
}

#[inline(always)]
fn dedup_menu_variants(actions: &mut Vec<(VirtualAction, bool)>) {
    use VirtualAction as A;
    // If both menu and non-menu variants for the same direction are present with the same
    // pressed state, drop the menu variant to avoid double-triggering navigation.
    let mut p1 = [[false; 2]; 4];
    let mut p2 = [[false; 2]; 4];

    for (act, pressed) in actions.iter() {
        let idx = usize::from(*pressed);
        match *act {
            A::p1_up => p1[0][idx] = true,
            A::p1_down => p1[1][idx] = true,
            A::p1_left => p1[2][idx] = true,
            A::p1_right => p1[3][idx] = true,
            A::p2_up => p2[0][idx] = true,
            A::p2_down => p2[1][idx] = true,
            A::p2_left => p2[2][idx] = true,
            A::p2_right => p2[3][idx] = true,
            _ => {}
        }
    }

    actions.retain(|(act, pressed)| {
        let idx = usize::from(*pressed);
        match *act {
            A::p1_menu_up => !p1[0][idx],
            A::p1_menu_down => !p1[1][idx],
            A::p1_menu_left => !p1[2][idx],
            A::p1_menu_right => !p1[3][idx],
            A::p2_menu_up => !p2[0][idx],
            A::p2_menu_down => !p2[1][idx],
            A::p2_menu_left => !p2[2][idx],
            A::p2_menu_right => !p2[3][idx],
            _ => true,
        }
    });
}

/* ------------------------ Platform pad backends ------------------------ */

#[cfg(all(unix, not(target_os = "macos")))]
mod linux_evdev {
    use super::{PadDir, uuid_from_bytes, GpSystemEvent, PadBackend, PadCode, PadEvent, PadId};
    use std::collections::HashSet;
    use std::ffi::c_void;
    use std::fs;
    use std::mem::{MaybeUninit, size_of};
    use std::os::unix::io::AsRawFd;

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

    pub fn run(
        mut emit_pad: impl FnMut(PadEvent),
        mut emit_sys: impl FnMut(GpSystemEvent),
    ) {
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
                        let pressed = ev.value != 0;
                        emit_pad(PadEvent::RawButton {
                            id: dev.id,
                            code: PadCode(code_u32(ev.type_, ev.code)),
                            uuid: dev.uuid,
                            value: if pressed { 1.0 } else { 0.0 },
                            pressed,
                        });
                        continue;
                    }

                    if ev.type_ == EV_ABS {
                        emit_pad(PadEvent::RawAxis {
                            id: dev.id,
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
                                dir: dirs[k],
                                pressed: want[k],
                            });
                        }
                    }
                }
            }
        }
    }
}

#[cfg(windows)]
mod windows_raw_input {
    use super::{PadDir, uuid_from_bytes, GpSystemEvent, PadBackend, PadCode, PadEvent, PadId};
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::ptr;

    use windows::core::PCWSTR;
    use windows::Win32::Devices::HumanInterfaceDevice::*;
    use windows::Win32::Foundation::*;
    use windows::Win32::System::LibraryLoader::*;
    use windows::Win32::UI::Input::*;
    use windows::Win32::UI::WindowsAndMessaging::*;

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
        fn emit_connected(&mut self, dev: &Dev) {
            (self.emit_sys)(GpSystemEvent::Connected {
                name: dev.name.clone(),
                id: dev.id,
                vendor_id: dev.vendor_id,
                product_id: dev.product_id,
                backend: PadBackend::WindowsRawInput,
            });
        }

        #[inline(always)]
        fn emit_disconnected(&mut self, dev: &Dev) {
            (self.emit_sys)(GpSystemEvent::Disconnected {
                name: dev.name.clone(),
                id: dev.id,
                backend: PadBackend::WindowsRawInput,
            });
        }
    }

    #[inline(always)]
    fn is_controller_usage(usage_page: u16, usage: u16) -> bool {
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
            let _ = GetRawInputDeviceInfoW(Some(h), RIDI_DEVICENAME, None, &mut size);
            if size == 0 {
                return None;
            }
            let mut buf: Vec<u16> = vec![0u16; size as usize];
            let mut size2 = size;
            let rc = GetRawInputDeviceInfoW(
                Some(h),
                RIDI_DEVICENAME,
                Some(buf.as_mut_ptr().cast::<c_void>()),
                &mut size2,
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
                &mut size,
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
            let _ = GetRawInputDeviceInfoW(Some(h), RIDI_PREPARSEDDATA, None, &mut size);
            if size == 0 {
                return None;
            }
            let mut buf = vec![0u8; size as usize];
            let mut size2 = size;
            let rc = GetRawInputDeviceInfoW(
                Some(h),
                RIDI_PREPARSEDDATA,
                Some(buf.as_mut_ptr().cast::<c_void>()),
                &mut size2,
            );
            if rc == u32::MAX {
                return None;
            }
            Some(buf)
        }
    }

    fn add_device(ctx: &mut Ctx, h: HANDLE) {
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

        let name = get_device_name(h).unwrap_or_else(|| format!("RawInput:{:?}", h));
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

        let refs = ctx.refs_by_uuid.get(&uuid).copied().unwrap_or(0);
        ctx.refs_by_uuid.insert(uuid, refs + 1);

        let preparsed = get_preparsed(h).unwrap_or_default();
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
            dir: [false; 4],
        };

        if refs == 0 {
            ctx.emit_connected(&dev);
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
            ctx.emit_disconnected(&dev);
        } else {
            ctx.refs_by_uuid.insert(dev.uuid, refs - 1);
        }
    }

    fn enumerate_existing(ctx: &mut Ctx) {
        unsafe {
            let mut count: u32 = 0;
            let _ = GetRawInputDeviceList(None, &mut count, size_of::<RAWINPUTDEVICELIST>() as u32);
            if count == 0 {
                return;
            }
            let mut list = vec![RAWINPUTDEVICELIST::default(); count as usize];
            let rc = GetRawInputDeviceList(
                Some(list.as_mut_ptr()),
                &mut count,
                size_of::<RAWINPUTDEVICELIST>() as u32,
            );
            if rc == u32::MAX {
                return;
            }
            for item in list {
                if item.dwType != RIM_TYPEHID {
                    continue;
                }
                add_device(ctx, item.hDevice);
            }
        }
    }

    #[inline(always)]
    fn emit_button_diff(
        emit_pad: &mut (dyn FnMut(PadEvent) + Send),
        dev: &Dev,
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
                    code: PadCode(((USAGE_PAGE_BUTTON as u32) << 16) | (pa as u32)),
                    uuid: dev.uuid,
                    value: 0.0,
                    pressed: false,
                });
                a += 1;
            } else {
                // Pressed.
                (emit_pad)(PadEvent::RawButton {
                    id: dev.id,
                    code: PadCode(((USAGE_PAGE_BUTTON as u32) << 16) | (nb as u32)),
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
                code: PadCode(((USAGE_PAGE_BUTTON as u32) << 16) | (u as u32)),
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
                code: PadCode(((USAGE_PAGE_BUTTON as u32) << 16) | (u as u32)),
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
        report: &mut [u8],
    ) {
        if dev.max_buttons == 0 || dev.preparsed.is_empty() {
            return;
        }

        let want_cap = dev.max_buttons as usize;
        if dev.buttons_now.capacity() < want_cap {
            dev.buttons_now.reserve(want_cap - dev.buttons_now.capacity());
        }
        dev.buttons_now.clear();

        let mut len = dev.max_buttons;
        let status = unsafe {
            HidP_GetUsages(
                HidP_Input,
                USAGE_PAGE_BUTTON,
                None,
                dev.buttons_now.as_mut_ptr(),
                &mut len,
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

        emit_button_diff(emit_pad, dev, &dev.buttons_now);
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
                &mut hat,
                PHIDP_PREPARSED_DATA(dev.preparsed.as_ptr() as isize),
                report,
            )
        };
        if status != HIDP_STATUS_SUCCESS {
            return;
        }

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
            (emit_pad)(PadEvent::Dir {
                id: dev.id,
                dir: dirs[i],
                pressed: want[i],
            });
        }
    }

    fn handle_wm_input(ctx: &mut Ctx, hraw: HRAWINPUT) {
        unsafe {
            let mut size: u32 = 0;
            let _ = GetRawInputData(hraw, RID_INPUT, None, &mut size, size_of::<RAWINPUTHEADER>() as u32);
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
                &mut size2,
                size_of::<RAWINPUTHEADER>() as u32,
            );
            if rc == u32::MAX {
                return;
            }

            // Parse header unaligned; buffer alignment is not guaranteed.
            if (size2 as usize) < size_of::<RAWINPUTHEADER>() {
                return;
            }
            let header: RAWINPUTHEADER =
                ptr::read_unaligned(ctx.buf.as_ptr().cast::<RAWINPUTHEADER>());
            if header.dwType != RIM_TYPEHID_U32 {
                return;
            }

            let dev_handle = header.hDevice;
            if !ctx.devices.contains_key(&hkey(dev_handle)) {
                add_device(ctx, dev_handle);
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
                process_hid_report(ctx.emit_pad.as_mut(), dev, report);
                idx += 1;
            }
        }
    }

    unsafe extern "system" fn wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let ctx_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut Ctx;
        match msg {
            WM_CREATE => {
                let cs = lparam.0 as *const CREATESTRUCTW;
                if !cs.is_null() {
                    let p = unsafe { (*cs).lpCreateParams } as *mut Ctx;
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
                    match wparam.0 as usize {
                        GIDC_ARRIVAL_U32 => add_device(ctx, h),
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
            RegisterClassExW(&wc);

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
                Some(ptr::addr_of_mut!(*ctx).cast::<c_void>() as *const c_void),
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
                let ok = GetMessageW(&mut msg, None, 0, 0);
                if ok.0 <= 0 {
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            // Keep ctx alive forever (message loop runs until process exit).
            std::mem::forget(ctx);
        }
    }
}

#[cfg(target_os = "macos")]
mod macos_iohid {
    use super::{PadDir, uuid_from_bytes, GpSystemEvent, PadBackend, PadCode, PadEvent, PadId};
    use std::collections::HashMap;
    use std::ffi::{c_char, c_void};
    use std::ptr;

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
                    code: PadCode(code),
                    uuid: dev.uuid,
                    value: if pressed { 1.0 } else { 0.0 },
                    pressed,
                });
            } else {
                (ctx.emit_pad)(PadEvent::RawAxis {
                    id: dev.id,
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
}
