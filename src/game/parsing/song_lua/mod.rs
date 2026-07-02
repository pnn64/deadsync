use mlua::{Lua, Table};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use deadlib_present::actors::TextAttribute;
use deadlib_render::TexturedMeshVertex;
use deadsync_noteskin::{NUM_QUANTIZATIONS, Style};
#[cfg(test)]
use deadsync_song_lua::SONG_LUA_STARTUP_MESSAGE;
use deadsync_song_lua::{
    compile_song_lua_with_actors, overlay_actor_tree_has_visual, overlay_model_layers_from_slots,
    song_lua_human_player_count, song_lua_style_info,
};

mod actor_host;
mod managers;

use self::actor_host::{
    create_dummy_actor, create_named_child_actor, install_actor_methods, read_model_layers,
    read_noteskin_tap_actor_slots,
};
use self::managers::song_lua_noteskin_resolver;
pub use deadsync_song_lua::{
    SONG_LUA_INITIAL_LIFE, SongLuaCapturedActor, SongLuaColumnOffsetWindow, SongLuaCompileContext,
    SongLuaCompileInfo, SongLuaDifficulty, SongLuaEaseTarget, SongLuaEaseWindow,
    SongLuaMessageEvent, SongLuaModWindow, SongLuaNoteHideWindow, SongLuaNoteskinResolver,
    SongLuaOverlayBlendMode, SongLuaOverlayCommandBlock, SongLuaOverlayEase,
    SongLuaOverlayMeshVertex, SongLuaOverlayMessageCommand, SongLuaOverlayModelDraw,
    SongLuaOverlayState, SongLuaOverlayStateDelta, SongLuaPlayerContext, SongLuaProxyTarget,
    SongLuaSpanMode, SongLuaSpeedMod, SongLuaTextGlowMode, SongLuaTimeUnit, THEME_RECEPTOR_Y_STD,
    file_path_string, overlay_state_after_blocks, parse_overlay_blend_mode,
    parse_overlay_effect_clock, parse_overlay_effect_mode, parse_overlay_text_align,
    parse_overlay_text_glow_mode,
};

pub type SongLuaOverlayModelLayer = deadsync_song_lua::SongLuaOverlayModelLayer<TexturedMeshVertex>;
pub type SongLuaOverlayKind = deadsync_song_lua::SongLuaOverlayKind<
    crate::game::parsing::noteskin::SpriteSlot,
    TexturedMeshVertex,
    TextAttribute,
>;
pub type SongLuaOverlayActor = deadsync_song_lua::SongLuaOverlayActor<SongLuaOverlayKind>;
pub type CompiledSongLua = deadsync_song_lua::CompiledSongLua<SongLuaOverlayActor>;
type OverlayCompileActor = deadsync_song_lua::SongLuaOverlayCompileActor<SongLuaOverlayKind>;

pub fn compile_song_lua(
    entry_path: &Path,
    context: &SongLuaCompileContext,
) -> Result<CompiledSongLua, String> {
    compile_song_lua_with_actors(
        entry_path,
        context,
        song_lua_noteskin_resolver(),
        create_dummy_actor,
        create_named_child_actor,
        install_actor_methods,
        read_model_layers,
        read_noteskin_tap_actor_slots,
        ensure_multitap_arrow_visual,
    )
}

fn ensure_multitap_arrow_visual(
    lua: &Lua,
    overlays: &mut Vec<OverlayCompileActor>,
    arrow_index: usize,
    context: &SongLuaCompileContext,
    noteskin: &str,
) -> Result<(), String> {
    if overlay_actor_tree_has_visual(overlays, arrow_index) {
        return Ok(());
    }
    let Some((kind, initial_state)) = multitap_arrow_visual_spec(noteskin, context) else {
        return Ok(());
    };
    overlays.push(OverlayCompileActor {
        table: create_dummy_actor(lua, "Model").map_err(|err| err.to_string())?,
        actor: SongLuaOverlayActor {
            kind,
            name: None,
            parent_index: Some(arrow_index),
            initial_state,
            message_commands: Vec::new(),
        },
    });
    Ok(())
}

fn multitap_arrow_visual_spec(
    noteskin: &str,
    context: &SongLuaCompileContext,
) -> Option<(SongLuaOverlayKind, SongLuaOverlayState)> {
    let style = Style {
        num_cols: song_lua_style_info(&context.style_name).columns,
        num_players: song_lua_human_player_count(context).max(1),
    };
    let ns = crate::game::parsing::noteskin::load_itg_skin_cached(&style, noteskin).ok()?;
    let down_col = 1.min(style.num_cols.saturating_sub(1));
    let note_idx = down_col * NUM_QUANTIZATIONS;
    let layers = ns.note_layers.get(note_idx)?;
    if let Some(model_layers) = multitap_arrow_model_layers(layers) {
        return Some((
            SongLuaOverlayKind::Model {
                layers: model_layers,
            },
            SongLuaOverlayState::default(),
        ));
    }
    let slot = layers.iter().find(|slot| slot.model.is_none())?;
    let texture_key = slot.texture_key_shared();
    let mut state = SongLuaOverlayState {
        custom_texture_rect: Some(slot.uv_for_frame_at(slot.frame_index_from_phase(0.0), 0.0)),
        size: Some(slot.logical_size()),
        rot_z_deg: -slot.def.rotation_deg as f32,
        ..SongLuaOverlayState::default()
    };
    if state
        .size
        .is_some_and(|size| size[0] <= 0.0 || size[1] <= 0.0)
    {
        state.size = None;
    }
    Some((
        SongLuaOverlayKind::Sprite {
            texture_path: PathBuf::from(texture_key.as_ref()),
            texture_key,
        },
        state,
    ))
}

fn multitap_arrow_model_layers(
    slots: &[crate::game::parsing::noteskin::SpriteSlot],
) -> Option<Arc<[SongLuaOverlayModelLayer]>> {
    overlay_model_layers_from_slots(slots, multitap_arrow_model_layer_from_slot)
}

fn multitap_arrow_model_layer_from_slot(
    slot: &crate::game::parsing::noteskin::SpriteSlot,
) -> Option<SongLuaOverlayModelLayer> {
    let model = slot.model.as_ref()?;
    if model.vertices.is_empty() {
        return None;
    }
    let uv_rect = slot.uv_for_frame_at(slot.frame_index_from_phase(0.0), 0.0);
    let (uv_scale, uv_offset, uv_tex_shift) = slot.model_uv_params(uv_rect);
    let draw = slot.model_draw_at(0.0, 0.0);
    Some(SongLuaOverlayModelLayer {
        texture_key: slot.texture_key_shared(),
        vertices: crate::game::parsing::noteskin::build_model_geometry(slot),
        model_size: model.size(),
        uv_scale,
        uv_offset,
        uv_tex_shift,
        uv_velocity: slot.uv_velocity,
        uv_cycle_seconds: slot.uv_cycle_seconds,
        draw: SongLuaOverlayModelDraw::new(
            draw.pos,
            draw.rot,
            draw.zoom,
            draw.tint,
            draw.vert_align,
            draw.blend_add,
            draw.visible,
        ),
    })
}

#[cfg(test)]
mod tests;
