use mlua::{Function, Lua, MultiValue, Table, Value, ffi};
use std::ffi::c_int;
use std::fs;
use std::path::{Path, PathBuf};

use super::actor_host::{
    current_gamestate_player_value, current_gamestate_value, current_song_value,
    current_steps_value, retarget_loader_env,
};
use super::runtime::note_song_lua_side_effect;
use super::sl::{create_sl_streams, init_sl_streams, player_short_name};
use super::theme_colors::install_theme_color_helpers;
use super::util::{
    create_color_constants_table, create_string_array, file_path_string, lua_text_value,
    lua_values_equal, make_color_table, method_arg, player_index_from_value,
    preprocess_lua_cmd_syntax, read_boolish, read_color_call, read_color_value, read_f32,
    read_i32_value, read_string, truthy,
};
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

pub(super) fn resolve_compat_path(song_dir: &Path, raw_path: &str) -> PathBuf {
    let path = Path::new(raw_path.trim());
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        song_dir.join(path)
    }
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

    let path = resolve_compat_path(song_dir, raw_path.as_str());
    let mut entries = Vec::new();
    if path.is_dir() {
        entries = fileman_read_dir(&path, None, only_dirs, return_path_too)?;
    } else if let Some(pattern) = path.file_name().and_then(|name| name.to_str())
        && pattern.bytes().any(|byte| matches!(byte, b'*' | b'?'))
        && let Some(parent) = path.parent()
    {
        entries = fileman_read_dir(parent, Some(pattern), only_dirs, return_path_too)?;
    } else if path.exists() && (!only_dirs || path.is_dir()) {
        entries.push(fileman_entry_name(&path, return_path_too));
    }

    entries.sort_unstable();
    for (idx, entry) in entries.into_iter().enumerate() {
        table.raw_set(idx + 1, entry)?;
    }
    Ok(table)
}

fn fileman_read_dir(
    path: &Path,
    pattern: Option<&str>,
    only_dirs: bool,
    return_path_too: bool,
) -> mlua::Result<Vec<String>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)
        .map_err(mlua::Error::external)?
        .filter_map(Result::ok)
    {
        let entry_path = entry.path();
        if only_dirs && !entry_path.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().into_string().ok() else {
            continue;
        };
        if pattern.is_some_and(|pattern| !wildcard_matches(pattern, &name)) {
            continue;
        }
        entries.push(if return_path_too {
            file_path_string(entry_path.as_path())
        } else {
            name
        });
    }
    Ok(entries)
}

fn fileman_entry_name(path: &Path, return_path_too: bool) -> String {
    if return_path_too {
        return file_path_string(path);
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

fn wildcard_matches(pattern: &str, text: &str) -> bool {
    let pattern = pattern.as_bytes();
    let text = text.as_bytes();
    let (mut pattern_idx, mut text_idx) = (0, 0);
    let (mut star_idx, mut star_text_idx) = (None, 0);
    while text_idx < text.len() {
        if pattern_idx < pattern.len()
            && (pattern[pattern_idx] == b'?' || pattern[pattern_idx] == text[text_idx])
        {
            pattern_idx += 1;
            text_idx += 1;
        } else if pattern_idx < pattern.len() && pattern[pattern_idx] == b'*' {
            star_idx = Some(pattern_idx);
            star_text_idx = text_idx;
            pattern_idx += 1;
        } else if let Some(star) = star_idx {
            pattern_idx = star + 1;
            star_text_idx += 1;
            text_idx = star_text_idx;
        } else {
            return false;
        }
    }
    pattern[pattern_idx..].iter().all(|byte| *byte == b'*')
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
            find_compat_files(lua, &find_files_song_dir, &args)
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
    globals.set("IniFile", create_ini_file_table(lua)?)?;
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
            Ok(serde_json::to_string(&lua_to_json_value(value, 0)).unwrap_or_default())
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

fn create_cryptman_table(lua: &Lua) -> mlua::Result<Table> {
    let cryptman = lua.create_table()?;
    for name in ["SHA1File", "SHA1String"] {
        cryptman.set(
            name,
            lua.create_function(|lua, _args: MultiValue| {
                Ok(Value::String(lua.create_string(&[0_u8; 20])?))
            })?,
        )?;
    }
    for name in ["SHA256File", "SHA256String"] {
        cryptman.set(
            name,
            lua.create_function(|lua, _args: MultiValue| {
                Ok(Value::String(lua.create_string(&[0_u8; 32])?))
            })?,
        )?;
    }
    cryptman.set(
        "GenerateRandomUUID",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(
                lua.create_string("00000000-0000-4000-8000-000000000000")?,
            ))
        })?,
    )?;
    cryptman.set(
        "SignFileToFile",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(false)
        })?,
    )?;
    Ok(cryptman)
}

