use crate::actor_builder::actor_with_world_z;
use crate::feedback::{hold_glow_color, itg_actor_glow_alpha};
use crate::style::*;
use crate::transforms::{
    NoteAlphaParams, appearance_needs_rows, appearance_note_actor_alpha, appearance_note_glow,
};
use deadlib_present::actors::{Actor, SizeSpec, SpriteSource};
use deadlib_present::dsl::SpriteBuilder;
use deadlib_render::{BlendMode, TexturedMeshVertex};
use deadsync_core::note::NoteType;
use deadsync_core::song_time::SongTimeNs;
use deadsync_gameplay::let_go_head_beat as gameplay_let_go_head_beat;
use deadsync_noteskin::{HoldVisuals, NoteAnimPart, NoteskinSlot};
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

/// Renderer-neutral geometry sampled along one hold's transformed path.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HoldPathSample {
    pub adjusted_travel: f32,
    pub center_x: f32,
    pub world_z: f32,
    pub arrow_px: f32,
}

/// Canonical body and cap inputs after concrete noteskin/state resolution.
#[derive(Clone, Copy, Debug)]
pub struct HoldBodyCapRequest<'a, S> {
    pub body_slot: Option<&'a S>,
    pub top_cap_slot: Option<&'a S>,
    pub bottom_cap_slot: Option<&'a S>,
    pub y_head: f32,
    pub y_tail: f32,
    pub draw_span: Option<(f32, f32)>,
    pub body_flipped: bool,
    pub lane_reverse: bool,
    pub top_anchor_reverse: bool,
    pub body_phase: f32,
    pub top_cap_phase: f32,
    pub bottom_cap_phase: f32,
    pub body_uv_translation: [f32; 2],
    pub top_cap_uv_translation: [f32; 2],
    pub bottom_cap_uv_translation: [f32; 2],
    pub target_arrow_px: f32,
    pub diffuse: [f32; 4],
    pub elapsed_s: f32,
    pub mini: f32,
    pub lane_offset: f32,
    pub appearance: NoteAlphaParams,
    pub use_legacy_sprites: bool,
    pub rotation_y_deg: f32,
    pub depth_test: bool,
    pub screen_height: f32,
    pub body_z: i16,
    pub cap_z: i16,
    pub glow_z: i16,
}

/// Whether body/cap composition reached the hold-head stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldComposeControl {
    Continue,
    AbortHold,
}

#[derive(Default)]
struct RenderedHoldBody {
    top: Option<f32>,
    bottom: Option<f32>,
    head_row: Option<[[f32; 3]; 2]>,
    tail_row: Option<[[f32; 3]; 2]>,
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

/// Append one hold's body, top cap, and bottom cap in canonical ITG order.
///
/// Concrete asset owners inject sprite sources and a path sampler. The sampler
/// keeps theme/profile state outside this crate while canonical hold geometry,
/// UV clipping, actor ordering, and glow passes remain shared by every theme.
pub fn compose_hold_body_caps<S, F, P>(
    actors: &mut Vec<Actor>,
    request: HoldBodyCapRequest<'_, S>,
    sample_path: &P,
    sprite_source: &F,
) -> HoldComposeControl
where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
    P: Fn(f32) -> HoldPathSample,
{
    let rendered = compose_hold_body(actors, &request, sample_path, sprite_source);
    if compose_top_cap(actors, &request, &rendered, sample_path, sprite_source)
        == HoldComposeControl::AbortHold
    {
        return HoldComposeControl::AbortHold;
    }
    compose_bottom_cap(actors, &request, &rendered, sample_path, sprite_source)
}

fn hold_alpha<S>(request: &HoldBodyCapRequest<'_, S>, sample: HoldPathSample) -> f32 {
    appearance_note_actor_alpha(
        sample.adjusted_travel + request.lane_offset,
        request.elapsed_s,
        request.mini,
        request.appearance,
    )
}

fn hold_glow<S>(request: &HoldBodyCapRequest<'_, S>, sample: HoldPathSample) -> f32 {
    itg_actor_glow_alpha(appearance_note_glow(
        sample.adjusted_travel + request.lane_offset,
        request.elapsed_s,
        request.mini,
        request.appearance,
    ))
}

struct HoldSpritePass<'a, S> {
    slot: &'a S,
    center: [f32; 2],
    size: [f32; 2],
    uv: [f32; 4],
    rotation_y_deg: f32,
    rotation_z_deg: f32,
    diffuse: [f32; 4],
    alpha: f32,
    glow: f32,
    diffuse_z: i16,
    glow_z: i16,
    world_z: f32,
}

fn compose_hold_sprite<S, F>(
    actors: &mut Vec<Actor>,
    pass: HoldSpritePass<'_, S>,
    sprite_source: &F,
) where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
{
    if pass.alpha > f32::EPSILON {
        let mut actor = SpriteBuilder::with_source(sprite_source(pass.slot));
        actor.align(0.5, 0.5);
        actor.xy(pass.center[0], pass.center[1]);
        actor.size(pass.size[0], pass.size[1]);
        actor.rotationy(pass.rotation_y_deg);
        actor.rotationz(pass.rotation_z_deg);
        actor.customtexturerect(pass.uv);
        actor.diffuse([
            pass.diffuse[0],
            pass.diffuse[1],
            pass.diffuse[2],
            pass.diffuse[3] * pass.alpha,
        ]);
        actor.blend(BlendMode::Alpha);
        actor.z(pass.diffuse_z);
        actors.push(actor_with_world_z(actor.build(0), pass.world_z));
    }
    if pass.glow > f32::EPSILON {
        let mut actor = SpriteBuilder::with_source(sprite_source(pass.slot));
        actor.align(0.5, 0.5);
        actor.xy(pass.center[0], pass.center[1]);
        actor.size(pass.size[0], pass.size[1]);
        actor.rotationy(pass.rotation_y_deg);
        actor.rotationz(pass.rotation_z_deg);
        actor.customtexturerect(pass.uv);
        actor.diffuse([1.0, 1.0, 1.0, 0.0]);
        actor.glow([1.0, 1.0, 1.0, pass.glow]);
        actor.blend(BlendMode::Alpha);
        actor.z(pass.glow_z);
        actors.push(actor_with_world_z(actor.build(0), pass.world_z));
    }
}

