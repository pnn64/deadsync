use log::debug;
use mlua::{Function, Lua, Table, Value};
use std::collections::{HashMap, hash_map::RandomState};
use std::ffi::c_void;
use std::hash::{BuildHasher, Hash, Hasher};

use crate::{
    LUA_PLAYERS, SongLuaCompileContext, SongLuaCompileInfo, SongLuaEaseTarget, SongLuaEaseWindow,
    SongLuaOverlayCompileActor, SongLuaOverlayEase, SongLuaOverlayStateDelta, SongLuaSpanMode,
    SongLuaTimeUnit, actor_pointers_touch_actor, capture_overlay_compile_actor_function_eases,
    probe_function_ease_target, push_unique_compile_detail, read_easing_name, read_f32,
    read_player, read_string, truthy,
};

#[derive(Clone)]
pub struct RuntimeModEaseEntry {
    pub unit: SongLuaTimeUnit,
    pub start: f32,
    pub limit: f32,
    pub easing: String,
    pub to: f32,
    pub target: String,
    pub start_val: Option<f32>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
    pub player: Option<u8>,
    pub add: bool,
}

#[derive(Clone)]
pub struct XeroRuntimeOverlayFunctionEntry {
    pub entry: RuntimeModEaseEntry,
    pub function: Function,
}

