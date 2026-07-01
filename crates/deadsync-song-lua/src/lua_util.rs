use deadlib_present::actors::TextAttribute;
use mlua::{Function, Lua, MultiValue, Table, Value, ffi};
use std::collections::{HashMap, HashSet};
use std::ffi::c_int;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crate::{
    GRAPH_DISPLAY_VALUE_RESOLUTION, LUA_PLAYERS, SONG_LUA_CAPTURE_ACTOR_SET_KEY,
    SONG_LUA_CAPTURE_ACTORS_KEY, SONG_LUA_CAPTURE_SNAPSHOTS_KEY, SONG_LUA_INITIAL_LIFE,
    SONG_LUA_PROBE_ACTOR_SET_KEY, SONG_LUA_PROBE_ACTORS_KEY, SONG_LUA_PROBE_METHODS_KEY,
    SONG_LUA_SOUND_PATHS_KEY, SONG_LUA_SPRITE_STATE_CLEAR, SONG_LUA_THEME_PATH_PREFIX,
    SongLuaActorMultiVertexPoint, SongLuaCompileContext, SongLuaDifficulty, SongLuaEaseTarget,
    SongLuaNoteHideWindow, SongLuaOverlayCommandBlock, SongLuaOverlayMeshVertex,
    SongLuaOverlayState, SongLuaOverlayStateDelta, SongLuaProxyTarget, SongLuaTrackedActor,
    call_with_script_dir, clone_lua_value, compile_song_runtime_values, graph_display_body_size,
    is_song_lua_media_path, is_song_lua_video_path, parse_color_text, parse_overlay_blend_mode,
    parse_overlay_effect_clock, parse_overlay_effect_mode, parse_overlay_text_align,
    parse_overlay_text_glow_mode, player_number_name, read_boolish, read_f32, read_i32_value,
    read_string, set_compile_song_runtime_beat, set_compile_song_runtime_values, set_string_method,
    song_lua_human_player_count, theme_path,
};

type ActorAssetPrefixKey = (String, String);
type ActorAssetPrefixCache = Mutex<HashMap<ActorAssetPrefixKey, Option<PathBuf>>>;
static ACTOR_ASSET_PREFIX_CACHE: OnceLock<ActorAssetPrefixCache> = OnceLock::new();
const SONG_LUA_CHILD_GROUP_KEY: &str = "__songlua_child_group";

pub fn read_song_lua_sound_paths(lua: &Lua) -> Result<Vec<PathBuf>, String> {
    let globals = lua.globals();
    let Some(paths) = globals
        .get::<Option<Table>>(SONG_LUA_SOUND_PATHS_KEY)
        .map_err(|err| err.to_string())?
    else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(paths.raw_len());
    for path in paths.sequence_values::<String>() {
        out.push(PathBuf::from(path.map_err(|err| err.to_string())?));
    }
    Ok(out)
}

