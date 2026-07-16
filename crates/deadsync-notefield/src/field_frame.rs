use crate::{
    HoldBodyCapRequest, HoldComposeControl, HoldEntryPlanRequest, HoldMeshScratch, HoldPathSample,
    MeasureComposeRequest, MeasureLineMode, MineLayerRequest, ModelMeshCache, NoteAlphaParams,
    NoteLayerRequest, NotePlacement, NoteXParams, NotefieldComposeRequest,
    NotefieldFeedbackFrameView, NotefieldPlacementPlan, NotefieldPlacementScratch,
    PreparedNotefield, PreparedNotefieldNotes, TornadoBounds, VisualEffectParams,
    appearance_note_actor_alpha, appearance_note_actor_alpha_from_alpha, appearance_note_alpha,
    appearance_note_glow, appearance_note_glow_from_alpha, compose_hold_body_caps,
    compose_measure_lines, compose_mine_layers, compose_note_layer, compose_notefield_feedback,
    for_each_visible_hold_index, for_each_visible_note_index, gameplay_visual_effect_params,
    hold_entry_head_beat, hold_entry_plan, hold_overlaps_visible_window, hold_parts_for_note_type,
    mine_hides_after_resolution, mine_part, note_world_z_for_bumpy,
    note_x_offset as canonical_note_x_offset, notefield_view_proj, offset_center,
    receptor_row_center as canonical_receptor_row_center, scale_sprite_to_arrow, share_actor_range,
    song_lua_note_model_draw, tap_part_for_note_type, tap_replacement_head, translated_uv_rect,
    visual_arrow_effect_zoom, visual_hold_body_needs_z_buffer, visual_note_rotation_z,
    visual_pulse_zoom_for_y, visual_tiny_zoom, visual_use_legacy_hold_sprites,
};
use deadlib_present::actors::{Actor, SpriteSource};
use deadlib_render::BlendMode;
use deadsync_core::note::NoteType;
use deadsync_gameplay::{
    AppearanceEffects, CompletedRowVisibility, VisualEffects, hold_head_render_flags,
    song_lua_note_hidden,
};
use deadsync_noteskin::{NUM_QUANTIZATIONS, NoteskinSlot};
use deadsync_rules::note::HoldResult;
use std::sync::Arc;

/// Borrowed runtime state needed by the canonical field actor pass.
///
/// The concrete theme resolves gameplay state into this fixed-size view;
/// canonical notefield code owns all actor selection, placement, and ordering.
#[derive(Clone, Copy, Debug)]
pub struct NotefieldFieldFrameView<'a> {
    pub feedback: NotefieldFeedbackFrameView<'a>,
    pub completed_rows: CompletedRowVisibility<'a>,
}

/// Optional shared field capture produced while preserving the live actor tree.
#[derive(Clone, Debug, Default)]
pub struct NotefieldFieldResult {
    pub captured_actors: Vec<Arc<[Actor]>>,
}

/// Compose the complete canonical playfield pass in display order:
/// measure lines/cues, receptor feedback, holds, taps/mines, then camera wrap.
pub fn compose_notefield_field<S, F>(
    actors: &mut Vec<Actor>,
    cue_hud_actors: &mut Vec<Actor>,
    model_cache: &mut ModelMeshCache,
    hold_mesh_scratch: &mut HoldMeshScratch,
    placement_scratch: &mut NotefieldPlacementScratch,
    request: &NotefieldComposeRequest<'_, S>,
    prepared: &PreparedNotefield<'_, S>,
    frame: &NotefieldFieldFrameView<'_>,
    sprite_source: &F,
) -> NotefieldFieldResult
where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
{
    model_cache.begin_frame();
    hold_mesh_scratch.begin_frame();
    let field_start = actors.len();
    actors.reserve(prepared.frame_plan.field_actor_reserve);
    cue_hud_actors.reserve(prepared.frame_plan.hud_actor_reserve);
    let Some(notes) = prepared.notes.as_ref() else {
        return NotefieldFieldResult::default();
    };

    compose_field_contents(
        actors,
        cue_hud_actors,
        model_cache,
        hold_mesh_scratch,
        placement_scratch,
        request,
        prepared,
        notes,
        frame,
        sprite_source,
    );
    wrap_field_camera(actors, field_start, request, prepared);

    let captured_actors = request
        .capture_requests
        .note_field
        .then(|| share_actor_range(actors, field_start))
        .flatten()
        .unwrap_or_default();
    NotefieldFieldResult { captured_actors }
}

