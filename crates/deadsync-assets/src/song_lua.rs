use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use deadlib_present::actors::TextAttribute;
use deadlib_render_core::{SamplerDesc, SamplerFilter, SamplerWrap, TexturedMeshVertex};
use deadsync_chart::{ChartData, GameplayChartData, SongData};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::{GameplayInputPlayStyle as GameplayStyle, GameplaySession};
use deadsync_noteskin::{NUM_QUANTIZATIONS, Style};
use deadsync_profile_gameplay::stage_charts::{StageChartReload, apply_stage_chart_reload};
use deadsync_song_lua::playback::PreparedGameplaySongLua;
use deadsync_song_lua::{
    SongLuaMediaProbes, overlay_model_layers_from_slots, song_lua_human_player_count,
    song_lua_style_info,
};

mod context;

pub use context::{
    init_song_lua_bake_cache, resolve_song_lua_judgment_textures, song_lua_bake_cache,
    song_lua_catalog_note_bounds, song_lua_compile_context,
    song_lua_play_context,
};

pub use deadsync_song_lua::{
    SONG_LUA_INITIAL_LIFE, SongLuaCapturedActor, SongLuaCapturedChildActor,
    SongLuaColumnOffsetWindow, SongLuaCompileContext, SongLuaCompileInfo, SongLuaDifficulty,
    SongLuaEaseTarget, SongLuaEaseWindow, SongLuaMessageEvent, SongLuaModWindow,
    SongLuaNoteHideWindow, SongLuaNoteskinResolver, SongLuaOverlayBlendMode,
    SongLuaOverlayCommandBlock, SongLuaOverlayEase, SongLuaOverlayMeshVertex,
    SongLuaOverlayMessageCommand, SongLuaOverlayModelDraw, SongLuaOverlayState,
    SongLuaOverlayStateDelta, SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue,
    SongLuaPlayerContext, SongLuaProxyTarget, SongLuaSpanMode, SongLuaSpeedMod,
    SongLuaStatefulMessageCapture, SongLuaStatefulMessageWrite, SongLuaTextGlowMode,
    SongLuaTimeUnit, THEME_RECEPTOR_Y_STD, file_path_string, overlay_state_after_blocks,
    parse_overlay_blend_mode, parse_overlay_effect_clock, parse_overlay_effect_mode,
    parse_overlay_text_align, parse_overlay_text_glow_mode, parse_song_timing_bpms,
    song_elapsed_seconds_at,
};

pub type SongLuaOverlayModelLayer = deadsync_song_lua::SongLuaOverlayModelLayer<TexturedMeshVertex>;
pub type SongLuaOverlayKind = deadsync_song_lua::SongLuaOverlayKind<
    crate::noteskin::SpriteSlot,
    TexturedMeshVertex,
    TextAttribute,
>;
pub type SongLuaOverlayActor = deadsync_song_lua::SongLuaOverlayActor<SongLuaOverlayKind>;
pub type CompiledSongLua = deadsync_song_lua::CompiledSongLua<SongLuaOverlayActor>;

/// Movie metadata used by asset-backed compilation. Stage callers supply sound metadata.
pub const MEDIA_PROBES: SongLuaMediaProbes = SongLuaMediaProbes {
    sound_length_seconds: None,
    movie_dimensions: Some(deadlib_video::dimensions),
};

fn context_with_movie_probe(context: &SongLuaCompileContext) -> Cow<'_, SongLuaCompileContext> {
    if context.media_probes.movie_dimensions.is_some() {
        return Cow::Borrowed(context);
    }
    let mut context = context.clone();
    context.media_probes.movie_dimensions = MEDIA_PROBES.movie_dimensions;
    Cow::Owned(context)
}

/// Identifies this build to the song-Lua bake cache. The compile functions
/// below resolve noteskins, models and multitap arrows for the baker, so a
/// change to them or to anything they link must not reuse older bakes.
pub const SONG_LUA_BAKE_HOST_BUILD: &str = env!("DEADSYNC_ASSETS_BAKE_BUILD");

#[must_use]
pub fn overlay_sampler(overlay: &SongLuaOverlayActor) -> SamplerDesc {
    SamplerDesc {
        filter: if deadsync_song_lua::overlay_actor_uses_nearest_sampler(overlay) {
            SamplerFilter::Nearest
        } else {
            SamplerFilter::Linear
        },
        wrap: if deadsync_song_lua::overlay_actor_uses_repeat_sampler(overlay) {
            SamplerWrap::Repeat
        } else {
            SamplerWrap::Clamp
        },
        ..SamplerDesc::default()
    }
}

pub fn compile_song_lua(
    entry_path: &Path,
    context: &SongLuaCompileContext,
) -> Result<CompiledSongLua, String> {
    let context = context_with_movie_probe(context);
    deadsync_song_lua::compile_song_lua(
        entry_path,
        &context,
        song_lua_noteskin_resolver(),
        crate::noteskin::load_itg_model_slots,
        model_layer_from_slot,
        |context, noteskin| multitap_arrow_visual_spec(noteskin, context),
    )
}

