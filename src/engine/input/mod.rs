use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use winit::keyboard::KeyCode;

mod backends;
mod debounce;

use debounce::{
    DebounceBinding, DebounceEdges, DebounceStore, DebounceWindows, DebouncedEdge,
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
    pub repeat: bool,
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
/// `GpSystemEvent` into the winit `EventLoopProxy` (see `deadsync/src/app/mod.rs`).
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
    return backends::evdev::run_pad_only(emit_pad, emit_sys);
    #[cfg(target_os = "freebsd")]
    {
        let mut emit_pad = emit_pad;
        let mut emit_sys = emit_sys;
        if let Err(err) = backends::hidraw::run(&mut emit_pad, &mut emit_sys) {
            log::warn!("freebsd hidraw unavailable or unusable ({err}); falling back to evdev");
        }
        return backends::evdev::run_pad_only(emit_pad, emit_sys);
    }
    #[cfg(target_os = "macos")]
    return backends::iohid::run(emit_pad, emit_sys, |_| {});

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

#[cfg(target_os = "linux")]
pub fn run_linux_backend(
    emit_pad: impl FnMut(PadEvent) + Send + 'static,
    emit_sys: impl FnMut(GpSystemEvent) + Send + 'static,
    emit_key: impl FnMut(RawKeyboardEvent) + Send + 'static,
) {
    backends::evdev::run(emit_pad, emit_sys, emit_key);
}

#[cfg(target_os = "freebsd")]
pub fn run_freebsd_backend(
    emit_pad: impl FnMut(PadEvent) + Send + 'static,
    emit_sys: impl FnMut(GpSystemEvent) + Send + 'static,
    emit_key: impl FnMut(RawKeyboardEvent) + Send + 'static,
) {
    backends::evdev::run(emit_pad, emit_sys, emit_key);
}

#[cfg(target_os = "macos")]
pub fn run_macos_backend(
    emit_pad: impl FnMut(PadEvent) + Send + 'static,
    emit_sys: impl FnMut(GpSystemEvent) + Send + 'static,
    emit_key: impl FnMut(RawKeyboardEvent) + Send + 'static,
) {
    backends::iohid::run(emit_pad, emit_sys, emit_key);
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

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[inline(always)]
pub fn set_raw_keyboard_window_focused(focused: bool) {
    backends::evdev::set_keyboard_window_focused(focused);
}

#[cfg(target_os = "macos")]
#[inline(always)]
pub fn set_raw_keyboard_window_focused(focused: bool) {
    backends::iohid::set_keyboard_window_focused(focused);
}

#[cfg(all(
    not(windows),
    not(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))
))]
#[inline(always)]
pub fn set_raw_keyboard_window_focused(focused: bool) {
    let _ = focused;
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[inline(always)]
pub fn set_raw_keyboard_capture_enabled(enabled: bool) {
    backends::evdev::set_keyboard_capture_enabled(enabled);
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[inline(always)]
pub fn unix_raw_keyboard_backend_active() -> bool {
    backends::evdev::keyboard_backend_active()
}

#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
#[inline(always)]
pub fn unix_raw_keyboard_backend_active() -> bool {
    true
}

#[cfg(target_os = "macos")]
#[inline(always)]
pub fn set_raw_keyboard_capture_enabled(enabled: bool) {
    backends::iohid::set_keyboard_capture_enabled(enabled);
}

#[cfg(all(
    not(windows),
    not(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))
))]
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

const KEY_CODE_CAP: usize = KeyCode::F35 as usize + 1;

#[inline(always)]
fn new_key_rev() -> Box<[Vec<VirtualAction>]> {
    vec![Vec::new(); KEY_CODE_CAP].into_boxed_slice()
}

#[inline(always)]
fn new_key_mask_rev() -> Box<[u32]> {
    vec![0; KEY_CODE_CAP].into_boxed_slice()
}

#[inline(always)]
const fn dense_key_ix(code: KeyCode) -> Option<usize> {
    let ix = code as usize;
    if ix < KEY_CODE_CAP { Some(ix) } else { None }
}