pub fn lua_text_value(value: Value) -> mlua::Result<String> {
    match value {
        Value::String(text) => Ok(text.to_str()?.to_string()),
        Value::Integer(value) => Ok(value.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        Value::Boolean(value) => Ok(value.to_string()),
        _ => Ok(String::new()),
    }
}

pub fn table_string_field(table: &Table, names: &[&str]) -> mlua::Result<Option<String>> {
    for name in names {
        if let Some(value) = read_string(table.get::<Value>(*name)?) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

pub fn table_f32_field(table: &Table, names: &[&str]) -> mlua::Result<Option<f32>> {
    for name in names {
        if let Some(value) = read_f32(table.get::<Value>(*name)?) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

pub fn table_i32_field(table: &Table, names: &[&str]) -> mlua::Result<Option<i32>> {
    for name in names {
        if let Some(value) = read_i32_value(table.get::<Value>(*name)?) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

pub fn table_bool_field(table: &Table, names: &[&str]) -> mlua::Result<Option<bool>> {
    for name in names {
        if let Some(value) = read_boolish(table.get::<Value>(*name)?) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

pub fn current_song_lua_style_name(lua: &Lua) -> String {
    let Ok(Value::Table(style)) = current_gamestate_value(lua, "GetCurrentStyle") else {
        return "single".to_string();
    };
    let Ok(Some(method)) = style.get::<Option<Function>>("GetName") else {
        return "single".to_string();
    };
    method
        .call::<Value>(style)
        .ok()
        .and_then(read_string)
        .unwrap_or_else(|| "single".to_string())
}

pub fn read_child_index(value: &Value) -> Option<usize> {
    match value {
        Value::Integer(value) if *value >= 0 => Some(*value as usize),
        Value::Number(value) if value.is_finite() && *value >= 0.0 => Some(*value as usize),
        _ => None,
    }
}

pub fn push_unique_actor_child(out: &mut Vec<Table>, seen: &mut Vec<usize>, child: Table) {
    let ptr = child.to_pointer() as usize;
    if seen.contains(&ptr) {
        return;
    }
    seen.push(ptr);
    out.push(child);
}

pub fn actor_children(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if let Some(children) = actor.get::<Option<Table>>("__songlua_children")? {
        return Ok(children);
    }
    let children = lua.create_table()?;
    actor.set("__songlua_children", children.clone())?;
    Ok(children)
}

pub fn actor_named_children(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    let children = lua.create_table()?;
    for pair in actor_children(lua, actor)?.pairs::<Value, Value>() {
        let (key, value) = pair?;
        children.set(key, value)?;
    }
    merge_actor_sequence_children(lua, actor, &children)?;
    Ok(children)
}

pub fn actor_direct_children(lua: &Lua, actor: &Table) -> mlua::Result<Vec<Table>> {
    let mut out = Vec::new();
    let mut seen = Vec::new();
    for value in actor.sequence_values::<Value>() {
        let Value::Table(child) = value? else {
            continue;
        };
        push_unique_actor_child(&mut out, &mut seen, child);
    }
    for pair in actor_children(lua, actor)?.pairs::<Value, Value>() {
        let (_, value) = pair?;
        let Value::Table(child) = value else {
            continue;
        };
        if actor_is_child_group(&child)? {
            for group_value in child.sequence_values::<Value>() {
                if let Value::Table(group_child) = group_value? {
                    push_unique_actor_child(&mut out, &mut seen, group_child);
                }
            }
        } else {
            push_unique_actor_child(&mut out, &mut seen, child);
        }
    }
    Ok(out)
}

pub fn actor_child_at(lua: &Lua, actor: &Table, index: usize) -> mlua::Result<Value> {
    Ok(actor_direct_children(lua, actor)?
        .into_iter()
        .nth(index)
        .map_or(Value::Nil, Value::Table))
}

pub fn remove_actor_child(lua: &Lua, actor: &Table, name: &str) -> mlua::Result<()> {
    actor_children(lua, actor)?.set(name, Value::Nil)?;
    let mut write = 1;
    for read in 1..=actor.raw_len() {
        let value = actor.raw_get::<Value>(read)?;
        let remove = match &value {
            Value::Table(child) => child
                .get::<Option<String>>("Name")?
                .is_some_and(|child_name| child_name == name),
            _ => false,
        };
        if remove {
            continue;
        }
        if write != read {
            actor.raw_set(write, value)?;
        }
        write += 1;
    }
    for index in write..=actor.raw_len() {
        actor.raw_set(index, Value::Nil)?;
    }
    Ok(())
}

pub fn remove_all_actor_children(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    for index in 1..=actor.raw_len() {
        actor.raw_set(index, Value::Nil)?;
    }
    let children = actor_children(lua, actor)?;
    let mut keys = Vec::new();
    for pair in children.pairs::<Value, Value>() {
        let (key, _) = pair?;
        keys.push(key);
    }
    for key in keys {
        children.set(key, Value::Nil)?;
    }
    Ok(())
}

pub fn push_sequence_child_once(actor: &Table, child: Table) -> mlua::Result<bool> {
    let child_ptr = child.to_pointer();
    for index in 1..=actor.raw_len() {
        if let Some(Value::Table(existing)) = actor.raw_get::<Option<Value>>(index)?
            && existing.to_pointer() == child_ptr
        {
            return Ok(false);
        }
    }
    actor.raw_set(actor.raw_len() + 1, child)?;
    Ok(true)
}

pub fn install_course_contents_list_children(actor: &Table) -> mlua::Result<()> {
    actor.set("__songlua_scroller_current_item", 0.0_f32)?;
    actor.set("__songlua_scroller_destination_item", 0.0_f32)?;
    actor.set("__songlua_scroller_num_items", 1_i64)?;
    if let Some(display) = actor.get::<Option<Table>>("Display")? {
        if display.get::<Option<String>>("Name")?.is_none() {
            display.set("Name", "Display")?;
        }
        display.set("__songlua_parent", actor.clone())?;
        push_sequence_child_once(actor, display)?;
    }
    Ok(())
}

pub fn position_scroller_items(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some(display) = actor_named_children(lua, actor)?.get::<Option<Table>>("Display")? else {
        return Ok(());
    };
    let num_items = actor
        .get::<Option<i64>>("__songlua_scroller_num_items")?
        .unwrap_or(1)
        .max(1);
    if let Some(transform) =
        actor.get::<Option<Function>>("__songlua_scroller_transform_function")?
    {
        transform.call::<()>((display, 0.0_f32, 0_i64, num_items))?;
    } else if let Some(height) = actor.get::<Option<f32>>("__songlua_scroller_item_height")? {
        display.set("Y", 0.0_f32 * height)?;
    } else if let Some(width) = actor.get::<Option<f32>>("__songlua_scroller_item_width")? {
        display.set("X", 0.0_f32 * width)?;
    }
    Ok(())
}

pub fn populate_course_contents_display(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some(display) = actor_named_children(lua, actor)?.get::<Option<Table>>("Display")? else {
        return Ok(());
    };
    for player_index in 0..LUA_PLAYERS {
        let params = create_course_contents_params(lua, player_index)?;
        let params = Some(Value::Table(params));
        run_actor_named_command_with_drain_and_params(
            lua,
            &display,
            "SetSongCommand",
            true,
            params.clone(),
        )?;
        run_named_command_on_leaves(lua, &display, "SetSongCommand", params)?;
    }
    Ok(())
}

fn create_course_contents_params(lua: &Lua, player_index: usize) -> mlua::Result<Table> {
    let params = lua.create_table()?;
    params.set("Number", 0_i64)?;
    params.set("PlayerNumber", player_number_name(player_index))?;
    params.set("Song", current_song_value(lua)?)?;
    let steps = current_steps_value(lua, player_index)?;
    params.set("Steps", steps.clone())?;
    set_course_contents_steps_params(lua, &params, steps)?;
    Ok(params)
}

pub fn current_song_value(lua: &Lua) -> mlua::Result<Value> {
    current_gamestate_value(lua, "GetCurrentSong")
}

pub fn current_gamestate_value(lua: &Lua, method_name: &str) -> mlua::Result<Value> {
    let Some(gamestate) = lua.globals().get::<Option<Table>>("GAMESTATE")? else {
        return Ok(Value::Nil);
    };
    let Some(method) = gamestate.get::<Option<Function>>(method_name)? else {
        return Ok(Value::Nil);
    };
    method.call::<Value>(gamestate)
}

pub fn current_steps_value(lua: &Lua, player_index: usize) -> mlua::Result<Value> {
    current_gamestate_player_value(lua, "GetCurrentSteps", player_index)
}

pub fn current_gamestate_player_value(
    lua: &Lua,
    method_name: &str,
    player_index: usize,
) -> mlua::Result<Value> {
    let Some(gamestate) = lua.globals().get::<Option<Table>>("GAMESTATE")? else {
        return Ok(Value::Nil);
    };
    let Some(method) = gamestate.get::<Option<Function>>(method_name)? else {
        return Ok(Value::Nil);
    };
    method.call::<Value>((gamestate, player_number_name(player_index)))
}

fn set_course_contents_steps_params(lua: &Lua, params: &Table, steps: Value) -> mlua::Result<()> {
    let Value::Table(steps) = steps else {
        params.set("Meter", "?")?;
        params.set("Difficulty", SongLuaDifficulty::Medium.sm_name())?;
        return Ok(());
    };
    let meter = call_table_method(&steps, "GetMeter")?;
    params.set(
        "Meter",
        if matches!(meter, Value::Nil) {
            Value::String(lua.create_string("?")?)
        } else {
            meter
        },
    )?;
    let difficulty = call_table_method(&steps, "GetDifficulty")?;
    params.set(
        "Difficulty",
        if matches!(difficulty, Value::Nil) {
            Value::String(lua.create_string(SongLuaDifficulty::Medium.sm_name())?)
        } else {
            difficulty
        },
    )
}

pub fn call_table_method(table: &Table, method_name: &str) -> mlua::Result<Value> {
    let Some(method) = table.get::<Option<Function>>(method_name)? else {
        return Ok(Value::Nil);
    };
    method.call::<Value>(table.clone())
}

fn merge_actor_sequence_children(lua: &Lua, actor: &Table, children: &Table) -> mlua::Result<()> {
    for value in actor.sequence_values::<Value>() {
        let Value::Table(child) = value? else {
            continue;
        };
        let name = child.get::<Option<String>>("Name")?.unwrap_or_default();
        match children.get::<Option<Value>>(name.as_str())? {
            Some(Value::Table(group)) if actor_is_child_group(&group)? => {
                group.raw_set(group.raw_len() + 1, child)?;
            }
            Some(Value::Table(existing)) => {
                let group = create_actor_child_group(lua)?;
                group.raw_set(1, existing)?;
                group.raw_set(2, child)?;
                children.set(name.as_str(), group)?;
            }
            Some(_) => {}
            None => {
                children.set(name.as_str(), child)?;
            }
        }
    }
    Ok(())
}

pub fn create_actor_child_group(lua: &Lua) -> mlua::Result<Table> {
    let group = lua.create_table()?;
    let mt = lua.create_table()?;
    mt.set(SONG_LUA_CHILD_GROUP_KEY, true)?;
    let _ = group.set_metatable(Some(mt));
    Ok(group)
}

pub fn actor_is_child_group(table: &Table) -> mlua::Result<bool> {
    table
        .metatable()
        .map(|mt| mt.get::<Option<bool>>(SONG_LUA_CHILD_GROUP_KEY))
        .transpose()
        .map(|value| value.flatten().unwrap_or(false))
}

pub fn actor_wrappers(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if let Some(wrappers) = actor.get::<Option<Table>>("__songlua_wrappers")? {
        return Ok(wrappers);
    }
    let wrappers = lua.create_table()?;
    actor.set("__songlua_wrappers", wrappers.clone())?;
    Ok(wrappers)
}

pub fn copy_dummy_actor_tags(from: &Table, into: &Table) -> mlua::Result<()> {
    if let Some(player_index) = from.get::<Option<i64>>("__songlua_player_index")? {
        into.set("__songlua_player_index", player_index)?;
    }
    if let Some(child_name) = from.get::<Option<String>>("__songlua_player_child_name")? {
        into.set("__songlua_player_child_name", child_name)?;
    }
    if let Some(child_name) = from.get::<Option<String>>("__songlua_top_screen_child_name")? {
        into.set("__songlua_top_screen_child_name", child_name)?;
    }
    for key in [
        "__songlua_main_title",
        "__songlua_bpm_text",
        "__songlua_steps_text_1",
        "__songlua_steps_text_2",
        "__songlua_style_name",
    ] {
        if let Some(value) = from.get::<Option<String>>(key)? {
            into.set(key, value)?;
        }
    }
    Ok(())
}

fn is_actor_mutable_state_key(key: &str) -> bool {
    key.starts_with("__songlua_state_")
        || key.starts_with("__songlua_capture_")
        || key.starts_with("__songlua_graph_display_")
        || key.starts_with("__songlua_scroller_")
        || matches!(
            key,
            "__songlua_visible"
                | "__songlua_diffuse"
                | "__songlua_text_attributes"
                | "__songlua_stroke_color"
                | "__songlua_aux"
                | "__songlua_aft_capture_name"
                | "__songlua_stream_width"
                | "__songlua_sprite_animation_length_seconds"
                | "__songlua_sprite_effect_mode"
                | "Text"
                | "Texture"
                | "File"
                | "StreamWidth"
        )
}

fn is_actor_semantic_state_key(key: &str) -> bool {
    is_actor_mutable_state_key(key) && !key.starts_with("__songlua_capture_")
}

pub fn snapshot_actor_mutable_state(
    lua: &Lua,
    actor: &Table,
) -> mlua::Result<Vec<(String, Value)>> {
    snapshot_actor_state(lua, actor, is_actor_mutable_state_key)
}

fn snapshot_actor_semantic_state(lua: &Lua, actor: &Table) -> mlua::Result<Vec<(String, Value)>> {
    snapshot_actor_state(lua, actor, is_actor_semantic_state_key)
}

fn snapshot_actor_state(
    lua: &Lua,
    actor: &Table,
    keep_key: fn(&str) -> bool,
) -> mlua::Result<Vec<(String, Value)>> {
    let mut out = Vec::new();
    for pair in actor.clone().pairs::<Value, Value>() {
        let (key, value) = pair?;
        let Value::String(key) = key else {
            continue;
        };
        let key = key.to_str()?.to_string();
        if keep_key(&key) {
            out.push((key, clone_lua_value(lua, value)?));
        }
    }
    Ok(out)
}

pub fn restore_actor_mutable_state(
    actor: &Table,
    snapshot: Vec<(String, Value)>,
) -> mlua::Result<()> {
    restore_actor_state(actor, snapshot, is_actor_mutable_state_key)
}

fn restore_actor_semantic_state(actor: &Table, snapshot: Vec<(String, Value)>) -> mlua::Result<()> {
    restore_actor_state(actor, snapshot, is_actor_semantic_state_key)
}

fn restore_actor_state(
    actor: &Table,
    snapshot: Vec<(String, Value)>,
    clear_key: fn(&str) -> bool,
) -> mlua::Result<()> {
    let mut keys = Vec::new();
    for pair in actor.clone().pairs::<Value, Value>() {
        let (key, _) = pair?;
        let Value::String(key) = key else {
            continue;
        };
        let key = key.to_str()?.to_string();
        if clear_key(&key) {
            keys.push(key);
        }
    }
    for key in keys {
        actor.set(key, Value::Nil)?;
    }
    for (key, value) in snapshot {
        actor.set(key, value)?;
    }
    Ok(())
}

pub fn snapshot_actors_semantic_state(
    lua: &Lua,
    actors: &[Table],
) -> mlua::Result<Vec<(Table, Vec<(String, Value)>)>> {
    actors
        .iter()
        .map(|actor| Ok((actor.clone(), snapshot_actor_semantic_state(lua, actor)?)))
        .collect()
}

pub fn restore_actors_semantic_state(
    snapshots: Vec<(Table, Vec<(String, Value)>)>,
) -> mlua::Result<()> {
    for (actor, snapshot) in snapshots {
        restore_actor_semantic_state(&actor, snapshot)?;
    }
    Ok(())
}

pub fn snapshot_actor_semantic_state_table(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    let snapshot = lua.create_table()?;
    for (index, (key, value)) in snapshot_actor_semantic_state(lua, actor)?
        .into_iter()
        .enumerate()
    {
        let entry = lua.create_table()?;
        entry.raw_set(1, key)?;
        entry.raw_set(2, value)?;
        snapshot.raw_set(index + 1, entry)?;
    }
    Ok(snapshot)
}

pub fn read_actor_semantic_state_table(snapshot: &Table) -> mlua::Result<Vec<(String, Value)>> {
    let mut out = Vec::with_capacity(snapshot.raw_len());
    for entry in snapshot.sequence_values::<Table>() {
        let entry = entry?;
        out.push((entry.raw_get(1)?, entry.raw_get(2)?));
    }
    Ok(out)
}

pub fn song_lua_actor_registry(lua: &Lua) -> mlua::Result<Table> {
    let globals = lua.globals();
    if let Some(registry) = globals.get::<Option<Table>>("__songlua_actor_registry")? {
        return Ok(registry);
    }
    let registry = lua.create_table()?;
    globals.set("__songlua_actor_registry", registry.clone())?;
    Ok(registry)
}

pub fn register_song_lua_actor(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let registry = song_lua_actor_registry(lua)?;
    let actor_ptr = actor.to_pointer() as usize;
    for value in registry.sequence_values::<Value>() {
        let Value::Table(existing) = value? else {
            continue;
        };
        if existing.to_pointer() as usize == actor_ptr {
            return Ok(());
        }
    }
    registry.raw_set(registry.raw_len() + 1, actor.clone())?;
    Ok(())
}

pub fn inherit_actor_dirs(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if let Some(script_dir) = lua
        .globals()
        .get::<Option<String>>("__songlua_script_dir")?
    {
        actor.set("__songlua_script_dir", script_dir)?;
    }
    if let Some(song_dir) = lua.globals().get::<Option<String>>("__songlua_song_dir")? {
        actor.set("__songlua_song_dir", song_dir)?;
    }
    Ok(())
}

pub fn set_proxy_target_fields(actor: &Table, target: &Table) -> mlua::Result<()> {
    actor.set("__songlua_proxy_target_kind", Value::Nil)?;
    actor.set("__songlua_proxy_player_index", Value::Nil)?;
    if let Some(child_name) = target.get::<Option<String>>("__songlua_top_screen_child_name")? {
        let kind = if child_name.eq_ignore_ascii_case("Underlay") {
            Some("underlay")
        } else if child_name.eq_ignore_ascii_case("Overlay") {
            Some("overlay")
        } else {
            None
        };
        if let Some(kind) = kind {
            actor.set("__songlua_proxy_target_kind", kind)?;
        }
        return Ok(());
    }
    let Some(player_index) = target.get::<Option<i64>>("__songlua_player_index")? else {
        return Ok(());
    };
    actor.set("__songlua_proxy_player_index", player_index)?;
    let kind = match target
        .get::<Option<String>>("__songlua_player_child_name")?
        .as_deref()
    {
        Some(name) if name.eq_ignore_ascii_case("NoteField") => "notefield",
        Some(name) if name.eq_ignore_ascii_case("Judgment") => "judgment",
        Some(name) if name.eq_ignore_ascii_case("Combo") => "combo",
        _ => "player",
    };
    actor.set("__songlua_proxy_target_kind", kind)?;
    Ok(())
}

pub fn normalize_broadcast_params(
    lua: &Lua,
    message: &str,
    params: Option<Value>,
) -> mlua::Result<Option<Value>> {
    match params {
        Some(Value::Table(table)) => {
            if message.eq_ignore_ascii_case("Judgment") {
                normalize_judgment_params(lua, &table)?;
            }
            Ok(Some(Value::Table(table)))
        }
        params => Ok(params),
    }
}

fn normalize_judgment_params(lua: &Lua, params: &Table) -> mlua::Result<()> {
    if matches!(params.get::<Value>("Player")?, Value::Nil) {
        params.set("Player", player_number_name(0))?;
    }
    if matches!(params.get::<Value>("TapNoteScore")?, Value::Nil) {
        params.set("TapNoteScore", "TapNoteScore_W3")?;
    }
    if matches!(params.get::<Value>("HoldNoteScore")?, Value::Nil) {
        params.set("HoldNoteScore", "HoldNoteScore_None")?;
    }
    if matches!(params.get::<Value>("TapNoteOffset")?, Value::Nil) {
        params.set("TapNoteOffset", 0.0_f32)?;
    }

    let notes = match params.get::<Value>("Notes")? {
        Value::Table(notes) => notes,
        _ => {
            let notes = lua.create_table()?;
            let first_track = table_i32_field(params, &["FirstTrack", "Column"])?.unwrap_or(0);
            notes.raw_set(
                i64::from(first_track.max(0)) + 1,
                create_tap_note_table(lua, params, None)?,
            )?;
            params.set("Notes", notes.clone())?;
            notes
        }
    };
    if table_has_entries(&notes)? {
        let entries = notes
            .pairs::<Value, Value>()
            .collect::<mlua::Result<Vec<_>>>()?;
        for (key, value) in entries {
            let note = match value {
                Value::Table(note) => normalize_tap_note_table(lua, params, note)?,
                value => create_tap_note_table(lua, params, Some(value))?,
            };
            notes.set(key, note)?;
        }
    } else {
        notes.raw_set(1, create_tap_note_table(lua, params, None)?)?;
    }
    Ok(())
}

pub fn default_message_command_params(
    lua: &Lua,
    command_name: &str,
) -> mlua::Result<Option<Value>> {
    let Some(message) = command_name.strip_suffix("MessageCommand") else {
        return Ok(None);
    };
    let params = lua.create_table()?;
    match message {
        "Judgment" => {
            params.set("Player", player_number_name(0))?;
            params.set("TapNoteScore", "TapNoteScore_W3")?;
            params.set("TapNoteOffset", 0.0_f32)?;
            params.set("FirstTrack", 0_i64)?;
            normalize_judgment_params(lua, &params)?;
        }
        "EarlyHit" => {
            params.set("Player", player_number_name(0))?;
            params.set("TapNoteScore", "TapNoteScore_W3")?;
            params.set("TapNoteOffset", -0.02_f32)?;
            params.set("Early", true)?;
        }
        "LifeChanged" => {
            params.set("Player", player_number_name(0))?;
            params.set("LifeMeter", create_life_meter_param_table(lua)?)?;
        }
        "HealthStateChanged" => {
            params.set("Player", player_number_name(0))?;
            params.set("PlayerNumber", player_number_name(0))?;
            params.set("HealthState", "HealthState_Alive")?;
        }
        "ExCountsChanged" => {
            params.set("Player", player_number_name(0))?;
            params.set("ExCounts", create_ex_counts_param_table(lua)?)?;
            params.set("ExScore", 0.0_f32)?;
            params.set("ActualPoints", 0.0_f32)?;
            params.set("ActualPossible", 1.0_f32)?;
            params.set("CurrentPossible", 1.0_f32)?;
        }
        "PlayerOptionsChanged" => {
            params.set("Player", player_number_name(0))?;
            params.set("PlayerNumber", player_number_name(0))?;
        }
        _ => return Ok(None),
    }
    Ok(Some(Value::Table(params)))
}

fn create_life_meter_param_table(lua: &Lua) -> mlua::Result<Table> {
    let meter = lua.create_table()?;
    meter.set(
        "GetLife",
        lua.create_function(|_, _args: MultiValue| Ok(SONG_LUA_INITIAL_LIFE))?,
    )?;
    meter.set(
        "IsFailing",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    meter.set(
        "IsHot",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    meter.set(
        "IsInDanger",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    Ok(meter)
}

fn create_ex_counts_param_table(lua: &Lua) -> mlua::Result<Table> {
    let counts = lua.create_table()?;
    for key in [
        "W0", "W1", "W2", "W3", "W4", "W5", "Miss", "Held", "LetGo", "HitMine",
    ] {
        counts.set(key, 0_i64)?;
    }
    Ok(counts)
}

fn table_has_entries(table: &Table) -> mlua::Result<bool> {
    Ok(table.pairs::<Value, Value>().next().transpose()?.is_some())
}

fn normalize_tap_note_table(lua: &Lua, params: &Table, note: Table) -> mlua::Result<Table> {
    if !matches!(note.get::<Value>("GetTapNoteType")?, Value::Nil) {
        return Ok(note);
    }
    let result = match note.get::<Value>("TapNoteResult")? {
        Value::Table(result) => normalize_tap_note_result_table(lua, params, result, Some(&note))?,
        _ => create_tap_note_result_table(lua, params, Some(&note))?,
    };
    note.set("TapNoteResult", result.clone())?;
    note.set("HoldNoteResult", result.clone())?;
    install_tap_note_methods(lua, params, &note, result)?;
    Ok(note)
}

fn create_tap_note_table(
    lua: &Lua,
    params: &Table,
    note_value: Option<Value>,
) -> mlua::Result<Table> {
    let note = lua.create_table()?;
    if let Some(value) = note_value {
        note.set("TapNoteType", value)?;
    }
    let result = create_tap_note_result_table(lua, params, Some(&note))?;
    note.set("TapNoteResult", result.clone())?;
    note.set("HoldNoteResult", result.clone())?;
    install_tap_note_methods(lua, params, &note, result)?;
    Ok(note)
}

fn install_tap_note_methods(
    lua: &Lua,
    params: &Table,
    note: &Table,
    result: Table,
) -> mlua::Result<()> {
    let note_type = table_string_field(note, &["TapNoteType", "Type", "NoteType"])?
        .or(table_string_field(
            params,
            &["TapNoteType", "Type", "NoteType"],
        )?)
        .unwrap_or_else(|| "TapNoteType_Tap".to_string());
    let source = table_string_field(note, &["TapNoteSource", "Source"])?
        .unwrap_or_else(|| "TapNoteSource_Original".to_string());
    let subtype = table_string_field(note, &["TapNoteSubType", "SubType"])?
        .unwrap_or_else(|| "TapNoteSubType_Hold".to_string());
    let player = table_string_field(params, &["Player"])?
        .unwrap_or_else(|| player_number_name(0).to_string());
    let hold_duration = table_f32_field(note, &["HoldDuration"])?.unwrap_or(0.0);
    let attack_duration = table_f32_field(note, &["AttackDuration"])?.unwrap_or(0.0);
    let attack_mods = table_string_field(note, &["AttackModifiers"])?.unwrap_or_default();
    let keysound = table_i32_field(note, &["KeysoundIndex"])?.unwrap_or(0);

    set_string_method(lua, note, "GetTapNoteType", &note_type)?;
    set_string_method(lua, note, "GetTapNoteSource", &source)?;
    set_string_method(lua, note, "GetTapNoteSubType", &subtype)?;
    set_string_method(lua, note, "GetPlayerNumber", &player)?;
    set_string_method(lua, note, "GetAttackModifiers", &attack_mods)?;
    note.set(
        "GetTapNoteResult",
        lua.create_function({
            let result = result.clone();
            move |_, _args: MultiValue| Ok(result.clone())
        })?,
    )?;
    note.set(
        "GetHoldNoteResult",
        lua.create_function({
            let result = result.clone();
            move |_, _args: MultiValue| Ok(result.clone())
        })?,
    )?;
    note.set(
        "GetHoldDuration",
        lua.create_function(move |_, _args: MultiValue| Ok(hold_duration))?,
    )?;
    note.set(
        "GetAttackDuration",
        lua.create_function(move |_, _args: MultiValue| Ok(attack_duration))?,
    )?;
    note.set(
        "GetKeysoundIndex",
        lua.create_function(move |_, _args: MultiValue| Ok(keysound))?,
    )?;
    Ok(())
}

fn create_tap_note_result_table(
    lua: &Lua,
    params: &Table,
    note: Option<&Table>,
) -> mlua::Result<Table> {
    let result = lua.create_table()?;
    install_tap_note_result_methods(lua, params, note, &result)?;
    Ok(result)
}

fn normalize_tap_note_result_table(
    lua: &Lua,
    params: &Table,
    result: Table,
    note: Option<&Table>,
) -> mlua::Result<Table> {
    if matches!(result.get::<Value>("GetHeld")?, Value::Nil) {
        install_tap_note_result_methods(lua, params, note, &result)?;
    }
    Ok(result)
}

fn install_tap_note_result_methods(
    lua: &Lua,
    params: &Table,
    note: Option<&Table>,
    result: &Table,
) -> mlua::Result<()> {
    let held = match note {
        Some(note) => table_bool_field(note, &["Held", "held"])?,
        None => None,
    }
    .unwrap_or(false);
    let hidden = match note {
        Some(note) => table_bool_field(note, &["Hidden", "hidden"])?,
        None => None,
    }
    .unwrap_or(false);
    let offset = match note {
        Some(note) => table_f32_field(note, &["TapNoteOffset", "Offset"])?,
        None => None,
    }
    .or(table_f32_field(params, &["TapNoteOffset", "Offset"])?)
    .unwrap_or(0.0);
    let score = match note {
        Some(note) => table_string_field(note, &["TapNoteScore", "Score"])?,
        None => None,
    }
    .or(table_string_field(params, &["TapNoteScore", "Score"])?)
    .unwrap_or_else(|| "TapNoteScore_None".to_string());

    result.set(
        "GetHeld",
        lua.create_function(move |_, _args: MultiValue| Ok(held))?,
    )?;
    result.set(
        "GetHidden",
        lua.create_function(move |_, _args: MultiValue| Ok(hidden))?,
    )?;
    result.set(
        "GetTapNoteOffset",
        lua.create_function(move |_, _args: MultiValue| Ok(offset))?,
    )?;
    set_string_method(lua, result, "GetTapNoteScore", &score)?;
    Ok(())
}

pub fn run_actor_named_command(lua: &Lua, actor: &Table, name: &str) -> mlua::Result<()> {
    run_actor_named_command_with_drain(lua, actor, name, true)
}

pub fn run_actor_named_command_with_drain(
    lua: &Lua,
    actor: &Table,
    name: &str,
    drain_queue: bool,
) -> mlua::Result<()> {
    run_actor_named_command_with_drain_and_params(lua, actor, name, drain_queue, None)
}

pub fn run_actor_named_command_with_drain_and_params(
    lua: &Lua,
    actor: &Table,
    name: &str,
    drain_queue: bool,
    params: Option<Value>,
) -> mlua::Result<()> {
    let Some(command) = actor.get::<Option<Function>>(name)? else {
        return Ok(());
    };
    run_guarded_actor_command(lua, actor, name, &command, drain_queue, params)
}

pub fn actor_runs_startup_commands(actor: &Table) -> mlua::Result<bool> {
    let _ = actor;
    Ok(true)
}

fn actor_command_args(actor: &Table, params: Option<Value>) -> MultiValue {
    let mut args = MultiValue::new();
    args.push_back(Value::Table(actor.clone()));
    if let Some(params) = params {
        args.push_back(params);
    }
    args
}

pub fn call_actor_function(
    lua: &Lua,
    actor: &Table,
    command: &Function,
    params: Option<Value>,
) -> mlua::Result<()> {
    if let Some(script_dir) = actor
        .get::<Option<String>>("__songlua_script_dir")?
        .filter(|dir| !dir.trim().is_empty())
    {
        return call_with_script_dir(lua, Path::new(&script_dir), || {
            command.call::<()>(actor_command_args(actor, params))
        });
    }
    command.call::<()>(actor_command_args(actor, params))
}

fn run_guarded_actor_command(
    lua: &Lua,
    actor: &Table,
    name: &str,
    command: &Function,
    drain_queue: bool,
    params: Option<Value>,
) -> mlua::Result<()> {
    let active = actor_active_commands(lua, actor)?;
    if active.get::<Option<bool>>(name)?.unwrap_or(false) {
        return Ok(());
    }
    active.set(name, true)?;
    let result = call_actor_function(lua, actor, command, params).map_err(|err| {
        mlua::Error::external(format!(
            "{} failed for {}: {err}",
            name,
            actor_debug_label(actor)
        ))
    });
    active.set(name, Value::Nil)?;
    result?;
    if drain_queue {
        drain_actor_command_queue(lua, actor)?;
    }
    Ok(())
}

pub fn actor_active_commands(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if let Some(active) = actor.get::<Option<Table>>("__songlua_active_commands")? {
        return Ok(active);
    }
    let active = lua.create_table()?;
    actor.set("__songlua_active_commands", active.clone())?;
    Ok(active)
}

pub fn actor_command_queue(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if let Some(queue) = actor.get::<Option<Table>>("__songlua_command_queue")? {
        return Ok(queue);
    }
    let queue = lua.create_table()?;
    actor.set("__songlua_command_queue", queue.clone())?;
    Ok(queue)
}

pub fn drain_actor_command_queue(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let queue = actor_command_queue(lua, actor)?;
    while queue.raw_len() > 0 {
        let Some(name) = queue.raw_get::<Option<String>>(1)? else {
            break;
        };
        let len = queue.raw_len();
        for index in 1..len {
            let value = queue.raw_get::<Value>(index + 1)?;
            queue.raw_set(index, value)?;
        }
        queue.raw_set(len, Value::Nil)?;
        run_actor_named_command(lua, actor, &format!("{name}Command"))?;
    }
    Ok(())
}

pub fn actor_debug_label(actor: &Table) -> String {
    let actor_type = actor
        .get::<Option<String>>("__songlua_actor_type")
        .ok()
        .flatten()
        .unwrap_or_else(|| "Actor".to_string());
    let name = actor.get::<Option<String>>("Name").ok().flatten();
    match name {
        Some(name) if !name.trim().is_empty() => format!("{actor_type} '{name}'"),
        _ => actor_type,
    }
}

pub fn install_actor_metatable(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let mt = lua.create_table()?;
    let actor_clone = actor.clone();
    mt.set(
        "__concat",
        lua.create_function(move |lua, (_lhs, rhs): (Value, Value)| {
            if let Value::Table(rhs) = rhs {
                merge_actor_concat(lua, &actor_clone, &rhs)?;
            }
            Ok(actor_clone.clone())
        })?,
    )?;
    let actor_clone = actor.clone();
    mt.set(
        "__tostring",
        lua.create_function(move |_, _self: Value| Ok(actor_debug_label(&actor_clone)))?,
    )?;
    let _ = actor.set_metatable(Some(mt));
    Ok(())
}

fn merge_actor_concat(_lua: &Lua, actor: &Table, rhs: &Table) -> mlua::Result<()> {
    let next_index = actor.raw_len() + 1;
    let mut append_index = next_index;
    for value in rhs.sequence_values::<Value>() {
        actor.raw_set(append_index, value?)?;
        append_index += 1;
    }
    for pair in rhs.pairs::<Value, Value>() {
        let (key, value) = pair?;
        let is_sequence_key = match key {
            Value::Integer(index) => index >= 1,
            Value::Number(index) => index.is_finite() && index >= 1.0 && index.fract() == 0.0,
            _ => false,
        };
        if is_sequence_key {
            continue;
        }
        if matches!(
            &key,
            Value::String(text)
                if text.to_str().ok().is_some_and(|name|
                    name == "__songlua_actor_type"
                        || name == "__songlua_script_dir"
                        || name == "__songlua_song_dir")
        ) {
            continue;
        }
        actor.set(key, value)?;
    }
    Ok(())
}

pub fn actor_shadow_len(lua: &Lua, actor: &Table) -> mlua::Result<[f32; 2]> {
    let block = actor_current_capture_block(lua, actor)?;
    if let Some(value) = block
        .get::<Option<Table>>("shadow_len")?
        .and_then(|value| table_vec2(&value))
    {
        return Ok(value);
    }
    Ok(actor
        .get::<Option<Table>>("__songlua_state_shadow_len")?
        .and_then(|value| table_vec2(&value))
        .unwrap_or([0.0, 0.0]))
}

fn capture_actor_command(
    lua: &Lua,
    actor: &Table,
    command_name: &str,
) -> Result<Vec<SongLuaOverlayCommandBlock>, String> {
    let Some(command) = actor
        .get::<Option<Function>>(command_name)
        .map_err(|err| err.to_string())?
    else {
        return Ok(Vec::new());
    };
    reset_actor_capture(lua, actor).map_err(|err| err.to_string())?;
    let params =
        default_message_command_params(lua, command_name).map_err(|err| err.to_string())?;
    call_actor_function(lua, actor, &command, params).map_err(|err| err.to_string())?;
    flush_actor_capture(actor).map_err(|err| err.to_string())?;
    read_actor_capture_blocks(actor)
}

pub fn capture_actor_command_preserving_state(
    lua: &Lua,
    actor: &Table,
    command_name: &str,
) -> Result<Vec<SongLuaOverlayCommandBlock>, String> {
    let snapshot = snapshot_actor_mutable_state(lua, actor).map_err(|err| err.to_string())?;
    let captured = capture_actor_command(lua, actor, command_name);
    restore_actor_mutable_state(actor, snapshot).map_err(|err| err.to_string())?;
    captured
}

pub fn actor_current_capture_block(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    record_probe_actor_call(lua, actor)?;
    prepare_capture_scope_actor(lua, actor)?;
    if let Some(block) = actor.get::<Option<Table>>("__songlua_capture_block")? {
        return Ok(block);
    }
    let block = lua.create_table()?;
    let start = actor
        .get::<Option<f32>>("__songlua_capture_cursor")?
        .unwrap_or(0.0);
    let duration = actor
        .get::<Option<f32>>("__songlua_capture_duration")?
        .unwrap_or(0.0)
        .max(0.0);
    block.set("start", start)?;
    block.set("duration", duration)?;
    block.set("easing", actor.get::<Value>("__songlua_capture_easing")?)?;
    block.set("opt1", actor.get::<Value>("__songlua_capture_opt1")?)?;
    block.set("opt2", actor.get::<Value>("__songlua_capture_opt2")?)?;
    block.set("__songlua_has_changes", false)?;
    actor.set("__songlua_capture_block", block.clone())?;
    Ok(block)
}

pub fn capture_block_set_f32(lua: &Lua, actor: &Table, key: &str, value: f32) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    block.set(key, value)?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

pub fn capture_block_set_bool(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value: bool,
) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    block.set(key, value)?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

pub fn capture_block_set_color(lua: &Lua, actor: &Table, color: [f32; 4]) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, color[0])?;
    value.raw_set(2, color[1])?;
    value.raw_set(3, color[2])?;
    value.raw_set(4, color[3])?;
    block.set("diffuse", value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set("__songlua_diffuse", value.clone())?;
    actor.set("__songlua_state_diffuse", value)?;
    Ok(())
}

pub fn capture_actor_vertex_diffuse(
    lua: &Lua,
    actor: &Table,
    args: &MultiValue,
    corner_mask: u8,
) -> mlua::Result<()> {
    let Some(color) = read_color_args(args) else {
        return Ok(());
    };
    let mut colors = actor_vertex_colors(actor)?;
    for (index, vertex_color) in colors.iter_mut().enumerate() {
        if corner_mask & (1 << index) != 0 {
            *vertex_color = color;
        }
    }
    capture_block_set_vertex_colors(lua, actor, colors)
}

fn actor_text_attributes_table(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if let Some(attributes) = actor.get::<Option<Table>>("__songlua_text_attributes")? {
        return Ok(attributes);
    }
    let attributes = lua.create_table()?;
    actor.set("__songlua_text_attributes", attributes.clone())?;
    Ok(attributes)
}

pub fn capture_actor_text_attribute(
    lua: &Lua,
    actor: &Table,
    args: &MultiValue,
) -> mlua::Result<()> {
    let Some(start) = method_arg(args, 0).cloned().and_then(read_i32_value) else {
        return Ok(());
    };
    let Some(Value::Table(params)) = method_arg(args, 1).cloned() else {
        return Ok(());
    };
    let length = text_attribute_value(&params, &["Length", "length"])?
        .and_then(read_i32_value)
        .unwrap_or(1);
    if length == 0 {
        return Ok(());
    }
    let mut vertex_colors = text_attribute_value(&params, &["Diffuses", "diffuses"])?
        .and_then(read_vertex_colors_value)
        .unwrap_or([[1.0, 1.0, 1.0, 1.0]; 4]);
    if let Some(color) = text_attribute_value(
        &params,
        &[
            "Diffuse",
            "diffuse",
            "DiffuseColor",
            "diffusecolor",
            "Color",
            "color",
        ],
    )?
    .and_then(read_color_value)
    {
        vertex_colors = [color; 4];
    }
    let color = vertex_colors[0];
    let glow = text_attribute_value(&params, &["Glow", "glow"])?.and_then(read_color_value);

    let attr = lua.create_table()?;
    attr.raw_set("start", start.max(0))?;
    attr.raw_set("length", length)?;
    attr.raw_set("color", make_color_table(lua, color)?)?;
    if vertex_colors != [color; 4] {
        attr.raw_set(
            "vertex_colors",
            make_vertex_color_table(lua, vertex_colors)?,
        )?;
    }
    if let Some(glow) = glow {
        attr.raw_set("glow", make_color_table(lua, glow)?)?;
    }
    let attributes = actor_text_attributes_table(lua, actor)?;
    for existing in attributes.sequence_values::<Table>() {
        if text_attribute_matches(&existing?, start.max(0), length, color, vertex_colors, glow)? {
            return Ok(());
        }
    }
    attributes.raw_set(attributes.raw_len() + 1, attr)?;
    Ok(())
}

pub fn capture_block_set_vec4(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value4: [f32; 4],
) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, value4[0])?;
    value.raw_set(2, value4[1])?;
    value.raw_set(3, value4[2])?;
    value.raw_set(4, value4[3])?;
    block.set(key, value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

pub fn capture_block_set_vertex_colors(
    lua: &Lua,
    actor: &Table,
    colors: [[f32; 4]; 4],
) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = make_vertex_color_table(lua, colors)?;
    block.set("vertex_colors", value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set("__songlua_state_vertex_colors", value)?;
    Ok(())
}

pub fn capture_block_set_vec5(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value5: [f32; 5],
) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, value5[0])?;
    value.raw_set(2, value5[1])?;
    value.raw_set(3, value5[2])?;
    value.raw_set(4, value5[3])?;
    value.raw_set(5, value5[4])?;
    block.set(key, value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

pub fn capture_block_set_u32(lua: &Lua, actor: &Table, key: &str, value: u32) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    block.set(key, value)?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

pub fn capture_block_set_i32(lua: &Lua, actor: &Table, key: &str, value: i32) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    block.set(key, value)?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

pub fn capture_block_set_vec2(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value2: [f32; 2],
) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, value2[0])?;
    value.raw_set(2, value2[1])?;
    block.set(key, value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

pub fn capture_block_set_stretch(lua: &Lua, actor: &Table, rect: [f32; 4]) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, rect[0])?;
    value.raw_set(2, rect[1])?;
    value.raw_set(3, rect[2])?;
    value.raw_set(4, rect[3])?;
    block.set("stretch_rect", value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set("__songlua_state_stretch_rect", value)?;
    Ok(())
}

pub fn capture_block_set_size(lua: &Lua, actor: &Table, size: [f32; 2]) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, size[0])?;
    value.raw_set(2, size[1])?;
    block.set("size", value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set("__songlua_state_size", value)?;
    Ok(())
}

pub fn capture_block_set_zoom_axes(
    lua: &Lua,
    actor: &Table,
    zoom: f32,
    zoom_x_key: &str,
    zoom_y_key: &str,
    zoom_z_key: &str,
) -> mlua::Result<()> {
    capture_block_set_f32(lua, actor, zoom_x_key, zoom)?;
    capture_block_set_f32(lua, actor, zoom_y_key, zoom)?;
    capture_block_set_f32(lua, actor, zoom_z_key, zoom)?;
    Ok(())
}

pub fn capture_block_set_vec3(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value3: [f32; 3],
) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, value3[0])?;
    value.raw_set(2, value3[1])?;
    value.raw_set(3, value3[2])?;
    block.set(key, value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

pub fn capture_block_set_string(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value: &str,
) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    block.set(key, value)?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

pub fn capture_texture_rect(lua: &Lua, actor: &Table, rect: [f32; 4]) -> mlua::Result<()> {
    capture_block_set_u32(
        lua,
        actor,
        "sprite_state_index",
        SONG_LUA_SPRITE_STATE_CLEAR,
    )?;
    capture_block_set_vec4(lua, actor, "custom_texture_rect", rect)
}

pub fn actor_halign(actor: &Table) -> mlua::Result<f32> {
    Ok(actor
        .get::<Option<f32>>("__songlua_state_halign")?
        .unwrap_or(0.5))
}

pub fn actor_valign(actor: &Table) -> mlua::Result<f32> {
    Ok(actor
        .get::<Option<f32>>("__songlua_state_valign")?
        .unwrap_or(0.5))
}

pub fn set_actor_sprite_state(lua: &Lua, actor: &Table, state_index: u32) -> mlua::Result<()> {
    capture_block_set_u32(lua, actor, "sprite_state_index", state_index)?;
    let block = actor_current_capture_block(lua, actor)?;
    block.set("custom_texture_rect", Value::Nil)?;
    block.set("__songlua_has_changes", true)?;
    actor.set("__songlua_state_custom_texture_rect", Value::Nil)?;
    Ok(())
}

pub fn set_actor_effect_mode(lua: &Lua, actor: &Table, mode: &str) -> mlua::Result<()> {
    capture_block_set_string(lua, actor, "effect_mode", mode)
}

pub fn set_actor_effect_defaults(
    lua: &Lua,
    actor: &Table,
    mode: &str,
    period: Option<f32>,
    magnitude: Option<[f32; 3]>,
    color1: Option<[f32; 4]>,
    color2: Option<[f32; 4]>,
) -> mlua::Result<()> {
    set_actor_effect_mode(lua, actor, mode)?;
    if let Some(value) = period {
        capture_block_set_f32(lua, actor, "effect_period", value)?;
    }
    if let Some(value) = magnitude {
        capture_block_set_vec3(lua, actor, "effect_magnitude", value)?;
    }
    if let Some(value) = color1 {
        capture_block_set_vec4(lua, actor, "effect_color1", value)?;
    }
    if let Some(value) = color2 {
        capture_block_set_vec4(lua, actor, "effect_color2", value)?;
    }
    Ok(())
}

pub fn set_actor_texture_from_path(actor: &Table, path: &str) -> mlua::Result<bool> {
    let path = path.trim();
    if path.is_empty() {
        return Ok(false);
    }
    actor.set("Texture", path.to_string())?;
    actor.set("__songlua_aft_capture_name", Value::Nil)?;
    set_actor_decode_movie_for_texture(actor)?;
    Ok(true)
}

pub fn set_actor_texture_from_path_methods(
    actor: &Table,
    value: Option<&Value>,
    method_names: &[&str],
) -> mlua::Result<bool> {
    let Some(Value::Table(source)) = value else {
        return Ok(false);
    };
    for method_name in method_names {
        if let Value::String(path) = call_table_method(source, method_name)? {
            if set_actor_texture_from_path(actor, &path.to_str()?)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub fn set_actor_texture_from_path_methods_or_fallback(
    actor: &Table,
    value: Option<&Value>,
    method_names: &[&str],
    fallback_path: &str,
) -> mlua::Result<()> {
    if !set_actor_texture_from_path_methods(actor, value, method_names)? {
        set_actor_texture_from_path(actor, fallback_path)?;
    }
    Ok(())
}

pub fn set_actor_texture_from_value(
    actor: &Table,
    value: Option<&Value>,
    clear_on_nil: bool,
) -> mlua::Result<()> {
    match value {
        Some(Value::String(texture)) => {
            actor.set("Texture", texture.to_str()?.to_string())?;
            actor.set("__songlua_aft_capture_name", Value::Nil)?;
            set_actor_decode_movie_for_texture(actor)?;
        }
        Some(Value::Table(texture)) => {
            if let Some(capture_name) =
                texture.get::<Option<String>>("__songlua_aft_capture_name")?
            {
                actor.set("__songlua_aft_capture_name", capture_name)?;
                actor.set("Texture", Value::Nil)?;
                set_actor_decode_movie_for_texture(actor)?;
            } else if let Some(texture_path) = texture
                .get::<Option<String>>("__songlua_texture_path")?
                .filter(|path| !path.is_empty())
            {
                actor.set("Texture", texture_path)?;
                actor.set("__songlua_aft_capture_name", Value::Nil)?;
                set_actor_decode_movie_for_texture(actor)?;
            }
        }
        Some(Value::Nil) | None if clear_on_nil => {
            actor.set("Texture", Value::Nil)?;
            actor.set("__songlua_aft_capture_name", Value::Nil)?;
            actor.set("__songlua_state_decode_movie", false)?;
        }
        _ => {}
    }
    Ok(())
}

pub fn set_actor_sound_file_from_value(
    actor: &Table,
    value: Option<&Value>,
    clear_on_nil: bool,
) -> mlua::Result<()> {
    match value {
        Some(Value::String(file)) => actor.set("File", file.to_str()?.to_string())?,
        Some(Value::Nil) | None if clear_on_nil => actor.set("File", Value::Nil)?,
        _ => {}
    }
    Ok(())
}

pub fn actor_update_text_pre_zoom_flags(
    lua: &Lua,
    actor: &Table,
    update_width: bool,
    update_height: bool,
) -> mlua::Result<()> {
    if !actor_is_bitmap_text(actor)? {
        return Ok(());
    }
    if update_width && actor.get::<Option<bool>>("__songlua_text_saw_max_width")? == Some(true) {
        capture_block_set_bool(lua, actor, "max_w_pre_zoom", true)?;
    }
    if update_height && actor.get::<Option<bool>>("__songlua_text_saw_max_height")? == Some(true) {
        capture_block_set_bool(lua, actor, "max_h_pre_zoom", true)?;
    }
    Ok(())
}

pub fn run_command_on_leaves(lua: &Lua, actor: &Table, command: &Function) -> mlua::Result<()> {
    let mut saw_child = false;
    for child in actor_direct_children(lua, actor)? {
        saw_child = true;
        run_command_on_leaves(lua, &child, command)?;
    }
    if !saw_child {
        let _ = command.call::<Value>(actor.clone())?;
    }
    Ok(())
}

pub fn run_named_command_on_leaves(
    lua: &Lua,
    actor: &Table,
    command: &str,
    params: Option<Value>,
) -> mlua::Result<()> {
    let mut saw_child = false;
    for child in actor_direct_children(lua, actor)? {
        saw_child = true;
        run_named_command_on_leaves(lua, &child, command, params.clone())?;
    }
    if !saw_child {
        run_actor_named_command_with_drain_and_params(lua, actor, command, true, params)?;
    }
    Ok(())
}

pub fn run_named_command_on_children_recursively(
    lua: &Lua,
    actor: &Table,
    command: &str,
    params: Option<Value>,
) -> mlua::Result<()> {
    for child in actor_direct_children(lua, actor)? {
        run_actor_named_command_with_drain_and_params(lua, &child, command, true, params.clone())?;
        run_named_command_on_children_recursively(lua, &child, command, params.clone())?;
    }
    Ok(())
}

pub fn make_actor_chain_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |_, _args: MultiValue| Ok(actor.clone()))
}

pub fn make_actor_stop_tweening_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, _args: MultiValue| {
        prepare_capture_scope_actor(lua, &actor)?;
        flush_actor_capture(&actor)?;
        reset_actor_capture(lua, &actor)?;
        Ok(actor.clone())
    })
}

pub fn make_actor_finish_tweening_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, _args: MultiValue| {
        prepare_capture_scope_actor(lua, &actor)?;
        finish_actor_tweening(lua, &actor)?;
        Ok(actor.clone())
    })
}

pub fn make_actor_wrap_width_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, args: MultiValue| {
        let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
            return Ok(actor.clone());
        };
        let wrap = value as i32;
        if wrap >= 0 {
            capture_block_set_i32(lua, &actor, "wrap_width_pixels", wrap)?;
        }
        Ok(actor.clone())
    })
}

pub fn make_actor_set_size_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, args: MultiValue| {
        let Some(width) = method_arg(&args, 0).cloned().and_then(read_f32) else {
            return Ok(actor.clone());
        };
        let Some(height) = method_arg(&args, 1).cloned().and_then(read_f32) else {
            return Ok(actor.clone());
        };
        capture_block_set_size(lua, &actor, [width, height])?;
        Ok(actor.clone())
    })
}

pub fn make_actor_capture_f32_method(
    lua: &Lua,
    actor: &Table,
    key: &'static str,
    probe_name: Option<&'static str>,
) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, args: MultiValue| {
        if let Some(probe_name) = probe_name {
            record_probe_method_call(lua, &actor, probe_name)?;
        }
        if let Some(value) = args.get(1).cloned().and_then(read_f32) {
            capture_block_set_f32(lua, &actor, key, value)?;
        }
        Ok(actor.clone())
    })
}

