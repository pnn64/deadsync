use std::collections::HashMap;
use std::ffi::c_void;
use std::path::Path;

use chrono::{Datelike, Local, Timelike};
use mlua::{Function, Lua, MultiValue, Table, Value};

use crate::{
    LUA_PLAYERS, SONG_LUA_EASING_NAME_KEY, SONG_LUA_EASING_NAMES,
    SONG_LUA_PLAYER_OPTION_CAPABILITIES, SONG_LUA_PLAYER_OPTION_MULTICOL_PREFIXES,
    SONG_LUA_PRODUCT_FAMILY, SONG_LUA_PRODUCT_ID, SONG_LUA_PRODUCT_VERSION,
    SONG_LUA_RUNTIME_BEAT_KEY, SONG_LUA_RUNTIME_KEY, SONG_LUA_RUNTIME_SECONDS_KEY,
    SONG_LUA_SIDE_EFFECT_COUNT_KEY, SONG_LUA_SOUND_PATHS_KEY, SongLuaCompileContext,
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
    easiest_steps_difficulty, format_number_and_suffix, format_song_options_text,
    install_def_globals, install_default_stdlib_compat, install_file_loader_globals,
    is_song_lua_audio_path, lua_values_equal, method_arg, note_song_lua_side_effect,
    player_index_from_value, player_number_name, read_f32, read_string, record_song_lua_broadcast,
    resolve_script_path, scale_value, seconds_to_hhmmss, seconds_to_mmss, seconds_to_mmss_ms_ms,
    seconds_to_mss, seconds_to_mss_ms_ms, song_dir_string, song_display_bps,
    song_lua_human_player_count, song_lua_runtime_number, song_lua_style_column_x, song_music_rate,
    theme_string, truthy,
};

pub const SONG_LUA_STARTUP_MESSAGE: &str = "__songlua_startup";

#[derive(Default)]
pub struct SongLuaHostState {
    pub easing_names: HashMap<*const c_void, String>,
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
    host: &mut SongLuaHostState,
    noteskin_resolver: SongLuaNoteskinResolver,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
    create_named_child_actor: fn(&Lua, &Table, &str) -> mlua::Result<Table>,
    install_actor_methods: fn(&Lua, &Table) -> mlua::Result<()>,
) -> mlua::Result<()> {
    install_default_stdlib_compat(lua, context.song_dir.as_path())?;
    install_ease_table(lua, host)?;
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
    let current_sort_order = game_state_globals.current_sort_order;
    let current_song = game_state_globals.current_song;

    let top_screen = create_top_screen_table(
        lua,
        context,
        current_sort_order,
        current_song,
        create_dummy_actor,
        create_named_child_actor,
    )?;
    install_screen_manager_globals(
        lua,
        top_screen.top_screen.clone(),
        top_screen.players.clone(),
    )?;
    install_sound_globals(lua, context.song_dir.as_path())?;
    install_message_manager_globals(lua, broadcast_song_lua_message)?;
    install_late_globals(lua, context)?;
    Ok(())
}