pub enum XeroRuntimeModEaseEntry {
    Player(RuntimeModEaseEntry),
    Overlay(XeroRuntimeOverlayFunctionEntry),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeOverlayCaptureKey {
    pub function: usize,
    pub unit: SongLuaTimeUnit,
    pub start: u32,
    pub limit: u32,
    pub easing: String,
    pub target: String,
    pub from: u32,
    pub to: u32,
    pub opt1: Option<u32>,
    pub opt2: Option<u32>,
}

/// # Panics
///
/// Panics if an internal state invariant is violated.
pub fn read_runtime_mod_eases(
    table: Option<Table>,
    easing_names: &HashMap<*const c_void, String>,
    static_overlay: Option<usize>,
    context: &SongLuaCompileContext,
) -> Result<(Vec<SongLuaEaseWindow>, Vec<SongLuaOverlayEase>), String> {
    let Some(table) = table else {
        return Ok((Vec::new(), Vec::new()));
    };
    let mut entries = RuntimeModEntryDedup::new(table.raw_len());
    for value in table.sequence_values::<Value>() {
        let Value::Table(entry) = value.map_err(|err| err.to_string())? else {
            continue;
        };
        let Some(entry) = read_runtime_mod_ease_entry(entry, easing_names)? else {
            continue;
        };
        entries.push(entry);
    }
    let entries = entries.finish();
    if entries.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let mut current: [HashMap<String, f32>; LUA_PLAYERS] = std::array::from_fn(|_| HashMap::new());
    let mut eases = Vec::new();
    let mut overlay_eases = Vec::new();
    let static_player = context
        .players
        .iter()
        .position(|player| player.enabled)
        .unwrap_or(0);

    for entry in entries {
        let key = runtime_mod_key(&entry.target);
        if key == "static" {
            let mut static_window = None;
            let mut players = runtime_mod_player_indices(entry.player).peekable();
            let mut map_key = Some(key);
            while let Some(player) = players.next() {
                let from = runtime_mod_current_value(
                    &current[player],
                    map_key.as_deref().unwrap(),
                    &entry,
                );
                let to = runtime_mod_end_value(from, &entry);
                runtime_mod_store_current(
                    &mut current[player],
                    &mut map_key,
                    to,
                    players.peek().is_some(),
                );
                if player == static_player {
                    static_window = Some((from, to));
                }
            }
            if let (Some(overlay_index), Some((from, to))) = (static_overlay, static_window) {
                overlay_eases.push(runtime_static_overlay_ease(overlay_index, &entry, from, to));
            }
            continue;
        }

        let Some(target) = runtime_mod_ease_target(&key, &entry.target) else {
            continue;
        };
        let mut players = runtime_mod_player_indices(entry.player).peekable();
        let mut map_key = Some(key);
        while let Some(player) = players.next() {
            let from =
                runtime_mod_current_value(&current[player], map_key.as_deref().unwrap(), &entry);
            let to = runtime_mod_end_value(from, &entry);
            runtime_mod_store_current(
                &mut current[player],
                &mut map_key,
                to,
                players.peek().is_some(),
            );
            eases.push(SongLuaEaseWindow {
                unit: entry.unit,
                start: entry.start,
                limit: entry.limit,
                span_mode: SongLuaSpanMode::Len,
                from,
                to,
                target: target.clone(),
                easing: Some(entry.easing.clone()),
                player: Some((player + 1) as u8),
                sustain: None,
                opt1: entry.opt1,
                opt2: entry.opt2,
            });
        }
    }
    extend_runtime_mod_sustains(&mut eases);
    Ok((eases, overlay_eases))
}

pub fn read_xero_runtime_mod_entries(
    ease_tables: Vec<Table>,
    node_tables: Vec<Table>,
    easing_names: &HashMap<*const c_void, String>,
) -> Result<Vec<XeroRuntimeModEaseEntry>, String> {
    let node_functions = read_xero_node_functions(node_tables)?;
    let mut entries = Vec::new();
    for table in ease_tables {
        for value in table.sequence_values::<Value>() {
            let Value::Table(entry) = value.map_err(|err| err.to_string())? else {
                continue;
            };
            let Some(start) = read_f32(entry.raw_get::<Value>(1).map_err(|err| err.to_string())?)
            else {
                continue;
            };
            let Some(limit) = read_f32(entry.raw_get::<Value>(2).map_err(|err| err.to_string())?)
            else {
                continue;
            };
            let Some(easing) = read_easing_name(
                entry.raw_get::<Value>(3).map_err(|err| err.to_string())?,
                easing_names,
            ) else {
                continue;
            };
            if !start.is_finite() || !limit.is_finite() || limit < 0.0 {
                continue;
            }
            let unit = if truthy(
                &entry
                    .raw_get::<Value>("time")
                    .map_err(|err| err.to_string())?,
            ) {
                SongLuaTimeUnit::Second
            } else {
                SongLuaTimeUnit::Beat
            };
            let player = read_player(
                entry
                    .raw_get::<Value>("plr")
                    .map_err(|err| err.to_string())?,
            );
            let add = truthy(
                &entry
                    .raw_get::<Value>("relative")
                    .map_err(|err| err.to_string())?,
            );
            let mut index = 4;
            loop {
                let to_value = entry
                    .raw_get::<Value>(index)
                    .map_err(|err| err.to_string())?;
                let target_value = entry
                    .raw_get::<Value>(index + 1)
                    .map_err(|err| err.to_string())?;
                if matches!(to_value, Value::Nil) && matches!(target_value, Value::Nil) {
                    break;
                }
                let Some(to) = read_f32(to_value) else {
                    index += 2;
                    continue;
                };
                let Some(target) = read_string(target_value) else {
                    index += 2;
                    continue;
                };
                let key = runtime_mod_key(&target);
                let base = RuntimeModEaseEntry {
                    unit,
                    start,
                    limit,
                    easing: easing.clone(),
                    to,
                    target,
                    start_val: None,
                    opt1: None,
                    opt2: None,
                    player,
                    add,
                };
                if runtime_player_option_ease_target(&key, &base.target).is_some() {
                    entries.push(XeroRuntimeModEaseEntry::Player(base));
                } else if let Some(function) = node_functions.get(&key) {
                    entries.push(XeroRuntimeModEaseEntry::Overlay(
                        XeroRuntimeOverlayFunctionEntry {
                            entry: base,
                            function: function.clone(),
                        },
                    ));
                }
                index += 2;
            }
        }
    }
    Ok(entries)
}

/// # Panics
///
/// Panics if an internal state invariant is violated.
pub fn read_xero_runtime_mod_eases_with_overlay_capture<F>(
    ease_tables: Vec<Table>,
    node_tables: Vec<Table>,
    easing_names: &HashMap<*const c_void, String>,
    mut compile_overlay: F,
) -> Result<
    (
        Vec<SongLuaEaseWindow>,
        Vec<SongLuaOverlayEase>,
        SongLuaCompileInfo,
    ),
    String,
>
where
    F: FnMut(
        &RuntimeModEaseEntry,
        &Function,
        f32,
        f32,
        &mut SongLuaCompileInfo,
    ) -> Result<Vec<SongLuaOverlayEase>, String>,
{
    let entries = read_xero_runtime_mod_entries(ease_tables, node_tables, easing_names)?;
    if entries.is_empty() {
        return Ok((Vec::new(), Vec::new(), SongLuaCompileInfo::default()));
    }

    let mut current: [HashMap<String, f32>; LUA_PLAYERS] = std::array::from_fn(|_| HashMap::new());
    let mut out = Vec::new();
    let mut overlay_eases = Vec::new();
    let mut overlay_capture_keys = RuntimeOverlayCaptureDedup::new(entries.len());
    let mut info = SongLuaCompileInfo::default();
    for entry in entries {
        match entry {
            XeroRuntimeModEaseEntry::Player(entry) => {
                let key = runtime_mod_key(&entry.target);
                let Some(target) = runtime_player_option_ease_target(&key, &entry.target) else {
                    continue;
                };
                let mut players = runtime_mod_player_indices(entry.player).peekable();
                let mut map_key = Some(key);
                while let Some(player) = players.next() {
                    let from = runtime_mod_current_value(
                        &current[player],
                        map_key.as_deref().unwrap(),
                        &entry,
                    );
                    let to = runtime_mod_end_value(from, &entry);
                    runtime_mod_store_current(
                        &mut current[player],
                        &mut map_key,
                        to,
                        players.peek().is_some(),
                    );
                    out.push(SongLuaEaseWindow {
                        unit: entry.unit,
                        start: entry.start,
                        limit: entry.limit,
                        span_mode: SongLuaSpanMode::Len,
                        from,
                        to,
                        target: target.clone(),
                        easing: Some(entry.easing.clone()),
                        player: Some((player + 1) as u8),
                        sustain: None,
                        opt1: entry.opt1,
                        opt2: entry.opt2,
                    });
                }
            }
            XeroRuntimeModEaseEntry::Overlay(entry) => {
                let key = runtime_mod_key(&entry.entry.target);
                let mut players = runtime_mod_player_indices(entry.entry.player).peekable();
                let mut map_key = Some(key);
                while let Some(player) = players.next() {
                    let from = runtime_mod_current_value(
                        &current[player],
                        map_key.as_deref().unwrap(),
                        &entry.entry,
                    );
                    let to = runtime_mod_end_value(from, &entry.entry);
                    runtime_mod_store_current(
                        &mut current[player],
                        &mut map_key,
                        to,
                        players.peek().is_some(),
                    );
                    let capture_key =
                        runtime_overlay_capture_key(&entry.entry, &entry.function, from, to);
                    if !overlay_capture_keys.insert(capture_key) {
                        continue;
                    }
                    overlay_eases.extend(compile_overlay(
                        &entry.entry,
                        &entry.function,
                        from,
                        to,
                        &mut info,
                    )?);
                }
            }
        }
    }
    extend_runtime_mod_sustains(&mut out);
    Ok((out, overlay_eases, info))
}

pub fn read_xero_runtime_mod_eases_for_overlay_actors<Kind>(
    lua: &Lua,
    ease_tables: Vec<Table>,
    node_tables: Vec<Table>,
    easing_names: &HashMap<*const c_void, String>,
    overlays: &[SongLuaOverlayCompileActor<Kind>],
) -> Result<
    (
        Vec<SongLuaEaseWindow>,
        Vec<SongLuaOverlayEase>,
        SongLuaCompileInfo,
    ),
    String,
> {
    read_xero_runtime_mod_eases_with_overlay_capture(
        ease_tables,
        node_tables,
        easing_names,
        |entry, function, from, to, info| {
            compile_xero_overlay_function_ease(lua, overlays, entry, function, from, to, info)
        },
    )
}

fn compile_xero_overlay_function_ease<Kind>(
    lua: &Lua,
    overlays: &[SongLuaOverlayCompileActor<Kind>],
    entry: &RuntimeModEaseEntry,
    function: &Function,
    from: f32,
    to: f32,
    info: &mut SongLuaCompileInfo,
) -> Result<Vec<SongLuaOverlayEase>, String> {
    let (probed_target, probe_methods, probe_actor_ptrs) =
        probe_function_ease_target(lua, function).map_err(|err| err.to_string())?;
    if !xero_node_touches_overlay(overlays, &probe_actor_ptrs)
        || !matches!(probed_target, None | Some(SongLuaEaseTarget::Function))
    {
        return Ok(Vec::new());
    }
    match capture_overlay_compile_actor_function_eases(
        lua,
        overlays,
        function,
        entry.unit,
        entry.start,
        entry.limit,
        SongLuaSpanMode::Len,
        from,
        to,
        Some(entry.easing.clone()),
        None,
        entry.opt1,
        entry.opt2,
        &probe_actor_ptrs,
    ) {
        Ok(compiled) if !compiled.is_empty() => Ok(compiled),
        Ok(_) => {
            let detail = record_unsupported_xero_overlay_function_ease(
                info,
                entry,
                from,
                to,
                &probe_methods,
            );
            debug!("Unsupported xero overlay function ease capture: {detail}");
            Ok(Vec::new())
        }
        Err(err) => {
            let detail = record_unsupported_xero_overlay_function_ease(
                info,
                entry,
                from,
                to,
                &probe_methods,
            );
            debug!("Unsupported xero overlay function ease capture: {detail}");
            debug!(
                "Unsupported xero overlay function ease capture for '{}': {err}",
                entry.target
            );
            Ok(Vec::new())
        }
    }
}

fn xero_node_touches_overlay<Kind>(
    overlays: &[SongLuaOverlayCompileActor<Kind>],
    probe_actor_ptrs: &[usize],
) -> bool {
    actor_pointers_touch_actor(
        overlays.len(),
        |index| overlays[index].table.to_pointer() as usize,
        probe_actor_ptrs,
    )
}

fn read_xero_node_functions(tables: Vec<Table>) -> Result<HashMap<String, Function>, String> {
    let mut out = HashMap::new();
    for table in tables {
        for value in table.sequence_values::<Value>() {
            let Value::Table(node) = value.map_err(|err| err.to_string())? else {
                continue;
            };
            let Value::Table(inputs) = node.raw_get::<Value>(1).map_err(|err| err.to_string())?
            else {
                continue;
            };
            let Value::Function(function) =
                node.raw_get::<Value>(3).map_err(|err| err.to_string())?
            else {
                continue;
            };
            for input in inputs.sequence_values::<Value>() {
                let Some(name) = read_string(input.map_err(|err| err.to_string())?) else {
                    continue;
                };
                out.entry(runtime_mod_key(&name))
                    .or_insert_with(|| function.clone());
            }
        }
    }
    Ok(out)
}

pub fn read_runtime_mod_ease_entry(
    entry: Table,
    easing_names: &HashMap<*const c_void, String>,
) -> Result<Option<RuntimeModEaseEntry>, String> {
    let Some(start) = read_f32(entry.raw_get::<Value>(1).map_err(|err| err.to_string())?) else {
        return Ok(None);
    };
    let Some(mut limit) = read_f32(entry.raw_get::<Value>(2).map_err(|err| err.to_string())?)
    else {
        return Ok(None);
    };
    let Some(easing) = read_easing_name(
        entry.raw_get::<Value>(3).map_err(|err| err.to_string())?,
        easing_names,
    ) else {
        return Ok(None);
    };
    let Some(to) = read_f32(entry.raw_get::<Value>(4).map_err(|err| err.to_string())?) else {
        return Ok(None);
    };
    let Some(target) = read_string(entry.raw_get::<Value>(5).map_err(|err| err.to_string())?)
    else {
        return Ok(None);
    };
    if read_string(
        entry
            .raw_get::<Value>("timing")
            .map_err(|err| err.to_string())?,
    )
    .is_some_and(|value| value.eq_ignore_ascii_case("end"))
    {
        limit -= start;
    }
    if !start.is_finite() || !limit.is_finite() || limit < 0.0 || !to.is_finite() {
        return Ok(None);
    }
    let player = read_player(
        entry
            .raw_get::<Value>("plr")
            .map_err(|err| err.to_string())?,
    );
    let player = match player {
        Some(player) => Some(player),
        None => read_player(
            entry
                .raw_get::<Value>("pn")
                .map_err(|err| err.to_string())?,
        ),
    };
    Ok(Some(RuntimeModEaseEntry {
        unit: SongLuaTimeUnit::Beat,
        start,
        limit,
        easing,
        to,
        target,
        start_val: read_f32(
            entry
                .raw_get::<Value>("startVal")
                .map_err(|err| err.to_string())?,
        ),
        opt1: read_f32(
            entry
                .raw_get::<Value>("opt1")
                .map_err(|err| err.to_string())?,
        ),
        opt2: read_f32(
            entry
                .raw_get::<Value>("opt2")
                .map_err(|err| err.to_string())?,
        ),
        player,
        add: truthy(
            &entry
                .raw_get::<Value>("add")
                .map_err(|err| err.to_string())?,
        ),
    }))
}

fn runtime_mod_entries_equal(left: &RuntimeModEaseEntry, right: &RuntimeModEaseEntry) -> bool {
    left.unit == right.unit
        && left.start.to_bits() == right.start.to_bits()
        && left.limit.to_bits() == right.limit.to_bits()
        && left.to.to_bits() == right.to.to_bits()
        && left.target == right.target
        && left.easing == right.easing
        && left.start_val.map(f32::to_bits) == right.start_val.map(f32::to_bits)
        && left.opt1.map(f32::to_bits) == right.opt1.map(f32::to_bits)
        && left.opt2.map(f32::to_bits) == right.opt2.map(f32::to_bits)
        && left.player == right.player
        && left.add == right.add
}

fn runtime_mod_entry_hash(entry: &RuntimeModEaseEntry, state: &RandomState) -> u64 {
    let mut hash = state.build_hasher();
    match entry.unit {
        SongLuaTimeUnit::Beat => 0u8,
        SongLuaTimeUnit::Second => 1,
    }
    .hash(&mut hash);
    entry.start.to_bits().hash(&mut hash);
    entry.limit.to_bits().hash(&mut hash);
    entry.to.to_bits().hash(&mut hash);
    entry.target.hash(&mut hash);
    entry.easing.hash(&mut hash);
    entry.start_val.map(f32::to_bits).hash(&mut hash);
    entry.opt1.map(f32::to_bits).hash(&mut hash);
    entry.opt2.map(f32::to_bits).hash(&mut hash);
    entry.player.hash(&mut hash);
    entry.add.hash(&mut hash);
    hash.finish()
}

struct RuntimeModEntryDedup {
    entries: Vec<RuntimeModEaseEntry>,
    slots: smallvec::SmallVec<[usize; 2048]>,
    hash_state: RandomState,
}

impl RuntimeModEntryDedup {
    fn new(entry_count: usize) -> Self {
        let slot_count = entry_count
            .clamp(1, 1024)
            .saturating_mul(2)
            .next_power_of_two();
        let mut slots = smallvec::SmallVec::with_capacity(slot_count);
        slots.resize(slot_count, 0);
        Self {
            entries: Vec::with_capacity(entry_count.min(256)),
            slots,
            hash_state: RandomState::new(),
        }
    }

