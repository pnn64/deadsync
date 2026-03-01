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
        value.0 as Self
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
    #[cfg(windows)]
    WindowsRawInput,
    #[cfg(windows)]
    WindowsWgi,
    #[cfg(all(unix, not(target_os = "macos")))]
    LinuxEvdev,
    #[cfg(target_os = "macos")]
    MacOsIohid,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowsPadBackend {
    /// Choose the default Windows backend (currently WGI).
    #[default]
    Auto,
    RawInput,
    Wgi,
}

impl WindowsPadBackend {
    #[inline(always)]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::RawInput => "RawInput",
            Self::Wgi => "WGI",
        }
    }
}

impl std::fmt::Display for WindowsPadBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for WindowsPadBackend {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("auto") {
            return Ok(Self::Auto);
        }
        if s.eq_ignore_ascii_case("rawinput")
            || s.eq_ignore_ascii_case("raw_input")
            || s.eq_ignore_ascii_case("raw")
        {
            return Ok(Self::RawInput);
        }
        if s.eq_ignore_ascii_case("wgi")
            || s.eq_ignore_ascii_case("windowsgaminginput")
            || s.eq_ignore_ascii_case("gaminginput")
        {
            return Ok(Self::Wgi);
        }
        Err(())
    }
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
            h ^= u64::from(bytes[i]);
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