#[allow(clippy::too_many_arguments)]
fn compose_field_contents<S, F>(
    actors: &mut Vec<Actor>,
    cue_hud_actors: &mut Vec<Actor>,
    model_cache: &mut ModelMeshCache,
    hold_mesh_scratch: &mut HoldMeshScratch,
    placement_scratch: &mut NotefieldPlacementScratch,
    request: &NotefieldComposeRequest<'_, S>,
    prepared: &PreparedNotefield<'_, S>,
    note_inputs: &PreparedNotefieldNotes<'_, S>,
    frame: &NotefieldFieldFrameView<'_>,
    sprite_source: &F,
) where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
{
    let style = request.style;
    let options = &request.options;
    let elapsed_screen = request.visual.elapsed_screen_s;
    let visual = request.visual.visual;
    let appearance = request.visual.appearance;
    let spacing_mult = request.visual.spacing_multiplier;
    let frame_plan = prepared.frame_plan;
    let col_start = frame_plan.col_start;
    let num_cols = frame_plan.num_cols;
    let col_end = col_start + num_cols;
    let field_zoom = prepared.field_zoom;
    let scroll_speed = prepared.scroll_speed;
    let draw_distance_before_targets = request.geometry.draw_distance_before_targets;
    let draw_distance_after_targets = request.geometry.draw_distance_after_targets;
    let current_beat = prepared.current_beat;
    let field = prepared.field;
    let playfield_center_x = field.playfield_center_x;
    let column_dirs = field.column_dirs;
    let column_receptor_ys = field.column_receptor_ys;
    let mini = prepared.mini;
    let ns = note_inputs.base;
    let target_arrow_px = note_inputs.target_arrow_px;
    let scale_sprite =
        |size: [i32; 2]| -> [f32; 2] { scale_sprite_to_arrow(size, target_arrow_px) };
    let scale_mine_slot = |slot: &S| -> [f32; 2] {
        // Model-backed mines preserve native geometry scale. Sprite mines use
        // the same arrow-normalized path as ITG NoteDisplay::DrawTap.
        if let Some(model) = slot.model() {
            let model_size = model.size();
            if model_size[0] > f32::EPSILON && model_size[1] > f32::EPSILON {
                return [model_size[0] * field_zoom, model_size[1] * field_zoom];
            }
        }
        scale_sprite(slot.size())
    };
    let note_rotation_y = 0.0_f32;
    let prefer_sprite_note_path = false;
    let flat_tap_face_rotation_y = 0.0_f32;
    let beat_push = note_inputs.beat_factor;
    let col_offsets = note_inputs.col_offsets;
    let invert_distances = note_inputs.invert_distances;
    let tornado_bounds = note_inputs.tornado_bounds;
    let travel = &note_inputs.travel;
    let (note_start, note_end) = request.chart.note_range;
    let lane_center_x_from_travel = |local_col: usize, travel_offset: f32| -> f32 {
        playfield_center_x
            + note_x_offset(
                request.geometry.screen_height,
                local_col,
                travel.adjusted(travel_offset),
                travel.arrow_effect_time_s(),
                beat_push,
                visual,
                &col_offsets[..num_cols],
                &invert_distances[..num_cols],
                &tornado_bounds[..num_cols],
            )
    };
    let lane_center_x_from_adjusted_travel = |local_col: usize, adjusted_travel: f32| -> f32 {
        playfield_center_x
            + note_x_offset(
                request.geometry.screen_height,
                local_col,
                adjusted_travel,
                travel.arrow_effect_time_s(),
                beat_push,
                visual,
                &col_offsets[..num_cols],
                &invert_distances[..num_cols],
                &tornado_bounds[..num_cols],
            )
    };
    let actor_alpha_for_travel = |local_col: usize, travel_offset: f32| -> f32 {
        let adjusted = travel.adjusted(travel_offset);
        note_actor_alpha(
            adjusted + travel.lane_offset(local_col),
            elapsed_screen,
            mini,
            appearance,
        )
    };
    let glow_for_travel = |local_col: usize, travel_offset: f32| -> f32 {
        let adjusted = travel.adjusted(travel_offset);
        note_glow(
            adjusted + travel.lane_offset(local_col),
            elapsed_screen,
            mini,
            appearance,
        )
    };
    let world_z_for_raw_travel = |local_col: usize, travel_offset: f32| -> f32 {
        note_world_z_for_bumpy(
            travel.adjusted(travel_offset),
            gameplay_visual_effect_params(&visual, local_col).bumpy,
            visual.bumpy_offset,
            visual.bumpy_period,
        )
    };
    let world_z_for_adjusted_travel = |local_col: usize, travel_offset: f32| -> f32 {
        note_world_z_for_bumpy(
            travel_offset,
            gameplay_visual_effect_params(&visual, local_col).bumpy,
            visual.bumpy_offset,
            visual.bumpy_period,
        )
    };
    let placement_plan = build_note_placement_plan(
        placement_scratch,
        request,
        prepared,
        note_inputs,
        frame.completed_rows,
        &lane_center_x_from_adjusted_travel,
        &world_z_for_adjusted_travel,
    );
    let visible_row_range = placement_plan.visible_row_range;

    let measure_line_mode = if request.view.edit_beat_bars {
        MeasureLineMode::Edit
    } else {
        options.frame_features.measure_line_mode
    };
    compose_measure_lines(
        actors,
        MeasureComposeRequest {
            mode: measure_line_mode,
            show_cues: options.frame_features.measure_cues,
            style,
            column_xs: &note_inputs.measure_column_xs,
            column_dirs: &column_dirs,
            column_receptor_ys: &column_receptor_ys,
            num_cols,
            spacing_multiplier: spacing_mult,
            field_zoom,
            playfield_center_x,
            screen_height: request.geometry.screen_height,
            current_beat,
            scroll_speed,
            scroll_reference_bpm: request.chart.scroll_reference_bpm,
            music_rate: request.chart.music_rate,
            time_signatures: request.chart.time_signatures,
            bpms: request.chart.bpms,
            stops: request.chart.stops,
            delays: request.chart.delays,
            scrolls: request.chart.scrolls,
            travel,
        },
    );
    compose_notefield_feedback(
        actors,
        cue_hud_actors,
        model_cache,
        request,
        prepared,
        &frame.feedback,
        sprite_source,
    );

    let mut render_hold = |note_index: usize| {
        let note = &request.chart.notes[note_index];
        if note.column < col_start || note.column >= col_end {
            return;
        }
        let local_col = note.column - col_start;
        if !matches!(note.note_type, NoteType::Hold | NoteType::Roll) {
            return;
        }
        if song_lua_note_hidden(request.song_lua.note_hides, local_col, note.beat) {
            return;
        }
        let Some(hold) = &note.hold else {
            return;
        };
        if matches!(hold.result, Some(HoldResult::Held)) {
            return;
        }

        let is_head_dynamic = hold.let_go_started_at.is_some()
            || matches!(hold.result, Some(HoldResult::LetGo | HoldResult::Missed));
        let head_beat = hold_entry_head_beat(
            note.beat,
            hold.end_beat,
            hold.last_held_beat,
            current_beat,
            is_head_dynamic,
        );
        let col_dir = column_dirs[local_col];
        let dir = col_dir;
        let lane_receptor_y = column_receptor_ys[local_col];
        let receptor_center = receptor_row_center(
            request.geometry.screen_height,
            playfield_center_x,
            local_col,
            lane_receptor_y,
            request.arrow_effect_time_s,
            beat_push,
            visual,
            &col_offsets[..num_cols],
            &invert_distances[..num_cols],
            &tornado_bounds[..num_cols],
        );
        let receptor_draw_y = receptor_center[1];
        let receptor_center_x = receptor_center[0];
        let head_travel_offset = if is_head_dynamic {
            travel.raw_beat(head_beat)
        } else {
            travel.raw_note(note, false)
        };
        let tail_travel_offset = travel.raw_note(note, true);
        let head_y = travel.lane_y(local_col, lane_receptor_y, dir, head_travel_offset);
        let tail_y = travel.lane_y(local_col, lane_receptor_y, dir, tail_travel_offset);
        let note_display = ns.note_display_metrics;
        let lane_reverse = col_dir < 0.0;
        let active_state = frame.feedback.lanes[local_col]
            .active_hold
            .filter(|active| active.note_index == note_index);
        let (engaged, use_active) = hold_head_render_flags(active_state, current_beat, note.beat);
        let visuals = ns.hold_visuals_for_col(local_col, matches!(note.note_type, NoteType::Roll));
        let hold_parts = hold_parts_for_note_type(note.note_type);
        let hold_part_phase =
            ns.part_uv_phase(hold_parts.head, elapsed_screen, current_beat, note.beat);
        let hold_body_phase =
            ns.part_uv_phase(hold_parts.body, elapsed_screen, current_beat, note.beat);
        let hold_topcap_phase =
            ns.part_uv_phase(hold_parts.topcap, elapsed_screen, current_beat, note.beat);
        let hold_bottomcap_phase = ns.part_uv_phase(
            hold_parts.bottomcap,
            elapsed_screen,
            current_beat,
            note.beat,
        );
        let hold_plan = hold_entry_plan(HoldEntryPlanRequest {
            note_type: note.note_type,
            head_travel: head_travel_offset,
            tail_travel: tail_travel_offset,
            head_y,
            tail_y,
            receptor_y: receptor_draw_y,
            screen_height: request.geometry.screen_height,
            lane_reverse,
            engaged,
            use_active,
            flip_body_reverse: note_display.flip_hold_body_when_reverse,
            flip_head_tail_reverse: note_display.flip_head_and_tail_when_reverse,
            start_body_offset: note_display.start_drawing_hold_body_offset_from_head,
            stop_body_offset: note_display.stop_drawing_hold_body_offset_from_tail,
            let_go_gray: ns.hold_let_go_gray_percent,
            life: hold.life,
            head_phase: hold_part_phase,
            body_phase: hold_body_phase,
            top_cap_phase: hold_topcap_phase,
            bottom_cap_phase: hold_bottomcap_phase,
            visuals,
        });
        let body_flipped = hold_plan.body_flipped;
        let y_head = hold_plan.y_head;
        let y_tail = hold_plan.y_tail;
        let hold_diffuse = hold_plan.diffuse;
        let head_anchor_y = hold_plan.head_anchor_y;
        let head_anchor_travel = hold_plan.head_anchor_travel;
        let hold_parts = hold_plan.parts;
        let hold_part_phase = hold_plan.head_phase;
        let head_layers = hold_plan.head_layers;
        let head_slot = hold_plan.head_slot;

        let hold_tiny_zoom = tiny_zoom_for_col(&visual, local_col);
        let hold_base_target_arrow_px = target_arrow_px * hold_tiny_zoom;
        let hold_arrow_px_for_adjusted_travel = |travel_offset: f32| -> f32 {
            hold_base_target_arrow_px * pulse_zoom_for_y(travel_offset, &visual)
        };
        let hold_target_arrow_px = hold_arrow_px_for_adjusted_travel(0.0);
        let hold_head_zoom =
            hold_tiny_zoom * pulse_zoom_for_y(travel.adjusted(head_anchor_travel), &visual);
        let hold_head_target_arrow_px = target_arrow_px * hold_head_zoom;
        let hold_note_scale = field_zoom * hold_head_zoom;
        let col_bumpy = gameplay_visual_effect_params(&visual, local_col).bumpy;
        let hold_depth_test = hold_body_needs_z_buffer(&visual);
        let use_legacy_hold_sprites = visual_use_legacy_hold_sprites(
            col_bumpy,
            visual.drunk,
            visual.tornado,
            visual.beat,
            visual.pulse_outer,
        );
        let sample_hold_path = |screen_y: f32| {
            let adjusted_travel =
                travel.adjusted_from_screen_y(local_col, lane_receptor_y, dir, screen_y);
            HoldPathSample {
                adjusted_travel,
                center_x: lane_center_x_from_adjusted_travel(local_col, adjusted_travel),
                world_z: world_z_for_adjusted_travel(local_col, adjusted_travel),
                arrow_px: hold_arrow_px_for_adjusted_travel(adjusted_travel),
            }
        };
        if compose_hold_body_caps(
            actors,
            hold_mesh_scratch,
            HoldBodyCapRequest {
                body_slot: hold_plan.body_slot,
                top_cap_slot: hold_plan.top_cap_slot,
                bottom_cap_slot: hold_plan.bottom_cap_slot,
                y_head,
                y_tail,
                draw_span: hold_plan.draw_span,
                body_flipped,
                lane_reverse,
                top_anchor_reverse: note_display.top_hold_anchor_when_reverse,
                body_phase: hold_plan.body_phase,
                top_cap_phase: hold_plan.top_cap_phase,
                bottom_cap_phase: hold_plan.bottom_cap_phase,
                body_uv_translation: ns.part_uv_translation(hold_parts.body, note.beat, false),
                top_cap_uv_translation: ns.part_uv_translation(hold_parts.topcap, note.beat, false),
                bottom_cap_uv_translation: ns.part_uv_translation(
                    hold_parts.bottomcap,
                    note.beat,
                    false,
                ),
                target_arrow_px: hold_target_arrow_px,
                diffuse: hold_diffuse,
                elapsed_s: elapsed_screen,
                mini,
                lane_offset: travel.lane_offset(local_col),
                appearance: note_alpha_params(appearance),
                use_legacy_sprites: use_legacy_hold_sprites,
                rotation_y_deg: note_rotation_y,
                depth_test: hold_depth_test,
                screen_height: request.geometry.screen_height,
                body_z: style.actors.hold_body_z,
                cap_z: style.actors.hold_cap_z,
                glow_z: style.actors.hold_glow_z,
            },
            &sample_hold_path,
            sprite_source,
        ) == HoldComposeControl::AbortHold
        {
            return;
        }

        let head_draw_y = head_anchor_y;
        let head_draw_delta = (head_draw_y - receptor_draw_y) * dir;
        if head_draw_delta < -draw_distance_after_targets
            || head_draw_delta > draw_distance_before_targets
        {
            return;
        }
        let head_alpha = actor_alpha_for_travel(local_col, head_anchor_travel);
        let head_glow = glow_for_travel(local_col, head_anchor_travel);
        if head_alpha <= f32::EPSILON && head_glow <= f32::EPSILON {
            return;
        }
        let hold_head_rot = calc_note_rotation_z(visual, note.beat, current_beat, true, local_col);
        let note_idx = local_col * NUM_QUANTIZATIONS + note.quantization_idx as usize;
        let head_center_x = if (head_draw_y - receptor_draw_y).abs() <= 0.5 {
            receptor_center_x
        } else {
            lane_center_x_from_travel(local_col, head_anchor_travel)
        };
        let head_center = [head_center_x, head_draw_y];
        let head_world_z = world_z_for_raw_travel(local_col, head_anchor_travel);
        let elapsed = elapsed_screen;
        let hold_head_translation = ns.part_uv_translation(hold_parts.head, note.beat, false);
        let head_slot = head_slot.and_then(|slot| {
            let draw = song_lua_note_model_draw(
                model_cache.draw_at(slot, elapsed, current_beat),
                note_rotation_y,
            );
            if !draw.visible {
                return None;
            }
            let note_scale = hold_note_scale;
            let base_size = note_slot_base_size(slot, note_scale);
            (base_size[0] * draw.zoom[0].max(0.0) > f32::EPSILON
                && base_size[1] * draw.zoom[1].max(0.0) > f32::EPSILON)
                .then_some((slot, draw, note_scale, base_size))
        });
        if let Some((head_slot, draw, note_scale, base_size)) = head_slot {
            let frame_index = head_slot.frame_index_from_phase(hold_part_phase);
            let uv_elapsed = if head_slot.model().is_some() {
                hold_part_phase
            } else {
                elapsed
            };
            let uv = translated_uv_rect(
                head_slot.uv_for_frame_at(frame_index, uv_elapsed),
                hold_head_translation,
            );
            let local_offset = [draw.pos[0] * note_scale, draw.pos[1] * note_scale];
            let local_offset_rot_sin_cos = head_slot.base_rot_sin_cos();
            let model_center = model_center(
                head_slot,
                head_center,
                local_offset,
                local_offset_rot_sin_cos,
            );
            let size = [
                base_size[0] * draw.zoom[0].max(0.0),
                base_size[1] * draw.zoom[1].max(0.0),
            ];
            if size[0] <= f32::EPSILON || size[1] <= f32::EPSILON {
                return;
            }
            let color = [
                draw.tint[0] * hold_diffuse[0],
                draw.tint[1] * hold_diffuse[1],
                draw.tint[2] * hold_diffuse[2],
                draw.tint[3] * hold_diffuse[3] * head_alpha,
            ];
            let blend = if draw.blend_add {
                BlendMode::Add
            } else {
                BlendMode::Alpha
            };
            let rotation = -head_slot.sprite_def().rotation_deg as f32;
            compose_note_layer(
                actors,
                model_cache,
                NoteLayerRequest {
                    slot: head_slot,
                    draw,
                    model_center,
                    sprite_center: offset_center(
                        head_center,
                        local_offset,
                        local_offset_rot_sin_cos,
                    ),
                    size,
                    uv,
                    rotation_y_deg: flat_tap_face_rotation_y,
                    model_rotation_z_deg: rotation + hold_head_rot,
                    sprite_rotation_z_deg: draw.rot[2] + rotation + hold_head_rot,
                    tint: color,
                    glow_alpha: head_glow,
                    blend,
                    z: style.actors.note_z,
                    world_z: head_world_z,
                    prefer_sprite: prefer_sprite_note_path,
                },
                sprite_source,
            );
        } else if let Some(note_slots) =
            head_layers.or_else(|| ns.note_layers.get(note_idx).map(|layers| layers.as_ref()))
        {
            let note_scale = hold_note_scale;
            for note_slot in note_slots {
                compose_noteskin_layer(
                    actors,
                    model_cache,
                    note_slot,
                    head_center,
                    note_scale,
                    hold_part_phase,
                    hold_head_translation,
                    elapsed,
                    current_beat,
                    note_rotation_y,
                    flat_tap_face_rotation_y,
                    hold_head_rot,
                    [
                        hold_diffuse[0],
                        hold_diffuse[1],
                        hold_diffuse[2],
                        hold_diffuse[3] * head_alpha,
                    ],
                    head_glow,
                    style.actors.note_z,
                    head_world_z,
                    prefer_sprite_note_path,
                    sprite_source,
                );
            }
        } else if let Some(note_slot) = ns.notes.get(note_idx) {
            let frame_index = note_slot.frame_index_from_phase(hold_part_phase);
            let uv_elapsed = if note_slot.model().is_some() {
                hold_part_phase
            } else {
                elapsed
            };
            let uv = translated_uv_rect(
                note_slot.uv_for_frame_at(frame_index, uv_elapsed),
                hold_head_translation,
            );
            let size = scale_sprite_to_arrow(note_slot.size(), hold_head_target_arrow_px);
            let draw = song_lua_note_model_draw(
                model_cache.draw_at(note_slot, elapsed, current_beat),
                note_rotation_y,
            );
            let rotation = -note_slot.sprite_def().rotation_deg as f32;
            compose_note_layer(
                actors,
                model_cache,
                NoteLayerRequest {
                    slot: note_slot,
                    draw,
                    model_center: head_center,
                    sprite_center: head_center,
                    size,
                    uv,
                    rotation_y_deg: flat_tap_face_rotation_y,
                    model_rotation_z_deg: rotation + hold_head_rot,
                    sprite_rotation_z_deg: rotation + hold_head_rot,
                    tint: [
                        hold_diffuse[0],
                        hold_diffuse[1],
                        hold_diffuse[2],
                        hold_diffuse[3] * head_alpha,
                    ],
                    glow_alpha: head_glow,
                    blend: BlendMode::Alpha,
                    z: style.actors.note_z,
                    world_z: head_world_z,
                    prefer_sprite: prefer_sprite_note_path,
                },
                sprite_source,
            );
        }
    };

    for local_col in 0..num_cols {
        let col = col_start + local_col;
        for_each_visible_hold_index(
            request.chart.lane_hold_indices[col],
            request.chart.notes,
            visible_row_range,
            &mut render_hold,
        );
    }
    for note_index in frame
        .feedback
        .lanes
        .iter()
        .take(num_cols)
        .filter_map(|lane| lane.active_hold.map(|hold| hold.note_index))
        .chain(request.chart.decaying_hold_indices.iter().copied())
        .filter(|&idx| {
            idx >= note_start
                && idx < note_end
                && !hold_overlaps_visible_window(idx, request.chart.notes, visible_row_range)
        })
    {
        render_hold(note_index);
    }
    compose_visible_notes(
        actors,
        model_cache,
        request,
        prepared,
        note_inputs,
        placement_plan,
        &scale_mine_slot,
        sprite_source,
    );
}

