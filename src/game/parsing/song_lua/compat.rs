use mlua::{Function, Lua, MultiValue, Table, Value, ffi};
use std::ffi::c_int;
use std::fs;
use std::path::Path;

use super::actor_host::{
    current_gamestate_player_value, current_gamestate_value, current_song_value,
    current_steps_value, retarget_loader_env,
};
use deadsync_song_lua::{
    actor_util_class_registered, actor_util_file_type, create_author_table,
    create_background_filter_values, create_color_constants_table, create_credits_table,
    create_cryptman_table, create_ex_judgment_counts, create_index_array, create_ini_file_table,
    create_network_table, create_rage_file_util_table, create_range_table, create_sl_streams,
    create_split_table, create_string_array, create_version_parts_table, deduplicate_lua_table,
    file_path_string, fileman_dir_listing, find_compat_files, init_sl_streams, json_to_lua_value,
    lua_binary_to_hex, lua_table_to_string, lua_to_json_value, make_color_table, map_lua_table,
    method_arg, note_song_lua_side_effect, path_basename, player_index_from_value,
    player_short_name, preprocess_lua_cmd_syntax, read_boolish, read_color_call, read_color_value,
    read_f32, read_string, resolve_compat_path, rotate_lua_table, stringify_lua_table,
    strip_sprite_hints, timing_window_arg_index, timing_window_seconds, truthy,
    worst_judgment_from_offsets,
};
use deadsync_song_lua::{
    is_minimum_product_version as crate_is_minimum_product_version,
    is_product_version as crate_is_product_version,
};

use super::theme_colors::install_theme_color_helpers;
use super::{
    SONG_LUA_NOTE_COLUMNS, SONG_LUA_PRODUCT_VERSION, SONG_LUA_THEME_NAME,
    SONG_LUA_THEME_PATH_PREFIX, is_compile_global_name, seconds_to_hhmmss, set_string_method,
};

pub(super) fn create_fileman_table(lua: &Lua, song_dir: &Path) -> mlua::Result<Table> {
    let fileman = lua.create_table()?;
    let song_dir = song_dir.to_path_buf();
    let listing_song_dir = song_dir.clone();
    fileman.set(
        "GetDirListing",
        lua.create_function(move |lua, args: MultiValue| {
            fileman_dir_listing_table(lua, &listing_song_dir, &args).map(Value::Table)
        })?,
    )?;
    let file_song_dir = song_dir.clone();
    fileman.set(
        "DoesFileExist",
        lua.create_function(move |_, args: MultiValue| {
            let Some(raw_path) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(false);
            };
            Ok(resolve_compat_path(&file_song_dir, raw_path.as_str()).exists())
        })?,
    )?;
    let size_song_dir = song_dir.clone();
    fileman.set(
        "GetFileSizeBytes",
        lua.create_function(move |_, args: MultiValue| {
            let Some(raw_path) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(0_i64);
            };
            let size = resolve_compat_path(&size_song_dir, raw_path.as_str())
                .metadata()
                .ok()
                .filter(|metadata| metadata.is_file())
                .map(|metadata| metadata.len().min(i64::MAX as u64) as i64)
                .unwrap_or(0);
            Ok(size)
        })?,
    )?;
    fileman.set(
        "GetHashForFile",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    fileman.set(
        "Copy",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(false)
        })?,
    )?;
    for (name, value) in [("CreateDir", true), ("Remove", true), ("Unzip", false)] {
        fileman.set(
            name,
            lua.create_function(move |lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(value)
            })?,
        )?;
    }
    fileman.set(
        "FlushDirCache",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    Ok(fileman)
}

fn fileman_dir_listing_table(lua: &Lua, song_dir: &Path, args: &MultiValue) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let Some(raw_path) = method_arg(args, 0).cloned().and_then(read_string) else {
        return Ok(table);
    };
    let only_dirs = method_arg(args, 1)
        .cloned()
        .and_then(read_boolish)
        .unwrap_or(false);
    let return_path_too = method_arg(args, 2)
        .cloned()
        .and_then(read_boolish)
        .unwrap_or(false);

    let entries = fileman_dir_listing(song_dir, raw_path.as_str(), only_dirs, return_path_too)
        .map_err(mlua::Error::external)?;
    for (idx, entry) in entries.into_iter().enumerate() {
        table.raw_set(idx + 1, entry)?;
    }
    Ok(table)
}