pub fn make_actor_add_f32_method(
    lua: &Lua,
    actor: &Table,
    key: &'static str,
) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, args: MultiValue| {
        let Some(delta) = args.get(1).cloned().and_then(read_f32) else {
            return Ok(actor.clone());
        };
        let block = actor_current_capture_block(lua, &actor)?;
        let current = block
            .get::<Option<f32>>(key)?
            .or(actor.get::<Option<f32>>(format!("__songlua_state_{key}"))?)
            .unwrap_or(0.0);
        capture_block_set_f32(lua, &actor, key, current + delta)?;
        Ok(actor.clone())
    })
}

pub fn make_actor_tween_method(
    lua: &Lua,
    actor: &Table,
    easing: Option<&'static str>,
) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, args: MultiValue| {
        prepare_capture_scope_actor(lua, &actor)?;
        flush_actor_capture(&actor)?;
        let cursor = actor
            .get::<Option<f32>>("__songlua_capture_cursor")?
            .unwrap_or(0.0);
        let duration = args
            .get(1)
            .cloned()
            .and_then(read_f32)
            .unwrap_or(0.0)
            .max(0.0);
        actor.set("__songlua_capture_duration", duration)?;
        actor.set("__songlua_capture_tween_time_left", cursor + duration)?;
        actor.set("__songlua_capture_easing", easing)?;
        actor.set("__songlua_capture_opt1", Value::Nil)?;
        actor.set("__songlua_capture_opt2", Value::Nil)?;
        Ok(actor.clone())
    })
}