#[allow(clippy::too_many_arguments)]
fn build_note_placement_plan<'a, S>(
    scratch: &'a mut NotefieldPlacementScratch,
    request: &NotefieldComposeRequest<'_, S>,
    prepared: &PreparedNotefield<'_, S>,
    notes: &PreparedNotefieldNotes<'_, S>,
    completed_rows: CompletedRowVisibility<'_>,
    lane_center_x: &impl Fn(usize, f32) -> f32,
    world_z: &impl Fn(usize, f32) -> f32,
) -> NotefieldPlacementPlan<'a> {
    debug_assert!(u32::try_from(request.chart.notes.len()).is_ok());
    let travel = &notes.travel;
    let visible_row_range = scratch.begin_frame(travel);
    let frame_plan = prepared.frame_plan;
    let col_start = frame_plan.col_start;
    let num_cols = frame_plan.num_cols;
    let alpha_params = note_alpha_params(request.visual.appearance);
    let elapsed = request.visual.elapsed_screen_s;
    let mini = prepared.mini;
    for local_col in 0..num_cols {
        let col = col_start + local_col;
        let direction = prepared.field.column_dirs[local_col];
        let receptor_y = prepared.field.column_receptor_ys[local_col];
        let lane_offset = travel.lane_offset(local_col);
        for_each_visible_note_index(
            request.chart.lane_note_row_indices[col],
            request.chart.notes,
            visible_row_range,
            |note_index| {
                let note = &request.chart.notes[note_index];
                if matches!(note.note_type, NoteType::Hold | NoteType::Roll)
                    || song_lua_note_hidden(request.song_lua.note_hides, local_col, note.beat)
                {
                    return;
                }
                if !note.is_fake {
                    if matches!(note.note_type, NoteType::Mine) {
                        if mine_hides_after_resolution(note.mine_result) {
                            return;
                        }
                    } else if note.result.is_some() && completed_rows.hides_note(note.row_index) {
                        return;
                    }
                }
                let adjusted_travel = travel.adjusted(travel.raw_note(note, false));
                if adjusted_travel < -request.geometry.draw_distance_after_targets
                    || adjusted_travel > request.geometry.draw_distance_before_targets
                {
                    return;
                }
                let percent_visible = appearance_note_alpha(
                    adjusted_travel + lane_offset,
                    elapsed,
                    mini,
                    alpha_params,
                );
                let actor_alpha = appearance_note_actor_alpha_from_alpha(percent_visible);
                let glow_alpha = appearance_note_glow_from_alpha(percent_visible);
                if actor_alpha <= f32::EPSILON && glow_alpha <= f32::EPSILON {
                    return;
                }
                scratch.push(NotePlacement {
                    note_index: note_index as u32,
                    local_col: local_col as u8,
                    adjusted_travel,
                    center: [
                        lane_center_x(local_col, adjusted_travel),
                        receptor_y + direction * adjusted_travel + lane_offset,
                    ],
                    actor_alpha,
                    glow_alpha,
                    world_z: world_z(local_col, adjusted_travel),
                });
            },
        );
    }
    scratch.plan(visible_row_range)
}

