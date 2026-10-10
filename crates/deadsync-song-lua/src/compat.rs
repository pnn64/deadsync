use mlua::{Function, Lua, MultiValue, Table, Value};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use crate::{
    SONG_LUA_NOTE_COLUMNS, SONG_LUA_PRODUCT_VERSION, SONG_LUA_THEME_NAME,
    SONG_LUA_THEME_PATH_PREFIX, create_actor_util_table, create_author_table,
    create_background_filter_values, create_color_constants_table, create_credits_table,
    create_cryptman_table, create_debug_table, create_ex_judgment_counts, create_fileman_table,
    create_find_files_function, create_index_array, create_ini_file_table, create_lua_compat_table,
    create_network_table, create_rage_file_util_table, create_range_table, create_split_table,
    create_string_array, create_version_parts_table, current_gamestate_player_value,
    current_gamestate_value, current_song_value, current_steps_value, deduplicate_lua_table,
    install_theme_color_helpers, json_to_lua_value, lua_binary_to_hex, lua_table_to_string,
    lua_to_json_value, make_color_table, map_lua_table, note_song_lua_side_effect,
    parse_chart_info, path_basename, read_color_call, read_color_value, read_f32, read_string,
    retarget_loader_env, rotate_lua_table, seconds_to_hhmmss, set_string_method,
    song_lua_is_minimum_product_version, song_lua_is_product_version, stringify_lua_table,
    strip_sprite_hints, timing_window_arg_index, timing_window_seconds, truthy,
    worst_judgment_from_offsets,
};

#[derive(Clone, Copy)]
pub struct SongLuaCompatCallbacks {
    pub current_gamestate_player_value: fn(&Lua, &str, usize) -> mlua::Result<Value>,
    pub current_gamestate_value: fn(&Lua, &str) -> mlua::Result<Value>,
    pub current_song_value: fn(&Lua) -> mlua::Result<Value>,
    pub current_steps_value: fn(&Lua, usize) -> mlua::Result<Value>,
    pub retarget_loader_env: fn(&Lua, &Function, &Table) -> mlua::Result<()>,
}