pub fn run_actor_init_commands_for_table(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if actor
        .get::<Option<bool>>("__songlua_init_commands_ran")?
        .unwrap_or(false)
    {
        return Ok(());
    }
    run_actor_named_command(lua, actor, "InitCommand")?;
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child? else {
            continue;
        };
        child.set("__songlua_parent", actor.clone())?;
        run_actor_init_commands_for_table(lua, &child)?;
    }
    run_song_meter_stream_init_command(lua, actor)?;
    actor.set("__songlua_init_commands_ran", true)?;
    Ok(())
}

pub fn run_actor_startup_commands_for_table(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if actor
        .get::<Option<bool>>("__songlua_startup_commands_ran")?
        .unwrap_or(false)
    {
        return Ok(());
    }
    actor.set("__songlua_startup_command_started", true)?;
    actor.set("__songlua_startup_children_walked", false)?;
    if actor_runs_startup_commands(actor)? {
        run_actor_named_command_with_drain(lua, actor, "OnCommand", false)?;
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child? else {
            continue;
        };
        child.set("__songlua_parent", actor.clone())?;
        run_actor_startup_commands_for_table(lua, &child)?;
    }
    run_song_meter_stream_startup_command(lua, actor)?;
    actor.set("__songlua_startup_children_walked", true)?;
    drain_actor_command_queue(lua, actor)?;
    actor.set("__songlua_startup_commands_ran", true)?;
    Ok(())
}

pub fn run_added_actor_child_commands(
    lua: &Lua,
    parent: &Table,
    child: &Table,
) -> mlua::Result<()> {
    if parent
        .get::<Option<bool>>("__songlua_init_commands_ran")?
        .unwrap_or(false)
    {
        run_actor_init_commands_for_table(lua, child)?;
    }
    let startup_already_needs_child = parent
        .get::<Option<bool>>("__songlua_startup_commands_ran")?
        .unwrap_or(false)
        || parent
            .get::<Option<bool>>("__songlua_startup_children_walked")?
            .unwrap_or(false);
    if startup_already_needs_child {
        run_actor_startup_commands_for_table(lua, child)?;
    }
    Ok(())
}

pub fn run_actor_init_commands(lua: &Lua, root: &Value) -> mlua::Result<()> {
    let Value::Table(root) = root else {
        return Ok(());
    };
    run_actor_init_commands_for_table(lua, root)
}

pub fn run_actor_startup_commands(lua: &Lua, root: &Value) -> mlua::Result<()> {
    let Value::Table(root) = root else {
        return Ok(());
    };
    run_actor_startup_commands_for_table(lua, root)
}

pub fn run_actor_update_functions(lua: &Lua, root: &Value) -> mlua::Result<()> {
    run_actor_update_functions_with_delta(lua, root, 1.0_f64 / 60.0)
}

pub fn run_actor_update_functions_with_delta(
    lua: &Lua,
    root: &Value,
    delta_seconds: f64,
) -> mlua::Result<()> {
    let Value::Table(root) = root else {
        return Ok(());
    };
    run_actor_update_functions_for_table(lua, root, delta_seconds)
}

pub fn actor_tree_has_update_functions(lua: &Lua, root: &Value) -> mlua::Result<bool> {
    let Value::Table(root) = root else {
        return Ok(false);
    };
    actor_table_has_update_functions(lua, root)
}

fn song_meter_stream_child(lua: &Lua, actor: &Table) -> mlua::Result<Option<Table>> {
    if !actor_type_is(actor, "SongMeterDisplay")? {
        return Ok(None);
    }
    let Some(stream) = actor_named_children(lua, actor)?.get::<Option<Table>>("Stream")? else {
        return Ok(None);
    };
    stream.set("__songlua_parent", actor.clone())?;
    Ok(Some(stream))
}

fn run_song_meter_stream_init_command(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some(stream) = song_meter_stream_child(lua, actor)? else {
        return Ok(());
    };
    run_actor_init_commands_for_table(lua, &stream)
}

fn run_song_meter_stream_startup_command(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some(stream) = song_meter_stream_child(lua, actor)? else {
        return Ok(());
    };
    run_actor_startup_commands_for_table(lua, &stream)
}

fn actor_update_rate(actor: &Table) -> mlua::Result<f64> {
    let rate = actor
        .get::<Option<f64>>("__songlua_update_rate")?
        .unwrap_or(1.0);
    Ok(if rate.is_finite() && rate > 0.0 {
        rate
    } else {
        1.0
    })
}

pub fn run_actor_update_functions_for_table(
    lua: &Lua,
    actor: &Table,
    parent_delta_seconds: f64,
) -> mlua::Result<()> {
    let delta_seconds = parent_delta_seconds * actor_update_rate(actor)?;
    if let Some(update) = actor.get::<Option<Function>>("__songlua_update_function")? {
        call_actor_function(lua, actor, &update, Some(Value::Number(delta_seconds)))?;
        drain_actor_command_queue(lua, actor)?;
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child? else {
            continue;
        };
        child.set("__songlua_parent", actor.clone())?;
        run_actor_update_functions_for_table(lua, &child, delta_seconds)?;
    }
    if let Some(stream) = song_meter_stream_child(lua, actor)? {
        run_actor_update_functions_for_table(lua, &stream, delta_seconds)?;
    }
    Ok(())
}

pub fn actor_table_has_update_functions(lua: &Lua, actor: &Table) -> mlua::Result<bool> {
    if actor
        .get::<Option<Function>>("__songlua_update_function")?
        .is_some()
    {
        return Ok(true);
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child? else {
            continue;
        };
        if actor_table_has_update_functions(lua, &child)? {
            return Ok(true);
        }
    }
    if let Some(stream) = song_meter_stream_child(lua, actor)? {
        return actor_table_has_update_functions(lua, &stream);
    }
    Ok(false)
}

pub fn actor_type_is(actor: &Table, expected: &str) -> mlua::Result<bool> {
    Ok(actor
        .get::<Option<String>>("__songlua_actor_type")?
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case(expected)))
}

pub fn actor_is_bitmap_text(actor: &Table) -> mlua::Result<bool> {
    Ok(actor
        .get::<Option<String>>("__songlua_actor_type")?
        .as_deref()
        .is_some_and(|kind| {
            kind.eq_ignore_ascii_case("BitmapText") || kind.eq_ignore_ascii_case("RollingNumbers")
        }))
}

pub fn note_zoom_point_hides(point: &Table) -> bool {
    let zoom_x = point.raw_get::<Value>(1).ok().and_then(read_f32);
    let zoom_y = point.raw_get::<Value>(2).ok().and_then(read_f32);
    let zoom_z = point.raw_get::<Value>(3).ok().and_then(read_f32);
    match (zoom_x, zoom_y, zoom_z) {
        (Some(x), Some(y), Some(z)) => x <= -0.99 && y <= -0.99 && z <= -0.99,
        _ => false,
    }
}

pub fn push_note_hide_window(
    out: &mut Vec<SongLuaNoteHideWindow>,
    player: usize,
    column: usize,
    beats_per_t: f32,
    start_index: usize,
    end_index: usize,
) {
    if start_index == 0 || end_index < start_index {
        return;
    }
    let start_beat = (start_index - 1) as f32 * beats_per_t;
    let end_beat = (end_index - 1) as f32 * beats_per_t;
    if !start_beat.is_finite() || !end_beat.is_finite() || end_beat < start_beat {
        return;
    }
    out.push(SongLuaNoteHideWindow {
        player,
        column,
        start_beat,
        end_beat,
    });
}

pub fn probe_target_kind(actor: &Table) -> mlua::Result<&'static str> {
    let Some(_player_index) = actor.get::<Option<i64>>("__songlua_player_index")? else {
        return Ok("overlay");
    };
    Ok(
        match actor
            .get::<Option<String>>("__songlua_player_child_name")?
            .as_deref()
        {
            None => "player",
            Some(name) if name.eq_ignore_ascii_case("NoteField") => "notefield",
            Some(name) if name.eq_ignore_ascii_case("Judgment") => "judgment",
            Some(name) if name.eq_ignore_ascii_case("Combo") => "combo",
            _ => "player-child",
        },
    )
}

