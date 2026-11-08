//! Noteskin pieces drawn outside the gameplay notefield.
//!
//! Player Options previews each skin component from these helpers, and the
//! Content Browser's chart preview composes the same pieces into a small field
//! of the player's real noteskin. Every helper takes its clocks explicitly
//! (`time` in seconds, `beat` in song beats) so each screen owns its preview
//! timing, and appends to the caller's actor list without allocating storage
//! of its own.
//!
//! Note previews preserve authored proportions at the same field scale as taps.
//! Other menu helpers fit their component to the requested icon bounds.
//! The field helpers (`draw_tap`, `draw_hold`, `draw_mine`, `draw_receptor`,
//! `draw_explosion`) scale every layer by gameplay's field zoom, so a
//! receptor glow or an explosion keeps its authored size relative to the arrow
//! exactly as the notefield draws it.
//!
//! The field helpers also take the caller's `ModelMeshCache`, because a field
//! draws the same few model slots dozens of times a frame: with one, a slot's
//! geometry is built once and shared under a stable key, as the notefield's
//! is. The menu helpers draw a handful of icons and build it per draw, which
//! is what Player Options has always done.

use crate::act;
use deadlib_present::actors::Actor;
use deadlib_render_core::BlendMode;
use deadsync_assets::noteskin::{
    NUM_QUANTIZATIONS, NoteAnimPart, Noteskin, Quantization, SpriteSlot,
};
use deadsync_notefield::{
    ModelMeshCache, noteskin_model_actor_from_draw, noteskin_model_actor_from_draw_cached,
};
use deadsync_noteskin::{
    ModelDrawState, NoteskinSlot, ReceptorGlowBehavior, ReceptorIdleGlow, TapExplosion,
};
use deadsync_rules::scroll::ScrollSpeedSetting;

/// Player Options loops its tap-explosion icon at this fraction of real time.
pub(crate) const TAP_EXPLOSION_PREVIEW_SPEED: f32 = 0.7;

/// Gameplay's field unit. Noteskins are authored against a 64-pixel arrow and
/// the notefield scales every layer by `arrow_px / 64`.
const FIELD_UNIT_PX: f32 = ScrollSpeedSetting::ARROW_SPACING;

/// The first beat offset that quantizes to each `Quantization` (4th, 8th, 12th,
/// 16th, 24th, 32nd, 48th, 64th, 192nd). The runtime derives note color and
/// vivid phase from a note's beat, so a field note placed at one of these
/// offsets samples the same strip and frame gameplay draws for that quant.
const QUANT_NOTE_BEATS: [f32; NUM_QUANTIZATIONS] = [
    0.0,
    24.0 / 48.0,
    16.0 / 48.0,
    12.0 / 48.0,
    8.0 / 48.0,
    6.0 / 48.0,
    4.0 / 48.0,
    3.0 / 48.0,
    1.0 / 48.0,
];

/// How long a field hit keeps its lane down before releasing: a quick human
/// tap. The receptor holds the skin's Press state until then and plays Lift
/// after, the same sequence gameplay runs from a step's input edges.
const RECEPTOR_PRESS_HOLD_SECONDS: f32 = 0.09;

/// Every layer authored for a tap or lift. Lifts fall back to the tap layers
/// and both fall back to the legacy single-sprite note when a skin has no
/// layered note for `index` (`col * NUM_QUANTIZATIONS + quant`).
pub(crate) fn preview_note_slots(
    skin: &Noteskin,
    part: NoteAnimPart,
    index: usize,
) -> &[SpriteSlot] {
    let layers = if part == NoteAnimPart::Lift {
        skin.lift_note_layers.get(index)
    } else {
        None
    };
    layers
        .or_else(|| skin.note_layers.get(index))
        .map(AsRef::as_ref)
        .or_else(|| skin.notes.get(index).map(std::slice::from_ref))
        .unwrap_or_default()
}

/// Every slot the field helpers can draw in columns `0..cols`: taps and lifts
/// at every quantization, hold and roll heads, bodies and bottom caps, the
/// mine, the receptor's layers and the W1 explosion. What a caller must have
/// ready -- textures resident, model geometry built -- before it draws a
/// field, and nothing a field never draws (top caps, other judgments'
/// explosions). A slot two pieces share is visited once for each; callers
/// dedupe.
pub(crate) fn for_each_field_slot<'a>(
    skin: &'a Noteskin,
    cols: usize,
    mut visit: impl FnMut(&'a SpriteSlot),
) {
    for col in 0..cols {
        let col = skin_col(skin, col);
        for part in [NoteAnimPart::Tap, NoteAnimPart::Lift] {
            for quant in 0..NUM_QUANTIZATIONS {
                preview_note_slots(skin, part, col * NUM_QUANTIZATIONS + quant)
                    .iter()
                    .for_each(&mut visit);
            }
        }
        for roll in [false, true] {
            let visuals = skin.hold_visuals_for_col(col, roll);
            for layers in [&visuals.head_inactive_layers, &visuals.head_active_layers]
                .into_iter()
                .flatten()
            {
                layers.iter().for_each(&mut visit);
            }
            for slot in [&visuals.head_inactive, &visuals.head_active]
                .into_iter()
                .flatten()
            {
                visit(slot);
            }
            for slot in [
                &visuals.body_inactive,
                &visuals.body_active,
                &visuals.bottomcap_inactive,
                &visuals.bottomcap_active,
            ]
            .into_iter()
            .flatten()
            .filter(|slot| hold_part_drawn(slot))
            {
                visit(slot);
            }
        }
        if let Some(layers) = skin.mine_layers.get(col) {
            layers.iter().for_each(&mut visit);
        }
        if let Some(slot) = skin.receptor_off.get(col) {
            visit(slot);
        }
        for slot in [
            skin.receptor_idle_glow_layers.get(col),
            skin.receptor_glow.get(col),
        ]
        .into_iter()
        .flatten()
        .flatten()
        {
            visit(slot);
        }
        for layer in skin
            .receptor_overlays
            .get(col)
            .into_iter()
            .flat_map(|layers| layers.iter())
        {
            visit(&layer.slot);
        }
        if let Some(explosion) = skin.tap_explosion_for_col(col, "W1") {
            for layer in explosion.layers.iter() {
                visit(&layer.slot);
            }
        }
    }
}

// Keep layer selection, animation, and model/sprite composition together so the
// noteskin row, component rows, and picker use the same note presentation.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_noteskin_note(
    actors: &mut Vec<Actor>,
    ns: &Noteskin,
    part: NoteAnimPart,
    note_idx: usize,
    quant_idx: f32,
    center: [f32; 2],
    target_height: f32,
    alpha: f32,
    z: i16,
    time: f32,
    beat: f32,
) {
    let phase = ns.part_uv_phase(part, time, beat, 0.0);
    let spacing = ns.note_display_metrics.part_texture_translate[part as usize].note_color_spacing;
    let translation = [spacing[0] * quant_idx, spacing[1] * quant_idx];
    let slots = preview_note_slots(ns, part, note_idx);
    let note_scale = target_height / FIELD_UNIT_PX;
    draw_note_slots(
        actors,
        slots,
        phase,
        translation,
        center,
        note_scale,
        alpha,
        z,
        time,
        beat,
        None,
    );
}

/// A tap (or a lift when `lift`) in column `col` (0-based within a 4-column
/// skin) at quantization `quant` (0=4th .. 8=finer, clamped to the skin),
/// centred at `center`, scaled so the arrow is `size` px tall, at skin time
/// `time` (seconds) and `beat`.
///
/// Gameplay's tap path: the column's slot `col * NUM_QUANTIZATIONS + quant`
/// carries the column's facing, and the quant selects the texture strip and
/// the vivid frame offset through a note beat of that quantization. Layers
/// scale by the field zoom `size / 64`, so a standard 64-pixel arrow is exactly
/// `size` px tall and every layer keeps its authored size relative to it.
///
/// `cache` holds model geometry across draws; `None` builds it per draw.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_tap(
    actors: &mut Vec<Actor>,
    skin: &Noteskin,
    col: usize,
    quant: usize,
    lift: bool,
    center: [f32; 2],
    size: f32,
    alpha: f32,
    z: i16,
    time: f32,
    beat: f32,
    cache: Option<&mut ModelMeshCache>,
) {
    let col = skin_col(skin, col);
    let quant = quant.min(NUM_QUANTIZATIONS - 1);
    let part = if lift {
        NoteAnimPart::Lift
    } else {
        NoteAnimPart::Tap
    };
    let note_beat = QUANT_NOTE_BEATS[quant];
    draw_note_slots(
        actors,
        preview_note_slots(skin, part, col * NUM_QUANTIZATIONS + quant),
        skin.part_uv_phase(part, time, beat, note_beat),
        skin.part_uv_translation(part, note_beat, false),
        center,
        size / FIELD_UNIT_PX,
        alpha,
        z,
        time,
        beat,
        cache,
    );
}