pub fn clone_lua_value(lua: &Lua, value: Value) -> mlua::Result<Value> {
    match value {
        Value::Table(table) => {
            let cloned = lua.create_table()?;
            for pair in table.pairs::<Value, Value>() {
                let (key, value) = pair?;
                cloned.set(clone_lua_value(lua, key)?, clone_lua_value(lua, value)?)?;
            }
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
pub fn is_compile_global_name(name: &str) -> bool {
    matches!(
        name,
        "prefix_globals" | "mods" | "mod_time" | "mods_ease" | "mod_perframes" | "mod_actions"
    )
}

pub fn create_chunk_env_proxy(lua: &Lua, target: Table) -> mlua::Result<Table> {
    let proxy = lua.create_table()?;
    proxy.set("__songlua_env_target", target.clone())?;
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
    let proxy_for_newindex = proxy.clone();
    let globals_for_newindex = globals.clone();
    mt.set(
        "__newindex",
        lua.create_function(move |_, (_self, key, value): (Table, Value, Value)| {
            let target: Table = proxy_for_newindex.get("__songlua_env_target")?;
            target.set(key.clone(), value.clone())?;
            if let Some(name) = read_string(key.clone())
                && is_compile_global_name(name.as_str())
            {
                globals_for_newindex.set(name, value)?;
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
    globals.set("PlayerNumber", create_player_number_table(lua)?)?;
    globals.set("OtherPlayer", create_other_player_table(lua)?)?;
    globals.set("Difficulty", create_difficulty_table(lua)?)?;
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
    globals.set(
        "FormatPercentScore",
        lua.create_function(|lua, args: MultiValue| {
            let value = args.front().cloned().and_then(read_f32).unwrap_or(0.0);
            Ok(Value::String(
                lua.create_string(format!("{:.2}%", value * 100.0))?,
            ))
        })?,
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
    let screen_width = context.screen_width.max(1.0);
    let screen_height = context.screen_height.max(1.0);
    let global_offset_seconds = context.global_offset_seconds;
    let display_aspect_ratio = screen_width / screen_height.max(1.0);
    let display_width = screen_width.round() as i32;
    let display_height = screen_height.round() as i32;
    globals.set(
        "PREFSMAN",
        create_prefsmgr_table(
            lua,
            global_offset_seconds,
            display_aspect_ratio,
            display_width,
            display_height,
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

    let screenman = lua.create_table()?;
    let top_screen_table = top_screen.clone();
    screenman.set(
        "GetTopScreen",
        lua.create_function(move |_, _args: MultiValue| Ok(top_screen_table.clone()))?,
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
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, args: MultiValue| {
                if let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) {
                    top_screen.set("Name", name)?;
                }
                note_song_lua_side_effect(lua)?;
                Ok(())
            }
        })?,
    )?;
    screenman.set(
        "AddNewScreenToTop",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, args: MultiValue| {
                if let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) {
                    top_screen.set("Name", name)?;
                }
                note_song_lua_side_effect(lua)?;
                Ok(())
            }
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
    let globals = lua.globals();
    let messageman = lua.create_table()?;
    let messageman_for_broadcast = messageman.clone();
    messageman.set(
        "Broadcast",
        lua.create_function(move |lua, args: MultiValue| {
            if let Some(message) = method_arg(&args, 0).cloned().and_then(read_string) {
                let params = method_arg(&args, 1).cloned();
                note_song_lua_side_effect(lua)?;
                record_song_lua_broadcast(lua, &message, params.is_some())?;
                broadcast(lua, &message, params)?;
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
    let Some(method) = player_state.get::<Option<Function>>("GetPlayerOptions")? else {
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
    Ok(read_f32(method.call::<Value>((options, column))?)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0))
}

pub fn create_arrow_effects_table(
    lua: &Lua,
    current_style_name: fn(&Lua) -> String,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetYOffset",
        lua.create_function(|_, args: MultiValue| {
            let speed = arrow_effects_speed_multiplier(&args)?;
            Ok(args
                .get(2)
                .cloned()
                .and_then(read_f32)
                .map(|beat| 64.0 * beat * speed)
                .unwrap_or(0.0_f32))
        })?,
    )?;
    table.set(
        "GetYPos",
        lua.create_function(|_, args: MultiValue| {
            let y_offset = args.get(2).cloned().and_then(read_f32).unwrap_or(0.0_f32);
            let reverse = arrow_effects_reverse_percent(&args)?;
            let receptor_y = (THEME_RECEPTOR_Y_REV - THEME_RECEPTOR_Y_STD)
                .mul_add(reverse, THEME_RECEPTOR_Y_STD);
            Ok(receptor_y + y_offset * (1.0 - 2.0 * reverse))
        })?,
    )?;
    table.set(
        "GetXPos",
        lua.create_function(move |lua, args: MultiValue| {
            let column_index = args
                .get(1)
                .cloned()
                .and_then(read_f32)
                .map(|value| value as isize - 1)
                .filter(|value| *value >= 0)
                .map(|value| value as usize)
                .unwrap_or(0);
            Ok(song_lua_style_column_x(
                &current_style_name(lua),
                column_index,
            ))
        })?,
    )?;
    table.set(
        "GetZPos",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    table.set(
        "GetRotationX",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    table.set(
        "GetRotationY",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    table.set(
        "GetRotationZ",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    table.set(
        "GetZoom",
        lua.create_function(|_, _args: MultiValue| Ok(1.0_f32))?,
    )?;
    table.set(
        "GetAlpha",
        lua.create_function(|_, _args: MultiValue| Ok(1.0_f32))?,
    )?;
    table.set(
        "GetGlow",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    Ok(table)
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

fn record_song_lua_sound_path(lua: &Lua, song_dir: &Path, args: &MultiValue) -> mlua::Result<()> {
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
    for name in ["PlayMusicPart", "PlayOnce"] {
        let sound_for_return = sound.clone();
        sound.set(
            name,
            lua.create_function({
                let song_dir = song_dir.to_path_buf();
                move |lua, args: MultiValue| {
                    note_song_lua_side_effect(lua)?;
                    record_song_lua_sound_path(lua, song_dir.as_path(), &args)?;
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

pub struct SongLuaGameStateGlobals {
    pub current_sort_order: Table,
    pub current_song: Table,
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
    globals.set("NOTESKIN", noteskin)?;
    install_screen_utility_globals(lua)?;
    globals.set(
        "ArrowEffects",
        create_arrow_effects_table(lua, current_style_name)?,
    )?;
    install_screen_string_globals(lua)?;
    install_game_state_globals(lua, context)
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
    let players = create_player_tables(lua, context, &song_runtime)?;
    let song_options = create_song_options_table(lua, context.song_music_rate)?;
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
            move |_, _args: MultiValue| song_runtime.get::<f32>(SONG_LUA_RUNTIME_SECONDS_KEY)
        })?,
    )?;
    let current_sort_order = lua.create_table()?;
    current_sort_order.raw_set(1, "SortOrder_Group")?;
    let gamestate = lua.create_table()?;
    let game_env = lua.create_table()?;
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
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
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
        lua.create_function(|_, _args: MultiValue| Ok(256.0_f32))?,
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
        lua.create_function(move |_, args: MultiValue| {
            let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
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
            Ok(global_player_states[player]
                .get::<Option<String>>("__songlua_player_options_string")?
                .unwrap_or_default())
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
    let course = create_course_table(lua, context, song.clone(), primary_trail.clone())?;
    globals.set(
        "SONGMAN",
        create_songman_table(
            lua,
            song.clone(),
            players.steps[0].clone(),
            course.clone(),
            context,
        )?,
    )?;
    gamestate.set(
        "GetCurrentSteps",
        lua.create_function({
            let current_steps = current_steps.clone();
            move |_, args: MultiValue| {
                let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
                    return Ok(Value::Nil);
                };
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
    let easiest_steps_difficulty = easiest_steps_difficulty(&context.players);
    gamestate.set(
        "GetEasiestStepsDifficulty",
        lua.create_function(move |lua, _args: MultiValue| {
            let Some(difficulty) = easiest_steps_difficulty else {
                return Ok(Value::Nil);
            };
            Ok(Value::String(lua.create_string(difficulty.sm_name())?))
        })?,
    )?;
    gamestate.set(
        "SetCurrentSteps",
        lua.create_function({
            let current_steps = current_steps.clone();
            move |lua, args: MultiValue| {
                let Some(player) = method_arg(&args, 0).and_then(player_index_from_value) else {
                    return Ok(());
                };
                let Some(steps) = method_arg(&args, 1).and_then(|value| match value {
                    Value::Table(table) => Some(table.clone()),
                    _ => None,
                }) else {
                    return Ok(());
                };
                current_steps.raw_set(player + 1, steps)?;
                note_song_lua_side_effect(lua)?;
                Ok(())
            }
        })?,
    )?;
    gamestate.set(
        "GetSongBeat",
        lua.create_function({
            let song_runtime = song_runtime.clone();
            move |_, _args: MultiValue| song_runtime.get::<f32>(SONG_LUA_RUNTIME_BEAT_KEY)
        })?,
    )?;
    let song_bps = song_display_bps(context);
    gamestate.set(
        "GetSongBPS",
        lua.create_function(move |_, _args: MultiValue| Ok(song_bps))?,
    )?;
    gamestate.set(
        "GetCurMusicSeconds",
        lua.create_function({
            let song_runtime = song_runtime.clone();
            move |_, _args: MultiValue| song_runtime.get::<f32>(SONG_LUA_RUNTIME_SECONDS_KEY)
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
            move |_, _args: MultiValue| Ok(song_options.clone())
        })?,
    )?;
    gamestate.set(
        "GetSongOptions",
        lua.create_function({
            let song_options = song_options.clone();
            move |lua, _args: MultiValue| {
                let rate = song_options
                    .get::<Option<f32>>("__songlua_music_rate")?
                    .unwrap_or(1.0);
                Ok(Value::String(
                    lua.create_string(format_song_options_text(rate))?,
                ))
            }
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
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
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
            let current_style = current_style.clone();
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
            let current_trail = current_trail.clone();
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
    globals.set("GAMESTATE", gamestate)?;

    Ok(SongLuaGameStateGlobals {
        current_sort_order,
        current_song,
    })
}

pub fn install_late_globals(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.set("STATSMAN", create_statsman_table(lua, context)?)?;
    globals.set(
        "SM",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    globals.set("ThemePrefs", create_theme_prefs_table(lua)?)?;
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
    globals.set("SL", create_sl_table(lua, context)?)?;
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

pub fn install_ease_table(lua: &Lua, host: &mut SongLuaHostState) -> mlua::Result<()> {
    let globals = lua.globals();
    let ease = lua.create_table()?;
    for &name in SONG_LUA_EASING_NAMES {
        let function = lua.create_function(
            |_, (_t, b, c, d, _p1, _p2): (f32, f32, f32, f32, Value, Value)| {
                if d.abs() <= f32::EPSILON {
                    Ok(b + c)
                } else {
                    Ok(b + c * (f32::min(f32::max(_t / d, 0.0), 1.0)))
                }
            },
        )?;
        host.easing_names
            .insert(function.to_pointer(), name.to_string());
        ease.set(name, function)?;
    }
    globals.set("ease", ease)?;
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
            let command_name = args.front().cloned().and_then(read_string);
            let command_args = args.into_iter().skip(1).collect::<Vec<_>>();
            lua.create_function(move |_, actor: Table| {
                let Some(command_name) = command_name.as_deref() else {
                    return Ok(Value::Table(actor));
                };
                let Value::Function(method) = actor.get::<Value>(command_name)? else {
                    return Ok(Value::Table(actor));
                };
                let mut call_args = MultiValue::new();
                call_args.push_back(Value::Table(actor.clone()));
                for arg in &command_args {
                    call_args.push_back(arg.clone());
                }
                let _ = method.call::<Value>(call_args)?;
                Ok(Value::Table(actor))
            })
        })?,
    )?;
    Ok(())
}
