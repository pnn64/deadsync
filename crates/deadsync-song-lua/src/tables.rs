use std::path::Path;

use mlua::{Function, Lua, MultiValue, Table, Value};

use crate::files::file_path_string;
use crate::lua_util::{lua_text_value, method_arg};
use crate::runtime::note_song_lua_side_effect;
use crate::values::{lua_values_equal, player_number_name, read_f32, read_i32_value, read_string};
use crate::version::version_parts;
use crate::{LUA_PLAYERS, SongLuaDifficulty};

pub fn lua_table_to_string(args: &MultiValue) -> String {
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

pub fn create_version_parts_table(lua: &Lua, version: &str) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (index, part) in version_parts(version).into_iter().enumerate() {
        table.raw_set(index + 1, part)?;
    }
    Ok(table)
}

pub fn create_display_bpms_table(lua: &Lua, bpms: [f32; 2]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set(1, bpms[0])?;
    table.raw_set(2, bpms[1])?;
    Ok(table)
}

pub fn create_radar_values_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetValue",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    Ok(table)
}

pub fn create_song_group_table(lua: &Lua) -> mlua::Result<Table> {
    let group = lua.create_table()?;
    group.set(
        "GetSyncOffset",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    Ok(group)
}

pub fn create_single_value_array(lua: &Lua, value: Table) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set(1, value)?;
    Ok(table)
}

pub fn set_string_method(lua: &Lua, table: &Table, name: &str, value: &str) -> mlua::Result<()> {
    let value = value.to_string();
    table.set(
        name,
        lua.create_function(move |lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(&value)?))
        })?,
    )
}

pub fn create_difficulty_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let reverse = lua.create_table()?;
    for (idx, difficulty) in [
        SongLuaDifficulty::Beginner,
        SongLuaDifficulty::Easy,
        SongLuaDifficulty::Medium,
        SongLuaDifficulty::Hard,
        SongLuaDifficulty::Challenge,
        SongLuaDifficulty::Edit,
    ]
    .into_iter()
    .enumerate()
    {
        table.raw_set(idx + 1, difficulty.sm_name())?;
        reverse.raw_set(difficulty.sm_name(), idx)?;
    }
    table.set(
        "Reverse",
        lua.create_function(move |_, _args: MultiValue| Ok(reverse.clone()))?,
    )?;
    Ok(table)
}

pub fn create_string_enum_table(lua: &Lua, names: &[&str]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let reverse = lua.create_table()?;
    for (idx, name) in names.iter().enumerate() {
        table.raw_set(idx + 1, *name)?;
        reverse.raw_set(*name, idx)?;
    }
    table.set(
        "Reverse",
        lua.create_function(move |_, _args: MultiValue| Ok(reverse.clone()))?,
    )?;
    Ok(table)
}

pub fn create_player_number_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let reverse = lua.create_table()?;
    for player in 0..LUA_PLAYERS {
        let name = player_number_name(player);
        table.raw_set(player + 1, name)?;
        table.raw_set(name, name)?;
        reverse.raw_set(name, player)?;
    }
    table.set(
        "Reverse",
        lua.create_function(move |_, _args: MultiValue| Ok(reverse.clone()))?,
    )?;
    Ok(table)
}

pub fn create_other_player_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set(player_number_name(0), player_number_name(1))?;
    table.raw_set(player_number_name(1), player_number_name(0))?;
    Ok(table)
}

pub fn create_screen_system_layer_helpers_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    set_string_method(lua, &table, "GetCreditsMessage", "Free Play")?;
    Ok(table)
}