fn compose_hold_body<S, F, P>(
    actors: &mut Vec<Actor>,
    request: &HoldBodyCapRequest<'_, S>,
    sample_path: &P,
    sprite_source: &F,
) -> RenderedHoldBody
where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
    P: Fn(f32) -> HoldPathSample,
{
    let Some((body_top, mut body_bottom)) = request.draw_span else {
        return RenderedHoldBody::default();
    };
    if let Some(cap_slot) = request.bottom_cap_slot {
        let cap_size = scale_cap_to_arrow(cap_slot.size(), request.target_arrow_px);
        body_bottom = hold_body_bottom_for_tail_cap(body_bottom, request.y_tail, cap_size[1]);
    }
    if request.y_tail <= request.y_head || body_bottom <= body_top {
        return RenderedHoldBody::default();
    }
    let Some(body_slot) = request.body_slot else {
        return RenderedHoldBody::default();
    };
    let texture_size = body_slot.size();
    let texture_width = texture_size[0].max(1) as f32;
    let texture_height = texture_size[1].max(1) as f32;
    if texture_width <= f32::EPSILON || texture_height <= f32::EPSILON {
        return RenderedHoldBody::default();
    }

    let body_frame = body_slot.frame_index_from_phase(request.body_phase);
    let body_width = request.target_arrow_px;
    let scale = body_width / texture_width;
    let segment_height = (texture_height * scale).max(f32::EPSILON);
    let uv_elapsed = if body_slot.model().is_some() {
        request.body_phase
    } else {
        request.elapsed_s
    };
    let body_uv = maybe_flip_uv_vert(
        translated_uv_rect(
            body_slot.uv_for_frame_at(body_frame, uv_elapsed),
            request.body_uv_translation,
        ),
        request.body_flipped,
    );
    let Some((clipped_top, clipped_bottom)) =
        clipped_hold_body_bounds(body_top, body_bottom, request.y_head, request.y_tail)
    else {
        return RenderedHoldBody::default();
    };
    let hold_length = request.y_tail - request.y_head;
    if hold_length <= f32::EPSILON {
        return RenderedHoldBody::default();
    }

    let visible_top_distance = clipped_top - request.y_head;
    let visible_bottom_distance = clipped_bottom - request.y_head;
    let visible_span = visible_bottom_distance - visible_top_distance;
    let (max_segments, allow_legacy_sprites) =
        hold_body_segment_budget(visible_span, segment_height);
    let phase_offset = hold_body_phase_offset(
        hold_length,
        segment_height,
        request.lane_reverse && request.top_anchor_reverse,
    );
    let phase = visible_top_distance / segment_height + phase_offset;
    let phase_end = visible_bottom_distance / segment_height + phase_offset;
    let uv = [body_uv[0], body_uv[2], body_uv[1], body_uv[3]];
    if request.use_legacy_sprites
        && allow_legacy_sprites
        && !appearance_needs_rows(request.appearance)
    {
        compose_legacy_hold_body(
            actors,
            request,
            body_slot,
            body_top,
            body_bottom,
            segment_height,
            phase,
            phase_end,
            max_segments,
            uv,
            phase_offset,
            sample_path,
            sprite_source,
        )
    } else {
        compose_sliced_hold_body(
            actors,
            request,
            body_slot,
            body_top,
            body_bottom,
            segment_height,
            phase,
            phase_end,
            max_segments,
            uv,
            phase_offset,
            sample_path,
            sprite_source,
        )
    }
}

const SEGMENT_PHASE_EPS: f32 = 1e-4;

fn hold_body_phase_offset(hold_length: f32, segment_height: f32, anchor_to_top: bool) -> f32 {
    if anchor_to_top {
        return 0.0;
    }
    let total_phase = hold_length / segment_height;
    if total_phase < 1.0 + SEGMENT_PHASE_EPS {
        return 0.0;
    }
    let fractional = total_phase.fract();
    if fractional > SEGMENT_PHASE_EPS && (1.0 - fractional) > SEGMENT_PHASE_EPS {
        1.0 - fractional
    } else {
        0.0
    }
}

fn next_body_phase(phase: f32, phase_end: f32) -> Option<f32> {
    let mut next = (phase.floor() + 1.0).min(phase_end);
    if next - phase < SEGMENT_PHASE_EPS {
        next = phase_end;
    }
    (next - phase >= SEGMENT_PHASE_EPS).then_some(next)
}

fn body_segment_v(
    phase: f32,
    next_phase: f32,
    v_top: f32,
    v_bottom: f32,
    segment_size: f32,
    segment_height: f32,
    body_bottom: f32,
    segment_bottom: f32,
    natural_bottom: f32,
    phase_end: f32,
) -> (f32, f32) {
    let v_range = v_bottom - v_top;
    let base_floor = phase.floor();
    let mut v0 = v_top + v_range * (phase - base_floor).clamp(0.0, 1.0);
    let mut v1 = v_top + v_range * (next_phase - base_floor).clamp(0.0, 1.0);
    let portion = (segment_size / segment_height).clamp(0.0, 1.0);
    let body_reaches_tail = (natural_bottom - body_bottom).max(0.0) <= segment_height + 1.0;
    let is_last_visible =
        (body_bottom - segment_bottom).abs() <= 0.5 || next_phase >= phase_end - SEGMENT_PHASE_EPS;
    if body_reaches_tail && is_last_visible {
        v1 = v_bottom;
        v0 = if v_range >= 0.0 {
            v_bottom - v_range.abs() * portion
        } else {
            v_bottom + v_range.abs() * portion
        };
    }
    (v0, v1)
}

