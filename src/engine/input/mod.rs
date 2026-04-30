use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use winit::keyboard::KeyCode;

mod backends;
mod debounce;

use debounce::{
    DebounceEdges, DebounceStore, DebounceWindows, DebouncedEdge, debounce_input_edge_in_store_mut,
    emit_due_debounce_edges_from_mut,
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
    // Integer song time for this edge, in nanoseconds. Gameplay treats this as
    // the authoritative judgment-time clock and reconstructs seconds only at
    // presentation/logging boundaries.
    pub event_music_time_ns: i64,
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

const UNMAPPED_DEBOUNCE_SLOT: u32 = u32::MAX;

#[derive(Clone, Copy, Debug)]
struct CompiledBindingRev {
    mask: u32,
    slot: u32,
}

impl CompiledBindingRev {
    const UNMAPPED: Self = Self {
        mask: 0,
        slot: UNMAPPED_DEBOUNCE_SLOT,
    };

    #[inline(always)]
    const fn mapped(self) -> bool {
        self.mask != 0
    }
}

impl Default for CompiledBindingRev {
    fn default() -> Self {
        Self::UNMAPPED
    }
}

#[inline(always)]
fn new_compiled_key_rev() -> Box<[CompiledBindingRev]> {
    vec![CompiledBindingRev::UNMAPPED; KEY_CODE_CAP].into_boxed_slice()
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
struct CompiledPadCodeMap {
    slot: u32,
    entries: Vec<CompiledPadCodeRev>,
}

#[derive(Clone, Debug)]
struct CompiledKeymap {
    key_rev: Box<[CompiledBindingRev]>,
    key_rev_extra: HashMap<KeyCode, CompiledBindingRev>,
    pad_dir_rev: [u32; 4],
    pad_dir_on_rev: HashMap<(usize, PadDir), u32>,
    pad_code_rev: HashMap<u32, CompiledPadCodeMap>,
    key_slot_count: usize,
    pad_stride: usize,
    pad_slot_count: usize,
}

impl Default for CompiledKeymap {
    fn default() -> Self {
        Self {
            key_rev: new_compiled_key_rev(),
            key_rev_extra: HashMap::new(),
            pad_dir_rev: [0; 4],
            pad_dir_on_rev: HashMap::new(),
            pad_code_rev: HashMap::new(),
            key_slot_count: 0,
            pad_stride: 4,
            pad_slot_count: 0,
        }
    }
}

impl CompiledKeymap {
    #[inline(always)]
    fn from_keymap(km: &Keymap) -> Self {
        let mut key_rev = new_compiled_key_rev();
        let mut next_key_slot = 0u32;
        for (ix, actions) in km.key_rev.iter().enumerate() {
            if actions.is_empty() {
                continue;
            }
            let mut mask = 0;
            for &action in actions {
                mask |= action_bit(action);
            }
            key_rev[ix] = CompiledBindingRev {
                mask,
                slot: next_key_slot,
            };
            next_key_slot = next_key_slot.saturating_add(1);
        }
        let mut key_rev_extra = HashMap::with_capacity(km.key_rev_extra.len());
        for (&code, actions) in &km.key_rev_extra {
            let mut mask = 0;
            for &action in actions {
                mask |= action_bit(action);
            }
            key_rev_extra.insert(
                code,
                CompiledBindingRev {
                    mask,
                    slot: next_key_slot,
                },
            );
            next_key_slot = next_key_slot.saturating_add(1);
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
        let mut max_pad_device: Option<usize> = None;
        for (&key, actions) in &km.pad_dir_on_rev {
            let mut mask = 0;
            for &action in actions {
                mask |= action_bit(action);
            }
            max_pad_device = Some(max_pad_device.map_or(key.0, |max| max.max(key.0)));
            pad_dir_on_rev.insert(key, mask);
        }
        let mut pad_code_rev = HashMap::with_capacity(km.pad_code_rev.len());
        let mut next_pad_button_slot = 0u32;
        for (&code, entries) in &km.pad_code_rev {
            let mut compiled_entries = Vec::with_capacity(entries.len());
            for entry in entries {
                if let Some(existing) =
                    compiled_entries
                        .iter_mut()
                        .find(|item: &&mut CompiledPadCodeRev| {
                            item.device == entry.device && item.uuid == entry.uuid
                        })
                {
                    existing.mask |= action_bit(entry.act);
                    continue;
                }
                compiled_entries.push(CompiledPadCodeRev {
                    mask: action_bit(entry.act),
                    device: entry.device,
                    uuid: entry.uuid,
                });
                if let Some(device) = entry.device {
                    max_pad_device = Some(max_pad_device.map_or(device, |max| max.max(device)));
                }
            }
            pad_code_rev.insert(
                code,
                CompiledPadCodeMap {
                    slot: next_pad_button_slot,
                    entries: compiled_entries,
                },
            );
            next_pad_button_slot = next_pad_button_slot.saturating_add(1);
        }
        let pad_stride = 4 + next_pad_button_slot as usize;
        let has_pad_bindings = pad_dir_rev.iter().any(|&mask| mask != 0)
            || !pad_dir_on_rev.is_empty()
            || !pad_code_rev.is_empty();
        let pad_slot_count = if has_pad_bindings {
            pad_stride.saturating_mul(max_pad_device.map_or(1, |max| max.saturating_add(1)))
        } else {
            0
        };
        Self {
            key_rev,
            key_rev_extra,
            pad_dir_rev,
            pad_dir_on_rev,
            pad_code_rev,
            key_slot_count: next_key_slot as usize,
            pad_stride,
            pad_slot_count,
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
static COMPILED_KEYMAP: std::sync::LazyLock<RwLock<CompiledKeymap>> =
    std::sync::LazyLock::new(|| RwLock::new(CompiledKeymap::default()));
static COMPILED_KEYMAP_GEN: AtomicU64 = AtomicU64::new(1);
static ONLY_DEDICATED_MENU_BUTTONS: AtomicBool = AtomicBool::new(false);
static INPUT_DEBOUNCE_SECONDS_BITS: AtomicU32 = AtomicU32::new((0.02f32).to_bits());

thread_local! {
    static THREAD_COMPILED_KEYMAP: RefCell<(u64, CompiledKeymap)> =
        RefCell::new((0, CompiledKeymap::default()));
    // Input mapping/draining is app-thread work; keep debounce state local to
    // that thread instead of paying a mutex on every raw input event.
    static THREAD_KEYBOARD_DEBOUNCE_STATE: RefCell<DebounceStore> =
        RefCell::new(DebounceStore::new());
    static THREAD_PAD_DEBOUNCE_STATE: RefCell<DebounceStore> =
        RefCell::new(DebounceStore::new());
}

const INPUT_DEBOUNCE_MAX_SECONDS: f32 = 0.2;

#[inline(always)]
fn reset_debounce_state(key_slot_count: usize, pad_slot_count: usize) {
    THREAD_KEYBOARD_DEBOUNCE_STATE.with(|states| states.borrow_mut().prepare_slots(key_slot_count));
    THREAD_PAD_DEBOUNCE_STATE.with(|states| states.borrow_mut().clear_and_reserve(pad_slot_count));
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
    let compiled = CompiledKeymap::from_keymap(&new_map);
    let key_slot_count = compiled.key_slot_count;
    let pad_slot_count = compiled.pad_slot_count;
    *KEYMAP.write().unwrap() = new_map;
    *COMPILED_KEYMAP.write().unwrap() = compiled;
    reset_debounce_state(key_slot_count, pad_slot_count);
    COMPILED_KEYMAP_GEN.fetch_add(1, Ordering::Release);
}

#[inline(always)]
pub fn clear_debounce_state() {
    let (key_slot_count, pad_slot_count) =
        with_compiled_keymap(|compiled| (compiled.key_slot_count, compiled.pad_slot_count));
    reset_debounce_state(key_slot_count, pad_slot_count);
}

#[inline(always)]
fn with_compiled_keymap<R>(f: impl FnOnce(&CompiledKeymap) -> R) -> R {
    let generation = COMPILED_KEYMAP_GEN.load(Ordering::Acquire);
    THREAD_COMPILED_KEYMAP.with(|local| {
        if local.borrow().0 != generation {
            *local.borrow_mut() = (generation, COMPILED_KEYMAP.read().unwrap().clone());
        }
        f(&local.borrow().1)
    })
}

#[inline(always)]
pub fn set_only_dedicated_menu_buttons(enabled: bool) {
    ONLY_DEDICATED_MENU_BUTTONS.store(enabled, Ordering::Relaxed);
}

#[inline(always)]
fn player_has_action_set(actions: &[VirtualAction]) -> bool {
    with_keymap(|km| {
        actions
            .iter()
            .all(|action| km.binding_at(*action, 0).is_some())
    })
}

/// Returns `true` if at least one player has dedicated left/right menu buttons
/// plus Start bound.
pub fn any_player_has_three_key_menu_buttons() -> bool {
    player_has_action_set(&[
        VirtualAction::p1_menu_left,
        VirtualAction::p1_menu_right,
        VirtualAction::p1_start,
    ]) || player_has_action_set(&[
        VirtualAction::p2_menu_left,
        VirtualAction::p2_menu_right,
        VirtualAction::p2_start,
    ])
}

/// Returns `true` if at least one player has all four dedicated menu
/// directional buttons (menu_up, menu_down, menu_left, menu_right) bound.
pub fn any_player_has_four_way_menu_buttons() -> bool {
    player_has_action_set(&[
        VirtualAction::p1_menu_up,
        VirtualAction::p1_menu_down,
        VirtualAction::p1_menu_left,
        VirtualAction::p1_menu_right,
    ]) || player_has_action_set(&[
        VirtualAction::p2_menu_up,
        VirtualAction::p2_menu_down,
        VirtualAction::p2_menu_left,
        VirtualAction::p2_menu_right,
    ])
}

#[inline(always)]
pub fn any_player_has_dedicated_menu_buttons_for_mode(three_key_navigation: bool) -> bool {
    if three_key_navigation {
        any_player_has_three_key_menu_buttons()
    } else {
        any_player_has_four_way_menu_buttons()
    }
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
fn collect_key_binding_from_compiled(
    km: &CompiledKeymap,
    code: KeyCode,
) -> Option<CompiledBindingRev> {
    let binding = match dense_key_ix(code) {
        Some(ix) => km.key_rev[ix],
        None => km
            .key_rev_extra
            .get(&code)
            .copied()
            .unwrap_or(CompiledBindingRev::UNMAPPED),
    };
    binding.mapped().then_some(binding)
}

#[inline(always)]
fn collect_pad_dir_mask_from_compiled(km: &CompiledKeymap, id: PadId, dir: PadDir) -> u32 {
    let dev = usize::from(id);
    km.pad_dir_rev[dir.ix()] | km.pad_dir_on_rev.get(&(dev, dir)).copied().unwrap_or(0)
}

#[inline(always)]
fn collect_pad_button_binding_from_compiled(
    km: &CompiledKeymap,
    id: PadId,
    code: PadCode,
    uuid: [u8; 16],
) -> Option<CompiledBindingRev> {
    let Some(code_map) = km.pad_code_rev.get(&code.into_u32()) else {
        return None;
    };
    let dev = usize::from(id);
    let mut mask = 0;
    for entry in &code_map.entries {
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
    if mask == 0 {
        return None;
    }
    Some(CompiledBindingRev {
        mask,
        slot: code_map.slot,
    })
}

#[inline(always)]
fn pad_slot_base(km: &CompiledKeymap, id: PadId) -> usize {
    usize::from(id).saturating_mul(km.pad_stride)
}

#[inline(always)]
fn pad_dir_slot_from_compiled(km: &CompiledKeymap, id: PadId, dir: PadDir) -> usize {
    pad_slot_base(km, id).saturating_add(dir.ix())
}

#[inline(always)]
fn pad_button_slot_from_compiled(km: &CompiledKeymap, id: PadId, code_slot: u32) -> usize {
    pad_slot_base(km, id)
        .saturating_add(4)
        .saturating_add(code_slot as usize)
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

fn emit_input_events_from_edge(edge: DebouncedEdge, mut emit: impl FnMut(InputEvent)) {
    emit_normalized_actions(edge.action_mask, edge.pressed, |action, pressed| {
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
fn emit_debounced_edges(edges: DebounceEdges, mut emit: impl FnMut(InputEvent)) {
    if let Some(edge) = edges.first {
        emit_input_events_from_edge(edge, &mut emit);
    }
    if let Some(edge) = edges.second {
        emit_input_events_from_edge(edge, &mut emit);
    }
}

#[inline(always)]
pub fn map_raw_key_event_with(ev: &RawKeyboardEvent, emit: impl FnMut(InputEvent)) {
    if ev.pressed && ev.repeat {
        return;
    }
    let Some(binding) = with_compiled_keymap(|km| collect_key_binding_from_compiled(km, ev.code))
    else {
        return;
    };
    let edges = THREAD_KEYBOARD_DEBOUNCE_STATE.with(|states| {
        debounce_input_edge_in_store_mut(
            &mut states.borrow_mut(),
            binding.slot as usize,
            binding.mask,
            InputSource::Keyboard,
            ev.pressed,
            ev.timestamp,
            ev.host_nanos,
            debounce_windows(),
        )
    });
    emit_debounced_edges(edges, emit);
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
    let Some(binding) = with_compiled_keymap(|km| collect_key_binding_from_compiled(km, code))
    else {
        return;
    };
    emit_normalized_actions(binding.mask, pressed, |action, pressed| {
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
    let Some((slot, mask, pressed, timestamp, host_nanos)) = with_compiled_keymap(|km| match *ev {
        PadEvent::Dir {
            id,
            dir,
            pressed,
            timestamp,
            host_nanos,
        } => {
            let mask = collect_pad_dir_mask_from_compiled(km, id, dir);
            if mask == 0 {
                return None;
            }
            Some((
                pad_dir_slot_from_compiled(km, id, dir),
                mask,
                pressed,
                timestamp,
                host_nanos,
            ))
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
            let Some(binding) = collect_pad_button_binding_from_compiled(km, id, code, uuid) else {
                return None;
            };
            Some((
                pad_button_slot_from_compiled(km, id, binding.slot),
                binding.mask,
                pressed,
                timestamp,
                host_nanos,
            ))
        }
        PadEvent::RawAxis { .. } => None,
    }) else {
        return;
    };
    let edges = THREAD_PAD_DEBOUNCE_STATE.with(|states| {
        debounce_input_edge_in_store_mut(
            &mut states.borrow_mut(),
            slot,
            mask,
            InputSource::Gamepad,
            pressed,
            timestamp,
            host_nanos,
            debounce_windows(),
        )
    });
    emit_debounced_edges(edges, &mut emit);
}

pub fn drain_debounced_input_events_with(mut emit: impl FnMut(InputEvent)) -> bool {
    let now = Instant::now();
    let mut flushed = THREAD_KEYBOARD_DEBOUNCE_STATE.with(|states| {
        emit_due_debounce_edges_from_mut(
            &mut states.borrow_mut(),
            now,
            debounce_windows(),
            |edge| emit_input_events_from_edge(edge, &mut emit),
        )
    });
    flushed |= THREAD_PAD_DEBOUNCE_STATE.with(|states| {
        emit_due_debounce_edges_from_mut(
            &mut states.borrow_mut(),
            now,
            debounce_windows(),
            |edge| emit_input_events_from_edge(edge, &mut emit),
        )
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

    static TEST_GUARD: std::sync::LazyLock<std::sync::Mutex<()>> =
        std::sync::LazyLock::new(|| std::sync::Mutex::new(()));

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
    fn dedicated_menu_button_capabilities_distinguish_three_key_from_four_way() {
        let _guard = lock_test_guard();
        let _reset = TestReset::capture();
        let mut km = Keymap::default();
        km.bind(
            VirtualAction::p1_menu_left,
            &[InputBinding::Key(KeyCode::KeyA)],
        );
        km.bind(
            VirtualAction::p1_menu_right,
            &[InputBinding::Key(KeyCode::KeyD)],
        );
        km.bind(
            VirtualAction::p1_start,
            &[InputBinding::Key(KeyCode::Enter)],
        );
        set_keymap(km);

        assert!(any_player_has_three_key_menu_buttons());
        assert!(!any_player_has_four_way_menu_buttons());
        assert!(any_player_has_dedicated_menu_buttons_for_mode(true));
        assert!(!any_player_has_dedicated_menu_buttons_for_mode(false));
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
    }

    #[test]
    fn map_pad_event_with_ignores_duplicate_raw_button_state() {
        let _guard = lock_test_guard();
        let _reset = TestReset::capture();
        let mut km = Keymap::default();
        km.bind(
            VirtualAction::p1_left,
            &[InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32: 77,
                device: Some(1),
                uuid: Some([7; 16]),
            })],
        );
        set_keymap(km);

        let t0 = Instant::now();
        let press = PadEvent::RawButton {
            id: PadId(1),
            timestamp: t0,
            host_nanos: 456,
            code: PadCode(77),
            uuid: [7; 16],
            value: 1.0,
            pressed: true,
        };
        let repeat_press = PadEvent::RawButton {
            id: PadId(1),
            timestamp: t0 + Duration::from_millis(1),
            host_nanos: 457,
            code: PadCode(77),
            uuid: [7; 16],
            value: 1.0,
            pressed: true,
        };

        let mut actual = Vec::new();
        map_pad_event_with(&press, |event| actual.push(event));
        assert_eq!(actual.len(), 1, "initial press should emit once");
        assert_eq!(actual[0].action, VirtualAction::p1_left);
        assert!(actual[0].pressed);

        actual.clear();
        map_pad_event_with(&repeat_press, |event| actual.push(event));
        assert!(
            actual.is_empty(),
            "duplicate raw button state should be suppressed by shared debounce"
        );
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
    fn set_keymap_prepares_dense_debounce_slots() {
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

        set_keymap(km);
        let (key_slot_count, pad_stride, pad_slot_count) = with_compiled_keymap(|compiled| {
            (
                compiled.key_slot_count,
                compiled.pad_stride,
                compiled.pad_slot_count,
            )
        });

        assert_eq!(key_slot_count, 2);
        assert_eq!(pad_stride, 5);
        assert_eq!(pad_slot_count, 15);
    }
}