    fn push(&mut self, entry: RuntimeModEaseEntry) {
        if self.entries.len().saturating_mul(2) >= self.slots.len() && !self.grow_slots() {
            if !self
                .entries
                .iter()
                .any(|other| runtime_mod_entries_equal(other, &entry))
            {
                self.entries.push(entry);
            }
            return;
        }
        let hash = runtime_mod_entry_hash(&entry, &self.hash_state);
        let mask = self.slots.len() - 1;
        let mut slot = hash as usize & mask;
        loop {
            let stored = self.slots[slot];
            if stored == 0 {
                let index = self.entries.len();
                self.entries.push(entry);
                self.slots[slot] = index + 1;
                return;
            }
            if runtime_mod_entries_equal(&self.entries[stored - 1], &entry) {
                return;
            }
            slot = (slot + 1) & mask;
        }
    }

    fn grow_slots(&mut self) -> bool {
        let Some(slot_count) = self.slots.len().checked_mul(2) else {
            return false;
        };
        let mut slots = smallvec::SmallVec::<[usize; 2048]>::with_capacity(slot_count);
        slots.resize(slot_count, 0);
        let mask = slot_count - 1;
        for (index, entry) in self.entries.iter().enumerate() {
            let mut slot = runtime_mod_entry_hash(entry, &self.hash_state) as usize & mask;
            while slots[slot] != 0 {
                slot = (slot + 1) & mask;
            }
            slots[slot] = index + 1;
        }
        self.slots = slots;
        true
    }