#[allow(clippy::too_many_arguments)]
fn compose_legacy_hold_body<S, F, P>(
    actors: &mut Vec<Actor>,
    request: &HoldBodyCapRequest<'_, S>,
    slot: &S,
    body_top: f32,
    body_bottom: f32,
    segment_height: f32,
    mut phase: f32,
    phase_end: f32,
    max_segments: usize,
    uv: [f32; 4],
    phase_offset: f32,
    sample_path: &P,
    sprite_source: &F,
) -> RenderedHoldBody
where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
    P: Fn(f32) -> HoldPathSample,
{
    let [u0, u1, v_top, v_bottom] = uv;
    let mut rendered = RenderedHoldBody::default();
    let mut emitted = 0;
    while phase + SEGMENT_PHASE_EPS < phase_end && emitted < max_segments {
        let Some(next_phase) = next_body_phase(phase, phase_end) else {
            break;
        };
        let y_start = request.y_head + (phase - phase_offset) * segment_height;
        let y_end = request.y_head + (next_phase - phase_offset) * segment_height;
        let segment_top = y_start.max(body_top);
        let segment_bottom = y_end.min(body_bottom);
        if segment_bottom - segment_top <= f32::EPSILON {
            phase = next_phase;
            continue;
        }

        let segment_size = segment_bottom - segment_top;
        let (v0, v1) = body_segment_v(
            phase,
            next_phase,
            v_top,
            v_bottom,
            segment_size,
            segment_height,
            body_bottom,
            segment_bottom,
            request.y_tail,
            phase_end,
        );
        let center_y = (segment_top + segment_bottom) * 0.5;
        let sample = sample_path(center_y);
        let alpha = hold_alpha(request, sample);
        let glow = hold_glow(request, sample);
        if alpha > f32::EPSILON || glow > f32::EPSILON {
            rendered.top = Some(rendered.top.map_or(segment_top, |v| v.min(segment_top)));
            rendered.bottom = Some(
                rendered
                    .bottom
                    .map_or(segment_bottom, |v| v.max(segment_bottom)),
            );
            compose_hold_sprite(
                actors,
                HoldSpritePass {
                    slot,
                    center: [sample.center_x, center_y],
                    size: [request.target_arrow_px, segment_size],
                    uv: [u0, v0, u1, v1],
                    rotation_y_deg: request.rotation_y_deg,
                    rotation_z_deg: 0.0,
                    diffuse: request.diffuse,
                    alpha,
                    glow,
                    diffuse_z: request.body_z,
                    glow_z: request.glow_z,
                    world_z: sample.world_z,
                },
                sprite_source,
            );
        }
        phase = next_phase;
        emitted += 1;
    }
    rendered
}

#[allow(clippy::too_many_arguments)]
fn compose_sliced_hold_body<S, F, P>(
    actors: &mut Vec<Actor>,
    request: &HoldBodyCapRequest<'_, S>,
    slot: &S,
    body_top: f32,
    body_bottom: f32,
    segment_height: f32,
    mut phase: f32,
    phase_end: f32,
    max_segments: usize,
    uv: [f32; 4],
    phase_offset: f32,
    sample_path: &P,
    sprite_source: &F,
) -> RenderedHoldBody
where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
    P: Fn(f32) -> HoldPathSample,
{
    let [u0, u1, v_top, v_bottom] = uv;
    let slice_step = if request.depth_test { 4.0 } else { 16.0 };
    let use_mesh = slot.model().is_none() && request.rotation_y_deg.abs() <= f32::EPSILON;
    let mut diffuse_vertices: Option<Vec<TexturedMeshVertex>> = None;
    let mut glow_vertices: Option<Vec<TexturedMeshVertex>> = None;
    let mut prev_row: Option<[[f32; 3]; 2]> = None;
    let mut rendered = RenderedHoldBody::default();
    let mut emitted = 0;

    while phase + SEGMENT_PHASE_EPS < phase_end && emitted < max_segments {
        let Some(next_phase) = next_body_phase(phase, phase_end) else {
            break;
        };
        let y_start = request.y_head + (phase - phase_offset) * segment_height;
        let y_end = request.y_head + (next_phase - phase_offset) * segment_height;
        let segment_top = y_start.max(body_top);
        let segment_bottom = y_end.min(body_bottom);
        if segment_bottom - segment_top <= f32::EPSILON {
            phase = next_phase;
            continue;
        }

        let segment_size = segment_bottom - segment_top;
        let (v0, v1) = body_segment_v(
            phase,
            next_phase,
            v_top,
            v_bottom,
            segment_size,
            segment_height,
            body_bottom,
            segment_bottom,
            request.y_tail,
            phase_end,
        );
        let mut slice_top = segment_top;
        while slice_top + f32::EPSILON < segment_bottom {
            let slice_bottom = (slice_top + slice_step).min(segment_bottom);
            let slice_size = slice_bottom - slice_top;
            if slice_size <= f32::EPSILON {
                break;
            }
            let t0 = ((slice_top - segment_top) / segment_size).clamp(0.0, 1.0);
            let t1 = ((slice_bottom - segment_top) / segment_size).clamp(0.0, 1.0);
            let slice_v0 = (v1 - v0).mul_add(t0, v0);
            let slice_v1 = (v1 - v0).mul_add(t1, v0);
            let center_y = (slice_top + slice_bottom) * 0.5;
            let center = sample_path(center_y);
            let alpha = hold_alpha(request, center);
            let glow = hold_glow(request, center);
            if alpha <= f32::EPSILON && glow <= f32::EPSILON {
                prev_row = None;
                slice_top = slice_bottom;
                continue;
            }

            let top = sample_path(slice_top);
            let bottom = sample_path(slice_bottom);
            let (center_xy, slice_height, rotation_z) =
                hold_segment_pose([top.center_x, slice_top], [bottom.center_x, slice_bottom]);
            if slice_height <= f32::EPSILON {
                slice_top = slice_bottom;
                continue;
            }
            rendered.top = Some(rendered.top.map_or(slice_top, |v| v.min(slice_top)));
            rendered.bottom = Some(
                rendered
                    .bottom
                    .map_or(slice_bottom, |v| v.max(slice_bottom)),
            );

            if use_mesh {
                append_hold_body_mesh_slice(
                    request,
                    u0,
                    u1,
                    slice_v0,
                    slice_v1,
                    top,
                    bottom,
                    slice_top,
                    slice_bottom,
                    &mut prev_row,
                    &mut rendered,
                    &mut diffuse_vertices,
                    &mut glow_vertices,
                );
            } else {
                compose_hold_sprite(
                    actors,
                    HoldSpritePass {
                        slot,
                        center: center_xy,
                        size: [request.target_arrow_px, slice_height],
                        uv: [u0, slice_v0, u1, slice_v1],
                        rotation_y_deg: request.rotation_y_deg,
                        rotation_z_deg: rotation_z,
                        diffuse: request.diffuse,
                        alpha,
                        glow,
                        diffuse_z: request.body_z,
                        glow_z: request.glow_z,
                        world_z: center.world_z,
                    },
                    sprite_source,
                );
            }
            slice_top = slice_bottom;
        }
        phase = next_phase;
        emitted += 1;
    }

    if let Some(vertices) = diffuse_vertices.filter(|vertices| !vertices.is_empty()) {
        actors.push(hold_strip_actor(
            slot.texture_key_shared(),
            Arc::from(vertices),
            BlendMode::Alpha,
            request.depth_test,
            request.body_z,
        ));
    }
    if let Some(vertices) = glow_vertices.filter(|vertices| !vertices.is_empty()) {
        actors.push(hold_strip_glow_actor(
            slot.texture_key_shared(),
            Arc::from(vertices),
            request.depth_test,
            request.glow_z,
        ));
    }
    rendered
}

