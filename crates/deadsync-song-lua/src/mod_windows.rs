use mlua::{Table, Value};

use crate::{
    SongLuaModWindow, SongLuaSpanMode, SongLuaTimeUnit, read_f32, read_player, read_span_mode,
    read_string,
};

pub fn read_mod_windows(
    table: Option<Table>,
    unit: SongLuaTimeUnit,
) -> Result<Vec<SongLuaModWindow>, String> {
    let Some(table) = table else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for value in table.sequence_values::<Value>() {
        let Value::Table(entry) = value.map_err(|err| err.to_string())? else {
            continue;
        };
        let Some(start) = read_f32(entry.raw_get::<Value>(1).map_err(|err| err.to_string())?)
        else {
            continue;
        };
        let Some(limit) = read_f32(entry.raw_get::<Value>(2).map_err(|err| err.to_string())?)
        else {
            continue;
        };
        let Some(mods) = read_string(entry.raw_get::<Value>(3).map_err(|err| err.to_string())?)
        else {
            continue;
        };
        let span_mode = read_span_mode(entry.raw_get::<Value>(4).map_err(|err| err.to_string())?)
            .unwrap_or(SongLuaSpanMode::Len);
        let player = read_player(entry.raw_get::<Value>(5).map_err(|err| err.to_string())?);
        out.push(SongLuaModWindow {
            unit,
            start,
            limit,
            span_mode,
            mods,
            player,
        });
    }
    Ok(out)
}