    fn finish(self) -> Vec<RuntimeModEaseEntry> {
        self.entries
    }
}

/// Collects exact-unique runtime-mod entries in first-seen order. The
/// compilation-only lookup stays inline through 1,024 source entries and
/// retains at most 256 output slots before measured growth is necessary.
#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub fn collect_unique_runtime_mod_entries(
    entries: impl IntoIterator<Item = RuntimeModEaseEntry>,
    entry_count: usize,
) -> Vec<RuntimeModEaseEntry> {
    let mut out = RuntimeModEntryDedup::new(entry_count);
    for entry in entries {
        out.push(entry);
    }
    out.finish()
}

#[must_use]
pub fn runtime_mod_entry_players(player: Option<u8>) -> Vec<usize> {
    runtime_mod_player_indices(player).collect()
}

fn runtime_mod_player_indices(player: Option<u8>) -> std::ops::Range<usize> {
    match player {
        Some(player) if (1..=LUA_PLAYERS as u8).contains(&player) => {
            let player = (player - 1) as usize;
            player..player + 1
        }
        _ => 0..LUA_PLAYERS,
    }
}

#[must_use]
pub fn runtime_mod_key(target: &str) -> String {
    target.to_ascii_lowercase()
}

fn runtime_mod_initial_value(key: &str) -> f32 {
    if matches!(key, "zoom" | "zoomx" | "zoomy" | "zoomz") {
        1.0
    } else {
        0.0
    }
}