#[allow(clippy::too_many_arguments)]
fn append_hold_body_mesh_slice<S>(
    request: &HoldBodyCapRequest<'_, S>,
    u0: f32,
    u1: f32,
    v0: f32,
    v1: f32,
    top: HoldPathSample,
    bottom: HoldPathSample,
    top_y: f32,
    bottom_y: f32,
    prev_row: &mut Option<[[f32; 3]; 2]>,
    rendered: &mut RenderedHoldBody,
    diffuse_vertices: &mut Option<Vec<TexturedMeshVertex>>,
    glow_vertices: &mut Option<Vec<TexturedMeshVertex>>,
) {
    let top_alpha = hold_alpha(request, top);
    let bottom_alpha = hold_alpha(request, bottom);
    let top_glow = hold_glow(request, top);
    let bottom_glow = hold_glow(request, bottom);
    let forward = [bottom.center_x - top.center_x, bottom_y - top_y];
    let top_row_positions = prev_row.unwrap_or_else(|| {
        let row = hold_strip_row_3d(
            [top.center_x, top_y, top.world_z],
            forward,
            top.arrow_px * 0.5,
            u0,
            u1,
            v0,
            [
                request.diffuse[0],
                request.diffuse[1],
                request.diffuse[2],
                request.diffuse[3] * top_alpha,
            ],
        );
        [row[0].pos, row[1].pos]
    });
    let top_row = hold_strip_row_from_positions(
        top_row_positions[0],
        top_row_positions[1],
        u0,
        u1,
        v0,
        [
            request.diffuse[0],
            request.diffuse[1],
            request.diffuse[2],
            request.diffuse[3] * top_alpha,
        ],
    );
    if rendered.head_row.is_none() {
        rendered.head_row = Some([top_row[0].pos, top_row[1].pos]);
    }
    let bottom_row = hold_strip_row_3d(
        [bottom.center_x, bottom_y, bottom.world_z],
        forward,
        bottom.arrow_px * 0.5,
        u0,
        u1,
        v1,
        [
            request.diffuse[0],
            request.diffuse[1],
            request.diffuse[2],
            request.diffuse[3] * bottom_alpha,
        ],
    );
    if top_alpha > f32::EPSILON || bottom_alpha > f32::EPSILON {
        diffuse_vertices
            .get_or_insert_with(|| Vec::with_capacity(96))
            .extend_from_slice(&hold_strip_quad(top_row, bottom_row));
    }
    if top_glow > f32::EPSILON || bottom_glow > f32::EPSILON {
        let top_glow_row = hold_strip_row_from_positions(
            top_row[0].pos,
            top_row[1].pos,
            u0,
            u1,
            v0,
            hold_glow_color(top_glow),
        );
        let bottom_glow_row = hold_strip_row_from_positions(
            bottom_row[0].pos,
            bottom_row[1].pos,
            u0,
            u1,
            v1,
            hold_glow_color(bottom_glow),
        );
        glow_vertices
            .get_or_insert_with(|| Vec::with_capacity(96))
            .extend_from_slice(&hold_strip_quad(top_glow_row, bottom_glow_row));
    }
    rendered.tail_row = Some([bottom_row[0].pos, bottom_row[1].pos]);
    *prev_row = Some([bottom_row[0].pos, bottom_row[1].pos]);
}

