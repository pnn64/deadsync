use crate::style::*;
use deadlib_present::actors::{Actor, SizeSpec};
use deadlib_render::{BlendMode, TexturedMeshVertex};
use deadsync_core::note::NoteType;
use deadsync_core::song_time::SongTimeNs;
use deadsync_gameplay::let_go_head_beat as gameplay_let_go_head_beat;
use deadsync_noteskin::{HoldVisuals, NoteAnimPart};
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

#[derive(Debug)]
pub struct HoldEntryPlanRequest<'a, T> {
    pub note_type: NoteType,
    pub head_travel: f32,
    pub tail_travel: f32,
    pub head_y: f32,
    pub tail_y: f32,
    pub receptor_y: f32,
    pub screen_height: f32,
    pub lane_reverse: bool,
    pub engaged: bool,
    pub use_active: bool,
    pub flip_body_reverse: bool,
    pub flip_head_tail_reverse: bool,
    pub start_body_offset: f32,
    pub stop_body_offset: f32,
    pub let_go_gray: f32,
    pub life: f32,
    pub head_phase: f32,
    pub body_phase: f32,
    pub top_cap_phase: f32,
    pub bottom_cap_phase: f32,
    pub visuals: &'a HoldVisuals<T>,
}

#[derive(Debug, PartialEq)]
pub struct HoldEntryPlan<'a, T> {
    pub body_flipped: bool,
    pub y_head: f32,
    pub y_tail: f32,
    pub draw_span: Option<(f32, f32)>,
    pub diffuse: [f32; 4],
    pub head_anchor_y: f32,
    pub head_anchor_travel: f32,
    pub parts: HoldAnimParts,
    pub head_phase: f32,
    pub body_phase: f32,
    pub top_cap_phase: f32,
    pub bottom_cap_phase: f32,
    pub top_cap_slot: Option<&'a T>,
    pub bottom_cap_slot: Option<&'a T>,
    pub body_slot: Option<&'a T>,
    pub head_layers: Option<&'a [T]>,
    pub head_slot: Option<&'a T>,
}

/// Resolve the beat used by a hold head without exposing gameplay hold state to
/// the canonical notefield planner.
pub fn hold_entry_head_beat(
    note_beat: f32,
    end_beat: f32,
    last_held_beat: f32,
    visible_beat: f32,
    dynamic: bool,
) -> f32 {
    if !dynamic {
        return note_beat;
    }
    gameplay_let_go_head_beat(note_beat, end_beat, last_held_beat, visible_beat)
}