fn runtime_mod_current_value(
    current: &HashMap<String, f32>,
    key: &str,
    entry: &RuntimeModEaseEntry,
) -> f32 {
    entry.start_val.unwrap_or_else(|| {
        current
            .get(key)
            .copied()
            .unwrap_or_else(|| runtime_mod_initial_value(key))
    })
}

fn runtime_mod_store_current(
    current: &mut HashMap<String, f32>,
    key: &mut Option<String>,
    value: f32,
    has_more_players: bool,
) {
    if let Some(current_value) = current.get_mut(key.as_deref().unwrap()) {
        *current_value = value;
        return;
    }
    let key = if has_more_players {
        key.as_ref().unwrap().clone()
    } else {
        key.take().unwrap()
    };
    current.insert(key, value);
}

pub fn runtime_mod_start_value(
    current: &mut HashMap<String, f32>,
    key: &str,
    entry: &RuntimeModEaseEntry,
) -> f32 {
    entry.start_val.unwrap_or_else(|| {
        *current
            .entry(key.to_string())
            .or_insert_with(|| runtime_mod_initial_value(key))
    })
}

#[must_use]
pub fn runtime_mod_end_value(from: f32, entry: &RuntimeModEaseEntry) -> f32 {
    if entry.add { from + entry.to } else { entry.to }
}

pub fn runtime_overlay_capture_key(
    entry: &RuntimeModEaseEntry,
    function: &Function,
    from: f32,
    to: f32,
) -> RuntimeOverlayCaptureKey {
    RuntimeOverlayCaptureKey {
        function: function.to_pointer() as usize,
        unit: entry.unit,
        start: entry.start.to_bits(),
        limit: entry.limit.to_bits(),
        easing: entry.easing.clone(),
        target: runtime_mod_key(&entry.target),
        from: from.to_bits(),
        to: to.to_bits(),
        opt1: entry.opt1.map(f32::to_bits),
        opt2: entry.opt2.map(f32::to_bits),
    }
}

fn runtime_overlay_capture_key_hash(key: &RuntimeOverlayCaptureKey) -> u64 {
    let mut hash = rustc_hash::FxHasher::default();
    key.function.hash(&mut hash);
    match key.unit {
        SongLuaTimeUnit::Beat => 0u8,
        SongLuaTimeUnit::Second => 1,
    }
    .hash(&mut hash);
    key.start.hash(&mut hash);
    key.limit.hash(&mut hash);
    key.easing.hash(&mut hash);
    key.target.hash(&mut hash);
    key.from.hash(&mut hash);
    key.to.hash(&mut hash);
    key.opt1.hash(&mut hash);
    key.opt2.hash(&mut hash);
    hash.finish()
}

struct RuntimeOverlayCaptureDedup {
    keys: Vec<RuntimeOverlayCaptureKey>,
    slots: smallvec::SmallVec<[usize; 2048]>,
}