/// Every layer of one note at a shared `scale`, stacked upward from `z`.
#[allow(clippy::too_many_arguments)]
fn draw_note_slots(
    actors: &mut Vec<Actor>,
    slots: &[SpriteSlot],
    phase: f32,
    translation: [f32; 2],
    center: [f32; 2],
    scale: f32,
    alpha: f32,
    z: i16,
    time: f32,
    beat: f32,
    mut cache: Option<&mut ModelMeshCache>,
) {
    for (layer_idx, slot) in slots.iter().enumerate() {
        let frame = slot.frame_index_from_phase(phase);
        let uv_elapsed = if slot.uv_uses_phase() { phase } else { time };
        let uv = slot.uv_for_note_at(frame, uv_elapsed, translation);
        let logical = slot.logical_size();
        draw_slot(
            actors,
            slot,
            ModelDrawState {
                texture_seconds: if slot.actor_frame_child {
                    time
                } else {
                    phase * slot.model_animation_length
                },
                ..preview_slot_draw(slot, time, beat)
            },
            center,
            [logical[0] * scale, logical[1] * scale],
            uv,
            -slot.def.rotation_deg as f32,
            [1.0, 1.0, 1.0, alpha],
            BlendMode::Alpha,
            z + layer_idx as i16,
            cache.as_deref_mut(),
        );
    }
}

/// Player Options' mine icon: the second column's mine when the skin has more
/// than one, with every layer fitted to `target_height`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_mine_preview(
    actors: &mut Vec<Actor>,
    mine_ns: &Noteskin,
    mine_center: [f32; 2],
    target_height: f32,
    alpha: f32,
    z: i16,
    time: f32,
    beat: f32,
) {
    draw_mine_layers(
        actors,
        mine_ns,
        usize::from(mine_ns.mine_layers.len() > 1),
        mine_center,
        alpha,
        z,
        time,
        beat,
        |slot| {
            let logical = slot.logical_size();
            let scale = target_height / logical[1].max(1.0);
            [logical[0] * scale, target_height]
        },
        None,
    );
}

/// A mine in column `col`, sized like gameplay's mines on an arrow `size` px
/// tall: models and ActorFrame children keep their field-unit dimensions and
/// plain sprites fit the arrow height. `cache` as for [`draw_tap`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_mine(
    actors: &mut Vec<Actor>,
    skin: &Noteskin,
    col: usize,
    center: [f32; 2],
    size: f32,
    alpha: f32,
    z: i16,
    time: f32,
    beat: f32,
    cache: Option<&mut ModelMeshCache>,
) {
    let zoom = size / FIELD_UNIT_PX;
    draw_mine_layers(
        actors,
        skin,
        skin_col(skin, col),
        center,
        alpha,
        z,
        time,
        beat,
        |slot| {
            if slot.model.is_some() || slot.actor_frame_child {
                let logical = slot.logical_size();
                return [logical[0] * zoom, logical[1] * zoom];
            }
            let frame = slot.size();
            let width = frame[0].max(0) as f32;
            let height = frame[1].max(0) as f32;
            if height <= 0.0 {
                [width, height]
            } else {
                [width * size / height, size]
            }
        },
        cache,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_mine_layers(
    actors: &mut Vec<Actor>,
    mine_ns: &Noteskin,
    mine_col: usize,
    mine_center: [f32; 2],
    alpha: f32,
    z: i16,
    time: f32,
    beat: f32,
    layer_size: impl Fn(&SpriteSlot) -> [f32; 2],
    mut cache: Option<&mut ModelMeshCache>,
) {
    let Some(layers) = mine_ns.mine_layers.get(mine_col) else {
        return;
    };
    let phase = mine_ns.tap_mine_uv_phase(time, beat, 0.0);
    let translation = mine_ns.part_uv_translation(NoteAnimPart::Mine, 0.0, false);
    let mine_start = actors.len();
    let mut first_mesh = true;
    for slot in layers.iter() {
        let draw = slot.model_draw_at(time, beat);
        let frame = if slot.actor_frame_child {
            slot.frame_index(time, beat)
        } else {
            slot.frame_index_from_phase(phase)
        };
        let uv_time = if slot.uv_uses_phase() { phase } else { time };
        let uv = slot.uv_for_note_at(frame, uv_time, translation);
        let layer_start = actors.len();
        draw_slot(
            actors,
            slot,
            ModelDrawState {
                texture_seconds: if slot.actor_frame_child {
                    draw.texture_seconds
                } else {
                    phase * slot.model_animation_length
                },
                ..draw
            },
            mine_center,
            layer_size(slot),
            uv,
            -slot.def.rotation_deg as f32,
            [1.0, 1.0, 1.0, alpha],
            BlendMode::Alpha,
            z,
            cache.as_deref_mut(),
        );
        if slot.model_cull_back()
            && let Some(model) = slot.model.as_ref()
        {
            for actor in &mut actors[layer_start..] {
                if let Actor::TexturedMesh {
                    local_transform,
                    depth_test,
                    depth_write,
                    clear_depth,
                    tint,
                    glow,
                    ..
                } = actor
                {
                    if tint[3] <= 0.0 && glow[3] <= 0.0001 {
                        continue;
                    }
                    deadsync_notefield::noteskin_model_depth(model, local_transform);
                    // ITG's menu camera spans +/-1000 model units; ours spans
                    // +/-1. Normalize only Z, preserving perspective and XY.
                    *local_transform =
                        glam::Mat4::from_scale(glam::Vec3::new(1.0, 1.0, 0.001)) * *local_transform;
                    *depth_test = true;
                    *depth_write = true;
                    *clear_depth = first_mesh;
                    first_mesh = false;
                }
            }
        }
    }
    if let Some(clear) = actors[mine_start..]
        .iter_mut()
        .rev()
        .find_map(|actor| match actor {
            Actor::TexturedMesh {
                depth_test: true,
                depth_write: true,
                depth_test_on_fail: false,
                tint,
                glow,
                clear_depth_after,
                ..
            } if tint[3] > 0.0 || glow[3] > 0.0001 => Some(clear_depth_after),
            _ => None,
        })
    {
        *clear = true;
    }
}

/// How near a tile's edge a hold body counts as on it, as the notefield's.
const HOLD_PHASE_EPS: f32 = 1e-4;
/// The most tiles one body is drawn in: a bound on a skin whose body texture
/// is a sliver, far past what any real one needs.
const HOLD_MAX_TILES: usize = 512;

/// A hold or roll in column `col` on an upscroll field, as the notefield
/// draws one: the body from the head down to the tail at `tail_y`, the bottom
/// cap under it, and the head over them both at `head`. `active` is the held
/// look; where a held head sits -- its receptor -- is the caller's to say. The
/// body and cap are clipped to `bounds`, top and bottom.
///
/// The head takes its quantization's colour as a tap does, and a skin with no
/// hold head of its own draws its tap there, as gameplay does. The body and
/// cap are drawn at `z`, and the head's layers stack up from `z + 1`. A model
/// body is a strip the notefield slices along the hold; this leaves it out.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_hold(
    actors: &mut Vec<Actor>,
    skin: &Noteskin,
    col: usize,
    quant: usize,
    roll: bool,
    active: bool,
    head: [f32; 2],
    tail_y: f32,
    bounds: [f32; 2],
    size: f32,
    alpha: f32,
    z: i16,
    time: f32,
    beat: f32,
    cache: Option<&mut ModelMeshCache>,
) {
    let visuals = skin.hold_visuals_for_col(skin_col(skin, col), roll);
    let (head_part, body_part, cap_part) = if roll {
        (
            NoteAnimPart::RollHead,
            NoteAnimPart::RollBody,
            NoteAnimPart::RollBottomCap,
        )
    } else {
        (
            NoteAnimPart::HoldHead,
            NoteAnimPart::HoldBody,
            NoteAnimPart::HoldBottomCap,
        )
    };
    let quant = quant.min(NUM_QUANTIZATIONS - 1);
    let note_beat = QUANT_NOTE_BEATS[quant];
    let zoom = size / FIELD_UNIT_PX;
    let uv = |slot: &SpriteSlot, part: NoteAnimPart| {
        let phase = skin.part_uv_phase(part, time, beat, note_beat);
        slot.uv_for_note_at(
            slot.frame_index_from_phase(phase),
            time,
            skin.part_uv_translation(part, note_beat, false),
        )
    };
    if let Some(body) = hold_visual(
        visuals.body_active.as_ref(),
        visuals.body_inactive.as_ref(),
        active,
    )
    .filter(|slot| hold_part_drawn(slot))
    {
        let uv = uv(body, body_part);
        draw_hold_body(actors, body, uv, head, tail_y, bounds, zoom, alpha, z);
    }
    if let Some(cap) = hold_visual(
        visuals.bottomcap_active.as_ref(),
        visuals.bottomcap_inactive.as_ref(),
        active,
    )
    .filter(|slot| hold_part_drawn(slot))
    {
        let uv = uv(cap, cap_part);
        draw_hold_cap(actors, cap, uv, head[0], tail_y, bounds, zoom, alpha, z);
    }
    let head_slots = hold_visual(
        visuals.head_active_layers.as_deref(),
        visuals.head_inactive_layers.as_deref(),
        active,
    )
    .or_else(|| {
        hold_visual(
            visuals.head_active.as_ref(),
            visuals.head_inactive.as_ref(),
            active,
        )
        .map(std::slice::from_ref)
    });
    match head_slots {
        Some(slots) => draw_note_slots(
            actors,
            slots,
            skin.part_uv_phase(head_part, time, beat, note_beat),
            skin.part_uv_translation(head_part, note_beat, false),
            head,
            zoom,
            alpha,
            z + 1,
            time,
            beat,
            cache,
        ),
        None => draw_tap(
            actors,
            skin,
            col,
            quant,
            false,
            head,
            size,
            alpha,
            z + 1,
            time,
            beat,
            cache,
        ),
    }
}

/// The held look or the idle one, falling back to whichever the skin has.
fn hold_visual<'a, T: ?Sized>(
    active: Option<&'a T>,
    inactive: Option<&'a T>,
    held: bool,
) -> Option<&'a T> {
    if held {
        active.or(inactive)
    } else {
        inactive.or(active)
    }
}