fn compose_top_cap<S, F, P>(
    actors: &mut Vec<Actor>,
    request: &HoldBodyCapRequest<'_, S>,
    rendered: &RenderedHoldBody,
    sample_path: &P,
    sprite_source: &F,
) -> HoldComposeControl
where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
    P: Fn(f32) -> HoldPathSample,
{
    if request.draw_span.is_none() {
        return HoldComposeControl::Continue;
    }
    let Some(slot) = request.top_cap_slot else {
        return HoldComposeControl::Continue;
    };
    if request.y_head <= -400.0 || request.y_head >= request.screen_height + 400.0 {
        return HoldComposeControl::Continue;
    }

    let frame = slot.frame_index_from_phase(request.top_cap_phase);
    let uv_elapsed = if slot.model().is_some() {
        request.top_cap_phase
    } else {
        request.elapsed_s
    };
    let uv = maybe_mirror_uv_horiz_for_reverse_flipped(
        maybe_flip_uv_vert(
            translated_uv_rect(
                slot.uv_for_frame_at(frame, uv_elapsed),
                request.top_cap_uv_translation,
            ),
            request.body_flipped,
        ),
        request.lane_reverse,
        request.body_flipped,
    );
    let cap_size = scale_cap_to_arrow(slot.size(), request.target_arrow_px);
    let cap_width = cap_size[0];
    let mut cap_height = cap_size[1];
    let [u0, v0, u1, mut v1] = uv;
    let cap_top = request.y_head - cap_height;
    let mut cap_bottom = request.y_head;
    if cap_height > f32::EPSILON && request.y_tail < cap_bottom {
        let trimmed = (cap_bottom - request.y_tail).clamp(0.0, cap_height);
        if trimmed >= cap_height - f32::EPSILON {
            cap_height = 0.0;
        } else if trimmed > f32::EPSILON {
            v1 -= (v1 - v0) * (trimmed / cap_height);
            cap_bottom -= trimmed;
            cap_height = cap_bottom - cap_top;
        }
    }
    if cap_height <= f32::EPSILON {
        return HoldComposeControl::Continue;
    }

    let center_y = (cap_top + cap_bottom) * 0.5;
    let center = sample_path(center_y);
    let alpha = hold_alpha(request, center);
    let glow = hold_glow(request, center);
    if alpha <= f32::EPSILON && glow <= f32::EPSILON {
        return HoldComposeControl::AbortHold;
    }
    let top = sample_path(cap_top);
    let bottom = sample_path(cap_bottom);
    let (center_xy, draw_height, path_rotation) =
        hold_segment_pose([top.center_x, cap_top], [bottom.center_x, cap_bottom]);
    if draw_height <= f32::EPSILON {
        return HoldComposeControl::AbortHold;
    }

    let use_mesh = !request.use_legacy_sprites
        && slot.model().is_none()
        && request.rotation_y_deg.abs() <= f32::EPSILON;
    if use_mesh {
        let top_alpha = hold_alpha(request, top);
        let bottom_alpha = hold_alpha(request, bottom);
        let top_glow = hold_glow(request, top);
        let bottom_glow = hold_glow(request, bottom);
        let forward = [bottom.center_x - top.center_x, cap_bottom - cap_top];
        let top_row = hold_strip_row_3d(
            [top.center_x, cap_top, top.world_z],
            forward,
            top.arrow_px * 0.5,
            u0,
            u1,
            v0,
            [
                request.diffuse[0],
                request.diffuse[1],
                request.diffuse[2],
                request.diffuse[3] * top_alpha,
            ],
        );
        let bottom_row = if let Some(body_head_row) = rendered.head_row
            && rendered
                .top
                .is_some_and(|body_top| (body_top - cap_bottom).abs() <= 2.0)
        {
            hold_strip_row_from_positions(
                body_head_row[0],
                body_head_row[1],
                u0,
                u1,
                v1,
                [
                    request.diffuse[0],
                    request.diffuse[1],
                    request.diffuse[2],
                    request.diffuse[3] * bottom_alpha,
                ],
            )
        } else {
            hold_strip_row_3d(
                [bottom.center_x, cap_bottom, bottom.world_z],
                forward,
                bottom.arrow_px * 0.5,
                u0,
                u1,
                v1,
                [
                    request.diffuse[0],
                    request.diffuse[1],
                    request.diffuse[2],
                    request.diffuse[3] * bottom_alpha,
                ],
            )
        };
        compose_cap_mesh(
            actors,
            slot,
            top_row,
            bottom_row,
            [u0, v0, u1, v1],
            top_glow,
            bottom_glow,
            top_alpha,
            bottom_alpha,
            request,
        );
    } else {
        compose_hold_sprite(
            actors,
            HoldSpritePass {
                slot,
                center: center_xy,
                size: [cap_width, draw_height],
                uv: [u0, v0, u1, v1],
                rotation_y_deg: request.rotation_y_deg,
                rotation_z_deg: path_rotation
                    + top_cap_rotation_deg(request.lane_reverse, request.body_flipped),
                diffuse: request.diffuse,
                alpha,
                glow,
                diffuse_z: request.cap_z,
                glow_z: request.glow_z,
                world_z: center.world_z,
            },
            sprite_source,
        );
    }
    HoldComposeControl::Continue
}

#[allow(clippy::too_many_arguments)]
fn compose_cap_mesh<S>(
    actors: &mut Vec<Actor>,
    slot: &S,
    top_row: [TexturedMeshVertex; 2],
    bottom_row: [TexturedMeshVertex; 2],
    uv: [f32; 4],
    top_glow: f32,
    bottom_glow: f32,
    top_alpha: f32,
    bottom_alpha: f32,
    request: &HoldBodyCapRequest<'_, S>,
) where
    S: NoteskinSlot,
{
    if top_alpha > f32::EPSILON || bottom_alpha > f32::EPSILON {
        actors.push(hold_strip_actor(
            slot.texture_key_shared(),
            Arc::new(hold_strip_quad(top_row, bottom_row)),
            BlendMode::Alpha,
            request.depth_test,
            request.cap_z,
        ));
    }
    if top_glow > f32::EPSILON || bottom_glow > f32::EPSILON {
        let [u0, v0, u1, v1] = uv;
        let top_glow_row = hold_strip_row_from_positions(
            top_row[0].pos,
            top_row[1].pos,
            u0,
            u1,
            v0,
            hold_glow_color(top_glow),
        );
        let bottom_glow_row = hold_strip_row_from_positions(
            bottom_row[0].pos,
            bottom_row[1].pos,
            u0,
            u1,
            v1,
            hold_glow_color(bottom_glow),
        );
        actors.push(hold_strip_glow_actor(
            slot.texture_key_shared(),
            Arc::new(hold_strip_quad(top_glow_row, bottom_glow_row)),
            request.depth_test,
            request.glow_z,
        ));
    }
}