pub fn create_branch_table(lua: &Lua) -> mlua::Result<Table> {
    let branch = lua.create_table()?;
    for (name, screen) in [
        ("AfterScreenRankingDouble", "ScreenRainbow"),
        ("TitleMenu", "ScreenTitleMenu"),
        ("AfterInit", "ScreenTitleMenu"),
        ("AllowScreenSelectProfile", "ScreenSelectProfile"),
        ("AfterSelectProfile", "ScreenSelectColor"),
        ("AllowScreenSelectColor", "ScreenSelectColor"),
        ("AfterScreenSelectColor", "ScreenSelectStyle"),
        ("AllowScreenSelectPlayMode", "ScreenSelectPlayMode"),
        ("AllowScreenSelectPlayMode2", "ScreenProfileLoad"),
        ("AfterSelectPlayMode", "ScreenSelectMusic"),
        ("AfterEvaluationStage", "ScreenProfileSave"),
        ("AfterGameplay", "ScreenEvaluationStage"),
        ("AfterHeartEntry", "ScreenEvaluationStage"),
        ("AfterSelectMusic", "ScreenGameplay"),
        ("SSMCancel", "ScreenTitleMenu"),
        ("AllowScreenNameEntry", "ScreenNameEntryTraditional"),
        ("AllowScreenEvalSummary", "ScreenEvaluationSummary"),
        ("AfterProfileSave", "ScreenSelectMusic"),
        ("AfterProfileSaveSummary", "ScreenGameOver"),
        ("GameplayScreen", "ScreenGameplay"),
    ] {
        branch.set(
            name,
            lua.create_function(move |lua, _args: MultiValue| {
                Ok(Value::String(lua.create_string(screen)?))
            })?,
        )?;
    }
    Ok(branch)
}

pub fn scale_value(value: f32, from_low: f32, from_high: f32, to_low: f32, to_high: f32) -> f32 {
    let span = from_high - from_low;
    if span.abs() <= f32::EPSILON {
        to_low
    } else {
        (value - from_low) / span * (to_high - to_low) + to_low
    }
}

fn seconds_to_time_parts(seconds: f64) -> (i64, i64, i64) {
    let minutes = (seconds / 60.0).trunc() as i64;
    let secs = (seconds % 60.0).trunc() as i64;
    let centis = ((seconds - (minutes * 60 + secs) as f64) * 100.0).trunc() as i64;
    let centis = centis.clamp(0, 99);
    (minutes, secs, centis)
}

pub fn seconds_to_hhmmss(seconds: f64) -> String {
    let (minutes, seconds, _) = seconds_to_time_parts(seconds);
    format!("{:02}:{:02}:{seconds:02}", minutes / 60, minutes % 60)
}

pub fn seconds_to_mss(seconds: f64) -> String {
    let (minutes, seconds, _) = seconds_to_time_parts(seconds);
    format!("{minutes:01}:{seconds:02}")
}

pub fn seconds_to_mmss(seconds: f64) -> String {
    let (minutes, seconds, _) = seconds_to_time_parts(seconds);
    format!("{minutes:02}:{seconds:02}")
}

pub fn seconds_to_mss_ms_ms(seconds: f64) -> String {
    let (minutes, seconds, centis) = seconds_to_time_parts(seconds);
    format!("{minutes:01}:{seconds:02}.{centis:02}")
}

pub fn seconds_to_mmss_ms_ms(seconds: f64) -> String {
    let (minutes, seconds, centis) = seconds_to_time_parts(seconds);
    format!("{minutes:02}:{seconds:02}.{centis:02}")
}

pub fn format_number_and_suffix(value: i64) -> String {
    let suffix = if (value % 100) / 10 == 1 {
        "th"
    } else {
        match value % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        }
    };
    format!("{value}{suffix}")
}

pub fn create_game_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    set_string_method(lua, &table, "GetName", "dance")?;
    Ok(table)
}

pub fn create_screen_table(
    lua: &Lua,
    width: f32,
    height: f32,
    center_x: f32,
    center_y: f32,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("w", width)?;
    table.set("h", height)?;
    table.set("cx", center_x)?;
    table.set("cy", center_y)?;
    table.set("l", 0.0_f32)?;
    table.set("t", 0.0_f32)?;
    table.set("r", width)?;
    table.set("b", height)?;
    Ok(table)
}

pub fn set_path_methods(
    lua: &Lua,
    table: &Table,
    path_name: &str,
    has_name: &str,
    path: Option<&Path>,
) -> mlua::Result<()> {
    let path = path.map(file_path_string).unwrap_or_default();
    set_string_method(lua, table, path_name, &path)?;
    let has_file = !path.is_empty();
    table.set(
        has_name,
        lua.create_function(move |_, _args: MultiValue| Ok(has_file))?,
    )?;
    Ok(())
}

