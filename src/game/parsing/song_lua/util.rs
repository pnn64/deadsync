use mlua::{Function, Lua, MultiValue, Table, Value};
use std::collections::HashMap;
use std::ffi::c_void;
use std::fs;
use std::path::{Path, PathBuf};

use super::types::{SongLuaEaseWindow, SongLuaMessageEvent, SongLuaModWindow, SongLuaSpanMode};
use super::{LUA_PLAYERS, SONG_LUA_SOUND_PATHS_KEY};

#[inline(always)]
pub(super) fn read_f32(value: Value) -> Option<f32> {
    match value {
        Value::Integer(value) => Some(value as f32),
        Value::Number(value) => {
            let value = value as f32;
            value.is_finite().then_some(value)
        }
        Value::String(text) => text.to_str().ok()?.trim().parse::<f32>().ok(),
        _ => None,
    }
}

#[inline(always)]
pub(super) fn read_boolish(value: Value) -> Option<bool> {
    match value {
        Value::Boolean(value) => Some(value),
        Value::Integer(value) => Some(value != 0),
        Value::Number(value) => Some(value != 0.0),
        Value::String(text) => {
            let text = text.to_str().ok()?.trim().to_string();
            if text.eq_ignore_ascii_case("true")
                || text.eq_ignore_ascii_case("yes")
                || text.eq_ignore_ascii_case("on")
            {
                Some(true)
            } else if text.eq_ignore_ascii_case("false")
                || text.eq_ignore_ascii_case("no")
                || text.eq_ignore_ascii_case("off")
            {
                Some(false)
            } else {
                text.parse::<f32>().ok().map(|value| value != 0.0)
            }
        }
        _ => None,
    }
}

#[inline(always)]
pub(super) fn read_string(value: Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.to_str().ok()?.to_string()),
        _ => None,
    }
}

