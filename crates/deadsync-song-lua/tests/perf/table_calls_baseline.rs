//! Frozen from 56a0f78eb48cb5af19d2b33667585fcdbcb79ffb; only function visibility and formatting differ.

use super::*;

pub(super) fn call_string_method(table: &Table, name: &str) -> mlua::Result<Option<String>> {
    let Some(function) = table.get::<Option<Function>>(name)? else {
        return Ok(None);
    };
    let mut args = MultiValue::new();
    args.push_back(Value::Table(table.clone()));
    Ok(Some(lua_text_value(function.call::<Value>(args)?)?))
}

pub(super) fn call_table_function(table: &Table, function: &Function) -> mlua::Result<Value> {
    let mut args = MultiValue::new();
    args.push_back(Value::Table(table.clone()));
    function.call(args)
}

pub(super) fn display_bpms_from_table(table: &Table) -> mlua::Result<[f32; 2]> {
    if let Some(bpms) = table
        .get::<Option<Function>>("GetDisplayBpms")?
        .map(|function| call_table_function(table, &function))
        .transpose()?
        .and_then(read_bpms_table)
        && bpms[0] > 0.0
        && bpms[1] > 0.0
    {
        return Ok(bpms);
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

pub(super) fn create_author_table(lua: &Lua, steps: Option<&Value>) -> mlua::Result<Table> {
    let out = lua.create_table()?;
    let Some(Value::Table(steps)) = steps else {
        return Ok(out);
    };
    let mut values = Vec::new();
    for method in ["GetDescription", "GetAuthorCredit", "GetChartName"] {
        if let Some(text) = call_string_method(steps, method)?
            && !text.is_empty()
            && !values.iter().any(|value| value == &text)
        {
            values.push(text);
        }
    }
    for (index, value) in values.into_iter().enumerate() {
        out.raw_set(index + 1, value)?;
    }
    Ok(out)
}