/// Whether [`draw_hold`] draws a body or cap slot: sprites only.
fn hold_part_drawn(slot: &SpriteSlot) -> bool {
    slot.model.is_none()
}

/// A hold's body from `head` down to `tail_y`, in tiles of its texture's own
/// height at the field's `zoom`, as the notefield draws an upscroll body:
/// counted so whole tiles end on the tail and a part one starts at the head,
/// which keeps the pattern fixed to the tail while a held body shortens.
#[allow(clippy::too_many_arguments)]
fn draw_hold_body(
    actors: &mut Vec<Actor>,
    slot: &SpriteSlot,
    uv: [f32; 4],
    head: [f32; 2],
    tail_y: f32,
    bounds: [f32; 2],
    zoom: f32,
    alpha: f32,
    z: i16,
) {
    let [width, tile] = slot.source_size().map(|px| px.max(0) as f32 * zoom);
    let top = head[1].max(bounds[0]);
    let bottom = tail_y.min(bounds[1]);
    if width <= f32::EPSILON || tile <= f32::EPSILON || bottom - top <= f32::EPSILON {
        return;
    }
    let [u0, v_top, u1, v_bottom] = uv;
    let v_range = v_bottom - v_top;
    let tiles = (tail_y - head[1]) / tile;
    let part = tiles.fract();
    let offset =
        if tiles >= 1.0 + HOLD_PHASE_EPS && part > HOLD_PHASE_EPS && 1.0 - part > HOLD_PHASE_EPS {
            1.0 - part
        } else {
            0.0
        };
    let mut phase = (top - head[1]) / tile + offset;
    let end = (bottom - head[1]) / tile + offset;
    // The tail's own tile ends on the texture's end, float error or not.
    let tail_shows = tail_y <= bounds[1];
    for _ in 0..HOLD_MAX_TILES {
        if end - phase <= HOLD_PHASE_EPS {
            break;
        }
        let base = phase.floor();
        let next = (base + 1.0).min(end);
        if next - phase <= HOLD_PHASE_EPS {
            // on a tile's end already: the next tile starts here
            phase = next;
            continue;
        }
        let seg_top = (phase - offset).mul_add(tile, head[1]);
        let height = (next - phase) * tile;
        let (v0, v1) = if tail_shows && next >= end - HOLD_PHASE_EPS {
            (
                v_range.mul_add(-(height / tile).min(1.0), v_bottom),
                v_bottom,
            )
        } else {
            (
                v_range.mul_add(phase - base, v_top),
                v_range.mul_add((next - base).min(1.0), v_top),
            )
        };
        actors.push(act!(sprite(slot.texture_key_shared()):
            align(0.5, 0.5): xy(head[0], height.mul_add(0.5, seg_top)): setsize(width, height):
            customtexturerect(u0, v0, u1, v1):
            diffuse(1.0, 1.0, 1.0, alpha): z(z)
        ));
        phase = next;
    }
}

/// A hold's bottom cap, from the tail down by its own height at `zoom`,
/// clipped to `bounds`.
#[allow(clippy::too_many_arguments)]
fn draw_hold_cap(
    actors: &mut Vec<Actor>,
    slot: &SpriteSlot,
    uv: [f32; 4],
    x: f32,
    tail_y: f32,
    bounds: [f32; 2],
    zoom: f32,
    alpha: f32,
    z: i16,
) {
    let [width, height] = slot.source_size().map(|px| px.max(0) as f32 * zoom);
    let top = tail_y.max(bounds[0]);
    let bottom = (tail_y + height).min(bounds[1]);
    if width <= f32::EPSILON || height <= f32::EPSILON || bottom - top <= f32::EPSILON {
        return;
    }
    let [u0, v0, u1, v1] = uv;
    let v_at = |y: f32| (v1 - v0).mul_add((y - tail_y) / height, v0);
    actors.push(act!(sprite(slot.texture_key_shared()):
        align(0.5, 0.5): xy(x, f32::midpoint(top, bottom)): setsize(width, bottom - top):
        customtexturerect(u0, v_at(top), u1, v_at(bottom)):
        diffuse(1.0, 1.0, 1.0, alpha): z(z)
    ));
}

#[inline(always)]
fn slot_preview_zoom_x(slot: &SpriteSlot, zoom: f32) -> f32 {
    if slot.def.mirror_h { -zoom } else { zoom }
}

#[inline(always)]
fn slot_preview_zoom_y(slot: &SpriteSlot, zoom: f32) -> f32 {
    if slot.def.mirror_v { -zoom } else { zoom }
}

/// Draw Player Options' preview of one skin component. `part` is the menu's
/// component index: 0 arrows, 1 receptors, 2-5 active/inactive hold and roll
/// bodies, 6 tap explosions, 7 hold explosions, 8 mines, 10 lifts.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_skin_part(
    actors: &mut Vec<Actor>,
    skin: &Noteskin,
    part: usize,
    center: [f32; 2],
    size: f32,
    alpha: f32,
    z: i16,
    time: f32,
    beat: f32,
) {
    match part {
        0 | 10 => draw_noteskin_note(
            actors,
            skin,
            if part == 10 {
                NoteAnimPart::Lift
            } else {
                NoteAnimPart::Tap
            },
            Quantization::Q4th as usize,
            0.0,
            center,
            size,
            alpha,
            z,
            time,
            beat,
        ),
        1 => draw_receptor_note(actors, skin, 0, center, size, alpha, z, time, beat),
        6 => draw_tap_explosion_preview(actors, skin, center, size, alpha, z, time, beat),
        8 => draw_mine_preview(actors, skin, center, size, alpha, z, time, beat),
        _ => {
            let (slot, anim) = match part {
                2 => (skin.hold.body_active.as_ref(), Some(NoteAnimPart::HoldBody)),
                3 => (
                    skin.hold.body_inactive.as_ref(),
                    Some(NoteAnimPart::HoldBody),
                ),
                4 => (skin.roll.body_active.as_ref(), Some(NoteAnimPart::RollBody)),
                5 => (
                    skin.roll.body_inactive.as_ref(),
                    Some(NoteAnimPart::RollBody),
                ),
                7 => (skin.hold.explosion.as_ref(), None),
                _ => return,
            };
            let Some(slot) = slot else { return };
            let phase = anim.map(|part| skin.part_uv_phase(part, time, beat, 0.0));
            let frame = phase.map_or_else(
                || slot.frame_index(time, beat),
                |phase| slot.frame_index_from_phase(phase),
            );
            let uv_time = if slot.uv_uses_phase() {
                phase.unwrap_or(time)
            } else {
                time
            };
            let uv = slot.uv_for_frame_at(frame, uv_time);
            let logical = slot.logical_size();
            let scale = size / logical[1].max(1.0);
            draw_preview_slot(
                actors,
                slot,
                ModelDrawState {
                    texture_seconds: if slot.actor_frame_child {
                        time
                    } else {
                        phase.map_or(time, |phase| phase * slot.model_animation_length)
                    },
                    ..preview_slot_draw(slot, time, beat)
                },
                center,
                [logical[0] * scale, logical[1] * scale],
                uv,
                -slot.def.rotation_deg as f32,
                [1.0, 1.0, 1.0, alpha],
                BlendMode::Alpha,
                z,
            );
        }
    }
}

/// A slot's idle draw state, including its authored glow effect.
pub(crate) fn preview_slot_draw(slot: &SpriteSlot, time: f32, beat: f32) -> ModelDrawState {
    let mut draw = slot.model_draw_at(time, beat);
    if let Some(glow) = slot.model_glow_with_draw(draw, time, beat, 1.0) {
        draw.glow = glow;
    }
    draw
}