fn create_network_table(lua: &Lua) -> mlua::Result<Table> {
    let network = lua.create_table()?;
    network.set(
        "IsUrlAllowed",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    network.set(
        "HttpRequest",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            create_network_response_table(lua)
        })?,
    )?;
    network.set(
        "WebSocket",
        lua.create_function(|lua, _args: MultiValue| create_websocket_table(lua))?,
    )?;
    network.set(
        "EncodeQueryParameters",
        lua.create_function(|_, args: MultiValue| {
            let Some(Value::Table(query)) = method_arg(&args, 0).cloned() else {
                return Ok(String::new());
            };
            encode_query_params(query)
        })?,
    )?;
    Ok(network)
}

fn create_split_table(lua: &Lua, text: &str, separator: &str) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    if separator.is_empty() {
        if text.is_empty() {
            table.raw_set(1, "")?;
        } else {
            for (idx, value) in text.chars().enumerate() {
                table.raw_set(idx + 1, value.to_string())?;
            }
        }
        return Ok(table);
    }
    for (idx, value) in text.split(separator).enumerate() {
        table.raw_set(idx + 1, value)?;
    }
    Ok(table)
}

fn path_basename(text: &str) -> &str {
    let trimmed = text.trim_end_matches(|c| c == '/' || c == '\\');
    trimmed
        .rsplit(|c| c == '/' || c == '\\')
        .next()
        .unwrap_or_default()
}

fn create_range_table(lua: &Lua, args: &MultiValue) -> mlua::Result<Value> {
    let Some(mut start) = args.front().cloned().and_then(read_f32) else {
        return Ok(Value::Nil);
    };
    let stop = if let Some(stop) = args.get(1).cloned().and_then(read_f32) {
        stop
    } else {
        let stop = start;
        start = 1.0;
        stop
    };
    let mut step = args.get(2).cloned().and_then(read_f32).unwrap_or(1.0);
    if step.abs() <= f32::EPSILON {
        return Ok(Value::Table(lua.create_table()?));
    }
    if step > 0.0 && start > stop {
        step = -step;
    }
    if step < 0.0 && start < stop {
        return Ok(Value::Table(lua.create_table()?));
    }

    let table = lua.create_table()?;
    let mut index = 1;
    let mut value = start;
    while (step > 0.0 && value <= stop + f32::EPSILON)
        || (step < 0.0 && value >= stop - f32::EPSILON)
    {
        table.raw_set(index, lua_number_value(value))?;
        index += 1;
        value += step;
        if index > 10_000 {
            break;
        }
    }
    Ok(Value::Table(table))
}

fn lua_number_value(value: f32) -> Value {
    if value.is_finite() && value.fract().abs() <= f32::EPSILON {
        Value::Integer(value as i64)
    } else {
        Value::Number(value.into())
    }
}

fn stringify_lua_table(lua: &Lua, args: &MultiValue) -> mlua::Result<Value> {
    let Some(Value::Table(table)) = args.front().cloned() else {
        return Ok(Value::Nil);
    };
    let form = args.get(1).cloned().and_then(read_string);
    let format = lua
        .globals()
        .get::<Table>("string")?
        .get::<Function>("format")?;
    let out = lua.create_table()?;
    for (idx, value) in table.sequence_values::<Value>().enumerate() {
        let value = value?;
        let text = if let Some(form) = form.as_deref()
            && matches!(value, Value::Integer(_) | Value::Number(_))
        {
            let mut call_args = MultiValue::new();
            call_args.push_back(Value::String(lua.create_string(form)?));
            call_args.push_back(value);
            lua_text_value(format.call::<Value>(call_args)?)?
        } else {
            lua_text_value(value)?
        };
        out.raw_set(idx + 1, text)?;
    }
    Ok(Value::Table(out))
}

fn map_lua_table(lua: &Lua, function: &Function, table: &Table) -> mlua::Result<Table> {
    let out = lua.create_table()?;
    for (idx, value) in table.sequence_values::<Value>().enumerate() {
        out.raw_set(idx + 1, function.call::<Value>(value?)?)?;
    }
    Ok(out)
}