pub fn record_probe_method_call(lua: &Lua, actor: &Table, method_name: &str) -> mlua::Result<()> {
    record_probe_actor_call(lua, actor)?;
    let globals = lua.globals();
    let Some(calls) = globals.get::<Option<Table>>(SONG_LUA_PROBE_METHODS_KEY)? else {
        return Ok(());
    };
    calls.raw_set(
        calls.raw_len() + 1,
        format!("{}.{}", probe_target_kind(actor)?, method_name),
    )?;
    Ok(())
}

fn record_probe_actor_call(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let globals = lua.globals();
    let Some(actors) = globals.get::<Option<Table>>(SONG_LUA_PROBE_ACTORS_KEY)? else {
        return Ok(());
    };
    let Some(seen) = globals.get::<Option<Table>>(SONG_LUA_PROBE_ACTOR_SET_KEY)? else {
        return Ok(());
    };
    if seen
        .raw_get::<Option<bool>>(actor.clone())?
        .unwrap_or(false)
    {
        return Ok(());
    }
    seen.raw_set(actor.clone(), true)?;
    actors.raw_set(actors.raw_len() + 1, actor.clone())?;
    Ok(())
}

pub fn prepare_capture_scope_actor(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let globals = lua.globals();
    let Some(actors) = globals.get::<Option<Table>>(SONG_LUA_CAPTURE_ACTORS_KEY)? else {
        return Ok(());
    };
    let Some(seen) = globals.get::<Option<Table>>(SONG_LUA_CAPTURE_ACTOR_SET_KEY)? else {
        return Ok(());
    };
    let Some(snapshots) = globals.get::<Option<Table>>(SONG_LUA_CAPTURE_SNAPSHOTS_KEY)? else {
        return Ok(());
    };
    if seen
        .raw_get::<Option<bool>>(actor.clone())?
        .unwrap_or(false)
    {
        return Ok(());
    }
    seen.raw_set(actor.clone(), true)?;
    actors.raw_set(actors.raw_len() + 1, actor.clone())?;

    let snapshot = lua.create_table()?;
    snapshot.raw_set(1, actor.clone())?;
    snapshot.raw_set(2, snapshot_actor_semantic_state_table(lua, actor)?)?;
    snapshots.raw_set(snapshots.raw_len() + 1, snapshot)?;

    reset_actor_capture(lua, actor)?;
    Ok(())
}

pub fn classify_function_ease_probe(calls: &Table) -> mlua::Result<Option<SongLuaEaseTarget>> {
    let mut saw_x = false;
    let mut saw_y = false;
    let mut saw_z = false;
    let mut saw_rotation_x = false;
    let mut saw_rotation_z = false;
    let mut saw_rotation_y = false;
    let mut saw_skew_x = false;
    let mut saw_skew_y = false;
    let mut saw_zoom = false;
    let mut saw_zoom_x = false;
    let mut saw_zoom_y = false;
    let mut saw_zoom_z = false;
    for value in calls.sequence_values::<String>() {
        let value = value?;
        let (target_kind, method_name) =
            value.split_once('.').unwrap_or(("player", value.as_str()));
        if !matches!(target_kind, "player" | "notefield" | "overlay") {
            return Ok(None);
        }
        match method_name {
            "x" if target_kind == "player" => saw_x = true,
            "y" if target_kind == "player" => saw_y = true,
            "z" if target_kind != "overlay" => saw_z = true,
            "rotationx" if target_kind != "overlay" => saw_rotation_x = true,
            "rotationz" if target_kind != "overlay" => saw_rotation_z = true,
            "rotationy" if target_kind != "overlay" => saw_rotation_y = true,
            "skewx" => saw_skew_x = true,
            "skewy" => saw_skew_y = true,
            "zoom" if target_kind != "overlay" => saw_zoom = true,
            "zoomx" if target_kind != "overlay" => saw_zoom_x = true,
            "zoomy" if target_kind != "overlay" => saw_zoom_y = true,
            "zoomz" if target_kind != "overlay" => saw_zoom_z = true,
            _ => return Ok(None),
        }
    }
    Ok(
        match (
            saw_x,
            saw_y,
            saw_z,
            saw_rotation_x,
            saw_rotation_z,
            saw_rotation_y,
            saw_skew_x,
            saw_skew_y,
            saw_zoom,
            saw_zoom_x,
            saw_zoom_y,
            saw_zoom_z,
        ) {
            (true, false, false, false, false, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerX)
            }
            (false, true, false, false, false, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerY)
            }
            (false, false, true, false, false, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerZ)
            }
            (false, false, false, true, false, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerRotationX)
            }
            (false, false, false, false, true, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerRotationZ)
            }
            (false, false, false, false, false, true, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerRotationY)
            }
            (false, false, false, false, false, false, true, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerSkewX)
            }
            (false, false, false, false, false, false, false, true, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerSkewY)
            }
            (false, false, false, false, false, false, false, false, true, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerZoom)
            }
            (false, false, false, false, false, false, false, false, false, true, false, false) => {
                Some(SongLuaEaseTarget::PlayerZoomX)
            }
            (false, false, false, false, false, false, false, false, false, false, true, false) => {
                Some(SongLuaEaseTarget::PlayerZoomY)
            }
            (false, false, false, false, false, false, false, false, false, false, false, true) => {
                Some(SongLuaEaseTarget::PlayerZoomZ)
            }
            _ => None,
        },
    )
}

pub fn probe_function_ease_target(
    lua: &Lua,
    function: &Function,
) -> mlua::Result<(Option<SongLuaEaseTarget>, Vec<String>, Vec<usize>)> {
    let globals = lua.globals();
    let previous_time = compile_song_runtime_values(lua)?;
    set_compile_song_runtime_beat(lua, 0.0)?;
    let previous_methods = globals.get::<Value>(SONG_LUA_PROBE_METHODS_KEY)?;
    let previous_actors = globals.get::<Value>(SONG_LUA_PROBE_ACTORS_KEY)?;
    let previous_actor_set = globals.get::<Value>(SONG_LUA_PROBE_ACTOR_SET_KEY)?;
    let calls = lua.create_table()?;
    let actors = lua.create_table()?;
    globals.set(SONG_LUA_PROBE_METHODS_KEY, calls.clone())?;
    globals.set(SONG_LUA_PROBE_ACTORS_KEY, actors.clone())?;
    globals.set(SONG_LUA_PROBE_ACTOR_SET_KEY, lua.create_table()?)?;
    let result = function.call::<Value>(1.0_f32);
    let methods = probe_call_names(&calls)?;
    let actor_ptrs = probe_actor_pointers(&actors)?;
    let classify = classify_function_ease_probe(&calls);
    globals.set(SONG_LUA_PROBE_METHODS_KEY, previous_methods)?;
    globals.set(SONG_LUA_PROBE_ACTORS_KEY, previous_actors)?;
    globals.set(SONG_LUA_PROBE_ACTOR_SET_KEY, previous_actor_set)?;
    set_compile_song_runtime_values(lua, previous_time.0, previous_time.1)?;
    match result {
        Ok(_) => Ok((classify?, methods, actor_ptrs)),
        Err(_) => Ok((None, methods, actor_ptrs)),
    }
}

pub fn probe_call_names(calls: &Table) -> mlua::Result<Vec<String>> {
    let mut out = Vec::new();
    for value in calls.sequence_values::<String>() {
        out.push(value?);
    }
    Ok(out)
}

pub fn probe_actor_pointers(actors: &Table) -> mlua::Result<Vec<usize>> {
    let mut out = Vec::new();
    for value in actors.sequence_values::<Table>() {
        out.push(value?.to_pointer() as usize);
    }
    Ok(out)
}

pub fn capture_scope_actor_pointers(actors: &Table) -> mlua::Result<HashSet<usize>> {
    let mut out = HashSet::with_capacity(actors.raw_len());
    for actor in actors.sequence_values::<Table>() {
        out.insert(actor?.to_pointer() as usize);
    }
    Ok(out)
}

pub fn capture_scope_actor_tables(actors: &Table) -> mlua::Result<Vec<Table>> {
    let mut out = Vec::with_capacity(actors.raw_len());
    for actor in actors.sequence_values::<Table>() {
        out.push(actor?);
    }
    Ok(out)
}

pub struct SongLuaActionCaptureScope {
    pub actors: Table,
    pub snapshots: Table,
    pub previous_actors: Value,
    pub previous_actor_set: Value,
    pub previous_snapshots: Value,
}

pub fn begin_action_capture_scope(lua: &Lua) -> mlua::Result<SongLuaActionCaptureScope> {
    let globals = lua.globals();
    let previous_actors = globals.get::<Value>(SONG_LUA_CAPTURE_ACTORS_KEY)?;
    let previous_actor_set = globals.get::<Value>(SONG_LUA_CAPTURE_ACTOR_SET_KEY)?;
    let previous_snapshots = globals.get::<Value>(SONG_LUA_CAPTURE_SNAPSHOTS_KEY)?;
    let actors = lua.create_table()?;
    let snapshots = lua.create_table()?;
    globals.set(SONG_LUA_CAPTURE_ACTORS_KEY, actors.clone())?;
    globals.set(SONG_LUA_CAPTURE_ACTOR_SET_KEY, lua.create_table()?)?;
    globals.set(SONG_LUA_CAPTURE_SNAPSHOTS_KEY, snapshots.clone())?;
    Ok(SongLuaActionCaptureScope {
        actors,
        snapshots,
        previous_actors,
        previous_actor_set,
        previous_snapshots,
    })
}

pub fn restore_action_capture_scope(
    lua: &Lua,
    scope: SongLuaActionCaptureScope,
) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.set(SONG_LUA_CAPTURE_ACTORS_KEY, scope.previous_actors)?;
    globals.set(SONG_LUA_CAPTURE_ACTOR_SET_KEY, scope.previous_actor_set)?;
    globals.set(SONG_LUA_CAPTURE_SNAPSHOTS_KEY, scope.previous_snapshots)?;
    Ok(())
}

pub fn capture_scope_snapshots(
    snapshots: &Table,
) -> mlua::Result<Vec<(Table, Vec<(String, Value)>)>> {
    let mut out = Vec::with_capacity(snapshots.raw_len());
    for snapshot in snapshots.sequence_values::<Table>() {
        let snapshot = snapshot?;
        let actor = snapshot.raw_get(1)?;
        let state = read_actor_semantic_state_table(&snapshot.raw_get(2)?)?;
        out.push((actor, state));
    }
    Ok(out)
}

pub fn reset_actor_capture_tables(lua: &Lua, actors: &[Table]) -> Result<(), String> {
    for actor in actors {
        reset_actor_capture(lua, actor).map_err(|err| err.to_string())?;
    }
    Ok(())
}

pub fn reset_tracked_capture_tables(
    lua: &Lua,
    tracked_actors: &[SongLuaTrackedActor],
) -> Result<(), String> {
    let indices: Vec<_> = (0..tracked_actors.len()).collect();
    reset_tracked_capture_tables_for_indices(lua, tracked_actors, &indices)
}

fn reset_tracked_capture_tables_for_indices(
    lua: &Lua,
    tracked_actors: &[SongLuaTrackedActor],
    indices: &[usize],
) -> Result<(), String> {
    for &index in indices {
        let Some(actor) = tracked_actors.get(index) else {
            continue;
        };
        reset_actor_capture(lua, &actor.table).map_err(|err| err.to_string())?;
    }
    Ok(())
}

pub fn collect_tracked_capture_blocks_for_indices(
    tracked_actors: &[SongLuaTrackedActor],
    indices: &[usize],
) -> Result<Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>, String> {
    let mut out = Vec::new();
    for &idx in indices {
        let Some(actor) = tracked_actors.get(idx) else {
            continue;
        };
        flush_actor_capture(&actor.table).map_err(|err| err.to_string())?;
        let blocks = read_actor_capture_blocks(&actor.table)?;
        if !blocks.is_empty() {
            out.push((idx, blocks));
        }
    }
    Ok(out)
}

pub fn tracked_indices_for_actor_pointers(
    tracked_actors: &[SongLuaTrackedActor],
    actor_ptrs: &HashSet<usize>,
) -> Vec<usize> {
    tracked_actors
        .iter()
        .enumerate()
        .filter(|(_, actor)| actor_ptrs.contains(&(actor.table.to_pointer() as usize)))
        .map(|(index, _)| index)
        .collect()
}

pub fn read_color_args(args: &MultiValue) -> Option<[f32; 4]> {
    if let Some(color) = method_arg(args, 0).cloned().and_then(read_color_value) {
        return Some(color);
    }
    let r = method_arg(args, 0).cloned().and_then(read_f32)?;
    let g = method_arg(args, 1).cloned().and_then(read_f32)?;
    let b = method_arg(args, 2).cloned().and_then(read_f32)?;
    let a = method_arg(args, 3)
        .cloned()
        .and_then(read_f32)
        .unwrap_or(1.0);
    Some([r, g, b, a])
}

pub fn reset_actor_capture(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set("__songlua_capture_cursor", 0.0_f32)?;
    actor.set("__songlua_capture_duration", 0.0_f32)?;
    actor.set("__songlua_capture_tween_time_left", 0.0_f32)?;
    actor.set("__songlua_capture_easing", Value::Nil)?;
    actor.set("__songlua_capture_opt1", Value::Nil)?;
    actor.set("__songlua_capture_opt2", Value::Nil)?;
    actor.set("__songlua_capture_blocks", lua.create_table()?)?;
    actor.set("__songlua_capture_block", Value::Nil)?;
    Ok(())
}

pub fn flush_actor_capture(actor: &Table) -> mlua::Result<()> {
    let Some(block) = actor.get::<Option<Table>>("__songlua_capture_block")? else {
        return Ok(());
    };
    let duration = block
        .get::<Option<f32>>("duration")?
        .unwrap_or(0.0)
        .max(0.0);
    let cursor = block.get::<Option<f32>>("start")?.unwrap_or(0.0);
    if block
        .get::<Option<bool>>("__songlua_has_changes")?
        .unwrap_or(false)
    {
        let blocks: Table = actor.get("__songlua_capture_blocks")?;
        let index = blocks.raw_len() + 1;
        blocks.raw_set(index, block.clone())?;
    }
    actor.set("__songlua_capture_cursor", cursor + duration)?;
    actor.set("__songlua_capture_duration", 0.0_f32)?;
    actor.set("__songlua_capture_easing", Value::Nil)?;
    actor.set("__songlua_capture_opt1", Value::Nil)?;
    actor.set("__songlua_capture_opt2", Value::Nil)?;
    actor.set("__songlua_capture_block", Value::Nil)?;
    Ok(())
}

pub fn actor_tween_time_left(actor: &Table) -> mlua::Result<f32> {
    Ok(actor
        .get::<Option<f32>>("__songlua_capture_tween_time_left")?
        .unwrap_or(0.0)
        .max(0.0))
}

fn scale_actor_capture_f32(actor: &Table, key: &str, scale: f32) -> mlua::Result<()> {
    let value = actor.get::<Option<f32>>(key)?.unwrap_or(0.0);
    actor.set(key, value * scale)
}

pub fn hurry_actor_tweening(actor: &Table, factor: f32) -> mlua::Result<()> {
    if !factor.is_finite() || factor <= f32::EPSILON {
        return Ok(());
    }
    flush_actor_capture(actor)?;
    let scale = factor.recip();
    if let Some(blocks) = actor.get::<Option<Table>>("__songlua_capture_blocks")? {
        for block in blocks.sequence_values::<Table>() {
            let block = block?;
            let start = block.get::<Option<f32>>("start")?.unwrap_or(0.0);
            let duration = block.get::<Option<f32>>("duration")?.unwrap_or(0.0);
            block.set("start", start * scale)?;
            block.set("duration", duration * scale)?;
        }
    }
    scale_actor_capture_f32(actor, "__songlua_capture_cursor", scale)?;
    scale_actor_capture_f32(actor, "__songlua_capture_duration", scale)?;
    scale_actor_capture_f32(actor, "__songlua_capture_tween_time_left", scale)
}

fn is_capture_block_meta_key(key: &str) -> bool {
    matches!(
        key,
        "start" | "duration" | "easing" | "opt1" | "opt2" | "__songlua_has_changes"
    )
}

