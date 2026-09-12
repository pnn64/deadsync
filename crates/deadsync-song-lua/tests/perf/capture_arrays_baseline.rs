// Frozen from 4c7312013a6143518f0ea5088c046e56afb7c8a4; only test visibility/formatting differs.

use super::*;

#[inline(always)]
pub(super) fn make_color_table(lua: &Lua, rgba: [f32; 4]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set(1, rgba[0])?;
    table.raw_set(2, rgba[1])?;
    table.raw_set(3, rgba[2])?;
    table.raw_set(4, rgba[3])?;
    Ok(table)
}

pub(super) fn make_vertex_color_table(lua: &Lua, colors: [[f32; 4]; 4]) -> mlua::Result<Table> {
    let out = lua.create_table()?;
    for (index, color) in colors.into_iter().enumerate() {
        out.raw_set(index + 1, make_color_table(lua, color)?)?;
    }
    Ok(out)
}

pub(super) fn capture_block_set_color(
    lua: &Lua,
    actor: &Table,
    color: [f32; 4],
) -> mlua::Result<()> {
    let value = lua.create_table()?;
    value.raw_set(1, color[0])?;
    value.raw_set(2, color[1])?;
    value.raw_set(3, color[2])?;
    value.raw_set(4, color[3])?;
    if !record_overlay_update_capture(
        lua,
        actor,
        "diffuse",
        SongLuaOverlayUpdateValue::Vec4(color),
    ) {
        let block = actor_current_capture_block(lua, actor)?;
        block.set("diffuse", value.clone())?;
        block.set("__songlua_has_changes", true)?;
    }
    actor.set("__songlua_diffuse", value.clone())?;
    actor.set("__songlua_state_diffuse", value)?;
    Ok(())
}

pub(super) fn capture_block_set_vec2(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value2: [f32; 2],
) -> mlua::Result<()> {
    let value = lua.create_table()?;
    value.raw_set(1, value2[0])?;
    value.raw_set(2, value2[1])?;
    if !record_overlay_update_capture(lua, actor, key, SongLuaOverlayUpdateValue::Vec2(value2)) {
        let block = actor_current_capture_block(lua, actor)?;
        block.set(key, value.clone())?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, value)?;
    Ok(())
}

pub(super) fn capture_block_set_vec3(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value3: [f32; 3],
) -> mlua::Result<()> {
    let value = lua.create_table()?;
    value.raw_set(1, value3[0])?;
    value.raw_set(2, value3[1])?;
    value.raw_set(3, value3[2])?;
    if !record_overlay_update_capture(lua, actor, key, SongLuaOverlayUpdateValue::Vec3(value3)) {
        let block = actor_current_capture_block(lua, actor)?;
        block.set(key, value.clone())?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, value)?;
    Ok(())
}

pub(super) fn capture_block_set_vec4(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value4: [f32; 4],
) -> mlua::Result<()> {
    let value = lua.create_table()?;
    value.raw_set(1, value4[0])?;
    value.raw_set(2, value4[1])?;
    value.raw_set(3, value4[2])?;
    value.raw_set(4, value4[3])?;
    if !record_overlay_update_capture(lua, actor, key, SongLuaOverlayUpdateValue::Vec4(value4)) {
        let block = actor_current_capture_block(lua, actor)?;
        block.set(key, value.clone())?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, value)?;
    Ok(())
}

pub(super) fn capture_block_set_vec5(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value5: [f32; 5],
) -> mlua::Result<()> {
    let value = lua.create_table()?;
    value.raw_set(1, value5[0])?;
    value.raw_set(2, value5[1])?;
    value.raw_set(3, value5[2])?;
    value.raw_set(4, value5[3])?;
    value.raw_set(5, value5[4])?;
    if !record_overlay_update_capture(lua, actor, key, SongLuaOverlayUpdateValue::Vec5(value5)) {
        let block = actor_current_capture_block(lua, actor)?;
        block.set(key, value.clone())?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, value)?;
    Ok(())
}

pub(super) fn capture_block_set_size(lua: &Lua, actor: &Table, size: [f32; 2]) -> mlua::Result<()> {
    let value = lua.create_table()?;
    value.raw_set(1, size[0])?;
    value.raw_set(2, size[1])?;
    if !record_overlay_update_capture(lua, actor, "size", SongLuaOverlayUpdateValue::Vec2(size)) {
        let block = actor_current_capture_block(lua, actor)?;
        block.set("size", value.clone())?;
        block.set("__songlua_has_changes", true)?;
    }
    actor.set("__songlua_state_size", value)?;
    Ok(())
}

pub(super) fn capture_block_set_stretch(
    lua: &Lua,
    actor: &Table,
    rect: [f32; 4],
) -> mlua::Result<()> {
    capture_block_set_f32(lua, actor, "x", f32::midpoint(rect[0], rect[2]))?;
    capture_block_set_f32(lua, actor, "y", f32::midpoint(rect[1], rect[3]))?;
    let value = lua.create_table()?;
    value.raw_set(1, rect[0])?;
    value.raw_set(2, rect[1])?;
    value.raw_set(3, rect[2])?;
    value.raw_set(4, rect[3])?;
    if !record_overlay_update_capture(
        lua,
        actor,
        "stretch_rect",
        SongLuaOverlayUpdateValue::Vec4(rect),
    ) {
        let block = actor_current_capture_block(lua, actor)?;
        block.set("stretch_rect", value.clone())?;
        block.set("__songlua_has_changes", true)?;
    }
    actor.set("__songlua_state_stretch_rect", value)?;
    Ok(())
}

pub(super) fn capture_immediate_vec3(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value: [f32; 3],
) -> mlua::Result<()> {
    let table = lua.create_table()?;
    for (index, component) in value.into_iter().enumerate() {
        table.raw_set(index + 1, component)?;
    }
    if !record_overlay_update_capture_immediate(
        lua,
        actor,
        key,
        SongLuaOverlayUpdateValue::Vec3(value),
    ) {
        let block = actor_immediate_capture_block(lua, actor)?;
        block.set(key, table.clone())?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, table)
}

pub(super) fn capture_immediate_vec4(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value: [f32; 4],
) -> mlua::Result<()> {
    let table = lua.create_table()?;
    for (index, component) in value.into_iter().enumerate() {
        table.raw_set(index + 1, component)?;
    }
    if !record_overlay_update_capture_immediate(
        lua,
        actor,
        key,
        SongLuaOverlayUpdateValue::Vec4(value),
    ) {
        let block = actor_immediate_capture_block(lua, actor)?;
        block.set(key, table.clone())?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, table)
}

pub(super) fn capture_immediate_vec5(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value: [f32; 5],
) -> mlua::Result<()> {
    let table = lua.create_table()?;
    for (index, component) in value.into_iter().enumerate() {
        table.raw_set(index + 1, component)?;
    }
    if !record_overlay_update_capture_immediate(
        lua,
        actor,
        key,
        SongLuaOverlayUpdateValue::Vec5(value),
    ) {
        let block = actor_immediate_capture_block(lua, actor)?;
        block.set(key, table.clone())?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, table)
}