fn deduplicate_lua_table(lua: &Lua, table: &Table) -> mlua::Result<Table> {
    let out = lua.create_table()?;
    let mut seen = Vec::new();
    let mut out_index = 1;
    for value in table.sequence_values::<Value>() {
        let value = value?;
        if seen.iter().any(|seen| lua_values_equal(seen, &value)) {
            continue;
        }
        seen.push(value.clone());
        out.raw_set(out_index, value)?;
        out_index += 1;
    }
    Ok(out)
}

fn rotate_lua_table(lua: &Lua, args: &MultiValue, left: bool) -> mlua::Result<Table> {
    let Some(Value::Table(input)) = args.front() else {
        return lua.create_table();
    };
    let len = input.raw_len();
    if len == 0 {
        return lua.create_table();
    }
    let shift = args.get(1).cloned().and_then(read_i32_value).unwrap_or(1) as i64;
    let len_i64 = len as i64;
    let out = lua.create_table()?;
    for index in 1..=len_i64 {
        let source = if left {
            (index + shift - 1).rem_euclid(len_i64) + 1
        } else {
            (index - shift - 1).rem_euclid(len_i64) + 1
        };
        out.raw_set(index, input.raw_get::<Value>(source)?)?;
    }
    Ok(out)
}

fn find_compat_files(lua: &Lua, song_dir: &Path, args: &MultiValue) -> mlua::Result<Table> {
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
        .trim_start_matches('.')
        .to_ascii_lowercase();
    let path = resolve_compat_path(song_dir, &dir);
    let mut files = if path.is_dir() {
        fs::read_dir(path)
            .map_err(mlua::Error::external)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.eq_ignore_ascii_case(&extension))
            })
            .map(|path| file_path_string(path.as_path()))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    files.sort_unstable();
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

fn actor_util_class_registered(name: &str) -> bool {
    matches!(
        name,
        "Actor"
            | "ActorFrame"
            | "Sprite"
            | "Banner"
            | "ActorMultiVertex"
            | "Sound"
            | "BitmapText"
            | "RollingNumbers"
            | "GraphDisplay"
            | "SongMeterDisplay"
            | "CourseContentsList"
            | "DeviceList"
            | "InputList"
            | "Model"
            | "Quad"
            | "ActorProxy"
            | "ActorFrameTexture"
    )
}

fn actor_util_file_type(path: &str) -> &'static str {
    let path = Path::new(path);
    if path.is_dir() {
        return "FileType_Directory";
    }
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp") => "FileType_Bitmap",
        Some("mp4" | "avi" | "mov" | "mkv" | "webm" | "mpeg" | "mpg") => "FileType_Movie",
        Some("ogg" | "oga" | "mp3" | "wav" | "flac" | "opus") => "FileType_Sound",
        Some("lua") => "FileType_Lua",
        Some("xml" | "ini" | "txt" | "json" | "ssc" | "sm") => "FileType_Text",
        _ => "FileType_Unknown",
    }
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

fn lua_table_to_string(args: &MultiValue) -> String {
    let name = args
        .get(1)
        .cloned()
        .and_then(read_string)
        .unwrap_or_else(|| "Value".to_string());
    match args.front() {
        Some(Value::Table(table)) if table.raw_len() == 0 => format!("{name} = {{}}"),
        Some(Value::Table(table)) => format!("{name} = {{...{} item(s)}}", table.raw_len()),
        Some(value) => format!(
            "{name} = {}",
            lua_text_value(value.clone()).unwrap_or_default()
        ),
        None => format!("{name} = nil"),
    }
}

fn create_version_parts_table(lua: &Lua, version: &str) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (index, part) in version_parts(version).into_iter().enumerate() {
        table.raw_set(index + 1, part)?;
    }
    Ok(table)
}

fn version_parts(version: &str) -> [i64; 3] {
    let mut parts = [0_i64; 3];
    for (index, part) in version.trim().split('.').take(3).enumerate() {
        let digits = part
            .bytes()
            .take_while(|byte| byte.is_ascii_digit())
            .collect::<Vec<_>>();
        parts[index] = std::str::from_utf8(&digits)
            .ok()
            .and_then(|digits| digits.parse::<i64>().ok())
            .unwrap_or(0);
    }
    parts
}