pub fn finish_actor_tweening(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    flush_actor_capture(actor)?;
    let final_block = lua.create_table()?;
    let mut has_changes = false;
    if let Some(blocks) = actor.get::<Option<Table>>("__songlua_capture_blocks")? {
        for block in blocks.sequence_values::<Table>() {
            for pair in block?.pairs::<Value, Value>() {
                let (key, value) = pair?;
                if matches!(
                    &key,
                    Value::String(text)
                        if text
                            .to_str()
                            .ok()
                            .is_some_and(|name| is_capture_block_meta_key(&name))
                ) {
                    continue;
                }
                final_block.set(clone_lua_value(lua, key)?, clone_lua_value(lua, value)?)?;
                has_changes = true;
            }
        }
    }

    let blocks = lua.create_table()?;
    if has_changes {
        final_block.set("start", 0.0_f32)?;
        final_block.set("duration", 0.0_f32)?;
        final_block.set("easing", Value::Nil)?;
        final_block.set("opt1", Value::Nil)?;
        final_block.set("opt2", Value::Nil)?;
        final_block.set("__songlua_has_changes", true)?;
        blocks.raw_set(1, final_block)?;
    }
    actor.set("__songlua_capture_blocks", blocks)?;
    actor.set("__songlua_capture_block", Value::Nil)?;
    actor.set("__songlua_capture_cursor", 0.0_f32)?;
    actor.set("__songlua_capture_duration", 0.0_f32)?;
    actor.set("__songlua_capture_tween_time_left", 0.0_f32)?;
    actor.set("__songlua_capture_easing", Value::Nil)?;
    actor.set("__songlua_capture_opt1", Value::Nil)?;
    actor.set("__songlua_capture_opt2", Value::Nil)?;
    Ok(())
}

pub fn read_actor_capture_blocks(actor: &Table) -> Result<Vec<SongLuaOverlayCommandBlock>, String> {
    let Some(blocks) = actor
        .get::<Option<Table>>("__songlua_capture_blocks")
        .map_err(|err| err.to_string())?
    else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for value in blocks.sequence_values::<Value>() {
        let Value::Table(block) = value.map_err(|err| err.to_string())? else {
            continue;
        };
        let start = block
            .get::<Option<f32>>("start")
            .map_err(|err| err.to_string())?
            .unwrap_or(0.0);
        let duration = block
            .get::<Option<f32>>("duration")
            .map_err(|err| err.to_string())?
            .unwrap_or(0.0)
            .max(0.0);
        let easing = block
            .get::<Option<String>>("easing")
            .map_err(|err| err.to_string())?;
        out.push(SongLuaOverlayCommandBlock {
            start,
            duration,
            easing,
            opt1: block
                .get::<Option<f32>>("opt1")
                .map_err(|err| err.to_string())?,
            opt2: block
                .get::<Option<f32>>("opt2")
                .map_err(|err| err.to_string())?,
            delta: SongLuaOverlayStateDelta {
                x: block
                    .get::<Option<f32>>("x")
                    .map_err(|err| err.to_string())?,
                y: block
                    .get::<Option<f32>>("y")
                    .map_err(|err| err.to_string())?,
                z: block
                    .get::<Option<f32>>("z")
                    .map_err(|err| err.to_string())?,
                z_bias: block
                    .get::<Option<f32>>("z_bias")
                    .map_err(|err| err.to_string())?,
                draw_order: block
                    .get::<Option<i32>>("draw_order")
                    .map_err(|err| err.to_string())?,
                draw_by_z_position: block
                    .get::<Option<bool>>("draw_by_z_position")
                    .map_err(|err| err.to_string())?,
                halign: block
                    .get::<Option<f32>>("halign")
                    .map_err(|err| err.to_string())?,
                valign: block
                    .get::<Option<f32>>("valign")
                    .map_err(|err| err.to_string())?,
                text_align: block
                    .get::<Option<String>>("text_align")
                    .map_err(|err| err.to_string())?
                    .as_deref()
                    .and_then(parse_overlay_text_align),
                uppercase: block
                    .get::<Option<bool>>("uppercase")
                    .map_err(|err| err.to_string())?,
                shadow_len: block
                    .get::<Option<Table>>("shadow_len")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec2(&value)),
                shadow_color: block
                    .get::<Option<Table>>("shadow_color")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                glow: block
                    .get::<Option<Table>>("glow")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                fov: block
                    .get::<Option<f32>>("fov")
                    .map_err(|err| err.to_string())?,
                vanishpoint: block
                    .get::<Option<Table>>("vanishpoint")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec2(&value)),
                diffuse: block
                    .get::<Option<Table>>("diffuse")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                vertex_colors: block
                    .get::<Option<Table>>("vertex_colors")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vertex_colors(&value)),
                visible: block
                    .get::<Option<bool>>("visible")
                    .map_err(|err| err.to_string())?,
                cropleft: block
                    .get::<Option<f32>>("cropleft")
                    .map_err(|err| err.to_string())?,
                cropright: block
                    .get::<Option<f32>>("cropright")
                    .map_err(|err| err.to_string())?,
                croptop: block
                    .get::<Option<f32>>("croptop")
                    .map_err(|err| err.to_string())?,
                cropbottom: block
                    .get::<Option<f32>>("cropbottom")
                    .map_err(|err| err.to_string())?,
                fadeleft: block
                    .get::<Option<f32>>("fadeleft")
                    .map_err(|err| err.to_string())?,
                faderight: block
                    .get::<Option<f32>>("faderight")
                    .map_err(|err| err.to_string())?,
                fadetop: block
                    .get::<Option<f32>>("fadetop")
                    .map_err(|err| err.to_string())?,
                fadebottom: block
                    .get::<Option<f32>>("fadebottom")
                    .map_err(|err| err.to_string())?,
                mask_source: block
                    .get::<Option<bool>>("mask_source")
                    .map_err(|err| err.to_string())?,
                mask_dest: block
                    .get::<Option<bool>>("mask_dest")
                    .map_err(|err| err.to_string())?,
                depth_test: block
                    .get::<Option<bool>>("depth_test")
                    .map_err(|err| err.to_string())?,
                zoom: block
                    .get::<Option<f32>>("zoom")
                    .map_err(|err| err.to_string())?,
                zoom_x: block
                    .get::<Option<f32>>("zoom_x")
                    .map_err(|err| err.to_string())?,
                zoom_y: block
                    .get::<Option<f32>>("zoom_y")
                    .map_err(|err| err.to_string())?,
                zoom_z: block
                    .get::<Option<f32>>("zoom_z")
                    .map_err(|err| err.to_string())?,
                basezoom: block
                    .get::<Option<f32>>("basezoom")
                    .map_err(|err| err.to_string())?,
                basezoom_x: block
                    .get::<Option<f32>>("basezoom_x")
                    .map_err(|err| err.to_string())?,
                basezoom_y: block
                    .get::<Option<f32>>("basezoom_y")
                    .map_err(|err| err.to_string())?,
                basezoom_z: block
                    .get::<Option<f32>>("basezoom_z")
                    .map_err(|err| err.to_string())?,
                rot_x_deg: block
                    .get::<Option<f32>>("rot_x_deg")
                    .map_err(|err| err.to_string())?,
                rot_y_deg: block
                    .get::<Option<f32>>("rot_y_deg")
                    .map_err(|err| err.to_string())?,
                rot_z_deg: block
                    .get::<Option<f32>>("rot_z_deg")
                    .map_err(|err| err.to_string())?,
                skew_x: block
                    .get::<Option<f32>>("skew_x")
                    .map_err(|err| err.to_string())?,
                skew_y: block
                    .get::<Option<f32>>("skew_y")
                    .map_err(|err| err.to_string())?,
                blend: block
                    .get::<Option<String>>("blend")
                    .map_err(|err| err.to_string())?
                    .as_deref()
                    .and_then(parse_overlay_blend_mode),
                vibrate: block
                    .get::<Option<bool>>("vibrate")
                    .map_err(|err| err.to_string())?,
                effect_magnitude: block
                    .get::<Option<Table>>("effect_magnitude")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec3(&value)),
                effect_clock: block
                    .get::<Option<String>>("effect_clock")
                    .map_err(|err| err.to_string())?
                    .as_deref()
                    .and_then(parse_overlay_effect_clock),
                effect_mode: block
                    .get::<Option<String>>("effect_mode")
                    .map_err(|err| err.to_string())?
                    .as_deref()
                    .and_then(parse_overlay_effect_mode),
                effect_color1: block
                    .get::<Option<Table>>("effect_color1")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                effect_color2: block
                    .get::<Option<Table>>("effect_color2")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                effect_period: block
                    .get::<Option<f32>>("effect_period")
                    .map_err(|err| err.to_string())?,
                effect_offset: block
                    .get::<Option<f32>>("effect_offset")
                    .map_err(|err| err.to_string())?,
                effect_timing: block
                    .get::<Option<Table>>("effect_timing")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec5(&value)),
                rainbow: block
                    .get::<Option<bool>>("rainbow")
                    .map_err(|err| err.to_string())?,
                rainbow_scroll: block
                    .get::<Option<bool>>("rainbow_scroll")
                    .map_err(|err| err.to_string())?,
                text_jitter: block
                    .get::<Option<bool>>("text_jitter")
                    .map_err(|err| err.to_string())?,
                text_distortion: block
                    .get::<Option<f32>>("text_distortion")
                    .map_err(|err| err.to_string())?,
                text_glow_mode: block
                    .get::<Option<String>>("text_glow_mode")
                    .map_err(|err| err.to_string())?
                    .as_deref()
                    .and_then(parse_overlay_text_glow_mode),
                mult_attrs_with_diffuse: block
                    .get::<Option<bool>>("mult_attrs_with_diffuse")
                    .map_err(|err| err.to_string())?,
                sprite_animate: block
                    .get::<Option<bool>>("sprite_animate")
                    .map_err(|err| err.to_string())?,
                sprite_loop: block
                    .get::<Option<bool>>("sprite_loop")
                    .map_err(|err| err.to_string())?,
                sprite_playback_rate: block
                    .get::<Option<f32>>("sprite_playback_rate")
                    .map_err(|err| err.to_string())?,
                sprite_state_delay: block
                    .get::<Option<f32>>("sprite_state_delay")
                    .map_err(|err| err.to_string())?,
                sprite_state_index: block
                    .get::<Option<u32>>("sprite_state_index")
                    .map_err(|err| err.to_string())?,
                vert_spacing: block
                    .get::<Option<i32>>("vert_spacing")
                    .map_err(|err| err.to_string())?,
                wrap_width_pixels: block
                    .get::<Option<i32>>("wrap_width_pixels")
                    .map_err(|err| err.to_string())?,
                max_width: block
                    .get::<Option<f32>>("max_width")
                    .map_err(|err| err.to_string())?,
                max_height: block
                    .get::<Option<f32>>("max_height")
                    .map_err(|err| err.to_string())?,
                max_w_pre_zoom: block
                    .get::<Option<bool>>("max_w_pre_zoom")
                    .map_err(|err| err.to_string())?,
                max_h_pre_zoom: block
                    .get::<Option<bool>>("max_h_pre_zoom")
                    .map_err(|err| err.to_string())?,
                max_dimension_uses_zoom: block
                    .get::<Option<bool>>("max_dimension_uses_zoom")
                    .map_err(|err| err.to_string())?,
                texture_filtering: block
                    .get::<Option<bool>>("texture_filtering")
                    .map_err(|err| err.to_string())?,
                texture_wrapping: block
                    .get::<Option<bool>>("texture_wrapping")
                    .map_err(|err| err.to_string())?,
                texcoord_offset: block
                    .get::<Option<Table>>("texcoord_offset")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec2(&value)),
                custom_texture_rect: block
                    .get::<Option<Table>>("custom_texture_rect")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                texcoord_velocity: block
                    .get::<Option<Table>>("texcoord_velocity")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec2(&value)),
                size: block
                    .get::<Option<Table>>("size")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec2(&value)),
                stretch_rect: block
                    .get::<Option<Table>>("stretch_rect")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                sound_play: block
                    .get::<Option<bool>>("sound_play")
                    .map_err(|err| err.to_string())?,
            },
        });
    }
    Ok(out)
}

pub fn read_graph_display_size(
    state: SongLuaOverlayState,
    context: &SongLuaCompileContext,
) -> [f32; 2] {
    let fallback = graph_display_body_size(song_lua_human_player_count(context));
    let size = state.size.unwrap_or(fallback);
    [size[0].abs().max(1.0), size[1].abs().max(1.0)]
}

pub fn read_graph_display_values(actor: &Table) -> Result<Arc<[f32]>, String> {
    let values = actor
        .get::<Option<Table>>("__songlua_graph_display_values")
        .map_err(|err| err.to_string())?;
    let Some(values) = values else {
        return Ok(default_graph_display_values());
    };
    let mut out = Vec::with_capacity(GRAPH_DISPLAY_VALUE_RESOLUTION);
    for index in 1..=GRAPH_DISPLAY_VALUE_RESOLUTION {
        let value = values
            .raw_get::<Option<Value>>(index)
            .map_err(|err| err.to_string())?
            .and_then(read_f32)
            .unwrap_or(SONG_LUA_INITIAL_LIFE)
            .clamp(0.0, 1.0);
        out.push(value);
    }
    Ok(Arc::from(out.into_boxed_slice()))
}

fn default_graph_display_values() -> Arc<[f32]> {
    Arc::from([SONG_LUA_INITIAL_LIFE; GRAPH_DISPLAY_VALUE_RESOLUTION])
}

pub fn read_actor_multi_vertex_texture_path(
    actor: &Table,
    aft_capture_names: &HashSet<String>,
) -> Result<Option<PathBuf>, String> {
    if actor
        .get::<Option<String>>("__songlua_aft_capture_name")
        .map_err(|err| err.to_string())?
        .is_some()
    {
        return Ok(None);
    }
    let Some(texture) = actor
        .get::<Option<String>>("Texture")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    if aft_capture_names.contains(&texture) {
        return Ok(None);
    }
    Ok(resolve_actor_asset_path(actor, &texture).ok())
}

pub fn read_actor_multi_vertex_mesh(
    actor: &Table,
) -> Result<Option<Arc<[SongLuaOverlayMeshVertex]>>, String> {
    let Some(vertices) = actor
        .get::<Option<Table>>("__songlua_vertices")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let source = read_actor_multi_vertex_points(&vertices)?;
    let Some((start, end)) = read_actor_multi_vertex_range(actor, source.len())? else {
        return Ok(None);
    };
    let draw_mode = actor
        .get::<Option<String>>("__songlua_draw_state_mode")
        .map_err(|err| err.to_string())?
        .unwrap_or_else(|| "DrawMode_Triangles".to_string());
    let line_width = actor
        .get::<Option<f32>>("__songlua_line_width")
        .map_err(|err| err.to_string())?
        .unwrap_or(1.0)
        .max(0.0);
    let mesh = actor_multi_vertex_triangles(&source[start..end], draw_mode.as_str(), line_width);
    if mesh.is_empty() {
        Ok(None)
    } else {
        Ok(Some(Arc::from(mesh.into_boxed_slice())))
    }
}

