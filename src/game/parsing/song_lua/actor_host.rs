use super::*;
use deadsync_song_lua::{
    add_actor_child_from_path as add_lua_actor_child_from_path,
    create_dummy_actor as create_lua_dummy_actor,
    create_named_child_actor as create_lua_named_child_actor,
    install_actor_methods as install_lua_actor_methods,
    note_field_column_actors as create_note_field_column_actors, read_actor_model_layers,
    read_noteskin_tap_actor_slots as read_lua_noteskin_tap_actor_slots,
};

pub(super) fn create_named_child_actor(
    lua: &Lua,
    parent: &Table,
    name: &str,
) -> mlua::Result<Table> {
    create_lua_named_child_actor(
        lua,
        parent,
        name,
        create_dummy_actor,
        create_named_child_actor,
    )
}

fn note_field_column_actors(lua: &Lua, note_field: &Table) -> mlua::Result<Table> {
    create_note_field_column_actors(lua, note_field, create_dummy_actor)
}

pub(super) fn read_model_layers(
    actor: &Table,
) -> Result<Option<Arc<[SongLuaOverlayModelLayer]>>, String> {
    read_actor_model_layers(
        actor,
        crate::game::parsing::noteskin::load_itg_model_slots_from_path,
        model_layer_from_slot,
    )
}

fn model_layer_from_slot(
    slot: &crate::game::parsing::noteskin::SpriteSlot,
) -> Option<SongLuaOverlayModelLayer> {
    let model = slot.model.as_ref()?;
    if model.vertices.is_empty() {
        return None;
    }
    let uv_rect = slot.uv_for_frame_at(0, 0.0);
    let (uv_scale, uv_offset, uv_tex_shift) = slot.model_uv_params(uv_rect);
    Some(SongLuaOverlayModelLayer {
        texture_key: slot.texture_key_shared(),
        vertices: crate::game::parsing::noteskin::build_model_geometry(slot),
        model_size: model.size(),
        uv_scale,
        uv_offset,
        uv_tex_shift,
        uv_velocity: slot.uv_velocity,
        uv_cycle_seconds: slot.uv_cycle_seconds,
        draw: song_lua_model_draw(slot.model_draw_at(0.0, 0.0)),
    })
}

pub(super) fn read_noteskin_tap_actor_slots(
    actor: &Table,
    _context: &SongLuaCompileContext,
) -> Result<Option<Arc<[crate::game::parsing::noteskin::SpriteSlot]>>, String> {
    read_lua_noteskin_tap_actor_slots(
        actor,
        crate::game::parsing::noteskin::load_itg_model_slots_from_path,
    )
}

fn song_lua_model_draw(draw: deadsync_noteskin::ModelDrawState) -> SongLuaOverlayModelDraw {
    SongLuaOverlayModelDraw::new(
        draw.pos,
        draw.rot,
        draw.zoom,
        draw.tint,
        draw.vert_align,
        draw.blend_add,
        draw.visible,
    )
}

pub(super) fn create_dummy_actor(lua: &Lua, actor_type: &'static str) -> mlua::Result<Table> {
    create_lua_dummy_actor(lua, actor_type, install_actor_methods)
}

pub(super) fn install_actor_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    install_lua_actor_methods(
        lua,
        actor,
        add_actor_child_from_path,
        note_field_column_actors,
        create_named_child_actor,
        create_dummy_actor,
    )
}

fn add_actor_child_from_path(lua: &Lua, actor: &Table, path: &str) -> mlua::Result<()> {
    add_lua_actor_child_from_path(lua, actor, path, create_dummy_actor)
}
