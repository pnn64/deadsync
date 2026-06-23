use mlua::{Function, Lua, MultiValue, Table, Value};

use crate::lua_util::lua_text_value;
use crate::runtime::note_song_lua_side_effect;
use crate::values::{lua_values_equal, read_f32, read_i32_value, read_string};
use crate::version::version_parts;

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

fn call_string_method(table: &Table, name: &str) -> mlua::Result<Option<String>> {
    let Some(function) = table.get::<Option<Function>>(name)? else {
        return Ok(None);
    };
    let mut args = MultiValue::new();
    args.push_back(Value::Table(table.clone()));
    Ok(Some(lua_text_value(function.call::<Value>(args)?)?))
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
    use mlua::{Lua, MultiValue, Table, Value};

    use super::{
        create_author_table, create_ex_judgment_counts, create_gameplay_layout, create_range_table,
        create_split_table, lua_table_to_string, rotate_lua_table,
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
}
