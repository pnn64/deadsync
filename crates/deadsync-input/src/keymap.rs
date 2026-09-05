//! Configurable bindings and allocation-free logical input mapping.
//!
//! [`InputState::new`] compiles and preallocates the mutable state needed by the
//! event path. Events processed by that application-owned state
//! map, debounce, normalize, and drain without heap allocation for keyboard
//! input and native pad IDs below [`crate::PAD_ID_COUNT_CAP`]. Reconfiguration
//! remains an intentionally cold path.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use rustc_hash::{FxBuildHasher, FxHashMap};
use winit::keyboard::KeyCode;

use crate::debounce::{
    DebounceEdges, DebounceStore, DebounceWindows, DebouncedEdge, debounce_input_edge_in_store_mut,
    next_due_edge,
};
use crate::{
    GamepadCodeBinding, InputEvent, PAD_ID_COUNT_CAP, PadCode, PadDir, PadEvent, PadId,
    RawKeyboardEvent, SYSTEM_ACTION_MASK, VirtualAction, clamp_input_debounce_seconds,
    normalized_actions,
};
use deadsync_core::input::InputSource;

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
    system_mask: u32,
    slot: u32,
}

impl CompiledBindingRev {
    const UNMAPPED: Self = Self {
        mask: 0,
        system_mask: 0,
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
    wildcard_mask: u32,
    device_masks: Box<[u32]>,
    entries: Box<[CompiledPadCodeRev]>,
}

#[inline]
fn compile_pad_code_map(entries: &[PadCodeRev], slot: u32) -> CompiledPadCodeMap {
    let mut wildcard_mask = 0;
    let device_mask_len = entries
        .iter()
        .filter(|entry| {
            !entry.act.is_system()
                && entry.uuid.is_none()
                && entry.device.is_some_and(|device| device < PAD_ID_COUNT_CAP)
        })
        .filter_map(|entry| entry.device)
        .max()
        .map_or(0, |device| device + 1);
    let mut device_masks = vec![0; device_mask_len].into_boxed_slice();
    let specific_capacity = entries
        .iter()
        .filter(|entry| {
            !entry.act.is_system()
                && (entry.uuid.is_some()
                    || entry
                        .device
                        .is_some_and(|device| device >= PAD_ID_COUNT_CAP))
        })
        .count();
    let mut compiled_entries = Vec::with_capacity(specific_capacity);
    for entry in entries {
        if entry.act.is_system() {
            continue;
        }
        if entry.device.is_none() && entry.uuid.is_none() {
            wildcard_mask |= entry.act.bit();
            continue;
        }
        if entry.uuid.is_none()
            && let Some(mask) = entry.device.and_then(|device| device_masks.get_mut(device))
        {
            *mask |= entry.act.bit();
            continue;
        }
        if let Some(existing) =
            compiled_entries
                .iter_mut()
                .find(|item: &&mut CompiledPadCodeRev| {
                    item.device == entry.device && item.uuid == entry.uuid
                })
        {
            existing.mask |= entry.act.bit();
            continue;
        }
        compiled_entries.push(CompiledPadCodeRev {
            mask: entry.act.bit(),
            device: entry.device,
            uuid: entry.uuid,
        });
    }
    CompiledPadCodeMap {
        slot,
        wildcard_mask,
        device_masks,
        entries: compiled_entries.into_boxed_slice(),
    }
}

const PAD_DIR_ON_CAP: usize = PAD_ID_COUNT_CAP * 4;
// Pad codes preserve backend identity in their high bits. Projecting the low
// byte covers normal button/usage ranges across backends; validation plus
// binary-search fallback makes collisions and uncommon codes exact.
const PAD_CODE_LOOKUP_CAP: usize = 256;
const PAD_CODE_LOOKUP_MASK: usize = PAD_CODE_LOOKUP_CAP - 1;
const COLLIDING_PAD_CODE_INDEX: u16 = u16::MAX - 1;
const UNMAPPED_PAD_CODE_INDEX: u16 = u16::MAX;

const _: () = assert!(PAD_CODE_LOOKUP_CAP.is_power_of_two());

#[derive(Clone, Debug)]
struct CompiledKeymap {
    key_rev: Box<[CompiledBindingRev]>,
    key_rev_extra: HashMap<KeyCode, CompiledBindingRev>,
    pad_dir_rev: [u32; 4],
    pad_dir_on_rev: [u32; PAD_DIR_ON_CAP],
    pad_dir_on_extra: FxHashMap<(usize, PadDir), u32>,
    pad_code_rev: Box<[(u32, CompiledPadCodeMap)]>,
    pad_code_lookup: [u16; PAD_CODE_LOOKUP_CAP],
    key_slot_count: usize,
    pad_stride: usize,
    pad_slot_count: usize,
    pad_slot_capacity: usize,
}

impl CompiledKeymap {
    #[inline(always)]
    fn from_keymap(km: &Keymap) -> Self {
        let mut key_rev = new_compiled_key_rev();
        let mut next_key_slot = 0u32;
        for (ix, actions) in km.key_rev.iter().enumerate() {
            key_rev[ix] = compile_key_binding(actions, &mut next_key_slot);
        }
        let key_rev_extra = km
            .key_rev_extra
            .iter()
            .map(|(&code, actions)| (code, compile_key_binding(actions, &mut next_key_slot)))
            .collect();
        let mut pad_dir_rev = [0; 4];
        for (ix, actions) in km.pad_dir_rev.iter().enumerate() {
            let mut mask = 0;
            for &action in actions {
                mask |= action.bit();
            }
            pad_dir_rev[ix] = mask & !SYSTEM_ACTION_MASK;
        }
        let mut pad_dir_on_rev = [0; PAD_DIR_ON_CAP];
        let extra_dir_count = km
            .pad_dir_on_rev
            .keys()
            .filter(|(device, _)| *device >= PAD_ID_COUNT_CAP)
            .count();
        let mut pad_dir_on_extra =
            FxHashMap::with_capacity_and_hasher(extra_dir_count, FxBuildHasher::default());
        let mut max_pad_device: Option<usize> = None;
        for (&key, actions) in &km.pad_dir_on_rev {
            let mut mask = 0;
            for &action in actions {
                mask |= action.bit();
            }
            mask &= !SYSTEM_ACTION_MASK;
            if mask == 0 {
                continue;
            }
            max_pad_device = Some(max_pad_device.map_or(key.0, |max| max.max(key.0)));
            if key.0 < PAD_ID_COUNT_CAP {
                pad_dir_on_rev[key.0 * 4 + key.1.ix()] = mask;
            } else {
                pad_dir_on_extra.insert(key, mask);
            }
        }
        let mut pad_code_rev = Vec::with_capacity(km.pad_code_rev.len());
        let mut next_pad_button_slot = 0u32;
        for (&code, entries) in &km.pad_code_rev {
            let compiled = compile_pad_code_map(entries, next_pad_button_slot);
            for entry in entries {
                if !entry.act.is_system()
                    && let Some(device) = entry.device
                {
                    max_pad_device = Some(max_pad_device.map_or(device, |max| max.max(device)));
                }
            }
            pad_code_rev.push((code, compiled));
            next_pad_button_slot = next_pad_button_slot.saturating_add(1);
        }
        pad_code_rev.sort_unstable_by_key(|&(code, _)| code);
        let pad_code_rev = pad_code_rev.into_boxed_slice();
        let mut pad_code_lookup = [UNMAPPED_PAD_CODE_INDEX; PAD_CODE_LOOKUP_CAP];
        for (index, &(code, _)) in pad_code_rev
            .iter()
            .take(COLLIDING_PAD_CODE_INDEX as usize)
            .enumerate()
        {
            let projected = code as usize & PAD_CODE_LOOKUP_MASK;
            match pad_code_lookup[projected] {
                UNMAPPED_PAD_CODE_INDEX => pad_code_lookup[projected] = index as u16,
                COLLIDING_PAD_CODE_INDEX => {}
                _ => pad_code_lookup[projected] = COLLIDING_PAD_CODE_INDEX,
            }
        }
        let pad_stride = 4 + next_pad_button_slot as usize;
        let has_pad_bindings = pad_dir_rev.iter().any(|&mask| mask != 0)
            || pad_dir_on_rev.iter().any(|&mask| mask != 0)
            || !pad_dir_on_extra.is_empty()
            || !pad_code_rev.is_empty();
        let pad_slot_count = if has_pad_bindings {
            pad_stride.saturating_mul(max_pad_device.map_or(1, |max| max.saturating_add(1)))
        } else {
            0
        };
        let pad_slot_capacity = if has_pad_bindings {
            pad_stride.saturating_mul(max_pad_device.map_or(PAD_ID_COUNT_CAP, |max| {
                max.saturating_add(1).max(PAD_ID_COUNT_CAP)
            }))
        } else {
            0
        };
        Self {
            key_rev,
            key_rev_extra,
            pad_dir_rev,
            pad_dir_on_rev,
            pad_dir_on_extra,
            pad_code_rev,
            pad_code_lookup,
            key_slot_count: next_key_slot as usize,
            pad_stride,
            pad_slot_count,
            pad_slot_capacity,
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
#[inline(always)]
/// # Panics
///
/// Panics if an internal synchronization lock is poisoned.
pub fn with_keymap<R>(f: impl FnOnce(&Keymap) -> R) -> R {
    f(&KEYMAP.read().unwrap())
}

#[inline(always)]
/// # Panics
///
/// Panics if an internal synchronization lock is poisoned.
pub fn get_keymap() -> Keymap {
    KEYMAP.read().unwrap().clone()
}

/// Publishes editable bindings for settings and persistence.
///
/// Runtime owners must also call [`InputState::set_keymap`] at this boundary.
///
/// # Panics
///
/// Panics if the editable keymap lock is poisoned.
pub fn set_keymap(new_map: Keymap) {
    *KEYMAP.write().expect("editable keymap lock poisoned") = new_map;
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
#[must_use]
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
/// directional buttons (`menu_up`, `menu_down`, `menu_left`, `menu_right`) bound.
#[must_use]
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
#[must_use]
pub fn any_player_has_dedicated_menu_buttons_for_mode(three_key_navigation: bool) -> bool {
    if three_key_navigation {
        any_player_has_three_key_menu_buttons()
    } else {
        any_player_has_four_way_menu_buttons()
    }
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
    #[must_use]
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
    #[must_use]
    pub fn binding_at(&self, action: VirtualAction, index: usize) -> Option<InputBinding> {
        self.map
            .get(&action)
            .and_then(|bindings| bindings.get(index))
            .copied()
    }

    #[inline(always)]
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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

#[inline(always)]
fn collect_pad_dir_mask_from_compiled(km: &CompiledKeymap, id: PadId, dir: PadDir) -> u32 {
    let dev = usize::from(id);
    let device_mask = if dev < PAD_ID_COUNT_CAP {
        km.pad_dir_on_rev[dev * 4 + dir.ix()]
    } else {
        km.pad_dir_on_extra.get(&(dev, dir)).copied().unwrap_or(0)
    };
    km.pad_dir_rev[dir.ix()] | device_mask
}

#[inline(always)]
fn find_pad_code_map(km: &CompiledKeymap, code: u32) -> Option<&CompiledPadCodeMap> {
    let candidate = km.pad_code_lookup[code as usize & PAD_CODE_LOOKUP_MASK];
    if candidate == UNMAPPED_PAD_CODE_INDEX {
        return None;
    }
    if candidate != COLLIDING_PAD_CODE_INDEX {
        let (candidate_code, code_map) = &km.pad_code_rev[candidate as usize];
        return (*candidate_code == code).then_some(code_map);
    }
    let index = km
        .pad_code_rev
        .binary_search_by_key(&code, |&(candidate, _)| candidate)
        .ok()?;
    Some(&km.pad_code_rev[index].1)
}

#[inline(always)]
fn collect_pad_code_mask(code_map: &CompiledPadCodeMap, dev: usize, uuid: [u8; 16]) -> u32 {
    let mut mask = code_map.wildcard_mask | code_map.device_masks.get(dev).copied().unwrap_or(0);
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
    mask
}

#[inline(always)]
fn collect_pad_button_binding_from_compiled(
    km: &CompiledKeymap,
    id: PadId,
    code: PadCode,
    uuid: [u8; 16],
) -> Option<CompiledBindingRev> {
    let code = code.into_u32();
    let code_map = find_pad_code_map(km, code)?;
    let dev = usize::from(id);
    let mask = collect_pad_code_mask(code_map, dev, uuid);
    if mask == 0 {
        return None;
    }
    Some(CompiledBindingRev {
        mask,
        system_mask: 0,
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

fn compile_key_binding(actions: &[VirtualAction], next_slot: &mut u32) -> CompiledBindingRev {
    let actions = actions.iter().fold(0, |mask, action| mask | action.bit());
    let mask = actions & !SYSTEM_ACTION_MASK;
    let slot = if mask == 0 {
        UNMAPPED_DEBOUNCE_SLOT
    } else {
        let slot = *next_slot;
        *next_slot += 1;
        slot
    };
    CompiledBindingRev {
        mask,
        system_mask: actions & SYSTEM_ACTION_MASK,
        slot,
    }
}

/// A physical key event with its compiled system and logical bindings.
///
/// Read system actions before raw shortcuts; pass unconsumed events to
/// [`InputState::map_key`] without another lookup. Discard this value if the
/// keymap is replaced before dispatch.
#[derive(Clone, Copy, Debug)]
pub struct KeyEvent {
    /// Raw system-action bits, including on repeats and undebounced releases.
    pub system_mask: u32,
    raw: RawKeyboardEvent,
    binding: CompiledBindingRev,
}

/// Application-owned bindings and debounce state for one input stream.
///
/// The application thread exclusively owns this session-lifetime state; no
/// locks, publication, or thread-local refresh is needed for mapping or draining.
/// Construction and rebinding preallocate keyboard slots and all native pad IDs
/// below [`PAD_ID_COUNT_CAP`]. Unmapped input is ignored. Larger synthetic pad
/// IDs can grow storage. Rebinding discards pending edges and frees the old map
/// on the caller's thread, so it belongs at a settings/load boundary.
///
/// Mapping uses indexed lookup (with exact fallback for uncommon pad codes) and
/// at most two debounce edges. Due work uses a bounded per-slot heap and drains
/// one edge at a time; debug logging reports debounce activity. [`Self::clear`]
/// reuses storage without scanning slots except on epoch wraparound.
#[derive(Debug)]
pub struct InputState {
    compiled: CompiledKeymap,
    keyboard: DebounceStore,
    pad: DebounceStore,
    windows: DebounceWindows,
}

impl InputState {
    /// Compiles bindings and prepares debounce storage before processing input.
    pub fn new(keymap: &Keymap, debounce_seconds: f32) -> Self {
        let mut state = Self {
            compiled: CompiledKeymap::from_keymap(keymap),
            keyboard: DebounceStore::new(),
            pad: DebounceStore::new(),
            windows: DebounceWindows::uniform(Duration::from_secs_f32(
                clamp_input_debounce_seconds(debounce_seconds),
            )),
        };
        state.clear();
        state
    }

    /// Recompiles bindings and discards pending edges at a settings boundary.
    pub fn set_keymap(&mut self, keymap: &Keymap) {
        self.compiled = CompiledKeymap::from_keymap(keymap);
        self.clear();
    }

    /// Updates the debounce window while preserving held and pending edges.
    pub fn set_debounce_seconds(&mut self, seconds: f32) {
        self.windows = DebounceWindows::uniform(Duration::from_secs_f32(
            clamp_input_debounce_seconds(seconds),
        ));
    }

    /// Clears held and pending input, retaining preallocated debounce storage.
    pub fn clear(&mut self) {
        self.keyboard.prepare_slots(self.compiled.key_slot_count);
        self.pad.prepare_slots(self.compiled.pad_slot_capacity);
        log::debug!(
            "INPUT DEBOUNCE CLEAR: key_slots={} pad_slots={}",
            self.compiled.key_slot_count,
            self.compiled.pad_slot_count
        );
    }

    /// Resolves system controls and logical bindings with one keyboard lookup.
    pub fn key_event(&self, raw: RawKeyboardEvent) -> KeyEvent {
        let binding = match dense_key_ix(raw.code) {
            Some(ix) => self.compiled.key_rev[ix],
            None => self
                .compiled
                .key_rev_extra
                .get(&raw.code)
                .copied()
                .unwrap_or_default(),
        };
        KeyEvent {
            system_mask: binding.system_mask,
            raw,
            binding,
        }
    }

    /// Debounces an unconsumed mapped key, retaining the original edge timestamps.
    ///
    /// `now` supplies the application-thread receipt time only when an edge needs
    /// processing. Pass `Instant::now` in production or a fixed-time closure in
    /// tests. The owned iterator permits reconfiguration between dispatched events.
    pub fn map_key<F: FnOnce() -> Instant>(
        &mut self,
        key: KeyEvent,
        now: F,
    ) -> impl Iterator<Item = InputEvent> + use<F> {
        let ev = key.raw;
        let binding = key.binding;
        let edges = if (ev.pressed && ev.repeat) || !binding.mapped() {
            DebounceEdges::default()
        } else {
            debounce_input_edge_in_store_mut(
                &mut self.keyboard,
                binding.slot as usize,
                binding.mask,
                InputSource::Keyboard,
                ev.pressed,
                ev.timestamp,
                ev.host_nanos,
                self.windows,
                now,
            )
        };
        edges
            .first
            .into_iter()
            .chain(edges.second)
            .flat_map(input_events)
    }

    /// Maps and debounces a pad edge, ignoring raw axes and unmapped buttons.
    ///
    /// `now` is called at most once, after unmapped and settled duplicate exits.
    pub fn map_pad<F: FnOnce() -> Instant>(
        &mut self,
        ev: &PadEvent,
        now: F,
    ) -> impl Iterator<Item = InputEvent> + use<F> {
        let km = &self.compiled;
        let binding = match *ev {
            PadEvent::Dir {
                id,
                dir,
                pressed,
                timestamp,
                host_nanos,
            } => {
                let mask = collect_pad_dir_mask_from_compiled(km, id, dir);
                (mask != 0).then_some((
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
            } => collect_pad_button_binding_from_compiled(km, id, code, uuid).map(|binding| {
                (
                    pad_button_slot_from_compiled(km, id, binding.slot),
                    binding.mask,
                    pressed,
                    timestamp,
                    host_nanos,
                )
            }),
            PadEvent::RawAxis { .. } => None,
        };
        let edges = binding.map_or_else(
            DebounceEdges::default,
            |(slot, mask, pressed, timestamp, host_nanos)| {
                debounce_input_edge_in_store_mut(
                    &mut self.pad,
                    slot,
                    mask,
                    InputSource::Gamepad,
                    pressed,
                    timestamp,
                    host_nanos,
                    self.windows,
                    now,
                )
            },
        );
        edges
            .first
            .into_iter()
            .chain(edges.second)
            .flat_map(input_events)
    }

    /// Reports pending edges or debounce cleanup before the caller reads its clock.
    pub const fn has_pending(&self) -> bool {
        self.keyboard.has_scheduled_work() || self.pad.has_scheduled_work()
    }

    /// Removes the next due edge and returns its owned logical events.
    ///
    /// Keyboard edges drain before pad edges, preserving source and alias order.
    /// Delayed edges retain their raw timestamps and record `now` as emission time.
    pub fn next_due(&mut self, now: Instant) -> Option<impl Iterator<Item = InputEvent> + use<>> {
        next_due_edge(&mut self.keyboard, now, self.windows)
            .or_else(|| next_due_edge(&mut self.pad, now, self.windows))
            .map(input_events)
    }
}

fn input_events(edge: DebouncedEdge) -> impl Iterator<Item = InputEvent> {
    normalized_actions(edge.action_mask, edge.pressed).map(move |action| {
        InputEvent::new(
            action,
            edge.input_slot,
            edge.pressed,
            edge.source,
            edge.timestamp,
            edge.timestamp_host_nanos,
            edge.stored_at,
            edge.emitted_at,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_keymap_prepares_dense_debounce_slots() {
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

        let input = InputState::new(&km, 0.02);
        let (key_slot_count, pad_stride, pad_slot_count) = {
            let compiled = &input.compiled;
            (
                compiled.key_slot_count,
                compiled.pad_stride,
                compiled.pad_slot_count,
            )
        };

        assert_eq!(key_slot_count, 2);
        assert_eq!(pad_stride, 5);
        assert_eq!(pad_slot_count, 15);
    }

    #[test]
    fn device_specific_button_contributes_to_compiled_pad_slot_count() {
        let mut km = Keymap::default();
        km.bind(
            VirtualAction::p1_start,
            &[InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32: 77,
                device: Some(5),
                uuid: None,
            })],
        );

        let input = InputState::new(&km, 0.02);
        let (pad_stride, pad_slot_count) =
            (input.compiled.pad_stride, input.compiled.pad_slot_count);

        assert_eq!(pad_stride, 5);
        assert_eq!(pad_slot_count, 30);
    }
}