pub(super) fn read_song_lua_sound_paths(lua: &Lua) -> Result<Vec<PathBuf>, String> {
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

pub(super) fn lua_text_value(value: Value) -> mlua::Result<String> {
    match value {
        Value::String(text) => Ok(text.to_str()?.to_string()),
        Value::Integer(value) => Ok(value.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        Value::Boolean(value) => Ok(value.to_string()),
        _ => Ok(String::new()),
    }
}

pub(super) fn lua_format_text(lua: &Lua, args: &MultiValue) -> mlua::Result<String> {
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

#[inline(always)]
pub(super) fn make_color_table(lua: &Lua, rgba: [f32; 4]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set(1, rgba[0])?;
    table.raw_set(2, rgba[1])?;
    table.raw_set(3, rgba[2])?;
    table.raw_set(4, rgba[3])?;
    Ok(table)
}

pub(super) fn create_color_constants_table(lua: &Lua) -> mlua::Result<Table> {
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

pub(super) fn parse_color_text(text: &str) -> Option<[f32; 4]> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if let Some(hex) = text.strip_prefix('#') {
        if matches!(hex.len(), 6 | 8) {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
            let a = if hex.len() == 8 {
                u8::from_str_radix(&hex[6..8], 16).ok()? as f32 / 255.0
            } else {
                1.0
            };
            return Some([r, g, b, a]);
        }
    }
    let parts = text
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    match parts.as_slice() {
        [r, g, b] => Some([
            r.parse::<f32>().ok()?,
            g.parse::<f32>().ok()?,
            b.parse::<f32>().ok()?,
            1.0,
        ]),
        [r, g, b, a] => Some([
            r.parse::<f32>().ok()?,
            g.parse::<f32>().ok()?,
            b.parse::<f32>().ok()?,
            a.parse::<f32>().ok()?,
        ]),
        _ => None,
    }
}

#[inline(always)]
pub(super) fn read_color_value(value: Value) -> Option<[f32; 4]> {
    match value {
        Value::Table(table) => table_color(&table),
        Value::String(text) => Some(parse_color_text(&text.to_str().ok()?).unwrap_or([1.0; 4])),
        _ => None,
    }
}

pub(super) fn read_vertex_colors_value(value: Value) -> Option<[[f32; 4]; 4]> {
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

pub(super) fn read_color_call(args: &MultiValue) -> Option<[f32; 4]> {
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
pub(super) fn method_arg(args: &MultiValue, index: usize) -> Option<&Value> {
    let offset = method_arg_offset(args);
    args.get(offset + index)
}

#[inline(always)]
pub(super) fn method_arg_offset(args: &MultiValue) -> usize {
    usize::from(matches!(args.front(), Some(Value::Table(_))))
}

pub(super) fn song_group_name(song_dir: &Path) -> String {
    song_dir
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

pub(super) fn song_music_path(song_dir: &Path) -> Option<PathBuf> {
    song_named_file_path(
        song_dir,
        &["music", "song", "audio"],
        is_song_lua_audio_path,
    )
    .or_else(|| song_first_file_path(song_dir, is_song_lua_audio_path))
}

pub(super) fn song_named_image_path(song_dir: &Path, stems: &[&str]) -> Option<PathBuf> {
    song_named_file_path(song_dir, stems, is_song_lua_image_path)
}

pub(super) fn song_simfile_path(song_dir: &Path) -> Option<PathBuf> {
    song_first_file_path(song_dir, is_song_lua_simfile_path)
}

fn song_named_file_path(
    song_dir: &Path,
    stems: &[&str],
    predicate: fn(&Path) -> bool,
) -> Option<PathBuf> {
    let files = song_dir_files(song_dir);
    for stem in stems {
        if let Some(path) = files
            .iter()
            .find(|path| predicate(path) && path_stem_eq(path, stem))
        {
            return Some(path.clone());
        }
    }
    for stem in stems {
        if let Some(path) = files.iter().find(|path| {
            predicate(path)
                && path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.to_ascii_lowercase().contains(stem))
        }) {
            return Some(path.clone());
        }
    }
    None
}

fn song_first_file_path(song_dir: &Path, predicate: fn(&Path) -> bool) -> Option<PathBuf> {
    song_dir_files(song_dir)
        .into_iter()
        .find(|path| predicate(path))
}

fn song_dir_files(song_dir: &Path) -> Vec<PathBuf> {
    let mut files = fs::read_dir(song_dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    files.sort_by_key(|path| file_path_string(path));
    files
}

fn path_stem_eq(path: &Path, stem: &str) -> bool {
    path.file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(stem))
}

#[inline(always)]
pub(super) fn read_u32_value(value: Value) -> Option<u32> {
    match value {
        Value::Integer(value) if value >= 0 => u32::try_from(value).ok(),
        Value::Number(value) if value.is_finite() && value >= 0.0 && value.fract() == 0.0 => {
            u32::try_from(value as u64).ok()
        }
        _ => None,
    }
}

#[inline(always)]
pub(super) fn read_i32_value(value: Value) -> Option<i32> {
    match value {
        Value::Integer(value) => Some(value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32),
        Value::Number(value) if value.is_finite() => Some(
            value
                .round()
                .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32,
        ),
        Value::String(text) => text
            .to_str()
            .ok()?
            .trim()
            .parse::<i32>()
            .ok()
            .or_else(|| read_f32(Value::String(text)).map(|value| value.round() as i32)),
        _ => None,
    }
}

#[inline(always)]
pub(super) fn read_player(value: Value) -> Option<u8> {
    match value {
        Value::Integer(value) if (1..=2).contains(&value) => Some(value as u8),
        Value::Number(value) if (1.0..=2.0).contains(&value) => Some(value as u8),
        _ => None,
    }
}

#[inline(always)]
pub(super) fn read_span_mode(value: Value) -> Option<SongLuaSpanMode> {
    let text = read_string(value)?;
    if text.eq_ignore_ascii_case("len") {
        Some(SongLuaSpanMode::Len)
    } else if text.eq_ignore_ascii_case("end") {
        Some(SongLuaSpanMode::End)
    } else {
        None
    }
}

#[inline(always)]
pub(super) fn read_easing_name(
    value: Value,
    easing_names: &HashMap<*const c_void, String>,
) -> Option<String> {
    match value {
        Value::String(text) => Some(text.to_str().ok()?.to_string()),
        Value::Function(function) => easing_names.get(&function.to_pointer()).cloned(),
        _ => None,
    }
}

#[inline(always)]
pub(super) fn truthy(value: &Value) -> bool {
    !matches!(value, Value::Nil | Value::Boolean(false))
}

pub(super) fn lua_values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Nil, Value::Nil) => true,
        (Value::Boolean(left), Value::Boolean(right)) => left == right,
        (Value::Integer(left), Value::Integer(right)) => left == right,
        (Value::Integer(left), Value::Number(right)) => (*left as f64) == *right,
        (Value::Number(left), Value::Integer(right)) => *left == (*right as f64),
        (Value::Number(left), Value::Number(right)) => left == right,
        (Value::String(left), Value::String(right)) => left
            .to_str()
            .ok()
            .zip(right.to_str().ok())
            .is_some_and(|(left, right)| left == right),
        (Value::Table(left), Value::Table(right)) => left.to_pointer() == right.to_pointer(),
        _ => false,
    }
}

#[inline(always)]
pub(super) fn player_index_from_value(value: &Value) -> Option<usize> {
    match value {
        Value::Integer(value) => usize::try_from(*value).ok().filter(|v| *v < LUA_PLAYERS),
        Value::Number(value) => {
            if *value >= 0.0 && *value < LUA_PLAYERS as f64 {
                Some(*value as usize)
            } else {
                None
            }
        }
        Value::String(text) => match text.to_str().ok()?.as_ref() {
            "P1" => Some(0),
            "P2" => Some(1),
            "PlayerNumber_P1" => Some(0),
            "PlayerNumber_P2" => Some(1),
            _ => None,
        },
        _ => None,
    }
}

#[inline(always)]
pub(super) fn player_number_name(player: usize) -> &'static str {
    match player {
        0 => "PlayerNumber_P1",
        1 => "PlayerNumber_P2",
        _ => unreachable!("song lua only exposes two player numbers"),
    }
}

#[inline(always)]
pub(super) fn song_dir_string(path: &Path) -> String {
    let mut text = path.to_string_lossy().replace('\\', "/");
    if !text.ends_with('/') {
        text.push('/');
    }
    text
}

#[inline(always)]
pub(super) fn file_path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[inline(always)]
pub(super) fn is_song_lua_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp" | "qoi" | "tif" | "tiff"
            )
        })
}

