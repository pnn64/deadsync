use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

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
static INPUT_DEBOUNCE_STATE: std::sync::LazyLock<Mutex<HashMap<DebounceBinding, DebounceState>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

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
    INPUT_DEBOUNCE_STATE.lock().unwrap().clear();
}

#[inline(always)]
pub fn clear_debounce_state() {
    INPUT_DEBOUNCE_STATE.lock().unwrap().clear();
}

#[inline(always)]
pub fn set_only_dedicated_menu_buttons(enabled: bool) {
    ONLY_DEDICATED_MENU_BUTTONS.store(enabled, Ordering::Relaxed);
}

#[inline(always)]
pub fn set_input_debounce_seconds(seconds: f32) {
    let clamped = seconds.clamp(0.0, INPUT_DEBOUNCE_MAX_SECONDS);
    INPUT_DEBOUNCE_SECONDS_BITS.store(clamped.to_bits(), Ordering::Relaxed);
}

#[inline(always)]
pub fn input_debounce_seconds() -> f32 {
    f32::from_bits(INPUT_DEBOUNCE_SECONDS_BITS.load(Ordering::Relaxed))
}

#[inline(always)]
fn input_debounce_window() -> Duration {
    Duration::from_secs_f32(input_debounce_seconds())
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
    pub timestamp: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum DebounceBinding {
    Keyboard(KeyCode),
    PadDir {
        id: PadId,
        dir: PadDir,
    },
    PadButton {
        id: PadId,
        code: PadCode,
        uuid: [u8; 16],
    },
}

#[derive(Clone, Copy, Debug)]
struct DebounceState {
    held_raw: bool,
    held_reported: bool,
    last_raw_change_time: Instant,
    last_report_time: Instant,
}

#[derive(Clone, Copy, Debug)]
struct DebouncedEdge {
    binding: DebounceBinding,
    pressed: bool,
    source: InputSource,
    timestamp: Instant,
}

#[inline(always)]
fn debounce_emit_if_due(
    state: &mut DebounceState,
    now: Instant,
    window: Duration,
) -> Option<(bool, Instant)> {
    if state.held_raw == state.held_reported || now.duration_since(state.last_report_time) < window
    {
        return None;
    }
    state.last_report_time = now;
    state.held_reported = state.held_raw;
    Some((state.held_reported, state.last_raw_change_time))
}

#[inline(always)]
fn debounce_binding_source(binding: DebounceBinding) -> InputSource {
    match binding {
        DebounceBinding::Keyboard(_) => InputSource::Keyboard,
        DebounceBinding::PadDir { .. } | DebounceBinding::PadButton { .. } => InputSource::Gamepad,
    }
}

#[inline(always)]
fn debounce_binding_actions(
    km: &Keymap,
    binding: DebounceBinding,
    pressed: bool,
) -> Vec<(VirtualAction, bool)> {
    match binding {
        DebounceBinding::Keyboard(code) => km.actions_for_key_code(code, pressed),
        DebounceBinding::PadDir { id, dir } => km.actions_for_pad_dir(id, dir, pressed),
        DebounceBinding::PadButton { id, code, uuid } => {
            km.actions_for_pad_button(id, code, uuid, pressed)
        }
    }
}

#[inline(always)]
fn should_prune_debounce_state(state: DebounceState, now: Instant, window: Duration) -> bool {
    !state.held_raw && !state.held_reported && now.duration_since(state.last_report_time) >= window
}

fn debounce_input_edge(
    binding: DebounceBinding,
    pressed: bool,
    timestamp: Instant,
) -> Option<DebouncedEdge> {
    let now = Instant::now();
    let window = input_debounce_window();
    let mut states = INPUT_DEBOUNCE_STATE.lock().unwrap();
    let (edge, prune) = {
        let state = states.entry(binding).or_insert_with(|| DebounceState {
            held_raw: false,
            held_reported: false,
            last_raw_change_time: timestamp,
            last_report_time: now.checked_sub(window).unwrap_or(now),
        });
        state.held_raw = pressed;
        state.last_raw_change_time = timestamp;
        let edge =
            debounce_emit_if_due(state, now, window).map(|(debounced_pressed, ts)| DebouncedEdge {
                binding,
                pressed: debounced_pressed,
                source: debounce_binding_source(binding),
                timestamp: ts,
            });
        (edge, should_prune_debounce_state(*state, now, window))
    };
    if prune {
        states.remove(&binding);
    }
    edge
}

fn collect_due_debounce_edges(now: Instant) -> Vec<DebouncedEdge> {
    let window = input_debounce_window();
    let mut states = INPUT_DEBOUNCE_STATE.lock().unwrap();
    let mut out: Vec<DebouncedEdge> = Vec::with_capacity(states.len().min(8));
    let mut prune: Vec<DebounceBinding> = Vec::new();
    for (&binding, state) in states.iter_mut() {
        if let Some((pressed, timestamp)) = debounce_emit_if_due(state, now, window) {
            out.push(DebouncedEdge {
                binding,
                pressed,
                source: debounce_binding_source(binding),
                timestamp,
            });
        }
        if should_prune_debounce_state(*state, now, window) {
            prune.push(binding);
        }
    }
    for binding in prune {
        states.remove(&binding);
    }
    out
}

#[inline(always)]
fn input_events_from_debounced_edge(edge: DebouncedEdge) -> Vec<InputEvent> {
    let mut actions = with_keymap(|km| debounce_binding_actions(km, edge.binding, edge.pressed));
    append_secondary_menu_actions(&mut actions);
    dedup_primary_vs_menu_alias(&mut actions);
    dedup_action_pairs(&mut actions);
    actions
        .into_iter()
        .map(|(action, pressed)| InputEvent {
            action,
            pressed,
            source: edge.source,
            timestamp: edge.timestamp,
        })
        .collect()
}

#[inline(always)]
pub fn map_key_event(ev: &KeyEvent) -> Vec<InputEvent> {
    // Ignore OS key auto-repeat for pressed events (prevents resetting hold timers)
    if ev.state == ElementState::Pressed && ev.repeat {
        return Vec::new();
    }
    let PhysicalKey::Code(code) = ev.physical_key else {
        return Vec::new();
    };
    let pressed = ev.state == ElementState::Pressed;
    if let Some(edge) =
        debounce_input_edge(DebounceBinding::Keyboard(code), pressed, Instant::now())
    {
        return input_events_from_debounced_edge(edge);
    }
    Vec::new()
}

#[inline(always)]
pub fn map_pad_event(ev: &PadEvent) -> Vec<InputEvent> {
    let edge = match *ev {
        PadEvent::Dir {
            id,
            timestamp,
            dir,
            pressed,
        } => debounce_input_edge(DebounceBinding::PadDir { id, dir }, pressed, timestamp),
        PadEvent::RawButton {
            id,
            timestamp,
            code,
            uuid,
            pressed,
            ..
        } => debounce_input_edge(
            DebounceBinding::PadButton { id, code, uuid },
            pressed,
            timestamp,
        ),
        PadEvent::RawAxis { timestamp, .. } => {
            let _ = timestamp;
            None
        }
    };
    if let Some(edge) = edge {
        return input_events_from_debounced_edge(edge);
    }
    Vec::new()
}

pub fn drain_debounced_events() -> Vec<InputEvent> {
    let mut out: Vec<InputEvent> = Vec::with_capacity(4);
    for edge in collect_due_debounce_edges(Instant::now()) {
        out.extend(input_events_from_debounced_edge(edge));
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
fn dedup_action_pairs(actions: &mut Vec<(VirtualAction, bool)>) {
    let mut seen_pressed: u32 = 0;
    let mut seen_released: u32 = 0;
    actions.retain(|(act, pressed)| {
        let bit = 1u32 << (act.ix() as u32);
        if *pressed {
            let keep = (seen_pressed & bit) == 0;
            seen_pressed |= bit;
            keep
        } else {
            let keep = (seen_released & bit) == 0;
            seen_released |= bit;
            keep
        }
    });
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

#[inline(always)]
fn dedup_primary_vs_menu_alias(actions: &mut Vec<(VirtualAction, bool)>) {
    let mut pressed: u32 = 0;
    for (act, is_pressed) in actions.iter().copied() {
        let bit = 1u32 << (act.ix() as u32);
        if is_pressed {
            pressed |= bit;
        }
    }
    actions.retain(|(act, is_pressed)| {
        let Some(primary) = primary_from_menu_alias(*act) else {
            return true;
        };
        if !*is_pressed {
            // Keep release aliases so dedicated-menu filtering can still
            // propagate a menu release if a primary action is suppressed.
            return true;
        }
        let bit = 1u32 << (primary.ix() as u32);
        (pressed & bit) == 0
    });
}

#[inline(always)]
fn append_secondary_menu_actions(actions: &mut Vec<(VirtualAction, bool)>) {
    let only_dedicated = ONLY_DEDICATED_MENU_BUTTONS.load(Ordering::Relaxed);
    let original_len = actions.len();
    for i in 0..original_len {
        let (act, pressed) = actions[i];
        // Keep releasing secondary menu aliases even in dedicated-only mode so
        // menu hold/repeat state cannot get orphaned if the preference changes
        // while an arrow is held.
        if let Some(menu_act) = act.secondary_menu()
            && (!only_dedicated || !pressed)
        {
            actions.push((menu_act, pressed));
        }
    }
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