pub fn create_timing_table(lua: &Lua, bpms: [f32; 2]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let actual_bpms = create_display_bpms_table(lua, bpms)?;
    table.set(
        "GetActualBPM",
        lua.create_function(move |_, _args: MultiValue| Ok(actual_bpms.clone()))?,
    )?;
    let timing_bpms = create_timing_bpms_table(lua, bpms)?;
    table.set(
        "GetBPMs",
        lua.create_function(move |_, _args: MultiValue| Ok(timing_bpms.clone()))?,
    )?;
    let has_bpm_changes = (bpms[0] - bpms[1]).abs() > f32::EPSILON;
    table.set(
        "HasBPMChanges",
        lua.create_function(move |_, _args: MultiValue| Ok(has_bpm_changes))?,
    )?;
    let bpm_at_beat = bpms[0].max(0.0);
    table.set(
        "GetBPMAtBeat",
        lua.create_function(move |_, _args: MultiValue| Ok(bpm_at_beat))?,
    )?;
    table.set(
        "GetElapsedTimeFromBeat",
        lua.create_function(|_, args: MultiValue| {
            Ok(method_arg(&args, 0)
                .cloned()
                .and_then(read_f32)
                .unwrap_or(0.0))
        })?,
    )?;
    table.set(
        "GetBeatFromElapsedTime",
        lua.create_function(|_, args: MultiValue| {
            Ok(method_arg(&args, 0)
                .cloned()
                .and_then(read_f32)
                .unwrap_or(0.0))
        })?,
    )?;
    Ok(table)
}

pub fn create_style_table(lua: &Lua, style_name: &str) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let style = crate::song_lua_style_info(style_name);
    let style_name_for_get = style.name.to_string();
    table.set(
        "GetName",
        lua.create_function(move |lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(&style_name_for_get)?))
        })?,
    )?;
    set_string_method(lua, &table, "GetStepsType", style.steps_type)?;
    set_string_method(lua, &table, "GetStyleType", style.style_type)?;
    table.set(
        "ColumnsPerPlayer",
        lua.create_function(move |_, _args: MultiValue| Ok(style.columns as i64))?,
    )?;
    table.set(
        "GetWidth",
        lua.create_function(move |_, _args: MultiValue| Ok(style.width))?,
    )?;
    table.set(
        "GetColumnInfo",
        lua.create_function(move |lua, args: MultiValue| {
            let index = method_arg(&args, 1)
                .cloned()
                .and_then(read_i32_value)
                .unwrap_or(1)
                .clamp(1, style.columns as i32) as usize
                - 1;
            let info = lua.create_table()?;
            info.set("Name", crate::song_lua_style_column_name(index))?;
            info.set("Track", index as i64)?;
            info.set("XOffset", crate::song_lua_style_column_x(style.name, index))?;
            Ok(info)
        })?,
    )?;
    Ok(table)
}

pub fn display_bpms_for_args(
    args: &MultiValue,
    fallback: [f32; 2],
    default_rate: f32,
) -> mlua::Result<([f32; 2], f32)> {
    let rate = args
        .get(2)
        .cloned()
        .and_then(read_f32)
        .unwrap_or(default_rate)
        .max(f32::EPSILON);
    let bpms = args
        .get(1)
        .and_then(display_bpms_from_value)
        .transpose()?
        .unwrap_or(fallback);
    Ok(([bpms[0] * rate, bpms[1] * rate], rate))
}