#[inline(always)]
pub(super) fn is_song_lua_video_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "mp4" | "avi" | "webm" | "mov" | "mkv" | "mpg" | "mpeg" | "ogv"
            )
        })
}

#[inline(always)]
pub(super) fn is_song_lua_audio_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "ogg" | "mp3" | "wav" | "flac" | "opus" | "m4a" | "aac"
            )
        })
}

#[inline(always)]
pub(super) fn is_song_lua_simfile_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "sm" | "ssc"))
}

#[inline(always)]
pub(super) fn is_song_lua_media_path(path: &Path) -> bool {
    is_song_lua_image_path(path) || is_song_lua_video_path(path) || is_song_lua_audio_path(path)
}

#[inline(always)]
pub(super) fn mod_window_cmp(
    left: &SongLuaModWindow,
    right: &SongLuaModWindow,
) -> std::cmp::Ordering {
    left.start
        .total_cmp(&right.start)
        .then_with(|| left.limit.total_cmp(&right.limit))
        .then_with(|| left.mods.cmp(&right.mods))
}

#[inline(always)]
pub(super) fn ease_window_cmp(
    left: &SongLuaEaseWindow,
    right: &SongLuaEaseWindow,
) -> std::cmp::Ordering {
    left.start
        .total_cmp(&right.start)
        .then_with(|| left.limit.total_cmp(&right.limit))
}

#[inline(always)]
pub(super) fn message_event_cmp(
    left: &SongLuaMessageEvent,
    right: &SongLuaMessageEvent,
) -> std::cmp::Ordering {
    left.beat.total_cmp(&right.beat)
}