impl PadDir {
    #[inline(always)]
    pub const fn ix(self) -> usize {
        match self {
            Self::Up => 0,
            Self::Down => 1,
            Self::Left => 2,
            Self::Right => 3,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum PadEvent {
    Dir {
        id: PadId,
        timestamp: Instant,
        dir: PadDir,
        pressed: bool,
    },
    /// Raw low-level button event with platform-specific code and device UUID.
    RawButton {
        id: PadId,
        timestamp: Instant,
        code: PadCode,
        uuid: [u8; 16],
        value: f32,
        pressed: bool,
    },
    /// Raw low-level axis event with platform-specific code and device UUID.
    #[cfg_attr(windows, allow(dead_code))]
    RawAxis {
        id: PadId,
        timestamp: Instant,
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
        /// True when this connection is part of startup enumeration (no hotplug overlay).
        initial: bool,
    },
    #[cfg_attr(all(unix, not(target_os = "macos")), allow(dead_code))]
    Disconnected {
        name: String,
        id: PadId,
        backend: PadBackend,
        /// True when this disconnect is part of startup enumeration (no hotplug overlay).
        initial: bool,
    },
    StartupComplete,
}

/// Run the platform pad backend on the current thread.
///
/// This is intended to be called from a dedicated thread which forwards `PadEvent` and
/// `GpSystemEvent` into the winit `EventLoopProxy` (see `deadsync/src/app.rs`).
pub fn run_pad_backend(
    win_backend: WindowsPadBackend,
    emit_pad: impl FnMut(PadEvent) + Send + 'static,
    emit_sys: impl FnMut(GpSystemEvent) + Send + 'static,
) {
    #[cfg(not(windows))]
    let _ = win_backend;

    #[cfg(windows)]
    match win_backend {
        WindowsPadBackend::RawInput => windows_raw_input::run(emit_pad, emit_sys),
        WindowsPadBackend::Auto | WindowsPadBackend::Wgi => windows_wgi::run(emit_pad, emit_sys),
    }
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
#[repr(u8)]
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

impl VirtualAction {
    #[inline(always)]
    pub const fn ix(self) -> usize {
        self as usize
    }

    /// Returns `true` for gameplay arrow actions (p1/p2 up/down/left/right).
    #[inline(always)]
    pub const fn is_gameplay_arrow(self) -> bool {
        matches!(
            self,
            Self::p1_up
                | Self::p1_down
                | Self::p1_left
                | Self::p1_right
                | Self::p2_up
                | Self::p2_down
                | Self::p2_left
                | Self::p2_right
        )
    }
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
    PadDirOn { device: usize, dir: PadDir },
    GamepadCode(GamepadCodeBinding),
}

#[derive(Clone, Copy, Debug)]
struct PadCodeRev {
    act: VirtualAction,
    device: Option<usize>,
    uuid: Option<[u8; 16]>,
}

#[derive(Clone, Debug)]
pub struct Keymap {
    map: HashMap<VirtualAction, Vec<InputBinding>>,
    key_rev: HashMap<KeyCode, Vec<VirtualAction>>,
    pad_dir_rev: [Vec<VirtualAction>; 4],
    pad_dir_on_rev: HashMap<(usize, PadDir), Vec<VirtualAction>>,
    pad_code_rev: HashMap<u32, Vec<PadCodeRev>>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self {
            map: HashMap::new(),
            key_rev: HashMap::new(),
            pad_dir_rev: std::array::from_fn(|_| Vec::new()),
            pad_dir_on_rev: HashMap::new(),
            pad_code_rev: HashMap::new(),
        }
    }
}

static KEYMAP: std::sync::LazyLock<RwLock<Keymap>> =
    std::sync::LazyLock::new(|| RwLock::new(Keymap::default()));

#[inline(always)]
pub fn with_keymap<R>(f: impl FnOnce(&Keymap) -> R) -> R {
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
    fn remove_rev(&mut self, action: VirtualAction, prev: &[InputBinding]) {
        for b in prev {
            match *b {
                InputBinding::Key(code) => {
                    if let Some(v) = self.key_rev.get_mut(&code) {
                        v.retain(|a| *a != action);
                        if v.is_empty() {
                            self.key_rev.remove(&code);
                        }
                    }
                }
                InputBinding::PadDir(dir) => {
                    self.pad_dir_rev[dir.ix()].retain(|a| *a != action);
                }
                InputBinding::PadDirOn { device, dir } => {
                    let key = (device, dir);
                    if let Some(v) = self.pad_dir_on_rev.get_mut(&key) {
                        v.retain(|a| *a != action);
                        if v.is_empty() {
                            self.pad_dir_on_rev.remove(&key);
                        }
                    }
                }
                InputBinding::GamepadCode(binding) => {
                    if let Some(v) = self.pad_code_rev.get_mut(&binding.code_u32) {
                        v.retain(|e| {
                            e.act != action || e.device != binding.device || e.uuid != binding.uuid
                        });
                        if v.is_empty() {
                            self.pad_code_rev.remove(&binding.code_u32);
                        }
                    }
                }
            }
        }
    }

    #[inline(always)]
    fn add_rev(&mut self, action: VirtualAction, inputs: &[InputBinding]) {
        for b in inputs {
            match *b {
                InputBinding::Key(code) => self.key_rev.entry(code).or_default().push(action),
                InputBinding::PadDir(dir) => self.pad_dir_rev[dir.ix()].push(action),
                InputBinding::PadDirOn { device, dir } => self
                    .pad_dir_on_rev
                    .entry((device, dir))
                    .or_default()
                    .push(action),
                InputBinding::GamepadCode(binding) => self
                    .pad_code_rev
                    .entry(binding.code_u32)
                    .or_default()
                    .push(PadCodeRev {
                        act: action,
                        device: binding.device,
                        uuid: binding.uuid,
                    }),
            }
        }
    }

    #[inline(always)]
    pub fn bind(&mut self, action: VirtualAction, inputs: &[InputBinding]) {
        if let Some(prev) = self.map.remove(&action) {
            self.remove_rev(action, &prev);
        }
        self.map.insert(action, inputs.to_vec());
        self.add_rev(action, inputs);
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
        let pressed = ev.state == ElementState::Pressed;
        let PhysicalKey::Code(code) = ev.physical_key else {
            return Vec::new();
        };
        let Some(actions) = self.key_rev.get(&code) else {
            return Vec::new();
        };

        let mut out = Vec::with_capacity(actions.len());
        let mut seen: u32 = 0;
        for &act in actions {
            let bit = 1u32 << (act.ix() as u32);
            if (seen & bit) != 0 {
                continue;
            }
            seen |= bit;
            out.push((act, pressed));
        }
        out
    }

    #[inline(always)]
    pub fn actions_for_pad_event(&self, ev: &PadEvent) -> Vec<(VirtualAction, bool)> {
        match *ev {
            PadEvent::Dir {
                id, dir, pressed, ..
            } => {
                let dev = usize::from(id);
                let any = &self.pad_dir_rev[dir.ix()];
                let on = self.pad_dir_on_rev.get(&(dev, dir));
                if any.is_empty() && on.is_none() {
                    return Vec::new();
                }

                let mut out = Vec::with_capacity(any.len() + on.map_or(0, |v| v.len()));
                let mut seen: u32 = 0;
                for &act in any {
                    let bit = 1u32 << (act.ix() as u32);
                    if (seen & bit) != 0 {
                        continue;
                    }
                    seen |= bit;
                    out.push((act, pressed));
                }
                if let Some(v) = on {
                    for &act in v {
                        let bit = 1u32 << (act.ix() as u32);
                        if (seen & bit) != 0 {
                            continue;
                        }
                        seen |= bit;
                        out.push((act, pressed));
                    }
                }
                out
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
                let Some(entries) = self.pad_code_rev.get(&code_u32) else {
                    return Vec::new();
                };

                let mut out = Vec::with_capacity(entries.len().min(4));
                let mut seen: u32 = 0;
                for e in entries {
                    if let Some(d_expected) = e.device
                        && d_expected != dev
                    {
                        continue;
                    }
                    if let Some(u_expected) = e.uuid
                        && u_expected != uuid
                    {
                        continue;
                    }
                    let bit = 1u32 << (e.act.ix() as u32);
                    if (seen & bit) != 0 {
                        continue;
                    }
                    seen |= bit;
                    out.push((e.act, pressed));
                }
                out
            }
            PadEvent::RawAxis { .. } => {
                // Axis events are exposed for debugging but are not yet
                // mapped directly to virtual actions.
                Vec::new()
            }
        }
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
    let timestamp = match *ev {
        PadEvent::Dir { timestamp, .. }
        | PadEvent::RawButton { timestamp, .. }
        | PadEvent::RawAxis { timestamp, .. } => timestamp,
    };
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
pub const fn lane_from_action(act: VirtualAction) -> Option<Lane> {
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
mod linux_evdev;
#[cfg(target_os = "macos")]
mod macos_iohid;
#[cfg(windows)]
mod windows_raw_input;
#[cfg(windows)]
mod windows_wgi;