fn version_args(args: &MultiValue) -> Vec<i64> {
    if let Some(Value::String(version)) = args.front() {
        if let Ok(version) = version.to_str() {
            return version_parts(version.as_ref()).into_iter().collect();
        }
    }
    args.iter()
        .filter_map(|value| read_f32(value.clone()))
        .map(|value| value.round() as i64)
        .collect()
}

fn is_product_version(args: &MultiValue) -> bool {
    let expected = version_args(args);
    if expected.is_empty() {
        return false;
    }
    let product = version_parts(SONG_LUA_PRODUCT_VERSION);
    expected
        .into_iter()
        .enumerate()
        .all(|(index, value)| product.get(index).is_some_and(|part| *part == value))
}

fn is_minimum_product_version(args: &MultiValue) -> bool {
    let expected = version_args(args);
    if expected.is_empty() {
        return true;
    }
    let product = version_parts(SONG_LUA_PRODUCT_VERSION);
    for (index, expected) in expected.into_iter().enumerate() {
        let product = product.get(index).copied().unwrap_or(0);
        if product != expected {
            return product > expected;
        }
    }
    true
}

fn create_author_table(lua: &Lua, steps: Option<&Value>) -> mlua::Result<Table> {
    let out = lua.create_table()?;
    let Some(Value::Table(steps)) = steps else {
        return Ok(out);
    };
    let mut values = Vec::new();
    for method in ["GetDescription", "GetAuthorCredit", "GetChartName"] {
        if let Some(text) = call_string_method(steps, method)? {
            if !text.is_empty() && !values.iter().any(|value| value == &text) {
                values.push(text);
            }
        }
    }
    for (index, value) in values.into_iter().enumerate() {
        out.raw_set(index + 1, value)?;
    }
    Ok(out)
}

fn call_string_method(table: &Table, name: &str) -> mlua::Result<Option<String>> {
    let Some(function) = table.get::<Option<Function>>(name)? else {
        return Ok(None);
    };
    let mut args = MultiValue::new();
    args.push_back(Value::Table(table.clone()));
    Ok(Some(lua_text_value(function.call::<Value>(args)?)?))
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

fn create_background_filter_values(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("Off", 0)?;
    table.set("Dark", 50)?;
    table.set("Darker", 75)?;
    table.set("Darkest", 95)?;
    table.raw_set(0, 0)?;
    table.raw_set(50, 50)?;
    table.raw_set(75, 75)?;
    table.raw_set(95, 95)?;
    Ok(table)
}

pub(super) fn create_gameplay_layout(
    lua: &Lua,
    screen_center_y: f32,
    reverse: bool,
) -> mlua::Result<Table> {
    let combo_y = screen_center_y + if reverse { -30.0 } else { 30.0 };
    let judgment_y = screen_center_y + if reverse { 30.0 } else { -30.0 };
    let sign = if reverse { -1.0 } else { 1.0 };
    let table = lua.create_table()?;
    table.set("Combo", create_layout_slot(lua, combo_y, None)?)?;
    table.set("ErrorBar", create_layout_slot(lua, judgment_y, Some(30.0))?)?;
    table.set(
        "MeasureCounter",
        create_layout_slot(lua, judgment_y + sign * 28.0, None)?,
    )?;
    table.set(
        "SubtractiveScoring",
        create_layout_slot(lua, judgment_y - sign * 28.0, None)?,
    )?;
    Ok(table)
}

fn create_layout_slot(lua: &Lua, y: f32, max_height: Option<f32>) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("y", y)?;
    if let Some(max_height) = max_height {
        table.set("maxHeight", max_height)?;
    }
    Ok(table)
}

fn create_credits_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("Credits", 0)?;
    table.set("Remainder", 0)?;
    table.set("CoinsPerCredit", 1)?;
    Ok(table)
}

fn create_index_array(lua: &Lua, len: usize) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for index in 1..=len {
        table.raw_set(index, index)?;
    }
    Ok(table)
}

fn strip_sprite_hints(filename: &str) -> String {
    let mut text = filename.replace(" (doubleres)", "");
    if text
        .as_bytes()
        .get(text.len().saturating_sub(4)..)
        .is_some_and(|suffix| suffix.eq_ignore_ascii_case(b".png"))
    {
        text.truncate(text.len() - 4);
    }
    if let Some(space) = text.rfind(' ')
        && frame_hint(&text[space + 1..])
    {
        text.truncate(space);
    }
    text
}

fn frame_hint(text: &str) -> bool {
    let Some((wide, tall)) = text.split_once('x') else {
        return false;
    };
    !wide.is_empty()
        && !tall.is_empty()
        && wide.bytes().all(|byte| byte.is_ascii_digit())
        && tall.bytes().all(|byte| byte.is_ascii_digit())
}