/// Plan one hold's canonical geometry, noteskin state fallbacks, and tint. The
/// caller retains concrete asset lookup and actor/model emission. The adapter
/// supplies only Hold/Roll entries and phases computed from
/// `hold_parts_for_note_type(note_type)` at the original note beat; keeping
/// those eligibility and noteskin calls outside preserves their existing
/// runtime ownership.
pub fn hold_entry_plan<T>(request: HoldEntryPlanRequest<'_, T>) -> HoldEntryPlan<'_, T> {
    let mut hold_start_y = if request.lane_reverse {
        request.tail_y
    } else {
        request.head_y
    };
    let mut hold_end_y = if request.lane_reverse {
        request.head_y
    } else {
        request.tail_y
    };
    let mut hold_start_travel = if request.lane_reverse {
        request.tail_travel
    } else {
        request.head_travel
    };
    let mut hold_end_travel = if request.lane_reverse {
        request.head_travel
    } else {
        request.tail_travel
    };
    if request.engaged {
        if request.lane_reverse {
            hold_end_y = request.receptor_y;
            hold_end_travel = 0.0;
        } else {
            hold_start_y = request.receptor_y;
            hold_start_travel = 0.0;
        }
    }

    let body_flipped = request.lane_reverse && request.flip_body_reverse;
    let (y_head, y_tail) = if body_flipped {
        (
            hold_start_y - request.stop_body_offset,
            hold_end_y - request.start_body_offset,
        )
    } else {
        (
            hold_start_y + request.start_body_offset,
            hold_end_y + request.stop_body_offset,
        )
    };
    let flip_head_tail = request.lane_reverse && request.flip_head_tail_reverse;
    let (head_anchor_y, head_anchor_travel) = if flip_head_tail {
        (hold_end_y, hold_end_travel)
    } else {
        (hold_start_y, hold_start_travel)
    };

    let let_go_gray = request.let_go_gray.clamp(0.0, 1.0);
    let life = request.life.clamp(0.0, 1.0);
    let color_scale = let_go_gray + (1.0 - let_go_gray) * life;
    let mut parts = hold_parts_for_note_type(request.note_type);
    let mut top_cap_phase = request.top_cap_phase;
    let mut bottom_cap_phase = request.bottom_cap_phase;
    let mut top_cap_slot = preferred_hold_visual(
        request.visuals.topcap_active.as_ref(),
        request.visuals.topcap_inactive.as_ref(),
        request.use_active,
    );
    let mut bottom_cap_slot = preferred_hold_visual(
        request.visuals.bottomcap_active.as_ref(),
        request.visuals.bottomcap_inactive.as_ref(),
        request.use_active,
    );
    if body_flipped {
        std::mem::swap(&mut top_cap_slot, &mut bottom_cap_slot);
        std::mem::swap(&mut parts.topcap, &mut parts.bottomcap);
        std::mem::swap(&mut top_cap_phase, &mut bottom_cap_phase);
    }

    let body_slot = preferred_hold_visual(
        request.visuals.body_active.as_ref(),
        request.visuals.body_inactive.as_ref(),
        request.use_active,
    );
    let head_layers = preferred_hold_visual(
        request.visuals.head_active_layers.as_deref(),
        request.visuals.head_inactive_layers.as_deref(),
        request.use_active,
    );
    let head_slot = if head_layers.is_none() {
        preferred_hold_visual(
            request.visuals.head_active.as_ref(),
            request.visuals.head_inactive.as_ref(),
            request.use_active,
        )
    } else {
        None
    };

    HoldEntryPlan {
        body_flipped,
        y_head,
        y_tail,
        draw_span: hold_draw_span(y_head, y_tail, request.screen_height),
        diffuse: [color_scale, color_scale, color_scale, 1.0],
        head_anchor_y,
        head_anchor_travel,
        parts,
        head_phase: request.head_phase,
        body_phase: request.body_phase,
        top_cap_phase,
        bottom_cap_phase,
        top_cap_slot,
        bottom_cap_slot,
        body_slot,
        head_layers,
        head_slot,
    }
}

fn preferred_hold_visual<'a, T: ?Sized>(
    active: Option<&'a T>,
    inactive: Option<&'a T>,
    use_active: bool,
) -> Option<&'a T> {
    if use_active {
        active.or(inactive)
    } else {
        inactive.or(active)
    }
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
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        world_z: 0.0,
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        local_transform: Matrix4::IDENTITY,
        texture,
        tint: [1.0, 1.0, 1.0, 1.0],
        glow: [1.0, 1.0, 1.0, 0.0],
        vertices,
        geom_cache_key: deadlib_render::INVALID_TMESH_CACHE_KEY,
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
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        world_z: 0.0,
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        local_transform: Matrix4::IDENTITY,
        texture,
        tint: [1.0, 1.0, 1.0, 0.0],
        glow: [1.0, 1.0, 1.0, 1.0],
        vertices,
        geom_cache_key: deadlib_render::INVALID_TMESH_CACHE_KEY,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn visuals() -> HoldVisuals<u8> {
        HoldVisuals {
            head_inactive: Some(10),
            head_active: Some(11),
            head_inactive_layers: None,
            head_active_layers: None,
            body_inactive: Some(20),
            body_active: Some(21),
            topcap_inactive: Some(30),
            topcap_active: Some(31),
            bottomcap_inactive: Some(40),
            bottomcap_active: Some(41),
            explosion: None,
        }
    }

    fn request<T>(visuals: &HoldVisuals<T>) -> HoldEntryPlanRequest<'_, T> {
        HoldEntryPlanRequest {
            note_type: NoteType::Hold,
            head_travel: 10.0,
            tail_travel: 40.0,
            head_y: 100.0,
            tail_y: 220.0,
            receptor_y: 80.0,
            screen_height: 480.0,
            lane_reverse: false,
            engaged: false,
            use_active: false,
            flip_body_reverse: false,
            flip_head_tail_reverse: false,
            start_body_offset: 5.0,
            stop_body_offset: 7.0,
            let_go_gray: 0.25,
            life: 0.5,
            head_phase: 1.0,
            body_phase: 2.0,
            top_cap_phase: 3.0,
            bottom_cap_phase: 4.0,
            visuals,
        }
    }

    #[test]
    fn hold_entry_prefers_requested_state_then_falls_back() {
        let visuals = visuals();
        let inactive = hold_entry_plan(request(&visuals));
        assert_eq!(inactive.head_slot.copied(), Some(10));
        assert_eq!(inactive.body_slot.copied(), Some(20));
        assert_eq!(inactive.top_cap_slot.copied(), Some(30));
        assert_eq!(inactive.bottom_cap_slot.copied(), Some(40));

        let mut active_request = request(&visuals);
        active_request.use_active = true;
        let active = hold_entry_plan(active_request);
        assert_eq!(active.head_slot.copied(), Some(11));
        assert_eq!(active.body_slot.copied(), Some(21));
        assert_eq!(active.top_cap_slot.copied(), Some(31));
        assert_eq!(active.bottom_cap_slot.copied(), Some(41));

        let fallback_visuals = HoldVisuals {
            head_inactive: Some(10),
            body_inactive: Some(20),
            topcap_inactive: Some(30),
            bottomcap_inactive: Some(40),
            ..HoldVisuals::default()
        };
        let mut fallback_request = request(&fallback_visuals);
        fallback_request.use_active = true;
        let fallback = hold_entry_plan(fallback_request);
        assert_eq!(fallback.head_slot.copied(), Some(10));
        assert_eq!(fallback.body_slot.copied(), Some(20));
        assert_eq!(fallback.top_cap_slot.copied(), Some(30));
        assert_eq!(fallback.bottom_cap_slot.copied(), Some(40));

        let active_only_visuals = HoldVisuals {
            head_active: Some(11),
            body_active: Some(21),
            topcap_active: Some(31),
            bottomcap_active: Some(41),
            ..HoldVisuals::default()
        };
        let active_fallback = hold_entry_plan(request(&active_only_visuals));
        assert_eq!(active_fallback.head_slot.copied(), Some(11));
        assert_eq!(active_fallback.body_slot.copied(), Some(21));
        assert_eq!(active_fallback.top_cap_slot.copied(), Some(31));
        assert_eq!(active_fallback.bottom_cap_slot.copied(), Some(41));
    }

    #[test]
    fn hold_entry_layers_suppress_single_head_even_when_empty() {
        let layered_visuals = HoldVisuals {
            head_active: Some(11),
            head_active_layers: Some(Arc::from(Vec::<u8>::new())),
            ..visuals()
        };
        let mut layer_request = request(&layered_visuals);
        layer_request.use_active = true;
        let plan = hold_entry_plan(layer_request);
        assert!(
            plan.head_layers
                .expect("layers should remain selected")
                .is_empty()
        );
        assert_eq!(plan.head_slot, None);

        let fallback_visuals = HoldVisuals {
            head_active_layers: None,
            head_inactive_layers: Some(Arc::from([7, 8])),
            ..visuals()
        };
        let mut fallback_request = request(&fallback_visuals);
        fallback_request.use_active = true;
        let fallback = hold_entry_plan(fallback_request);
        assert_eq!(fallback.head_layers, Some([7, 8].as_slice()));
        assert_eq!(fallback.head_slot, None);

        let active_only_visuals = HoldVisuals {
            head_inactive_layers: None,
            head_active_layers: Some(Arc::from([9, 10])),
            ..visuals()
        };
        let active_fallback = hold_entry_plan(request(&active_only_visuals));
        assert_eq!(active_fallback.head_layers, Some([9, 10].as_slice()));
        assert_eq!(active_fallback.head_slot, None);
    }

    #[test]
    fn reverse_hold_swaps_geometry_caps_parts_and_phases() {
        let visuals = visuals();
        let mut request = request(&visuals);
        request.lane_reverse = true;
        request.flip_body_reverse = true;
        request.flip_head_tail_reverse = true;
        let plan = hold_entry_plan(request);

        assert!(plan.body_flipped);
        assert_eq!((plan.y_head, plan.y_tail), (213.0, 95.0));
        assert_eq!(plan.draw_span, Some((95.0, 213.0)));
        assert_eq!((plan.head_anchor_y, plan.head_anchor_travel), (100.0, 10.0));
        assert_eq!(plan.parts.topcap, NoteAnimPart::HoldBottomCap);
        assert_eq!(plan.parts.bottomcap, NoteAnimPart::HoldTopCap);
        assert_eq!((plan.top_cap_phase, plan.bottom_cap_phase), (4.0, 3.0));
        assert_eq!(plan.top_cap_slot.copied(), Some(40));
        assert_eq!(plan.bottom_cap_slot.copied(), Some(30));
    }

    #[test]
    fn engaged_hold_clamps_after_direction_swap() {
        let visuals = visuals();
        let mut normal_request = request(&visuals);
        normal_request.engaged = true;
        let normal = hold_entry_plan(normal_request);
        assert_eq!((normal.y_head, normal.y_tail), (85.0, 227.0));
        assert_eq!(
            (normal.head_anchor_y, normal.head_anchor_travel),
            (80.0, 0.0)
        );

        let mut reverse_request = request(&visuals);
        reverse_request.engaged = true;
        reverse_request.lane_reverse = true;
        let reverse = hold_entry_plan(reverse_request);
        assert_eq!((reverse.y_head, reverse.y_tail), (225.0, 87.0));
        assert_eq!(
            (reverse.head_anchor_y, reverse.head_anchor_travel),
            (220.0, 40.0)
        );
    }

    #[test]
    fn reverse_engaged_hold_keeps_body_and_head_flips_independent() {
        let visuals = visuals();
        let mut head_flip_request = request(&visuals);
        head_flip_request.lane_reverse = true;
        head_flip_request.engaged = true;
        head_flip_request.flip_head_tail_reverse = true;
        let head_flip = hold_entry_plan(head_flip_request);
        assert!(!head_flip.body_flipped);
        assert_eq!((head_flip.y_head, head_flip.y_tail), (225.0, 87.0));
        assert_eq!(
            (head_flip.head_anchor_y, head_flip.head_anchor_travel),
            (80.0, 0.0)
        );
        assert_eq!(head_flip.top_cap_slot.copied(), Some(30));
        assert_eq!(head_flip.bottom_cap_slot.copied(), Some(40));
        assert_eq!(
            (head_flip.top_cap_phase, head_flip.bottom_cap_phase),
            (3.0, 4.0)
        );

        let mut body_flip_request = request(&visuals);
        body_flip_request.lane_reverse = true;
        body_flip_request.engaged = true;
        body_flip_request.flip_body_reverse = true;
        let body_flip = hold_entry_plan(body_flip_request);
        assert!(body_flip.body_flipped);
        assert_eq!((body_flip.y_head, body_flip.y_tail), (213.0, 75.0));
        assert_eq!(
            (body_flip.head_anchor_y, body_flip.head_anchor_travel),
            (220.0, 40.0)
        );
        assert_eq!(body_flip.top_cap_slot.copied(), Some(40));
        assert_eq!(body_flip.bottom_cap_slot.copied(), Some(30));
        assert_eq!(
            (body_flip.top_cap_phase, body_flip.bottom_cap_phase),
            (4.0, 3.0)
        );
    }

    #[test]
    fn hold_entry_head_beat_preserves_static_and_dynamic_paths() {
        assert_eq!(
            hold_entry_head_beat(100.0, f32::NAN, f32::NAN, f32::NAN, false),
            100.0
        );
        assert_eq!(
            hold_entry_head_beat(100.0, 108.0, 102.0, 101.25, true),
            101.25
        );
        assert_eq!(
            hold_entry_head_beat(100.0, 108.0, 102.0, 103.0, true),
            102.0
        );
    }

    #[test]
    fn hold_entry_tint_uses_clamped_life_and_let_go_gray() {
        let visuals = visuals();
        let plan = hold_entry_plan(request(&visuals));
        assert_eq!(plan.diffuse, [0.625, 0.625, 0.625, 1.0]);

        let mut clamped_request = request(&visuals);
        clamped_request.let_go_gray = -2.0;
        clamped_request.life = 2.0;
        let clamped = hold_entry_plan(clamped_request);
        assert_eq!(clamped.diffuse, [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn hold_entry_keeps_invalid_span_local_to_body_and_caps() {
        let partial_visuals = HoldVisuals {
            head_inactive: Some(10),
            topcap_inactive: Some(30),
            bottomcap_inactive: Some(40),
            ..HoldVisuals::default()
        };
        let mut offscreen_request = request(&partial_visuals);
        offscreen_request.head_y = f32::NAN;
        offscreen_request.tail_y = -450.0;
        let offscreen = hold_entry_plan(offscreen_request);
        assert_eq!(offscreen.draw_span, None);
        assert!(offscreen.y_head.is_nan());
        assert_eq!(offscreen.body_slot, None);
        assert_eq!(offscreen.head_slot.copied(), Some(10));
        assert_eq!(offscreen.top_cap_slot.copied(), Some(30));
        assert_eq!(offscreen.bottom_cap_slot.copied(), Some(40));
    }
}
