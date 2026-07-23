use log::debug;
use mlua::{Function, Lua, Table, Value};
use std::collections::HashMap;
use std::ffi::c_void;

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

#[derive(Clone, PartialEq, Eq)]
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

pub fn read_runtime_mod_eases(
    table: Option<Table>,
    easing_names: &HashMap<*const c_void, String>,
    static_overlay: Option<usize>,
    context: &SongLuaCompileContext,
) -> Result<(Vec<SongLuaEaseWindow>, Vec<SongLuaOverlayEase>), String> {
    let Some(table) = table else {
        return Ok((Vec::new(), Vec::new()));
    };
    let mut entries = Vec::new();
    for value in table.sequence_values::<Value>() {
        let Value::Table(entry) = value.map_err(|err| err.to_string())? else {
            continue;
        };
        let Some(entry) = read_runtime_mod_ease_entry(entry, easing_names)? else {
            continue;
        };
        if !entries
            .iter()
            .any(|other| runtime_mod_entries_equal(other, &entry))
        {
            entries.push(entry);
        }
    }
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
    let mut overlay_capture_keys = Vec::new();
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
                    if overlay_capture_keys.contains(&capture_key) {
                        continue;
                    }
                    overlay_capture_keys.push(capture_key);
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

#[cfg(any(test, feature = "bench-support"))]
pub fn runtime_mod_state_updates_for_bench(entries: &[RuntimeModEaseEntry]) -> u64 {
    let mut current: [HashMap<String, f32>; LUA_PLAYERS] = std::array::from_fn(|_| HashMap::new());
    let mut checksum = 0_u64;
    for entry in entries {
        let mut key = Some(runtime_mod_key(&entry.target));
        let mut players = runtime_mod_player_indices(entry.player).peekable();
        while let Some(player) = players.next() {
            let from = runtime_mod_current_value(&current[player], key.as_deref().unwrap(), entry);
            let to = runtime_mod_end_value(from, entry);
            runtime_mod_store_current(&mut current[player], &mut key, to, players.peek().is_some());
            checksum = checksum.rotate_left(7)
                ^ from.to_bits() as u64
                ^ (to.to_bits() as u64).rotate_left(17)
                ^ player as u64;
        }
    }
    checksum
}

#[cfg(any(test, feature = "bench-support"))]
pub fn runtime_mod_state_updates_legacy_for_bench(entries: &[RuntimeModEaseEntry]) -> u64 {
    let mut current: [HashMap<String, f32>; LUA_PLAYERS] = std::array::from_fn(|_| HashMap::new());
    let mut checksum = 0_u64;
    for entry in entries {
        let key = runtime_mod_key(&entry.target);
        for player in runtime_mod_entry_players(entry.player) {
            let from = runtime_mod_start_value(&mut current[player], &key, entry);
            let to = runtime_mod_end_value(from, entry);
            current[player].insert(key.clone(), to);
            checksum = checksum.rotate_left(7)
                ^ from.to_bits() as u64
                ^ (to.to_bits() as u64).rotate_left(17)
                ^ player as u64;
        }
    }
    checksum
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
        | "hidden" | "sudden" | "stealth" | "blink" | "rvanish" | "randomvanish"
        | "reversevanish" | "dark" | "blind" | "cover" | "reverse" | "split" | "alternate"
        | "cross" | "centered" | "incoming" | "space" | "hallway" | "distant" | "overhead"
        | "xmod" | "cmod" | "mmod" | "tiny" | "mini" | "confusionyoffset" | "skewx" | "skewy" => {
            SongLuaEaseTarget::Mod(original.to_string())
        }
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

    for index in 0..windows.len() {
        let end = windows[index].start + windows[index].limit;
        let next_start = windows
            .iter()
            .enumerate()
            .filter_map(|(other_index, other)| {
                if other_index == index
                    || other.player != windows[index].player
                    || other.target != windows[index].target
                    || other.start <= windows[index].start + SAME_TICK_EPSILON
                {
                    None
                } else {
                    Some(other.start)
                }
            })
            .fold(None::<f32>, |acc, start| {
                Some(acc.map_or(start, |current| current.min(start)))
            })
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

    fn entry(target: &str, player: Option<u8>, to: f32, add: bool) -> RuntimeModEaseEntry {
        RuntimeModEaseEntry {
            unit: SongLuaTimeUnit::Beat,
            start: 0.0,
            limit: 1.0,
            easing: "linear".to_owned(),
            to,
            target: target.to_owned(),
            start_val: None,
            opt1: None,
            opt2: None,
            player,
            add,
        }
    }

    #[test]
    fn allocation_free_state_updates_match_legacy_player_and_value_behavior() {
        let entries = [
            entry("Zoom", None, 2.0, false),
            entry("zoom", None, 0.5, true),
            entry("Dark", Some(1), 0.75, false),
            entry("Dark", Some(1), 0.25, true),
            entry("Reverse", Some(2), 1.0, false),
            entry("Reverse", Some(9), 0.5, true),
        ];
        assert_eq!(
            runtime_mod_state_updates_for_bench(&entries),
            runtime_mod_state_updates_legacy_for_bench(&entries)
        );
    }

    #[test]
    fn exported_player_list_keeps_invalid_player_fallback_behavior() {
        assert_eq!(runtime_mod_entry_players(Some(1)), vec![0]);
        assert_eq!(runtime_mod_entry_players(Some(2)), vec![1]);
        assert_eq!(runtime_mod_entry_players(None), vec![0, 1]);
        assert_eq!(runtime_mod_entry_players(Some(0)), vec![0, 1]);
        assert_eq!(runtime_mod_entry_players(Some(3)), vec![0, 1]);
    }
}