#[allow(clippy::too_many_arguments)]
fn compose_visible_notes<S, F>(
    actors: &mut Vec<Actor>,
    model_cache: &mut ModelMeshCache,
    request: &NotefieldComposeRequest<'_, S>,
    prepared: &PreparedNotefield<'_, S>,
    notes: &PreparedNotefieldNotes<'_, S>,
    placement_plan: NotefieldPlacementPlan<'_>,
    scale_mine_slot: &impl Fn(&S) -> [f32; 2],
    sprite_source: &F,
) where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
{
    let style = request.style;
    let elapsed = request.visual.elapsed_screen_s;
    let visual = request.visual.visual;
    let field_zoom = prepared.field_zoom;
    let current_beat = prepared.current_beat;
    let num_cols = prepared.frame_plan.num_cols;
    let ns = notes.base;
    let mine_ns = notes.mine;
    let note_display_time = elapsed * notes.note_display_time_scale;
    let mine_fill_phase = current_beat.rem_euclid(1.0);
    let draw_hold_same_row = ns.note_display_metrics.draw_hold_head_for_taps_on_same_row;
    let draw_roll_same_row = ns.note_display_metrics.draw_roll_head_for_taps_on_same_row;
    let tap_same_row_means_hold = ns.note_display_metrics.tap_hold_roll_on_row_means_hold;
    let note_rotation_y = 0.0_f32;
    let flat_tap_face_rotation_y = 0.0_f32;
    let prefer_sprite_note_path = false;

    let mut placement_start = 0;
    for local_col in 0..num_cols {
        let fill_slot = mine_ns.mines.get(local_col).and_then(|slot| slot.as_ref());
        let fill_gradient_slot = mine_ns
            .mine_fill_slots
            .get(local_col)
            .and_then(|slot| slot.as_ref());
        let frame_slot = mine_ns
            .mine_frames
            .get(local_col)
            .and_then(|slot| slot.as_ref());
        while placement_start < placement_plan.notes.len()
            && usize::from(placement_plan.notes[placement_start].local_col) < local_col
        {
            placement_start += 1;
        }
        let mut placement_end = placement_start;
        while placement_end < placement_plan.notes.len()
            && usize::from(placement_plan.notes[placement_end].local_col) == local_col
        {
            placement_end += 1;
        }
        for placement in &placement_plan.notes[placement_start..placement_end] {
            let note_index = placement.note_index as usize;
            let note = &request.chart.notes[note_index];
            let adjusted_travel = placement.adjusted_travel;
            let [column_center_x, y_pos] = placement.center;
            let note_alpha = placement.actor_alpha;
            let glow_alpha = placement.glow_alpha;
            let world_z = placement.world_z;
            let effect_zoom = visual_arrow_effect_zoom(
                adjusted_travel,
                gameplay_visual_effect_params(&visual, local_col),
            );
            let note_scale = field_zoom * effect_zoom;
            let target_arrow_px = notes.target_arrow_px * effect_zoom;
            let scale_mine_for_note = |slot: &S| -> [f32; 2] {
                let size = scale_mine_slot(slot);
                [size[0] * effect_zoom, size[1] * effect_zoom]
            };
            let note_rotation_z =
                calc_note_rotation_z(visual, note.beat, current_beat, false, local_col);

            if matches!(note.note_type, NoteType::Mine) {
                if fill_slot.is_none() && frame_slot.is_none() {
                    continue;
                }
                let mine_uv_phase = mine_ns.tap_mine_uv_phase(elapsed, current_beat, note.beat);
                let mine_translation = mine_ns.part_uv_translation(mine_part(), note.beat, false);
                let circle_reference = frame_slot
                    .map(&scale_mine_for_note)
                    .or_else(|| fill_slot.map(&scale_mine_for_note))
                    .unwrap_or([
                        request.geometry.target_arrow_pixel_size * note_scale,
                        request.geometry.target_arrow_pixel_size * note_scale,
                    ]);
                compose_mine_layers(
                    actors,
                    model_cache,
                    MineLayerRequest {
                        fill_slot,
                        gradient_slot: fill_gradient_slot,
                        frame_slot,
                        gradient_size: [
                            circle_reference[0] * style.actors.mine_core_size_ratio,
                            circle_reference[1] * style.actors.mine_core_size_ratio,
                        ],
                        center: [column_center_x, y_pos],
                        mine_uv_phase,
                        mine_fill_phase,
                        elapsed_s: elapsed,
                        display_time_s: note_display_time,
                        current_beat,
                        uv_translation: mine_translation,
                        rotation_y_deg: note_rotation_y,
                        note_rotation_z_deg: note_rotation_z,
                        alpha: note_alpha,
                        glow_alpha,
                        note_z: style.actors.note_z,
                        world_z,
                        prefer_sprite: prefer_sprite_note_path,
                    },
                    &scale_mine_for_note,
                    sprite_source,
                );
                continue;
            }

            let tap_part = tap_part_for_note_type(note.note_type);
            let tap_row_flags = request.chart.tap_row_flags(note_index);
            if let Some(replacement) = tap_replacement_head(
                note.note_type,
                tap_row_flags & 0b01 != 0,
                tap_row_flags & 0b10 != 0,
                draw_hold_same_row,
                draw_roll_same_row,
                tap_same_row_means_hold,
            ) {
                let visuals = ns.hold_visuals_for_col(local_col, replacement.is_roll);
                let part = replacement.part;
                let phase = ns.part_uv_phase(part, elapsed, current_beat, note.beat);
                let translation = ns.part_uv_translation(part, note.beat, false);
                let center = [column_center_x, y_pos];
                if let Some(head_slots) = visuals
                    .head_inactive_layers
                    .as_deref()
                    .or(visuals.head_active_layers.as_deref())
                {
                    for head_slot in head_slots {
                        compose_noteskin_layer(
                            actors,
                            model_cache,
                            head_slot,
                            center,
                            note_scale,
                            phase,
                            translation,
                            elapsed,
                            current_beat,
                            note_rotation_y,
                            flat_tap_face_rotation_y,
                            note_rotation_z,
                            [1.0, 1.0, 1.0, note_alpha],
                            glow_alpha,
                            style.actors.note_z,
                            world_z,
                            prefer_sprite_note_path,
                            sprite_source,
                        );
                    }
                    continue;
                }
                if let Some(head_slot) = visuals
                    .head_inactive
                    .as_ref()
                    .or(visuals.head_active.as_ref())
                {
                    compose_single_slot(
                        actors,
                        model_cache,
                        head_slot,
                        center,
                        note_scale,
                        phase,
                        translation,
                        elapsed,
                        current_beat,
                        note_rotation_y,
                        flat_tap_face_rotation_y,
                        note_rotation_z,
                        [1.0, 1.0, 1.0, note_alpha],
                        glow_alpha,
                        style.actors.note_z,
                        world_z,
                        prefer_sprite_note_path,
                        sprite_source,
                    );
                    continue;
                }
            }

            let note_idx = local_col * NUM_QUANTIZATIONS + note.quantization_idx as usize;
            let translation = ns.part_uv_translation(tap_part, note.beat, false);
            let lift_layers = (note.note_type == NoteType::Lift)
                .then(|| ns.lift_note_layers.get(note_idx))
                .flatten();
            if let Some(note_slots) = lift_layers.or_else(|| ns.note_layers.get(note_idx)) {
                let center = [column_center_x, y_pos];
                let phase = ns.part_uv_phase(tap_part, elapsed, current_beat, note.beat);
                for note_slot in note_slots.iter() {
                    compose_noteskin_layer(
                        actors,
                        model_cache,
                        note_slot,
                        center,
                        note_scale,
                        phase,
                        translation,
                        elapsed,
                        current_beat,
                        note_rotation_y,
                        flat_tap_face_rotation_y,
                        note_rotation_z,
                        [1.0, 1.0, 1.0, note_alpha],
                        glow_alpha,
                        style.actors.note_z,
                        world_z,
                        prefer_sprite_note_path,
                        sprite_source,
                    );
                }
            } else if let Some(note_slot) = ns.notes.get(note_idx) {
                let phase = ns.part_uv_phase(tap_part, elapsed, current_beat, note.beat);
                let frame_index = note_slot.frame_index_from_phase(phase);
                let uv_elapsed = if note_slot.model().is_some() {
                    phase
                } else {
                    elapsed
                };
                let uv = translated_uv_rect(
                    note_slot.uv_for_frame_at(frame_index, uv_elapsed),
                    translation,
                );
                let size = scale_sprite_to_arrow(note_slot.size(), target_arrow_px);
                let center = [column_center_x, y_pos];
                let draw = song_lua_note_model_draw(
                    model_cache.draw_at(note_slot, elapsed, current_beat),
                    note_rotation_y,
                );
                let rotation = -note_slot.sprite_def().rotation_deg as f32;
                compose_note_layer(
                    actors,
                    model_cache,
                    NoteLayerRequest {
                        slot: note_slot,
                        draw,
                        model_center: center,
                        sprite_center: center,
                        size,
                        uv,
                        rotation_y_deg: flat_tap_face_rotation_y,
                        model_rotation_z_deg: rotation + note_rotation_z,
                        sprite_rotation_z_deg: rotation + note_rotation_z,
                        tint: [1.0, 1.0, 1.0, note_alpha],
                        glow_alpha,
                        blend: BlendMode::Alpha,
                        z: style.actors.note_z,
                        world_z,
                        prefer_sprite: prefer_sprite_note_path,
                    },
                    sprite_source,
                );
            }
        }
        placement_start = placement_end;
    }
}