// Model and sprite transforms share one path, including diffuse/glow effects.
// Explosion commands supply their sampled draw state instead of the idle state.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_preview_slot(
    actors: &mut Vec<Actor>,
    slot: &SpriteSlot,
    draw: ModelDrawState,
    center: [f32; 2],
    size: [f32; 2],
    uv: [f32; 4],
    rotation: f32,
    color: [f32; 4],
    blend: BlendMode,
    z: i16,
) {
    draw_slot(
        actors, slot, draw, center, size, uv, rotation, color, blend, z, None,
    );
}

/// [`draw_preview_slot`], with a model's geometry taken from `cache` when the
/// caller keeps one: built once per slot and shared under the slot's stable
/// key, rather than rebuilt for every note every frame.
#[allow(clippy::too_many_arguments)]
fn draw_slot(
    actors: &mut Vec<Actor>,
    slot: &SpriteSlot,
    draw: ModelDrawState,
    center: [f32; 2],
    size: [f32; 2],
    uv: [f32; 4],
    rotation: f32,
    color: [f32; 4],
    blend: BlendMode,
    z: i16,
    cache: Option<&mut ModelMeshCache>,
) {
    if !draw.visible {
        return;
    }
    let blend = if draw.blend_add {
        BlendMode::Add
    } else {
        blend
    };
    let model = match cache {
        Some(cache) => noteskin_model_actor_from_draw_cached(
            slot, draw, center, size, uv, rotation, color, blend, z, cache,
        ),
        None => {
            noteskin_model_actor_from_draw(slot, draw, center, size, uv, rotation, color, blend, z)
        }
    };
    let mut actor = if let Some(actor) = model {
        actor
    } else {
        let logical = slot.logical_size();
        let ox = draw.pos[0] * size[0] / logical[0].max(1.0);
        let oy = draw.pos[1] * size[1] / logical[1].max(1.0);
        let (sin, cos) = rotation.to_radians().sin_cos();
        let mut pos = [
            center[0] + ox * cos - oy * sin,
            center[1] + ox * sin + oy * cos,
        ];
        let size = [size[0] * draw.zoom[0].abs(), size[1] * draw.zoom[1].abs()];
        if size[0] <= f32::EPSILON || size[1] <= f32::EPSILON {
            return;
        }
        let align_offset = (0.5 - draw.vert_align) * size[1];
        let (sin, cos) = (draw.rot[2] + rotation).to_radians().sin_cos();
        pos[0] -= sin * align_offset;
        pos[1] += cos * align_offset;
        let tint = std::array::from_fn::<_, 4, _>(|i| color[i] * draw.tint[i]);
        let mut actor = act!(sprite(slot.texture_key_shared()):
            align(0.5, 0.5): xy(pos[0], pos[1]): setsize(size[0], size[1]):
            zoomx(slot_preview_zoom_x(slot, draw.zoom[0].signum())):
            zoomy(slot_preview_zoom_y(slot, draw.zoom[1].signum())):
            rotationz(draw.rot[2] + rotation):
            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
            diffuse(tint[0], tint[1], tint[2], tint[3]): z(z)
        );
        if let Actor::Sprite {
            blend: actor_blend, ..
        } = &mut actor
        {
            *actor_blend = blend;
        }
        actor
    };
    match &mut actor {
        Actor::Sprite { glow, .. } | Actor::TexturedMesh { glow, .. } => {
            *glow = [
                draw.glow[0],
                draw.glow[1],
                draw.glow[2],
                draw.glow[3] * color[3],
            ];
        }
        _ => {}
    }
    actors.push(actor);
}

/// Player Options' receptor icon for `col`: every layer fitted to `size`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_receptor_note(
    actors: &mut Vec<Actor>,
    skin: &Noteskin,
    col: usize,
    center: [f32; 2],
    size: f32,
    alpha: f32,
    z: i16,
    time: f32,
    beat: f32,
) {
    draw_receptor_layers(
        actors,
        skin,
        col,
        center,
        size,
        alpha,
        z,
        time,
        beat,
        ReceptorLook::Menu,
        None,
    );
}

/// The receptor for `col`, idle pulse at `beat`; `press_age` = seconds since a
/// note was hit in this column (None = not pressed): apply the skin's W1 step
/// zoom and press glow, then its lift after ~0.09 s, the way gameplay animates
/// a receptor on a hit.
///
/// Layers scale by the same field zoom as [`draw_tap`]. Once the press has
/// decayed the receptor draws exactly as with `press_age: None`. `cache` as
/// for [`draw_tap`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_receptor(
    actors: &mut Vec<Actor>,
    skin: &Noteskin,
    col: usize,
    center: [f32; 2],
    size: f32,
    alpha: f32,
    z: i16,
    time: f32,
    beat: f32,
    press_age: Option<f32>,
    cache: Option<&mut ModelMeshCache>,
) {
    let col = skin_col(skin, col);
    let press = press_age
        .filter(|age| *age >= 0.0)
        .map_or(ReceptorPress::IDLE, |age| {
            ReceptorPress::after_hit(skin, col, age)
        });
    draw_receptor_layers(
        actors,
        skin,
        col,
        center,
        size,
        alpha,
        z,
        time,
        beat,
        ReceptorLook::Field(press),
        cache,
    );
}

/// How a receptor is composed.
#[derive(Clone, Copy)]
enum ReceptorLook {
    /// Player Options: each layer fitted to the icon, the idle flash additive
    /// unless the skin animates it as an actor effect.
    Menu,
    /// Gameplay: field-unit layer sizes, gameplay's idle-glow blending, and the
    /// press state of a hit.
    Field(ReceptorPress),
}

/// A receptor's response to a hit, sampled at one instant.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ReceptorPress {
    /// The step command's zoom on the whole receptor.
    bop_zoom: f32,
    /// Press-glow `(alpha, zoom)`, or `None` once its lift has finished.
    glow: Option<(f32, f32)>,
}

impl ReceptorPress {
    const IDLE: Self = Self {
        bop_zoom: 1.0,
        glow: None,
    };

    /// Closed form of gameplay's receptor timers `age` seconds after a W1 hit
    /// in `col`: the lane goes down (Press plus the W1 step command) and comes
    /// up after `RECEPTOR_PRESS_HOLD_SECONDS` (Lift).
    fn after_hit(skin: &Noteskin, col: usize, age: f32) -> Self {
        let step = skin.receptor_step_behavior_for_col(col, Some("W1"));
        let bop_zoom = if age < step.duration {
            step.sample_zoom(step.duration - age)
        } else {
            1.0
        };
        Self {
            bop_zoom,
            glow: receptor_glow_after_hit(skin.receptor_glow_behavior, age),
        }
    }
}

/// Gameplay's press-glow visual `age` seconds after the lane went down, with
/// the lane released at `RECEPTOR_PRESS_HOLD_SECONDS`.
fn receptor_glow_after_hit(behavior: ReceptorGlowBehavior, age: f32) -> Option<(f32, f32)> {
    let press = behavior.press_duration.max(0.0);
    let hold = RECEPTOR_PRESS_HOLD_SECONDS;
    if age < hold {
        // Held: Press tweens, then rests on its end state.
        let remaining = press - age;
        return Some(if remaining > f32::EPSILON {
            behavior.sample_press(remaining)
        } else {
            (behavior.press_alpha_end, behavior.press_zoom_end)
        });
    }
    // Released: Lift either interrupts Press or queues behind its tween.
    let remaining = (press - hold).max(0.0);
    let (lift_at, start) = if !behavior.lift_interrupts_press && remaining > f32::EPSILON {
        (press, receptor_glow_lift_start(behavior, 0.0))
    } else {
        (hold, receptor_glow_lift_start(behavior, remaining))
    };
    if age < lift_at {
        return Some(behavior.sample_press(press - age));
    }
    let duration = if behavior.duration > f32::EPSILON {
        behavior.duration
    } else {
        deadsync_gameplay::RECEPTOR_GLOW_DURATION
    };
    let lift_remaining = duration - (age - lift_at);
    (lift_remaining > f32::EPSILON).then(|| behavior.sample_lift(lift_remaining, start.0, start.1))
}

/// Where Lift starts: the interrupted Press state, unless the skin finishes
/// Press first or authors explicit Lift start values.
fn receptor_glow_lift_start(behavior: ReceptorGlowBehavior, press_remaining: f32) -> (f32, f32) {
    let (alpha, zoom) = if !behavior.lift_finishes_press
        && press_remaining > f32::EPSILON
        && behavior.press_duration > f32::EPSILON
    {
        behavior.sample_press(press_remaining)
    } else {
        (behavior.press_alpha_end, behavior.press_zoom_end)
    };
    (
        behavior.lift_alpha_start.unwrap_or(alpha),
        behavior.lift_zoom_start.unwrap_or(zoom),
    )
}