#[derive(Clone, Copy, Debug)]
struct CompiledPadCodeRev {
    mask: u32,
    device: Option<usize>,
    uuid: Option<[u8; 16]>,
}

#[derive(Clone, Debug)]
struct CompiledKeymap {
    key_rev: Box<[u32]>,
    key_rev_extra: HashMap<KeyCode, u32>,
    pad_dir_rev: [u32; 4],
    pad_dir_on_rev: HashMap<(usize, PadDir), u32>,
    pad_code_rev: HashMap<u32, Vec<CompiledPadCodeRev>>,
}

impl Default for CompiledKeymap {
    fn default() -> Self {
        Self {
            key_rev: new_key_mask_rev(),
            key_rev_extra: HashMap::new(),
            pad_dir_rev: [0; 4],
            pad_dir_on_rev: HashMap::new(),
            pad_code_rev: HashMap::new(),
        }
    }
}

impl CompiledKeymap {
    #[inline(always)]
    fn from_keymap(km: &Keymap) -> Self {
        let mut key_rev = new_key_mask_rev();
        for (ix, actions) in km.key_rev.iter().enumerate() {
            let mut mask = 0;
            for &action in actions {
                mask |= action_bit(action);
            }
            key_rev[ix] = mask;
        }
        let mut key_rev_extra = HashMap::with_capacity(km.key_rev_extra.len());
        for (&code, actions) in &km.key_rev_extra {
            let mut mask = 0;
            for &action in actions {
                mask |= action_bit(action);
            }
            key_rev_extra.insert(code, mask);
        }
        let mut pad_dir_rev = [0; 4];
        for (ix, actions) in km.pad_dir_rev.iter().enumerate() {
            let mut mask = 0;
            for &action in actions {
                mask |= action_bit(action);
            }
            pad_dir_rev[ix] = mask;
        }
        let mut pad_dir_on_rev = HashMap::with_capacity(km.pad_dir_on_rev.len());
        for (&key, actions) in &km.pad_dir_on_rev {
            let mut mask = 0;
            for &action in actions {
                mask |= action_bit(action);
            }
            pad_dir_on_rev.insert(key, mask);
        }
        let mut pad_code_rev = HashMap::with_capacity(km.pad_code_rev.len());
        for (&code, entries) in &km.pad_code_rev {
            let mut compiled = Vec::with_capacity(entries.len());
            for entry in entries {
                if let Some(existing) =
                    compiled.iter_mut().find(|item: &&mut CompiledPadCodeRev| {
                        item.device == entry.device && item.uuid == entry.uuid
                    })
                {
                    existing.mask |= action_bit(entry.act);
                    continue;
                }
                compiled.push(CompiledPadCodeRev {
                    mask: action_bit(entry.act),
                    device: entry.device,
                    uuid: entry.uuid,
                });
            }
            pad_code_rev.insert(code, compiled);
        }
        Self {
            key_rev,
            key_rev_extra,
            pad_dir_rev,
            pad_dir_on_rev,
            pad_code_rev,
        }
    }