#[allow(clippy::too_many_arguments)]
fn compose_noteskin_layer<S, F>(
    actors: &mut Vec<Actor>,
    model_cache: &mut ModelMeshCache,
    slot: &S,
    center: [f32; 2],
    scale: f32,
    phase: f32,
    translation: [f32; 2],
    elapsed: f32,
    current_beat: f32,
    rotation_y_deg: f32,
    face_rotation_y_deg: f32,
    rotation_z_deg: f32,
    tint_scale: [f32; 4],
    glow_alpha: f32,
    z: i16,
    world_z: f32,
    prefer_sprite: bool,
    sprite_source: &F,
) where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
{
    let draw = song_lua_note_model_draw(
        model_cache.draw_at(slot, elapsed, current_beat),
        rotation_y_deg,
    );
    if !draw.visible {
        return;
    }
    let frame_index = slot.frame_index_from_phase(phase);
    let uv_elapsed = if slot.model().is_some() {
        phase
    } else {
        elapsed
    };
    let uv = translated_uv_rect(slot.uv_for_frame_at(frame_index, uv_elapsed), translation);
    let base_size = note_slot_base_size(slot, scale);
    let local_offset = [draw.pos[0] * scale, draw.pos[1] * scale];
    let rotation_sin_cos = slot.base_rot_sin_cos();
    let size = [
        base_size[0] * draw.zoom[0].max(0.0),
        base_size[1] * draw.zoom[1].max(0.0),
    ];
    if size[0] <= f32::EPSILON || size[1] <= f32::EPSILON {
        return;
    }
    let blend = if draw.blend_add {
        BlendMode::Add
    } else {
        BlendMode::Alpha
    };
    let tint = [
        draw.tint[0] * tint_scale[0],
        draw.tint[1] * tint_scale[1],
        draw.tint[2] * tint_scale[2],
        draw.tint[3] * tint_scale[3],
    ];
    let rotation = -slot.sprite_def().rotation_deg as f32;
    compose_note_layer(
        actors,
        model_cache,
        NoteLayerRequest {
            slot,
            draw,
            model_center: model_center(slot, center, local_offset, rotation_sin_cos),
            sprite_center: offset_center(center, local_offset, rotation_sin_cos),
            size,
            uv,
            rotation_y_deg: face_rotation_y_deg,
            model_rotation_z_deg: rotation + rotation_z_deg,
            sprite_rotation_z_deg: draw.rot[2] + rotation + rotation_z_deg,
            tint,
            glow_alpha,
            blend,
            z,
            world_z,
            prefer_sprite,
        },
        sprite_source,
    );
}