pub fn create_split_table(lua: &Lua, text: &str, separator: &str) -> mlua::Result<Table> {
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

pub fn create_range_table(lua: &Lua, args: &MultiValue) -> mlua::Result<Value> {
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

pub fn stringify_lua_table(lua: &Lua, args: &MultiValue) -> mlua::Result<Value> {
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

pub fn map_lua_table(lua: &Lua, function: &Function, table: &Table) -> mlua::Result<Table> {
    let out = lua.create_table()?;
    for (idx, value) in table.sequence_values::<Value>().enumerate() {
        out.raw_set(idx + 1, function.call::<Value>(value?)?)?;
    }
    Ok(out)
}

pub fn deduplicate_lua_table(lua: &Lua, table: &Table) -> mlua::Result<Table> {
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

pub fn rotate_lua_table(lua: &Lua, args: &MultiValue, left: bool) -> mlua::Result<Table> {
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

pub fn create_background_filter_values(lua: &Lua) -> mlua::Result<Table> {
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

pub fn create_gameplay_layout(
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

pub fn create_credits_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("Credits", 0)?;
    table.set("Remainder", 0)?;
    table.set("CoinsPerCredit", 1)?;
    Ok(table)
}

pub fn create_index_array(lua: &Lua, len: usize) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for index in 1..=len {
        table.raw_set(index, index)?;
    }
    Ok(table)
}

pub fn create_author_table(lua: &Lua, steps: Option<&Value>) -> mlua::Result<Table> {
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

pub fn create_ex_judgment_counts(lua: &Lua) -> mlua::Result<Table> {
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

pub fn create_network_response_table(lua: &Lua) -> mlua::Result<Table> {
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

pub fn create_websocket_table(lua: &Lua) -> mlua::Result<Table> {
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

pub fn create_ini_file_table(
    lua: &Lua,
    theme_name: &'static str,
    product_version: &'static str,
) -> mlua::Result<Table> {
    let ini = lua.create_table()?;
    ini.set(
        "ReadFile",
        lua.create_function(move |lua, args: MultiValue| {
            let path = crate::method_arg(&args, 0)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let table = lua.create_table()?;
            if path.ends_with("ThemeInfo.ini") {
                let info = lua.create_table()?;
                info.set("DisplayName", theme_name)?;
                info.set("Version", product_version)?;
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

pub fn create_rage_file_util_table(lua: &Lua) -> mlua::Result<Table> {
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

fn create_layout_slot(lua: &Lua, y: f32, max_height: Option<f32>) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("y", y)?;
    if let Some(max_height) = max_height {
        table.set("maxHeight", max_height)?;
    }
    Ok(table)
}

fn create_timing_bpms_table(lua: &Lua, bpms: [f32; 2]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set(1, bpms[0].max(0.0))?;
    if (bpms[0] - bpms[1]).abs() > f32::EPSILON {
        table.raw_set(2, bpms[1].max(0.0))?;
    }
    Ok(table)
}

fn call_string_method(table: &Table, name: &str) -> mlua::Result<Option<String>> {
    let Some(function) = table.get::<Option<Function>>(name)? else {
        return Ok(None);
    };
    let mut args = MultiValue::new();
    args.push_back(Value::Table(table.clone()));
    Ok(Some(lua_text_value(function.call::<Value>(args)?)?))
}

fn display_bpms_from_value(value: &Value) -> Option<mlua::Result<[f32; 2]>> {
    let Value::Table(table) = value else {
        return None;
    };
    Some(display_bpms_from_table(table))
}

fn display_bpms_from_table(table: &Table) -> mlua::Result<[f32; 2]> {
    if let Some(bpms) = table
        .get::<Option<Function>>("GetDisplayBpms")?
        .map(|function| call_table_function(table, &function))
        .transpose()?
        .and_then(read_bpms_table)
    {
        if bpms[0] > 0.0 && bpms[1] > 0.0 {
            return Ok(bpms);
        }
    }
    let Some(timing) = table
        .get::<Option<Function>>("GetTimingData")?
        .map(|function| call_table_function(table, &function))
        .transpose()?
        .and_then(|value| match value {
            Value::Table(table) => Some(table),
            _ => None,
        })
    else {
        return Ok([0.0, 0.0]);
    };
    Ok(timing
        .get::<Option<Function>>("GetActualBPM")?
        .map(|function| call_table_function(&timing, &function))
        .transpose()?
        .and_then(read_bpms_table)
        .unwrap_or([0.0, 0.0]))
}

fn call_table_function(table: &Table, function: &Function) -> mlua::Result<Value> {
    let mut args = MultiValue::new();
    args.push_back(Value::Table(table.clone()));
    function.call(args)
}

fn read_bpms_table(value: Value) -> Option<[f32; 2]> {
    let Value::Table(table) = value else {
        return None;
    };
    Some([
        table.raw_get::<Value>(1).ok().and_then(read_f32)?,
        table.raw_get::<Value>(2).ok().and_then(read_f32)?,
    ])
}

fn lua_number_value(value: f32) -> Value {
    if value.is_finite() && value.fract().abs() <= f32::EPSILON {
        Value::Integer(value as i64)
    } else {
        Value::Number(value.into())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use mlua::{Lua, MultiValue, Table, Value};

    use super::{
        create_author_table, create_display_bpms_table, create_ex_judgment_counts,
        create_gameplay_layout, create_radar_values_table, create_range_table,
        create_single_value_array, create_song_group_table, create_split_table, create_style_table,
        create_timing_table, display_bpms_for_args, lua_table_to_string, rotate_lua_table,
        set_path_methods, set_string_method,
    };

    #[test]
    fn split_table_handles_empty_separator() {
        let lua = Lua::new();
        let table = create_split_table(&lua, "abc", "").unwrap();

        assert_eq!(table.raw_get::<String>(1).unwrap(), "a");
        assert_eq!(table.raw_get::<String>(2).unwrap(), "b");
        assert_eq!(table.raw_get::<String>(3).unwrap(), "c");
    }

    #[test]
    fn range_table_counts_down_when_needed() {
        let lua = Lua::new();
        let mut args = MultiValue::new();
        args.push_back(Value::Integer(3));
        args.push_back(Value::Integer(1));

        let Value::Table(table) = create_range_table(&lua, &args).unwrap() else {
            panic!("range should return a table");
        };
        assert_eq!(table.raw_get::<i64>(1).unwrap(), 3);
        assert_eq!(table.raw_get::<i64>(2).unwrap(), 2);
        assert_eq!(table.raw_get::<i64>(3).unwrap(), 1);
    }

    #[test]
    fn rotate_lua_table_wraps_indices() {
        let lua = Lua::new();
        let table = lua.create_table().unwrap();
        table.raw_set(1, "a").unwrap();
        table.raw_set(2, "b").unwrap();
        table.raw_set(3, "c").unwrap();

        let mut args = MultiValue::new();
        args.push_back(Value::Table(table));
        args.push_back(Value::Integer(1));

        let rotated = rotate_lua_table(&lua, &args, false).unwrap();
        assert_eq!(rotated.raw_get::<String>(1).unwrap(), "c");
        assert_eq!(rotated.raw_get::<String>(2).unwrap(), "a");
        assert_eq!(rotated.raw_get::<String>(3).unwrap(), "b");
    }

    #[test]
    fn lua_table_to_string_reports_sequence_length() {
        let lua = Lua::new();
        let table = lua.create_table().unwrap();
        table.raw_set(1, "a").unwrap();
        table.raw_set(2, "b").unwrap();
        let mut args = MultiValue::new();
        args.push_back(Value::Table(table));
        args.push_back(Value::String(lua.create_string("Rows").unwrap()));

        assert_eq!(lua_table_to_string(&args), "Rows = {...2 item(s)}");
    }

    #[test]
    fn gameplay_layout_places_reverse_slots() {
        let lua = Lua::new();
        let layout = create_gameplay_layout(&lua, 240.0, true).unwrap();
        let combo = layout.get::<Table>("Combo").unwrap();
        let error_bar = layout.get::<Table>("ErrorBar").unwrap();

        assert_eq!(combo.get::<f32>("y").unwrap(), 210.0);
        assert_eq!(error_bar.get::<f32>("y").unwrap(), 270.0);
        assert_eq!(error_bar.get::<f32>("maxHeight").unwrap(), 30.0);
    }

    #[test]
    fn author_table_deduplicates_step_methods() {
        let lua = Lua::new();
        let steps = lua.create_table().unwrap();
        for method in ["GetDescription", "GetAuthorCredit", "GetChartName"] {
            steps
                .set(
                    method,
                    lua.create_function(|_, _: Value| Ok("author")).unwrap(),
                )
                .unwrap();
        }

        let authors = create_author_table(&lua, Some(&Value::Table(steps))).unwrap();
        assert_eq!(authors.raw_len(), 1);
        assert_eq!(authors.raw_get::<String>(1).unwrap(), "author");
    }

    #[test]
    fn ex_judgment_counts_defaults_to_zero() {
        let lua = Lua::new();
        let counts = create_ex_judgment_counts(&lua).unwrap();

        assert_eq!(counts.get::<i64>("W1").unwrap(), 0);
        assert_eq!(counts.get::<i64>("Miss").unwrap(), 0);
        assert_eq!(counts.get::<i64>("totalRolls").unwrap(), 0);
    }

    #[test]
    fn radar_values_default_to_zero() {
        let lua = Lua::new();
        let radar = create_radar_values_table(&lua).unwrap();
        let get_value = radar.get::<mlua::Function>("GetValue").unwrap();

        assert_eq!(get_value.call::<f32>(MultiValue::new()).unwrap(), 0.0);
    }

    #[test]
    fn song_group_table_has_zero_sync_offset() {
        let lua = Lua::new();
        let group = create_song_group_table(&lua).unwrap();
        let get_sync_offset = group.get::<mlua::Function>("GetSyncOffset").unwrap();

        assert_eq!(get_sync_offset.call::<f32>(MultiValue::new()).unwrap(), 0.0);
    }

    #[test]
    fn single_value_array_wraps_table() {
        let lua = Lua::new();
        let value = lua.create_table().unwrap();
        value.set("name", "song").unwrap();

        let array = create_single_value_array(&lua, value).unwrap();
        assert_eq!(array.raw_len(), 1);
        assert_eq!(
            array
                .raw_get::<Table>(1)
                .unwrap()
                .get::<String>("name")
                .unwrap(),
            "song"
        );
    }

    #[test]
    fn string_method_returns_fixed_value() {
        let lua = Lua::new();
        let table = lua.create_table().unwrap();
        set_string_method(&lua, &table, "GetName", "single").unwrap();
        let get_name = table.get::<mlua::Function>("GetName").unwrap();

        assert_eq!(
            get_name.call::<String>(MultiValue::new()).unwrap(),
            "single"
        );
    }

    #[test]
    fn path_methods_report_path_presence() {
        let lua = Lua::new();
        let table = lua.create_table().unwrap();
        set_path_methods(
            &lua,
            &table,
            "GetPath",
            "HasPath",
            Some(Path::new("songs\\pack\\song.ogg")),
        )
        .unwrap();
        let get_path = table.get::<mlua::Function>("GetPath").unwrap();
        let has_path = table.get::<mlua::Function>("HasPath").unwrap();

        assert_eq!(
            get_path.call::<String>(MultiValue::new()).unwrap(),
            "songs/pack/song.ogg"
        );
        assert!(has_path.call::<bool>(MultiValue::new()).unwrap());
    }

    #[test]
    fn timing_table_exposes_bpm_policy() {
        let lua = Lua::new();
        let timing = create_timing_table(&lua, [120.0, 180.0]).unwrap();
        let get_bpms = timing.get::<mlua::Function>("GetBPMs").unwrap();
        let has_bpm_changes = timing.get::<mlua::Function>("HasBPMChanges").unwrap();
        let bpms = get_bpms.call::<Table>(MultiValue::new()).unwrap();

        assert_eq!(bpms.raw_get::<f32>(1).unwrap(), 120.0);
        assert_eq!(bpms.raw_get::<f32>(2).unwrap(), 180.0);
        assert!(has_bpm_changes.call::<bool>(MultiValue::new()).unwrap());
    }

    #[test]
    fn style_table_exposes_column_info() {
        let lua = Lua::new();
        let style = create_style_table(&lua, "double").unwrap();
        let columns = style.get::<mlua::Function>("ColumnsPerPlayer").unwrap();
        let get_column_info = style.get::<mlua::Function>("GetColumnInfo").unwrap();
        let mut args = MultiValue::new();
        args.push_back(Value::Table(style));
        args.push_back(Value::Integer(1));

        let info = get_column_info.call::<Table>(args).unwrap();
        assert_eq!(columns.call::<i64>(MultiValue::new()).unwrap(), 8);
        assert_eq!(info.get::<String>("Name").unwrap(), "Left");
        assert_eq!(info.get::<i64>("Track").unwrap(), 0);
    }

    #[test]
    fn display_bpms_for_args_uses_table_and_rate() {
        let lua = Lua::new();
        let bpms = create_display_bpms_table(&lua, [100.0, 200.0]).unwrap();
        let steps = lua.create_table().unwrap();
        steps
            .set(
                "GetDisplayBpms",
                lua.create_function(move |_, _: Value| Ok(bpms.clone()))
                    .unwrap(),
            )
            .unwrap();
        let mut args = MultiValue::new();
        args.push_back(Value::Nil);
        args.push_back(Value::Table(steps));
        args.push_back(Value::Number(1.5));

        let (bpms, rate) = display_bpms_for_args(&args, [60.0, 60.0], 1.0).unwrap();
        assert_eq!(bpms, [150.0, 300.0]);
        assert_eq!(rate, 1.5);
    }
}