fn create_network_response_table(lua: &Lua) -> mlua::Result<Table> {
    let response = lua.create_table()?;
    response.set("status", 0)?;
    response.set("code", 0)?;
    response.set("body", "")?;
    response.set("error", "offline")?;
    response.set("headers", lua.create_table()?)?;
    response.set(
        "IsFinished",
        lua.create_function(|_, _args: MultiValue| Ok(true))?,
    )?;
    let response_for_get = response.clone();
    response.set(
        "GetResponse",
        lua.create_function(move |_, _args: MultiValue| Ok(response_for_get.clone()))?,
    )?;
    response.set(
        "Cancel",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    Ok(response)
}

fn create_websocket_table(lua: &Lua) -> mlua::Result<Table> {
    let websocket = lua.create_table()?;
    websocket.set("is_open", false)?;
    websocket.set(
        "IsOpen",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    for name in ["Send", "Close"] {
        websocket.set(
            name,
            lua.create_function(|lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(())
            })?,
        )?;
    }
    Ok(websocket)
}

fn create_ini_file_table(lua: &Lua) -> mlua::Result<Table> {
    let ini = lua.create_table()?;
    ini.set(
        "ReadFile",
        lua.create_function(|lua, args: MultiValue| {
            let path = method_arg(&args, 0)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let table = lua.create_table()?;
            if path.ends_with("ThemeInfo.ini") {
                let info = lua.create_table()?;
                info.set("DisplayName", SONG_LUA_THEME_NAME)?;
                info.set("Version", SONG_LUA_PRODUCT_VERSION)?;
                info.set("Author", "DeadSync song-Lua compat")?;
                table.set("ThemeInfo", info)?;
            }
            Ok(table)
        })?,
    )?;
    ini.set(
        "WriteFile",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(true)
        })?,
    )?;
    Ok(ini)
}

fn create_rage_file_util_table(lua: &Lua) -> mlua::Result<Table> {
    let util = lua.create_table()?;
    util.set(
        "CreateRageFile",
        lua.create_function(|lua, _args: MultiValue| create_rage_file_table(lua))?,
    )?;
    Ok(util)
}

fn create_rage_file_table(lua: &Lua) -> mlua::Result<Table> {
    let file = lua.create_table()?;
    file.set(
        "Open",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(true)
        })?,
    )?;
    file.set(
        "Write",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(true)
        })?,
    )?;
    file.set(
        "Read",
        lua.create_function(|_, _args: MultiValue| Ok(String::new()))?,
    )?;
    for name in ["Close", "destroy"] {
        file.set(
            name,
            lua.create_function(|lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(())
            })?,
        )?;
    }
    Ok(file)
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

fn json_to_lua_value(lua: &Lua, value: serde_json::Value) -> mlua::Result<Value> {
    Ok(match value {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(value) => Value::Boolean(value),
        serde_json::Value::Number(value) => value
            .as_i64()
            .map(Value::Integer)
            .or_else(|| value.as_f64().map(Value::Number))
            .unwrap_or(Value::Nil),
        serde_json::Value::String(value) => Value::String(lua.create_string(value)?),
        serde_json::Value::Array(values) => {
            let table = lua.create_table()?;
            for (index, value) in values.into_iter().enumerate() {
                table.raw_set(index + 1, json_to_lua_value(lua, value)?)?;
            }
            Value::Table(table)
        }
        serde_json::Value::Object(values) => {
            let table = lua.create_table()?;
            for (key, value) in values {
                table.set(key, json_to_lua_value(lua, value)?)?;
            }
            Value::Table(table)
        }
    })
}

fn lua_to_json_value(value: Value, depth: usize) -> serde_json::Value {
    if depth >= 16 {
        return serde_json::Value::Null;
    }
    match value {
        Value::Nil => serde_json::Value::Null,
        Value::Boolean(value) => serde_json::Value::Bool(value),
        Value::Integer(value) => serde_json::Value::Number(value.into()),
        Value::Number(value) => serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String(value) => serde_json::Value::String(value.to_string_lossy()),
        Value::Table(table) => lua_table_to_json_value(table, depth + 1),
        _ => serde_json::Value::Null,
    }
}