#[allow(clippy::too_many_arguments)]
fn compose_single_slot<S, F>(
    actors: &mut Vec<Actor>,
    model_cache: &mut ModelMeshCache,
    slot: &S,
    center: [f32; 2],
    scale: f32,
    phase: f32,
    translation: [f32; 2],
    elapsed: f32,
    current_beat: f32,
    rotation_y_deg: f32,
    face_rotation_y_deg: f32,
    rotation_z_deg: f32,
    tint: [f32; 4],
    glow_alpha: f32,
    z: i16,
    world_z: f32,
    prefer_sprite: bool,
    sprite_source: &F,
) where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
{
    let frame_index = slot.frame_index_from_phase(phase);
    let uv_elapsed = if slot.model().is_some() {
        phase
    } else {
        elapsed
    };
    let uv = translated_uv_rect(slot.uv_for_frame_at(frame_index, uv_elapsed), translation);
    let size = note_slot_base_size(slot, scale);
    let draw = song_lua_note_model_draw(
        model_cache.draw_at(slot, elapsed, current_beat),
        rotation_y_deg,
    );
    let rotation = -slot.sprite_def().rotation_deg as f32;
    compose_note_layer(
        actors,
        model_cache,
        NoteLayerRequest {
            slot,
            draw,
            model_center: center,
            sprite_center: center,
            size,
            uv,
            rotation_y_deg: face_rotation_y_deg,
            model_rotation_z_deg: rotation + rotation_z_deg,
            sprite_rotation_z_deg: rotation + rotation_z_deg,
            tint,
            glow_alpha,
            blend: BlendMode::Alpha,
            z,
            world_z,
            prefer_sprite,
        },
        sprite_source,
    );
}

