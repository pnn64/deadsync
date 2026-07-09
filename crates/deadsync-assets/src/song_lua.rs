use std::path::{Path, PathBuf};

use deadlib_present::actors::TextAttribute;
use deadlib_render::TexturedMeshVertex;
use deadsync_noteskin::{NUM_QUANTIZATIONS, Style};
use deadsync_song_lua::{
    compile_song_lua_with_default_host, overlay_model_layers_from_slots,
    song_lua_human_player_count, song_lua_style_info,
};

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
    crate::noteskin::SpriteSlot,
    TexturedMeshVertex,
    TextAttribute,
>;
pub type SongLuaOverlayActor = deadsync_song_lua::SongLuaOverlayActor<SongLuaOverlayKind>;
pub type CompiledSongLua = deadsync_song_lua::CompiledSongLua<SongLuaOverlayActor>;

pub fn compile_song_lua(
    entry_path: &Path,
    context: &SongLuaCompileContext,
) -> Result<CompiledSongLua, String> {
    compile_song_lua_with_default_host(
        entry_path,
        context,
        song_lua_noteskin_resolver(),
        crate::noteskin::load_itg_model_slots_from_path,
        model_layer_from_slot,
        |context, noteskin| multitap_arrow_visual_spec(noteskin, context),
    )
}

fn song_lua_noteskin_resolver() -> SongLuaNoteskinResolver {
    SongLuaNoteskinResolver {
        resolve_path: crate::noteskin::song_lua_noteskin_resolve_path,
        metric: crate::noteskin::song_lua_noteskin_metric,
        metric_f: crate::noteskin::song_lua_noteskin_metric_f,
        metric_b: crate::noteskin::song_lua_noteskin_metric_b,
        exists: crate::noteskin::song_lua_noteskin_exists,
        names: crate::noteskin::song_lua_noteskin_names,
    }
}

fn model_layer_from_slot(slot: &crate::noteskin::SpriteSlot) -> Option<SongLuaOverlayModelLayer> {
    model_layer_from_slot_frame(slot, 0)
}

fn model_layer_from_slot_frame(
    slot: &crate::noteskin::SpriteSlot,
    frame_index: usize,
) -> Option<SongLuaOverlayModelLayer> {
    let model = slot.model.as_ref()?;
    if model.vertices.is_empty() {
        return None;
    }
    let uv_rect = slot.uv_for_frame_at(frame_index, 0.0);
    let (uv_scale, uv_offset, uv_tex_shift) = slot.model_uv_params(uv_rect);
    Some(SongLuaOverlayModelLayer::new(
        slot.texture_key_shared(),
        crate::noteskin::build_model_geometry(slot),
        model.size(),
        uv_scale,
        uv_offset,
        uv_tex_shift,
        slot.uv_velocity,
        slot.uv_cycle_seconds,
        song_lua_model_draw(slot.model_draw_at(0.0, 0.0)),
    ))
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

fn multitap_arrow_visual_spec(
    noteskin: &str,
    context: &SongLuaCompileContext,
) -> Option<(SongLuaOverlayKind, SongLuaOverlayState)> {
    let style = Style {
        num_cols: song_lua_style_info(&context.style_name).columns,
        num_players: song_lua_human_player_count(context).max(1),
    };
    let ns = crate::noteskin::load_itg_skin_cached(&style, noteskin).ok()?;
    let down_col = 1.min(style.num_cols.saturating_sub(1));
    let note_idx = down_col * NUM_QUANTIZATIONS;
    let layers = ns.note_layers.get(note_idx)?;
    if let Some(model_layers) =
        overlay_model_layers_from_slots(layers, multitap_arrow_model_layer_from_slot)
    {
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

fn multitap_arrow_model_layer_from_slot(
    slot: &crate::noteskin::SpriteSlot,
) -> Option<SongLuaOverlayModelLayer> {
    model_layer_from_slot_frame(slot, slot.frame_index_from_phase(0.0))
}
