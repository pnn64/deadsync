use crate::style::*;
use deadlib_present::actors::{Actor, SizeSpec};
use deadlib_render::{BlendMode, TexturedMeshVertex};
use deadsync_core::note::NoteType;
use deadsync_core::song_time::SongTimeNs;
use deadsync_noteskin::NoteAnimPart;
use glam::Mat4 as Matrix4;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HoldAnimParts {
    pub head: NoteAnimPart,
    pub body: NoteAnimPart,
    pub topcap: NoteAnimPart,
    pub bottomcap: NoteAnimPart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TapReplacementHead {
    pub is_roll: bool,
    pub part: NoteAnimPart,
}

pub fn translated_uv_rect(mut uv: [f32; 4], translate: [f32; 2]) -> [f32; 4] {
    uv[0] += translate[0];
    uv[2] += translate[0];
    uv[1] += translate[1];
    uv[3] += translate[1];
    uv
}

pub fn maybe_flip_uv_vert(mut uv: [f32; 4], flip: bool) -> [f32; 4] {
    if flip {
        uv.swap(1, 3);
    }
    uv
}

pub const fn maybe_mirror_uv_horiz_for_reverse_flipped(
    mut uv: [f32; 4],
    lane_reverse: bool,
    body_flipped: bool,
) -> [f32; 4] {
    if lane_reverse && body_flipped {
        let tmp = uv[0];
        uv[0] = uv[2];
        uv[2] = tmp;
    }
    uv
}

pub const fn top_cap_rotation_deg(lane_reverse: bool, body_flipped: bool) -> f32 {
    if lane_reverse && body_flipped {
        180.0
    } else {
        0.0
    }
}

pub fn scale_effect_size(logical_size: [f32; 2], field_zoom: f32, effect_zoom: f32) -> [f32; 2] {
    [
        logical_size[0] * field_zoom * effect_zoom,
        logical_size[1] * field_zoom * effect_zoom,
    ]
}

pub fn scale_sprite_to_arrow(size: [i32; 2], target_arrow_px: f32) -> [f32; 2] {
    let width = size[0].max(0) as f32;
    let height = size[1].max(0) as f32;
    if height <= 0.0 || target_arrow_px <= 0.0 {
        return [width, height];
    }
    let scale = target_arrow_px / height;
    [width * scale, target_arrow_px]
}

pub fn scale_cap_to_arrow(size: [i32; 2], target_arrow_px: f32) -> [f32; 2] {
    let width = size[0].max(0) as f32;
    let height = size[1].max(0) as f32;
    if width <= 0.0 || target_arrow_px <= 0.0 {
        return [width, height];
    }
    let scale = target_arrow_px / width;
    [target_arrow_px, height * scale]
}

pub fn offset_center(
    center: [f32; 2],
    local_offset: [f32; 2],
    local_offset_rot_sin_cos: [f32; 2],
) -> [f32; 2] {
    let [s, c] = local_offset_rot_sin_cos;
    [
        center[0] + local_offset[0] * c - local_offset[1] * s,
        center[1] + local_offset[0] * s + local_offset[1] * c,
    ]
}

pub fn hold_tail_cap_bounds(
    body_tail_y: f32,
    cap_height: f32,
    body_top: Option<f32>,
    body_bottom: Option<f32>,
) -> Option<(f32, f32)> {
    let default_bounds = (body_tail_y, body_tail_y + cap_height);
    let rendered_bottom = match (body_top, body_bottom) {
        (Some(top), Some(bottom)) if bottom > top + 0.5 => bottom,
        _ => return Some(default_bounds),
    };

    let dist = body_tail_y - rendered_bottom;
    if dist < -2.0 || dist > cap_height + 2.0 {
        return Some(default_bounds);
    }

    Some((rendered_bottom, rendered_bottom + cap_height))
}

pub fn clipped_hold_body_bounds(
    body_top: f32,
    body_bottom: f32,
    natural_top: f32,
    natural_bottom: f32,
) -> Option<(f32, f32)> {
    let top = body_top.max(natural_top);
    let bottom = body_bottom.min(natural_bottom);
    (bottom > top).then_some((top, bottom))
}

pub fn hold_body_bottom_for_tail_cap(body_bottom: f32, y_tail: f32, cap_height: f32) -> f32 {
    if cap_height > 0.0 && body_bottom >= y_tail - 1.0 {
        y_tail + 1.0
    } else {
        body_bottom
    }
}

pub fn hold_draw_span(y_head: f32, y_tail: f32, screen_height: f32) -> Option<(f32, f32)> {
    if ![y_head, y_tail, screen_height]
        .iter()
        .all(|v| v.is_finite())
    {
        return None;
    }
    let mut top = y_head.min(y_tail);
    let mut bottom = y_head.max(y_tail);
    if bottom < -200.0 || top > screen_height + 200.0 {
        return None;
    }
    top = top.max(-400.0);
    bottom = bottom.min(screen_height + 400.0);
    (bottom >= top).then_some((top, bottom))
}

pub fn hold_body_segment_budget(visible_span: f32, segment_height: f32) -> (usize, bool) {
    let estimated = if visible_span <= f32::EPSILON || segment_height <= f32::EPSILON {
        1
    } else {
        (visible_span / segment_height).ceil() as usize
    };
    let max_segments = estimated
        .saturating_add(2)
        .clamp(2048, HOLD_BODY_SEGMENT_SAFETY_MAX);
    (max_segments, estimated <= HOLD_BODY_LEGACY_SEGMENT_LIMIT)
}

pub fn hold_strip_row_3d(
    center: [f32; 3],
    forward: [f32; 2],
    half_width: f32,
    u0: f32,
    u1: f32,
    v: f32,
    color: [f32; 4],
) -> [TexturedMeshVertex; 2] {
    let len = (forward[0] * forward[0] + forward[1] * forward[1])
        .sqrt()
        .max(0.0001);
    let nx = -forward[1] / len * half_width;
    let ny = forward[0] / len * half_width;
    hold_strip_row_from_positions(
        [center[0] + nx, center[1] + ny, center[2]],
        [center[0] - nx, center[1] - ny, center[2]],
        u0,
        u1,
        v,
        color,
    )
}

pub fn hold_strip_row_from_positions(
    left: [f32; 3],
    right: [f32; 3],
    u0: f32,
    u1: f32,
    v: f32,
    color: [f32; 4],
) -> [TexturedMeshVertex; 2] {
    [
        TexturedMeshVertex {
            pos: left,
            uv: [u0, v],
            color,
            tex_matrix_scale: [1.0, 1.0],
        },
        TexturedMeshVertex {
            pos: right,
            uv: [u1, v],
            color,
            tex_matrix_scale: [1.0, 1.0],
        },
    ]
}

pub fn hold_strip_quad(
    top: [TexturedMeshVertex; 2],
    bottom: [TexturedMeshVertex; 2],
) -> [TexturedMeshVertex; 6] {
    [top[0], top[1], bottom[1], top[0], bottom[1], bottom[0]]
}

pub fn hold_strip_actor(
    texture: Arc<str>,
    vertices: Arc<[TexturedMeshVertex]>,
    blend: BlendMode,
    depth_test: bool,
    z: i16,
) -> Actor {
    Actor::TexturedMesh {
        align: [0.5, 0.5],
        offset: [0.0, 0.0],
        world_z: 0.0,
        size: [SizeSpec::Fill, SizeSpec::Fill],
        local_transform: Matrix4::IDENTITY,
        texture,
        tint: [1.0, 1.0, 1.0, 1.0],
        glow: [0.0, 0.0, 0.0, 0.0],
        vertices,
        geom_cache_key: 0,
        uv_scale: [1.0, 1.0],
        uv_offset: [0.0, 0.0],
        uv_tex_shift: [0.0, 0.0],
        depth_test,
        visible: true,
        blend,
        z,
    }
}

pub fn hold_strip_glow_actor(
    texture: Arc<str>,
    vertices: Arc<[TexturedMeshVertex]>,
    depth_test: bool,
    z: i16,
) -> Actor {
    Actor::TexturedMesh {
        align: [0.5, 0.5],
        offset: [0.0, 0.0],
        world_z: 0.0,
        size: [SizeSpec::Fill, SizeSpec::Fill],
        local_transform: Matrix4::IDENTITY,
        texture,
        tint: [1.0, 1.0, 1.0, 0.0],
        glow: [1.0, 1.0, 1.0, 1.0],
        vertices,
        geom_cache_key: 0,
        uv_scale: [1.0, 1.0],
        uv_offset: [0.0, 0.0],
        uv_tex_shift: [0.0, 0.0],
        depth_test,
        visible: true,
        blend: BlendMode::Alpha,
        z,
    }
}

pub const fn tap_part_for_note_type(note_type: NoteType) -> NoteAnimPart {
    match note_type {
        NoteType::Lift => NoteAnimPart::Lift,
        NoteType::Fake => NoteAnimPart::Fake,
        _ => NoteAnimPart::Tap,
    }
}

pub const fn mine_part() -> NoteAnimPart {
    NoteAnimPart::Mine
}

pub const fn hold_parts_for_note_type(note_type: NoteType) -> HoldAnimParts {
    match note_type {
        NoteType::Roll => HoldAnimParts {
            head: NoteAnimPart::RollHead,
            body: NoteAnimPart::RollBody,
            topcap: NoteAnimPart::RollTopCap,
            bottomcap: NoteAnimPart::RollBottomCap,
        },
        _ => HoldAnimParts {
            head: NoteAnimPart::HoldHead,
            body: NoteAnimPart::HoldBody,
            topcap: NoteAnimPart::HoldTopCap,
            bottomcap: NoteAnimPart::HoldBottomCap,
        },
    }
}

#[cfg(test)]
pub(crate) const fn hold_head_part_for_roll(is_roll: bool) -> NoteAnimPart {
    if is_roll {
        NoteAnimPart::RollHead
    } else {
        NoteAnimPart::HoldHead
    }
}

pub const fn tap_replacement_head(
    note_type: NoteType,
    has_hold: bool,
    has_roll: bool,
    draw_hold: bool,
    draw_roll: bool,
    hold_priority: bool,
) -> Option<TapReplacementHead> {
    match note_type {
        NoteType::Tap | NoteType::Lift => {
            if has_hold && draw_hold && (hold_priority || !has_roll || !draw_roll) {
                Some(TapReplacementHead {
                    is_roll: false,
                    part: NoteAnimPart::HoldHead,
                })
            } else if has_roll && draw_roll {
                Some(TapReplacementHead {
                    is_roll: true,
                    part: NoteAnimPart::RollHead,
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn bottom_cap_uv_window(
    v0: f32,
    v1: f32,
    visible: f32,
    full: f32,
    reverse: bool,
) -> Option<(f32, f32)> {
    if visible <= 0.0 || full <= 0.0 {
        return None;
    }
    let t = (visible / full).clamp(0.0, 1.0);
    if reverse {
        Some((v0, v0 + (v1 - v0) * t))
    } else {
        Some((v1 - (v1 - v0) * t, v1))
    }
}

pub fn hold_segment_pose(top: [f32; 2], bottom: [f32; 2]) -> ([f32; 2], f32, f32) {
    let dx = bottom[0] - top[0];
    let dy = bottom[1] - top[1];
    let center = [(top[0] + bottom[0]) * 0.5, (top[1] + bottom[1]) * 0.5];
    let len = (dx * dx + dy * dy).sqrt();
    let rot = dx.atan2(dy).to_degrees();
    (center, len, rot)
}

pub fn song_time_ns_to_seconds(time_ns: SongTimeNs) -> f32 {
    (time_ns as f64 * 1.0e-9) as f32
}

pub fn song_time_ns_delta_seconds(lhs: SongTimeNs, rhs: SongTimeNs) -> f32 {
    ((lhs as i128 - rhs as i128) as f64 * 1.0e-9) as f32
}
