use mlua::{Lua, MultiValue, Table, Value};

use crate::{
    SONG_LUA_BROADCASTS_KEY, SONG_LUA_RUNTIME_BEAT_KEY, SONG_LUA_RUNTIME_BPS_KEY,
    SONG_LUA_RUNTIME_DELTA_BEAT_KEY, SONG_LUA_RUNTIME_DELTA_SECONDS_KEY, SONG_LUA_RUNTIME_KEY,
    SONG_LUA_RUNTIME_RATE_KEY, SONG_LUA_RUNTIME_SECONDS_KEY, SONG_LUA_SIDE_EFFECT_COUNT_KEY,
    SongLuaCompileContext, song_display_bps, song_elapsed_seconds_for_beat, song_music_rate,
};

pub fn create_song_runtime_table(
    lua: &Lua,
    context: &SongLuaCompileContext,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(SONG_LUA_RUNTIME_BEAT_KEY, 0_i64)?;
    table.set(SONG_LUA_RUNTIME_SECONDS_KEY, 0_i64)?;
    table.set(SONG_LUA_RUNTIME_DELTA_BEAT_KEY, 0_i64)?;
    table.set(SONG_LUA_RUNTIME_DELTA_SECONDS_KEY, 0_i64)?;
    table.set(SONG_LUA_RUNTIME_BPS_KEY, song_display_bps(context))?;
    table.set(SONG_LUA_RUNTIME_RATE_KEY, song_music_rate(context))?;
    Ok(table)
}

pub fn create_song_position_table(lua: &Lua, song_runtime: &Table) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for method in ["GetSongBeat", "GetSongBeatVisible"] {
        table.set(
            method,
            lua.create_function({
                let song_runtime = song_runtime.clone();
                move |_, _args: MultiValue| {
                    song_lua_runtime_number(song_runtime.get::<f32>(SONG_LUA_RUNTIME_BEAT_KEY)?)
                }
            })?,
        )?;
    }
    for method in ["GetMusicSeconds", "GetMusicSecondsVisible"] {
        table.set(
            method,
            lua.create_function({
                let song_runtime = song_runtime.clone();
                move |_, _args: MultiValue| {
                    song_lua_runtime_number(song_runtime.get::<f32>(SONG_LUA_RUNTIME_SECONDS_KEY)?)
                }
            })?,
        )?;
    }
    table.set(
        "GetCurBPS",
        lua.create_function({
            let song_runtime = song_runtime.clone();
            move |_, _args: MultiValue| Ok(song_runtime.get::<f32>(SONG_LUA_RUNTIME_BPS_KEY)?)
        })?,
    )?;
    Ok(table)
}

pub fn song_lua_runtime_number(value: f32) -> mlua::Result<Value> {
    if value.is_finite() && value.fract().abs() <= f32::EPSILON {
        Ok(Value::Integer(value as i64))
    } else {
        Ok(Value::Number(value as f64))
    }
}

fn compile_song_runtime_table(lua: &Lua) -> mlua::Result<Table> {
    lua.globals().get(SONG_LUA_RUNTIME_KEY)
}

pub fn song_lua_side_effect_count(lua: &Lua) -> mlua::Result<i64> {
    Ok(lua
        .globals()
        .get::<Option<i64>>(SONG_LUA_SIDE_EFFECT_COUNT_KEY)?
        .unwrap_or(0))
}

pub fn note_song_lua_side_effect(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    let count = song_lua_side_effect_count(lua)?;
    globals.set(SONG_LUA_SIDE_EFFECT_COUNT_KEY, count.saturating_add(1))
}

pub fn record_song_lua_broadcast(lua: &Lua, message: &str, has_params: bool) -> mlua::Result<()> {
    let globals = lua.globals();
    let Some(broadcasts) = globals.get::<Option<Table>>(SONG_LUA_BROADCASTS_KEY)? else {
        return Ok(());
    };
    let entry = lua.create_table()?;
    entry.set("message", message)?;
    entry.set("has_params", has_params)?;
    broadcasts.raw_set(broadcasts.raw_len() + 1, entry)?;
    Ok(())
}

pub fn read_song_lua_broadcasts(table: &Table) -> mlua::Result<Vec<(String, bool)>> {
    let mut out = Vec::new();
    for entry in table.sequence_values::<Table>() {
        let entry = entry?;
        let Some(message) = entry.get::<Option<String>>("message")? else {
            continue;
        };
        out.push((
            message,
            entry.get::<Option<bool>>("has_params")?.unwrap_or(false),
        ));
    }
    Ok(out)
}

pub fn compile_song_runtime_values(lua: &Lua) -> mlua::Result<(f32, f32)> {
    let runtime = compile_song_runtime_table(lua)?;
    Ok((
        runtime.get(SONG_LUA_RUNTIME_BEAT_KEY)?,
        runtime.get(SONG_LUA_RUNTIME_SECONDS_KEY)?,
    ))
}

pub fn set_compile_song_runtime_values(lua: &Lua, beat: f32, seconds: f32) -> mlua::Result<()> {
    let runtime = compile_song_runtime_table(lua)?;
    runtime.set(SONG_LUA_RUNTIME_BEAT_KEY, beat)?;
    runtime.set(SONG_LUA_RUNTIME_SECONDS_KEY, seconds)?;
    Ok(())
}

pub fn compile_song_runtime_delta_values(lua: &Lua) -> mlua::Result<(f32, f32)> {
    let runtime = compile_song_runtime_table(lua)?;
    Ok((
        runtime.get(SONG_LUA_RUNTIME_DELTA_BEAT_KEY)?,
        runtime.get(SONG_LUA_RUNTIME_DELTA_SECONDS_KEY)?,
    ))
}

pub fn set_compile_song_runtime_delta_values(
    lua: &Lua,
    delta_beat: f32,
    delta_seconds: f32,
) -> mlua::Result<()> {
    let runtime = compile_song_runtime_table(lua)?;
    runtime.set(SONG_LUA_RUNTIME_DELTA_BEAT_KEY, delta_beat)?;
    runtime.set(SONG_LUA_RUNTIME_DELTA_SECONDS_KEY, delta_seconds)?;
    Ok(())
}

pub fn set_compile_song_runtime_beat(lua: &Lua, beat: f32) -> mlua::Result<()> {
    let runtime = compile_song_runtime_table(lua)?;
    let song_bps = runtime
        .get::<Option<f32>>(SONG_LUA_RUNTIME_BPS_KEY)?
        .unwrap_or(1.0);
    let music_rate = runtime
        .get::<Option<f32>>(SONG_LUA_RUNTIME_RATE_KEY)?
        .unwrap_or(1.0);
    set_compile_song_runtime_values(
        lua,
        beat,
        song_elapsed_seconds_for_beat(beat, song_bps, music_rate),
    )
}