pub fn install_stdlib_compat(
    lua: &Lua,
    song_dir: &Path,
    callbacks: SongLuaCompatCallbacks,
) -> mlua::Result<()> {
    install_random_compat(lua)?;
    let globals = lua.globals();
    let table: Table = globals.get("table")?;
    table.set(
        "rotate_right",
        lua.create_function(|lua, args: MultiValue| rotate_lua_table(lua, &args, false))?,
    )?;
    table.set(
        "rotate_left",
        lua.create_function(|lua, args: MultiValue| rotate_lua_table(lua, &args, true))?,
    )?;
    let shuffle = lua
        .load(
            r#"
        return function(input)
            local result = {}
            for i = 1, #input do
                table.insert(result, math.random(i), input[i])
            end
            return result
        end
    "#,
        )
        .eval::<Function>()?;
    table.set("shuffle", shuffle.clone())?;
    globals.set("tableshuffle", shuffle)?;
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
        lua.create_function(move |lua, args: MultiValue| {
            let name = args
                .front()
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            if name == "GameCommand" {
                return Ok(Value::Table(create_game_command_table(lua, callbacks)?));
            }
            Ok(Value::String(lua.create_string(&name)?))
        })?,
    )?;
    globals.set("ActorUtil", create_actor_util_table(lua, song_dir)?)?;
    globals.set("findFiles", create_find_files_function(lua, song_dir)?)?;
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
        lua.create_function(|_, args: MultiValue| Ok(song_lua_is_product_version(&args)))?,
    )?;
    globals.set(
        "IsMinimumProductVersion",
        lua.create_function(|_, args: MultiValue| Ok(song_lua_is_minimum_product_version(&args)))?,
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
        // Native 01 base.lua uses normal indexing, accepts input before the
        // first lookup, and observes changes through the captured table.
        lua.load(
            "return function(t) local n = 0; return function() n = n + 1; return t[n] end end",
        )
        .eval::<mlua::Function>()?,
    )?;
    globals.set("Trace", lua.create_function(|_, _msg: String| Ok(()))?)?;
    // _fallback/Scripts/02 Utilities.lua: preserve actor and shared-name walks.
    lua.load(
        r#"
function rec_print_children(parent, indent)
    indent = indent or ""
    if #parent > 0 and type(parent) == "table" then
        for i, child in ipairs(parent) do rec_print_children(child, indent .. i .. "->") end
    elseif parent.GetChildren then
        local name = (parent.GetName and parent:GetName()) or ""
        Trace(indent .. name .. " children:")
        for key, child in pairs(parent:GetChildren()) do
            if #child > 0 then
                Trace(indent .. name .. "->" .. key .. " shared name:")
                rec_print_children(child, indent .. name .. "->")
                Trace(indent .. name .. "->" .. key .. " shared name over.")
            else rec_print_children(child, indent .. name .. "->") end
        end
        Trace(indent .. name .. " children over.")
    else
        local name = (parent.GetName and parent:GetName()) or ""
        Trace(indent .. name .. "(" .. tostring(parent) .. ")")
    end
end
"#,
    )
    .exec()?;
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
                    (b[0] - a[0]).mul_add(percent, a[0]),
                    (b[1] - a[1]).mul_add(percent, a[1]),
                    (b[2] - a[2]).mul_add(percent, a[2]),
                    (b[3] - a[3]).mul_add(percent, a[3]),
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
        lua.create_function(move |lua, (target, env): (Value, Table)| match target {
            Value::Function(function) => {
                (callbacks.retarget_loader_env)(lua, &function, &env)?;
                let _ = function.set_environment(env)?;
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
            let code = crate::preprocess_lua_cmd_syntax(&code).map_err(mlua::Error::external)?;
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

// ITGmania's _fallback/Scripts/00 init.lua replaces Lua's generator with
// RageUtil/RandomNumbers.cpp. Keep its MT19937 state local to this song VM;
// compilation and cached playback must not consume another song's stream.
#[derive(Clone)]
struct SongLuaRandom {
    words: [u32; 624],
    index: usize,
}

impl SongLuaRandom {
    fn seeded(seed: u32) -> Self {
        let mut words = [0; 624];
        words[0] = seed;
        for index in 1..words.len() {
            let previous = words[index - 1];
            words[index] = 1_812_433_253_u32
                .wrapping_mul(previous ^ (previous >> 30))
                .wrapping_add(index as u32);
        }
        Self { words, index: 624 }
    }

    fn next(&mut self) -> u32 {
        if self.index == self.words.len() {
            for index in 0..self.words.len() {
                let pair = (self.words[index] & 0x8000_0000)
                    | (self.words[(index + 1) % 624] & 0x7fff_ffff);
                self.words[index] = self.words[(index + 397) % 624]
                    ^ (pair >> 1)
                    ^ if pair % 2 == 0 { 0 } else { 0x9908_b0df };
            }
            self.index = 0;
        }
        let mut value = self.words[self.index];
        self.index += 1;
        value ^= value >> 11;
        value ^= (value << 7) & 0x9d2c_5680;
        value ^= (value << 15) & 0xefc6_0000;
        value ^ (value >> 18)
    }

    fn real(&mut self) -> f64 {
        let low = self.next();
        let high = self.next();
        // Match std::uniform_real_distribution<double> in the platform's
        // native C++ library, including MSVC's discarded low eleven bits.
        #[cfg(target_os = "windows")]
        {
            ((u64::from(low >> 11) + (u64::from(high) << 21)) as f64) / 9_007_199_254_740_992.0
        }
        #[cfg(not(target_os = "windows"))]
        {
            (f64::from(low) + f64::from(high) * 4_294_967_296.0) / 18_446_744_073_709_551_616.0
        }
    }

    fn integer(&mut self, lower: i32, upper: i32) -> f64 {
        let width = (i64::from(upper) - i64::from(lower) + 1) as u64;
        if width == 1_u64 << 32 {
            return (i64::from(lower) + i64::from(self.next())) as f64;
        }
        let width = width as u32;
        #[cfg(target_vendor = "apple")]
        let offset = {
            // libc++ uses independent_bits_engine and rejects above the bound.
            if width == 1 {
                return f64::from(lower);
            }
            let mask = u32::MAX >> width.wrapping_sub(1).leading_zeros();
            loop {
                let value = self.next() & mask;
                if value < width {
                    break value;
                }
            }
        };
        #[cfg(not(target_vendor = "apple"))]
        let offset = {
            // MSVC and libstdc++ use multiply-high with unbiased rejection.
            let threshold = width.wrapping_neg() % width;
            loop {
                let product = u64::from(self.next()) * u64::from(width);
                if product as u32 >= threshold {
                    break (product >> 32) as u32;
                }
            }
        };
        (i64::from(lower) + i64::from(offset)) as f64
    }
}

fn random_int_arg(lua: &Lua, args: &MultiValue, index: usize) -> mlua::Result<i32> {
    let value = lua
        .coerce_number(args.get(index).cloned().unwrap_or(Value::Nil))?
        .filter(|value| {
            value.is_finite()
                && value.trunc() >= i32::MIN as f64
                && value.trunc() <= i32::MAX as f64
        })
        .ok_or_else(|| {
            mlua::Error::RuntimeError(format!("bad argument #{} (number expected)", index + 1))
        })?;
    Ok(value as i32)
}

struct SongLuaRandomState(Rc<RefCell<SongLuaRandom>>);

pub(crate) struct RandomSnapshot {
    state: Rc<RefCell<SongLuaRandom>>,
    previous: SongLuaRandom,
}

impl Drop for RandomSnapshot {
    fn drop(&mut self) {
        *self.state.borrow_mut() = self.previous.clone();
    }
}

pub(crate) fn preserve_random(lua: &Lua) -> Option<RandomSnapshot> {
    let state = lua.app_data_ref::<SongLuaRandomState>()?;
    Some(RandomSnapshot {
        previous: state.0.borrow().clone(),
        state: Rc::clone(&state.0),
    })
}

fn install_random_compat(lua: &Lua) -> mlua::Result<()> {
    let state = Rc::new(RefCell::new(SongLuaRandom::seeded(1)));
    lua.set_app_data(SongLuaRandomState(Rc::clone(&state)));
    let seed_state = state.clone();
    let seed = lua.create_function(move |lua, args: MultiValue| {
        let seed = random_int_arg(lua, &args, 0)?;
        // Native MersenneTwister treats an explicit zero seed as wall time.
        let seed = if seed == 0 {
            chrono::Utc::now().timestamp() as u32
        } else {
            seed as u32
        };
        *seed_state.borrow_mut() = SongLuaRandom::seeded(seed);
        Ok(())
    })?;
    let random = lua.create_function(move |lua, args: MultiValue| match args.len() {
        0 => Ok(state.borrow_mut().real()),
        1 | 2 => {
            let (lower, upper) = if args.len() == 1 {
                (1, random_int_arg(lua, &args, 0)?)
            } else {
                (
                    random_int_arg(lua, &args, 0)?,
                    random_int_arg(lua, &args, 1)?,
                )
            };
            if lower > upper || (args.len() == 2 && lower == upper) {
                return Err(mlua::Error::RuntimeError("interval is empty".into()));
            }
            Ok(state.borrow_mut().integer(lower, upper))
        }
        _ => Err(mlua::Error::RuntimeError(
            "wrong number of arguments".into(),
        )),
    })?;
    let namespace = lua.create_table()?;
    namespace.set("Seed", seed.clone())?;
    namespace.set("Random", random.clone())?;
    lua.globals().set("MersenneTwister", namespace)?;
    let math: Table = lua.globals().get("math")?;
    math.set("randomseed", seed)?;
    math.set("random", random)
}

pub fn install_default_stdlib_compat(lua: &Lua, song_dir: &Path) -> mlua::Result<()> {
    install_stdlib_compat(
        lua,
        song_dir,
        SongLuaCompatCallbacks {
            current_gamestate_player_value,
            current_gamestate_value,
            current_song_value,
            current_steps_value,
            retarget_loader_env,
        },
    )
}

fn create_game_command_table(lua: &Lua, callbacks: SongLuaCompatCallbacks) -> mlua::Result<Table> {
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
        lua.create_function(move |lua, _args: MultiValue| {
            (callbacks.current_gamestate_value)(lua, "GetCurrentStyle")
        })?,
    )?;
    table.set(
        "GetSong",
        lua.create_function(move |lua, _args: MultiValue| (callbacks.current_song_value)(lua))?,
    )?;
    table.set(
        "GetSteps",
        lua.create_function(move |lua, _args: MultiValue| (callbacks.current_steps_value)(lua, 0))?,
    )?;
    table.set(
        "GetCourse",
        lua.create_function(move |lua, _args: MultiValue| {
            (callbacks.current_gamestate_value)(lua, "GetCurrentCourse")
        })?,
    )?;
    table.set(
        "GetTrail",
        lua.create_function(move |lua, _args: MultiValue| {
            (callbacks.current_gamestate_player_value)(lua, "GetCurrentTrail", 0)
        })?,
    )?;
    table.set(
        "GetCharacter",
        lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
    )?;
    table.set(
        "GetSongGroup",
        lua.create_function(move |lua, _args: MultiValue| {
            let Value::Table(song) = (callbacks.current_song_value)(lua)? else {
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