fn compose_bottom_cap<S, F, P>(
    actors: &mut Vec<Actor>,
    request: &HoldBodyCapRequest<'_, S>,
    rendered: &RenderedHoldBody,
    sample_path: &P,
    sprite_source: &F,
) -> HoldComposeControl
where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
    P: Fn(f32) -> HoldPathSample,
{
    if request.draw_span.is_none() {
        return HoldComposeControl::Continue;
    }
    let Some(slot) = request.bottom_cap_slot else {
        return HoldComposeControl::Continue;
    };
    let tail_position = request.y_tail + 1.0;
    if tail_position <= -400.0 || tail_position >= request.screen_height + 400.0 {
        return HoldComposeControl::Continue;
    }

    let frame = slot.frame_index_from_phase(request.bottom_cap_phase);
    let uv_elapsed = if slot.model().is_some() {
        request.bottom_cap_phase
    } else {
        request.elapsed_s
    };
    let uv = maybe_mirror_uv_horiz_for_reverse_flipped(
        maybe_flip_uv_vert(
            translated_uv_rect(
                slot.uv_for_frame_at(frame, uv_elapsed),
                request.bottom_cap_uv_translation,
            ),
            request.body_flipped,
        ),
        request.lane_reverse,
        request.body_flipped,
    );
    let cap_size = scale_cap_to_arrow(slot.size(), request.target_arrow_px);
    let cap_width = cap_size[0];
    let cap_span = cap_size[1];
    let [u0, base_v0, u1, base_v1] = uv;
    let Some((raw_top, draw_bottom)) =
        hold_tail_cap_bounds(tail_position, cap_span, rendered.top, rendered.bottom)
    else {
        return HoldComposeControl::AbortHold;
    };
    if cap_span <= f32::EPSILON {
        return HoldComposeControl::AbortHold;
    }
    let draw_top = if request.y_head > raw_top {
        request.y_head.min(draw_bottom)
    } else {
        raw_top
    };
    let draw_height = draw_bottom - draw_top;
    let Some((v0, v1)) = bottom_cap_uv_window(
        base_v0,
        base_v1,
        draw_height,
        cap_span,
        request.lane_reverse && request.top_anchor_reverse,
    ) else {
        return HoldComposeControl::AbortHold;
    };
    if !(draw_height > f32::EPSILON) {
        return HoldComposeControl::Continue;
    }

    let center_y = (draw_top + draw_bottom) * 0.5;
    let center = sample_path(center_y);
    let alpha = hold_alpha(request, center);
    let glow = hold_glow(request, center);
    if alpha <= f32::EPSILON && glow <= f32::EPSILON {
        return HoldComposeControl::AbortHold;
    }
    let top = sample_path(draw_top);
    let bottom = sample_path(draw_bottom);
    let (center_xy, cap_draw_height, rotation_z) =
        hold_segment_pose([top.center_x, draw_top], [bottom.center_x, draw_bottom]);
    if cap_draw_height <= f32::EPSILON {
        return HoldComposeControl::AbortHold;
    }

    let use_mesh = !request.use_legacy_sprites
        && slot.model().is_none()
        && !request.lane_reverse
        && request.rotation_y_deg.abs() <= f32::EPSILON;
    if use_mesh {
        let top_alpha = hold_alpha(request, top);
        let bottom_alpha = hold_alpha(request, bottom);
        let top_glow = hold_glow(request, top);
        let bottom_glow = hold_glow(request, bottom);
        let forward = [bottom.center_x - top.center_x, draw_bottom - draw_top];
        let top_row = if let Some(body_tail_row) = rendered.tail_row {
            hold_strip_row_from_positions(
                body_tail_row[0],
                body_tail_row[1],
                u0,
                u1,
                v0,
                [
                    request.diffuse[0],
                    request.diffuse[1],
                    request.diffuse[2],
                    request.diffuse[3] * top_alpha,
                ],
            )
        } else {
            hold_strip_row_3d(
                [top.center_x, draw_top, top.world_z],
                forward,
                top.arrow_px * 0.5,
                u0,
                u1,
                v0,
                [
                    request.diffuse[0],
                    request.diffuse[1],
                    request.diffuse[2],
                    request.diffuse[3] * top_alpha,
                ],
            )
        };
        let bottom_row = hold_strip_row_3d(
            [bottom.center_x, draw_bottom, bottom.world_z],
            forward,
            bottom.arrow_px * 0.5,
            u0,
            u1,
            v1,
            [
                request.diffuse[0],
                request.diffuse[1],
                request.diffuse[2],
                request.diffuse[3] * bottom_alpha,
            ],
        );
        compose_cap_mesh(
            actors,
            slot,
            top_row,
            bottom_row,
            [u0, v0, u1, v1],
            top_glow,
            bottom_glow,
            top_alpha,
            bottom_alpha,
            request,
        );
    } else {
        compose_hold_sprite(
            actors,
            HoldSpritePass {
                slot,
                center: center_xy,
                size: [cap_width, cap_draw_height],
                uv: [u0, v0, u1, v1],
                rotation_y_deg: request.rotation_y_deg,
                rotation_z_deg: rotation_z,
                diffuse: request.diffuse,
                alpha,
                glow,
                diffuse_z: request.cap_z,
                glow_z: request.glow_z,
                world_z: center.world_z,
            },
            sprite_source,
        );
    }
    HoldComposeControl::Continue
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

pub(crate) fn maybe_flip_uv_vert(mut uv: [f32; 4], flip: bool) -> [f32; 4] {
    if flip {
        uv.swap(1, 3);
    }
    uv
}

pub(crate) const fn maybe_mirror_uv_horiz_for_reverse_flipped(
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

pub(crate) const fn top_cap_rotation_deg(lane_reverse: bool, body_flipped: bool) -> f32 {
    if lane_reverse && body_flipped {
        180.0
    } else {
        0.0
    }
}

pub(crate) fn scale_effect_size(
    logical_size: [f32; 2],
    field_zoom: f32,
    effect_zoom: f32,
) -> [f32; 2] {
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

pub(crate) fn scale_cap_to_arrow(size: [i32; 2], target_arrow_px: f32) -> [f32; 2] {
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

pub(crate) fn hold_tail_cap_bounds(
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

pub(crate) fn clipped_hold_body_bounds(
    body_top: f32,
    body_bottom: f32,
    natural_top: f32,
    natural_bottom: f32,
) -> Option<(f32, f32)> {
    let top = body_top.max(natural_top);
    let bottom = body_bottom.min(natural_bottom);
    (bottom > top).then_some((top, bottom))
}

pub(crate) fn hold_body_bottom_for_tail_cap(body_bottom: f32, y_tail: f32, cap_height: f32) -> f32 {
    if cap_height > 0.0 && body_bottom >= y_tail - 1.0 {
        y_tail + 1.0
    } else {
        body_bottom
    }
}

pub(crate) fn hold_draw_span(y_head: f32, y_tail: f32, screen_height: f32) -> Option<(f32, f32)> {
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

pub(crate) fn hold_body_segment_budget(visible_span: f32, segment_height: f32) -> (usize, bool) {
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

pub(crate) fn hold_strip_row_3d(
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

pub(crate) fn hold_strip_row_from_positions(
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

pub(crate) fn hold_strip_quad(
    top: [TexturedMeshVertex; 2],
    bottom: [TexturedMeshVertex; 2],
) -> [TexturedMeshVertex; 6] {
    [top[0], top[1], bottom[1], top[0], bottom[1], bottom[0]]
}

pub(crate) fn hold_strip_actor(
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

pub(crate) fn hold_strip_glow_actor(
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

pub(crate) fn bottom_cap_uv_window(
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

pub(crate) fn hold_segment_pose(top: [f32; 2], bottom: [f32; 2]) -> ([f32; 2], f32, f32) {
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

pub(crate) fn song_time_ns_delta_seconds(lhs: SongTimeNs, rhs: SongTimeNs) -> f32 {
    ((lhs as i128 - rhs as i128) as f64 * 1.0e-9) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_noteskin::{ModelDrawState, ModelMesh, ModelVertex, SpriteDefinition};
    use std::cell::Cell;

    struct TestSlot {
        def: SpriteDefinition,
        model: Option<ModelMesh>,
        texture: Arc<str>,
        uv: [f32; 4],
        uv_elapsed: Cell<f32>,
        uv_calls: Cell<usize>,
    }

    impl TestSlot {
        fn sprite(texture: &str) -> Self {
            Self {
                def: SpriteDefinition {
                    size: [64, 64],
                    ..SpriteDefinition::default()
                },
                model: None,
                texture: Arc::from(texture),
                uv: [0.1, 0.2, 0.9, 0.8],
                uv_elapsed: Cell::new(f32::NAN),
                uv_calls: Cell::new(0),
            }
        }

        fn model(texture: &str) -> Self {
            Self {
                model: Some(ModelMesh {
                    vertices: Arc::from([ModelVertex {
                        pos: [0.0, 0.0, 0.0],
                        uv: [0.0, 0.0],
                        tex_matrix_scale: [1.0, 1.0],
                    }]),
                    bounds: [0.0, 0.0, 0.0, 64.0, 64.0, 0.0],
                }),
                ..Self::sprite(texture)
            }
        }
    }

    impl NoteskinSlot for TestSlot {
        fn sprite_def(&self) -> &SpriteDefinition {
            &self.def
        }

        fn source_size(&self) -> [i32; 2] {
            [64, 64]
        }

        fn texture_key_shared(&self) -> Arc<str> {
            Arc::clone(&self.texture)
        }

        fn model(&self) -> Option<&ModelMesh> {
            self.model.as_ref()
        }

        fn base_rot_sin_cos(&self) -> [f32; 2] {
            [0.0, 1.0]
        }

        fn frame_index(&self, _time: f32, _beat: f32) -> usize {
            0
        }

        fn frame_index_from_phase(&self, _phase: f32) -> usize {
            0
        }

        fn uv_for_frame_at(&self, _frame_index: usize, elapsed: f32) -> [f32; 4] {
            self.uv_elapsed.set(elapsed);
            self.uv_calls.set(self.uv_calls.get() + 1);
            self.uv
        }

        fn model_draw_at(&self, _time: f32, _beat: f32) -> ModelDrawState {
            ModelDrawState::default()
        }

        fn model_glow_with_draw(
            &self,
            _draw: ModelDrawState,
            _time: f32,
            _beat: f32,
            _diffuse_alpha: f32,
        ) -> Option<[f32; 4]> {
            None
        }

        fn model_uv_params(&self, uv: [f32; 4]) -> ([f32; 2], [f32; 2], [f32; 2]) {
            ([uv[2] - uv[0], uv[3] - uv[1]], [uv[0], uv[1]], [0.0, 0.0])
        }
    }

    fn body_cap_request<'a>(
        body: Option<&'a TestSlot>,
        top: Option<&'a TestSlot>,
        bottom: Option<&'a TestSlot>,
    ) -> HoldBodyCapRequest<'a, TestSlot> {
        HoldBodyCapRequest {
            body_slot: body,
            top_cap_slot: top,
            bottom_cap_slot: bottom,
            y_head: 100.0,
            y_tail: 164.0,
            draw_span: Some((100.0, 164.0)),
            body_flipped: false,
            lane_reverse: false,
            top_anchor_reverse: false,
            body_phase: 2.0,
            top_cap_phase: 3.0,
            bottom_cap_phase: 4.0,
            body_uv_translation: [0.0, 0.0],
            top_cap_uv_translation: [0.0, 0.0],
            bottom_cap_uv_translation: [0.0, 0.0],
            target_arrow_px: 64.0,
            diffuse: [0.5, 0.5, 0.5, 1.0],
            elapsed_s: 9.0,
            mini: 0.0,
            lane_offset: 0.0,
            appearance: NoteAlphaParams {
                stealth: 0.25,
                ..NoteAlphaParams::default()
            },
            use_legacy_sprites: true,
            rotation_y_deg: 0.0,
            depth_test: false,
            screen_height: 480.0,
            body_z: 110,
            cap_z: 110,
            glow_z: 111,
        }
    }

    fn straight_path(y: f32) -> HoldPathSample {
        HoldPathSample {
            adjusted_travel: y,
            center_x: 32.0,
            world_z: y * 0.1,
            arrow_px: 64.0,
        }
    }

    fn test_source(slot: &TestSlot) -> SpriteSource {
        SpriteSource::Texture(Arc::clone(&slot.texture))
    }

    fn sprite_key(actor: &Actor) -> &str {
        let Actor::Sprite { source, .. } = actor else {
            panic!("expected sprite actor");
        };
        source.texture_key().expect("sprite texture")
    }

    fn actor_z(actor: &Actor) -> i16 {
        match actor {
            Actor::Sprite { z, .. } | Actor::TexturedMesh { z, .. } => *z,
            _ => panic!("expected drawable hold actor"),
        }
    }

    #[test]
    fn legacy_body_and_caps_preserve_diffuse_glow_order() {
        let body = TestSlot::sprite("body");
        let top = TestSlot::sprite("top");
        let bottom = TestSlot::sprite("bottom");
        let mut actors = Vec::new();

        let control = compose_hold_body_caps(
            &mut actors,
            body_cap_request(Some(&body), Some(&top), Some(&bottom)),
            &straight_path,
            &test_source,
        );

        assert_eq!(control, HoldComposeControl::Continue);
        assert_eq!(actors.len(), 6);
        assert_eq!(
            actors.iter().map(sprite_key).collect::<Vec<_>>(),
            ["body", "body", "top", "top", "bottom", "bottom"]
        );
        assert_eq!(
            actors.iter().map(actor_z).collect::<Vec<_>>(),
            [110, 111, 110, 111, 110, 111]
        );
        let Actor::Sprite {
            tint,
            glow,
            world_z,
            blend,
            ..
        } = &actors[1]
        else {
            panic!("body glow should be a sprite");
        };
        assert_eq!(*tint, [1.0, 1.0, 1.0, 0.0]);
        assert!(glow[3] > 0.0);
        assert!(*world_z > 0.0);
        assert_eq!(*blend, BlendMode::Alpha);
    }

    #[test]
    fn sliced_body_mesh_preserves_depth_and_vertex_world_z() {
        let body = TestSlot::sprite("body-mesh");
        let mut request = body_cap_request(Some(&body), None, None);
        request.use_legacy_sprites = false;
        request.depth_test = true;
        let mut actors = Vec::new();

        compose_hold_body_caps(&mut actors, request, &straight_path, &test_source);

        assert_eq!(actors.len(), 2);
        assert_eq!(actors.iter().map(actor_z).collect::<Vec<_>>(), [110, 111]);
        for actor in &actors {
            let Actor::TexturedMesh {
                vertices,
                depth_test,
                world_z,
                blend,
                ..
            } = actor
            else {
                panic!("deformed sprite hold should use a strip mesh");
            };
            assert!(*depth_test);
            assert_eq!(*world_z, 0.0);
            assert_eq!(*blend, BlendMode::Alpha);
            assert!(vertices.iter().any(|vertex| vertex.pos[2] > 0.0));
        }
    }

    #[test]
    fn model_body_uses_phase_clock_and_sliced_sprite_fallback() {
        let body = TestSlot::model("model-body");
        let mut request = body_cap_request(Some(&body), None, None);
        request.use_legacy_sprites = false;
        let source_calls = Cell::new(0);
        let mut actors = Vec::new();

        compose_hold_body_caps(&mut actors, request, &straight_path, &|slot| {
            source_calls.set(source_calls.get() + 1);
            test_source(slot)
        });

        assert!(!actors.is_empty());
        assert!(
            actors
                .iter()
                .all(|actor| matches!(actor, Actor::Sprite { .. }))
        );
        assert_eq!(source_calls.get(), actors.len());
        assert_eq!(body.uv_elapsed.get(), 2.0);
        assert!(actors.iter().all(|actor| sprite_key(actor) == "model-body"));
    }

    #[test]
    fn reverse_flipped_caps_preserve_uv_mirror_and_top_rotation() {
        let top = TestSlot::sprite("top");
        let bottom = TestSlot::sprite("bottom");
        let mut request = body_cap_request(None, Some(&top), Some(&bottom));
        request.body_flipped = true;
        request.lane_reverse = true;
        request.top_anchor_reverse = true;
        request.top_cap_uv_translation = [0.01, 0.02];
        request.bottom_cap_uv_translation = [0.01, 0.02];
        let mut actors = Vec::new();

        compose_hold_body_caps(&mut actors, request, &straight_path, &test_source);

        assert_eq!(actors.len(), 4);
        let Actor::Sprite {
            uv_rect: top_uv,
            rot_z_deg: top_rotation,
            ..
        } = &actors[0]
        else {
            panic!("top cap should use reverse-safe sprite fallback");
        };
        let Actor::Sprite {
            uv_rect: bottom_uv,
            rot_z_deg: bottom_rotation,
            ..
        } = &actors[2]
        else {
            panic!("bottom cap should use reverse-safe sprite fallback");
        };
        for actual in [top_uv.expect("top UV"), bottom_uv.expect("bottom UV")] {
            for (actual, expected) in actual.into_iter().zip([0.91, 0.82, 0.11, 0.22]) {
                assert!((actual - expected).abs() <= 1e-6);
            }
        }
        assert_eq!(*top_rotation, 180.0);
        assert_eq!(*bottom_rotation, 0.0);
    }

    #[test]
    fn invisible_top_cap_aborts_before_bottom_and_head_stage() {
        let body = TestSlot::sprite("body");
        let top = TestSlot::sprite("top");
        let bottom = TestSlot::sprite("bottom");
        let mut request = body_cap_request(Some(&body), Some(&top), Some(&bottom));
        request.appearance = NoteAlphaParams {
            hidden: 1.0,
            ..NoteAlphaParams::default()
        };
        let mut actors = Vec::new();

        let control = compose_hold_body_caps(&mut actors, request, &straight_path, &test_source);

        assert_eq!(control, HoldComposeControl::AbortHold);
        assert!(
            !actors.is_empty(),
            "visible body slices should remain emitted"
        );
        assert_eq!(top.uv_calls.get(), 1);
        assert_eq!(bottom.uv_calls.get(), 0);
    }

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
