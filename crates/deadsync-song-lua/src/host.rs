use std::collections::HashMap;
use std::ffi::c_void;
use std::path::Path;

use chrono::{Datelike, Local, Timelike};
use std::sync::Arc;

use mlua::{Function, Lua, MultiValue, Table, Value};

use crate::{
    LUA_PLAYERS, SONG_LUA_EASING_NAME_KEY, SONG_LUA_EASING_NAMES,
    SONG_LUA_PLAYER_OPTION_CAPABILITIES, SONG_LUA_PLAYER_OPTION_MULTICOL_PREFIXES,
    SONG_LUA_PLAYER_OPTIONS_KEYS, SONG_LUA_PRODUCT_FAMILY, SONG_LUA_PRODUCT_ID,
    SONG_LUA_PRODUCT_VERSION, SONG_LUA_RUNTIME_BEAT_KEY, SONG_LUA_RUNTIME_KEY,
    SONG_LUA_RUNTIME_SECONDS_KEY, SONG_LUA_SIDE_EFFECT_COUNT_KEY, SONG_LUA_SOUND_CALLS_KEY,
    SONG_LUA_SOUND_PATHS_KEY, SONG_LUA_SUPPRESS_BROADCAST_KEY, SongLuaCompileContext,
    SongLuaNoteskinResolver, THEME_RECEPTOR_Y_REV, THEME_RECEPTOR_Y_STD,
    broadcast_song_lua_message, create_branch_table, create_charman_table, create_conf_option_row,
    create_course_table, create_custom_option_row, create_difficulty_table,
    create_display_bpms_table, create_display_table, create_enabled_players_table,
    create_game_table, create_gameman_table, create_gameplay_layout, create_hooks_table,
    create_memcardman_table, create_noteskin_table, create_operator_menu_option_rows_table,
    create_other_player_table, create_player_number_table, create_player_tables,
    create_prefsmgr_table, create_profileman_table, create_screen_system_layer_helpers_table,
    create_screen_table, create_sl_custom_prefs_table, create_sl_table, create_song_options_table,
    create_song_position_table, create_song_runtime_table, create_song_table,
    create_song_util_table, create_songman_table, create_statsman_table, create_string_enum_table,
    create_style_table, create_theme_prefs_rows_table, create_theme_prefs_table,
    create_theme_table, create_top_screen_table, create_trail_table, create_unlockman_table,
    current_song_lua_style_name, display_bpms_for_args, display_bpms_text,
    format_number_and_suffix, install_def_globals,
    install_default_stdlib_compat, install_file_loader_globals, is_song_lua_audio_path,
    lua_values_equal, method_arg, note_song_lua_side_effect, player_index_from_value,
    player_number_name, read_f32, read_string, record_song_lua_broadcast, resolve_script_path,
    scale_value, seconds_to_hhmmss, seconds_to_mmss, seconds_to_mmss_ms_ms, seconds_to_mss,
    seconds_to_mss_ms_ms, song_dir_string, song_lua_human_player_count,
    song_lua_runtime_number, song_lua_style_column_x, song_music_rate, theme_string, truthy,
};

pub const SONG_LUA_STARTUP_MESSAGE: &str = "__songlua_startup";

#[derive(Default)]
pub struct SongLuaHostState {
    pub easing_names: HashMap<*const c_void, String>,
}

pub(crate) struct SongLuaScreenRequest(pub String);