#[inline(always)]
fn note_slot_base_size<S: NoteskinSlot>(slot: &S, scale: f32) -> [f32; 2] {
    if let Some(model) = slot.model() {
        let size = model.size();
        if size[0] > f32::EPSILON && size[1] > f32::EPSILON {
            return [size[0] * scale, size[1] * scale];
        }
    }
    let logical = slot.logical_size();
    [logical[0] * scale, logical[1] * scale]
}

#[inline(always)]
fn model_center<S: NoteskinSlot>(
    slot: &S,
    center: [f32; 2],
    local_offset: [f32; 2],
    [sin_r, cos_r]: [f32; 2],
) -> [f32; 2] {
    if slot.model().is_none() {
        return center;
    }
    let offset = [
        local_offset[0] * cos_r - local_offset[1] * sin_r,
        local_offset[0] * sin_r + local_offset[1] * cos_r,
    ];
    [center[0] + offset[0], center[1] + offset[1]]
}

#[inline(always)]
fn note_glow(y: f32, elapsed: f32, mini: f32, appearance: AppearanceEffects) -> f32 {
    appearance_note_glow(y, elapsed, mini, note_alpha_params(appearance))
}

#[inline(always)]
fn note_actor_alpha(y: f32, elapsed: f32, mini: f32, appearance: AppearanceEffects) -> f32 {
    appearance_note_actor_alpha(y, elapsed, mini, note_alpha_params(appearance))
}

