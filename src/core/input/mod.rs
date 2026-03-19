use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

mod backends;
mod debounce;

use debounce::{
    DebounceBinding, DebounceEdges, DebounceState, DebounceWindows, DebouncedEdge,
    debounce_input_edge_in_store, emit_due_debounce_edges_from,
};

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
    #[cfg(target_os = "linux")]
    LinuxEvdev,
    #[cfg(target_os = "freebsd")]
    FreeBsdHidraw,
    #[cfg(target_os = "freebsd")]
    FreeBsdEvdev,
    #[cfg(target_os = "macos")]
    MacOsIohid,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowsPadBackend {
    /// Choose the default Windows backend (currently Raw Input).
    Auto,
    #[default]
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
        host_nanos: u64,
        dir: PadDir,
        pressed: bool,
    },
    /// Raw low-level button event with platform-specific code and device UUID.
    RawButton {
        id: PadId,
        timestamp: Instant,
        host_nanos: u64,
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
        host_nanos: u64,
        code: PadCode,
        uuid: [u8; 16],
        value: f32,
    },
}

#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Clone, Copy, Debug)]
pub struct RawKeyboardEvent {
    pub code: KeyCode,
    pub pressed: bool,
    pub timestamp: Instant,
    pub host_nanos: u64,
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
    #[cfg_attr(target_os = "linux", allow(dead_code))]
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
#[cfg_attr(windows, allow(dead_code))]
pub fn run_pad_backend(
    win_backend: WindowsPadBackend,
    emit_pad: impl FnMut(PadEvent) + Send + 'static,
    emit_sys: impl FnMut(GpSystemEvent) + Send + 'static,
) {
    #[cfg(not(windows))]
    let _ = win_backend;

    #[cfg(windows)]
    match win_backend {
        WindowsPadBackend::Auto | WindowsPadBackend::RawInput => {
            backends::w32_raw_input::run(emit_pad, emit_sys, |_| {})
        }
        WindowsPadBackend::Wgi => backends::wgi::run(emit_pad, emit_sys),
    }
    #[cfg(target_os = "linux")]
    return backends::evdev::run(emit_pad, emit_sys);
    #[cfg(target_os = "freebsd")]
    {
        let mut emit_pad = emit_pad;
        let mut emit_sys = emit_sys;
        if let Err(err) = backends::hidraw::run(&mut emit_pad, &mut emit_sys) {
            log::warn!("freebsd hidraw unavailable or unusable ({err}); falling back to evdev");
        }
        return backends::evdev::run(emit_pad, emit_sys);
    }
    #[cfg(target_os = "macos")]
    return backends::iohid::run(emit_pad, emit_sys);

    #[cfg(not(any(
        windows,
        target_os = "linux",
        target_os = "freebsd",
        target_os = "macos"
    )))]
    {
        let _ = emit_pad;
        let _ = emit_sys;
        loop {
            std::thread::park();
        }
    }
}

#[cfg(windows)]
pub fn run_windows_backend(
    win_backend: WindowsPadBackend,
    emit_pad: impl FnMut(PadEvent) + Send + 'static,
    emit_sys: impl FnMut(GpSystemEvent) + Send + 'static,
    emit_key: impl FnMut(RawKeyboardEvent) + Send + 'static,
) {
    match win_backend {
        WindowsPadBackend::Auto | WindowsPadBackend::RawInput => {
            backends::w32_raw_input::run(emit_pad, emit_sys, emit_key);
        }
        WindowsPadBackend::Wgi => {
            std::thread::spawn(move || backends::wgi::run(emit_pad, emit_sys));
            backends::w32_raw_input::run_keyboard_only(emit_key);
        }
    }
}

#[cfg(windows)]
#[inline(always)]
pub fn set_raw_keyboard_window_focused(focused: bool) {
    backends::w32_raw_input::set_window_focused(focused);
}

#[cfg(windows)]
#[inline(always)]
pub fn set_raw_keyboard_capture_enabled(enabled: bool) {
    backends::w32_raw_input::set_capture_enabled(enabled);
}

#[cfg(not(windows))]
#[inline(always)]
pub fn set_raw_keyboard_window_focused(focused: bool) {
    let _ = focused;
}