fn initialize_gameplay_steps(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<()> {
    let Some(human) = context.players.iter().position(|player| player.enabled) else {
        return Ok(());
    };
    let state = lua.app_data_ref::<SongLuaGameStateGlobals>()
        .ok_or_else(|| mlua::Error::runtime("missing gameplay selection state"))?;
    let steps = state.current_steps.raw_get::<Option<Table>>(human + 1)?;
    // ScreenGameplay::Init seeds every nonhuman potential CPU player before
    // screen actors are loaded. Preserve the owned Steps pointer directly;
    // Lua SetCurrentSteps has a different, enabled-player-only policy.
    for (other, player) in context.players.iter().enumerate() {
        if !player.enabled {
            state.current_steps.raw_set(other + 1, steps.clone())?;
        }
    }
    Ok(())
}

#[cfg(feature = "test-support")]
pub(crate) fn selected_step_indices(lua: &Lua, step_count: usize) -> mlua::Result<[Option<usize>; LUA_PLAYERS]> {
    let state = lua.app_data_ref::<SongLuaGameStateGlobals>()
        .ok_or_else(|| mlua::Error::runtime("missing gameplay selection state"))?;
    let mut indices = [None; LUA_PLAYERS];
    for (player, index) in indices.iter_mut().enumerate() {
        let steps = state.current_steps.raw_get::<Option<Table>>(player + 1)?;
        *index = steps.as_ref().and_then(|steps| state.step_objects.iter()
            .position(|(known, _)| known.to_pointer() == steps.to_pointer()))
            .filter(|&index| index < step_count);
    }
    Ok(indices)
}

pub(crate) fn selected_scene_context(
    lua: &Lua,
    context: &SongLuaCompileContext,
) -> mlua::Result<SongLuaCompileContext> {
    let state = lua.app_data_ref::<SongLuaGameStateGlobals>()
        .ok_or_else(|| mlua::Error::runtime("missing gameplay selection state"))?;
    let mut selected = context.clone();
    for player in 0..LUA_PLAYERS {
        let steps = state.current_steps.raw_get::<Option<Table>>(player + 1)?;
        let index = steps.as_ref().and_then(|steps| state.step_objects.iter()
            .position(|(known, _)| known.to_pointer() == steps.to_pointer()));
        selected.player_step_indices[player] = index.filter(|&index| index < context.song_steps.len());
        if let Some(index) = index {
            selected.players[player].difficulty = state.step_objects[index].1.difficulty;
        }
        if let Some(index) = selected.player_step_indices[player]
            && let Some(timing) = selected.song_step_timing.get(index)
        {
            selected.player_timing[player] = Some(timing.clone());
        }
    }
    selected.style_name = current_song_lua_style_name(lua);
    Ok(selected)
}

pub(crate) fn replace_gameplay_screen(
    lua: &Lua,
    context: &SongLuaCompileContext,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
    create_named_child_actor: fn(&Lua, &Table, &str) -> mlua::Result<Table>,
) -> mlua::Result<()> {
    crate::song_tables::restart_stage_options(lua)?;
    let state = lua.app_data_ref::<SongLuaGameStateGlobals>()
        .ok_or_else(|| mlua::Error::runtime("missing gameplay selection state"))?.clone();
    // Native screen deletion unsubscribes its actors even when Lua kept a
    // reference. A fresh scene must not receive its predecessor's messages.
    lua.globals().set("__songlua_actor_registry", lua.create_table()?)?;
    initialize_gameplay_steps(lua, context)?;
    let context = selected_scene_context(lua, context)?;
    let top = create_top_screen_table(lua, &context, state.current_sort_order,
        state.current_song, create_dummy_actor, create_named_child_actor)?;
    install_screen_manager_globals(lua, top.top_screen, top.players)?;
    crate::tables::reset_stage_stats(lua, &context)?;
    // LoadNextSong calls ResetMusicStatistics before loading authored actors.
    // End callbacks still observe the outgoing clock; constructors start at 0.
    crate::set_compile_song_runtime_values(lua, 0.0, 0.0)?;
    crate::lua_util::install_native_gameplay_underlay(lua, None)?;
    Ok(())
}

pub struct SongLuaCompileGlobals {
    prefix_globals: Value,
    mods: Value,
    mod_time: Value,
    mods_ease: Value,
    mod_perframes: Value,
    mod_actions: Value,
}

pub fn install_compile_host(
    lua: &Lua,
    context: &SongLuaCompileContext,
    noteskin_resolver: SongLuaNoteskinResolver,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
    create_named_child_actor: fn(&Lua, &Table, &str) -> mlua::Result<Table>,
    install_actor_methods: fn(&Lua, &Table) -> mlua::Result<()>,
) -> mlua::Result<()> {
    install_default_stdlib_compat(lua, context.song_dir.as_path())?;
    let math: Table = lua.globals().get("math")?;
    math.get::<Function>("randomseed")?
        .call::<()>(context.random_seed as i32)?;
    install_compile_globals(
        lua,
        context,
        noteskin_resolver,
        create_dummy_actor,
        create_named_child_actor,
    )?;
    install_cmd_helpers(lua)?;
    install_def_globals(
        lua,
        song_lua_human_player_count(context),
        create_dummy_actor,
        install_actor_methods,
    )?;
    install_file_loader_globals(lua, context.song_dir.clone(), create_dummy_actor)?;
    crate::lua_util::install_native_gameplay_underlay(lua, Some(install_actor_methods))?;
    Ok(())
}

fn install_compile_globals(
    lua: &Lua,
    context: &SongLuaCompileContext,
    noteskin_resolver: SongLuaNoteskinResolver,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
    create_named_child_actor: fn(&Lua, &Table, &str) -> mlua::Result<Table>,
) -> mlua::Result<()> {
    let game_state_globals = install_core_globals(
        lua,
        context,
        song_lua_local_date_globals(),
        create_noteskin_table(lua, context, noteskin_resolver, create_dummy_actor)?,
        current_song_lua_style_name,
    )?;
    let current_sort_order = game_state_globals.current_sort_order.clone();
    let current_song = game_state_globals.current_song.clone();
    lua.set_app_data(game_state_globals);
    initialize_gameplay_steps(lua, context)?;
    let selected_context = selected_scene_context(lua, context)?;
    let context = &selected_context;

    // Native gameplay loads the theme's player actors after SL's profile
    // modifiers are initialized, including the hold-judgment graphic.
    lua.globals().set("SL", create_sl_table(lua, context)?)?;

    let top_screen = create_top_screen_table(
        lua,
        context,
        current_sort_order,
        current_song,
        create_dummy_actor,
        create_named_child_actor,
    )?;
    install_screen_manager_globals(lua, top_screen.top_screen.clone(), top_screen.players)?;
    install_sound_globals(lua, context.song_dir.as_path())?;
    install_message_manager_globals(lua, broadcast_song_lua_message)?;
    install_late_globals(lua, context)?;
    Ok(())
}

pub fn clone_lua_value(lua: &Lua, value: Value) -> mlua::Result<Value> {
    match value {
        Value::Table(table) => {
            let cloned = lua.create_table()?;
            // Keep the traversal key on Lua's stack instead of cloning a Rust
            // cursor handle for every reference-valued key at every depth.
            table.for_each::<Value, Value>(|key, value| {
                cloned.set(clone_lua_value(lua, key)?, clone_lua_value(lua, value)?)
            })?;
            Ok(Value::Table(cloned))
        }
        other => Ok(other),
    }
}

pub fn snapshot_compile_globals(lua: &Lua, globals: &Table) -> mlua::Result<SongLuaCompileGlobals> {
    Ok(SongLuaCompileGlobals {
        prefix_globals: clone_lua_value(lua, globals.get::<Value>("prefix_globals")?)?,
        mods: clone_lua_value(lua, globals.get::<Value>("mods")?)?,
        mod_time: clone_lua_value(lua, globals.get::<Value>("mod_time")?)?,
        mods_ease: clone_lua_value(lua, globals.get::<Value>("mods_ease")?)?,
        mod_perframes: clone_lua_value(lua, globals.get::<Value>("mod_perframes")?)?,
        mod_actions: clone_lua_value(lua, globals.get::<Value>("mod_actions")?)?,
    })
}

pub fn restore_compile_globals(
    globals: &Table,
    snapshot: SongLuaCompileGlobals,
) -> mlua::Result<()> {
    globals.set("prefix_globals", snapshot.prefix_globals)?;
    globals.set("mods", snapshot.mods)?;
    globals.set("mod_time", snapshot.mod_time)?;
    globals.set("mods_ease", snapshot.mods_ease)?;
    globals.set("mod_perframes", snapshot.mod_perframes)?;
    globals.set("mod_actions", snapshot.mod_actions)?;
    Ok(())
}

#[inline(always)]
#[must_use]
pub fn is_compile_global_name(name: &str) -> bool {
    matches!(
        name,
        "prefix_globals" | "mods" | "mod_time" | "mods_ease" | "mod_perframes" | "mod_actions"
    )
}

pub fn create_chunk_env_proxy(lua: &Lua, target: Table) -> mlua::Result<Table> {
    let proxy = lua.create_table()?;
    proxy.set("__songlua_env_target", target)?;
    let globals = lua.globals();
    let mt = lua.create_table()?;
    let proxy_for_index = proxy.clone();
    let globals_for_index = globals.clone();
    mt.set(
        "__index",
        lua.create_function(move |_, (_self, key): (Table, Value)| {
            let target: Table = proxy_for_index.get("__songlua_env_target")?;
            let value = target.get::<Value>(key.clone())?;
            if !matches!(value, Value::Nil) {
                return Ok(value);
            }
            globals_for_index.get::<Value>(key)
        })?,
    )?;
    // Retain the six interned keys: identity classifies them without text copies.
    let compile_keys = [
        lua.create_string("prefix_globals")?,
        lua.create_string("mods")?,
        lua.create_string("mod_time")?,
        lua.create_string("mods_ease")?,
        lua.create_string("mod_perframes")?,
        lua.create_string("mod_actions")?,
    ]
    .map(|key| {
        let identity = key.to_pointer() as usize;
        (key, identity)
    });
    let proxy_for_newindex = proxy.clone();
    let globals_for_newindex = globals;
    mt.set(
        "__newindex",
        lua.create_function(move |_, (_self, key, value): (Table, Value, Value)| {
            let target: Table = proxy_for_newindex.get("__songlua_env_target")?;
            target.set(&key, &value)?;
            if let Value::String(name) = &key {
                let identity = name.to_pointer() as usize;
                if compile_keys.iter().any(|(_, key)| *key == identity) {
                    globals_for_newindex.set(&key, &value)?;
                }
            }
            Ok(())
        })?,
    )?;
    let _ = proxy.set_metatable(Some(mt));
    Ok(proxy)
}

pub fn initial_chunk_environment(lua: &Lua, path: &Path) -> mlua::Result<Table> {
    if path.file_name().and_then(|name| name.to_str()) == Some("std.lua")
        && path
            .parent()
            .and_then(|dir| dir.file_name())
            .and_then(|name| name.to_str())
            == Some("template")
        && let Some(xero) = lua.globals().get::<Option<Table>>("xero")?
    {
        return Ok(xero);
    }
    Ok(lua.globals())
}

pub fn install_basic_globals(
    lua: &Lua,
    context: &SongLuaCompileContext,
    holiday_cheer: bool,
) -> mlua::Result<()> {
    let globals = lua.globals();
    let screen_width = context.screen_width.max(1.0);
    let screen_height = context.screen_height.max(1.0);
    let screen_center_x = 0.5 * screen_width;
    let screen_center_y = 0.5 * screen_height;
    globals.set("PLAYER_1", player_number_name(0))?;
    globals.set("PLAYER_2", player_number_name(1))?;
    globals.set(
        "PlayerNumberToString",
        lua.create_function(|_, player: Value| {
            Ok(player_index_from_value(&player)
                .map(|player| format!("P{}", player + 1))
                .unwrap_or_default())
        })?,
    )?;
    globals.set("PlayerNumber", create_player_number_table(lua)?)?;
    globals.set("OtherPlayer", create_other_player_table(lua)?)?;
    globals.set("Difficulty", create_difficulty_table(lua)?)?;
    globals.set(
        "CullMode",
        create_string_enum_table(lua, &["CullMode_Back", "CullMode_Front", "CullMode_None"])?,
    )?;
    let timers = create_string_enum_table(lua, &crate::player_options::MOD_TIMER_NAMES)?;
    let reverse = timers.get::<Function>("Reverse")?.call::<Table>(())?;
    for (mode, label) in ["game", "beat", "song", "default"].into_iter().enumerate() {
        reverse.raw_set(label, mode)?;
        reverse.raw_set(mode, mode)?;
    }
    crate::set_string_method(lua, &timers, "GetName", "ModTimerType")?;
    globals.set("ModTimerType", timers)?;
    globals.set(
        "EditState",
        create_string_enum_table(
            lua,
            &[
                "EditState_Edit",
                "EditState_Record",
                "EditState_RecordPaused",
                "EditState_Playing",
            ],
        )?,
    )?;
    globals.set(
        "ProfileSlot",
        create_string_enum_table(
            lua,
            &[
                "ProfileSlot_Player1",
                "ProfileSlot_Player2",
                "ProfileSlot_Machine",
            ],
        )?,
    )?;
    globals.set(
        "GameController",
        create_string_enum_table(lua, &["GameController_1", "GameController_2"])?,
    )?;
    globals.set(
        "PlayerController",
        create_string_enum_table(
            lua,
            &[
                "PlayerController_Human",
                "PlayerController_Autoplay",
                "PlayerController_Cpu",
            ],
        )?,
    )?;
    globals.set(
        "HealthState",
        create_string_enum_table(
            lua,
            &[
                "HealthState_Hot",
                "HealthState_Alive",
                "HealthState_Danger",
                "HealthState_Dead",
            ],
        )?,
    )?;
    globals.set(
        "SortOrder",
        create_string_enum_table(
            lua,
            &[
                "SortOrder_Group",
                "SortOrder_Title",
                "SortOrder_Artist",
                "SortOrder_BPM",
                "SortOrder_Popularity",
                "SortOrder_Preferred",
                "SortOrder_Recent",
            ],
        )?,
    )?;
    globals.set(
        "TapNoteScore",
        create_string_enum_table(
            lua,
            &[
                "TapNoteScore_W1",
                "TapNoteScore_W2",
                "TapNoteScore_W3",
                "TapNoteScore_W4",
                "TapNoteScore_W5",
                "TapNoteScore_Miss",
                "TapNoteScore_HitMine",
                "TapNoteScore_AvoidMine",
                "TapNoteScore_CheckpointMiss",
                "TapNoteScore_CheckpointHit",
                "TapNoteScore_None",
            ],
        )?,
    )?;
    globals.set(
        "TapNoteType",
        create_string_enum_table(
            lua,
            &[
                "TapNoteType_Empty",
                "TapNoteType_Tap",
                "TapNoteType_HoldHead",
                "TapNoteType_HoldTail",
                "TapNoteType_Mine",
                "TapNoteType_Lift",
                "TapNoteType_Attack",
                "TapNoteType_AutoKeysound",
                "TapNoteType_Fake",
            ],
        )?,
    )?;
    globals.set(
        "TapNoteSource",
        create_string_enum_table(
            lua,
            &[
                "TapNoteSource_Original",
                "TapNoteSource_Addition",
                "TapNoteSource_Mine",
            ],
        )?,
    )?;
    globals.set(
        "TapNoteSubType",
        create_string_enum_table(
            lua,
            &[
                "TapNoteSubType_Invalid",
                "TapNoteSubType_Hold",
                "TapNoteSubType_Roll",
            ],
        )?,
    )?;
    globals.set(
        "HoldNoteScore",
        create_string_enum_table(
            lua,
            &[
                "HoldNoteScore_Held",
                "HoldNoteScore_LetGo",
                "HoldNoteScore_MissedHold",
                "HoldNoteScore_None",
            ],
        )?,
    )?;
    globals.set(
        "StepsType",
        create_string_enum_table(lua, &["StepsType_Dance_Single", "StepsType_Dance_Double"])?,
    )?;
    globals.set(
        "StyleType",
        create_string_enum_table(
            lua,
            &[
                "StyleType_OnePlayerOneSide",
                "StyleType_OnePlayerTwoSides",
                "StyleType_TwoPlayersTwoSides",
            ],
        )?,
    )?;
    globals.set("SCREEN_WIDTH", screen_width.round() as i32)?;
    globals.set("SCREEN_HEIGHT", screen_height.round() as i32)?;
    globals.set("SCREEN_CENTER_X", screen_center_x)?;
    globals.set("SCREEN_CENTER_Y", screen_center_y)?;
    globals.set("scx", screen_center_x)?;
    globals.set("scy", screen_center_y)?;
    globals.set("SCREEN_LEFT", 0)?;
    globals.set("SCREEN_TOP", 0)?;
    globals.set("SCREEN_RIGHT", screen_width.round() as i32)?;
    globals.set("SCREEN_BOTTOM", screen_height.round() as i32)?;
    globals.set(
        "_screen",
        create_screen_table(
            lua,
            screen_width,
            screen_height,
            screen_center_x,
            screen_center_y,
        )?,
    )?;
    globals.set(
        "ProductFamily",
        lua.create_function(|_, _args: MultiValue| Ok(SONG_LUA_PRODUCT_FAMILY))?,
    )?;
    globals.set(
        "ProductID",
        lua.create_function(|_, _args: MultiValue| Ok(SONG_LUA_PRODUCT_ID))?,
    )?;
    globals.set(
        "ProductVersion",
        lua.create_function(|_, _args: MultiValue| Ok(SONG_LUA_PRODUCT_VERSION))?,
    )?;
    globals.set(
        "ToEnumShortString",
        lua.create_function(|lua, value: String| {
            let short = value
                .split_once('_')
                .map(|(_, short)| short)
                .unwrap_or(value.as_str());
            Ok(Value::String(lua.create_string(short)?))
        })?,
    )?;
    globals.set("pname", globals.get::<Function>("ToEnumShortString")?)?;
    globals.set(
        "FormatPercentScore",
        lua.create_function(format_percent_score)?,
    )?;
    globals.set(
        "clamp",
        lua.create_function(|_, (value, min, max): (f64, f64, f64)| Ok(value.clamp(min, max)))?,
    )?;
    globals.set(
        "GetScreenAspectRatio",
        lua.create_function(move |_, _args: MultiValue| Ok(screen_width / screen_height.max(1.0)))?,
    )?;
    globals.set(
        "WideScale",
        lua.create_function(move |_, (ar4_3, ar16_9): (f32, f32)| {
            Ok(scale_value(screen_width, 640.0, 854.0, ar4_3, ar16_9))
        })?,
    )?;
    globals.set(
        "SL_WideScale",
        lua.create_function(move |_, (ar4_3, ar16_9): (f32, f32)| {
            Ok(scale_value(screen_width, 640.0, 854.0, ar4_3, ar16_9))
        })?,
    )?;
    let is_widescreen = screen_width / screen_height.max(1.0) > 1.5;
    globals.set(
        "IsUsingWideScreen",
        lua.create_function(move |_, _args: MultiValue| Ok(is_widescreen))?,
    )?;
    globals.set(
        "DarkUI",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    globals.set(
        "IsServiceAllowed",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    globals.set(
        "round",
        lua.create_function(|_, args: MultiValue| {
            let value = args.front().cloned().and_then(read_f32).unwrap_or(0.0);
            let precision = args.get(1).cloned().and_then(read_f32).unwrap_or(0.0);
            let factor = 10.0_f32.powf(precision.max(0.0));
            Ok((value * factor).round() / factor)
        })?,
    )?;
    globals.set(
        "FindInTable",
        lua.create_function(|_, (needle, haystack): (Value, Value)| {
            let Value::Table(haystack) = haystack else {
                return Ok(Value::Nil);
            };
            for pair in haystack.pairs::<Value, Value>() {
                let (key, value) = pair?;
                if lua_values_equal(&needle, &value) {
                    return Ok(key);
                }
            }
            Ok(Value::Nil)
        })?,
    )?;
    globals.set(
        "DeepCopy",
        lua.create_function(|lua, args: MultiValue| {
            clone_lua_value(lua, args.front().cloned().unwrap_or(Value::Nil))
        })?,
    )?;
    globals.set(
        "SecondsToHHMMSS",
        lua.create_function(|lua, seconds: f64| {
            Ok(Value::String(
                lua.create_string(seconds_to_hhmmss(seconds))?,
            ))
        })?,
    )?;
    globals.set(
        "SecondsToMSS",
        lua.create_function(|lua, seconds: f64| {
            Ok(Value::String(lua.create_string(seconds_to_mss(seconds))?))
        })?,
    )?;
    globals.set(
        "SecondsToMMSS",
        lua.create_function(|lua, seconds: f64| {
            Ok(Value::String(lua.create_string(seconds_to_mmss(seconds))?))
        })?,
    )?;
    globals.set(
        "SecondsToMSSMsMs",
        lua.create_function(|lua, seconds: f64| {
            Ok(Value::String(
                lua.create_string(seconds_to_mss_ms_ms(seconds))?,
            ))
        })?,
    )?;
    globals.set(
        "SecondsToMMSSMsMs",
        lua.create_function(|lua, seconds: f64| {
            Ok(Value::String(
                lua.create_string(seconds_to_mmss_ms_ms(seconds))?,
            ))
        })?,
    )?;
    globals.set(
        "FormatNumberAndSuffix",
        lua.create_function(|lua, value: i64| {
            Ok(Value::String(
                lua.create_string(format_number_and_suffix(value))?,
            ))
        })?,
    )?;
    globals.set(
        "GetTimeSinceStart",
        lua.create_function(|lua, _args: MultiValue| {
            if crate::live::active(lua) {
                let runtime = lua.globals().get::<Table>(SONG_LUA_RUNTIME_KEY)?;
                // RageTimer exposes a double process clock, independent of
                // the float SongPosition clock and paused music playback.
                return Ok(Value::Number(runtime.get::<Option<f64>>("__songlua_uptime_seconds")?
                    .unwrap_or(0.0)));
            }
            let seconds = lua
                .globals()
                .get::<Option<Table>>(SONG_LUA_RUNTIME_KEY)?
                .and_then(|runtime| {
                    runtime
                        .get::<Option<f32>>(SONG_LUA_RUNTIME_SECONDS_KEY)
                        .ok()
                })
                .flatten()
                .unwrap_or(0.0);
            song_lua_runtime_number(seconds)
        })?,
    )?;
    globals.set(
        "HolidayCheer",
        lua.create_function(move |_, _args: MultiValue| Ok(holiday_cheer))?,
    )?;
    globals.set(
        "ASPECT_SCALE_FACTOR",
        screen_width / (640.0 * (screen_height / 480.0)),
    )?;
    globals.set(
        "scale",
        lua.create_function(
            |_, (value, from_low, from_high, to_low, to_high): (f32, f32, f32, f32, f32)| {
                Ok(scale_value(value, from_low, from_high, to_low, to_high))
            },
        )?,
    )?;
    globals.set(
        "__songlua_song_dir",
        song_dir_string(context.song_dir.as_path()),
    )?;
    globals.set("__songlua_script_dir", Value::Nil)?;
    globals.set("__songlua_current_script_path", Value::Nil)?;

    let player_options = lua.create_table()?;
    for capability in SONG_LUA_PLAYER_OPTION_CAPABILITIES {
        player_options.set(*capability, true)?;
    }
    for prefix in SONG_LUA_PLAYER_OPTION_MULTICOL_PREFIXES {
        for column in 1..=16 {
            player_options.set(format!("{prefix}{column}"), true)?;
        }
    }
    player_options.set("ConfusionOffset", context.confusion_offset_available)?;
    player_options.set("Confusion", context.confusion_available)?;
    player_options.set("AMod", context.amod_available)?;
    globals.set("PlayerOptions", player_options)?;
    Ok(())
}

pub struct SongLuaDateGlobals {
    pub year: i32,
    pub month_of_year: i32,
    pub day_of_month: i32,
    pub hour: i32,
    pub minute: i32,
    pub second: i32,
}

#[must_use]
pub fn song_lua_local_date_globals() -> SongLuaDateGlobals {
    let now = Local::now();
    SongLuaDateGlobals {
        year: now.year(),
        month_of_year: now.month0() as i32,
        day_of_month: now.day() as i32,
        hour: now.hour() as i32,
        minute: now.minute() as i32,
        second: now.second() as i32,
    }
}

pub fn install_date_globals(lua: &Lua, date: SongLuaDateGlobals) -> mlua::Result<()> {
    let globals = lua.globals();
    let SongLuaDateGlobals {
        year,
        month_of_year,
        day_of_month,
        hour,
        minute,
        second,
    } = date;
    globals.set(
        "Year",
        lua.create_function(move |_, _args: MultiValue| Ok(year))?,
    )?;
    globals.set(
        "MonthOfYear",
        lua.create_function(move |_, _args: MultiValue| Ok(month_of_year))?,
    )?;
    globals.set(
        "DayOfMonth",
        lua.create_function(move |_, _args: MultiValue| Ok(day_of_month))?,
    )?;
    // DateTime.cpp exposes tm_yday: January 1 is zero, including leap years.
    let day_of_year = month_of_year.checked_add(1)
        .and_then(|month| chrono::NaiveDate::from_ymd_opt(year, month as u32, day_of_month as u32))
        .map(|date| date.ordinal0() as i32);
    globals.set(
        "DayOfYear",
        lua.create_function(move |_, _args: MultiValue| {
            day_of_year.ok_or_else(|| mlua::Error::RuntimeError("invalid calendar date for DayOfYear".into()))
        })?,
    )?;
    globals.set(
        "Hour",
        lua.create_function(move |_, _args: MultiValue| Ok(hour))?,
    )?;
    globals.set(
        "Minute",
        lua.create_function(move |_, _args: MultiValue| Ok(minute))?,
    )?;
    globals.set(
        "Second",
        lua.create_function(move |_, _args: MultiValue| Ok(second))?,
    )?;
    Ok(())
}

pub fn install_manager_globals(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<()> {
    let globals = lua.globals();
    let global_offset_seconds = context.global_offset_seconds;
    let display_width = context.display_width.max(0.0).round() as i32;
    let display_height = context.display_height.max(0.0).round() as i32;
    globals.set(
        "PREFSMAN",
        create_prefsmgr_table(
            lua,
            global_offset_seconds,
            context.display_aspect_ratio,
            display_width,
            display_height,
            context.video_renderers.as_str(),
            context.last_seen_video_driver.as_str(),
            context.event_mode,
        )?,
    )?;
    globals.set("DISPLAY", create_display_table(lua, context)?)?;
    globals.set("THEME", create_theme_table(lua, context)?)?;
    globals.set("GAMEMAN", create_gameman_table(lua)?)?;
    globals.set("CHARMAN", create_charman_table(lua)?)?;
    globals.set("MEMCARDMAN", create_memcardman_table(lua)?)?;
    globals.set("UNLOCKMAN", create_unlockman_table(lua)?)?;
    globals.set("HOOKS", create_hooks_table(lua)?)?;
    Ok(())
}

pub fn install_screen_utility_globals(lua: &Lua) -> mlua::Result<()> {
    // _fallback/Scripts/02 Actor.lua: background fit helpers used by song AFTs.
    lua.load(
        r#"
        bg_fit_functions = {
            BackgroundFitMode_CoverDistort = function(self, width, height)
                self:zoomx(width / self:GetWidth())
                self:zoomy(height / self:GetHeight())
            end,
            BackgroundFitMode_CoverPreserve = function(self, width, height)
                self:zoom(math.max(width / self:GetWidth(), height / self:GetHeight()))
            end,
            BackgroundFitMode_FitInside = function(self, width, height)
                self:zoom(math.min(width / self:GetWidth(), height / self:GetHeight()))
            end,
            BackgroundFitMode_FitInsideAvoidLetter = function(self, width, height)
                self:zoom(height / self:GetHeight())
            end,
            BackgroundFitMode_FitInsideAvoidPillar = function(self, width, height)
                self:zoom(width / self:GetWidth())
            end,
        }
        -- _fallback/Scripts/02 Utilities.lua: use the current style type and
        -- preference rather than the number of enabled players.
        function Center1Player()
            local styleType = GAMESTATE:GetCurrentStyle():GetStyleType()
            if styleType == "StyleType_OnePlayerTwoSides"
                or styleType == "StyleType_TwoPlayersSharedSides" then
                return true
            elseif PREFSMAN:GetPreference("Center1Player") then
                return styleType == "StyleType_OnePlayerOneSide"
            else
                return false
            end
        end
    "#,
    )
    .set_name("background fit helpers")
    .exec()?;
    let globals = lua.globals();
    globals.set("SongUtil", create_song_util_table(lua)?)?;
    globals.set(
        "ScreenSystemLayerHelpers",
        create_screen_system_layer_helpers_table(lua)?,
    )?;
    Ok(())
}

pub fn install_screen_string_globals(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.set(
        "ScreenString",
        lua.create_function(|lua, args: MultiValue| {
            let name = method_arg(&args, 0)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            Ok(Value::String(lua.create_string(theme_string("", &name))?))
        })?,
    )?;
    globals.set(SONG_LUA_SIDE_EFFECT_COUNT_KEY, 0_i64)?;
    Ok(())
}

pub type SongLuaBroadcastCallback = fn(&Lua, &str, Option<Value>) -> mlua::Result<()>;

pub(crate) fn broadcast_queued_message(lua: &Lua, message: &str) -> mlua::Result<()> {
    // Message(string) creates an empty parameter table. Retain that table in
    // baked callback captures, just as for parameterized Lua broadcasts.
    record_song_lua_broadcast(lua, message, true)?;
    let broadcast = *lua.app_data_ref::<SongLuaBroadcastCallback>()
        .ok_or_else(|| mlua::Error::runtime("message manager is not installed"))?;
    broadcast(lua, message, Some(Value::Table(lua.create_table()?)))
}

pub fn install_screen_manager_globals(
    lua: &Lua,
    top_screen: Table,
    players: [Table; LUA_PLAYERS],
) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.set("__songlua_top_screen_player_1", players[0].clone())?;
    globals.set("__songlua_top_screen_player_2", players[1].clone())?;
    globals.set("__songlua_top_screen", top_screen.clone())?;
    let player_1_actor = players[0].clone();
    let player_2_actor = players[1].clone();
    globals.set(
        "GetPlayerAF",
        lua.create_function(move |_, args: MultiValue| {
            let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
                return Ok(Value::Nil);
            };
            Ok(Value::Table(match player {
                0 => player_1_actor.clone(),
                1 => player_2_actor.clone(),
                _ => return Ok(Value::Nil),
            }))
        })?,
    )?;

    // SCREENMAN is process-owned; Lua references to it survive screen changes.
    if matches!(globals.raw_get::<Value>("SCREENMAN")?, Value::Table(_)) {
        return Ok(());
    }

    let screenman = lua.create_table()?;
    screenman.set(
        "GetTopScreen",
        lua.create_function(move |lua, _args: MultiValue| {
            if lua.globals().raw_get::<Option<bool>>("__songlua_screen_loading")?.unwrap_or(false) {
                Ok(Value::Nil)
            } else {
                lua.globals().raw_get::<Value>("__songlua_top_screen")
            }
        })?,
    )?;
    screenman.set(
        "SystemMessage",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    screenman.set(
        "SetNewScreen",
        lua.create_function(|lua, args: MultiValue| {
            if let Some(_name) = method_arg(&args, 0).cloned().and_then(read_string) {
                #[cfg(feature = "test-support")]
                record_state_call(lua, "ScreenManager.SetNewScreen", vec![serde_json::json!(_name)])?;
                crate::live::capture_source_request(lua, crate::live::SongLuaLiveRequest::NewScreen(_name.clone()))?;
                lua.set_app_data(SongLuaScreenRequest(_name));
                // Native ScreenManager queues the request; the current
                // screen keeps its identity through this command/update.
            }
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    screenman.set(
        "AddNewScreenToTop",
        lua.create_function(move |lua, args: MultiValue| {
            if let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) {
                let top_screen = lua.globals().get::<Table>("__songlua_top_screen")?;
                top_screen.set("Name", name)?;
            }
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    screenman.set(
        "set_input_redirected",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    globals.set("SCREENMAN", screenman)
}

pub fn install_message_manager_globals(
    lua: &Lua,
    broadcast: SongLuaBroadcastCallback,
) -> mlua::Result<()> {
    lua.set_app_data(broadcast);
    let globals = lua.globals();
    let messageman = lua.create_table()?;
    let messageman_for_broadcast = messageman.clone();
    messageman.set(
        "Broadcast",
        lua.create_function(move |lua, args: MultiValue| {
            if let Some(message) = method_arg(&args, 0).cloned().and_then(read_string) {
                let params = method_arg(&args, 1).cloned();
                // LunaMessageManager::Broadcast rejects these before dispatch.
                if let Some(value) = &params
                    && !matches!(value, Value::Table(_) | Value::Nil)
                {
                    return Err(mlua::Error::RuntimeError(format!(
                        "bad argument #2 to 'Broadcast' (table or nil expected, got {})",
                        value.type_name()
                    )));
                }
                note_song_lua_side_effect(lua)?;
                let has_params = matches!(params, Some(Value::Table(_)));
                record_song_lua_broadcast(lua, &message, has_params)?;
                crate::lua_util::capture_source_broadcast(lua, &message, params.as_ref());
                // Parameterized listeners need their actual values while an
                // action is baked; a named event cannot retain those values.
                if has_params
                    || !lua
                        .globals()
                        .raw_get::<Option<bool>>(SONG_LUA_SUPPRESS_BROADCAST_KEY)?
                        .unwrap_or(false)
                {
                    broadcast(lua, &message, params)?;
                }
                #[cfg(feature = "test-support")]
                record_state_call(lua, "MessageManager.Broadcast", vec![serde_json::json!(message)])?;
            }
            Ok(messageman_for_broadcast.clone())
        })?,
    )?;
    let messageman_for_logging = messageman.clone();
    messageman.set(
        "SetLogging",
        lua.create_function(move |lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(messageman_for_logging.clone())
        })?,
    )?;
    globals.set("MESSAGEMAN", messageman)
}

fn arrow_effects_player_options(args: &MultiValue) -> mlua::Result<Option<Table>> {
    let Some(Value::Table(player_state)) = args.front() else {
        return Ok(None);
    };
    let Some(method) = player_state.get::<Option<Function>>("GetCurrentPlayerOptions")? else {
        return Ok(None);
    };
    method.call::<Table>(player_state.clone()).map(Some)
}

fn arrow_effects_speedmod_value(options: &Table, name: &str) -> mlua::Result<Option<f32>> {
    let Some(method) = options.get::<Option<Function>>(name)? else {
        return Ok(None);
    };
    Ok(read_f32(method.call::<Value>(options.clone())?)
        .filter(|value| value.is_finite() && *value > 0.0))
}

fn arrow_effects_speed_multiplier(args: &MultiValue) -> mlua::Result<f32> {
    let Some(options) = arrow_effects_player_options(args)? else {
        return Ok(1.0);
    };
    if let Some(value) = arrow_effects_speedmod_value(&options, "XMod")? {
        return Ok(value);
    }
    let reference_bpm = options
        .get::<Option<f32>>("__songlua_reference_bpm")?
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(1.0);
    for name in ["CMod", "MMod", "AMod"] {
        if let Some(value) = arrow_effects_speedmod_value(&options, name)? {
            return Ok(value / reference_bpm);
        }
    }
    Ok(1.0)
}

fn arrow_effects_reverse_percent(args: &MultiValue) -> mlua::Result<f32> {
    let Some(options) = arrow_effects_player_options(args)? else {
        return Ok(0.0);
    };
    let Some(method) = options.get::<Option<Function>>("GetReversePercentForColumn")? else {
        return Ok(0.0);
    };
    let column = args
        .get(1)
        .cloned()
        .and_then(read_f32)
        .map(|value| value - 1.0)
        .unwrap_or(0.0);
    Ok(read_f32(method.call::<Value>((options, column))?).unwrap_or(0.0))
}

pub fn create_arrow_effects_table(
    lua: &Lua,
    context: &SongLuaCompileContext,
    current_style_name: fn(&Lua) -> String,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let timing = context.player_timing.clone();
    let rate = crate::song_music_rate(context);
    table.set(
        "GetYOffset",
        lua.create_function(move |lua, args: MultiValue| {
            let speed = arrow_effects_speed_multiplier(&args)?;
            let note = args.get(2).cloned().and_then(read_f32).unwrap_or(0.0);
            if timing.iter().all(Option::is_none) {
                return Ok(64.0 * note * speed);
            }
            let player = if let Some(Value::Table(state)) = args.front() {
                let method = state.get::<Function>("GetPlayerNumber")?;
                player_index_from_value(&method.call::<Value>(state.clone())?).unwrap_or(0)
            } else {
                0
            };
            let Some(timing) = timing.get(player).and_then(Option::as_ref) else {
                return Ok(64.0 * note * speed);
            };
            let runtime = lua.globals().get::<Table>(SONG_LUA_RUNTIME_KEY)?;
            let beat = runtime.get::<f32>(SONG_LUA_RUNTIME_BEAT_KEY)?;
            // Pauses hold the beat while music time and seconds-based speed
            // ramps keep advancing. Use the shared SongPosition music clock.
            let seconds = crate::runtime::song_music_time(
                lua, runtime.get(SONG_LUA_RUNTIME_SECONDS_KEY)?, rate,
            );
            if let Some(options) = arrow_effects_player_options(&args)? {
                if let Some(cmod) = arrow_effects_speedmod_value(&options, "CMod")? {
                    return Ok(
                        (timing.get_time_for_beat(note) - seconds) * cmod / 60.0 / rate * 64.0,
                    );
                }
                let speed = if arrow_effects_speedmod_value(&options, "XMod")?.is_some() {
                    speed
                } else {
                    speed / rate
                };
                return Ok(
                    (timing.get_displayed_beat(note) - timing.get_displayed_beat(beat))
                        * timing.get_speed_multiplier(beat, seconds)
                        * 64.0
                        * speed,
                );
            }
            Ok(
                (timing.get_displayed_beat(note) - timing.get_displayed_beat(beat))
                    * timing.get_speed_multiplier(beat, seconds)
                    * 64.0,
            )
        })?,
    )?;
    table.set(
        "GetYPos",
        lua.create_function(|_, args: MultiValue| {
            let y_offset = args.get(2).cloned().and_then(read_f32).unwrap_or(0.0_f32);
            let reverse = arrow_effects_reverse_percent(&args)?;
            // ArrowEffects is local to NoteField; Player places that actor at
            // the midpoint of the two theme receptor metrics.
            let reverse_offset = args
                .get(3)
                .cloned()
                .and_then(read_f32)
                .unwrap_or(THEME_RECEPTOR_Y_REV - THEME_RECEPTOR_Y_STD);
            let options = arrow_effects_player_options(&args)?;
            let option = |name| -> mlua::Result<f32> {
                let Some(options) = &options else {
                    return Ok(0.0);
                };
                let method = options.get::<Function>(name)?;
                Ok(read_f32(method.call::<Value>(options.clone())?).unwrap_or(0.0))
            };
            let zoom = 1.0 - option("Mini")? * 0.5;
            let zoom = if zoom.abs() < 0.01 { 0.01 } else { zoom };
            let half = reverse_offset / zoom / 2.0;
            let shift = reverse * (half + half) - half;
            let shift = option("Centered")? * -shift + shift;
            Ok(y_offset * (reverse * -2.0 + 1.0) + shift)
        })?,
    )?;
    lua.set_app_data(NativeArrowEffects {
        enabled: std::array::from_fn(|player| context.players[player].enabled),
        beat_x: [0.0; LUA_PLAYERS], bounds: [[[0.0; 2]; 10]; LUA_PLAYERS],
        invert: [[0.0; 10]; LUA_PLAYERS], style_name: current_style_name,
    });
    table.set("GetXPos", lua.create_function({
        let screen_height = context.screen_height.max(1.0);
        move |lua, args: MultiValue| arrow_effects_x_pos(lua, &args, screen_height, current_style_name)
    })?)?;
    table.set("Update", lua.create_function(|lua, _args: MultiValue| update_native_arrow_effects(lua))?)?;
    table.set(
        "GetZPos",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    table.set(
        "GetRotationX",
        lua.create_function(|_, args: MultiValue| {
            let column = args.get(2).cloned().and_then(read_f32).unwrap_or(1.0) as i32;
            Ok(arrow_effects_rotation_x(column))
        })?,
    )?;
    table.set(
        "GetRotationY",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    table.set(
        "GetRotationZ",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    table.set("GetZoom", lua.create_function(move |lua, args: MultiValue|
        arrow_effects_zoom(lua, &args, current_style_name))?)?;
    table.set(
        "GetAlpha",
        lua.create_function({
            let draw_distance = context.screen_height.max(1.0) * 1.5;
            move |lua, args: MultiValue| arrow_effects_alpha(lua, &args, draw_distance, current_style_name)
        })?,
    )?;
    table.set(
        "GetGlow",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    Ok(table)
}


#[derive(Clone, Copy)]
struct NativeArrowEffects {
    enabled: [bool; LUA_PLAYERS],
    beat_x: [f32; LUA_PLAYERS],
    bounds: [[[f32; 2]; 10]; LUA_PLAYERS],
    invert: [[f32; 10]; LUA_PLAYERS],
    style_name: fn(&Lua) -> String,
}

// ArrowEffects::Update runs after ScreenGameplay's actor updates. Lua reads
// the preceding update's Beat factor; the remaining X options are read now.
pub(crate) fn update_native_arrow_effects(lua: &Lua) -> mlua::Result<()> {
    let Some(previous) = lua.app_data_ref::<NativeArrowEffects>().map(|state| *state) else {
        return Ok(());
    };
    let runtime: Table = lua.globals().raw_get(SONG_LUA_RUNTIME_KEY)?;
    let positions = runtime.raw_get::<Option<Table>>(crate::runtime::PLAYER_SONG_POSITIONS_KEY)?;
    let options = lua.app_data_ref::<crate::song_tables::SongLuaOptionLevels>()
        .map(|levels| std::array::from_fn::<_, LUA_PLAYERS, _>(|player| levels.players[player][3].clone()));
    let mut next = previous;
    let style = crate::song_lua_style_info(&(previous.style_name)(lua));
    let two_sides = matches!(style.style_type, "StyleType_OnePlayerTwoSides" | "StyleType_TwoPlayersSharedSides");
    let per_side = style.columns / if two_sides { 2 } else { 1 };
    let scale = |x: f32, low: f32, high: f32, to_low: f32, to_high: f32| (x - low) * (to_high - to_low) / (high - low) + to_low;
    for player in 0..LUA_PLAYERS {
        if !previous.enabled[player] { continue; }
        let position = positions.as_ref().map(|positions| positions.raw_get::<Table>(player + 1)).transpose()?;
        let beat = position.as_ref().unwrap_or(&runtime).raw_get::<f32>(SONG_LUA_RUNTIME_BEAT_KEY)?;
        let state = options.as_ref().map(|options| options[player]
            .raw_get::<Option<Table>>("__songlua_player_option_state")).transpose()?.flatten();
        let option = |name| -> mlua::Result<f32> {
            Ok(state.as_ref().map(|state| state.raw_get::<Option<f32>>(name)).transpose()?.flatten().unwrap_or(0.0))
        };
        let mut phase = (beat + 0.2 + option("beatoffset")?) * (option("beatmult")? + 1.0);
        let reverse = phase as i32 % 2 != 0;
        let mut factor = 0.0;
        if phase >= 0.0 {
            phase -= phase.trunc(); phase += 1.0; phase -= phase.trunc();
            if phase < 0.5 {
                factor = if phase < 0.2 {
                    let value = phase / 0.2; value * value
                } else {
                    let value = (phase - 0.2) * -1.0 / (0.5 - 0.2) + 1.0;
                    1.0 - (1.0 - value) * (1.0 - value)
                };
                if reverse { factor *= -1.0; }
                factor *= 20.0;
            }
        }
        next.beat_x[player] = factor;
        for column in 0..style.columns {
            let width = if style.columns > 4 { 2 } else { 3 };
            let range = &style.x_offsets[column.saturating_sub(width)..=(column + width).min(style.columns - 1)];
            next.bounds[player][column] = [range.iter().copied().fold(f32::MAX, f32::min),
                range.iter().copied().fold(f32::MIN_POSITIVE, f32::max)];
            let local = column % per_side;
            let (first, last) = if local <= (per_side - 1) / 2 { (0, (per_side - 1) / 2) }
                else if local >= (per_side + 1) / 2 { ((per_side + 1) / 2, per_side - 1) }
                else { (local / 2, local / 2) };
            let mirrored = if first == last { 0 } else { scale(local as f32, first as f32, last as f32, last as f32, first as f32) as usize };
            next.invert[player][column] = style.x_offsets[column / per_side * per_side + mirrored] - style.x_offsets[column];
        }
    }
    lua.set_app_data(next);
    Ok(())
}

fn arrow_effects_x_pos(
    lua: &Lua, args: &MultiValue, screen_height: f32,
    current_style_name: fn(&Lua) -> String,
) -> mlua::Result<f32> {
    use std::f32::consts::PI;
    let options = arrow_effects_player_options(args)?;
    let state = options.as_ref().map(|options| options.raw_get::<Option<Table>>("__songlua_player_option_state"))
        .transpose()?.flatten();
    let value = |name| -> mlua::Result<f32> {
        Ok(state.as_ref().map(|state| state.raw_get::<Option<f32>>(name)).transpose()?.flatten().unwrap_or(0.0))
    };
    let player = options.as_ref().and_then(|options| lua.app_data_ref::<crate::song_tables::SongLuaOptionLevels>()
        .and_then(|levels| levels.players.iter().position(|levels| levels[3].to_pointer() == options.to_pointer())))
        .unwrap_or(0);
    let number = |index, default| -> mlua::Result<f32> {
        match args.get(index).filter(|value| !value.is_nil()) {
            Some(value) => lua.coerce_number(value.clone())?.map(|value| value as f32)
                .ok_or_else(|| mlua::Error::runtime("expected numeric ArrowEffects argument")),
            None => Ok(default),
        }
    };
    let column = number(1, 1.0)? as i32 - 1;
    let y = number(2, 0.0)?;
    let style = crate::song_lua_style_info(&current_style_name(lua));
    let column = usize::try_from(column).ok().filter(|column| *column < style.columns)
        .ok_or_else(|| mlua::Error::runtime("ArrowEffects.GetXPos column out of range"))?;
    let base_x = style.x_offsets[column];
    let two_sides = matches!(style.style_type, "StyleType_OnePlayerTwoSides" | "StyleType_TwoPlayersSharedSides");
    // _fallback GameplayMargins reserves 272px per side, 544px when centered.
    // ScreenGameplay clamps the resulting style fit zoom to one.
    let field_zoom = arrow_effects_field_zoom(&style);
    let runtime: Table = lua.globals().raw_get(SONG_LUA_RUNTIME_KEY)?;
    let beat = runtime.raw_get::<f32>(SONG_LUA_RUNTIME_BEAT_KEY)?;
    let seconds = runtime.raw_get::<f32>(SONG_LUA_RUNTIME_SECONDS_KEY)?;
    let uptime = runtime.raw_get::<Option<f64>>("__songlua_uptime_seconds")?.unwrap_or(f64::from(seconds));
    let timer = state.as_ref().map(|state| state.raw_get::<Option<u8>>("modtimersetting")).transpose()?.flatten().unwrap_or(3);
    let clock = match timer { 1 => f64::from(beat), 2 => f64::from(seconds), _ => uptime };
    let time = ((clock + f64::from(value("modtimeroffset")?)) * (1.0 + f64::from(value("modtimermult")?))) as f32;
    let cosecant = state.as_ref().map(|state| state.raw_get::<Option<bool>>("cosecant")).transpose()?.flatten().unwrap_or(false);
    let tangent = |angle: f32| if cosecant { 1.0 / angle.sin() } else { angle.tan() };
    let scale = |x: f32, low: f32, high: f32, to_low: f32, to_high: f32| (x - low) * (to_high - to_low) / (high - low) + to_low;
    let mut x = 0.0;
    for (amount, offset, period, tan) in [("tornado", "tornadooffset", "tornadoperiod", false), ("tantornado", "tantornadooffset", "tantornadoperiod", true)] {
        let amount = value(amount)?;
        if amount == 0.0 { continue; }
        let [low, high] = lua.app_data_ref::<NativeArrowEffects>()
            .map_or([0.0, 0.0], |state| state.bounds[player][column]);
        let low = low * field_zoom;
        let high = high * field_zoom;
        let position = scale(base_x * field_zoom, low, high, -1.0, 1.0);
        let angle = position.acos() + (y + value(offset)?) * (value(period)? * 6.0 + 6.0) / screen_height;
        let wave = if tan { tangent(angle) } else { angle.cos() };
        x += (scale(wave, -1.0, 1.0, low, high) - base_x * field_zoom) * amount;
    }
    for (amount, offset, period, tan) in [("bumpyx", "bumpyxoffset", "bumpyxperiod", false), ("tanbumpyx", "tanbumpyxoffset", "tanbumpyxperiod", true)] {
        let amount = value(amount)?;
        if amount == 0.0 { continue; }
        let angle = (y + 100.0 * value(offset)?) / (value(period)? * 16.0 + 16.0);
        x += amount * 40.0 * if tan { tangent(angle) } else { angle.sin() };
    }
    for (amount, speed, offset, period, tan) in [("drunk", "drunkspeed", "drunkoffset", "drunkperiod", false), ("tandrunk", "tandrunkspeed", "tandrunkoffset", "tandrunkperiod", true)] {
        let amount = value(amount)?;
        if amount == 0.0 { continue; }
        let angle = time * (1.0 + value(speed)?) + column as f32 * (value(offset)? * 0.2 + 0.2)
            + y * (value(period)? * 10.0 + 10.0) / screen_height;
        let wave = if tan { tangent(angle) } else { angle.cos() };
        x += amount * (wave * 64.0 * 0.5);
    }
    if value("flip")? != 0.0 {
        x += (style.x_offsets[style.columns - 1 - column] * field_zoom - base_x * field_zoom) * value("flip")?;
    }
    if value("invert")? != 0.0 {
        let distance = lua.app_data_ref::<NativeArrowEffects>().map_or(0.0, |state| state.invert[player][column]);
        x += distance * value("invert")?;
    }
    if value("beat")? != 0.0 {
        let factor = lua.app_data_ref::<NativeArrowEffects>().map_or(0.0, |state| state.beat_x[player]);
        x += value("beat")? * (factor * (y / (value("beatperiod")? * 15.0 + 15.0) + PI / 2.0).sin());
    }
    if value("zigzag")? != 0.0 {
        let angle = PI * (1.0 / (value("zigzagperiod")? + 1.0)) * ((y + 100.0 * value("zigzagoffset")?) / 64.0);
        let mut phase = angle % (PI * 2.0);
        if phase < 0.0 { phase += PI * 2.0; }
        let phase = f64::from(phase * (1.0 / PI));
        let wave = if phase < 0.5 { phase * 2.0 } else if phase < 1.5 { 1.0 - (phase - 0.5) * 2.0 } else { -4.0 + phase * 2.0 } as f32;
        x += (value("zigzag")? * 64.0 / 2.0) * wave;
    }
    if value("sawtooth")? != 0.0 {
        let phase = (0.5 / (value("sawtoothperiod")? + 1.0) * y) / 64.0;
        x += (value("sawtooth")? * 64.0) * (phase - phase.floor());
    }
    if value("parabolax")? != 0.0 { x += value("parabolax")? * (y / 64.0) * (y / 64.0); }
    if value("attenuatex")? != 0.0 { x += value("attenuatex")? * (y / 64.0) * (y / 64.0) * (base_x / 64.0); }
    for (amount, offset, period, steps, tan) in [("digital", "digitaloffset", "digitalperiod", "digitalsteps", false), ("tandigital", "tandigitaloffset", "tandigitalperiod", "tandigitalsteps", true)] {
        let amount = value(amount)?;
        if amount == 0.0 { continue; }
        let angle = PI * (y + value(offset)?) / (64.0 + value(period)? * 64.0);
        let wave = if tan { tangent(angle) } else { angle.sin() };
        x += (amount * 64.0 * 0.5) * ((value(steps)? + 1.0) * wave).round() / (value(steps)? + 1.0);
    }
    if value("square")? != 0.0 {
        let angle = PI * (y + value("squareoffset")?) / (64.0 + value("squareperiod")? * 64.0);
        let mut phase = angle % (PI * 2.0);
        if phase < 0.01 { phase += PI * 2.0; }
        x += (value("square")? * 64.0 * 0.5) * if phase >= PI { -1.0 } else { 1.0 };
    }
    if value("bounce")? != 0.0 {
        x += value("bounce")? * 64.0 * 0.5 * ((y + value("bounceoffset")?) / (60.0 + value("bounceperiod")? * 60.0)).sin().abs();
    }
    if value("xmode")? != 0.0 {
        let negative = if two_sides { column >= style.columns / 2 } else { player == 1 };
        x += value("xmode")? * if negative { -y } else { y };
    }
    x += base_x * field_zoom;
    if value("tiny")? != 0.0 { x *= 0.5f32.powf(value("tiny")?).min(1.0); }
    Ok(x)
}

fn arrow_effects_field_zoom(style: &crate::SongLuaStyleInfo) -> f32 {
    let two_sides = matches!(style.style_type,
        "StyleType_OnePlayerTwoSides" | "StyleType_TwoPlayersSharedSides");
    ((if two_sides { 544.0 } else { 272.0 }) / style.width).min(1.0)
}

fn arrow_effects_zoom(
    lua: &Lua, args: &MultiValue, current_style_name: fn(&Lua) -> String,
) -> mlua::Result<f32> {
    let options = arrow_effects_player_options(args)?;
    let state = options.as_ref().map(|options|
        options.raw_get::<Option<Table>>("__songlua_player_option_state"))
        .transpose()?.flatten();
    let value = |name: &str| -> mlua::Result<f32> {
        Ok(state.as_ref().map(|state| state.raw_get::<Option<f32>>(name))
            .transpose()?.flatten().unwrap_or(0.0))
    };
    let number = |index, default| -> mlua::Result<f32> {
        match args.get(index).filter(|value| !value.is_nil()) {
            Some(value) => lua.coerce_number(value.clone())?.map(|value| value as f32)
                .ok_or_else(|| mlua::Error::runtime("expected numeric ArrowEffects argument")),
            None => Ok(default),
        }
    };
    let y = number(1, 0.0)?;
    let style = crate::song_lua_style_info(&current_style_name(lua));
    let column = number(2, 1.0)? as i32 - 1;
    let column = usize::try_from(column).ok().filter(|column| *column < style.columns)
        .ok_or_else(|| mlua::Error::runtime("ArrowEffects.GetZoom column out of range"))?;
    let mut zoom = arrow_effects_field_zoom(&style);
    let inner = value("pulseinner")?;
    let outer = value("pulseouter")?;
    if inner != 0.0 || outer != 0.0 {
        let sine = ((y + 100.0 * value("pulseoffset")?)
            / (0.4 * (64.0 + value("pulseperiod")? * 64.0))).sin();
        let mut inner = inner * 0.5 + 1.0;
        if inner == 0.0 { inner = 0.01; }
        zoom *= sine * (outer * 0.5) + inner;
    }
    let shrink_mult = value("shrinkmult")?;
    if shrink_mult != 0.0 && y >= 0.0 {
        zoom *= 1.0 / (1.0 + y * (shrink_mult / 100.0));
    }
    let shrink_linear = value("shrinklinear")?;
    if shrink_linear != 0.0 && y >= 0.0 {
        zoom += y * (0.5 * shrink_linear / 64.0);
    }
    let tiny = value("tiny")?;
    if tiny != 0.0 { zoom *= 0.5f32.powf(tiny); }
    let tiny = value(&format!("tiny{}", column + 1))?;
    if tiny != 0.0 { zoom *= 0.5f32.powf(tiny); }
    Ok(zoom)
}

fn arrow_effects_alpha(
    lua: &Lua, args: &MultiValue, default_draw_distance: f32,
    current_style_name: fn(&Lua) -> String,
) -> mlua::Result<f32> {
    // ArrowEffects.cpp uses the owned Current options and the position without
    // reverse. Read option state directly so engine math creates no Lua queries.
    let options = arrow_effects_player_options(args)?;
    let state = options.as_ref().map(|options|
        options.raw_get::<Option<Table>>("__songlua_player_option_state"))
        .transpose()?.flatten();
    let value = |name: &str| -> mlua::Result<f32> {
        state.as_ref().map(|state| state.raw_get::<Option<f32>>(name))
            .transpose().map(|value| value.flatten().unwrap_or(0.0))
    };
    let flag = |name: &str| -> mlua::Result<bool> {
        state.as_ref().map(|state| state.raw_get::<Option<bool>>(name))
            .transpose().map(|value| value.flatten().unwrap_or(false))
    };
    let number = |index: usize, default: f32| -> mlua::Result<f32> {
        match args.get(index).filter(|value| !value.is_nil()) {
            Some(value) => lua.coerce_number(value.clone())?
                .map(|value| value as f32)
                .ok_or_else(|| mlua::Error::runtime("expected numeric ArrowEffects argument")),
            None => Ok(default),
        }
    };
    let column = number(1, 1.0)? as i32 - 1;
    let offset = number(2, 0.0)?;
    let fade_to_fail = number(3, -1.0)?;
    // Argument five controls reverse placement, which GetAlpha deliberately
    // disables in GetYPos. It still accepts the native optional argument.
    let _reverse_offset = number(4, THEME_RECEPTOR_Y_REV - THEME_RECEPTOR_Y_STD)?;
    let draw_distance = number(5, default_draw_distance)?;
    let fade_in = number(6, 0.0)?;
    let runtime = lua.globals().get::<Table>(SONG_LUA_RUNTIME_KEY)?;
    let beat = runtime.get::<f32>(SONG_LUA_RUNTIME_BEAT_KEY)?;
    let seconds = runtime.get::<f32>(SONG_LUA_RUNTIME_SECONDS_KEY)?;
    let uptime = runtime.get::<Option<f64>>("__songlua_uptime_seconds")?.unwrap_or(f64::from(seconds));
    let timer = state.as_ref().map(|state| state.raw_get::<Option<u8>>("modtimersetting"))
        .transpose()?.flatten().unwrap_or(3);
    let base_time = match timer { 1 => f64::from(beat), 2 => f64::from(seconds), _ => uptime };
    let time = ((base_time + f64::from(value("modtimeroffset")?))
        * (1.0 + f64::from(value("modtimermult")?))) as f32;
    let mut y = offset;
    let tipsy = value("tipsy")?;
    if tipsy != 0.0 {
        let phase = time * (value("tipsyspeed")? * 1.2 + 1.2)
            + column as f32 * (value("tipsyoffset")? * 1.8 + 1.8);
        y += tipsy * (phase.cos() * (64.0 * 0.4));
    }
    let tan_tipsy = value("tantipsy")?;
    if tan_tipsy != 0.0 {
        let phase = time * (value("tantipsyspeed")? * 1.2 + 1.2)
            + column as f32 * (value("tantipsyoffset")? * 1.8 + 1.8);
        let wave = if flag("cosecant")? { 1.0 / phase.sin() } else { phase.tan() };
        y += tan_tipsy * (wave * (64.0 * 0.4));
    }
    let attenuate = value("attenuatey")?;
    if attenuate != 0.0 {
        let x = song_lua_style_column_x(&current_style_name(lua), column.max(0) as usize);
        y += attenuate * (offset / 64.0) * (offset / 64.0) * (x / 64.0);
    }
    let beat_y = value("beaty")?;
    if beat_y != 0.0 {
        let mut pulse = (beat + 0.2 + value("beatyoffset")?) * (value("beatymult")? + 1.0);
        let odd = (pulse as i32) % 2 != 0;
        let mut factor = 0.0;
        if pulse >= 0.0 {
            pulse -= pulse.trunc(); pulse += 1.0; pulse -= pulse.trunc();
            if pulse < 0.5 {
                factor = if pulse < 0.2 {
                    let phase = pulse / 0.2; phase * phase
                } else {
                    let phase = 1.0 - (pulse - 0.2) / (0.5 - 0.2);
                    1.0 - (1.0 - phase) * (1.0 - phase)
                };
                if odd { factor = -factor; }
                factor *= 20.0;
            }
        }
        y += beat_y * (factor * (offset / (value("beatyperiod")? * 15.0 + 15.0)
            + std::f32::consts::PI / 2.0).sin());
    }
    let center = 160.0 / (1.0 - value("mini")? * 0.5);
    let appearance_y = if flag("stealthtype")? { offset } else { y };
    let mut visible = 1.0;
    if appearance_y >= 0.0 || flag("stealthpastreceptors")? {
        let hidden = value("hidden")?;
        let sudden = value("sudden")?;
        let combined = hidden * sudden;
        let scale = |x: f32, low: f32, high: f32, from: f32, to: f32| {
            (x - low) / (high - low) * (to - from) + from
        };
        let mut adjust = 0.0;
        if hidden != 0.0 {
            let start = center + 40.0 * (-0.25 * combined) + center * value("hiddenoffset")?;
            let end = center + 40.0 * (-1.0 - 0.25 * combined) + center * value("hiddenoffset")?;
            adjust += hidden * scale(appearance_y, start, end, 0.0, -1.0).clamp(-1.0, 0.0);
        }
        if sudden != 0.0 {
            let start = center + 40.0 * (1.0 + 0.25 * combined) + center * value("suddenoffset")?;
            let end = center + 40.0 * (0.25 * combined) + center * value("suddenoffset")?;
            adjust += sudden * scale(appearance_y, start, end, -1.0, 0.0).clamp(-1.0, 0.0);
        }
        adjust -= value("stealth")?;
        adjust -= value(&format!("stealth{}", column + 1))?;
        if value("blink")? != 0.0 {
            let wave = ((time * 10.0).sin() / 0.3333).round() * 0.3333;
            adjust += scale(wave, 0.0, 1.0, -1.0, 0.0);
        }
        let vanish = value("randomvanish")?;
        if vanish != 0.0 { adjust += scale((y - center).abs(), 80.0, 160.0, -1.0, 0.0) * vanish; }
        visible = (1.0 + adjust).clamp(0.0, 1.0);
    }
    if fade_to_fail != -1.0 { visible = 1.0 - fade_to_fail; }
    let full_alpha_y = draw_distance * (1.0 - fade_in);
    if y > full_alpha_y {
        // Preserve the native raw float result: zero fade width divides by
        // zero, and positions beyond the far edge produce negative alpha.
        return Ok((y - full_alpha_y) / (draw_distance - full_alpha_y) * -1.0 + 1.0);
    }
    Ok(if visible > 0.5 { 1.0 } else { 0.0 })
}

pub(crate) const fn arrow_effects_rotation_x(column: i32) -> f32 {
    // ITGmania's Lua binding reads the third argument as a one-based column.
    // Legacy multitap passes (ps, offset, 0, lane), so its column is zero.
    // PlayerOptions places m_SpeedfMovesZ[15] (default 1) immediately before
    // m_fConfusionX. Safely reproduce that legacy alias, not an out-of-bounds
    // read. ReceptorGetRotationX converts this one radian to degrees.
    if column == 0 {
        180.0 / std::f32::consts::PI
    } else {
        0.0
    }
}

fn song_lua_sound_paths_table(lua: &Lua) -> mlua::Result<Table> {
    let globals = lua.globals();
    if let Some(paths) = globals.get::<Option<Table>>(SONG_LUA_SOUND_PATHS_KEY)? {
        return Ok(paths);
    }
    let paths = lua.create_table()?;
    globals.set(SONG_LUA_SOUND_PATHS_KEY, paths.clone())?;
    Ok(paths)
}

fn record_song_lua_sound_path(
    lua: &Lua,
    song_dir: &Path,
    args: &MultiValue,
    capture_call: bool,
) -> mlua::Result<()> {
    let Some(raw_path) = method_arg(args, 0).cloned().and_then(read_string) else {
        return Ok(());
    };
    let Ok(path) = resolve_script_path(lua, song_dir, &raw_path) else {
        return Ok(());
    };
    if !path.is_file() || !is_song_lua_audio_path(path.as_path()) {
        return Ok(());
    }

    let key = path.to_string_lossy().into_owned();
    if capture_call {
        crate::lua_util::capture_runtime_sound(lua, path.clone());
        if let Some(calls) = lua
            .globals()
            .get::<Option<Table>>(SONG_LUA_SOUND_CALLS_KEY)?
        {
            calls.raw_set(calls.raw_len() + 1, key.clone())?;
        }
    }
    let paths = song_lua_sound_paths_table(lua)?;
    for existing in paths.sequence_values::<String>() {
        if existing? == key {
            return Ok(());
        }
    }
    paths.raw_set(paths.raw_len() + 1, key)?;
    Ok(())
}

pub fn install_sound_globals(lua: &Lua, song_dir: &Path) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.set(SONG_LUA_SOUND_PATHS_KEY, lua.create_table()?)?;
    globals.set(SONG_LUA_SOUND_CALLS_KEY, Value::Nil)?;
    let sound = lua.create_table()?;
    for name in ["DimMusic", "StopMusic"] {
        let sound_for_return = sound.clone();
        sound.set(
            name,
            lua.create_function(move |lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(sound_for_return.clone())
            })?,
        )?;
    }
    for (name, capture_call) in [("PlayMusicPart", false), ("PlayOnce", true)] {
        let sound_for_return = sound.clone();
        sound.set(
            name,
            lua.create_function({
                let song_dir = song_dir.to_path_buf();
                move |lua, args: MultiValue| {
                    note_song_lua_side_effect(lua)?;
                    record_song_lua_sound_path(lua, song_dir.as_path(), &args, capture_call)?;
                    Ok(sound_for_return.clone())
                }
            })?,
        )?;
    }
    let sound_for_return = sound.clone();
    sound.set(
        "PlayAnnouncer",
        lua.create_function(move |lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(sound_for_return.clone())
        })?,
    )?;
    sound.set(
        "GetPlayerBalance",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    sound.set(
        "IsTimingDelayed",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    globals.set("SOUND", sound)?;
    Ok(())
}

#[derive(Clone)]
pub struct SongLuaGameStateGlobals {
    pub current_sort_order: Table,
    pub current_song: Table,
    pub(super) current_steps: Table,
    step_objects: Arc<Vec<(Table, crate::SongLuaStep)>>,
}

pub fn install_core_globals(
    lua: &Lua,
    context: &SongLuaCompileContext,
    date: SongLuaDateGlobals,
    noteskin: Table,
    current_style_name: fn(&Lua) -> String,
) -> mlua::Result<SongLuaGameStateGlobals> {
    let holiday_cheer = date.month_of_year == 11;
    install_basic_globals(lua, context, holiday_cheer)?;
    install_date_globals(lua, date)?;
    install_manager_globals(lua, context)?;

    let globals = lua.globals();
    globals.set("ThemePrefs", create_theme_prefs_table(lua)?)?;
    globals.set("NOTESKIN", noteskin)?;
    install_screen_utility_globals(lua)?;
    globals.set(
        "ArrowEffects",
        create_arrow_effects_table(lua, context, current_style_name)?,
    )?;
    install_screen_string_globals(lua)?;
    install_game_state_globals(lua, context)
}

// Reference-only worker-VM audit, bounded to two million chronological calls.
// Shipping builds retain no recording state or allocation.
#[cfg(feature = "test-support")]
#[derive(Default)]
pub(crate) struct SongLuaStateCalls(pub Vec<crate::SongLuaStateCall>);

#[cfg(feature = "test-support")]
fn record_state_call(lua: &Lua, operation: &'static str, args: Vec<serde_json::Value>) -> mlua::Result<()> {
    if lua.app_data_ref::<SongLuaStateCalls>().is_none() { return Ok(()) }
    let runtime = lua.globals().get::<Table>(SONG_LUA_RUNTIME_KEY)?;
    let call = crate::SongLuaStateCall { operation, args,
        beat: runtime.get(SONG_LUA_RUNTIME_BEAT_KEY)?,
        second: runtime.get(SONG_LUA_RUNTIME_SECONDS_KEY)?,
    };
    if let Some(mut capture) = lua.app_data_mut::<SongLuaStateCalls>() {
        if capture.0.len() >= 2_000_000 {
            return Err(mlua::Error::runtime("game state audit exceeded two million calls"));
        }
        capture.0.push(call);
    }
    Ok(())
}

fn write_current_steps(lua: &Lua, selected: &Table, player: usize, steps: Option<Table>) -> mlua::Result<()> {
    selected.raw_set(player + 1, steps)?;
    let message = ["CurrentStepsP1Changed", "CurrentStepsP2Changed"][player];
    // BroadcastOnChange::Set broadcasts even if the pointer did not change.
    record_song_lua_broadcast(lua, message, false)?;
    if !lua.globals().raw_get::<Option<bool>>(SONG_LUA_SUPPRESS_BROADCAST_KEY)?.unwrap_or(false) {
        broadcast_song_lua_message(lua, message, None)?;
    }
    Ok(())
}

pub fn install_game_state_globals(
    lua: &Lua,
    context: &SongLuaCompileContext,
) -> mlua::Result<SongLuaGameStateGlobals> {
    let globals = lua.globals();
    let screen_height = context.screen_height.max(1.0);
    let screen_center_y = 0.5 * screen_height;

    let song_runtime = create_song_runtime_table(lua, context)?;
    globals.set(SONG_LUA_RUNTIME_KEY, song_runtime.clone())?;
    let song = create_song_table(lua, context)?;
    let players = create_player_tables(lua, context, &song_runtime, &song)?;
    // Selection compares immutable authored metadata, not Lua-overridden
    // getters or resolved audio paths. Keep each table's chart identity.
    let all_steps: Table = song.raw_get("__songlua_steps")?;
    let mut step_objects = Vec::with_capacity(context.song_steps.len() + LUA_PLAYERS);
    for (index, step) in context.song_steps.iter().enumerate() {
        step_objects.push((all_steps.raw_get::<Table>(index + 1)?, step.clone()));
    }
    for (index, table) in players.steps.iter().enumerate() {
        if step_objects.iter().any(|(known, _)| known.to_pointer() == table.to_pointer()) { continue; }
        step_objects.push((table.clone(), crate::SongLuaStep {
            steps_type: crate::song_lua_style_info(&context.style_name).steps_type.into(),
            difficulty: context.players[index].difficulty, description: String::new(),
            chart_name: String::new(), credit: String::new(),
            meter: context.players[index].difficulty.meter().max(0) as u32, music_file: String::new(),
            possible_grade_points: 0,
        }));
    }
    let step_objects = Arc::new(step_objects);
    for (key, options) in SONG_LUA_PLAYER_OPTIONS_KEYS
        .iter()
        .zip(players.player_options.iter())
    {
        globals.set(*key, options.clone())?;
    }
    let song_options = [
        create_song_options_table(lua, context.song_music_rate)?,
        create_song_options_table(lua, context.song_music_rate)?,
        create_song_options_table(lua, context.song_music_rate)?,
        create_song_options_table(lua, context.song_music_rate)?,
    ];
    lua.set_app_data(crate::song_tables::SongLuaOptionLevels {
        players: players.option_levels.clone(),
        songs: song_options.clone(),
        clock_us: 0,
    });
    let display_bpms = context.song_display_bpms;
    let default_music_rate = song_music_rate(context);
    globals.set(
        "GetDisplayBPMs",
        lua.create_function(move |lua, args: MultiValue| {
            let (bpms, _) = display_bpms_for_args(&args, display_bpms, default_music_rate)?;
            create_display_bpms_table(lua, bpms)
        })?,
    )?;
    globals.set(
        "StringifyDisplayBPMs",
        lua.create_function(move |lua, args: MultiValue| {
            let (bpms, rate) = display_bpms_for_args(&args, display_bpms, default_music_rate)?;
            Ok(Value::String(
                lua.create_string(display_bpms_text(bpms, rate))?,
            ))
        })?,
    )?;
    let total_song_seconds = context.music_length_seconds.max(0.0);
    globals.set(
        "totalLengthSongOrCourse",
        lua.create_function(move |_, _args: MultiValue| {
            Ok(total_song_seconds / default_music_rate.max(f32::EPSILON))
        })?,
    )?;
    globals.set(
        "currentTimeSongOrCourse",
        lua.create_function({
            let song_runtime = song_runtime.clone();
            move |_, _args: MultiValue| song_runtime.get::<f64>(SONG_LUA_RUNTIME_SECONDS_KEY)
        })?,
    )?;
    let current_sort_order = lua.create_table()?;
    current_sort_order.raw_set(1, "SortOrder_Group")?;
    let gamestate = lua.create_table()?;
    gamestate.set(
        "PlayerIsUsingModifier",
        lua.create_function({
            let options = [
                players.option_levels[0][3].clone(),
                players.option_levels[1][3].clone(),
            ];
            let song_options = song_options[3].clone();
            move |lua, args: MultiValue| {
                let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
                    return Ok(false);
                };
                let text = method_arg(&args, 1)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                crate::song_tables::player_uses_modifiers(
                    lua,
                    &options[player],
                    &song_options,
                    &text,
                )
            }
        })?,
    )?;
    let game_env = lua.create_table()?;
    globals.set(
        "getenv",
        lua.create_function({
            let env = game_env.clone();
            move |_, name: String| env.get::<Value>(name)
        })?,
    )?;
    globals.set(
        "setenv",
        lua.create_function({
            let env = game_env.clone();
            move |_, (name, value): (String, Value)| env.set(name, value)
        })?,
    )?;
    gamestate.set(
        "Env",
        lua.create_function(move |_, _args: MultiValue| Ok(game_env.clone()))?,
    )?;
    let enabled_players = create_enabled_players_table(lua, context.players.clone())?;
    let human_players = enabled_players.clone();
    let current_song = lua.create_table()?;
    current_song.raw_set(1, song.clone())?;
    gamestate.set(
        "GetCurrentSong",
        lua.create_function({
            let current_song = current_song.clone();
            move |_, _args: MultiValue| {
                Ok(current_song
                    .raw_get::<Option<Table>>(1)?
                    .map_or(Value::Nil, Value::Table))
            }
        })?,
    )?;
    let current_style = lua.create_table()?;
    current_style.raw_set(1, create_style_table(lua, &context.style_name)?)?;
    // Engine implementations read the shared style slot directly. Calling
    // the public Lua getter would manufacture a song query at their caller.
    gamestate.raw_set("__songlua_current_style", current_style.clone())?;
    gamestate.set(
        "GetCurrentStyle",
        lua.create_function({
            let current_style = current_style.clone();
            move |_, _args: MultiValue| {
                Ok(current_style
                    .raw_get::<Option<Table>>(1)?
                    .map_or(Value::Nil, Value::Table))
            }
        })?,
    )?;
    let players_enabled = context.players.clone();
    gamestate.set(
        "IsPlayerEnabled",
        lua.create_function(move |_, args: MultiValue| {
            let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
                return Ok(false);
            };
            Ok(players_enabled[player].enabled)
        })?,
    )?;
    let human_players_enabled = context.players.clone();
    gamestate.set(
        "IsHumanPlayer",
        lua.create_function(move |_, args: MultiValue| {
            let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
                return Ok(false);
            };
            Ok(human_players_enabled[player].enabled)
        })?,
    )?;
    let sides_joined = context.players.clone();
    gamestate.set(
        "IsSideJoined",
        lua.create_function(move |_, args: MultiValue| {
            let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
                return Ok(false);
            };
            Ok(sides_joined[player].enabled)
        })?,
    )?;
    let global_human_players_enabled = context.players.clone();
    globals.set(
        "IsHumanPlayer",
        lua.create_function(move |_, args: MultiValue| {
            let Some(player) = args.front().and_then(player_index_from_value) else {
                return Ok(false);
            };
            Ok(global_human_players_enabled[player].enabled)
        })?,
    )?;
    globals.set(
        "IsAutoplay",
        // SL-Helpers.lua compares the actual PlayerState controller, rather
        // than the preferred PlayerAutoPlay option (which can change in play).
        lua.load("return function(player) return GAMESTATE:GetPlayerState(player):GetPlayerController() == 'PlayerController_Autoplay' end")
            .set_name("@theme:/Scripts/SL-Helpers.lua").eval::<Function>()?,
    )?;
    globals.set(
        "IsW0Judgment",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    globals.set(
        "IsW010Judgment",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    globals.set(
        "GetDefaultFailType",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string("FailType_Immediate")?))
        })?,
    )?;
    globals.set(
        "GetComboThreshold",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string("TapNoteScore_W3")?))
        })?,
    )?;
    let notefield_players = context.players.clone();
    globals.set(
        "GetNotefieldX",
        lua.create_function(move |_, args: MultiValue| {
            let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
                return Ok(Value::Nil);
            };
            Ok(Value::Number(notefield_players[player].screen_x.into()))
        })?,
    )?;
    globals.set(
        "GetNotefieldWidth",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(crate::song_lua_style_info(&current_song_lua_style_name(lua)).width)
        })?,
    )?;
    globals.set(
        "GetGameplayLayout",
        lua.create_function(move |lua, args: MultiValue| {
            let reverse = method_arg(&args, 1).is_some_and(truthy);
            create_gameplay_layout(lua, screen_center_y, reverse)
        })?,
    )?;
    gamestate.set(
        "GetEnabledPlayers",
        lua.create_function(move |_, _args: MultiValue| Ok(enabled_players.clone()))?,
    )?;
    gamestate.set(
        "GetHumanPlayers",
        lua.create_function(move |_, _args: MultiValue| Ok(human_players.clone()))?,
    )?;
    let player_states = players.player_states.clone();
    gamestate.set(
        "GetPlayerState",
        lua.create_function(move |_, (_self, player): (Option<Value>, Option<Value>)| {
            let Some(player) = player.as_ref().and_then(player_index_from_value) else {
                return Ok(Value::Nil);
            };
            Ok(Value::Table(player_states[player].clone()))
        })?,
    )?;
    let global_player_states = players.player_states.clone();
    globals.set(
        "GetPlayerOptionsString",
        lua.create_function(move |_, args: MultiValue| {
            let Some(player) = args.front().and_then(player_index_from_value) else {
                return Ok(String::new());
            };
            let state = &global_player_states[player];
            state
                .get::<Function>("GetPlayerOptionsString")?
                .call::<String>((state.clone(), "ModsLevel_Preferred"))
        })?,
    )?;
    let current_steps = lua.create_table()?;
    for (player_index, steps) in players.steps.iter().enumerate() {
        current_steps.raw_set(player_index + 1, steps.clone())?;
    }
    let current_trail = lua.create_table()?;
    let primary_trail = create_trail_table(
        lua,
        song.clone(),
        players.steps[0].clone(),
        context.song_display_bpms,
    )?;
    for (player_index, steps) in players.steps.iter().enumerate() {
        let trail = if player_index == 0 {
            primary_trail.clone()
        } else {
            create_trail_table(lua, song.clone(), steps.clone(), context.song_display_bpms)?
        };
        current_trail.raw_set(player_index + 1, trail)?;
    }
    let course = create_course_table(lua, context, song.clone(), primary_trail)?;
    globals.set(
        "SONGMAN",
        create_songman_table(lua, song, players.steps[0].clone(), course.clone(), context)?,
    )?;
    gamestate.set(
        "GetCurrentSteps",
        lua.create_function({
            let current_steps = current_steps.clone();
            move |_, args: MultiValue| {
                let player = method_arg(&args, 0).and_then(player_index_from_value)
                    .ok_or_else(|| mlua::Error::runtime("Expected PlayerNumber"))?;
                Ok(current_steps
                    .raw_get::<Option<Table>>(player + 1)?
                    .map_or(Value::Nil, Value::Table))
            }
        })?,
    )?;
    globals.set(
        "GetSongAndSteps",
        lua.create_function({
            let current_song = current_song.clone();
            let current_steps = current_steps.clone();
            move |_, args: MultiValue| {
                let player = args.front().and_then(player_index_from_value).unwrap_or(0);
                let song = current_song
                    .raw_get::<Option<Table>>(1)?
                    .map_or(Value::Nil, Value::Table);
                let steps = current_steps
                    .raw_get::<Option<Table>>(player + 1)?
                    .map_or(Value::Nil, Value::Table);
                Ok((song, steps))
            }
        })?,
    )?;
    gamestate.set(
        "GetEasiestStepsDifficulty",
        lua.create_function({
            let current_steps = current_steps.clone();
            let step_objects = Arc::clone(&step_objects);
            let enabled = context.players.clone();
            move |lua, _args: MultiValue| {
                let mut easiest = None;
                for (player, active) in enabled.iter().enumerate() {
                    if !active.enabled { continue; }
                    let Some(table) = current_steps.raw_get::<Option<Table>>(player + 1)? else {
                        crate::lua_util::report_script_error(lua,
                            &format!("GetEasiestStepsDifficulty called but p{} hasn't chosen notes", player + 1))?;
                        continue;
                    };
                    let step = step_objects.iter().find(|(known, _)| known.to_pointer() == table.to_pointer())
                        .map(|(_, step)| step).ok_or_else(|| mlua::Error::runtime("Unknown selected Steps"))?;
                    if easiest.is_none_or(|value: crate::SongLuaDifficulty| step.difficulty.sort_key() < value.sort_key()) {
                        easiest = Some(step.difficulty);
                    }
                }
                easiest.map_or(Ok(Value::Nil), |difficulty| Ok(Value::String(lua.create_string(difficulty.sm_name())?)))
            }
        })?,
    )?;
    gamestate.set(
        "SetCurrentSteps",
        lua.create_function({
            let current_steps = current_steps.clone();
            let current_style = current_style.clone();
            let step_objects = Arc::clone(&step_objects);
            let enabled = context.players.clone();
            let owner = gamestate.clone();
            move |lua, args: MultiValue| {
                let player = method_arg(&args, 0).and_then(player_index_from_value)
                    .ok_or_else(|| mlua::Error::runtime("Expected PlayerNumber"))?;
                if matches!(method_arg(&args, 1), None | Some(Value::Nil)) {
                    write_current_steps(lua, &current_steps, player, None)?;
                    #[cfg(feature = "test-support")]
                    record_state_call(lua, "GameState.SetCurrentSteps",
                        vec![serde_json::json!(player), serde_json::Value::Null])?;
                    note_song_lua_side_effect(lua)?;
                    return Ok(owner.clone());
                }
                let Some(Value::Table(steps)) = method_arg(&args, 1) else {
                    return Err(mlua::Error::runtime("Expected Steps"));
                };
                let data = step_objects.iter().find(|(known, _)| known.to_pointer() == steps.to_pointer())
                    .map(|(_, data)| data).ok_or_else(|| mlua::Error::runtime("Expected Steps"))?;
                let style: Table = current_style.raw_get(1)?;
                let style_type: String = style.get::<Function>("GetStepsType")?.call(style.clone())?;
                // Simply Love's default PreferredStyle is none, so native
                // Common.AutoSetStyle is false. Incompatible charts error.
                if data.steps_type != style_type {
                    return Err(mlua::Error::runtime("No compatible style for steps/trail."));
                }
                write_current_steps(lua, &current_steps, player, Some(steps.clone()))?;
                // Native ForceOtherPlayers reads the main selection again after
                // the synchronous broadcast, which may have changed it.
                if let Some(main) = current_steps.raw_get::<Option<Table>>(player + 1)? {
                    let main_data = step_objects.iter().find(|(known, _)| known.to_pointer() == main.to_pointer())
                        .map(|(_, data)| data).ok_or_else(|| mlua::Error::runtime("Unknown selected Steps"))?;
                    for (other, active) in enabled.iter().enumerate() {
                        if !active.enabled { continue; }
                        let selected = current_steps.raw_get::<Option<Table>>(other + 1)?;
                        if selected.as_ref().is_some_and(|table| table.to_pointer() == main.to_pointer()) { continue; }
                        let other_data = selected.as_ref().and_then(|table| step_objects.iter()
                            .find(|(known, _)| known.to_pointer() == table.to_pointer()).map(|(_, data)| data));
                        let matches = other_data.is_some_and(|other| other.music_file == main_data.music_file
                            && !matches!(main_data.steps_type.as_str(), "StepsType_Dance_Routine" | "StepsType_Pump_Routine")
                            && !matches!(other.steps_type.as_str(), "StepsType_Dance_Routine" | "StepsType_Pump_Routine"));
                        if !matches { write_current_steps(lua, &current_steps, other, Some(main.clone()))?; }
                    }
                }
                #[cfg(feature = "test-support")]
                {
                    record_state_call(lua, "GameState.SetCurrentSteps", vec![
                        serde_json::json!(player), serde_json::json!(data.steps_type),
                        serde_json::json!(data.difficulty.sm_name()), serde_json::json!(data.description),
                    ])?;
                }
                note_song_lua_side_effect(lua)?;
                Ok(owner.clone())
            }
        })?,
    )?;
    gamestate.set(
        "GetSongBeat",
        lua.create_function({
            let song_runtime = song_runtime.clone();
            // ITGmania's SongPosition exposes a float, even though Lua numbers
            // and the compile clock retain double precision.
            move |_, _self: Option<Value>| song_runtime.get::<f32>(SONG_LUA_RUNTIME_BEAT_KEY)
        })?,
    )?;
    gamestate.set(
        "GetSongBPS",
        lua.create_function({
            let song_runtime = song_runtime.clone();
            move |_, _self: Option<Value>| song_runtime.get::<f32>(crate::SONG_LUA_RUNTIME_BPS_KEY)
        })?,
    )?;
    gamestate.set(
        "GetCurMusicSeconds",
        lua.create_function({
            let song_runtime = song_runtime.clone();
            move |lua, _self: Option<Value>| crate::runtime::song_music_seconds(lua, &song_runtime)
        })?,
    )?;
    let song_position = create_song_position_table(lua, &song_runtime)?;
    gamestate.set(
        "GetSongPosition",
        lua.create_function(move |_, _args: MultiValue| Ok(song_position.clone()))?,
    )?;
    gamestate.set(
        "GetSongOptionsObject",
        lua.create_function({
            let song_options = song_options.clone();
            move |_, args: MultiValue| {
                Ok(song_options[crate::song_tables::options_level(method_arg(&args, 0))?].clone())
            }
        })?,
    )?;
    let levels = song_options.clone();
    gamestate.set(
        "GetSongOptions",
        lua.create_function(move |lua, args: MultiValue| {
            let level = crate::song_tables::options_level(method_arg(&args, 0))?;
            lua.create_string(crate::song_tables::song_options_text(&levels[level])?)
        })?,
    )?;
    // GameState exposes the current options string without a ModsLevel argument.
    let current = song_options[3].clone();
    gamestate.set(
        "GetSongOptionsString",
        lua.create_function(move |lua, _args: MultiValue| {
            lua.create_string(crate::song_tables::song_options_text(&current)?)
        })?,
    )?;
    gamestate.set(
        "ApplyStageModifiers",
        lua.create_function({
            let state = gamestate.clone();
            let players = players.option_levels.clone();
            let songs = song_options.clone();
            move |lua, args: MultiValue| {
                let player = method_arg(&args, 0)
                    .and_then(player_index_from_value)
                    .ok_or_else(|| mlua::Error::runtime("Expected PlayerNumber"))?;
                let text = method_arg(&args, 1)
                    .cloned()
                    .and_then(read_string)
                    .ok_or_else(|| mlua::Error::runtime("Expected modifier string"))?;
                crate::song_tables::apply_stage_modifiers(lua, &players[player], &songs, &text)?;
                note_song_lua_side_effect(lua)?;
                Ok(state.clone())
            }
        })?,
    )?;
    let state = gamestate.clone();
    gamestate.set(
        "SetSongOptions",
        lua.create_function(move |lua, args: MultiValue| {
            let level = crate::song_tables::options_level(method_arg(&args, 0))?;
            let text = method_arg(&args, 1)
                .cloned()
                .and_then(read_string)
                .ok_or_else(|| mlua::Error::runtime("Expected song options string"))?;
            crate::song_tables::assign_song_options(lua, &song_options, level, &text)?;
            note_song_lua_side_effect(lua)?;
            Ok(state.clone())
        })?,
    )?;
    let master_player = context
        .players
        .iter()
        .position(|player| player.enabled)
        .unwrap_or(0);
    let master_player_name = player_number_name(master_player).to_string();
    gamestate.set(
        "GetMasterPlayerNumber",
        lua.create_function(move |lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(&master_player_name)?))
        })?,
    )?;
    let joined_sides = context
        .players
        .iter()
        .filter(|player| player.enabled)
        .count() as i32;
    gamestate.set(
        "GetNumSidesJoined",
        lua.create_function(move |_, _args: MultiValue| Ok(joined_sides))?,
    )?;
    gamestate.set(
        "GetNumPlayersEnabled",
        lua.create_function(move |_, _args: MultiValue| Ok(joined_sides))?,
    )?;
    gamestate.set(
        "IsCourseMode",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    gamestate.set(
        "IsEventMode",
        lua.create_function(|lua, _args: MultiValue| {
            let prefs: Table = lua.globals().get("PREFSMAN")?;
            prefs.get::<Function>("GetPreference")?.call::<bool>((prefs, "EventMode"))
        })?,
    )?;
    gamestate.set(
        "GetCurrentTrail",
        lua.create_function({
            let current_trail = current_trail.clone();
            move |_, args: MultiValue| {
                let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
                    return Ok(Value::Nil);
                };
                Ok(current_trail
                    .raw_get::<Option<Table>>(player + 1)?
                    .map_or(Value::Nil, Value::Table))
            }
        })?,
    )?;
    gamestate.set(
        "GetCurrentCourse",
        lua.create_function(move |_, _args: MultiValue| Ok(course.clone()))?,
    )?;
    let current_game = create_game_table(lua)?;
    gamestate.set(
        "GetCurrentGame",
        lua.create_function(move |_, _args: MultiValue| Ok(current_game.clone()))?,
    )?;
    gamestate.set(
        "GetCoinMode",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string("CoinMode_Free")?))
        })?,
    )?;
    gamestate.set(
        "GetPremium",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string("Premium_Off")?))
        })?,
    )?;
    gamestate.set(
        "GetCoins",
        lua.create_function(|_, _args: MultiValue| Ok(0))?,
    )?;
    gamestate.set(
        "GetCoinsNeededToJoin",
        lua.create_function(|_, _args: MultiValue| Ok(0))?,
    )?;
    gamestate.set(
        "GetNumStagesLeft",
        lua.create_function(|_, _args: MultiValue| Ok(1))?,
    )?;
    gamestate.set(
        "GetCurrentStageIndex",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    gamestate.set(
        "GetCourseSongIndex",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    gamestate.set(
        "GetPlayMode",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string("PlayMode_Regular")?))
        })?,
    )?;
    gamestate.set(
        "GetSortOrder",
        lua.create_function({
            let current_sort_order = current_sort_order.clone();
            move |lua, _args: MultiValue| {
                let sort = current_sort_order
                    .raw_get::<Option<String>>(1)?
                    .unwrap_or_else(|| "SortOrder_Group".to_string());
                Ok(Value::String(lua.create_string(&sort)?))
            }
        })?,
    )?;
    gamestate.set(
        "GetPlayerFailType",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string("FailType_Immediate")?))
        })?,
    )?;
    gamestate.set(
        "SetCurrentSong",
        lua.create_function({
            let current_song = current_song.clone();
            move |lua, args: MultiValue| {
                if let Some(Value::Table(song)) = method_arg(&args, 0) {
                    current_song.raw_set(1, song.clone())?;
                }
                note_song_lua_side_effect(lua)?;
                Ok(())
            }
        })?,
    )?;
    gamestate.set(
        "SetCurrentStyle",
        lua.create_function({
            let current_style = current_style;
            move |lua, args: MultiValue| {
                let style_name = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_else(|| "single".to_string());
                current_style.raw_set(1, create_style_table(lua, &style_name)?)?;
                note_song_lua_side_effect(lua)?;
                Ok(())
            }
        })?,
    )?;
    gamestate.set(
        "SetCurrentTrail",
        lua.create_function({
            let current_trail = current_trail;
            move |lua, args: MultiValue| {
                let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
                    note_song_lua_side_effect(lua)?;
                    return Ok(());
                };
                if let Some(Value::Table(trail)) = method_arg(&args, 1) {
                    current_trail.raw_set(player + 1, trail.clone())?;
                }
                note_song_lua_side_effect(lua)?;
                Ok(())
            }
        })?,
    )?;
    gamestate.set(
        "InsertCoin",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    for name in [
        "AddStageToPlayer",
        "ResetPlayerOptions",
        "SetPreferredDifficulty",
        "UnjoinPlayer",
    ] {
        gamestate.set(
            name,
            lua.create_function(|lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(true)
            })?,
        )?;
    }
    gamestate.set(
        "JoinPlayer",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(true)
        })?,
    )?;
    gamestate.set(
        "ApplyGameCommand",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    gamestate.set(
        "SaveProfiles",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    // LuaBinding.cpp publishes a writable GameState methods table, and the
    // owned instance resolves methods through that table. Authored mod readers
    // can add or replace class methods without changing the instance identity.
    let class = lua.create_table()?;
    let members = gamestate.pairs::<Value, Value>().collect::<mlua::Result<Vec<_>>>()?;
    for (name, value) in members {
        if let Value::Function(method) = value {
            class.raw_set(name.clone(), method)?;
            gamestate.raw_set(name, Value::Nil)?;
        }
    }
    let metatable = lua.create_table()?;
    metatable.raw_set("__index", class.clone())?;
    metatable.raw_set("__metatable", "(hidden)")?;
    gamestate.set_metatable(Some(metatable))?;
    gamestate.raw_set("__songlua_class_methods", class.clone())?;
    globals.set("GameState", class)?;
    globals.set("GAMESTATE", gamestate)?;

    Ok(SongLuaGameStateGlobals {
        current_sort_order,
        current_song,
        current_steps,
        step_objects,
    })
}

pub fn install_late_globals(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.set("STATSMAN", create_statsman_table(lua, context)?)?;
    // Simply Love's utility helpers broadcast the actual parameters and then
    // call the current Trace/formatter globals, including authored overrides.
    lua.load(r#"------------------------------------------------------------
-- 06 SL-Utilities.lua
-- Utility Functions for Development
--
-- The filename starts with "06" so that it loads before other SL scripts that rely on
-- global functions defined here.  For more information on this numbering system that
-- pretty much no one uses, see: ./Themes/_fallback/Scripts/hierarchy.txt

------------------------------------------------------------
-- define helper functions local to this file first
-- global utility functions (below) will depend on these
------------------------------------------------------------

-- TableToString() function via:
-- http://www.hpelbers.org/lua/print_r
-- Copyright 2009: hans@hpelbers.org
TableToString = function(t, name, indent)
	local tableList = {}
	local table_r

	table_r = function(t, name, indent, full)
		local id = not full and name or type(name)~="number" and tostring(name) or '['..name..']'
		local tag = indent .. id .. ' = '
		local out = {}	-- result

		if type(t) == "table" then
			if tableList[t] ~= nil then
				table.insert(out, tag .. '{} -- ' .. tableList[t] .. ' (self reference)')
			else
				tableList[t]= full and (full .. '.' .. id) or id
				if next(t) then -- Table not empty
					table.insert(out, tag .. '{')
					for key,value in pairs(t) do
						table.insert(out,table_r(value,key,indent .. '|    ',tableList[t]))
					end
					table.insert(out,indent .. '}')
				else
					table.insert(out,tag .. '{}')
				end
			end
		else
			local val = type(t)~="number" and type(t)~="boolean" and '"'..tostring(t)..'"' or tostring(t)
			table.insert(out, tag .. val)
		end

		return table.concat(out, '\n')
	end

	return table_r(t,name or 'Value',indent or '')
end


------------------------------------------------------------
-- GLOBAL UTILITY FUNCTIONS
-- use these to assist in theming/scripting efforts
------------------------------------------------------------

-- SM()
-- Shorthand for SCREENMAN:SystemMessage(), this is useful for rapid iterative
-- testing by allowing us to pretty-print tables and variables to the screen.
--
-- If the first argument is a table, SM() will use TableToString (from above)
-- to display children recursively.  Larger tables will spill offscreen, so
-- rec_print_table() from the _fallback theme is good to know about and use when
-- debugging.  That will recursively pretty-print table structures to ./Logs/Log.txt
--
-- The second arugment is optional and allows you to provide a specific duration,
-- in seconds, for how long you want the text to appear on screen.
-- in Simply Love, the default SystemMessage duration used in ./BGA/ScreenSystemLayer overlay.lua is 3

SM = function( arg, duration, stack )
	local msg

	-- if a table has been passed in, recursively stringify the table's keys and values
	if type( arg ) == "table" then
		msg = TableToString(arg)

	-- otherwise, Lua's standard tostring() should suffice
	else
		msg = tostring(arg)
	end

	-- SCREENMAN:SystemMessage() is effectively a convenience function for broadcasting
	-- "SystemMessage" with certain parameters.  see: ScreenManager.cpp
	-- let's broadcast directly using MESSAGEMAN so that we can also pack in a duration
	-- value (how long to display the SystemMessage for) if so desired
	MESSAGEMAN:Broadcast("SystemMessage", {Message=msg, Duration=duration, Stack=stack})
	Trace(msg)
end"#)
        .set_name("@theme:/Scripts/06 SL-Utilities.lua")
        .exec()?;
    globals.set("ThemePrefsRows", create_theme_prefs_rows_table(lua)?)?;
    globals.set("SL_CustomPrefs", create_sl_custom_prefs_table(lua)?)?;
    globals.set(
        "CustomOptionRow",
        lua.create_function(|lua, value: Value| {
            let Some(name) = read_string(value) else {
                return Ok(Value::Boolean(false));
            };
            create_custom_option_row(lua, &name)
                .map(|row| row.map_or(Value::Boolean(false), Value::Table))
        })?,
    )?;
    for name in [
        "ConfAspectRatio",
        "ConfDisplayResolution",
        "ConfDisplayMode",
        "ConfRefreshRate",
        "ConfFullscreenType",
    ] {
        globals.set(
            name,
            lua.create_function({
                let name = name.to_string();
                move |lua, _args: MultiValue| create_conf_option_row(lua, &name).map(Value::Table)
            })?,
        )?;
    }
    globals.set(
        "OperatorMenuOptionRows",
        create_operator_menu_option_rows_table(lua)?,
    )?;
    globals.set("Branch", create_branch_table(lua)?)?;
    globals.set(
        "SelectMusicOrCourse",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string("ScreenSelectMusic")?))
        })?,
    )?;
    globals.set("PROFILEMAN", create_profileman_table(lua)?)?;
    Ok(())
}