    #[inline(always)]
    fn key_mask(&self, code: KeyCode) -> u32 {
        match dense_key_ix(code) {
            Some(ix) => self.key_rev[ix],
            None => self.key_rev_extra.get(&code).copied().unwrap_or(0),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Keymap {
    map: HashMap<VirtualAction, Vec<InputBinding>>,
    key_rev: Box<[Vec<VirtualAction>]>,
    key_rev_extra: HashMap<KeyCode, Vec<VirtualAction>>,
    pad_dir_rev: [Vec<VirtualAction>; 4],
    pad_dir_on_rev: HashMap<(usize, PadDir), Vec<VirtualAction>>,
    pad_code_rev: HashMap<u32, Vec<PadCodeRev>>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self {
            map: HashMap::new(),
            key_rev: new_key_rev(),
            key_rev_extra: HashMap::new(),
            pad_dir_rev: std::array::from_fn(|_| Vec::new()),
            pad_dir_on_rev: HashMap::new(),
            pad_code_rev: HashMap::new(),
        }
    }
}

static KEYMAP: std::sync::LazyLock<RwLock<Keymap>> =
    std::sync::LazyLock::new(|| RwLock::new(Keymap::default()));
static COMPILED_KEYMAP: std::sync::LazyLock<RwLock<Arc<CompiledKeymap>>> =
    std::sync::LazyLock::new(|| RwLock::new(Arc::new(CompiledKeymap::default())));
static ONLY_DEDICATED_MENU_BUTTONS: AtomicBool = AtomicBool::new(false);
static INPUT_DEBOUNCE_SECONDS_BITS: AtomicU32 = AtomicU32::new((0.02f32).to_bits());
static KEYBOARD_DEBOUNCE_STATE: std::sync::LazyLock<Mutex<DebounceStore>> =
    std::sync::LazyLock::new(|| Mutex::new(DebounceStore::new()));
static PAD_DEBOUNCE_STATE: std::sync::LazyLock<Mutex<DebounceStore>> =
    std::sync::LazyLock::new(|| Mutex::new(DebounceStore::new()));

const INPUT_DEBOUNCE_MAX_SECONDS: f32 = 0.2;

#[inline(always)]
fn debounce_caps(km: &Keymap) -> (usize, usize) {
    let key_cap = km
        .key_rev
        .iter()
        .filter(|actions| !actions.is_empty())
        .count()
        + km.key_rev_extra.len();
    let mut pad_cap = km
        .pad_dir_rev
        .iter()
        .filter(|actions| !actions.is_empty())
        .count();
    pad_cap += km.pad_dir_on_rev.len();
    pad_cap += km.pad_code_rev.len();
    (key_cap, pad_cap)
}

#[inline(always)]
fn reset_debounce_state(key_cap: usize, pad_cap: usize) {
    let mut keyboard = KEYBOARD_DEBOUNCE_STATE.lock().unwrap();
    keyboard.clear_and_reserve(key_cap);
    drop(keyboard);

    let mut pad = PAD_DEBOUNCE_STATE.lock().unwrap();
    pad.clear_and_reserve(pad_cap);
}

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
    let (key_cap, pad_cap) = debounce_caps(&new_map);
    let compiled = Arc::new(CompiledKeymap::from_keymap(&new_map));
    *KEYMAP.write().unwrap() = new_map;
    *COMPILED_KEYMAP.write().unwrap() = compiled;
    reset_debounce_state(key_cap, pad_cap);
}

#[inline(always)]
pub fn clear_debounce_state() {
    with_keymap(|km| {
        let (key_cap, pad_cap) = debounce_caps(km);
        reset_debounce_state(key_cap, pad_cap);
    });
}

#[inline(always)]
fn load_compiled_keymap() -> Arc<CompiledKeymap> {
    COMPILED_KEYMAP.read().unwrap().clone()
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
fn debounce_windows() -> DebounceWindows {
    DebounceWindows::uniform(input_debounce_window())
}

// Defaults are provided by config.rs; keep this module free of config.

impl Keymap {
    #[inline(always)]
    fn key_actions(&self, code: KeyCode) -> &[VirtualAction] {
        match dense_key_ix(code) {
            Some(ix) => &self.key_rev[ix],
            None => self.key_rev_extra.get(&code).map_or(&[], Vec::as_slice),
        }
    }

    #[inline(always)]
    fn remove_rev(&mut self, action: VirtualAction, prev: &[InputBinding]) {
        for b in prev {
            match *b {
                InputBinding::Key(code) => {
                    if let Some(ix) = dense_key_ix(code) {
                        let v = &mut self.key_rev[ix];
                        v.retain(|a| *a != action);
                    } else if let Some(v) = self.key_rev_extra.get_mut(&code) {
                        v.retain(|a| *a != action);
                        if v.is_empty() {
                            self.key_rev_extra.remove(&code);
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
                InputBinding::Key(code) => {
                    if let Some(ix) = dense_key_ix(code) {
                        self.key_rev[ix].push(action);
                    } else {
                        self.key_rev_extra.entry(code).or_default().push(action);
                    }
                }
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
    pub fn keycode_mapped(&self, code: KeyCode) -> bool {
        !self.key_actions(code).is_empty()
    }

    #[inline(always)]
    pub fn keycode_has_action(&self, code: KeyCode, keep: impl Fn(VirtualAction) -> bool) -> bool {
        for &action in self.key_actions(code) {
            if keep(action) {
                return true;
            }
        }
        false
    }

    #[inline(always)]
    pub fn raw_key_event_mapped(&self, ev: &RawKeyboardEvent) -> bool {
        self.keycode_mapped(ev.code)
    }

    #[inline(always)]
    pub fn raw_key_event_has_action(
        &self,
        ev: &RawKeyboardEvent,
        keep: impl Fn(VirtualAction) -> bool,
    ) -> bool {
        self.keycode_has_action(ev.code, keep)
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

#[inline(always)]
const fn action_bit(action: VirtualAction) -> u32 {
    1u32 << (action.ix() as u32)
}

#[inline(always)]
const fn action_from_ix(ix: usize) -> VirtualAction {
    match ix {
        0 => VirtualAction::p1_up,
        1 => VirtualAction::p1_down,
        2 => VirtualAction::p1_left,
        3 => VirtualAction::p1_right,
        4 => VirtualAction::p1_start,
        5 => VirtualAction::p1_back,
        6 => VirtualAction::p1_menu_up,
        7 => VirtualAction::p1_menu_down,
        8 => VirtualAction::p1_menu_left,
        9 => VirtualAction::p1_menu_right,
        10 => VirtualAction::p1_select,
        11 => VirtualAction::p1_operator,
        12 => VirtualAction::p1_restart,
        13 => VirtualAction::p2_up,
        14 => VirtualAction::p2_down,
        15 => VirtualAction::p2_left,
        16 => VirtualAction::p2_right,
        17 => VirtualAction::p2_start,
        18 => VirtualAction::p2_back,
        19 => VirtualAction::p2_menu_up,
        20 => VirtualAction::p2_menu_down,
        21 => VirtualAction::p2_menu_left,
        22 => VirtualAction::p2_menu_right,
        23 => VirtualAction::p2_select,
        24 => VirtualAction::p2_operator,
        25 => VirtualAction::p2_restart,
        _ => unreachable!(),
    }
}

#[inline(always)]
fn for_each_action(mut mask: u32, mut f: impl FnMut(VirtualAction)) {
    while mask != 0 {
        let ix = mask.trailing_zeros() as usize;
        f(action_from_ix(ix));
        mask &= mask - 1;
    }
}

#[inline(always)]
fn secondary_menu_mask(mask: u32) -> u32 {
    let mut out = 0;
    for_each_action(mask, |action| {
        if let Some(menu_action) = action.secondary_menu() {
            out |= action_bit(menu_action);
        }
    });
    out
}

#[inline(always)]
fn collect_key_mask_from_compiled(km: &CompiledKeymap, code: KeyCode) -> u32 {
    km.key_mask(code)
}

#[inline(always)]
fn collect_pad_dir_mask_from_compiled(km: &CompiledKeymap, id: PadId, dir: PadDir) -> u32 {
    let dev = usize::from(id);
    km.pad_dir_rev[dir.ix()] | km.pad_dir_on_rev.get(&(dev, dir)).copied().unwrap_or(0)
}

#[inline(always)]
fn collect_pad_button_mask_from_compiled(
    km: &CompiledKeymap,
    id: PadId,
    code: PadCode,
    uuid: [u8; 16],
) -> u32 {
    let Some(entries) = km.pad_code_rev.get(&code.into_u32()) else {
        return 0;
    };
    let dev = usize::from(id);
    let mut mask = 0;
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
        mask |= entry.mask;
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
    direct_mask: u32,
    pressed: bool,
    mut emit: impl FnMut(VirtualAction, bool),
) {
    if direct_mask == 0 {
        return;
    }
    let mut emitted = 0;
    for_each_action(direct_mask, |action| {
        emit_normalized_action(action, pressed, direct_mask, &mut emitted, &mut emit)
    });
    if ONLY_DEDICATED_MENU_BUTTONS.load(Ordering::Relaxed) && pressed {
        return;
    }
    for_each_action(secondary_menu_mask(direct_mask), |action| {
        emit_normalized_action(action, pressed, direct_mask, &mut emitted, &mut emit)
    });
}

fn collect_actions_from_compiled(km: &CompiledKeymap, edge: DebouncedEdge) -> u32 {
    match edge.binding {
        DebounceBinding::Keyboard(code) => collect_key_mask_from_compiled(km, code),
        DebounceBinding::PadDir { id, dir } => collect_pad_dir_mask_from_compiled(km, id, dir),
        DebounceBinding::PadButton { id, code, uuid } => {
            collect_pad_button_mask_from_compiled(km, id, code, uuid)
        }
    }
}

#[inline(always)]
fn emit_input_events_from_edge(
    km: &CompiledKeymap,
    edge: DebouncedEdge,
    mut emit: impl FnMut(InputEvent),
) {
    let mask = collect_actions_from_compiled(km, edge);
    emit_normalized_actions(mask, edge.pressed, |action, pressed| {
        emit(input_event(
            action,
            pressed,
            edge.source,
            edge.timestamp,
            edge.timestamp_host_nanos,
            edge.stored_at,
            edge.emitted_at,
        ));
    });
}

#[inline(always)]
fn emit_debounced_edges(
    km: &CompiledKeymap,
    edges: DebounceEdges,
    mut emit: impl FnMut(InputEvent),
) {
    if let Some(edge) = edges.first {
        emit_input_events_from_edge(km, edge, &mut emit);
    }
    if let Some(edge) = edges.second {
        emit_input_events_from_edge(km, edge, &mut emit);
    }
}

#[inline(always)]
pub fn map_raw_key_event_with(ev: &RawKeyboardEvent, emit: impl FnMut(InputEvent)) {
    if ev.pressed && ev.repeat {
        return;
    }
    let km = load_compiled_keymap();
    if collect_key_mask_from_compiled(km.as_ref(), ev.code) == 0 {
        return;
    }
    let edges = debounce_input_edge_in_store(
        &KEYBOARD_DEBOUNCE_STATE,
        DebounceBinding::Keyboard(ev.code),
        ev.pressed,
        ev.timestamp,
        ev.host_nanos,
        debounce_windows(),
    );
    emit_debounced_edges(km.as_ref(), edges, emit);
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
    let km = load_compiled_keymap();
    let mask = collect_key_mask_from_compiled(km.as_ref(), code);
    emit_normalized_actions(mask, pressed, |action, pressed| {
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
pub fn map_pad_event_with(ev: &PadEvent, mut emit: impl FnMut(InputEvent)) {
    let km = load_compiled_keymap();
    let edges = match *ev {
        PadEvent::Dir {
            id,
            dir,
            pressed,
            timestamp,
            host_nanos,
        } => {
            if collect_pad_dir_mask_from_compiled(km.as_ref(), id, dir) == 0 {
                return;
            }
            debounce_input_edge_in_store(
                &PAD_DEBOUNCE_STATE,
                DebounceBinding::PadDir { id, dir },
                pressed,
                timestamp,
                host_nanos,
                debounce_windows(),
            )
        }
        PadEvent::RawButton {
            id,
            code,
            uuid,
            pressed,
            timestamp,
            host_nanos,
            ..
        } => {
            if collect_pad_button_mask_from_compiled(km.as_ref(), id, code, uuid) == 0 {
                return;
            }
            debounce_input_edge_in_store(
                &PAD_DEBOUNCE_STATE,
                DebounceBinding::PadButton { id, code, uuid },
                pressed,
                timestamp,
                host_nanos,
                debounce_windows(),
            )
        }
        PadEvent::RawAxis { .. } => return,
    };
    emit_debounced_edges(km.as_ref(), edges, &mut emit);
}

pub fn drain_debounced_input_events_with(mut emit: impl FnMut(InputEvent)) -> bool {
    let km = load_compiled_keymap();
    let now = Instant::now();
    let mut flushed =
        emit_due_debounce_edges_from(&KEYBOARD_DEBOUNCE_STATE, now, debounce_windows(), |edge| {
            emit_input_events_from_edge(km.as_ref(), edge, &mut emit)
        });
    flushed |= emit_due_debounce_edges_from(&PAD_DEBOUNCE_STATE, now, debounce_windows(), |edge| {
        emit_input_events_from_edge(km.as_ref(), edge, &mut emit)
    });
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

    fn lock_test_guard() -> std::sync::MutexGuard<'static, ()> {
        TEST_GUARD
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    struct TestReset(Option<Keymap>);

    impl TestReset {
        fn capture() -> Self {
            Self(Some(get_keymap()))
        }
    }

    impl Drop for TestReset {
        fn drop(&mut self) {
            if let Some(original) = self.0.take() {
                set_only_dedicated_menu_buttons(false);
                set_keymap(original);
            }
        }
    }

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
    fn map_keycode_event_with_emits_primary_action_for_pressed_arrow() {
        let _guard = lock_test_guard();
        let _reset = TestReset::capture();
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
    }

    #[test]
    fn map_pad_event_with_emits_primary_action_for_pressed_arrow() {
        let _guard = lock_test_guard();
        let _reset = TestReset::capture();
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
        assert_eq!(actual.len(), 1, "event count");
        let actual = actual[0];
        assert_eq!(actual.action, VirtualAction::p1_left);
        assert!(actual.pressed);
        assert_eq!(actual.source, InputSource::Gamepad);
        assert_eq!(actual.timestamp, timestamp);
        assert_eq!(actual.timestamp_host_nanos, 42);
        assert!(
            actual.stored_at >= timestamp,
            "debounce storage time should not precede the raw pad timestamp"
        );
        assert_eq!(
            actual.emitted_at, actual.stored_at,
            "initial pad press should emit immediately from the debounce store"
        );
    }

    #[test]
    fn map_keycode_event_with_suppresses_pressed_alias_when_primary_is_bound() {
        let _guard = lock_test_guard();
        let _reset = TestReset::capture();
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
    }

    #[test]
    fn map_keycode_event_with_keeps_release_alias_in_dedicated_mode() {
        let _guard = lock_test_guard();
        let _reset = TestReset::capture();
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
    }

    #[test]
    fn keycode_has_action_matches_without_allocating_action_vec() {
        let _guard = lock_test_guard();
        let _reset = TestReset::capture();
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
    }

    #[test]
    fn pad_event_mapped_checks_device_and_uuid_without_allocating_action_vec() {
        let _guard = lock_test_guard();
        let _reset = TestReset::capture();
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
    }

    #[test]
    fn map_raw_key_event_with_skips_unmapped_keys_before_debounce() {
        let _guard = lock_test_guard();
        let _reset = TestReset::capture();
        let mut km = Keymap::default();
        km.bind(
            VirtualAction::p1_back,
            &[InputBinding::Key(KeyCode::Escape)],
        );
        set_keymap(km);

        let raw = RawKeyboardEvent {
            code: KeyCode::ArrowLeft,
            pressed: true,
            repeat: false,
            timestamp: Instant::now(),
            host_nanos: 123,
        };
        let mut actual = Vec::new();
        map_raw_key_event_with(&raw, |event| actual.push(event));

        assert!(actual.is_empty());
        assert_eq!(KEYBOARD_DEBOUNCE_STATE.lock().unwrap().len(), 0);
    }

    #[test]
    fn map_pad_event_with_skips_unmapped_pad_buttons_before_debounce() {
        let _guard = lock_test_guard();
        let _reset = TestReset::capture();
        let mut km = Keymap::default();
        km.bind(
            VirtualAction::p1_left,
            &[InputBinding::PadDir(PadDir::Left)],
        );
        set_keymap(km);

        let pad = PadEvent::RawButton {
            id: PadId(1),
            timestamp: Instant::now(),
            host_nanos: 456,
            code: PadCode(77),
            uuid: [7; 16],
            value: 1.0,
            pressed: true,
        };
        let mut actual = Vec::new();
        map_pad_event_with(&pad, |event| actual.push(event));

        assert!(actual.is_empty());
        assert_eq!(PAD_DEBOUNCE_STATE.lock().unwrap().len(), 0);
    }

    #[test]
    fn map_raw_key_event_with_debounces_shared_arrow_input() {
        let _guard = lock_test_guard();
        let _reset = TestReset::capture();
        let mut km = Keymap::default();
        km.bind(
            VirtualAction::p1_left,
            &[InputBinding::Key(KeyCode::ArrowLeft)],
        );
        set_keymap(km);
        set_only_dedicated_menu_buttons(false);

        let t0 = Instant::now();
        let press = RawKeyboardEvent {
            code: KeyCode::ArrowLeft,
            pressed: true,
            repeat: false,
            timestamp: t0,
            host_nanos: 100,
        };
        let release = RawKeyboardEvent {
            code: KeyCode::ArrowLeft,
            pressed: false,
            repeat: false,
            timestamp: t0 + Duration::from_millis(1),
            host_nanos: 101,
        };
        let repress = RawKeyboardEvent {
            code: KeyCode::ArrowLeft,
            pressed: true,
            repeat: false,
            timestamp: t0 + Duration::from_millis(5),
            host_nanos: 105,
        };

        let mut actual = Vec::new();
        map_raw_key_event_with(&press, |event| actual.push(event));
        assert_eq!(actual.len(), 1, "press event count");
        assert_eq!(actual[0].action, VirtualAction::p1_left);
        assert!(actual[0].pressed);
        assert_eq!(actual[0].source, InputSource::Keyboard);
        assert_eq!(actual[0].timestamp, t0);
        assert_eq!(actual[0].timestamp_host_nanos, 100);

        actual.clear();
        map_raw_key_event_with(&release, |event| actual.push(event));
        assert!(
            actual.is_empty(),
            "release inside debounce window should be delayed"
        );

        map_raw_key_event_with(&repress, |event| actual.push(event));
        assert!(
            actual.is_empty(),
            "quick release/repress chatter should not escape the shared debounce path"
        );
    }

    #[test]
    fn set_keymap_presizes_debounce_state() {
        let _guard = lock_test_guard();
        let _reset = TestReset::capture();
        let mut km = Keymap::default();
        km.bind(
            VirtualAction::p1_left,
            &[
                InputBinding::Key(KeyCode::ArrowLeft),
                InputBinding::PadDir(PadDir::Left),
            ],
        );
        km.bind(
            VirtualAction::p1_down,
            &[InputBinding::PadDirOn {
                device: 2,
                dir: PadDir::Down,
            }],
        );
        km.bind(
            VirtualAction::p1_up,
            &[InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32: 77,
                device: None,
                uuid: None,
            })],
        );
        km.bind(
            VirtualAction::p2_right,
            &[InputBinding::Key(KeyCode::Numpad6)],
        );
        let (key_cap, pad_cap) = debounce_caps(&km);

        set_keymap(km);

        assert!(
            KEYBOARD_DEBOUNCE_STATE.lock().unwrap().capacity() >= key_cap,
            "keyboard debounce store should be pre-sized for mapped keys"
        );
        assert!(
            PAD_DEBOUNCE_STATE.lock().unwrap().capacity() >= pad_cap,
            "pad debounce store should be pre-sized for mapped bindings"
        );
    }
}