impl RuntimeOverlayCaptureDedup {
    fn new(entry_count: usize) -> Self {
        let expected = entry_count.saturating_mul(LUA_PLAYERS);
        let slot_count = expected
            .clamp(1, 1024)
            .saturating_mul(2)
            .next_power_of_two();
        let mut slots = smallvec::SmallVec::with_capacity(slot_count);
        slots.resize(slot_count, 0);
        Self {
            keys: Vec::with_capacity(expected.min(256)),
            slots,
        }
    }

    fn insert(&mut self, key: RuntimeOverlayCaptureKey) -> bool {
        if self.keys.len().saturating_mul(2) >= self.slots.len() && !self.grow_slots() {
            if self.keys.contains(&key) {
                return false;
            }
            self.keys.push(key);
            return true;
        }
        let hash = runtime_overlay_capture_key_hash(&key);
        let mask = self.slots.len() - 1;
        let mut slot = hash as usize & mask;
        loop {
            let stored = self.slots[slot];
            if stored == 0 {
                let index = self.keys.len();
                self.keys.push(key);
                self.slots[slot] = index + 1;
                return true;
            }
            if self.keys[stored - 1] == key {
                return false;
            }
            slot = (slot + 1) & mask;
        }
    }

    fn grow_slots(&mut self) -> bool {
        let Some(slot_count) = self.slots.len().checked_mul(2) else {
            return false;
        };
        let mut slots = smallvec::SmallVec::<[usize; 2048]>::with_capacity(slot_count);
        slots.resize(slot_count, 0);
        let mask = slot_count - 1;
        for (index, key) in self.keys.iter().enumerate() {
            let mut slot = runtime_overlay_capture_key_hash(key) as usize & mask;
            while slots[slot] != 0 {
                slot = (slot + 1) & mask;
            }
            slots[slot] = index + 1;
        }
        self.slots = slots;
        true
    }
}

/// Collects exact-unique overlay capture keys in first-seen order. This is
/// exposed only so the load-path benchmark can compare against the former
/// linear scan without duplicating the production index.
#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub fn collect_unique_runtime_overlay_capture_keys(
    keys: impl IntoIterator<Item = RuntimeOverlayCaptureKey>,
    entry_count: usize,
) -> Vec<RuntimeOverlayCaptureKey> {
    let mut dedup = RuntimeOverlayCaptureDedup::new(entry_count);
    for key in keys {
        dedup.insert(key);
    }
    dedup.keys
}

#[must_use]
pub fn runtime_mod_ease_target(key: &str, original: &str) -> Option<SongLuaEaseTarget> {
    Some(match key {
        "z" => SongLuaEaseTarget::PlayerZ,
        "rotationx" => SongLuaEaseTarget::PlayerRotationX,
        "rotationy" => SongLuaEaseTarget::PlayerRotationY,
        "rotationz" => SongLuaEaseTarget::PlayerRotationZ,
        "zoom" => SongLuaEaseTarget::PlayerZoom,
        "zoomx" => SongLuaEaseTarget::PlayerZoomX,
        "zoomy" => SongLuaEaseTarget::PlayerZoomY,
        "zoomz" => SongLuaEaseTarget::PlayerZoomZ,
        "x" | "y" => return None,
        _ => SongLuaEaseTarget::Mod(original.to_string()),
    })
}

fn runtime_mod_column_key(key: &str, prefix: &str) -> bool {
    key.strip_prefix(prefix)
        .and_then(|suffix| suffix.parse::<usize>().ok())
        .is_some_and(|column| (1..=16).contains(&column))
}

#[must_use]
pub fn runtime_player_option_ease_target(key: &str, original: &str) -> Option<SongLuaEaseTarget> {
    if runtime_mod_column_key(key, "bumpy")
        || runtime_mod_column_key(key, "tiny")
        || runtime_mod_column_key(key, "movex")
        || runtime_mod_column_key(key, "movey")
        || runtime_mod_column_key(key, "confusionoffset")
    {
        return Some(SongLuaEaseTarget::Mod(original.to_string()));
    }
    Some(match key {
        "z" => SongLuaEaseTarget::PlayerZ,
        "rotationx" => SongLuaEaseTarget::PlayerRotationX,
        "rotationy" => SongLuaEaseTarget::PlayerRotationY,
        "rotationz" => SongLuaEaseTarget::PlayerRotationZ,
        "zoom" => SongLuaEaseTarget::PlayerZoom,
        "zoomx" => SongLuaEaseTarget::PlayerZoomX,
        "zoomy" => SongLuaEaseTarget::PlayerZoomY,
        "zoomz" => SongLuaEaseTarget::PlayerZoomZ,
        "boost" | "brake" | "wave" | "expand" | "boomerang" | "drunk" | "dizzy" | "confusion"
        | "confusionoffset" | "flip" | "invert" | "tornado" | "tipsy" | "bumpy" | "bumpyoffset"
        | "bumpyperiod" | "pulseinner" | "pulseouter" | "pulseperiod" | "pulseoffset" | "beat"
        | "randomspeed" | "hidden" | "sudden" | "suddenoffset" | "stealth" | "blink"
        | "rvanish" | "randomvanish" | "reversevanish" | "dark" | "blind" | "cover" | "reverse"
        | "split" | "alternate" | "cross" | "centered" | "incoming" | "space" | "hallway"
        | "distant" | "overhead" | "xmod" | "cmod" | "mmod" | "tiny" | "mini"
        | "confusionyoffset" | "skewx" | "skewy" => SongLuaEaseTarget::Mod(original.to_string()),
        _ => return None,
    })
}