// Preserve the receptor's layer order, including field backgrounds and press
// lights authored beneath the target. Menu icons omit field backgrounds.
#[allow(clippy::too_many_arguments)]
fn draw_receptor_layers(
    actors: &mut Vec<Actor>,
    skin: &Noteskin,
    col: usize,
    center: [f32; 2],
    size: f32,
    alpha: f32,
    z: i16,
    time: f32,
    beat: f32,
    look: ReceptorLook,
    mut cache: Option<&mut ModelMeshCache>,
) {
    let pulse = skin.receptor_pulse.color_for_beat(beat);
    let idle = skin.receptor_idle_glow.alpha(beat, false);
    let distinct_idle = skin
        .receptor_idle_glow_layers
        .get(col)
        .and_then(Option::as_ref);
    let press_slot = skin.receptor_glow.get(col).and_then(Option::as_ref);
    let idle_slot = distinct_idle.or(press_slot);
    let menu = matches!(look, ReceptorLook::Menu);
    let press = match look {
        ReceptorLook::Menu => ReceptorPress::IDLE,
        ReceptorLook::Field(press) => press,
    };
    // Gameplay folds the press into an idle flash that reuses the press layer:
    // the flash takes the press zoom and no separate glow is drawn.
    let shares_press = !menu
        && skin.receptor_idle_glow.is_visible()
        && idle_slot.is_some()
        && distinct_idle.is_none();
    let idle_press_zoom = if shares_press {
        press.glow.map_or(1.0, |glow| glow.1)
    } else {
        1.0
    };
    let target_zoom = press.bop_zoom * idle_press_zoom;
    let layer_size = |slot: &SpriteSlot, zoom: f32| {
        let logical = slot.logical_size();
        let fit = if menu {
            size / logical[1].max(1.0)
        } else {
            size / FIELD_UNIT_PX
        };
        let scale = fit * zoom;
        [logical[0] * scale, logical[1] * scale]
    };
    for layer in skin
        .receptor_overlays
        .get(col)
        .into_iter()
        .flat_map(|layers| layers.iter())
        .filter(|layer| !menu && layer.before_target)
    {
        let logical = layer.slot.logical_size();
        draw_receptor_layer(
            actors,
            &layer.slot,
            center,
            [
                logical[0] * size / FIELD_UNIT_PX,
                logical[1] * size / FIELD_UNIT_PX,
            ],
            [1.0, 1.0, 1.0, alpha],
            BlendMode::Alpha,
            z - 2,
            time,
            beat,
            cache.as_deref_mut(),
        );
    }
    if let Some(slot) = skin.receptor_off.get(col) {
        draw_receptor_layer(
            actors,
            slot,
            center,
            layer_size(slot, target_zoom),
            [pulse[0], pulse[1], pulse[2], pulse[3] * alpha],
            BlendMode::Alpha,
            z,
            time,
            beat,
            cache.as_deref_mut(),
        );
    }
    if let Some(slot) = idle_slot {
        let add = if menu {
            skin.receptor_idle_glow != ReceptorIdleGlow::ActorEffect
        } else {
            shares_press && skin.receptor_glow_behavior.blend_add
        };
        draw_receptor_layer(
            actors,
            slot,
            center,
            layer_size(slot, target_zoom),
            [1.0, 1.0, 1.0, idle * alpha],
            if add {
                BlendMode::Add
            } else {
                BlendMode::Alpha
            },
            if skin.receptor_glow_behavior.draw_before_target {
                z - 1
            } else {
                z + 1
            },
            time,
            beat,
            cache.as_deref_mut(),
        );
    }
    if !shares_press && let (Some(slot), Some((glow_alpha, glow_zoom))) = (press_slot, press.glow) {
        draw_receptor_layer(
            actors,
            slot,
            center,
            layer_size(slot, press.bop_zoom * glow_zoom),
            [1.0, 1.0, 1.0, glow_alpha * alpha],
            if skin.receptor_glow_behavior.blend_add {
                BlendMode::Add
            } else {
                BlendMode::Alpha
            },
            if skin.receptor_glow_behavior.draw_before_target {
                z - 1
            } else {
                z + 1
            },
            time,
            beat,
            cache.as_deref_mut(),
        );
    }
    for (index, layer) in skin
        .receptor_overlays
        .get(col)
        .into_iter()
        .flat_map(|layers| layers.iter())
        .filter(|layer| !layer.before_target)
        .enumerate()
    {
        draw_receptor_layer(
            actors,
            &layer.slot,
            center,
            layer_size(&layer.slot, press.bop_zoom),
            [1.0, 1.0, 1.0, alpha],
            BlendMode::Alpha,
            z + index as i16 + 2,
            time,
            beat,
            cache.as_deref_mut(),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_receptor_layer(
    actors: &mut Vec<Actor>,
    slot: &SpriteSlot,
    center: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
    blend: BlendMode,
    z: i16,
    time: f32,
    beat: f32,
    cache: Option<&mut ModelMeshCache>,
) {
    if color[3] <= f32::EPSILON {
        return;
    }
    let frame = slot.frame_index(time, beat);
    let uv = slot.uv_for_frame_at(frame, time);
    draw_slot(
        actors,
        slot,
        preview_slot_draw(slot, time, beat),
        center,
        size,
        uv,
        -slot.def.rotation_deg as f32,
        color,
        blend,
        z,
        cache,
    );
}

/// Player Options' looping W1 explosion icon, fitted to `size`, at
/// `TAP_EXPLOSION_PREVIEW_SPEED` of the menu clock.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_tap_explosion_preview(
    actors: &mut Vec<Actor>,
    skin: &Noteskin,
    center: [f32; 2],
    size: f32,
    alpha: f32,
    z: i16,
    time: f32,
    beat: f32,
) {
    let Some(explosion) = skin
        .tap_explosions
        .get("W1")
        .or_else(|| skin.tap_explosions.values().next())
    else {
        return;
    };
    let time = time * TAP_EXPLOSION_PREVIEW_SPEED;
    let beat = beat * TAP_EXPLOSION_PREVIEW_SPEED;
    let duration = explosion.duration();
    let elapsed = if duration > f32::EPSILON {
        time.rem_euclid(duration)
    } else {
        0.0
    };
    let seed = if duration > f32::EPSILON {
        (time / duration).floor() as u64
    } else {
        0
    };
    draw_explosion_layers(
        actors,
        explosion,
        center,
        size / explosion.slot.logical_size()[1].max(1.0),
        alpha,
        z,
        ExplosionClock {
            age: elapsed,
            seed,
            frame_beat: beat,
            uv_time: time,
            seek: false,
        },
        None,
    );
}

/// The skin's W1 tap explosion for `col`, `age` seconds after the hit; draws
/// nothing once the animation has finished.
///
/// Layers use gameplay's field zoom for an arrow `size` px tall. Beat-based
/// sprite sheets advance one frame beat per second of `age`, since the hit's
/// beat is not known here; other sheets follow `beat`. `cache` as for
/// [`draw_tap`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_explosion(
    actors: &mut Vec<Actor>,
    skin: &Noteskin,
    col: usize,
    center: [f32; 2],
    size: f32,
    z: i16,
    age: f32,
    beat: f32,
    cache: Option<&mut ModelMeshCache>,
) {
    let col = skin_col(skin, col);
    let Some(explosion) = skin.tap_explosion_for_col(col, "W1") else {
        return;
    };
    // Also rejects NaN ages.
    if !(age >= 0.0 && age < explosion.duration()) {
        return;
    }
    draw_explosion_layers(
        actors,
        explosion,
        center,
        size / FIELD_UNIT_PX,
        1.0,
        z,
        ExplosionClock {
            age,
            seed: col as u64,
            frame_beat: beat,
            uv_time: age,
            seek: true,
        },
        cache,
    );
}

/// How long the W1 explosion for `col` lasts, so a caller can stop drawing it.
pub(crate) fn explosion_duration(skin: &Noteskin, col: usize) -> f32 {
    skin.tap_explosion_for_col(skin_col(skin, col), "W1")
        .map_or(0.0, TapExplosion::duration)
}

/// One sample of an explosion's clocks.
#[derive(Clone, Copy)]
struct ExplosionClock {
    /// Seconds since the hit: drives the tween commands and beat-based frames.
    age: f32,
    /// Per-hit seed for random rotation commands.
    seed: u64,
    /// Frame beat for sprite sheets that are not beat-based.
    frame_beat: f32,
    /// Texture-scroll clock for slots that do not scroll by animation phase.
    uv_time: f32,
    /// Apply each layer's authored animation seek, as gameplay does.
    seek: bool,
}

#[allow(clippy::too_many_arguments)]
fn draw_explosion_layers(
    actors: &mut Vec<Actor>,
    explosion: &TapExplosion<SpriteSlot>,
    center: [f32; 2],
    scale: f32,
    alpha: f32,
    z: i16,
    clock: ExplosionClock,
    mut cache: Option<&mut ModelMeshCache>,
) {
    for (index, layer) in explosion.layers.iter().enumerate() {
        let visual = layer.animation.state_at_seeded(
            clock.age,
            clock.age,
            clock
                .seed
                .wrapping_add((index as u64).wrapping_mul(0xd1b54a32d192ed03)),
        );
        let slot = &layer.slot;
        let (frame_time, frame_beat) = match layer.animation.animation_seconds {
            _ if layer.animation.animation_uses_screen_clock => (clock.uv_time, clock.frame_beat),
            Some(seconds) if clock.seek => {
                let seconds = seconds - slot.animation_start_time();
                (clock.age + seconds, clock.age + seconds)
            }
            _ if slot.source.is_beat_based() => (clock.age, clock.age),
            _ => (clock.age, clock.frame_beat),
        };
        let frame = slot.frame_index(frame_time, frame_beat);
        let uv = slot.uv_for_frame_at(
            frame,
            if slot.uv_uses_phase() {
                clock.age
            } else {
                clock.uv_time
            },
        );
        let logical = slot.logical_size();
        let draw = ModelDrawState {
            zoom: [visual.zoom, visual.zoom, 1.0],
            rot: [0.0, 0.0, visual.rotation_z],
            tint: visual.diffuse,
            glow: visual.glow,
            visible: visual.visible,
            blend_add: layer.animation.blend_add,
            ..Default::default()
        };
        draw_slot(
            actors,
            slot,
            draw,
            center,
            [logical[0] * scale, logical[1] * scale],
            uv,
            -slot.def.rotation_deg as f32,
            [1.0, 1.0, 1.0, alpha],
            BlendMode::Alpha,
            z + index as i16,
            cache.as_deref_mut(),
        );
    }
}

/// `col` clamped to the skin's columns.
#[inline(always)]
fn skin_col(skin: &Noteskin, col: usize) -> usize {
    col.min(skin.column_xs.len().saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::actors::SizeSpec;
    use deadsync_gameplay::{
        GameplayReceptorGlowBehavior, GameplayReceptorGlowState, GameplayTween,
        receptor_glow_press_timers, receptor_glow_release_timers, receptor_glow_visual,
        tick_receptor_glow_timers,
    };
    use deadsync_noteskin::{
        NoteColorType, ReceptorStepBehavior, ReceptorStepBehaviors, TweenType,
    };
    use std::sync::Arc;

    const CENTER: [f32; 2] = [100.0, 120.0];

    fn dance_skin(name: &str) -> Arc<Noteskin> {
        crate::tests::init_paths();
        deadsync_assets::noteskin::load_itg_skin_cached(
            &deadsync_noteskin::Style {
                num_cols: 4,
                num_players: 1,
            },
            name,
        )
        .unwrap()
    }

    #[test]
    fn thin_note_preview_keeps_authored_height_at_field_scale() {
        let mut skin = (*dance_skin("default")).clone();
        let mut slot = deadsync_assets::noteskin::test_model_slot();
        slot.model = None;
        slot.def.size = [64, 20];
        slot.source_size = [64, 20];
        skin.note_layers[0] = Arc::from([slot]);
        let mut actors = Vec::new();
        draw_noteskin_note(
            &mut actors,
            &skin,
            NoteAnimPart::Tap,
            0,
            0.0,
            CENTER,
            25.6,
            1.0,
            10,
            0.0,
            0.0,
        );
        assert_eq!(actors.len(), 1);
        assert!((sprite_height(&actors[0]) - 8.0).abs() < 0.001);
    }

    #[test]
    fn field_receptor_underlay_keeps_alignment_and_stays_out_of_menu_icons() {
        let mut skin = (*dance_skin("default")).clone();
        let mut rail = deadsync_assets::noteskin::test_model_slot();
        rail.model = None;
        rail.def.size = [64, 1];
        rail.source_size = [64, 1];
        rail.model_draw.zoom[1] = 480.0;
        rail.model_draw.vert_align = 1.0;
        rail.model_draw.rot[2] = 180.0;
        skin.receptor_overlays[0] = Arc::from([deadsync_noteskin::runtime::ReceptorOverlay {
            slot: rail,
            reverse: Default::default(),
            before_target: true,
        }]);
        let mut actors = Vec::new();
        draw_receptor(
            &mut actors,
            &skin,
            0,
            CENTER,
            32.0,
            1.0,
            10,
            0.0,
            0.0,
            None,
            None,
        );
        let Actor::Sprite { offset, z, .. } = &actors[0] else {
            panic!("receptor underlay")
        };
        assert_eq!(*z, 8);
        assert!((offset[0] - CENTER[0]).abs() < 0.001);
        assert!((offset[1] - (CENTER[1] + 120.0)).abs() < 0.001);
        assert!((sprite_height(&actors[0]) - 240.0).abs() < 0.001);
        actors.clear();
        draw_receptor_note(&mut actors, &skin, 0, CENTER, 32.0, 1.0, 10, 0.0, 0.0);
        assert!(!actors.is_empty());
        assert!(actors.iter().all(|actor| match actor {
            Actor::Sprite { z, .. } => *z >= 10,
            _ => false,
        }));
    }

    fn sprite_rotation(actor: &Actor) -> f32 {
        let Actor::Sprite { rot_z_deg, .. } = actor else {
            panic!("noteskin sprite expected, got {actor:?}");
        };
        *rot_z_deg
    }

    fn sprite_height(actor: &Actor) -> f32 {
        let Actor::Sprite {
            size: [_, SizeSpec::Px(height)],
            scale,
            ..
        } = actor
        else {
            panic!("noteskin sprite expected, got {actor:?}");
        };
        height * scale[1].abs()
    }

    // Opposite columns face opposite ways and neighbours a quarter turn apart.
    fn assert_dance_facings(rotations: [f32; 4]) {
        let turn = |a: f32, b: f32| (a - b).rem_euclid(360.0);
        assert_eq!(turn(rotations[0], rotations[3]), 180.0, "{rotations:?}");
        assert_eq!(turn(rotations[1], rotations[2]), 180.0, "{rotations:?}");
        assert_eq!(
            turn(rotations[0], rotations[1]) % 180.0,
            90.0,
            "{rotations:?}"
        );
    }

    fn tap(skin: &Noteskin, col: usize, quant: usize, lift: bool) -> Vec<Actor> {
        let mut actors = Vec::new();
        draw_tap(
            &mut actors,
            skin,
            col,
            quant,
            lift,
            CENTER,
            64.0,
            1.0,
            10,
            0.3,
            0.6,
            None,
        );
        actors
    }

    #[test]
    fn taps_draw_every_layer_facing_their_columns() {
        let skin = dance_skin("default");
        let mut rotations = [0.0; 4];
        for (col, rotation) in rotations.iter_mut().enumerate() {
            for lift in [false, true] {
                let part = if lift {
                    NoteAnimPart::Lift
                } else {
                    NoteAnimPart::Tap
                };
                let actors = tap(&skin, col, 0, lift);
                let slots = preview_note_slots(&skin, part, col * NUM_QUANTIZATIONS);
                assert!(!actors.is_empty(), "col {col} lift {lift}");
                assert_eq!(actors.len(), slots.len(), "col {col} lift {lift}");
            }
            *rotation = sprite_rotation(&tap(&skin, col, 0, false)[0]);
        }
        assert_dance_facings(rotations);
        // A 64-pixel arrow at the field unit keeps its authored size.
        let primary = &preview_note_slots(&skin, NoteAnimPart::Tap, 0)[0];
        let arrow = &tap(&skin, 0, 0, false)[0];
        assert!((sprite_height(arrow) - primary.logical_size()[1]).abs() < 1e-3);
        assert_eq!(
            format!("{:?}", tap(&skin, 9, 0, false)),
            format!("{:?}", tap(&skin, 3, 0, false)),
            "columns clamp to the skin"
        );
    }

    #[test]
    fn tap_quantization_selects_its_strip_and_clamps() {
        let skin = dance_skin("default");
        let uv = |quant| {
            let actors = tap(&skin, 1, quant, false);
            let Actor::Sprite { uv_rect, .. } = &actors[0] else {
                panic!("tap sprite");
            };
            uv_rect.expect("tap samples a texture rect")
        };
        assert_ne!(uv(0), uv(1), "8ths sample their own color strip");
        assert_ne!(uv(1), uv(3), "16ths sample their own color strip");
        assert_eq!(uv(99), uv(NUM_QUANTIZATIONS - 1), "quantization clamps");
    }

    #[test]
    fn quant_note_beats_select_gameplay_denominator_colors() {
        let mut skin = (*dance_skin("default")).clone();
        let translate =
            &mut skin.note_display_metrics.part_texture_translate[NoteAnimPart::Tap as usize];
        translate.note_color_type = NoteColorType::Denominator;
        translate.note_color_count = 8;
        translate.note_color_spacing = [0.0, 0.125];
        translate.addition_offset = [0.0; 2];
        for (quant, beat) in QUANT_NOTE_BEATS.iter().enumerate() {
            // Gameplay colors a Denominator note by min(quant, count - 1).
            assert_eq!(
                skin.part_uv_translation(NoteAnimPart::Tap, *beat, false),
                [0.0, 0.125 * quant.min(7) as f32],
                "quant {quant}"
            );
        }
    }

    #[test]
    fn mines_draw_in_every_column() {
        let skin = dance_skin("default");
        for col in 0..4 {
            let mut actors = Vec::new();
            draw_mine(
                &mut actors,
                &skin,
                col,
                CENTER,
                64.0,
                1.0,
                10,
                0.3,
                0.6,
                None,
            );
            assert!(!actors.is_empty(), "col {col}");
        }
        let mut bare = (*skin).clone();
        bare.mine_layers.clear();
        let mut actors = Vec::new();
        draw_mine(&mut actors, &bare, 0, CENTER, 64.0, 1.0, 10, 0.3, 0.6, None);
        assert!(actors.is_empty(), "a skin without mines draws nothing");
    }

    fn receptor(skin: &Noteskin, col: usize, press_age: Option<f32>) -> Vec<Actor> {
        let mut actors = Vec::new();
        draw_receptor(
            &mut actors,
            skin,
            col,
            CENTER,
            64.0,
            1.0,
            10,
            0.25,
            0.5,
            press_age,
            None,
        );
        actors
    }

    #[test]
    fn receptors_face_their_columns_and_respond_to_hits() {
        let skin = dance_skin("default");
        let mut rotations = [0.0; 4];
        for (col, rotation) in rotations.iter_mut().enumerate() {
            *rotation = sprite_rotation(&receptor(&skin, col, None)[0]);
        }
        assert_dance_facings(rotations);
        let idle = format!("{:?}", receptor(&skin, 2, None));
        assert_ne!(
            format!("{:?}", receptor(&skin, 2, Some(0.0))),
            idle,
            "a hit presses the receptor"
        );
        assert_eq!(
            format!("{:?}", receptor(&skin, 2, Some(5.0))),
            idle,
            "the press decays back to idle"
        );
        assert_eq!(
            format!("{:?}", receptor(&skin, 2, Some(-1.0))),
            idle,
            "a hit that has not happened yet does not press"
        );
    }

    #[test]
    fn receptor_hit_applies_the_w1_step_zoom() {
        let mut skin = (*dance_skin("default")).clone();
        let identity = ReceptorStepBehavior::identity();
        let w1 = ReceptorStepBehavior {
            duration: 0.11,
            zoom_start: 0.75,
            zoom_end: 1.0,
            tween: TweenType::Linear,
            interrupts: true,
        };
        skin.receptor_step_behaviors = vec![
            ReceptorStepBehaviors::new(
                ReceptorStepBehavior::default(),
                identity,
                [w1, identity, identity, identity, identity],
            );
            4
        ];
        // Without a press glow layer only the step zoom changes the receptor.
        skin.receptor_glow = vec![None; 4];
        skin.receptor_idle_glow_layers = vec![None; 4];
        let idle = sprite_height(&receptor(&skin, 1, None)[0]);
        let hit = sprite_height(&receptor(&skin, 1, Some(0.0))[0]);
        assert!((hit - idle * 0.75).abs() < 1e-3, "idle {idle}, hit {hit}");
        let settled = sprite_height(&receptor(&skin, 1, Some(0.11))[0]);
        assert!(
            (settled - idle).abs() < 1e-3,
            "idle {idle}, settled {settled}"
        );
    }

    #[test]
    fn receptor_glow_matches_gameplay_press_and_release() {
        let behaviors = [
            ReceptorGlowBehavior::default(),
            ReceptorGlowBehavior {
                press_duration: 0.05,
                press_alpha_start: 1.0,
                press_alpha_end: 0.6,
                press_zoom_start: 1.2,
                press_zoom_end: 1.0,
                tween: TweenType::Linear,
                ..ReceptorGlowBehavior::default()
            },
            ReceptorGlowBehavior {
                press_duration: 0.25,
                press_alpha_start: 1.0,
                press_alpha_end: 0.5,
                lift_interrupts_press: false,
                duration: 0.15,
                tween: TweenType::Linear,
                ..ReceptorGlowBehavior::default()
            },
            ReceptorGlowBehavior {
                press_duration: 0.2,
                press_alpha_start: 0.4,
                press_alpha_end: 1.0,
                lift_finishes_press: true,
                lift_zoom_start: Some(1.5),
                duration: 0.0,
                tween: TweenType::Linear,
                ..ReceptorGlowBehavior::default()
            },
        ];
        let tween = |tween| match tween {
            TweenType::Linear => GameplayTween::Linear,
            TweenType::Decelerate => GameplayTween::Decelerate,
            other => panic!("untested tween {other:?}"),
        };
        for behavior in behaviors {
            let gameplay = GameplayReceptorGlowBehavior {
                press_duration: behavior.press_duration,
                press_alpha_start: behavior.press_alpha_start,
                press_alpha_end: behavior.press_alpha_end,
                press_zoom_start: behavior.press_zoom_start,
                press_zoom_end: behavior.press_zoom_end,
                press_tween: tween(behavior.press_tween),
                lift_interrupts_press: behavior.lift_interrupts_press,
                lift_finishes_press: behavior.lift_finishes_press,
                lift_alpha_start: behavior.lift_alpha_start,
                lift_zoom_start: behavior.lift_zoom_start,
                duration: behavior.duration,
                alpha_start: behavior.alpha_start,
                alpha_end: behavior.alpha_end,
                zoom_start: behavior.zoom_start,
                zoom_end: behavior.zoom_end,
                tween: tween(behavior.tween),
                blend_add: behavior.blend_add,
            };
            // Step gameplay's timers through a press held for the preview's
            // hold time and its release edge, then compare away from the
            // frame-quantized transitions.
            let dt = 1.0 / 2000.0;
            let mut timers = receptor_glow_press_timers(gameplay);
            for frame in 0..1000 {
                let age = frame as f32 * dt;
                let lane_pressed = age < RECEPTOR_PRESS_HOLD_SECONDS;
                let expected = receptor_glow_visual(
                    gameplay,
                    GameplayReceptorGlowState {
                        press_timer: timers.press_timer,
                        lift_timer: timers.lift_timer,
                        lift_start_alpha: timers.lift_start_alpha,
                        lift_start_zoom: timers.lift_start_zoom,
                        lane_pressed,
                    },
                );
                let actual = receptor_glow_after_hit(behavior, age);
                let next_pressed = age + dt < RECEPTOR_PRESS_HOLD_SECONDS;
                if lane_pressed && !next_pressed {
                    timers = receptor_glow_release_timers(gameplay, timers.press_timer);
                }
                timers = tick_receptor_glow_timers(gameplay, timers, next_pressed, dt);
                let lift_at = if behavior.lift_interrupts_press {
                    RECEPTOR_PRESS_HOLD_SECONDS
                } else {
                    RECEPTOR_PRESS_HOLD_SECONDS.max(behavior.press_duration)
                };
                let lift = if behavior.duration > 0.0 {
                    behavior.duration
                } else {
                    deadsync_gameplay::RECEPTOR_GLOW_DURATION
                };
                let edges = [
                    RECEPTOR_PRESS_HOLD_SECONDS,
                    behavior.press_duration,
                    lift_at + lift,
                ];
                if edges.iter().any(|edge| (age - edge).abs() < 4.0 * dt) {
                    continue;
                }
                match (expected, actual) {
                    (None, None) => {}
                    (Some(expected), Some(actual)) => {
                        assert!(
                            (expected.0 - actual.0).abs() < 0.02
                                && (expected.1 - actual.1).abs() < 0.02,
                            "{behavior:?} at {age}: gameplay {expected:?}, preview {actual:?}"
                        );
                    }
                    _ => panic!("{behavior:?} at {age}: gameplay {expected:?}, preview {actual:?}"),
                }
            }
        }
    }

    #[test]
    fn explosions_face_their_columns_and_stop_when_finished() {
        let skin = dance_skin("default");
        let draw = |col, age| {
            let mut actors = Vec::new();
            draw_explosion(&mut actors, &skin, col, CENTER, 64.0, 10, age, 0.5, None);
            actors
        };
        let mut rotations = [0.0; 4];
        for (col, rotation) in rotations.iter_mut().enumerate() {
            let duration = explosion_duration(&skin, col);
            assert!(duration > 0.0, "col {col}");
            let actors = draw(col, 0.0);
            assert!(!actors.is_empty(), "col {col}");
            *rotation = sprite_rotation(&actors[0]);
            assert!(!draw(col, duration * 0.5).is_empty(), "col {col}");
            for age in [duration, duration + 0.5, -0.01, f32::NAN] {
                assert!(draw(col, age).is_empty(), "col {col} age {age}");
            }
        }
        assert_dance_facings(rotations);
    }

    /// Every piece the field helpers draw, in every column and quantization,
    /// pressed and idle, with or without a model cache.
    fn field_actors(skin: &Noteskin, mut cache: Option<&mut ModelMeshCache>) -> Vec<Actor> {
        let mut actors = Vec::new();
        for col in 0..4 {
            for quant in 0..NUM_QUANTIZATIONS {
                for lift in [false, true] {
                    draw_tap(
                        &mut actors,
                        skin,
                        col,
                        quant,
                        lift,
                        CENTER,
                        32.0,
                        1.0,
                        10,
                        0.3,
                        0.6,
                        cache.as_deref_mut(),
                    );
                }
            }
            for (quant, roll, active) in [(0, false, false), (3, true, false), (1, false, true)] {
                draw_hold(
                    &mut actors,
                    skin,
                    col,
                    quant,
                    roll,
                    active,
                    CENTER,
                    CENTER[1] + 150.0,
                    [CENTER[1], CENTER[1] + 400.0],
                    32.0,
                    1.0,
                    10,
                    0.3,
                    0.6,
                    cache.as_deref_mut(),
                );
            }
            draw_mine(
                &mut actors,
                skin,
                col,
                CENTER,
                32.0,
                1.0,
                10,
                0.3,
                0.6,
                cache.as_deref_mut(),
            );
            for press_age in [None, Some(0.0), Some(0.05), Some(0.2)] {
                draw_receptor(
                    &mut actors,
                    skin,
                    col,
                    CENTER,
                    32.0,
                    1.0,
                    10,
                    0.3,
                    0.6,
                    press_age,
                    cache.as_deref_mut(),
                );
            }
            for age in [0.0, explosion_duration(skin, col) * 0.5] {
                draw_explosion(
                    &mut actors,
                    skin,
                    col,
                    CENTER,
                    32.0,
                    10,
                    age,
                    0.6,
                    cache.as_deref_mut(),
                );
            }
        }
        actors
    }

    /// The field slots are the whole of what a field samples, so a caller that
    /// readies only those never draws a texture it did not load.
    #[test]
    fn the_field_slots_cover_every_texture_a_field_draws() {
        for name in ["default", "cyber"] {
            let skin = dance_skin(name);
            let mut keys: Vec<Arc<str>> = Vec::new();
            for_each_field_slot(&skin, 4, |slot| {
                keys.push(slot.texture_key_shared());
            });
            let actors = field_actors(&skin, None);
            assert!(!actors.is_empty(), "{name}");
            for actor in &actors {
                let key = match actor {
                    Actor::Sprite { source, .. } => source.texture_key(),
                    Actor::TexturedMesh { texture, .. } => texture.texture_key(),
                    _ => None,
                }
                .unwrap_or_else(|| panic!("{name}: an untextured piece {actor:?}"));
                assert!(
                    keys.iter().any(|known| known.as_ref() == key),
                    "{name}: '{key}' is drawn but not a field slot's"
                );
            }
            // and only that: top caps, other judgments' explosions and the
            // like are left for gameplay to load.
            let mut all: Vec<Arc<str>> = Vec::new();
            skin.for_each_slot(|slot| all.push(slot.texture_key_shared()));
            for list in [&mut keys, &mut all] {
                list.sort_unstable();
                list.dedup();
            }
            assert!(
                keys.len() < all.len(),
                "{name}: {} of {}",
                keys.len(),
                all.len()
            );
        }
    }

    /// A hold's body is tiles of its texture's own height at the field's zoom,
    /// counted from the tail: whole ones end on it, and the head's is a part
    /// one cut from the texture's top. Clipped short of the tail, the last
    /// tile keeps its place in the pattern; the cap hangs under the tail only
    /// while the tail is in bounds.
    #[test]
    fn a_hold_body_is_tiled_from_its_tail() {
        let skin = dance_skin("default");
        let visuals = skin.hold_visuals_for_col(0, false);
        let body = visuals.body_inactive.as_ref().expect("a body");
        let cap = visuals.bottomcap_inactive.as_ref().expect("a cap");
        let (body_key, cap_key) = (body.texture_key_shared(), cap.texture_key_shared());
        assert_ne!(body_key, cap_key);
        let tile = body.source_size()[1] as f32 * 0.5;
        let cap_h = cap.source_size()[1] as f32 * 0.5;
        let phase = skin.part_uv_phase(NoteAnimPart::HoldBody, 0.0, 0.0, 0.0);
        let [_, top, _, bottom] = body.uv_for_note_at(
            body.frame_index_from_phase(phase),
            0.0,
            skin.part_uv_translation(NoteAnimPart::HoldBody, 0.0, false),
        );
        let v = |at: f32| (bottom - top).mul_add(at, top);
        // (top, bottom, v0, v1) of each piece drawn from `key`, top down
        let pieces = |head: f32, tail: f32, bounds: [f32; 2], key: &str| {
            let mut actors = Vec::new();
            draw_hold(
                &mut actors,
                &skin,
                0,
                0,
                false,
                false,
                [50.0, head],
                tail,
                bounds,
                32.0,
                1.0,
                10,
                0.0,
                0.0,
                None,
            );
            let mut pieces: Vec<[f32; 4]> = actors
                .iter()
                .filter_map(|actor| match actor {
                    Actor::Sprite {
                        offset,
                        size: [_, SizeSpec::Px(height)],
                        source,
                        uv_rect: Some(uv),
                        ..
                    } if source.texture_key() == Some(key) => Some([
                        offset[1] - height * 0.5,
                        offset[1] + height * 0.5,
                        uv[1],
                        uv[3],
                    ]),
                    _ => None,
                })
                .collect();
            pieces.sort_by(|a, b| a[0].total_cmp(&b[0]));
            pieces
        };
        #[track_caller]
        fn close(actual: &[[f32; 4]], expected: &[[f32; 4]]) {
            assert_eq!(actual.len(), expected.len(), "{actual:?} vs {expected:?}");
            for (a, e) in actual.iter().zip(expected) {
                assert!(
                    a.iter().zip(e).all(|(a, e)| (a - e).abs() < 1e-3),
                    "{actual:?} vs {expected:?}"
                );
            }
        }

        let (head, tail) = (100.0, tile.mul_add(2.5, 100.0));
        let open = [0.0, 1000.0];
        close(
            &pieces(head, tail, open, &body_key),
            &[
                [head, tile.mul_add(0.5, head), v(0.5), v(1.0)],
                [
                    tile.mul_add(0.5, head),
                    tile.mul_add(1.5, head),
                    v(0.0),
                    v(1.0),
                ],
                [tile.mul_add(1.5, head), tail, v(0.0), v(1.0)],
            ],
        );
        let caps = pieces(head, tail, open, &cap_key);
        assert_eq!(caps.len(), 1);
        assert!((caps[0][0] - tail).abs() < 1e-3 && (caps[0][1] - (tail + cap_h)).abs() < 1e-3);

        let clipped = [tile.mul_add(0.25, head), tile.mul_add(2.0, head)];
        close(
            &pieces(head, tail, clipped, &body_key),
            &[
                [clipped[0], tile.mul_add(0.5, head), v(0.75), v(1.0)],
                [
                    tile.mul_add(0.5, head),
                    tile.mul_add(1.5, head),
                    v(0.0),
                    v(1.0),
                ],
                [tile.mul_add(1.5, head), clipped[1], v(0.0), v(0.5)],
            ],
        );
        assert!(pieces(head, tail, clipped, &cap_key).is_empty());
    }

    /// A model cache changes how geometry is kept, never what is drawn: the
    /// same actors as building it per draw, with one shared buffer per slot
    /// under a stable key.
    #[test]
    fn a_model_cache_shares_geometry_without_changing_the_picture() {
        let skin = dance_skin("cyber");
        let mut cache = ModelMeshCache::default();
        let cached = field_actors(&skin, Some(&mut cache));
        let again = field_actors(&skin, Some(&mut cache));
        let built = field_actors(&skin, None);
        assert_eq!(cached.len(), built.len());
        assert_eq!(again.len(), built.len());
        let keyless = |actor: &Actor| {
            let mut actor = actor.clone();
            if let Actor::TexturedMesh { geom_cache_key, .. } = &mut actor {
                *geom_cache_key = deadlib_render_core::INVALID_TMESH_CACHE_KEY;
            }
            format!("{actor:?}")
        };
        let mut meshes = 0;
        for ((cached, again), built) in cached.iter().zip(&again).zip(&built) {
            assert_eq!(keyless(cached), keyless(built));
            if let (
                Actor::TexturedMesh {
                    vertices,
                    geom_cache_key,
                    ..
                },
                Actor::TexturedMesh {
                    vertices: next,
                    geom_cache_key: next_key,
                    ..
                },
            ) = (cached, again)
            {
                meshes += 1;
                assert!(Arc::ptr_eq(vertices, next), "built once, then shared");
                assert_eq!(geom_cache_key, next_key);
                assert_ne!(
                    *geom_cache_key,
                    deadlib_render_core::INVALID_TMESH_CACHE_KEY
                );
            }
        }
        assert!(meshes > 0, "cyber draws its notes as models");
    }
}