pub fn read_model_path(actor: &Table) -> Result<Option<PathBuf>, String> {
    for key in ["Meshes", "Materials", "Bones"] {
        let Some(raw) = actor
            .get::<Option<String>>(key)
            .map_err(|err| err.to_string())?
            .filter(|path| !path.trim().is_empty())
        else {
            continue;
        };
        if let Ok(path) = resolve_actor_asset_path(actor, &raw) {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

pub fn collect_aft_capture_names(actor: &Table, out: &mut HashSet<String>) -> Result<(), String> {
    if actor
        .get::<Option<String>>("__songlua_actor_type")
        .map_err(|err| err.to_string())?
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("ActorFrameTexture"))
        && let Some(capture_name) = actor_aft_capture_name(actor).map_err(|err| err.to_string())?
    {
        out.insert(capture_name);
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child.map_err(|err| err.to_string())? else {
            continue;
        };
        collect_aft_capture_names(&child, out)?;
    }
    Ok(())
}

pub fn actor_aft_capture_name(actor: &Table) -> mlua::Result<Option<String>> {
    if let Some(capture_name) = actor
        .get::<Option<String>>("__songlua_aft_capture_name")?
        .filter(|name| !name.trim().is_empty())
    {
        return Ok(Some(capture_name));
    }
    Ok(actor
        .get::<Option<String>>("Name")?
        .filter(|name| !name.trim().is_empty()))
}

fn read_actor_multi_vertex_range(
    actor: &Table,
    vertex_count: usize,
) -> Result<Option<(usize, usize)>, String> {
    if vertex_count == 0 {
        return Ok(None);
    }
    let state = actor
        .get::<Option<Table>>("__songlua_draw_state")
        .map_err(|err| err.to_string())?;
    let first = state
        .as_ref()
        .and_then(|state| state.get::<Option<Value>>("First").ok().flatten())
        .and_then(read_i32_value)
        .unwrap_or(1)
        .max(1) as usize;
    let start = first.saturating_sub(1).min(vertex_count);
    let num = state
        .as_ref()
        .and_then(|state| state.get::<Option<Value>>("Num").ok().flatten())
        .and_then(read_i32_value)
        .unwrap_or(-1);
    let end = if num < 0 {
        vertex_count
    } else {
        start.saturating_add(num as usize).min(vertex_count)
    };
    Ok((end > start).then_some((start, end)))
}

fn read_actor_multi_vertex_points(
    vertices: &Table,
) -> Result<Vec<SongLuaActorMultiVertexPoint>, String> {
    let mut out = Vec::with_capacity(vertices.raw_len());
    for value in vertices.sequence_values::<Value>() {
        let Some(vertex) = read_actor_multi_vertex_point(value.map_err(|err| err.to_string())?)?
        else {
            continue;
        };
        out.push(vertex);
    }
    Ok(out)
}

fn read_actor_multi_vertex_point(
    value: Value,
) -> Result<Option<SongLuaActorMultiVertexPoint>, String> {
    let Value::Table(vertex) = value else {
        return Ok(None);
    };
    let Value::Table(position) = vertex.raw_get::<Value>(1).map_err(|err| err.to_string())? else {
        return Ok(None);
    };
    let Some(x) = read_f32(
        position
            .raw_get::<Value>(1)
            .map_err(|err| err.to_string())?,
    ) else {
        return Ok(None);
    };
    let Some(y) = read_f32(
        position
            .raw_get::<Value>(2)
            .map_err(|err| err.to_string())?,
    ) else {
        return Ok(None);
    };
    let color = read_color_value(vertex.raw_get::<Value>(2).map_err(|err| err.to_string())?)
        .unwrap_or([1.0; 4]);
    let uv = match vertex.raw_get::<Value>(3).map_err(|err| err.to_string())? {
        Value::Table(table) => [
            table
                .raw_get::<Value>(1)
                .ok()
                .and_then(read_f32)
                .unwrap_or(0.0),
            table
                .raw_get::<Value>(2)
                .ok()
                .and_then(read_f32)
                .unwrap_or(0.0),
        ],
        _ => [0.0, 0.0],
    };
    Ok(Some(SongLuaActorMultiVertexPoint {
        pos: [x, y],
        color,
        uv,
    }))
}

fn actor_multi_vertex_triangles(
    vertices: &[SongLuaActorMultiVertexPoint],
    draw_mode: &str,
    line_width: f32,
) -> Vec<SongLuaOverlayMeshVertex> {
    if draw_mode.eq_ignore_ascii_case("DrawMode_Quads") {
        return actor_multi_vertex_quads(vertices);
    }
    if draw_mode.eq_ignore_ascii_case("DrawMode_QuadStrip") {
        return actor_multi_vertex_quad_strip(vertices);
    }
    if draw_mode.eq_ignore_ascii_case("DrawMode_LineStrip") {
        return actor_multi_vertex_line_strip(vertices, line_width);
    }
    if draw_mode.eq_ignore_ascii_case("DrawMode_Fan") {
        return actor_multi_vertex_fan(vertices);
    }
    actor_multi_vertex_triangle_list(vertices)
}

fn actor_multi_vertex_triangle_list(
    vertices: &[SongLuaActorMultiVertexPoint],
) -> Vec<SongLuaOverlayMeshVertex> {
    let mut out = Vec::with_capacity(vertices.len() / 3 * 3);
    for chunk in vertices.chunks_exact(3) {
        push_actor_multi_vertex_triangle(&mut out, chunk[0], chunk[1], chunk[2]);
    }
    out
}

fn actor_multi_vertex_quads(
    vertices: &[SongLuaActorMultiVertexPoint],
) -> Vec<SongLuaOverlayMeshVertex> {
    let mut out = Vec::with_capacity(vertices.len() / 4 * 6);
    for chunk in vertices.chunks_exact(4) {
        push_actor_multi_vertex_quad(&mut out, chunk[0], chunk[1], chunk[2], chunk[3]);
    }
    out
}

fn actor_multi_vertex_quad_strip(
    vertices: &[SongLuaActorMultiVertexPoint],
) -> Vec<SongLuaOverlayMeshVertex> {
    let mut out = Vec::with_capacity(vertices.len().saturating_sub(2) / 2 * 6);
    let mut index = 0usize;
    while index + 3 < vertices.len() {
        push_actor_multi_vertex_quad(
            &mut out,
            vertices[index],
            vertices[index + 1],
            vertices[index + 3],
            vertices[index + 2],
        );
        index += 2;
    }
    out
}

fn actor_multi_vertex_fan(
    vertices: &[SongLuaActorMultiVertexPoint],
) -> Vec<SongLuaOverlayMeshVertex> {
    if vertices.len() < 3 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(vertices.len().saturating_sub(2) * 3);
    for index in 1..vertices.len() - 1 {
        push_actor_multi_vertex_triangle(
            &mut out,
            vertices[0],
            vertices[index],
            vertices[index + 1],
        );
    }
    out
}

fn actor_multi_vertex_line_strip(
    vertices: &[SongLuaActorMultiVertexPoint],
    line_width: f32,
) -> Vec<SongLuaOverlayMeshVertex> {
    if vertices.len() < 2 || line_width <= f32::EPSILON {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(vertices.len().saturating_sub(1) * 6);
    let half_width = 0.5 * line_width;
    let mut offsets = Vec::with_capacity(vertices.len());
    for index in 0..vertices.len() {
        let offset = if index == 0 {
            actor_multi_vertex_line_normal(vertices[0], vertices[1], half_width)
        } else if index + 1 == vertices.len() {
            actor_multi_vertex_line_normal(vertices[index - 1], vertices[index], half_width)
        } else {
            actor_multi_vertex_line_join_offset(
                vertices[index - 1],
                vertices[index],
                vertices[index + 1],
                half_width,
            )
        };
        offsets.push(offset);
    }
    for index in 0..vertices.len().saturating_sub(1) {
        let a = vertices[index];
        let b = vertices[index + 1];
        if actor_multi_vertex_segment_len(a, b) <= f32::EPSILON {
            continue;
        }
        let a0 = actor_multi_vertex_offset_point(a, offsets[index], 1.0);
        let a1 = actor_multi_vertex_offset_point(a, offsets[index], -1.0);
        let b0 = actor_multi_vertex_offset_point(b, offsets[index + 1], 1.0);
        let b1 = actor_multi_vertex_offset_point(b, offsets[index + 1], -1.0);
        push_actor_multi_vertex_triangle(&mut out, a0, b0, b1);
        push_actor_multi_vertex_triangle(&mut out, a0, b1, a1);
    }
    out
}

fn actor_multi_vertex_segment_len(
    a: SongLuaActorMultiVertexPoint,
    b: SongLuaActorMultiVertexPoint,
) -> f32 {
    (b.pos[0] - a.pos[0]).hypot(b.pos[1] - a.pos[1])
}

fn actor_multi_vertex_line_normal(
    a: SongLuaActorMultiVertexPoint,
    b: SongLuaActorMultiVertexPoint,
    half_width: f32,
) -> [f32; 2] {
    let dx = b.pos[0] - a.pos[0];
    let dy = b.pos[1] - a.pos[1];
    let len = dx.hypot(dy);
    if len <= f32::EPSILON {
        return [0.0, 0.0];
    }
    [-dy / len * half_width, dx / len * half_width]
}

fn actor_multi_vertex_line_join_offset(
    prev: SongLuaActorMultiVertexPoint,
    current: SongLuaActorMultiVertexPoint,
    next: SongLuaActorMultiVertexPoint,
    half_width: f32,
) -> [f32; 2] {
    let prev_normal = actor_multi_vertex_line_normal(prev, current, 1.0);
    let next_normal = actor_multi_vertex_line_normal(current, next, 1.0);
    let miter = [
        prev_normal[0] + next_normal[0],
        prev_normal[1] + next_normal[1],
    ];
    let miter_len = miter[0].hypot(miter[1]);
    if miter_len <= f32::EPSILON {
        return [prev_normal[0] * half_width, prev_normal[1] * half_width];
    }
    let miter = [miter[0] / miter_len, miter[1] / miter_len];
    let denom = miter[0] * prev_normal[0] + miter[1] * prev_normal[1];
    if denom.abs() <= 0.1 {
        return [next_normal[0] * half_width, next_normal[1] * half_width];
    }
    let scale = (half_width / denom).clamp(-half_width * 4.0, half_width * 4.0);
    [miter[0] * scale, miter[1] * scale]
}

fn actor_multi_vertex_offset_point(
    vertex: SongLuaActorMultiVertexPoint,
    offset: [f32; 2],
    sign: f32,
) -> SongLuaActorMultiVertexPoint {
    SongLuaActorMultiVertexPoint {
        pos: [
            vertex.pos[0] + offset[0] * sign,
            vertex.pos[1] + offset[1] * sign,
        ],
        color: vertex.color,
        uv: vertex.uv,
    }
}

fn push_actor_multi_vertex_quad(
    out: &mut Vec<SongLuaOverlayMeshVertex>,
    a: SongLuaActorMultiVertexPoint,
    b: SongLuaActorMultiVertexPoint,
    c: SongLuaActorMultiVertexPoint,
    d: SongLuaActorMultiVertexPoint,
) {
    push_actor_multi_vertex_triangle(out, a, b, c);
    push_actor_multi_vertex_triangle(out, c, d, a);
}

fn push_actor_multi_vertex_triangle(
    out: &mut Vec<SongLuaOverlayMeshVertex>,
    a: SongLuaActorMultiVertexPoint,
    b: SongLuaActorMultiVertexPoint,
    c: SongLuaActorMultiVertexPoint,
) {
    out.push(actor_multi_vertex_mesh_vertex(a));
    out.push(actor_multi_vertex_mesh_vertex(b));
    out.push(actor_multi_vertex_mesh_vertex(c));
}

#[inline(always)]
fn actor_multi_vertex_mesh_vertex(
    vertex: SongLuaActorMultiVertexPoint,
) -> SongLuaOverlayMeshVertex {
    SongLuaOverlayMeshVertex {
        pos: vertex.pos,
        color: vertex.color,
        uv: vertex.uv,
    }
}

pub fn actor_zoom_axis(actor: &Table, zoom_key: &str, basezoom_key: &str) -> mlua::Result<f32> {
    let zoom = actor
        .get::<Option<f32>>(zoom_key)?
        .or(actor.get::<Option<f32>>("__songlua_state_zoom")?)
        .unwrap_or(1.0);
    let basezoom = actor
        .get::<Option<f32>>(basezoom_key)?
        .or(actor.get::<Option<f32>>("__songlua_state_basezoom")?)
        .unwrap_or(1.0);
    Ok(basezoom * zoom)
}

pub fn actor_texture_path(actor: &Table) -> mlua::Result<Option<PathBuf>> {
    let Some(texture) = actor.get::<Option<String>>("Texture")? else {
        return Ok(None);
    };
    let texture = texture.trim();
    if texture.is_empty() {
        return Ok(None);
    }
    let raw = Path::new(texture);
    if raw.is_absolute() && raw.exists() {
        return Ok(Some(raw.to_path_buf()));
    }
    if let Some(script_dir) = actor.get::<Option<String>>("__songlua_script_dir")? {
        let candidate = Path::new(&script_dir).join(texture);
        if candidate.exists() {
            return Ok(Some(candidate));
        }
    }
    if let Some(song_dir) = actor.get::<Option<String>>("__songlua_song_dir")? {
        let candidate = Path::new(&song_dir).join(texture);
        if candidate.exists() {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

pub fn read_proxy_target_kind(actor: &Table) -> Result<Option<SongLuaProxyTarget>, String> {
    let Some(raw_kind) = actor
        .get::<Option<String>>("__songlua_proxy_target_kind")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let player_index = actor
        .get::<Option<i64>>("__songlua_proxy_player_index")
        .map_err(|err| err.to_string())?
        .and_then(|value| usize::try_from(value).ok());
    Ok(match raw_kind.as_str() {
        "player" => player_index.map(|player_index| SongLuaProxyTarget::Player { player_index }),
        "notefield" => {
            player_index.map(|player_index| SongLuaProxyTarget::NoteField { player_index })
        }
        "judgment" => {
            player_index.map(|player_index| SongLuaProxyTarget::Judgment { player_index })
        }
        "combo" => player_index.map(|player_index| SongLuaProxyTarget::Combo { player_index }),
        "underlay" => Some(SongLuaProxyTarget::Underlay),
        "overlay" => Some(SongLuaProxyTarget::Overlay),
        _ => None,
    })
}

pub fn resolve_actor_asset_path(actor: &Table, raw: &str) -> Result<PathBuf, String> {
    let raw_path = Path::new(raw.trim());
    if raw_path.is_absolute() && raw_path.is_file() {
        return Ok(raw_path.to_path_buf());
    }
    if let Some(script_dir) = actor
        .get::<Option<String>>("__songlua_script_dir")
        .map_err(|err| err.to_string())?
        .filter(|dir| !dir.trim().is_empty())
    {
        let candidate = Path::new(&script_dir).join(raw);
        if candidate.is_file() {
            return Ok(candidate);
        }
        if let Some(path) = resolve_actor_asset_prefix(Path::new(&script_dir), raw_path) {
            return Ok(path);
        }
    }
    Err(format!("actor asset '{}' could not be resolved", raw))
}

fn resolve_actor_asset_prefix(script_dir: &Path, raw_path: &Path) -> Option<PathBuf> {
    if raw_path.extension().is_some() {
        return None;
    }
    let prefix = raw_path.file_name()?.to_str()?.trim();
    if prefix.is_empty() {
        return None;
    }
    let dir = raw_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .map(|parent| script_dir.join(parent))
        .unwrap_or_else(|| script_dir.to_path_buf());
    if !dir.is_dir() {
        return None;
    }
    let key = (
        dir.to_string_lossy().to_ascii_lowercase(),
        prefix.to_ascii_lowercase(),
    );
    let cache = ACTOR_ASSET_PREFIX_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
        .cloned()
    {
        return cached;
    }
    let mut matches = fs::read_dir(&dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_song_lua_media_path(path))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.to_ascii_lowercase().starts_with(key.1.as_str()))
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        let left = left
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let right = right
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        left.cmp(&right)
    });
    let found = matches.into_iter().next();
    cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, found.clone());
    found
}

pub fn read_bitmap_font(actor: &Table) -> Result<Option<(&'static str, PathBuf)>, String> {
    let Some(font) = actor
        .get::<Option<String>>("Font")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    if let Ok(font_path) = resolve_actor_asset_path(actor, &font) {
        return Ok(Some((song_lua_font_name(font_path.as_path()), font_path)));
    }
    if song_lua_uses_builtin_theme_font(&font) {
        return Ok(Some((
            "miso",
            PathBuf::from("assets/fonts/miso/_miso light.ini"),
        )));
    }
    Ok(None)
}

pub fn read_bitmap_text_attributes(actor: &Table) -> Result<Arc<[TextAttribute]>, String> {
    let Some(attributes) = actor
        .get::<Option<Table>>("__songlua_text_attributes")
        .map_err(|err| err.to_string())?
    else {
        return Ok(Arc::<[TextAttribute]>::from([]));
    };
    let mut out = Vec::with_capacity(attributes.raw_len());
    for entry in attributes.sequence_values::<Table>() {
        let entry = entry.map_err(|err| err.to_string())?;
        let start_value = entry
            .raw_get::<Value>("start")
            .map_err(|err| err.to_string())?;
        let start = read_i32_value(start_value).unwrap_or(0).max(0) as usize;
        let length_value = entry
            .raw_get::<Value>("length")
            .map_err(|err| err.to_string())?;
        let length_raw = read_i32_value(length_value).unwrap_or(1);
        if length_raw == 0 {
            continue;
        }
        let length = if length_raw < 0 {
            usize::MAX
        } else {
            length_raw as usize
        };
        let color = entry
            .raw_get::<Table>("color")
            .ok()
            .and_then(|color| table_vec4(&color))
            .unwrap_or([1.0, 1.0, 1.0, 1.0]);
        let vertex_colors = entry
            .raw_get::<Table>("vertex_colors")
            .ok()
            .and_then(|colors| table_vertex_colors(&colors));
        let glow = entry
            .raw_get::<Table>("glow")
            .ok()
            .and_then(|color| table_vec4(&color));
        out.push(TextAttribute {
            start,
            length,
            color,
            vertex_colors,
            glow,
        });
    }
    Ok(Arc::from(out.into_boxed_slice()))
}

fn song_lua_uses_builtin_theme_font(raw: &str) -> bool {
    let trimmed = raw.trim().trim_start_matches('/');
    if trimmed.is_empty() {
        return false;
    }
    let normalized = trimmed.replace('\\', "/");
    normalized.starts_with(SONG_LUA_THEME_PATH_PREFIX)
        || (!normalized.contains('/') && Path::new(&normalized).extension().is_none())
}

fn song_lua_font_name(font_path: &Path) -> &'static str {
    static SONG_LUA_FONT_NAMES: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let canonical = font_path.to_string_lossy().replace('\\', "/");
    let cache = SONG_LUA_FONT_NAMES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(&name) = guard.get(&canonical) {
        return name;
    }
    let leaked = Box::leak(format!("songlua_font:{canonical}").into_boxed_str());
    guard.insert(canonical, leaked);
    leaked
}

pub fn actor_texture_is_video(actor: &Table) -> mlua::Result<bool> {
    if let Some(path) = actor_texture_path(actor)? {
        return Ok(is_song_lua_video_path(&path));
    }
    Ok(actor
        .get::<Option<String>>("Texture")?
        .is_some_and(|texture| is_song_lua_video_path(Path::new(texture.trim()))))
}

pub fn set_actor_decode_movie_for_texture(actor: &Table) -> mlua::Result<()> {
    actor.set(
        "__songlua_state_decode_movie",
        actor_texture_is_video(actor)?,
    )
}

pub fn actor_decode_movie(actor: &Table) -> mlua::Result<bool> {
    Ok(actor
        .get::<Option<bool>>("__songlua_state_decode_movie")?
        .unwrap_or(actor_texture_is_video(actor)?))
}

pub fn read_actor_color_field(actor: &Table, key: &str) -> Result<Option<[f32; 4]>, String> {
    Ok(actor
        .get::<Option<Table>>(key)
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value)))
}