pub fn register_loaded_easing_names(lua: &Lua, host: &mut SongLuaHostState) -> mlua::Result<()> {
    let globals = lua.globals();
    if let Some(ease) = globals.get::<Option<Table>>("ease")? {
        register_easing_table(&ease, host)?;
    }
    if let Some(xero) = globals.get::<Option<Table>>("xero")? {
        register_easing_table(&xero, host)?;
    }
    Ok(())
}

fn register_easing_table(table: &Table, host: &mut SongLuaHostState) -> mlua::Result<()> {
    for &name in SONG_LUA_EASING_NAMES {
        match table.get::<Value>(name)? {
            Value::Function(function) => {
                host.easing_names
                    .insert(function.to_pointer(), name.to_string());
            }
            Value::Table(table) => {
                host.easing_names
                    .insert(table.to_pointer(), name.to_string());
                table.raw_set(SONG_LUA_EASING_NAME_KEY, name)?;
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn install_cmd_helpers(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    for name in ["queuecommand", "playcommand"] {
        globals.set(name, name)?;
    }
    globals.set(
        "cmd",
        lua.create_function(move |lua, args: MultiValue| {
            let mut command_args = args;
            let command_name = command_args.pop_front().and_then(read_string);
            if command_args.is_empty() {
                // Do not retain the incoming name-only argument buffer.
                command_args = MultiValue::new();
            }
            lua.create_function(move |_, actor: Table| {
                let Some(command_name) = command_name.as_deref() else {
                    return Ok(Value::Table(actor));
                };
                let Value::Function(method) = actor.get::<Value>(command_name)? else {
                    return Ok(Value::Table(actor));
                };
                // mlua pushes the borrowed argument list directly onto its stack.
                // The immutable captured list also supports reentrant commands.
                let _ = method.call::<Value>((&actor, &command_args))?;
                Ok(Value::Table(actor))
            })
        })?,
    )?;
    Ok(())
}

fn format_percent_score(lua: &Lua, args: MultiValue) -> mlua::Result<Value> {
    use std::io::Write;
    let value = args.front().cloned().and_then(read_f32).unwrap_or(0.0);
    // Fixed precision f32 output, including its sign, percent suffix and
    // non-finite spellings, fits in 64 bytes even at the largest magnitude.
    let mut buffer = [0_u8; 64];
    let mut remaining = &mut buffer[..];
    write!(remaining, "{:.2}%", value * 100.0).expect("f32 percentage fits in 64 bytes");
    let len = 64 - remaining.len();
    Ok(Value::String(lua.create_string(&buffer[..len])?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_globals_expose_native_zero_based_day_of_year() {
        let lua = Lua::new();
        for (year, month, day, expected) in [
            (2026, 0, 1, 0), (2026, 0, 18, 17), (2026, 0, 19, 18),
            (2024, 1, 29, 59), (2024, 2, 1, 60), (2025, 2, 1, 59),
            (1900, 2, 1, 59), (2000, 2, 1, 60),
            (2024, 11, 31, 365), (2025, 11, 31, 364),
        ] {
            install_date_globals(&lua, SongLuaDateGlobals {
                year, month_of_year: month, day_of_month: day,
                hour: 12, minute: 0, second: 0,
            }).expect("calendar globals");
            let getter = lua.globals().get::<Function>("DayOfYear")
                .expect("native DayOfYear calendar getter");
            assert_eq!(getter.call::<i32>(()).expect("calendar day"), expected,
                "native zero-based day for {year}-{month}-{day}");
        }
    }

    #[test]
    fn arrow_offset_uses_native_music_clock() {
        use deadsync_rules::timing::{
            DelaySegment, SpeedSegment, SpeedUnit, StopSegment, TimingData, TimingSegments,
        };
        let native: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/itgmania-actors/pause-offset.json"
        ))
        .expect("native pause offset control");
        let mut failures = Vec::new();
        for map in native["maps"].as_array().expect("native timing maps") {
            let name = map["name"].as_str().expect("timing map name");
            let timing = TimingData::from_segments(
                0.125, 0.0,
                &TimingSegments {
                    bpms: vec![(0.0, 120.0)],
                    stops: if name == "stop" { vec![StopSegment { beat: 4.0, duration: 1.0 }] } else { vec![] },
                    delays: if name == "delay" { vec![DelaySegment { beat: 4.0, duration: 1.0 }] } else { vec![] },
                    speeds: vec![
                        SpeedSegment { beat: 0.0, ratio: 0.5, delay: 0.0, unit: SpeedUnit::Beats },
                        SpeedSegment { beat: 4.0, ratio: 2.0, delay: 2.0, unit: SpeedUnit::Seconds },
                    ],
                    ..Default::default()
                }, &[],
            );
            for rate in [0.5, 0.75, 1.0, 1.25, 1.5, 2.0] {
                let lua = Lua::new();
                let mut context = SongLuaCompileContext::new(".", "native pause offsets");
                context.song_timing = Some(timing.clone());
                context.player_timing[0] = Some(timing.clone());
                context.player_timing[1] = Some(timing.clone());
                context.song_music_rate = rate;
                let runtime = create_song_runtime_table(&lua, &context).expect("song runtime");
                let song = create_song_table(&lua, &context).expect("native song metadata");
                let players = create_player_tables(&lua, &context, &runtime, &song).expect("native player states");
                lua.globals().set(SONG_LUA_RUNTIME_KEY, runtime.clone()).expect("install song clock");
                let effects = create_arrow_effects_table(&lua, &context, |_| "single".into()).expect("ArrowEffects");
                let offset = effects.get::<Function>("GetYOffset").expect("native offset getter");
                for sample in map["samples"].as_array().expect("native samples") {
                    let elapsed = sample["elapsed"].as_f64().expect("elapsed timestamp") as f32;
                    let beat = sample["beat"].as_f64().expect("native beat") as f32;
                    let note = sample["note"].as_f64().expect("note beat") as f32;
                    let expected = sample["offset"].as_f64().expect("native offset") as f32;
                    runtime.set(SONG_LUA_RUNTIME_BEAT_KEY, beat).expect("native update beat");
                    runtime.set(SONG_LUA_RUNTIME_SECONDS_KEY, f64::from(elapsed) / f64::from(rate)).expect("replay wall clock");
                    for state in [Value::Nil, Value::Table(players.player_states[0].clone()), Value::Table(players.player_states[1].clone())] {
                        let actual = offset.call::<f32>((state, 1, note)).expect("arrow offset");
                        if actual.to_bits() != expected.to_bits() {
                            failures.push(format!("{name} rate={rate} elapsed={elapsed} beat={beat}: {actual} != {expected}"));
                        }
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn broadcast_params_match_native() {
        fn receive(lua: &Lua, _name: &str, params: Option<Value>) -> mlua::Result<()> {
            let globals = lua.globals();
            globals.set("received", globals.get::<usize>("received")? + 1)?;
            globals.set("received_params", params.unwrap_or(Value::Nil))
        }
        let lua = Lua::new();
        lua.globals().set("received", 0).expect("counter");
        install_message_manager_globals(&lua, receive).expect("message manager");
        lua.load(
            r#"
            for _, invalid in ipairs({1, true, "text", function() end}) do
                assert(not pcall(function() MESSAGEMAN:Broadcast("Probe", invalid) end))
            end
            assert(received == 0, "invalid parameters must not dispatch")
            local params = { value = 3 }
            assert(MESSAGEMAN:Broadcast("Probe", params) == MESSAGEMAN)
            assert(received == 1 and received_params == params)
            assert(MESSAGEMAN:Broadcast("Probe", nil) == MESSAGEMAN)
            assert(received == 2 and received_params == nil)
            assert(MESSAGEMAN:Broadcast("Probe") == MESSAGEMAN)
            assert(received == 3 and received_params == nil)
        "#,
        )
        .exec()
        .expect("native Broadcast parameter contract");
    }
}