#[cfg(not(windows))]
#[allow(dead_code)]
#[inline(always)]
pub fn set_raw_keyboard_capture_enabled(enabled: bool) {
    let _ = enabled;
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
    pub record_replay: bool,
    // Real-time timestamps for latency tracing. Filled in by gameplay when the
    // edge is accepted for lane processing.
    pub captured_at: Instant,
    pub captured_host_nanos: u64,
    pub stored_at: Instant,
    pub emitted_at: Instant,
    pub queued_at: Instant,
    // Music time (seconds) at which this edge occurred, in the gameplay
    // screen's timebase (includes music rate and global offset). Replay edges
    // carry a concrete value; live gameplay fills this in from the audio clock
    // snapshot at judgment time.
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
    pub const COUNT: usize = Self::p2_restart as usize + 1;

    #[inline(always)]
    pub const fn ix(self) -> usize {
        self as usize
    }

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

    #[inline(always)]
    pub const fn secondary_menu(self) -> Option<Self> {
        match self {
            Self::p1_up => Some(Self::p1_menu_up),
            Self::p1_down => Some(Self::p1_menu_down),
            Self::p1_left => Some(Self::p1_menu_left),
            Self::p1_right => Some(Self::p1_menu_right),
            Self::p2_up => Some(Self::p2_menu_up),
            Self::p2_down => Some(Self::p2_menu_down),
            Self::p2_left => Some(Self::p2_menu_left),
            Self::p2_right => Some(Self::p2_menu_right),
            _ => None,
        }
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
static ONLY_DEDICATED_MENU_BUTTONS: AtomicBool = AtomicBool::new(false);
static INPUT_DEBOUNCE_SECONDS_BITS: AtomicU32 = AtomicU32::new((0.02f32).to_bits());
static GAMEPLAY_RELEASE_DEBOUNCE_SECONDS_BITS: AtomicU32 = AtomicU32::new((0.005f32).to_bits());
static GAMEPLAY_KEYBOARD_DEBOUNCE_STATE: std::sync::LazyLock<
    Mutex<HashMap<DebounceBinding, DebounceState>>,
> = std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));
static GAMEPLAY_PAD_DEBOUNCE_STATE: std::sync::LazyLock<
    Mutex<HashMap<DebounceBinding, DebounceState>>,
> = std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

const INPUT_DEBOUNCE_MAX_SECONDS: f32 = 0.2;

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
    GAMEPLAY_KEYBOARD_DEBOUNCE_STATE.lock().unwrap().clear();
    GAMEPLAY_PAD_DEBOUNCE_STATE.lock().unwrap().clear();
}

#[inline(always)]
pub fn clear_debounce_state() {
    GAMEPLAY_KEYBOARD_DEBOUNCE_STATE.lock().unwrap().clear();
    GAMEPLAY_PAD_DEBOUNCE_STATE.lock().unwrap().clear();
}

#[inline(always)]
pub fn set_only_dedicated_menu_buttons(enabled: bool) {
    ONLY_DEDICATED_MENU_BUTTONS.store(enabled, Ordering::Relaxed);
}

/// Returns `true` if at least one player has all four dedicated menu
/// directional buttons (menu_up, menu_down, menu_left, menu_right) bound.
pub fn any_player_has_dedicated_menu_buttons() -> bool {
    with_keymap(|km| {
        let p1 = km.binding_at(VirtualAction::p1_menu_up, 0).is_some()
            && km.binding_at(VirtualAction::p1_menu_down, 0).is_some()
            && km.binding_at(VirtualAction::p1_menu_left, 0).is_some()
            && km.binding_at(VirtualAction::p1_menu_right, 0).is_some();
        let p2 = km.binding_at(VirtualAction::p2_menu_up, 0).is_some()
            && km.binding_at(VirtualAction::p2_menu_down, 0).is_some()
            && km.binding_at(VirtualAction::p2_menu_left, 0).is_some()
            && km.binding_at(VirtualAction::p2_menu_right, 0).is_some();
        p1 || p2
    })
}

#[inline(always)]
pub fn set_input_debounce_seconds(seconds: f32) {
    let clamped = seconds.clamp(0.0, INPUT_DEBOUNCE_MAX_SECONDS);
    INPUT_DEBOUNCE_SECONDS_BITS.store(clamped.to_bits(), Ordering::Relaxed);
}

#[inline(always)]
fn input_debounce_window() -> Duration {
    Duration::from_secs_f32(f32::from_bits(
        INPUT_DEBOUNCE_SECONDS_BITS.load(Ordering::Relaxed),
    ))
}

#[inline(always)]
pub fn set_gameplay_release_debounce_seconds(seconds: f32) {
    let clamped = seconds.clamp(0.0, INPUT_DEBOUNCE_MAX_SECONDS);
    GAMEPLAY_RELEASE_DEBOUNCE_SECONDS_BITS.store(clamped.to_bits(), Ordering::Relaxed);
}