pub fn compile_song_lua_layers(
    entry_paths: &[&Path],
    primary_index: usize,
    context: &SongLuaCompileContext,
) -> Result<Vec<CompiledSongLua>, String> {
    let context = context_with_movie_probe(context);
    deadsync_song_lua::compile_song_lua_layers(
        entry_paths,
        primary_index,
        &context,
        song_lua_noteskin_resolver(),
        crate::noteskin::load_itg_model_slots,
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

/// Apply the stage selection produced by the prepared Lua scene, when present.
pub fn apply_song_lua_reload<S>(
    song: &SongData,
    charts: &mut [Arc<ChartData>; MAX_PLAYERS],
    payloads: &mut [Arc<GameplayChartData>; MAX_PLAYERS],
    prepared: &PreparedGameplaySongLua<S>,
    session: &mut GameplaySession,
    load_charts: impl FnOnce(&SongData, &[usize]) -> Result<Vec<GameplayChartData>, String>,
) -> Result<bool, String> {
    let Some(steps) = prepared.loaded_steps() else {
        return Ok(false);
    };
    let joined_sides = prepared.loaded_players().map(|players| {
        if session.p2_runtime_player() {
            [players[1], players[0]]
        } else {
            players
        }
    });
    let play_style = prepared
        .loaded_style()
        .map(|style| lua_reload_style(session.play_style.is_pump(), style))
        .transpose()?;
    apply_stage_chart_reload(
        song,
        charts,
        payloads,
        StageChartReload {
            steps,
            joined_sides,
            play_style,
        },
        session,
        load_charts,
    )?;
    Ok(true)
}

fn lua_reload_style(pump: bool, style: &str) -> Result<GameplayStyle, String> {
    use GameplayStyle as Style;

    let selected = match (pump, style) {
        (false, "single") => Style::Single,
        (false, "double") => Style::Double,
        (false, "versus") => Style::Versus,
        (false, "couple") => Style::Couple,
        (false, "routine") => Style::Routine,
        // Prepared Pump scenes retain the game-qualified style key.
        (true, "single" | "pump-single") => Style::PumpSingle,
        (true, "double" | "pump-double") => Style::PumpDouble,
        (true, "versus" | "pump-versus") => Style::PumpVersus,
        (true, "couple" | "pump-couple") => Style::PumpCouple,
        (true, "routine" | "pump-routine") => Style::PumpRoutine,
        _ => return Err(format!("Lua reload selected unsupported style {style}")),
    };
    Ok(selected)
}

pub fn reload_song_lua(
    scene: deadsync_song_lua::SongLuaLiveScene,
) -> Result<Vec<CompiledSongLua>, String> {
    deadsync_song_lua::reload_song_lua(
        scene,
        crate::noteskin::load_itg_model_slots,
        model_layer_from_slot,
    )
}

pub fn model_layer_from_slot(
    slot: &crate::noteskin::SpriteSlot,
) -> Option<SongLuaOverlayModelLayer> {
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
    let mut layer = SongLuaOverlayModelLayer::new(
        slot.texture_key_shared(),
        crate::noteskin::build_model_geometry(slot),
        model.size(),
        uv_scale,
        uv_offset,
        uv_tex_shift,
        slot.uv_velocity,
        slot.uv_cycle_seconds,
        song_lua_model_draw(slot.model_draw_at(0.0, 0.0)),
    );
    layer.material = model.material;
    layer.texture_frames = slot
        .model_texture_states
        .iter()
        .enumerate()
        .map(|(index, state)| {
            let base = slot.uv_for_frame(index);
            let (scale, offset, shift) = slot.model_uv_params(base);
            let glow = deadsync_song_lua::SongLuaModelTextureUv {
                scale,
                offset,
                shift,
            };
            let mut diffuse = glow;
            for axis in 0..2 {
                let shift = (slot.uv_offset[axis] + state.translation[axis]) * scale[axis];
                diffuse.offset[axis] += shift;
                diffuse.shift[axis] += shift;
            }
            deadsync_song_lua::SongLuaModelTextureFrame {
                texture_key: slot.model_texture_keys.get(index).cloned(),
                delay: state.delay,
                diffuse,
                glow,
            }
        })
        .collect();
    if let Some(texture) = &slot.model_additive {
        layer.additive = Some(texture.texture_key_shared());
        layer.additive_frames = texture
            .model_texture_states
            .iter()
            .enumerate()
            .map(|(index, state)| {
                let rect = texture.uv_for_frame(index);
                let uv = deadsync_song_lua::SongLuaModelTextureUv {
                    scale: [rect[2] - rect[0], rect[3] - rect[1]],
                    offset: [rect[0], rect[1]],
                    shift: [0.0; 2],
                };
                deadsync_song_lua::SongLuaModelTextureFrame {
                    texture_key: texture.model_texture_keys.get(index).cloned(),
                    delay: state.delay,
                    diffuse: uv,
                    glow: uv,
                }
            })
            .collect();
    }
    Some(layer)
}

const fn song_lua_model_draw(draw: deadsync_noteskin::ModelDrawState) -> SongLuaOverlayModelDraw {
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
            states: [].into(),
            textures: [].into(),
        },
        state,
    ))
}

fn multitap_arrow_model_layer_from_slot(
    slot: &crate::noteskin::SpriteSlot,
) -> Option<SongLuaOverlayModelLayer> {
    model_layer_from_slot_frame(slot, slot.frame_index_from_phase(0.0))
}