fn runtime_static_overlay_ease(
    overlay_index: usize,
    entry: &RuntimeModEaseEntry,
    from: f32,
    to: f32,
) -> SongLuaOverlayEase {
    SongLuaOverlayEase {
        overlay_index,
        unit: SongLuaTimeUnit::Beat,
        start: entry.start,
        limit: entry.limit,
        span_mode: SongLuaSpanMode::Len,
        from: SongLuaOverlayStateDelta {
            diffuse: Some([1.0, 1.0, 1.0, from]),
            ..SongLuaOverlayStateDelta::default()
        },
        to: SongLuaOverlayStateDelta {
            diffuse: Some([1.0, 1.0, 1.0, to]),
            ..SongLuaOverlayStateDelta::default()
        },
        easing: Some(entry.easing.clone()),
        sustain: None,
        opt1: entry.opt1,
        opt2: entry.opt2,
    }
}

pub fn extend_runtime_mod_sustains(windows: &mut [SongLuaEaseWindow]) {
    const DEFAULT_SUSTAIN_BEATS: f32 = 1_000_000.0;
    const SAME_TICK_EPSILON: f32 = 0.001;

    // Typical and unusually dense mod charts fit inline, including 7th Gear.
    // Pathological charts spill once during compilation, then release the index.
    let mut order = smallvec::SmallVec::<[usize; 1024]>::with_capacity(windows.len());
    order.extend(0..windows.len());
    order.sort_unstable_by(|&left, &right| {
        windows[left]
            .player
            .cmp(&windows[right].player)
            .then_with(|| windows[left].target.cmp(&windows[right].target))
            .then_with(|| windows[left].start.total_cmp(&windows[right].start))
            .then_with(|| left.cmp(&right))
    });

    let mut group_start = 0;
    while group_start < order.len() {
        let first = order[group_start];
        let mut group_end = group_start + 1;
        while group_end < order.len()
            && windows[order[group_end]].player == windows[first].player
            && windows[order[group_end]].target == windows[first].target
        {
            group_end += 1;
        }
        for position in group_start..group_end {
            let index = order[position];
            let start = windows[index].start;
            let threshold = start + SAME_TICK_EPSILON;
            let later = &order[position + 1..group_end];
            let next = later.partition_point(|&other| windows[other].start <= threshold);
            let next_start = later
                .get(next)
                .map_or(DEFAULT_SUSTAIN_BEATS, |&other| windows[other].start);
            let end = start + windows[index].limit;
            if next_start > end + SAME_TICK_EPSILON {
                windows[index].sustain = Some(next_start - end);
            }
        }
        group_start = group_end;
    }
}

#[cfg(test)]
fn extend_runtime_mod_sustains_reference(windows: &mut [SongLuaEaseWindow]) {
    const DEFAULT_SUSTAIN_BEATS: f32 = 1_000_000.0;
    const SAME_TICK_EPSILON: f32 = 0.001;

    for index in 0..windows.len() {
        let end = windows[index].start + windows[index].limit;
        let next_start = windows
            .iter()
            .enumerate()
            .filter_map(|(other_index, other)| {
                (other_index != index
                    && other.player == windows[index].player
                    && other.target == windows[index].target
                    && other.start > windows[index].start + SAME_TICK_EPSILON)
                    .then_some(other.start)
            })
            .min_by(f32::total_cmp)
            .unwrap_or(DEFAULT_SUSTAIN_BEATS);
        if next_start > end + SAME_TICK_EPSILON {
            windows[index].sustain = Some(next_start - end);
        }
    }
}