pub(super) fn install_stdlib_compat(lua: &Lua, song_dir: &Path) -> mlua::Result<()> {
    let globals = lua.globals();
    let table: Table = globals.get("table")?;
    table.set(
        "getn",
        lua.create_function(|_, value: Value| {
            Ok(match value {
                Value::Table(table) => table.raw_len() as i64,
                _ => 0,
            })
        })?,
    )?;
    table.set(
        "rotate_right",
        lua.create_function(|lua, args: MultiValue| rotate_lua_table(lua, &args, false))?,
    )?;
    table.set(
        "rotate_left",
        lua.create_function(|lua, args: MultiValue| rotate_lua_table(lua, &args, true))?,
    )?;
    let string: Table = globals.get("string")?;
    if matches!(string.get::<Value>("gfind")?, Value::Nil) {
        let gmatch = string.get::<Value>("gmatch")?;
        string.set("gfind", gmatch)?;
    }
    string.set(
        "split",
        lua.create_function(|lua, args: MultiValue| {
            let text = args
                .front()
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let separator = args
                .get(1)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            create_split_table(lua, &text, &separator)
        })?,
    )?;
    let math: Table = globals.get("math")?;
    if matches!(math.get::<Value>("round")?, Value::Nil) {
        math.set(
            "round",
            lua.create_function(|_, value: f64| {
                Ok(if value >= 0.0 {
                    (value + 0.5).floor()
                } else {
                    (value - 0.5).ceil()
                })
            })?,
        )?;
    }
    if matches!(math.get::<Value>("clamp")?, Value::Nil) {
        math.set(
            "clamp",
            lua.create_function(|_, (value, min, max): (f64, f64, f64)| Ok(value.clamp(min, max)))?,
        )?;
    }
    if matches!(math.get::<Value>("mod")?, Value::Nil) {
        math.set(
            "mod",
            lua.create_function(|_, (left, right): (f64, f64)| {
                Ok(if right == 0.0 { f64::NAN } else { left % right })
            })?,
        )?;
    }
    globals.set("unpack", table.get::<Value>("unpack")?)?;
    globals.set(
        "split",
        lua.create_function(|lua, args: MultiValue| {
            let separator = args
                .front()
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let text = args
                .get(1)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            create_split_table(lua, &text, &separator)
        })?,
    )?;
    globals.set(
        "Basename",
        lua.create_function(|lua, value: Value| {
            Ok(Value::String(lua.create_string(path_basename(
                &read_string(value).unwrap_or_default(),
            ))?))
        })?,
    )?;
    globals.set(
        "Var",
        lua.create_function(|lua, args: MultiValue| {
            let name = args
                .front()
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            if name == "GameCommand" {
                return Ok(Value::Table(create_game_command_table(lua)?));
            }
            Ok(Value::String(lua.create_string(&name)?))
        })?,
    )?;
    globals.set("ActorUtil", create_actor_util_table(lua, song_dir)?)?;
    let find_files_song_dir = song_dir.to_path_buf();
    globals.set(
        "findFiles",
        lua.create_function(move |lua, args: MultiValue| {
            find_compat_files_table(lua, &find_files_song_dir, &args)
        })?,
    )?;
    globals.set(
        "cleanGSub",
        lua.create_function(|lua, args: MultiValue| {
            let text = args
                .front()
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let needle = args
                .get(1)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let replacement = args
                .get(2)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            Ok(Value::String(
                lua.create_string(text.replace(&needle, &replacement))?,
            ))
        })?,
    )?;
    globals.set(
        "range",
        lua.create_function(|lua, args: MultiValue| create_range_table(lua, &args))?,
    )?;
    globals.set(
        "stringify",
        lua.create_function(|lua, args: MultiValue| stringify_lua_table(lua, &args))?,
    )?;
    globals.set(
        "map",
        lua.create_function(|lua, (function, table): (Function, Table)| {
            map_lua_table(lua, &function, &table)
        })?,
    )?;
    globals.set(
        "deduplicate",
        lua.create_function(|lua, table: Table| deduplicate_lua_table(lua, &table))?,
    )?;
    globals.set(
        "TableToString",
        lua.create_function(|_, args: MultiValue| Ok(lua_table_to_string(&args)))?,
    )?;
    globals.set(
        "force_to_range",
        lua.create_function(|_, (min, number, max): (f64, f64, f64)| Ok(number.clamp(min, max)))?,
    )?;
    globals.set(
        "wrapped_index",
        lua.create_function(|_, (start, offset, set_size): (i64, i64, i64)| {
            Ok(if set_size <= 0 {
                0
            } else {
                (start + offset - 1).rem_euclid(set_size) + 1
            })
        })?,
    )?;
    globals.set(
        "GetVersionParts",
        lua.create_function(|lua, args: MultiValue| {
            let version = args
                .front()
                .cloned()
                .and_then(read_string)
                .unwrap_or_else(|| SONG_LUA_PRODUCT_VERSION.to_string());
            create_version_parts_table(lua, &version)
        })?,
    )?;
    globals.set(
        "GetProductVersion",
        lua.create_function(|lua, _args: MultiValue| {
            create_version_parts_table(lua, SONG_LUA_PRODUCT_VERSION)
        })?,
    )?;
    globals.set(
        "IsProductVersion",
        lua.create_function(|_, args: MultiValue| Ok(is_product_version(&args)))?,
    )?;
    globals.set(
        "IsMinimumProductVersion",
        lua.create_function(|_, args: MultiValue| Ok(is_minimum_product_version(&args)))?,
    )?;
    globals.set(
        "IsITGmania",
        lua.create_function(|_, _args: MultiValue| Ok(true))?,
    )?;
    globals.set(
        "StepManiaVersionIsSupported",
        lua.create_function(|_, _args: MultiValue| Ok(true))?,
    )?;
    globals.set(
        "MinimumVersionString",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string("1.2.0")?))
        })?,
    )?;
    globals.set(
        "CurrentGameIsSupported",
        lua.create_function(|_, _args: MultiValue| Ok(true))?,
    )?;
    globals.set(
        "GetThemeVersion",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(SONG_LUA_PRODUCT_VERSION)?))
        })?,
    )?;
    globals.set(
        "GetAuthor",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(
                lua.create_string("DeadSync song-Lua compat")?,
            ))
        })?,
    )?;
    globals.set(
        "SupportsRenderToTexture",
        lua.create_function(|_, _args: MultiValue| Ok(true))?,
    )?;
    globals.set(
        "BackgroundFilterValues",
        lua.create_function(|lua, _args: MultiValue| create_background_filter_values(lua))?,
    )?;
    globals.set(
        "NumJudgmentsAvailable",
        lua.create_function(|_, _args: MultiValue| Ok(5_i64))?,
    )?;
    globals.set(
        "DetermineTimingWindow",
        lua.create_function(|_, args: MultiValue| {
            let offset = args
                .front()
                .cloned()
                .and_then(read_f32)
                .unwrap_or(0.0)
                .abs();
            for index in 1..=5 {
                if offset <= timing_window_seconds(index, "", false) {
                    return Ok(index);
                }
            }
            Ok(5)
        })?,
    )?;
    globals.set(
        "GetCredits",
        lua.create_function(|lua, _args: MultiValue| create_credits_table(lua))?,
    )?;
    globals.set(
        "GetStepsCredit",
        lua.create_function(|lua, _args: MultiValue| lua.create_table())?,
    )?;
    globals.set(
        "IsSpooky",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    globals.set(
        "StripSpriteHints",
        lua.create_function(|_, filename: String| Ok(strip_sprite_hints(&filename)))?,
    )?;
    globals.set(
        "GetJudgmentGraphics",
        lua.create_function(|lua, _args: MultiValue| create_string_array(lua, &["None"]))?,
    )?;
    globals.set(
        "GetHoldJudgments",
        lua.create_function(|lua, _args: MultiValue| create_string_array(lua, &["None"]))?,
    )?;
    globals.set(
        "GetHeldMissGraphics",
        lua.create_function(|lua, _args: MultiValue| create_string_array(lua, &["None"]))?,
    )?;
    globals.set(
        "GetComboFonts",
        lua.create_function(|lua, _args: MultiValue| create_string_array(lua, &["None"]))?,
    )?;
    globals.set(
        "GetColumnMapping",
        lua.create_function(|lua, _args: MultiValue| {
            create_index_array(lua, SONG_LUA_NOTE_COLUMNS)
        })?,
    )?;
    globals.set(
        "GetPlayerOptionsString",
        lua.create_function(|lua, _args: MultiValue| Ok(Value::String(lua.create_string("")?)))?,
    )?;
    globals.set(
        "GetFallbackBanner",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(
                SONG_LUA_THEME_PATH_PREFIX.trim_end_matches('/'),
            )?))
        })?,
    )?;
    globals.set(
        "TotalCourseLength",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    globals.set(
        "TotalCourseLengthPlayed",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    globals.set(
        "IsGameAndMenuButton",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    for name in ["LoadGuest", "LoadProfileCustom", "SaveProfileCustom"] {
        globals.set(
            name,
            lua.create_function(|lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(true)
            })?,
        )?;
    }
    for name in ["GetAvatarPath", "GetPlayerAvatarPath"] {
        globals.set(
            name,
            lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
        )?;
    }
    globals.set(
        "getAuthorTable",
        lua.create_function(|lua, args: MultiValue| create_author_table(lua, args.front()))?,
    )?;
    globals.set(
        "courseLengthBySong",
        lua.create_function(|lua, _args: MultiValue| lua.create_table())?,
    )?;
    globals.set(
        "SecondsToHMMSS",
        lua.create_function(|lua, seconds: f64| {
            Ok(Value::String(
                lua.create_string(seconds_to_hhmmss(seconds))?,
            ))
        })?,
    )?;
    globals.set(
        "ParseChartInfo",
        lua.create_function(|lua, args: MultiValue| parse_chart_info(lua, &args))?,
    )?;
    globals.set(
        "ivalues",
        lua.create_function(|lua, table: Table| {
            let mut index = 0_i64;
            lua.create_function_mut(move |_, ()| {
                index += 1;
                table.raw_get::<Value>(index)
            })
        })?,
    )?;
    globals.set("Trace", lua.create_function(|_, _msg: String| Ok(()))?)?;
    globals.set("debug", create_debug_table(lua)?)?;
    globals.set("lua", create_lua_compat_table(lua, song_dir)?)?;
    globals.set(
        "Warn",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    globals.set(
        "color",
        lua.create_function(|lua, args: MultiValue| {
            Ok(match read_color_call(&args) {
                Some(color) => Value::Table(make_color_table(lua, color)?),
                None => Value::Nil,
            })
        })?,
    )?;
    globals.set(
        "lerp_color",
        lua.create_function(|lua, args: MultiValue| {
            let Some(percent) = args.front().cloned().and_then(read_f32) else {
                return Ok(Value::Nil);
            };
            let Some(a) = args.get(1).cloned().and_then(read_color_value) else {
                return Ok(Value::Nil);
            };
            let Some(b) = args.get(2).cloned().and_then(read_color_value) else {
                return Ok(Value::Nil);
            };
            Ok(Value::Table(make_color_table(
                lua,
                [
                    a[0] + (b[0] - a[0]) * percent,
                    a[1] + (b[1] - a[1]) * percent,
                    a[2] + (b[2] - a[2]) * percent,
                    a[3] + (b[3] - a[3]) * percent,
                ],
            )?))
        })?,
    )?;
    globals.set("Color", create_color_constants_table(lua)?)?;
    install_theme_color_helpers(lua, &globals)?;
    for (name, value) in [
        ("left", "left"),
        ("center", "center"),
        ("middle", "middle"),
        ("right", "right"),
        ("top", "top"),
        ("bottom", "bottom"),
        ("HorizAlign_Left", "HorizAlign_Left"),
        ("HorizAlign_Center", "HorizAlign_Center"),
        ("HorizAlign_Right", "HorizAlign_Right"),
        ("VertAlign_Top", "VertAlign_Top"),
        ("VertAlign_Middle", "VertAlign_Middle"),
        ("VertAlign_Bottom", "VertAlign_Bottom"),
    ] {
        globals.set(name, value)?;
    }
    globals.set(
        "setfenv",
        lua.create_function(|lua, (target, env): (Value, Table)| match target {
            Value::Function(function) => {
                retarget_loader_env(lua, &function, &env)?;
                let _ = function.set_environment(env.clone())?;
                Ok(Value::Function(function))
            }
            Value::Integer(_) | Value::Number(_) => {
                if let Some(current_env) = lua
                    .globals()
                    .get::<Option<Table>>("__songlua_current_chunk_env")?
                {
                    current_env.set("__songlua_env_target", env)?;
                }
                Ok(target)
            }
            _ => Ok(target),
        })?,
    )?;
    globals.set(
        "loadstring",
        lua.create_function(|lua, (code, chunk_name): (String, Option<String>)| {
            let code = preprocess_lua_cmd_syntax(&code).map_err(mlua::Error::external)?;
            let mut chunk = lua.load(&code);
            if let Some(chunk_name) = chunk_name.as_deref() {
                chunk = chunk.set_name(chunk_name);
            }
            Ok(Value::Function(chunk.into_function()?))
        })?,
    )?;
    globals.set("FILEMAN", create_fileman_table(lua, song_dir)?)?;
    globals.set("CRYPTMAN", create_cryptman_table(lua)?)?;
    globals.set("NETWORK", create_network_table(lua)?)?;
    globals.set(
        "IniFile",
        create_ini_file_table(lua, SONG_LUA_THEME_NAME, SONG_LUA_PRODUCT_VERSION)?,
    )?;
    globals.set("RageFileUtil", create_rage_file_util_table(lua)?)?;
    globals.set(
        "JsonDecode",
        lua.create_function(|lua, value: Value| match value {
            Value::String(text) => {
                let bytes = text.as_bytes();
                serde_json::from_slice::<serde_json::Value>(&bytes)
                    .ok()
                    .map(|value| json_to_lua_value(lua, value))
                    .transpose()?
                    .map_or_else(|| Ok(Value::Table(lua.create_table()?)), Ok)
            }
            _ => Ok(Value::Table(lua.create_table()?)),
        })?,
    )?;
    globals.set(
        "JsonEncode",
        lua.create_function(|_, args: MultiValue| {
            let value = args.front().cloned().unwrap_or(Value::Nil);
            Ok(serde_json::to_string(&lua_to_json_value(value)).unwrap_or_default())
        })?,
    )?;
    globals.set(
        "BinaryToHex",
        lua.create_function(|lua, value: Value| {
            Ok(Value::String(
                lua.create_string(lua_binary_to_hex(value).as_str())?,
            ))
        })?,
    )?;
    globals.set(
        "CalculateExScore",
        lua.create_function(|_, _args: MultiValue| Ok((0.0_f32, 0_i64, 0_i64)))?,
    )?;
    globals.set(
        "GetExJudgmentCounts",
        lua.create_function(|lua, _args: MultiValue| create_ex_judgment_counts(lua))?,
    )?;
    globals.set(
        "GetTimingWindow",
        lua.create_function(|_, args: MultiValue| {
            let index = args
                .front()
                .cloned()
                .and_then(timing_window_arg_index)
                .unwrap_or(1);
            let mode = args
                .get(1)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let tenms = args.get(2).is_some_and(truthy);
            Ok(timing_window_seconds(index, &mode, tenms))
        })?,
    )?;
    globals.set(
        "GetWorstJudgment",
        lua.create_function(|_, value: Value| Ok(worst_judgment_from_offsets(value)))?,
    )?;
    Ok(())
}

fn find_compat_files_table(lua: &Lua, song_dir: &Path, args: &MultiValue) -> mlua::Result<Table> {
    let dir = args
        .front()
        .cloned()
        .and_then(read_string)
        .unwrap_or_default();
    let extension = args
        .get(1)
        .cloned()
        .and_then(read_string)
        .unwrap_or_else(|| "ogg".to_string())
        .to_string();
    let files = find_compat_files(song_dir, &dir, &extension).map_err(mlua::Error::external)?;
    let table = lua.create_table()?;
    for (index, file) in files.into_iter().enumerate() {
        table.raw_set(index + 1, file)?;
    }
    Ok(table)
}

fn create_actor_util_table(lua: &Lua, song_dir: &Path) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let resolve_song_dir = song_dir.to_path_buf();
    table.set(
        "ResolvePath",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(path) = args.front().cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let resolved = resolve_compat_path(&resolve_song_dir, &path);
            let out = if resolved.exists() {
                file_path_string(resolved.as_path())
            } else {
                path
            };
            Ok(Value::String(lua.create_string(&out)?))
        })?,
    )?;
    table.set(
        "GetFileType",
        lua.create_function(|lua, args: MultiValue| {
            let file_type = args
                .front()
                .cloned()
                .and_then(read_string)
                .map(|path| actor_util_file_type(&path))
                .unwrap_or("FileType_Unknown");
            Ok(Value::String(lua.create_string(file_type)?))
        })?,
    )?;
    table.set(
        "IsRegisteredClass",
        lua.create_function(|_, args: MultiValue| {
            let registered = args
                .front()
                .cloned()
                .and_then(read_string)
                .is_some_and(|name| actor_util_class_registered(&name));
            Ok(registered)
        })?,
    )?;
    for method in ["LoadAllCommands", "LoadAllCommandsFromName"] {
        table.set(
            method,
            lua.create_function(|lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(())
            })?,
        )?;
    }
    table.set(
        "LoadAllCommandsAndSetXY",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    Ok(table)
}