#[inline(always)]
fn gameplay_debounce_windows() -> DebounceWindows {
    DebounceWindows {
        press: input_debounce_window(),
        // Gameplay release debounce is intentionally shorter than the generic
        // input window so taps do not feel sticky on fast streams.
        release: Duration::from_secs_f32(f32::from_bits(
            GAMEPLAY_RELEASE_DEBOUNCE_SECONDS_BITS.load(Ordering::Relaxed),
        )),
    }
}

#[inline(always)]
fn gameplay_keyboard_debounce_windows() -> DebounceWindows {
    gameplay_debounce_windows()
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
        let PhysicalKey::Code(code) = ev.physical_key else {
            return Vec::new();
        };
        self.actions_for_key_code(code, ev.state == ElementState::Pressed)
    }

    #[inline(always)]
    pub fn keycode_mapped(&self, code: KeyCode) -> bool {
        self.key_rev
            .get(&code)
            .is_some_and(|actions| !actions.is_empty())
    }

    #[inline(always)]
    pub fn keycode_has_action(&self, code: KeyCode, keep: impl Fn(VirtualAction) -> bool) -> bool {
        let Some(actions) = self.key_rev.get(&code) else {
            return false;
        };
        for &action in actions {
            if keep(action) {
                return true;
            }
        }
        false
    }

    #[inline(always)]
    pub fn key_event_mapped(&self, ev: &KeyEvent) -> bool {
        let PhysicalKey::Code(code) = ev.physical_key else {
            return false;
        };
        self.keycode_mapped(code)
    }

    #[inline(always)]
    pub fn key_event_has_action(
        &self,
        ev: &KeyEvent,
        keep: impl Fn(VirtualAction) -> bool,
    ) -> bool {
        let PhysicalKey::Code(code) = ev.physical_key else {
            return false;
        };
        self.keycode_has_action(code, keep)
    }

    #[inline(always)]
    pub fn actions_for_key_code(&self, code: KeyCode, pressed: bool) -> Vec<(VirtualAction, bool)> {
        let Some(actions) = self.key_rev.get(&code) else {
            return Vec::new();
        };
        dedup_actions(actions, pressed)
    }

    #[inline(always)]
    pub fn actions_for_pad_event(&self, ev: &PadEvent) -> Vec<(VirtualAction, bool)> {
        match *ev {
            PadEvent::Dir {
                id, dir, pressed, ..
            } => self.actions_for_pad_dir(id, dir, pressed),
            PadEvent::RawButton {
                id,
                code,
                uuid,
                pressed,
                ..
            } => self.actions_for_pad_button(id, code, uuid, pressed),
            PadEvent::RawAxis { .. } => {
                // Axis events are exposed for debugging but are not yet
                // mapped directly to virtual actions.
                Vec::new()
            }
        }
    }

    #[inline(always)]
    pub fn pad_event_mapped(&self, ev: &PadEvent) -> bool {
        match *ev {
            PadEvent::Dir { id, dir, .. } => {
                let dev = usize::from(id);
                !self.pad_dir_rev[dir.ix()].is_empty()
                    || self.pad_dir_on_rev.contains_key(&(dev, dir))
            }
            PadEvent::RawButton { id, code, uuid, .. } => {
                let dev = usize::from(id);
                let Some(entries) = self.pad_code_rev.get(&code.into_u32()) else {
                    return false;
                };
                for entry in entries {
                    if let Some(d_expected) = entry.device
                        && d_expected != dev
                    {
                        continue;
                    }
                    if let Some(u_expected) = entry.uuid
                        && u_expected != uuid
                    {
                        continue;
                    }
                    return true;
                }
                false
            }
            PadEvent::RawAxis { .. } => false,
        }
    }

    #[inline(always)]
    pub fn actions_for_pad_dir(
        &self,
        id: PadId,
        dir: PadDir,
        pressed: bool,
    ) -> Vec<(VirtualAction, bool)> {
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

    #[inline(always)]
    pub fn actions_for_pad_button(
        &self,
        id: PadId,
        code: PadCode,
        uuid: [u8; 16],
        pressed: bool,
    ) -> Vec<(VirtualAction, bool)> {
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
}

#[inline(always)]
fn dedup_actions(actions: &[VirtualAction], pressed: bool) -> Vec<(VirtualAction, bool)> {
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

// INI parsing and default emission moved to config.rs

/* ------------------------- Normalized input events ------------------------- */

#[derive(Clone, Copy, Debug)]
pub struct InputEvent {
    pub action: VirtualAction,
    pub pressed: bool,
    pub source: InputSource,
    // Timestamp of the raw input edge before debounce filtering.
    pub timestamp: Instant,
    // Host/QPC clock for `timestamp` when the backend can provide one; 0 means
    // the event only has a local `Instant` anchor.
    pub timestamp_host_nanos: u64,
    // Timestamp at which the edge entered the debounce store on the main input path.
    pub stored_at: Instant,
    // Timestamp at which the debounced/normalized input event was emitted.
    pub emitted_at: Instant,
}

#[inline(always)]
fn input_event(
    action: VirtualAction,
    pressed: bool,
    source: InputSource,
    timestamp: Instant,
    timestamp_host_nanos: u64,
    stored_at: Instant,
    emitted_at: Instant,
) -> InputEvent {
    InputEvent {
        action,
        pressed,
        source,
        timestamp,
        timestamp_host_nanos,
        stored_at,
        emitted_at,
    }
}

type ActionBuf = [Option<VirtualAction>; VirtualAction::COUNT];

#[inline(always)]
const fn action_bit(action: VirtualAction) -> u32 {
    1u32 << (action.ix() as u32)
}

#[inline(always)]
fn push_action(out: &mut ActionBuf, len: &mut usize, seen: &mut u32, action: VirtualAction) {
    let bit = action_bit(action);
    if (*seen & bit) != 0 {
        return;
    }
    *seen |= bit;
    out[*len] = Some(action);
    *len += 1;
}

#[inline(always)]
fn collect_key_actions(km: &Keymap, code: KeyCode, out: &mut ActionBuf) -> usize {
    let Some(actions) = km.key_rev.get(&code) else {
        return 0;
    };
    let mut len = 0;
    let mut seen = 0;
    for &action in actions {
        push_action(out, &mut len, &mut seen, action);
    }
    len
}

#[inline(always)]
fn collect_pad_dir_actions(km: &Keymap, id: PadId, dir: PadDir, out: &mut ActionBuf) -> usize {
    let dev = usize::from(id);
    let any = &km.pad_dir_rev[dir.ix()];
    let on = km.pad_dir_on_rev.get(&(dev, dir));
    if any.is_empty() && on.is_none() {
        return 0;
    }
    let mut len = 0;
    let mut seen = 0;
    for &action in any {
        push_action(out, &mut len, &mut seen, action);
    }
    if let Some(actions) = on {
        for &action in actions {
            push_action(out, &mut len, &mut seen, action);
        }
    }
    len
}

#[inline(always)]
fn collect_pad_button_actions(
    km: &Keymap,
    id: PadId,
    code: PadCode,
    uuid: [u8; 16],
    out: &mut ActionBuf,
) -> usize {
    let Some(entries) = km.pad_code_rev.get(&code.into_u32()) else {
        return 0;
    };
    let dev = usize::from(id);
    let mut len = 0;
    let mut seen = 0;
    for entry in entries {
        if let Some(d_expected) = entry.device
            && d_expected != dev
        {
            continue;
        }
        if let Some(u_expected) = entry.uuid
            && u_expected != uuid
        {
            continue;
        }
        push_action(out, &mut len, &mut seen, entry.act);
    }
    len
}

#[inline(always)]
fn direct_action_mask(actions: &ActionBuf, len: usize) -> u32 {
    let mut mask = 0;
    let mut i = 0;
    while i < len {
        if let Some(action) = actions[i] {
            mask |= action_bit(action);
        }
        i += 1;
    }
    mask
}

#[inline(always)]
fn emit_normalized_action(
    action: VirtualAction,
    pressed: bool,
    direct_mask: u32,
    emitted: &mut u32,
    emit: &mut impl FnMut(VirtualAction, bool),
) {
    if pressed
        && let Some(primary) = primary_from_menu_alias(action)
        && (direct_mask & action_bit(primary)) != 0
    {
        return;
    }
    let bit = action_bit(action);
    if (*emitted & bit) != 0 {
        return;
    }
    *emitted |= bit;
    emit(action, pressed);
}

#[inline(always)]
fn emit_normalized_actions(
    actions: &ActionBuf,
    len: usize,
    pressed: bool,
    mut emit: impl FnMut(VirtualAction, bool),
) {
    if len == 0 {
        return;
    }
    let direct_mask = direct_action_mask(actions, len);
    let mut emitted = 0;
    let mut i = 0;
    while i < len {
        if let Some(action) = actions[i] {
            emit_normalized_action(action, pressed, direct_mask, &mut emitted, &mut emit);
        }
        i += 1;
    }
    if ONLY_DEDICATED_MENU_BUTTONS.load(Ordering::Relaxed) && pressed {
        return;
    }
    let mut i = 0;
    while i < len {
        if let Some(menu_action) = actions[i].and_then(VirtualAction::secondary_menu) {
            emit_normalized_action(menu_action, pressed, direct_mask, &mut emitted, &mut emit);
        }
        i += 1;
    }
}

#[inline(always)]
fn emit_filtered_key_actions(
    km: &Keymap,
    code: KeyCode,
    keep: impl Fn(VirtualAction) -> bool,
    mut emit: impl FnMut(VirtualAction),
) {
    let Some(actions) = km.key_rev.get(&code) else {
        return;
    };
    let mut seen: u32 = 0;
    for &action in actions {
        if !keep(action) {
            continue;
        }
        let bit = 1u32 << (action.ix() as u32);
        if (seen & bit) != 0 {
            continue;
        }
        seen |= bit;
        emit(action);
    }
}

#[inline(always)]
fn emit_filtered_pad_dir_actions(
    km: &Keymap,
    id: PadId,
    dir: PadDir,
    keep: impl Fn(VirtualAction) -> bool,
    mut emit: impl FnMut(VirtualAction),
) {
    let dev = usize::from(id);
    let any = &km.pad_dir_rev[dir.ix()];
    let on = km.pad_dir_on_rev.get(&(dev, dir));
    if any.is_empty() && on.is_none() {
        return;
    }
    let mut seen: u32 = 0;
    for &action in any {
        if !keep(action) {
            continue;
        }
        let bit = 1u32 << (action.ix() as u32);
        if (seen & bit) != 0 {
            continue;
        }
        seen |= bit;
        emit(action);
    }
    if let Some(actions) = on {
        for &action in actions {
            if !keep(action) {
                continue;
            }
            let bit = 1u32 << (action.ix() as u32);
            if (seen & bit) != 0 {
                continue;
            }
            seen |= bit;
            emit(action);
        }
    }
}

#[inline(always)]
fn emit_filtered_pad_button_actions(
    km: &Keymap,
    id: PadId,
    code: PadCode,
    uuid: [u8; 16],
    keep: impl Fn(VirtualAction) -> bool,
    mut emit: impl FnMut(VirtualAction),
) {
    let dev = usize::from(id);
    let Some(entries) = km.pad_code_rev.get(&code.into_u32()) else {
        return;
    };
    let mut seen: u32 = 0;
    for entry in entries {
        if let Some(d_expected) = entry.device
            && d_expected != dev
        {
            continue;
        }
        if let Some(u_expected) = entry.uuid
            && u_expected != uuid
        {
            continue;
        }
        let action = entry.act;
        if !keep(action) {
            continue;
        }
        let bit = 1u32 << (action.ix() as u32);
        if (seen & bit) != 0 {
            continue;
        }
        seen |= bit;
        emit(action);
    }
}

#[inline(always)]
fn emit_filtered_actions_from_edge(
    km: &Keymap,
    edge: DebouncedEdge,
    keep: impl Fn(VirtualAction) -> bool + Copy,
    mut emit: impl FnMut(VirtualAction),
) {
    match edge.binding {
        DebounceBinding::Keyboard(code) => {
            emit_filtered_key_actions(km, code, keep, |action| emit(action))
        }
        DebounceBinding::PadDir { id, dir } => {
            emit_filtered_pad_dir_actions(km, id, dir, keep, |action| emit(action))
        }
        DebounceBinding::PadButton { id, code, uuid } => {
            emit_filtered_pad_button_actions(km, id, code, uuid, keep, |action| emit(action))
        }
    }
}

#[inline(always)]
fn emit_gameplay_events_from_edge(edge: DebouncedEdge, mut emit: impl FnMut(InputEvent)) {
    with_keymap(|km| {
        emit_filtered_actions_from_edge(km, edge, VirtualAction::is_gameplay_arrow, |action| {
            emit(input_event(
                action,
                edge.pressed,
                edge.source,
                edge.timestamp,
                edge.timestamp_host_nanos,
                edge.stored_at,
                edge.emitted_at,
            ));
        });
    });
}

#[cfg_attr(not(windows), allow(dead_code))]
#[inline(always)]
pub fn keycode_is_gameplay_arrow_only(code: KeyCode) -> bool {
    with_keymap(|km| {
        let Some(actions) = km.key_rev.get(&code) else {
            return false;
        };
        let mut saw_arrow = false;
        for &action in actions {
            if !action.is_gameplay_arrow() {
                return false;
            }
            saw_arrow = true;
        }
        saw_arrow
    })
}

#[inline(always)]
pub fn gameplay_arrow_keycode_events_with(
    code: KeyCode,
    pressed: bool,
    timestamp: Instant,
    mut emit: impl FnMut(InputEvent),
) {
    gameplay_arrow_keycode_events_with_host(code, pressed, timestamp, 0, &mut emit);
}

#[inline(always)]
pub fn gameplay_arrow_keycode_events_with_host(
    code: KeyCode,
    pressed: bool,
    timestamp: Instant,
    timestamp_host_nanos: u64,
    mut emit: impl FnMut(InputEvent),
) {
    let edges = debounce_input_edge_in_store(
        &GAMEPLAY_KEYBOARD_DEBOUNCE_STATE,
        DebounceBinding::Keyboard(code),
        pressed,
        timestamp,
        timestamp_host_nanos,
        gameplay_keyboard_debounce_windows(),
    );
    if let Some(edge) = edges.first {
        emit_gameplay_events_from_edge(edge, &mut emit);
    }
    if let Some(edge) = edges.second {
        emit_gameplay_events_from_edge(edge, &mut emit);
    }
}

#[inline(always)]
fn emit_debounced_input_events(edges: DebounceEdges, mut emit: impl FnMut(InputEvent)) {
    if let Some(edge) = edges.first {
        emit_gameplay_events_from_edge(edge, &mut emit);
    }
    if let Some(edge) = edges.second {
        emit_gameplay_events_from_edge(edge, &mut emit);
    }
}

#[inline(always)]
pub fn map_key_event_with(ev: &KeyEvent, timestamp: Instant, emit: impl FnMut(InputEvent)) {
    if ev.state == ElementState::Pressed && ev.repeat {
        return;
    }
    let PhysicalKey::Code(code) = ev.physical_key else {
        return;
    };
    map_keycode_event_with(code, ev.state == ElementState::Pressed, timestamp, emit);
}

#[inline(always)]
pub fn map_keycode_event_with(
    code: KeyCode,
    pressed: bool,
    timestamp: Instant,
    mut emit: impl FnMut(InputEvent),
) {
    map_keycode_event_with_host(code, pressed, timestamp, 0, &mut emit);
}

#[inline(always)]
pub fn map_keycode_event_with_host(
    code: KeyCode,
    pressed: bool,
    timestamp: Instant,
    timestamp_host_nanos: u64,
    mut emit: impl FnMut(InputEvent),
) {
    let mut actions: ActionBuf = [None; VirtualAction::COUNT];
    let len = with_keymap(|km| collect_key_actions(km, code, &mut actions));
    emit_normalized_actions(&actions, len, pressed, |action, pressed| {
        emit(input_event(
            action,
            pressed,
            InputSource::Keyboard,
            timestamp,
            timestamp_host_nanos,
            timestamp,
            timestamp,
        ));
    });
}

#[inline(always)]
pub fn gameplay_arrow_key_events_with(
    ev: &KeyEvent,
    timestamp: Instant,
    emit: impl FnMut(InputEvent),
) {
    if ev.state == ElementState::Pressed && ev.repeat {
        return;
    }
    let PhysicalKey::Code(code) = ev.physical_key else {
        return;
    };
    gameplay_arrow_keycode_events_with(code, ev.state == ElementState::Pressed, timestamp, emit);
}

#[inline(always)]
fn pad_event_timestamps(ev: &PadEvent) -> (Instant, u64) {
    match *ev {
        PadEvent::Dir {
            timestamp,
            host_nanos,
            ..
        }
        | PadEvent::RawButton {
            timestamp,
            host_nanos,
            ..
        }
        | PadEvent::RawAxis {
            timestamp,
            host_nanos,
            ..
        } => (timestamp, host_nanos),
    }
}

#[inline(always)]
pub fn map_pad_event_with(ev: &PadEvent, mut emit: impl FnMut(InputEvent)) {
    let (timestamp, timestamp_host_nanos) = pad_event_timestamps(ev);
    let mut actions: ActionBuf = [None; VirtualAction::COUNT];
    let (pressed, len) = with_keymap(|km| match *ev {
        PadEvent::Dir {
            id, dir, pressed, ..
        } => (pressed, collect_pad_dir_actions(km, id, dir, &mut actions)),
        PadEvent::RawButton {
            id,
            code,
            uuid,
            pressed,
            ..
        } => (
            pressed,
            collect_pad_button_actions(km, id, code, uuid, &mut actions),
        ),
        PadEvent::RawAxis { .. } => (false, 0),
    });
    emit_normalized_actions(&actions, len, pressed, |action, pressed| {
        emit(input_event(
            action,
            pressed,
            InputSource::Gamepad,
            timestamp,
            timestamp_host_nanos,
            timestamp,
            timestamp,
        ));
    });
}

#[inline(always)]
pub fn gameplay_arrow_pad_events_with(ev: &PadEvent, emit: impl FnMut(InputEvent)) {
    let edges = match *ev {
        PadEvent::Dir {
            id,
            dir,
            pressed,
            timestamp,
            host_nanos,
        } => debounce_input_edge_in_store(
            &GAMEPLAY_PAD_DEBOUNCE_STATE,
            DebounceBinding::PadDir { id, dir },
            pressed,
            timestamp,
            host_nanos,
            gameplay_debounce_windows(),
        ),
        PadEvent::RawButton {
            id,
            code,
            uuid,
            pressed,
            timestamp,
            host_nanos,
            ..
        } => debounce_input_edge_in_store(
            &GAMEPLAY_PAD_DEBOUNCE_STATE,
            DebounceBinding::PadButton { id, code, uuid },
            pressed,
            timestamp,
            host_nanos,
            gameplay_debounce_windows(),
        ),
        PadEvent::RawAxis { .. } => return,
    };
    emit_debounced_input_events(edges, emit);
}

pub fn drain_gameplay_arrow_events_with(mut emit: impl FnMut(InputEvent)) -> bool {
    let now = Instant::now();
    let mut flushed = emit_due_debounce_edges_from(
        &GAMEPLAY_KEYBOARD_DEBOUNCE_STATE,
        now,
        gameplay_keyboard_debounce_windows(),
        |edge| emit_gameplay_events_from_edge(edge, &mut emit),
    );
    flushed |= emit_due_debounce_edges_from(
        &GAMEPLAY_PAD_DEBOUNCE_STATE,
        now,
        gameplay_debounce_windows(),
        |edge| emit_gameplay_events_from_edge(edge, &mut emit),
    );
    flushed
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
const fn primary_from_menu_alias(act: VirtualAction) -> Option<VirtualAction> {
    match act {
        VirtualAction::p1_menu_up => Some(VirtualAction::p1_up),
        VirtualAction::p1_menu_down => Some(VirtualAction::p1_down),
        VirtualAction::p1_menu_left => Some(VirtualAction::p1_left),
        VirtualAction::p1_menu_right => Some(VirtualAction::p1_right),
        VirtualAction::p2_menu_up => Some(VirtualAction::p2_up),
        VirtualAction::p2_menu_down => Some(VirtualAction::p2_down),
        VirtualAction::p2_menu_left => Some(VirtualAction::p2_left),
        VirtualAction::p2_menu_right => Some(VirtualAction::p2_right),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_GUARD: std::sync::LazyLock<Mutex<()>> = std::sync::LazyLock::new(|| Mutex::new(()));

    fn assert_events_eq(actual: &[InputEvent], expected: &[InputEvent]) {
        assert_eq!(actual.len(), expected.len(), "event count");
        for (actual, expected) in actual.iter().zip(expected.iter()) {
            assert_eq!(actual.action, expected.action);
            assert_eq!(actual.pressed, expected.pressed);
            assert_eq!(actual.source, expected.source);
            assert_eq!(actual.timestamp, expected.timestamp);
            assert_eq!(actual.timestamp_host_nanos, expected.timestamp_host_nanos);
            assert_eq!(actual.stored_at, expected.stored_at);
            assert_eq!(actual.emitted_at, expected.emitted_at);
        }
    }

    #[test]
    fn map_keycode_event_with_emits_expected_actions() {
        let _guard = TEST_GUARD.lock().unwrap();
        let original = get_keymap();
        let mut km = Keymap::default();
        km.bind(
            VirtualAction::p1_left,
            &[InputBinding::Key(KeyCode::ArrowLeft)],
        );
        set_keymap(km);
        set_only_dedicated_menu_buttons(false);

        let timestamp = Instant::now();
        let mut actual = Vec::new();
        map_keycode_event_with(KeyCode::ArrowLeft, true, timestamp, |event| {
            actual.push(event);
        });
        let expected = [
            input_event(
                VirtualAction::p1_left,
                true,
                InputSource::Keyboard,
                timestamp,
                0,
                timestamp,
                timestamp,
            ),
            input_event(
                VirtualAction::p1_menu_left,
                true,
                InputSource::Keyboard,
                timestamp,
                0,
                timestamp,
                timestamp,
            ),
        ];
        assert_events_eq(&actual, &expected);

        set_keymap(original);
        set_only_dedicated_menu_buttons(false);
        clear_debounce_state();
    }

    #[test]
    fn map_pad_event_with_emits_expected_actions() {
        let _guard = TEST_GUARD.lock().unwrap();
        let original = get_keymap();
        let mut km = Keymap::default();
        km.bind(
            VirtualAction::p1_left,
            &[InputBinding::PadDir(PadDir::Left)],
        );
        set_keymap(km);
        set_only_dedicated_menu_buttons(false);

        let timestamp = Instant::now();
        let event = PadEvent::Dir {
            id: PadId(1),
            timestamp,
            host_nanos: 42,
            dir: PadDir::Left,
            pressed: true,
        };
        let mut actual = Vec::new();
        map_pad_event_with(&event, |input| actual.push(input));
        let expected = [
            input_event(
                VirtualAction::p1_left,
                true,
                InputSource::Gamepad,
                timestamp,
                42,
                timestamp,
                timestamp,
            ),
            input_event(
                VirtualAction::p1_menu_left,
                true,
                InputSource::Gamepad,
                timestamp,
                42,
                timestamp,
                timestamp,
            ),
        ];
        assert_events_eq(&actual, &expected);

        set_keymap(original);
        set_only_dedicated_menu_buttons(false);
        clear_debounce_state();
    }

    #[test]
    fn map_keycode_event_with_suppresses_pressed_alias_when_primary_is_bound() {
        let _guard = TEST_GUARD.lock().unwrap();
        let original = get_keymap();
        let mut km = Keymap::default();
        km.bind(
            VirtualAction::p1_left,
            &[InputBinding::Key(KeyCode::ArrowLeft)],
        );
        km.bind(
            VirtualAction::p1_menu_left,
            &[InputBinding::Key(KeyCode::ArrowLeft)],
        );
        set_keymap(km);
        set_only_dedicated_menu_buttons(false);

        let timestamp = Instant::now();
        let mut actual = Vec::new();
        map_keycode_event_with(KeyCode::ArrowLeft, true, timestamp, |event| {
            actual.push(event);
        });
        let expected = [input_event(
            VirtualAction::p1_left,
            true,
            InputSource::Keyboard,
            timestamp,
            0,
            timestamp,
            timestamp,
        )];
        assert_events_eq(&actual, &expected);

        set_keymap(original);
        set_only_dedicated_menu_buttons(false);
        clear_debounce_state();
    }

    #[test]
    fn map_keycode_event_with_keeps_release_alias_in_dedicated_mode() {
        let _guard = TEST_GUARD.lock().unwrap();
        let original = get_keymap();
        let mut km = Keymap::default();
        km.bind(
            VirtualAction::p1_left,
            &[InputBinding::Key(KeyCode::ArrowLeft)],
        );
        set_keymap(km);
        set_only_dedicated_menu_buttons(true);

        let timestamp = Instant::now();
        let mut actual = Vec::new();
        map_keycode_event_with(KeyCode::ArrowLeft, false, timestamp, |event| {
            actual.push(event);
        });
        let expected = [
            input_event(
                VirtualAction::p1_left,
                false,
                InputSource::Keyboard,
                timestamp,
                0,
                timestamp,
                timestamp,
            ),
            input_event(
                VirtualAction::p1_menu_left,
                false,
                InputSource::Keyboard,
                timestamp,
                0,
                timestamp,
                timestamp,
            ),
        ];
        assert_events_eq(&actual, &expected);

        set_keymap(original);
        set_only_dedicated_menu_buttons(false);
        clear_debounce_state();
    }

    #[test]
    fn keycode_has_action_matches_without_allocating_action_vec() {
        let _guard = TEST_GUARD.lock().unwrap();
        let original = get_keymap();
        let mut km = Keymap::default();
        km.bind(
            VirtualAction::p1_back,
            &[InputBinding::Key(KeyCode::Escape)],
        );
        set_keymap(km);

        with_keymap(|km| {
            assert!(km.keycode_mapped(KeyCode::Escape));
            assert!(
                km.keycode_has_action(KeyCode::Escape, |action| action == VirtualAction::p1_back)
            );
            assert!(
                !km.keycode_has_action(KeyCode::Escape, |action| action == VirtualAction::p2_back)
            );
        });

        set_keymap(original);
        clear_debounce_state();
    }

    #[test]
    fn pad_event_mapped_checks_device_and_uuid_without_allocating_action_vec() {
        let _guard = TEST_GUARD.lock().unwrap();
        let original = get_keymap();
        let mut km = Keymap::default();
        km.bind(
            VirtualAction::p1_start,
            &[InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32: 77,
                device: Some(3),
                uuid: Some([9; 16]),
            })],
        );
        set_keymap(km);

        let mapped = PadEvent::RawButton {
            id: PadId(3),
            timestamp: Instant::now(),
            host_nanos: 0,
            code: PadCode(77),
            uuid: [9; 16],
            value: 1.0,
            pressed: true,
        };
        let wrong_dev = PadEvent::RawButton {
            id: PadId(4),
            timestamp: Instant::now(),
            host_nanos: 0,
            code: PadCode(77),
            uuid: [9; 16],
            value: 1.0,
            pressed: true,
        };

        with_keymap(|km| {
            assert!(km.pad_event_mapped(&mapped));
            assert!(!km.pad_event_mapped(&wrong_dev));
        });

        set_keymap(original);
        clear_debounce_state();
    }
}