#[inline(always)]
fn note_alpha_params(appearance: AppearanceEffects) -> NoteAlphaParams {
    NoteAlphaParams {
        hidden: appearance.hidden,
        hidden_offset: appearance.hidden_offset,
        sudden: appearance.sudden,
        sudden_offset: appearance.sudden_offset,
        stealth: appearance.stealth,
        blink: appearance.blink,
        random_vanish: appearance.random_vanish,
    }
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn note_x_offset(
    screen_height: f32,
    local_col: usize,
    y: f32,
    arrow_effect_time_s: f32,
    beat_factor: f32,
    visual: VisualEffects,
    col_offsets: &[f32],
    invert_distances: &[f32],
    tornado_bounds: &[TornadoBounds],
) -> f32 {
    canonical_note_x_offset(
        local_col,
        y,
        beat_factor,
        arrow_effect_time_s,
        col_offsets,
        invert_distances,
        tornado_bounds,
        &visual.move_x_cols,
        NoteXParams {
            screen_height,
            tornado: visual.tornado,
            drunk: visual.drunk,
            flip: visual.flip,
            invert: visual.invert,
            beat: visual.beat,
        },
        visual.tiny,
    )
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn receptor_row_center(
    screen_height: f32,
    playfield_center_x: f32,
    local_col: usize,
    receptor_y: f32,
    arrow_effect_time_s: f32,
    beat_factor: f32,
    visual: VisualEffects,
    col_offsets: &[f32],
    invert_distances: &[f32],
    tornado_bounds: &[TornadoBounds],
) -> [f32; 2] {
    canonical_receptor_row_center(
        playfield_center_x,
        local_col,
        receptor_y,
        beat_factor,
        arrow_effect_time_s,
        col_offsets,
        invert_distances,
        tornado_bounds,
        &visual.move_x_cols,
        &visual.move_y_cols,
        NoteXParams {
            screen_height,
            tornado: visual.tornado,
            drunk: visual.drunk,
            flip: visual.flip,
            invert: visual.invert,
            beat: visual.beat,
        },
        visual.tiny,
        visual.tipsy,
    )
}

#[inline(always)]
fn hold_body_needs_z_buffer(visual: &VisualEffects) -> bool {
    visual_hold_body_needs_z_buffer(VisualEffectParams {
        bumpy: visual.bumpy,
        ..VisualEffectParams::default()
    })
}

#[inline(always)]
fn tiny_zoom_for_col(visual: &VisualEffects, local_col: usize) -> f32 {
    visual_tiny_zoom(gameplay_visual_effect_params(visual, local_col))
}

#[inline(always)]
fn pulse_zoom_for_y(y: f32, visual: &VisualEffects) -> f32 {
    // Preserve the current global Pulse behavior: per-column Pulse values are
    // intentionally sampled from lane zero while Tiny remains lane-specific.
    visual_pulse_zoom_for_y(y, gameplay_visual_effect_params(visual, 0))
}

#[inline(always)]
fn calc_note_rotation_z(
    visual: VisualEffects,
    note_beat: f32,
    song_beat: f32,
    is_hold_head: bool,
    local_col: usize,
) -> f32 {
    visual_note_rotation_z(
        note_beat,
        song_beat,
        is_hold_head,
        gameplay_visual_effect_params(&visual, local_col),
    )
}

fn wrap_field_camera<S>(
    actors: &mut Vec<Actor>,
    field_start: usize,
    request: &NotefieldComposeRequest<'_, S>,
    prepared: &PreparedNotefield<'_, S>,
) {
    if actors.len() <= field_start {
        return;
    }
    let field = prepared.field;
    let center_y = 0.5 * (field.receptor_y_normal + field.receptor_y_reverse);
    let perspective = request.visual.perspective;
    let Some(view_proj) = notefield_view_proj(
        request.geometry.screen_width,
        request.geometry.screen_height,
        field.playfield_center_x,
        center_y,
        perspective.tilt,
        perspective.skew,
        request.geometry.reverse_scroll,
    ) else {
        return;
    };
    actors.reserve(2);
    actors.insert(field_start, Actor::CameraPush { view_proj });
    actors.push(Actor::CameraPop);
}