fn lua_table_to_json_value(table: Table, depth: usize) -> serde_json::Value {
    let len = table.raw_len();
    let mut array = vec![serde_json::Value::Null; len];
    let mut object = serde_json::Map::new();
    let mut is_array = len > 0;
    for pair in table.pairs::<Value, Value>() {
        let Ok((key, value)) = pair else {
            continue;
        };
        let json_value = lua_to_json_value(value, depth);
        match key {
            Value::Integer(index) if is_array && index >= 1 && index as usize <= len => {
                array[index as usize - 1] = json_value;
            }
            Value::String(key) => {
                is_array = false;
                object.insert(key.to_string_lossy(), json_value);
            }
            Value::Integer(index) => {
                is_array = false;
                object.insert(index.to_string(), json_value);
            }
            Value::Number(index) => {
                is_array = false;
                object.insert(index.to_string(), json_value);
            }
            Value::Boolean(key) => {
                is_array = false;
                object.insert(key.to_string(), json_value);
            }
            _ => {}
        }
    }
    if is_array {
        serde_json::Value::Array(array)
    } else {
        serde_json::Value::Object(object)
    }
}

fn lua_binary_to_hex(value: Value) -> String {
    let Value::String(text) = value else {
        return String::new();
    };
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes.iter().copied() {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn encode_query_params(query: Table) -> mlua::Result<String> {
    let mut parts = Vec::new();
    for pair in query.pairs::<Value, Value>() {
        let (key, value) = pair?;
        let Some(key) = query_value_text(key) else {
            continue;
        };
        let value = query_value_text(value).unwrap_or_default();
        parts.push(format!(
            "{}={}",
            url_encode_component(&key),
            url_encode_component(&value)
        ));
    }
    parts.sort_unstable();
    Ok(parts.join("&"))
}

fn query_value_text(value: Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.to_string_lossy()),
        Value::Integer(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::Boolean(value) => Some(value.to_string()),
        _ => None,
    }
}

fn url_encode_component(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::new();
    for byte in text.as_bytes().iter().copied() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    out
}

fn timing_window_arg_index(value: Value) -> Option<i32> {
    read_i32_value(value.clone()).or_else(|| {
        let text = read_string(value)?;
        let (_, suffix) = text.rsplit_once('W')?;
        suffix.parse::<i32>().ok()
    })
}

fn timing_window_seconds(index: i32, mode: &str, tenms: bool) -> f32 {
    if mode.eq_ignore_ascii_case("FA+") && tenms && index == 1 {
        return 0.0085 + 0.0015;
    }
    let windows = if mode.eq_ignore_ascii_case("FA+") {
        [0.0135, 0.0215, 0.043, 0.135, 0.18]
    } else {
        [0.0215, 0.043, 0.102, 0.135, 0.18]
    };
    let index = index.clamp(1, windows.len() as i32) as usize - 1;
    windows[index] + 0.0015
}

fn worst_judgment_from_offsets(value: Value) -> i32 {
    let Value::Table(offsets) = value else {
        return 1;
    };
    let mut worst = 1;
    for pair in offsets.sequence_values::<Value>() {
        let Ok(value) = pair else {
            continue;
        };
        for offset in timing_offsets_from_value(value) {
            let abs = offset.abs();
            let judgment = (1..=5)
                .find(|window| abs <= timing_window_seconds(*window, "", false))
                .unwrap_or(5);
            worst = worst.max(judgment);
        }
    }
    worst
}

fn timing_offsets_from_value(value: Value) -> Vec<f32> {
    if let Some(offset) = read_f32(value.clone()) {
        return vec![offset];
    }
    let Value::Table(table) = value else {
        return Vec::new();
    };
    let mut offsets = Vec::new();
    if let Ok(value) = table.raw_get::<Value>(2) {
        if let Some(offset) = read_f32(value) {
            offsets.push(offset);
        }
    }
    if matches!(table.raw_get::<Value>(6), Ok(value) if truthy(&value)) {
        if let Ok(value) = table.raw_get::<Value>(7) {
            if let Some(offset) = read_f32(value) {
                offsets.push(offset);
            }
        }
    }
    offsets
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

fn create_ex_judgment_counts(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for name in [
        "W0",
        "W1",
        "W2",
        "W3",
        "W4",
        "W5",
        "Miss",
        "Hands",
        "Holds",
        "Mines",
        "Rolls",
        "totalHands",
        "totalHolds",
        "totalMines",
        "totalRolls",
    ] {
        table.set(name, 0_i64)?;
    }
    Ok(table)
}
