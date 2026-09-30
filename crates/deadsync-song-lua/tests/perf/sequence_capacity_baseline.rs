// Frozen functions from b27c63730 (0.5.1634).
use super::*;

pub(super) fn create_split_table(lua: &Lua, text: &str, separator: &str) -> mlua::Result<Table> {
    if separator.is_empty() {
        let table = lua.create_table_with_capacity(text.chars().count().max(1), 0)?;
        if text.is_empty() {
            table.raw_set(1, "")?;
        } else {
            let mut encoded = [0; 4];
            for (idx, value) in text.chars().enumerate() {
                table.raw_set(idx + 1, &*value.encode_utf8(&mut encoded))?;
            }
        }
        return Ok(table);
    }
    let table = lua.create_table()?;
    for (idx, value) in text.split(separator).enumerate() {
        table.raw_set(idx + 1, value)?;
    }
    Ok(table)
}

pub(super) fn create_range_table(lua: &Lua, args: &MultiValue) -> mlua::Result<Value> {
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