pub fn record_unsupported_xero_overlay_function_ease(
    info: &mut SongLuaCompileInfo,
    entry: &RuntimeModEaseEntry,
    from: f32,
    to: f32,
    probe_methods: &[String],
) -> String {
    info.unsupported_function_eases += 1;
    let detail = format!(
        "xero node '{}' unit={:?} start={:.3} limit={:.3} from={:.3} to={:.3} \
         easing={:?} probe_methods={:?}",
        entry.target, entry.unit, entry.start, entry.limit, from, to, entry.easing, probe_methods,
    );
    push_unique_compile_detail(&mut info.unsupported_function_ease_captures, detail.clone());
    detail
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime_mod_fixture(count: usize, unique: usize) -> Vec<RuntimeModEaseEntry> {
        let unique = unique.max(1);
        (0..count)
            .map(|index| {
                let key = (index * 73) % unique;
                RuntimeModEaseEntry {
                    unit: if key % 7 == 0 {
                        SongLuaTimeUnit::Second
                    } else {
                        SongLuaTimeUnit::Beat
                    },
                    start: key as f32 * 0.125,
                    limit: ((key % 5) as f32).mul_add(0.0625, 0.25),
                    easing: format!("ease{}", key % 11),
                    to: (key as f32 * -0.75).copysign(if key % 2 == 0 { 1.0 } else { -1.0 }),
                    target: format!("mod{}", key % 37),
                    start_val: (key % 3 == 0).then_some(key as f32),
                    opt1: (key % 4 == 0).then_some(key as f32 * 0.5),
                    opt2: (key % 6 == 0).then_some(key as f32 * -0.25),
                    player: Some((key % 2 + 1) as u8),
                    add: key % 5 == 0,
                }
            })
            .collect()
    }

    fn dedup_runtime_mod_entries_reference(
        entries: impl IntoIterator<Item = RuntimeModEaseEntry>,
    ) -> Vec<RuntimeModEaseEntry> {
        let mut out = Vec::new();
        for entry in entries {
            if !out
                .iter()
                .any(|other| runtime_mod_entries_equal(other, &entry))
            {
                out.push(entry);
            }
        }
        out
    }

    fn overlay_capture_key_fixture(count: usize, unique: usize) -> Vec<RuntimeOverlayCaptureKey> {
        let unique = unique.max(1);
        (0..count)
            .map(|index| {
                let key = (index * 73) % unique;
                RuntimeOverlayCaptureKey {
                    function: 0x1000 + key % 31,
                    unit: if key % 7 == 0 {
                        SongLuaTimeUnit::Second
                    } else {
                        SongLuaTimeUnit::Beat
                    },
                    start: (key as f32 * 0.125).to_bits(),
                    limit: ((key % 5) as f32).mul_add(0.0625, 0.25).to_bits(),
                    easing: format!("ease{}", key % 11),
                    target: format!("node{}", key % 37),
                    from: (key as f32 * 0.5).to_bits(),
                    to: (key as f32 * -0.75).to_bits(),
                    opt1: (key % 4 == 0).then(|| (key as f32 * 0.5).to_bits()),
                    opt2: (key % 6 == 0).then(|| (key as f32 * -0.25).to_bits()),
                }
            })
            .collect()
    }

    fn sustain_fixture(count: usize) -> Vec<SongLuaEaseWindow> {
        (0..count)
            .map(|index| SongLuaEaseWindow {
                unit: SongLuaTimeUnit::Beat,
                start: ((index * 73) % count.max(1)) as f32 * 0.25,
                limit: ((index % 5) as f32).mul_add(0.0625, 0.125),
                span_mode: SongLuaSpanMode::Len,
                from: 0.0,
                to: 1.0,
                target: if index % 3 == 0 {
                    SongLuaEaseTarget::PlayerRotationZ
                } else {
                    SongLuaEaseTarget::Mod(format!("mod{}", index % 11))
                },
                easing: Some("linear".to_string()),
                player: Some((index % 2 + 1) as u8),
                sustain: None,
                opt1: None,
                opt2: None,
            })
            .collect()
    }

    #[test]
    fn exported_player_list_keeps_invalid_player_fallback_behavior() {
        assert_eq!(runtime_mod_entry_players(Some(1)), vec![0]);
        assert_eq!(runtime_mod_entry_players(Some(2)), vec![1]);
        assert_eq!(runtime_mod_entry_players(None), vec![0, 1]);
        assert_eq!(runtime_mod_entry_players(Some(0)), vec![0, 1]);
        assert_eq!(runtime_mod_entry_players(Some(3)), vec![0, 1]);
    }

    #[test]
    fn indexed_runtime_mod_dedup_matches_first_seen_scan() {
        for (count, unique) in [(0, 1), (1, 1), (33, 11), (513, 127), (513, 513)] {
            let source = runtime_mod_fixture(count, unique);
            let reference = dedup_runtime_mod_entries_reference(source.iter().cloned());
            let current = collect_unique_runtime_mod_entries(source, count);
            assert_eq!(
                current.len(),
                reference.len(),
                "count={count} unique={unique}"
            );
            assert!(
                current
                    .iter()
                    .zip(&reference)
                    .all(|(left, right)| runtime_mod_entries_equal(left, right)),
                "count={count} unique={unique}"
            );
        }
    }

    #[test]
    fn indexed_overlay_capture_dedup_matches_first_seen_scan() {
        for (count, unique) in [(0, 1), (1, 1), (33, 11), (513, 127), (513, 513)] {
            let source = overlay_capture_key_fixture(count, unique);
            let mut reference = Vec::new();
            for key in source.iter().cloned() {
                if !reference.contains(&key) {
                    reference.push(key);
                }
            }
            assert_eq!(
                collect_unique_runtime_overlay_capture_keys(source, count),
                reference,
                "count={count} unique={unique}"
            );
        }
    }

    #[test]
    fn grouped_sustain_extension_matches_full_scan_for_unordered_windows() {
        let mut reference = sustain_fixture(513);
        let mut current = reference.clone();
        extend_runtime_mod_sustains_reference(&mut reference);
        extend_runtime_mod_sustains(&mut current);
        assert_eq!(current, reference);
    }
}