fn create_game_command_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetIndex",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    for name in [
        "GetText",
        "GetName",
        "GetScreen",
        "GetIcon",
        "GetProfileID",
        "GetAnnouncer",
        "GetPreferredModifiers",
        "GetStageModifiers",
    ] {
        set_string_method(lua, &table, name, "")?;
    }
    table.set(
        "GetMultiPlayer",
        lua.create_function(|_, _args: MultiValue| Ok(-1_i64))?,
    )?;
    table.set(
        "GetStyle",
        lua.create_function(|lua, _args: MultiValue| {
            current_gamestate_value(lua, "GetCurrentStyle")
        })?,
    )?;
    table.set(
        "GetSong",
        lua.create_function(|lua, _args: MultiValue| current_song_value(lua))?,
    )?;
    table.set(
        "GetSteps",
        lua.create_function(|lua, _args: MultiValue| current_steps_value(lua, 0))?,
    )?;
    table.set(
        "GetCourse",
        lua.create_function(|lua, _args: MultiValue| {
            current_gamestate_value(lua, "GetCurrentCourse")
        })?,
    )?;
    table.set(
        "GetTrail",
        lua.create_function(|lua, _args: MultiValue| {
            current_gamestate_player_value(lua, "GetCurrentTrail", 0)
        })?,
    )?;
    table.set(
        "GetCharacter",
        lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
    )?;
    table.set(
        "GetSongGroup",
        lua.create_function(|lua, _args: MultiValue| {
            let Value::Table(song) = current_song_value(lua)? else {
                return Ok(String::new());
            };
            let Some(method) = song.get::<Option<Function>>("GetGroupName")? else {
                return Ok(String::new());
            };
            method.call::<String>(song)
        })?,
    )?;
    table.set(
        "GetUrl",
        lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
    )?;
    for (method, value) in [
        ("GetDifficulty", "Difficulty_Invalid"),
        ("GetCourseDifficulty", "Difficulty_Invalid"),
        ("GetPlayMode", "PlayMode_Invalid"),
        ("GetSortOrder", "SortOrder_Invalid"),
    ] {
        set_string_method(lua, &table, method, value)?;
    }
    table.set(
        "ApplyToStyle",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    Ok(table)
}