pub fn actor_diffuse(actor: &Table) -> mlua::Result<[f32; 4]> {
    Ok(actor
        .get::<Option<Table>>("__songlua_diffuse")?
        .and_then(|value| table_vec4(&value))
        .unwrap_or([1.0, 1.0, 1.0, 1.0]))
}

pub fn actor_vertex_colors(actor: &Table) -> mlua::Result<[[f32; 4]; 4]> {
    Ok(actor
        .get::<Option<Table>>("__songlua_state_vertex_colors")?
        .and_then(|value| table_vertex_colors(&value))
        .unwrap_or([[1.0, 1.0, 1.0, 1.0]; 4]))
}

pub fn text_attribute_matches(
    attr: &Table,
    start: i32,
    length: i32,
    color: [f32; 4],
    vertex_colors: [[f32; 4]; 4],
    glow: Option<[f32; 4]>,
) -> mlua::Result<bool> {
    if attr.raw_get::<Option<i32>>("start")?.unwrap_or(-1) != start
        || attr.raw_get::<Option<i32>>("length")?.unwrap_or(0) != length
    {
        return Ok(false);
    }
    let existing_color = attr
        .raw_get::<Option<Table>>("color")?
        .and_then(|value| table_vec4(&value))
        .unwrap_or([1.0; 4]);
    let existing_vertex_colors = attr
        .raw_get::<Option<Table>>("vertex_colors")?
        .and_then(|value| table_vertex_colors(&value))
        .unwrap_or([existing_color; 4]);
    let existing_glow = attr
        .raw_get::<Option<Table>>("glow")?
        .and_then(|value| table_vec4(&value));
    Ok(existing_color == color && existing_vertex_colors == vertex_colors && existing_glow == glow)
}

pub fn text_attribute_value(params: &Table, keys: &[&str]) -> mlua::Result<Option<Value>> {
    for key in keys {
        let value = params.get::<Value>(*key)?;
        if !matches!(value, Value::Nil) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

pub fn actor_glow(actor: &Table) -> mlua::Result<[f32; 4]> {
    Ok(actor
        .get::<Option<Table>>("__songlua_state_glow")?
        .and_then(|value| table_vec4(&value))
        .unwrap_or([0.0, 0.0, 0.0, 0.0]))
}

pub fn actor_effect_magnitude(actor: &Table) -> mlua::Result<[f32; 3]> {
    Ok(actor
        .get::<Option<Table>>("__songlua_state_effect_magnitude")?
        .and_then(|value| table_vec3(&value))
        .unwrap_or([0.0, 0.0, 0.0]))
}

pub fn banner_sort_order_path(sort_order: &str) -> Option<String> {
    let short = sort_order
        .trim()
        .split_once('_')
        .map(|(_, short)| short)
        .unwrap_or(sort_order.trim());
    let short = short
        .strip_suffix("_P1")
        .or_else(|| short.strip_suffix("_P2"))
        .unwrap_or(short);
    match short {
        "Group" => None,
        "" | "Invalid" => Some(theme_path("G", "Common", "fallback banner")),
        sort => Some(theme_path("G", "Banner", sort)),
    }
}

pub fn song_lua_screen_size(lua: &Lua) -> mlua::Result<(f32, f32)> {
    let globals = lua.globals();
    let width = globals.get::<Option<i32>>("SCREEN_WIDTH")?.unwrap_or(640) as f32;
    let height = globals.get::<Option<i32>>("SCREEN_HEIGHT")?.unwrap_or(480) as f32;
    Ok((width, height))
}

pub fn song_lua_screen_center(lua: &Lua) -> mlua::Result<(f32, f32)> {
    let globals = lua.globals();
    let center_x = globals
        .get::<Option<f32>>("SCREEN_CENTER_X")?
        .unwrap_or(song_lua_screen_size(lua)?.0 * 0.5);
    let center_y = globals
        .get::<Option<f32>>("SCREEN_CENTER_Y")?
        .unwrap_or(song_lua_screen_size(lua)?.1 * 0.5);
    Ok((center_x, center_y))
}

pub fn lua_format_text(lua: &Lua, args: &MultiValue) -> mlua::Result<String> {
    let offset = method_arg_offset(args);
    if args.get(offset).is_none() {
        return Ok(String::new());
    }
    let mut call_args = MultiValue::new();
    for index in offset..args.len() {
        if let Some(value) = args.get(index) {
            call_args.push_back(value.clone());
        }
    }
    let string_table = lua.globals().get::<Table>("string")?;
    let format = string_table.get::<Function>("format")?;
    lua_text_value(format.call::<Value>(call_args)?)
}

pub fn create_string_array(lua: &Lua, values: &[&str]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (index, value) in values.iter().enumerate() {
        table.raw_set(index + 1, *value)?;
    }
    Ok(table)
}

pub fn create_owned_string_array(lua: &Lua, values: &[String]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (index, value) in values.iter().enumerate() {
        table.raw_set(index + 1, value.as_str())?;
    }
    Ok(table)
}

pub fn create_bool_array(lua: &Lua, values: &[bool]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (index, value) in values.iter().enumerate() {
        table.raw_set(index + 1, *value)?;
    }
    Ok(table)
}

pub fn create_debug_table(lua: &Lua) -> mlua::Result<Table> {
    let debug = lua.create_table()?;
    debug.set(
        "getinfo",
        lua.create_function(|lua, args: MultiValue| {
            let globals = lua.globals();
            let source = globals
                .get::<Option<String>>("__songlua_current_script_path")?
                .map(|path| format!("@{path}"))
                .unwrap_or_else(|| "=[songlua]".to_string());
            let info = lua.create_table()?;
            info.set("source", source)?;
            info.set("short_src", info.get::<String>("source")?)?;
            info.set("what", "Lua")?;
            info.set("currentline", 0)?;
            info.set("linedefined", 0)?;
            info.set("lastlinedefined", 0)?;
            if args.front().is_some() {
                info.set("namewhat", "")?;
            }
            Ok(info)
        })?,
    )?;
    debug.set(
        "getupvalue",
        lua.create_function(|lua, args: MultiValue| {
            let Some(function) = args.front().cloned().and_then(|value| match value {
                Value::Function(function) => Some(function),
                _ => None,
            }) else {
                return Ok((Value::Nil, Value::Nil));
            };
            let Some(index) = args.get(1).cloned().and_then(|value| match value {
                Value::Integer(value) => Some(value),
                Value::Number(value) => Some(value as i64),
                _ => None,
            }) else {
                return Ok((Value::Nil, Value::Nil));
            };
            // SAFETY: exec_raw owns the temporary stack frame for this call. We only
            // read the pushed function/index arguments, call Lua's debug API to fetch
            // a single upvalue, then replace the frame contents with plain Lua return
            // values before exec_raw converts them back into mlua Values.
            unsafe {
                lua.exec_raw((function, index), |state| {
                    let upvalue_index = ffi::lua_tointeger(state, 2) as c_int;
                    let name = ffi::lua_getupvalue(state, 1, upvalue_index);
                    ffi::lua_remove(state, 2);
                    ffi::lua_remove(state, 1);
                    if name.is_null() {
                        ffi::lua_pushnil(state);
                        ffi::lua_pushnil(state);
                        return;
                    }
                    ffi::lua_pushstring(state, name);
                    ffi::lua_insert(state, -2);
                })
            }
        })?,
    )?;
    debug.set(
        "traceback",
        lua.create_function(|_, args: MultiValue| {
            let message = args
                .front()
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            Ok(message)
        })?,
    )?;
    Ok(debug)
}

#[inline(always)]
pub fn make_color_table(lua: &Lua, rgba: [f32; 4]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set(1, rgba[0])?;
    table.raw_set(2, rgba[1])?;
    table.raw_set(3, rgba[2])?;
    table.raw_set(4, rgba[3])?;
    Ok(table)
}

pub fn table_vec2(table: &Table) -> Option<[f32; 2]> {
    Some([table.raw_get::<f32>(1).ok()?, table.raw_get::<f32>(2).ok()?])
}

pub fn table_vec3(table: &Table) -> Option<[f32; 3]> {
    Some([
        table.raw_get::<f32>(1).ok()?,
        table.raw_get::<f32>(2).ok()?,
        table.raw_get::<f32>(3).ok()?,
    ])
}

pub fn table_vec4(table: &Table) -> Option<[f32; 4]> {
    Some([
        table.raw_get::<f32>(1).ok()?,
        table.raw_get::<f32>(2).ok()?,
        table.raw_get::<f32>(3).ok()?,
        table.raw_get::<f32>(4).ok()?,
    ])
}

pub fn table_vec5(table: &Table) -> Option<[f32; 5]> {
    Some([
        table.raw_get::<f32>(1).ok()?,
        table.raw_get::<f32>(2).ok()?,
        table.raw_get::<f32>(3).ok()?,
        table.raw_get::<f32>(4).ok()?,
        table.raw_get::<f32>(5).ok()?,
    ])
}

pub fn table_vertex_colors(table: &Table) -> Option<[[f32; 4]; 4]> {
    Some([
        table_vec4(&table.raw_get::<Table>(1).ok()?)?,
        table_vec4(&table.raw_get::<Table>(2).ok()?)?,
        table_vec4(&table.raw_get::<Table>(3).ok()?)?,
        table_vec4(&table.raw_get::<Table>(4).ok()?)?,
    ])
}

pub fn make_vertex_color_table(lua: &Lua, colors: [[f32; 4]; 4]) -> mlua::Result<Table> {
    let out = lua.create_table()?;
    for (index, color) in colors.into_iter().enumerate() {
        out.raw_set(index + 1, make_color_table(lua, color)?)?;
    }
    Ok(out)
}

pub fn create_color_constants_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (name, text) in [
        ("Black", "0,0,0,1"),
        ("Blue", "#00aeef"),
        ("Green", "#39b54a"),
        ("HoloBlue", "#33B5E5"),
        ("HoloDarkBlue", "#0099CC"),
        ("HoloDarkGreen", "#669900"),
        ("HoloDarkOrange", "#FF8800"),
        ("HoloDarkPurple", "#9933CC"),
        ("HoloDarkRed", "#CC0000"),
        ("HoloGreen", "#99CC00"),
        ("HoloOrange", "#FFBB33"),
        ("HoloPurple", "#AA66CC"),
        ("HoloRed", "#FF4444"),
        ("Invisible", "1,1,1,0"),
        ("Orange", "#f7941d"),
        ("Outline", "0,0,0,0.5"),
        ("Pink", "1,0.75,0.8,1"),
        ("Purple", "#92278f"),
        ("Red", "#ed1c24"),
        ("Stealth", "0,0,0,0"),
        ("White", "1,1,1,1"),
        ("Yellow", "#fff200"),
    ] {
        table.set(
            name,
            make_color_table(lua, parse_color_text(text).unwrap_or([1.0, 1.0, 1.0, 1.0]))?,
        )?;
    }
    table.set(
        "Alpha",
        lua.create_function(|lua, (color, alpha): (Value, f32)| {
            let mut color = read_color_value(color).unwrap_or([1.0, 1.0, 1.0, 1.0]);
            color[3] = alpha;
            make_color_table(lua, color)
        })?,
    )?;
    let table_for_call = table.clone();
    let mt = lua.create_table()?;
    mt.set(
        "__call",
        lua.create_function(move |lua, (_self, value): (Table, Value)| {
            let Some(name) = read_string(value) else {
                return Ok(Value::Nil);
            };
            let existing = table_for_call.get::<Value>(name.as_str())?;
            if !matches!(existing, Value::Nil) {
                return Ok(existing);
            }
            Ok(parse_color_text(&name)
                .map(|color| make_color_table(lua, color).map(Value::Table))
                .transpose()?
                .unwrap_or(Value::Nil))
        })?,
    )?;
    let _ = table.set_metatable(Some(mt));
    Ok(table)
}

#[inline(always)]
fn table_color(table: &Table) -> Option<[f32; 4]> {
    Some([
        table.raw_get::<f32>(1).ok()?,
        table.raw_get::<f32>(2).ok()?,
        table.raw_get::<f32>(3).ok()?,
        table.raw_get::<Option<f32>>(4).ok()?.unwrap_or(1.0),
    ])
}

#[inline(always)]
pub fn read_color_value(value: Value) -> Option<[f32; 4]> {
    match value {
        Value::Table(table) => table_color(&table),
        Value::String(text) => Some(parse_color_text(&text.to_str().ok()?).unwrap_or([1.0; 4])),
        _ => None,
    }
}

pub fn read_vertex_colors_value(value: Value) -> Option<[[f32; 4]; 4]> {
    let Value::Table(table) = value else {
        return None;
    };
    let mut saw_color = false;
    let mut colors = [[1.0, 1.0, 1.0, 1.0]; 4];
    for (index, color) in colors.iter_mut().enumerate() {
        if let Some(value) = table
            .raw_get::<Value>(index + 1)
            .ok()
            .and_then(read_color_value)
        {
            *color = value;
            saw_color = true;
        }
    }
    saw_color.then_some(colors)
}

pub fn read_color_call(args: &MultiValue) -> Option<[f32; 4]> {
    if let Some(color) = args.front().cloned().and_then(read_color_value) {
        return Some(color);
    }
    let r = args.front().cloned().and_then(read_f32)?;
    let g = args.get(1).cloned().and_then(read_f32)?;
    let b = args.get(2).cloned().and_then(read_f32)?;
    let a = args.get(3).cloned().and_then(read_f32).unwrap_or(1.0);
    Some([r, g, b, a])
}

#[inline(always)]
pub fn method_arg(args: &MultiValue, index: usize) -> Option<&Value> {
    let offset = method_arg_offset(args);
    args.get(offset + index)
}

#[inline(always)]
pub fn method_arg_offset(args: &MultiValue) -> usize {
    usize::from(matches!(args.front(), Some(Value::Table(_))))
}
