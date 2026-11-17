//! Existing text, mask and sprite properties published by preparation setters.
use super::*;
use crate::{SongLuaOverlayStateDelta as Delta, SongLuaOverlayUpdateTarget as Target};

pub(crate) const TARGETS: &[Target] = &[
    Target::Uppercase,
    Target::MaskSource,
    Target::MaskDest,
    Target::RainbowScroll,
    Target::TextJitter,
    Target::TextDistortion,
    Target::TextGlowMode,
    Target::MultAttrsWithDiffuse,
    Target::SpriteAnimate,
    Target::SpriteLoop,
    Target::SpritePlaybackRate,
    Target::SpriteStateDelay,
    Target::VertSpacing,
    Target::WrapWidthPixels,
    Target::MaxWidth,
    Target::MaxHeight,
    Target::MaxWPreZoom,
    Target::MaxHPreZoom,
    Target::MaxDimensionUsesZoom,
    Target::TextureWrapping,
    Target::TexcoordOffset,
    Target::CustomTextureRect,
    Target::TexcoordVelocity,
];

// Each call represents one successful setter write, including equal values.
// Compound setters call this in native order instead of comparing snapshots.
pub(super) fn record(call: &mut HostCall<'_>, id: TableId, key: &str) {
    let delta = {
        let guard = call.store();
        let state = &guard.actors[&id].state.render;
        let mut delta = Delta::default();
        match key {
            "uppercase" => delta.uppercase = Some(state.uppercase),
            "mask_source" => delta.mask_source = Some(state.mask_source),
            "mask_dest" => delta.mask_dest = Some(state.mask_dest),
            "rainbow_scroll" => delta.rainbow_scroll = Some(state.rainbow_scroll),
            "text_jitter" => delta.text_jitter = Some(state.text_jitter),
            "text_distortion" => delta.text_distortion = Some(state.text_distortion),
            "text_glow_mode" => delta.text_glow_mode = Some(state.text_glow_mode),
            "mult_attrs_with_diffuse" => {
                delta.mult_attrs_with_diffuse = Some(state.mult_attrs_with_diffuse)
            }
            "sprite_animate" => delta.sprite_animate = Some(state.sprite_animate),
            "sprite_loop" => delta.sprite_loop = Some(state.sprite_loop),
            "sprite_playback_rate" => delta.sprite_playback_rate = Some(state.sprite_playback_rate),
            "sprite_state_delay" => delta.sprite_state_delay = Some(state.sprite_state_delay),
            "vert_spacing" => delta.vert_spacing = state.vert_spacing,
            "wrap_width_pixels" => delta.wrap_width_pixels = state.wrap_width_pixels,
            "max_width" => delta.max_width = state.max_width,
            "max_height" => delta.max_height = state.max_height,
            "max_w_pre_zoom" => delta.max_w_pre_zoom = Some(state.max_w_pre_zoom),
            "max_h_pre_zoom" => delta.max_h_pre_zoom = Some(state.max_h_pre_zoom),
            "max_dimension_uses_zoom" => {
                delta.max_dimension_uses_zoom = Some(state.max_dimension_uses_zoom)
            }
            "texture_wrapping" => delta.texture_wrapping = Some(state.texture_wrapping),
            "texcoord_offset" => delta.texcoord_offset = state.texcoord_offset,
            "custom_texture_rect" => delta.custom_texture_rect = state.custom_texture_rect,
            "texcoord_velocity" => delta.texcoord_velocity = state.texcoord_velocity,
            _ => unreachable!("unknown captured media property"),
        }
        delta
    };
    capture::write(call, id, delta, false);
}

pub(super) fn delta(call: &mut HostCall<'_>, id: TableId, delta: &Delta) {
    if song_immediate::context(&call.store(), id).is_none() {
        return;
    }
    let mut state = crate::SongLuaOverlayState::default();
    crate::apply_overlay_delta(&mut state, delta);
    for &target in TARGETS {
        if delta.has_update_target(target) {
            song_properties::record(
                call,
                id,
                target,
                crate::perframe::overlay_state_update_value(&state, target),
            );
        }
    }
}