fn is_product_version(args: &MultiValue) -> bool {
    crate_is_product_version(SONG_LUA_PRODUCT_VERSION, args)
}

fn is_minimum_product_version(args: &MultiValue) -> bool {
    crate_is_minimum_product_version(SONG_LUA_PRODUCT_VERSION, args)
}

fn parse_chart_info(lua: &Lua, args: &MultiValue) -> mlua::Result<Table> {
    let player = args
        .get(1)
        .and_then(player_index_from_value)
        .map(player_short_name)
        .unwrap_or("P1");
    let globals = lua.globals();
    let sl = match globals.get::<Option<Table>>("SL")? {
        Some(table) => table,
        None => {
            let table = lua.create_table()?;
            globals.set("SL", table.clone())?;
            table
        }
    };
    let player_table = match sl.get::<Option<Table>>(player)? {
        Some(table) => table,
        None => {
            let table = lua.create_table()?;
            sl.set(player, table.clone())?;
            table
        }
    };
    let streams = match player_table.get::<Option<Table>>("Streams")? {
        Some(table) => table,
        None => {
            let table = create_sl_streams(lua)?;
            player_table.set("Streams", table.clone())?;
            table
        }
    };
    init_sl_streams(lua, &streams)?;
    note_song_lua_side_effect(lua)?;
    Ok(streams)
}

fn create_debug_table(lua: &Lua) -> mlua::Result<Table> {
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

fn create_lua_compat_table(lua: &Lua, song_dir: &Path) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let read_song_dir = song_dir.to_path_buf();
    table.set(
        "ReadFile",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(raw_path) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let path = resolve_compat_path(&read_song_dir, &raw_path);
            match fs::read_to_string(path) {
                Ok(text) => Ok(Value::String(lua.create_string(&text)?)),
                Err(_) => Ok(Value::Nil),
            }
        })?,
    )?;
    table.set(
        "WriteFile",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(true)
        })?,
    )?;
    table.set(
        "ReportScriptError",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    Ok(table)
}

pub(super) fn create_chunk_env_proxy(lua: &Lua, target: Table) -> mlua::Result<Table> {
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

pub(super) fn initial_chunk_environment(lua: &Lua, path: &Path) -> mlua::Result<Table> {
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
