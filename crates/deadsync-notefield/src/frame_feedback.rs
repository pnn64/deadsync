use crate::explosions::{ExplosionComposeRequest, ExplosionRotation, compose_explosion_layers};
use crate::{
    ColumnFeedbackRequest, ModelMeshCache, NoteXParams, NotefieldComposeRequest, PreparedNotefield,
    ReceptorDrawRequest, ReceptorPress, compose_column_feedback, compose_receptor_draws,
    gameplay_visual_effect_params, receptor_row_center, visual_arrow_effect_zoom,
    visual_confusion_rotation_deg,
};
#[cfg(test)]
use deadlib_present::actors::FlatSprite;
use deadlib_present::actors::{FlatDraw, SpriteSource};
use deadsync_core::input::MAX_COLS;
use deadsync_core::note::NoteType;
use deadsync_gameplay::{
    ActiveColumnFlash, ActiveHold, ActiveMineExplosion, ActiveTapExplosion, ColumnCue,
    hold_explosion_active,
};
use deadsync_noteskin::{HoldEmitter, NoteskinSlot, actor::MAX_HOLD_FLASHES};
#[cfg(test)]
use deadsync_noteskin::{ReceptorIdleGlow, ReceptorReverseBehavior};
#[cfg(test)]
use std::sync::Arc;

// ITGmania draws each Pump pad center-first so overlapping inner and outer
// panels layer above it symmetrically.
const PUMP_SINGLE_DRAW_ORDER: [usize; 5] = [2, 1, 3, 0, 4];
const PUMP_DOUBLE_DRAW_ORDER: [usize; 10] = [2, 1, 3, 0, 4, 7, 6, 8, 5, 9];

#[inline(always)]
const fn column_draw_col(num_cols: usize, draw_index: usize) -> usize {
    match num_cols {
        5 => PUMP_SINGLE_DRAW_ORDER[draw_index],
        10 => PUMP_DOUBLE_DRAW_ORDER[draw_index],
        _ => draw_index,
    }
}

/// Dynamic receptor state for one local notefield lane.
#[derive(Clone, Copy, Debug, Default)]
pub struct NotefieldLaneFeedback<'a> {
    pub active_hold: Option<&'a ActiveHold>,
    pub receptor_bop_zoom: f32,
    pub receptor_press_visual: Option<(f32, f32)>,
    pub hold_emitter: HoldEmitterState,
}

/// Song-lifetime presentation state. Fixed storage, no allocations or asset
/// work; even after a long frame, update visits at most MAX_HOLD_FLASHES slots.
#[derive(Clone, Copy, Debug, Default)]
pub struct HoldEmitterState {
    pub is_roll: bool,
    showing: bool,
    next_emit_s: Option<f64>,
    next_child: usize,
    last_time_s: f32,
    flashes: [Option<f32>; MAX_HOLD_FLASHES],
}

impl HoldEmitterState {
    pub fn update<T>(&mut self, now: f32, showing: Option<bool>, emitter: Option<&HoldEmitter<T>>) {
        let Some(emitter) = emitter.filter(|e| {
            e.count > 0
                && e.count <= MAX_HOLD_FLASHES
                && e.interval_s.is_finite()
                && e.interval_s > 0.0
        }) else {
            *self = Self::default();
            return;
        };
        if !now.is_finite() {
            return;
        }
        if now < self.last_time_s {
            *self = Self::default();
        }
        let interval = f64::from(emitter.interval_s);
        if let Some(next) = self.next_emit_s.filter(|next| *next < f64::from(now)) {
            let due = if self.showing {
                ((f64::from(now) - next) / interval).ceil() as usize
            } else {
                1
            };
            // Skip overwritten emissions analytically instead of catching up
            // every queued command after a pause or stalled frame.
            for offset in due.saturating_sub(emitter.count)..due {
                let child = (self.next_child + offset % emitter.count) % emitter.count;
                // ActorFrame updates its commands before its children. A
                // newly flashed child receives the entire frame delta, even
                // when Emit became due partway through that frame.
                self.flashes[child] = Some(self.last_time_s);
            }
            self.next_child = (self.next_child + due % emitter.count) % emitter.count;
            self.next_emit_s = self.showing.then_some(next + due as f64 * interval);
        }
        if let Some(is_roll) = showing {
            if !self.showing || is_roll != self.is_roll {
                // finishtweening on the parent also finishes every child.
                self.flashes.fill(None);
                self.flashes[self.next_child % emitter.count] = Some(now);
                self.next_child = (self.next_child + 1) % emitter.count;
                self.next_emit_s = Some(f64::from(now) + interval);
            }
            self.is_roll = is_roll;
        }
        self.showing = showing.is_some();
        self.last_time_s = now;
        let duration = emitter.flash.animation.duration();
        for flash in &mut self.flashes {
            if flash.is_some_and(|start| now - start > duration) {
                *flash = None;
            }
        }
    }
}

/// Borrowed per-frame feedback emitted around the receptor row.
///
/// The concrete theme prepares this view from gameplay state; canonical
/// notefield code owns actor selection, placement, animation, and ordering.
#[derive(Clone, Copy, Debug)]
pub struct NotefieldFeedbackFrameView<'a> {
    /// Cue columns use chart-global column indices.
    pub column_cues: Option<&'a [ColumnCue]>,
    /// First regular column cue after the current music time.
    pub column_cue_cursor: Option<usize>,
    /// Crossover cue columns use chart-global column indices.
    pub crossover_cues: Option<&'a [ColumnCue]>,
    /// Per-cue fade-in anchors; only the prefix before the cursor is valid.
    pub crossover_cue_entries: Option<&'a [f32]>,
    /// First crossover cue after the current music time.
    pub crossover_cue_cursor: Option<usize>,
    /// Column flashes are ordered by local lane within the prepared player span.
    pub column_flashes: Option<&'a [Option<ActiveColumnFlash>]>,
    /// Tap explosions are ordered by local lane, or absent when no asset can render them.
    pub tap_explosions: Option<&'a [Option<ActiveTapExplosion>]>,
    /// Mine explosions are ordered by local lane, or absent when no asset can render them.
    pub mine_explosions: Option<&'a [Option<ActiveMineExplosion>]>,
    /// Lane feedback is ordered by local lane within the prepared player span.
    pub lanes: [NotefieldLaneFeedback<'a>; MAX_COLS],
    pub countdown_font: &'static str,
    pub countdown_text_slot: u8,
}

/// Compose cues/flashes, receptor targets and feedback, then tap and mine
/// explosions in the canonical field ordering.
#[allow(clippy::too_many_arguments)]
pub(crate) fn compose_notefield_feedback<S, F>(
    draws: &mut Vec<FlatDraw>,
    hud_draws: &mut Vec<FlatDraw>,
    model_cache: &mut ModelMeshCache,
    request: &NotefieldComposeRequest<'_, S>,
    prepared: &PreparedNotefield<'_, S>,
    frame: &NotefieldFeedbackFrameView<'_>,
    lane_move_y_offsets: &[f32],
    lane_tipsy_offsets: &[f32],
    sprite_source: &F,
) where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
{
    let Some(notes) = prepared.notes.as_ref() else {
        return;
    };
    let options = &request.options;
    let frame_plan = prepared.frame_plan;
    let col_start = frame_plan.col_start;
    let num_cols = frame_plan.num_cols;
    let field = prepared.field;
    let visual = request.visual.visual;
    let elapsed_screen = request.visual.elapsed_screen_s;
    let current_beat = prepared.current_beat;
    let field_zoom = prepared.field_zoom;
    let spacing_multiplier = request.visual.spacing_multiplier;
    let measure_column_xs = notes.measure_column_xs;

    compose_column_feedback(
        draws,
        hud_draws,
        ColumnFeedbackRequest {
            hud_style: request.hud_style,
            column_cues: frame.column_cues,
            column_cue_cursor: frame.column_cue_cursor,
            crossover_cues: frame.crossover_cues,
            crossover_cue_entries: frame.crossover_cue_entries,
            crossover_cue_cursor: frame.crossover_cue_cursor,
            column_flashes: frame.column_flashes,
            // The regular cue countdown is independent of the crossover-only
            // profile toggle.
            regular_countdown: true,
            crossover_countdown: options.frame_features.crossover_countdown,
            current_music_time: prepared.current_time_s,
            current_screen_time: elapsed_screen,
            music_rate: request.chart.music_rate,
            col_start,
            num_cols,
            column_xs: &measure_column_xs,
            column_dirs: &field.column_dirs,
            spacing_multiplier,
            field_zoom,
            playfield_center_x: field.playfield_center_x,
            field_center_y: field.notefield_offset_y,
            screen_height: request.geometry.screen_height,
            compact_flashes: options.column_flash_compact,
            dim_flashes: options.column_flash_dimmed,
            countdown_font: frame.countdown_font,
            countdown_text_slot: frame.countdown_text_slot,
        },
    );

    let receptor = notes.receptor;
    let tap_explosion = notes.tap_explosion;
    let col_offsets = notes.col_offsets;
    let invert_distances = notes.invert_distances;
    let tornado_bounds = notes.tornado_bounds;
    let beat_factor = notes.beat_factor;
    let mut lane_base_zooms = [0.0; MAX_COLS];
    let mut lane_rotations = [0.0; MAX_COLS];
    let mut lane_centers = [[0.0; 2]; MAX_COLS];
    let mut lane_zooms = [0.0; MAX_COLS];
    let mut lane_depths = [0.0; MAX_COLS];
    let targets_enabled = !options.hide_targets
        && prepared.receptor_alphas[..num_cols]
            .iter()
            .any(|alpha| *alpha > f32::EPSILON);
    let lane_work_mask = feedback_lane_work_mask(
        num_cols,
        targets_enabled,
        options.hold_explosion_enabled,
        tap_explosion.is_some(),
        notes.mine.mine_hit_explosion.is_some(),
        &frame.lanes,
        frame.tap_explosions.unwrap_or_default(),
        frame.mine_explosions.unwrap_or_default(),
    );
    if lane_work_mask == 0 {
        return;
    }
    let (pulse_color, idle_glow_alpha) = if targets_enabled {
        (
            receptor.receptor_pulse.color_for_beat(current_beat),
            receptor
                .receptor_idle_glow
                .alpha(current_beat, prepared.is_in_delay),
        )
    } else {
        ([1.0; 4], 0.0)
    };

    for draw_index in 0..num_cols {
        let lane_draw_start = draws.len();
        let local_col = column_draw_col(num_cols, draw_index);
        let receptor_y = field.column_receptor_ys[local_col];
        if lane_work_mask & (1 << local_col) == 0 {
            continue;
        }
        let lane = frame.lanes[local_col];
        let effect = gameplay_visual_effect_params(&visual, local_col);
        lane_depths[local_col] = crate::note_world_z_cached(
            0.0,
            notes.note_depth_frame_cache,
            crate::lane_note_transform_cache(current_beat, effect),
        );
        let base_zoom = visual_arrow_effect_zoom(0.0, effect);
        lane_base_zooms[local_col] = base_zoom;
        let effect_zoom = (base_zoom
            + request
                .song_lua
                .note_hides
                .zoom_offset(local_col, current_beat))
            * prepared.column_zooms[local_col];
        lane_zooms[local_col] = effect_zoom;
        let hidden = effect_zoom.abs() <= f32::EPSILON;
        let confusion_rotation_deg = visual_confusion_rotation_deg(current_beat, effect)
            + prepared.column_rotations_deg[local_col];
        lane_rotations[local_col] = confusion_rotation_deg;
        let mut center = receptor_row_center(
            field.playfield_center_x,
            local_col,
            receptor_y,
            beat_factor,
            request.arrow_effect_time_s,
            &col_offsets[..num_cols],
            &invert_distances[..num_cols],
            &tornado_bounds[..num_cols],
            &notes.tornado_lane_caches[..num_cols],
            &notes.move_x_offsets[..num_cols],
            lane_move_y_offsets[local_col],
            NoteXParams {
                screen_height: request.geometry.screen_height,
                tornado: visual.tornado,
                drunk: visual.drunk,
                drunk_offset: visual.drunk_offset,
                drunk_speed: visual.drunk_speed,
                drunk_period: visual.drunk_period,
                cosecant: visual.cosecant,
                tan_drunk: visual.tan_drunk,
                tan_drunk_offset: visual.tan_drunk_offset,
                tan_drunk_speed: visual.tan_drunk_speed,
                tan_drunk_period: visual.tan_drunk_period,
                flip: visual.flip,
                invert: visual.invert,
                beat: visual.beat,
                parabola_x: visual.parabola_x,
                square: visual.square,
                square_offset: visual.square_offset,
                square_period: visual.square_period,
                xmode: visual.xmode,
                player_p2: matches!(request.placement, crate::FieldPlacement::P2),
                double_style: request.geometry.double_style,
            },
            notes.tiny_spacing_scale,
            lane_tipsy_offsets[local_col],
        );
        center[0] += prepared.column_x_offsets[local_col];
        lane_centers[local_col] = center;
        let hold_slot = if hidden || !options.hold_explosion_enabled {
            None
        } else {
            lane.active_hold.and_then(|active| {
                let note = request.chart.notes.get(active.note_index)?;
                if !hold_explosion_active(Some(active), current_beat, note.beat) {
                    return None;
                }
                tap_explosion.and_then(|noteskin| {
                    noteskin
                        .hold_explosion_for_col(local_col, matches!(note.note_type, NoteType::Roll))
                })
            })
        };
        let receptor_alpha = prepared.receptor_alphas[local_col];
        let targets_visible = !hidden && targets_enabled && receptor_alpha > f32::EPSILON;
        let target_slot = targets_visible.then(|| &receptor.receptor_off[local_col]);
        let target_reverse = targets_visible
            .then(|| receptor.receptor_off_reverse.get(local_col).copied())
            .flatten();
        let distinct_idle_glow = receptor
            .receptor_idle_glow_layers
            .get(local_col)
            .and_then(Option::as_ref);
        let idle_glow_slot = (targets_visible && receptor.receptor_idle_glow.is_visible())
            .then(|| {
                distinct_idle_glow.or_else(|| {
                    receptor
                        .receptor_glow
                        .get(local_col)
                        .and_then(Option::as_ref)
                })
            })
            .flatten();
        let idle_glow_reverse = idle_glow_slot.and_then(|_| {
            if distinct_idle_glow.is_some() {
                receptor.receptor_idle_glow_reverse.get(local_col).copied()
            } else {
                receptor.receptor_glow_reverse.get(local_col).copied()
            }
        });
        let idle_glow_shares_press = idle_glow_slot.is_some() && distinct_idle_glow.is_none();
        let resolve_press = || {
            let visual = lane.receptor_press_visual?;
            let slot = receptor
                .receptor_glow
                .get(local_col)
                .and_then(|slot| slot.as_ref())?;
            Some(ReceptorPress {
                slot,
                reverse: receptor.receptor_glow_reverse.get(local_col).copied(),
                visual,
            })
        };
        if target_slot.is_some() || hold_slot.is_some() {
            compose_receptor_draws(
                draws,
                model_cache,
                ReceptorDrawRequest {
                    target_slot,
                    target_reverse,
                    idle_glow_slot,
                    overlays: receptor
                        .receptor_overlays
                        .get(local_col)
                        .map(AsRef::as_ref)
                        .unwrap_or_default(),
                    idle_glow_reverse,
                    idle_glow_shares_press,
                    hold_slot,
                    center,
                    hidden,
                    hide_targets: options.hide_targets,
                    reverse: field.column_reverse_percent[local_col] > 0.5,
                    bop_zoom: lane.receptor_bop_zoom,
                    effect_zoom,
                    confusion_rotation_deg,
                    elapsed: elapsed_screen,
                    beat: current_beat,
                    idle_glow_alpha,
                    press_visual: lane.receptor_press_visual,
                    receptor_alpha,
                    field_zoom,
                    rotation_y_deg: 0.0,
                    pulse_color,
                    idle_glow: receptor.receptor_idle_glow,
                    press_behavior: receptor.receptor_glow_behavior,
                },
                resolve_press,
                sprite_source,
            );
        }
        if !hidden
            && options.hold_explosion_enabled
            && let Some(emitter) = tap_explosion.and_then(|skin| {
                skin.hold_visuals_for_col(local_col, lane.hold_emitter.is_roll)
                    .emitter
                    .as_ref()
            })
        {
            for started in lane.hold_emitter.flashes[..emitter.count].iter().flatten() {
                let age = (elapsed_screen - started).max(0.0);
                compose_explosion_layers(
                    draws,
                    ExplosionComposeRequest {
                        layers: std::slice::from_ref(&emitter.flash),
                        hit_seed: (u64::from(started.to_bits()) << 32) | local_col as u64,
                        elapsed_s: age,
                        effect_elapsed_s: age,
                        current_frame_beat: current_beat,
                        relative_frame_beat: None,
                        uv_elapsed_s: elapsed_screen,
                        center,
                        field_zoom,
                        effect_zoom,
                        rotation: ExplosionRotation::Tap {
                            rotation_y_deg: 0.0,
                            extra_z_deg: confusion_rotation_deg,
                        },
                        z: crate::style::HOLD_EXPLOSION_Z,
                    },
                    sprite_source,
                );
            }
        }
        // Native receptor/ghost transforms sample depth at zero travel.
        shift_feedback_depth(&mut draws[lane_draw_start..], lane_depths[local_col]);
    }

    // Tap explosions are independent of the concrete "Hide Combo
    // Explosions" option, which applies only to combo milestone art.
    if let (Some(tap_explosion), Some(tap_explosions)) = (tap_explosion, frame.tap_explosions) {
        for draw_index in 0..num_cols {
            let lane_draw_start = draws.len();
            let local_col = column_draw_col(num_cols, draw_index);
            let Some(active) = tap_explosions.get(local_col) else {
                continue;
            };
            let Some(active) = active.as_ref() else {
                continue;
            };
            if lane_zooms[local_col].abs() <= f32::EPSILON {
                continue;
            }
            let Some(explosion) = tap_explosion.tap_explosion_for_col_with_bright(
                local_col,
                active.window,
                active.bright,
            ) else {
                continue;
            };
            let center = lane_centers[local_col];
            compose_explosion_layers(
                draws,
                ExplosionComposeRequest {
                    layers: explosion.layers.as_ref(),
                    elapsed_s: active.elapsed,
                    hit_seed: (u64::from(active.start_beat.to_bits()) << 32) | local_col as u64,
                    effect_elapsed_s: (elapsed_screen - active.effect_started_at_screen_s).max(0.0),
                    current_frame_beat: request.visual.current_display_beat,
                    relative_frame_beat: Some(
                        (request.visual.current_display_beat - active.start_beat).max(0.0),
                    ),
                    uv_elapsed_s: elapsed_screen,
                    center,
                    field_zoom,
                    effect_zoom: lane_zooms[local_col],
                    rotation: ExplosionRotation::Tap {
                        rotation_y_deg: 0.0,
                        extra_z_deg: lane_rotations[local_col],
                    },
                    z: crate::style::TAP_EXPLOSION_Z,
                },
                sprite_source,
            );
            // Native receptor/ghost transforms sample depth at zero travel.
            shift_feedback_depth(&mut draws[lane_draw_start..], lane_depths[local_col]);
        }
    }

    if let (Some(explosion), Some(mine_explosions)) = (
        notes.mine.mine_hit_explosion.as_ref(),
        frame.mine_explosions,
    ) {
        for draw_index in 0..num_cols {
            let lane_draw_start = draws.len();
            let local_col = column_draw_col(num_cols, draw_index);
            let Some(active) = mine_explosions.get(local_col) else {
                continue;
            };
            let Some(active) = active.as_ref() else {
                continue;
            };
            compose_explosion_layers(
                draws,
                ExplosionComposeRequest {
                    layers: explosion.layers.as_ref(),
                    elapsed_s: active.elapsed,
                    effect_elapsed_s: active.elapsed,
                    hit_seed: (u64::from(active.started_at_screen_s.to_bits()) << 32)
                        | local_col as u64,
                    current_frame_beat: current_beat,
                    relative_frame_beat: None,
                    uv_elapsed_s: elapsed_screen,
                    center: lane_centers[local_col],
                    field_zoom,
                    effect_zoom: lane_base_zooms[local_col],
                    rotation: ExplosionRotation::Mine,
                    z: crate::style::MINE_EXPLOSION_Z,
                },
                sprite_source,
            );
            // Native receptor/ghost transforms sample depth at zero travel.
            shift_feedback_depth(&mut draws[lane_draw_start..], lane_depths[local_col]);
        }
    }
}

fn shift_feedback_depth(draws: &mut [FlatDraw], z: f32) {
    if z == 0.0 {
        return;
    }
    for draw in draws {
        match draw {
            FlatDraw::Sprite(sprite) => sprite.world_z += z,
            FlatDraw::TexturedMesh(mesh) => mesh.world_z += z,
            FlatDraw::PreparedU32(_) | FlatDraw::PreparedInline(_) => {}
        }
    }
}

#[inline(always)]
fn feedback_lane_work_mask(
    num_cols: usize,
    targets_enabled: bool,
    hold_explosions_enabled: bool,
    tap_explosions_available: bool,
    mine_explosions_available: bool,
    lanes: &[NotefieldLaneFeedback<'_>; MAX_COLS],
    tap_explosions: &[Option<ActiveTapExplosion>],
    mine_explosions: &[Option<ActiveMineExplosion>],
) -> u16 {
    let num_cols = num_cols.min(MAX_COLS);
    if targets_enabled {
        return (1_u16 << num_cols) - 1;
    }
    let mut mask = 0;
    for local_col in 0..num_cols {
        let hold_active = hold_explosions_enabled
            && (lanes[local_col].active_hold.is_some()
                || lanes[local_col]
                    .hold_emitter
                    .flashes
                    .iter()
                    .any(Option::is_some));
        let tap_active =
            tap_explosions_available && tap_explosions.get(local_col).is_some_and(Option::is_some);
        let mine_active = mine_explosions_available
            && mine_explosions.get(local_col).is_some_and(Option::is_some);
        if hold_active || tap_active || mine_active {
            mask |= 1 << local_col;
        }
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ErrorBarModes, FieldPlacement, LayoutMiniIndicatorPosition, MeasureLineMode,
        NotefieldChartView, NotefieldFrameFeatures, NotefieldGeometry, NotefieldNoteskinView,
        NotefieldOptions, NotefieldSongLuaView, NotefieldVisualState, ProxyCaptureRequests,
        ViewOverride, ZmodLayoutParams, prepare_notefield,
    };
    use deadlib_present::actors::SpriteSource;
    use deadsync_gameplay::{
        AccelEffects, AppearanceEffects, PerspectiveEffects, ScrollEffects,
        SongLuaNoteHideWindowRuntime, SongLuaNoteHideWindows, VisibilityEffects, VisualEffects,
    };
    use deadsync_noteskin::{
        ExplosionAnimation, HoldVisuals, ModelDrawState, ModelMesh, NoteDisplayMetrics,
        NoteskinRuntime, ReceptorGlowBehavior, ReceptorPulse, SpriteDefinition, TapExplosion,
        TapExplosionMap,
    };
    use deadsync_rules::judgment::JudgeGrade;
    use deadsync_rules::note::Note;
    use deadsync_rules::scroll::ScrollSpeedSetting;
    use deadsync_rules::timing::TimingData;
    use deadsync_theme::{
        ColumnCueStyle, ColumnFlashLayoutStyle, ColumnFlashStyle, ComboFeedbackStyle,
        CounterHudStyle, ErrorBarLayers, ErrorBarPalette, ErrorBarStyle, JudgmentFeedbackStyle,
        MiniIndicatorStyle, NotefieldHudStyle,
    };

    // Standalone feedback tests prepare the same components as the field composer.
    fn compose_notefield_feedback(
        draws: &mut Vec<FlatDraw>,
        hud_draws: &mut Vec<FlatDraw>,
        model_cache: &mut ModelMeshCache,
        request: &NotefieldComposeRequest<'_, TestSlot>,
        prepared: &PreparedNotefield<'_, TestSlot>,
        frame: &NotefieldFeedbackFrameView<'_>,
        sprite_source: &impl Fn(&TestSlot) -> SpriteSource,
    ) {
        let mut move_y = [0.0; MAX_COLS];
        let mut tipsy = [0.0; MAX_COLS];
        crate::fill_gameplay_lane_effects(
            &request.visual.visual,
            request.arrow_effect_time_s,
            prepared.frame_plan.num_cols,
            &mut [crate::VisualEffectParams::default(); MAX_COLS],
            &mut [0.0; MAX_COLS],
            &mut tipsy,
            &mut move_y,
        );
        super::compose_notefield_feedback(
            draws,
            hud_draws,
            model_cache,
            request,
            prepared,
            frame,
            &move_y,
            &tipsy,
            sprite_source,
        );
    }

    #[derive(Clone, Debug)]
    struct TestSlot {
        def: SpriteDefinition,
        key: Arc<str>,
    }

    impl TestSlot {
        fn new(key: impl Into<Arc<str>>) -> Self {
            Self {
                def: SpriteDefinition {
                    size: [64, 64],
                    ..SpriteDefinition::default()
                },
                key: key.into(),
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
            Arc::clone(&self.key)
        }

        fn model(&self) -> Option<&ModelMesh> {
            None
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

        fn uv_for_frame_at(&self, _frame_index: usize, _elapsed: f32) -> [f32; 4] {
            [0.0, 0.0, 1.0, 1.0]
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
            ([uv[2] - uv[0], uv[3] - uv[1]], [uv[0], uv[1]], [0.0; 2])
        }
    }

    #[test]
    fn column_draw_order_matches_itgmania_pump_styles() {
        let draw_order = |num_cols| {
            (0..num_cols)
                .map(|draw_index| column_draw_col(num_cols, draw_index))
                .collect::<Vec<_>>()
        };

        assert_eq!(draw_order(4), [0, 1, 2, 3]);
        assert_eq!(draw_order(5), [2, 1, 3, 0, 4]);
        assert_eq!(draw_order(8), [0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(draw_order(10), [2, 1, 3, 0, 4, 7, 6, 8, 5, 9]);
    }

    fn explosion(key: &str) -> TapExplosion<TestSlot> {
        TapExplosion::from_single(TestSlot::new(key), ExplosionAnimation::default())
    }

    fn noteskin() -> NoteskinRuntime<TestSlot> {
        let tap_explosions_by_col = (0..2)
            .map(|lane| TapExplosionMap::from([("W1", explosion(&format!("tap{lane}")))]))
            .collect();
        NoteskinRuntime {
            notes: Vec::new(),
            note_layers: Vec::new(),
            lift_note_layers: Vec::new(),
            receptor_off: vec![TestSlot::new("target0"), TestSlot::new("target1")],
            receptor_glow: vec![Some(TestSlot::new("press0")), Some(TestSlot::new("press1"))],
            receptor_idle_glow_layers: vec![None; 2],
            receptor_overlays: Vec::new(),
            receptor_off_reverse: vec![ReceptorReverseBehavior::default(); 2],
            receptor_glow_reverse: vec![ReceptorReverseBehavior::default(); 2],
            receptor_idle_glow_reverse: vec![ReceptorReverseBehavior::default(); 2],
            receptor_step_behaviors: Vec::new(),
            mines: Vec::new(),
            mine_fill_slots: Vec::new(),
            mine_layers: Vec::new(),
            column_xs: vec![-32, 32],
            tap_explosions: TapExplosionMap::new(),
            tap_explosions_by_col,
            mine_hit_explosion: Some(explosion("mine")),
            receptor_glow_behavior: ReceptorGlowBehavior::default(),
            receptor_idle_glow: ReceptorIdleGlow::default(),
            receptor_pulse: ReceptorPulse::default(),
            hold_let_go_gray_percent: 0.25,
            hold_columns: vec![
                HoldVisuals {
                    explosion: Some(TestSlot::new("hold0")),
                    ..HoldVisuals::default()
                },
                HoldVisuals {
                    explosion: Some(TestSlot::new("hold1")),
                    ..HoldVisuals::default()
                },
            ],
            roll_columns: Vec::new(),
            hold: HoldVisuals::default(),
            roll: HoldVisuals::default(),
            custom_parts: Default::default(),
            part_animation_is_beat_based: [false; deadsync_noteskin::NOTE_ANIM_PART_COUNT],
            note_display_metrics: NoteDisplayMetrics::default(),
        }
    }

    fn style() -> NotefieldHudStyle {
        NotefieldHudStyle {
            judgment_normal_y: -30.0,
            judgment_reverse_y: 30.0,
            combo_normal_y: 30.0,
            combo_reverse_y: -30.0,
            combo_centered_y: 155.0,
            judgment_height: 40.0,
            error_bar_offset_y: 25.0,
            measure_cue_scroll_color: [0.8, 0.7, 0.5],
            measure_cue_bpm_color: [1.0, 1.0, 0.0],
            measure_cue_delay_color: [1.0, 0.4, 0.7],
            measure_cue_stop_color: [1.0, 0.0, 0.0],
            measure_cue_alpha: 0.7,
            edit_measure_number_font: "test",
            column_cue: ColumnCueStyle {
                top_y: 80.0,
                reverse_anchor_y: 304.0,
                crossover_height_trim: 270.0,
                body_fade: 0.333,
                base_alpha: 0.12,
                normal_color: [0.3, 1.0, 1.0],
                mine_color: [1.0, 0.0, 0.0],
                countdown_normal_y: 160.0,
                countdown_reverse_y: 340.0,
                countdown_color: [1.0; 3],
                countdown_zoom: 0.5,
                body_z: 90,
                countdown_z: 200,
            },
            column_flash: ColumnFlashStyle {
                default_layout: ColumnFlashLayoutStyle {
                    top_y: 80.0,
                    height_trim: 0.0,
                    reverse_trim: 0.0,
                    fade: 0.333,
                },
                compact_layout: ColumnFlashLayoutStyle {
                    top_y: 70.0,
                    height_trim: 270.0,
                    reverse_trim: 30.0,
                    fade: 0.2,
                },
                reverse_anchor_y: 304.0,
                normal_alpha: 0.66,
                dimmed_alpha: 0.3,
                miss_color: [1.0, 0.0, 0.0],
                decent_color: [0.7, 0.36, 1.0],
                way_off_color: [0.79, 0.52, 0.37],
                great_color: [0.4, 0.79, 0.33],
                excellent_color: [0.89, 0.61, 0.09],
                fantastic_color: [1.0; 3],
                fantastic_blue_color: [0.13, 0.8, 0.91],
                z: 91,
            },
            counter_hud: CounterHudStyle {
                text_z: 85,
                shadow_len: 1.0,
                base_zoom: 0.35,
                lookahead_zoom_step: 0.05,
                vertical_step_y: 20.0,
                left_column_scale: 4.0 / 3.0,
                horizontal_span: 2.0,
                break_lookahead_color: [0.4, 0.4, 0.4, 1.0],
                break_current_color: [0.5; 4],
                stream_lookahead_color: [0.45, 0.45, 0.45, 1.0],
                ratio_color: [1.0; 4],
                total_color: [0.5; 4],
                broken_y_offset: 15.0,
                broken_vertical_y_offset: -15.0,
                broken_vertical_x_scale: 4.0 / 3.0,
                broken_color: [1.0, 1.0, 1.0, 0.7],
                run_active_color: [1.0; 4],
                run_inactive_color: [0.5; 4],
            },
            mini_indicator: MiniIndicatorStyle {
                column_offset: 1.0,
                under_up_x_offset: -45.0,
                unanchored_x_offset: -12.0,
                failed_color: [0.5; 3],
                shadow_len: 1.0,
                text_z: 85,
            },
            judgment_feedback: JudgmentFeedbackStyle {
                tap_front_z: 200,
                tap_back_z: 95,
                split_overlay_alpha: 0.5,
                held_miss_normal_y: -50.0,
                held_miss_reverse_y: 110.0,
                held_miss_z: 196,
                hold_normal_y: -90.0,
                hold_reverse_y: 90.0,
                hold_z: 195,
                hold_initial_zoom: 25.6 / 140.0,
                hold_final_zoom: 32.0 / 140.0,
            },
            combo_feedback: ComboFeedbackStyle {
                threshold: 4,
                milestone_z: 89,
                number_z: 90,
                number_zoom: 0.75,
                shadow_len: 1.0,
                miss_color: [1.0, 0.0, 0.0, 1.0],
                burst_duration: 0.5,
                burst_start_zoom: 2.0,
                burst_end_zoom: 1.0,
                burst_start_alpha: 0.5,
                burst_rotation_deg: 90.0,
                hundred_start_zoom: 0.25,
                hundred_end_zoom: 2.0,
                hundred_start_alpha: 0.6,
                hundred_start_rotation_deg: 10.0,
                mini_duration: 0.4,
                mini_start_zoom: 0.25,
                mini_end_zoom: 1.8,
                mini_start_alpha: 1.0,
                mini_start_rotation_deg: 10.0,
                thousand_start_zoom: 0.25,
                thousand_end_zoom: 3.0,
                thousand_start_alpha: 0.7,
                thousand_x_travel: 100.0,
            },
            error_bar: ErrorBarStyle {
                colorful_width: 160.0,
                colorful_height: 10.0,
                colorful_border_size: 4.0,
                average_width: 325.0,
                average_height: 7.0,
                average_tick_padding: 4.0,
                monochrome_width: 240.0,
                monochrome_border_size: 2.0,
                monochrome_center_width: 2.0,
                monochrome_line_width: 1.0,
                tick_width: 2.0,
                colorful_tick_duration: 0.5,
                monochrome_tick_duration: 0.75,
                average_tick_extra_height: 75.0,
                monochrome_background_alpha: 0.5,
                line_alpha: 0.3,
                lines_fade_start: 2.5,
                lines_fade_duration: 0.5,
                label_fade_duration: 0.5,
                label_hold: 2.0,
                label_x_ratio: 0.25,
                label_zoom: 0.7,
                center_tick_width: 1.0,
                highlight_inactive_alpha: 0.3,
                offset_indicator_duration: 0.5,
                offset_indicator_gap: 6.0,
                offset_indicator_zoom: 0.25,
                offset_indicator_shadow_len: 1.0,
                long_average_tick_duration: 0.5,
                long_average_tick_extra_height: 65.0,
                long_average_tick_width: 1.0,
                text_duration: 0.5,
                text_x_offset: 40.0,
                text_zoom: 0.25,
                text_shadow_len: 1.0,
                background_color: [0.0, 0.0, 0.0, 1.0],
                monochrome_center_color: [0.5; 4],
                monochrome_line_color: [1.0; 4],
                label_color: [1.0; 4],
                colorful_tick_color: [0.7, 0.0, 0.0, 1.0],
                average_center_tick_color: [1.0, 1.0, 1.0, 0.3],
                long_average_tick_color: [0.0, 0.0, 1.0, 1.0],
                text_early_color: [0.0, 0.4, 1.0, 1.0],
                text_late_color: [1.0, 0.35, 0.3, 1.0],
                text_scaled_early_color: [0.0, 0.3, 0.86, 1.0],
                text_scaled_late_color: [1.0, 0.09, 0.02, 1.0],
                palette: ErrorBarPalette {
                    fantastic_blue: [0.13, 0.8, 0.91, 1.0],
                    fa_plus_white: [1.0; 4],
                    excellent: [0.89, 0.61, 0.09, 1.0],
                    great: [0.4, 0.79, 0.33, 1.0],
                    decent: [0.71, 0.36, 1.0, 1.0],
                    way_off: [0.79, 0.52, 0.37, 1.0],
                },
                label_font: "test",
                offset_indicator_font: "test",
                text_font: "test",
                early_label: "Early",
                late_label: "Late",
                front_layers: ErrorBarLayers {
                    background: 180,
                    band: 181,
                    line: 182,
                    tick: 183,
                    text: 184,
                },
                back_layers: ErrorBarLayers {
                    background: 86,
                    band: 87,
                    line: 88,
                    tick: 89,
                    text: 90,
                },
                average_z: 88,
            },
        }
    }

    fn options() -> NotefieldOptions {
        NotefieldOptions {
            mine_size_scale: 1.0,
            frame_features: NotefieldFrameFeatures {
                measure_line_mode: MeasureLineMode::Off,
                measure_cues: false,
                column_cues: true,
                crossover_cues: false,
                crossover_countdown: false,
                column_flash: true,
                error_bar: false,
                error_bar_text: false,
                combo_visible: false,
            },
            notefield_offset: [0.0; 2],
            judgment_offset: [0.0; 2],
            combo_offset: [0.0; 2],
            error_bar_offset: [0.0; 2],
            zmod_layout: ZmodLayoutParams {
                judgment_height: 40.0,
                has_error_bar: false,
                has_judgment_texture: false,
                error_bar_up: false,
                has_measure_counter: false,
                measure_counter_up: false,
                broken_run: false,
                mini_indicator_position: LayoutMiniIndicatorPosition::Default,
            },
            has_judgment_texture: false,
            error_bar_up: false,
            fallback_mini_percent: 0.0,
            column_flash_compact: false,
            column_flash_dimmed: false,
            hide_targets: false,
            hold_explosion_enabled: true,
            hide_combo_explosions: false,
            judgment_back: false,
            show_fa_plus_window: false,
            fa_plus_10ms_blue_window: false,
            split_15_10ms: false,
            custom_fantastic_window: false,
            judgment_tilt_enabled: false,
            judgment_tilt_min_ms: 0.0,
            judgment_tilt_max_ms: 0.0,
            judgment_tilt_multiplier: 0.0,
            blue_fantastic_window_s: 0.015,
            error_bar_modes: ErrorBarModes::default(),
            error_bar_max_window_ix: 0,
            monochrome_background: false,
            error_bar_multi_tick: false,
            short_average_error_bar: false,
            center_tick: false,
            error_ms_display: false,
            long_error_bar_enabled: false,
            long_error_bar_intensity: 0.0,
            measure_counter: None,
            mini_indicator_position: LayoutMiniIndicatorPosition::Default,
            mini_indicator_zoom: 1.0,
            counter_left: false,
        }
    }

    fn request<'a>(
        noteskin: &'a NoteskinRuntime<TestSlot>,
        timing: &'a TimingData,
        notes: &'a [Note],
        note_hides: &'a SongLuaNoteHideWindows,
        placement: FieldPlacement,
        player_idx: usize,
        num_players: usize,
        cols_per_player: usize,
        total_cols: usize,
    ) -> NotefieldComposeRequest<'a, TestSlot> {
        NotefieldComposeRequest {
            hud_style: style(),
            placement,
            view: ViewOverride::default(),
            geometry: NotefieldGeometry {
                player_idx,
                stage_seed: 0,
                num_players,
                cols_per_player,
                total_cols,
                single_style: true,
                double_style: false,
                center_one_player: false,
                screen_width: 640.0,
                screen_height: 480.0,
                screen_center_x: 320.0,
                screen_center_y: 240.0,
                field_zoom: 1.0,
                scroll_speed: ScrollSpeedSetting::XMod(1.0),
                draw_distance_before_targets: 480.0,
                draw_distance_after_targets: 480.0,
                column_dirs: [1.0; MAX_COLS],
                reverse_scroll: false,
            },
            visual: NotefieldVisualState {
                elapsed_screen_s: 0.1,
                current_display_beat: 1.0,
                accel: AccelEffects::default(),
                scroll: ScrollEffects::default(),
                perspective: PerspectiveEffects::default(),
                visual: VisualEffects::default(),
                appearance: AppearanceEffects::default(),
                dynamic_sudden: false,
                visibility: VisibilityEffects::default(),
                mini_percent: 0.0,
                spacing_multiplier: 1.0,
            },
            chart: NotefieldChartView {
                timing: Some(timing),
                notes,
                note_range: (0, notes.len()),
                lane_note_row_indices: &[],
                lane_hold_indices: &[],
                note_itg_rows: &[],
                note_time_cache_ns: &[],
                hold_end_time_cache_ns: &[],
                note_displayed_beat_cache: &[],
                decaying_hold_indices: &[],
                note_row_metadata: &[],
                visible_music_time_ns: 100_000_000,
                visible_beat: 1.0,
                is_in_delay: false,
                search_beat: 1.0,
                scroll_reference_bpm: 120.0,
                music_rate: 1.0,
                note_count_stats: &[],
                time_signatures: &[],
                bpms: &[],
                stops: &[],
                delays: &[],
                scrolls: &[],
                displayed_beat_monotonic: true,
            },
            noteskin: NotefieldNoteskinView {
                base: Some(noteskin),
                mine: Some(noteskin),
                receptor: Some(noteskin),
                tap_explosion: Some(noteskin),
            },
            song_lua: NotefieldSongLuaView {
                note_hides,
                column_offsets: &[],
            },
            options: options(),
            capture_requests: ProxyCaptureRequests::default(),
            edit_measure_text_slot_base: 0,
            arrow_effect_time_s: 0.1,
        }
    }

    fn note(column: usize) -> Note {
        Note {
            beat: 0.0,
            quantization_idx: 0,
            column,
            note_type: NoteType::Hold,
            row_index: 0,
            result: None,
            early_result: None,
            hold: None,
            mine_result: None,
            is_fake: false,
            can_be_judged: true,
        }
    }

    fn active_hold(note_index: usize) -> ActiveHold {
        ActiveHold {
            note_index,
            start_time_ns: 0,
            end_time_ns: 2_000_000_000,
            note_type: NoteType::Hold,
            let_go: false,
            is_pressed: true,
            life: 1.0,
            last_update_time_ns: 100_000_000,
        }
    }

    fn tap() -> Option<ActiveTapExplosion> {
        Some(ActiveTapExplosion {
            window: "W1",
            bright: false,
            elapsed: 0.1,
            duration: 1.0,
            start_beat: 0.0,
            effect_started_at_screen_s: 0.0,
        })
    }

    fn mine() -> Option<ActiveMineExplosion> {
        Some(ActiveMineExplosion {
            elapsed: 0.1,
            duration: 1.0,
            started_at_screen_s: 0.0,
        })
    }

    fn source(slot: &TestSlot) -> SpriteSource {
        SpriteSource::TextureHandle {
            key: Arc::clone(&slot.key),
            handle: 1,
            generation: 1,
        }
    }

    fn hold_emitter() -> HoldEmitter<TestSlot> {
        HoldEmitter {
            flash: deadsync_noteskin::TapExplosionLayer {
                slot: TestSlot::new("held-flash"),
                animation: deadsync_noteskin::parse_explosion_animation(
                    "blend,BlendMode_Add;diffuse,1,0.9411765,0.39215687,0.9;zoom,1;linear,0.15;diffuse,0,0,0,1;zoom,1.25",
                ),
            },
            interval_s: 4.0 / 60.0,
            count: 3,
        }
    }

    #[test]
    fn hold_emitter_cycles_children_and_finishes_queued_flash_after_off() {
        let emitter = hold_emitter();
        let mut state = HoldEmitterState::default();
        state.update(0.0, Some(false), Some(&emitter));
        assert_eq!(&state.flashes[..3], &[Some(0.0), None, None]);
        state.update(0.05, Some(false), Some(&emitter));
        state.update(0.10, Some(false), Some(&emitter));
        state.update(0.14, Some(false), Some(&emitter));
        assert_eq!(&state.flashes[..3], &[Some(0.0), Some(0.05), Some(0.10)]);
        state.update(0.14, None, Some(&emitter));
        state.update(0.19, None, Some(&emitter));
        state.update(0.21, None, Some(&emitter));
        assert_eq!(&state.flashes[..3], &[Some(0.19), None, Some(0.10)]);
        assert!(state.next_emit_s.is_none());
        state.update(0.40, None, Some(&emitter));
        assert!(state.flashes.iter().all(Option::is_none));
        state.update(0.41, Some(true), Some(&emitter));
        assert!(state.is_roll);
        assert_eq!(&state.flashes[..3], &[None, Some(0.41), None]);
        state.update(0.41, Some(true), Some(&emitter));
        assert_eq!(
            state.next_child, 2,
            "drawing another view must not emit twice"
        );
        state.update(0.45, None, Some(&emitter));
        state.update(0.46, Some(true), Some(&emitter));
        assert_eq!(
            &state.flashes[..3],
            &[None, None, Some(0.46)],
            "On finishes old child tweens"
        );
        state.update(1000.0, Some(true), Some(&emitter));
        assert!(
            state.flashes.iter().all(Option::is_none),
            "children consume the entire long frame delta"
        );
        state.update(0.0, Some(false), Some(&emitter));
        assert_eq!(
            &state.flashes[..3],
            &[Some(0.0), None, None],
            "rewinding resets the emitter"
        );
        let mut emitter = emitter;
        emitter.interval_s = 0.125;
        let mut state = HoldEmitterState::default();
        state.update(0.0, Some(false), Some(&emitter));
        state.update(0.125, Some(false), Some(&emitter));
        assert_eq!(
            state.next_child, 1,
            "a zero-duration command waits for positive remaining delta"
        );
        state.update(0.25, Some(false), Some(&emitter));
        assert_eq!(
            state.next_child, 2,
            "an exact second boundary must not emit twice"
        );
    }

    #[test]
    fn held_flash_layers_draw_additively_without_tap_or_receptor() {
        let mut skin = noteskin();
        skin.hold_columns[0].emitter = Some(hold_emitter());
        skin.hold_columns[0].explosion = None;
        skin.hold.explosion = None;
        let emitter = skin.hold_columns[0].emitter.as_ref().unwrap();
        let mut state = HoldEmitterState::default();
        for now in [0.0, 0.05, 0.10, 0.14] {
            state.update(now, Some(false), Some(emitter));
        }
        // Off must retain the fading children even with no active hold or tap.
        state.update(0.14, None, Some(emitter));
        let timing = TimingData::default();
        let notes = [note(0)];
        let hides = SongLuaNoteHideWindows::default();
        let mut request = request(
            &skin,
            &timing,
            &notes,
            &hides,
            FieldPlacement::P1,
            0,
            1,
            2,
            2,
        );
        request.options.hide_targets = true;
        request.visual.elapsed_screen_s = 0.14;
        let prepared = prepare_notefield(&request).unwrap();
        let frame = NotefieldFeedbackFrameView {
            column_cues: None,
            column_cue_cursor: None,
            crossover_cues: None,
            crossover_cue_entries: None,
            crossover_cue_cursor: None,
            column_flashes: None,
            tap_explosions: None,
            mine_explosions: None,
            lanes: std::array::from_fn(|col| NotefieldLaneFeedback {
                hold_emitter: if col == 0 {
                    state
                } else {
                    HoldEmitterState::default()
                },
                ..Default::default()
            }),
            countdown_font: "test",
            countdown_text_slot: 0,
        };
        let mut draws = Vec::new();
        compose_notefield_feedback(
            &mut draws,
            &mut Vec::new(),
            &mut ModelMeshCache::default(),
            &request,
            &prepared,
            &frame,
            &source,
        );
        assert_eq!(sprite_keys(&draws), ["held-flash"; 3]);
        for (draw, age) in draws.iter().zip([0.14, 0.09, 0.04]) {
            let FlatDraw::Sprite(sprite) = draw else {
                panic!("expected flash sprite");
            };
            assert_eq!(sprite.blend, deadlib_render_core::BlendMode::Add);
            assert!((sprite.tint[0] - (1.0 - age / 0.15)).abs() < 1e-5);
            assert!((sprite.size[0] / 64.0 - (1.0 + age / 0.15 * 0.25)).abs() < 1e-5);
            assert_eq!(sprite.z, crate::style::HOLD_EXPLOSION_Z);
        }
    }

    fn sprite_keys(draws: &[FlatDraw]) -> Vec<&str> {
        draws
            .iter()
            .filter_map(|draw| match draw {
                FlatDraw::Sprite(FlatSprite {
                    source: SpriteSource::TextureHandle { key, .. },
                    ..
                }) => Some(key.as_ref()),
                _ => None,
            })
            .collect()
    }

    fn sprite_positions(draws: &[FlatDraw]) -> Vec<(&str, [f32; 2])> {
        draws
            .iter()
            .filter_map(|draw| match draw {
                FlatDraw::Sprite(FlatSprite {
                    source: SpriteSource::TextureHandle { key, .. },
                    center,
                    ..
                }) => Some((key.as_ref(), *center)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn feedback_frame_preserves_phase_and_lane_order() {
        let noteskin = noteskin();
        let timing = TimingData::default();
        let notes = [note(0)];
        let note_hides = SongLuaNoteHideWindows::default();
        let mut request = request(
            &noteskin,
            &timing,
            &notes,
            &note_hides,
            FieldPlacement::P1,
            0,
            1,
            2,
            2,
        );
        request.visual.visual.tipsy = 0.7;
        request.visual.visual.drunk = 0.4;
        request.visual.visual.move_x_cols[0] = 0.25;
        request.visual.visual.move_y_cols[0] = -0.2;
        request.visual.visual.confusion_offset_cols[0] = 0.3;
        request.visual.visual.confusion = 0.7;
        request.visual.visual.tiny = 0.5;
        request.visual.visual.tiny_cols[1] = -0.25;
        request.visual.visual.pulse_outer = 0.75;
        request.visual.visual.pulse_period = 0.4;
        request.visual.visual.pulse_offset = 0.3;
        let mut prepared = prepare_notefield(&request).expect("test notefield should prepare");
        prepared.column_zooms[..2].copy_from_slice(&[1.75, 0.65]);
        prepared.column_rotations_deg[..2].copy_from_slice(&[13.5, -7.25]);
        let hold = active_hold(0);
        let flashes = [
            Some(ActiveColumnFlash {
                grade: JudgeGrade::Miss,
                blue_fantastic: false,
                started_at_screen_s: 0.0,
            }),
            None,
        ];
        let taps = [tap(), tap()];
        let mines = [mine(), mine()];
        let frame = NotefieldFeedbackFrameView {
            column_cues: None,
            column_cue_cursor: None,
            crossover_cues: None,
            crossover_cue_entries: None,
            crossover_cue_cursor: None,
            column_flashes: Some(&flashes),
            tap_explosions: Some(&taps),
            mine_explosions: Some(&mines),
            lanes: std::array::from_fn(|lane| match lane {
                0 => NotefieldLaneFeedback {
                    active_hold: Some(&hold),
                    receptor_bop_zoom: 1.0,
                    receptor_press_visual: Some((1.0, 1.0)),
                    ..NotefieldLaneFeedback::default()
                },
                1 => NotefieldLaneFeedback {
                    receptor_bop_zoom: 1.0,
                    receptor_press_visual: Some((1.0, 1.0)),
                    ..NotefieldLaneFeedback::default()
                },
                _ => NotefieldLaneFeedback::default(),
            }),
            countdown_font: "test",
            countdown_text_slot: 17,
        };
        let mut actors = Vec::new();
        let mut hud = Vec::new();

        compose_notefield_feedback(
            &mut actors,
            &mut hud,
            &mut ModelMeshCache::default(),
            &request,
            &prepared,
            &frame,
            &source,
        );

        assert!(matches!(
            actors.first(),
            Some(FlatDraw::Sprite(FlatSprite {
                source: SpriteSource::Solid,
                ..
            }))
        ));
        assert_eq!(
            sprite_keys(&actors),
            [
                "target0", "hold0", "press0", "target1", "press1", "tap0", "tap1", "mine", "mine",
            ]
        );
        let positions = sprite_positions(&actors);
        let target = positions
            .iter()
            .find(|(key, _)| *key == "target0")
            .map(|(_, position)| *position)
            .expect("target0 position");
        let tap = positions
            .iter()
            .find(|(key, _)| *key == "tap0")
            .map(|(_, position)| *position)
            .expect("tap0 position");
        let mine = positions
            .iter()
            .find(|(key, _)| *key == "mine")
            .map(|(_, position)| *position)
            .expect("lane-zero mine position");
        assert_eq!(tap.map(f32::to_bits), target.map(f32::to_bits));
        assert_eq!(mine.map(f32::to_bits), target.map(f32::to_bits));
        let explosions: Vec<_> = actors
            .iter()
            .filter_map(|draw| match draw {
                FlatDraw::Sprite(sprite)
                    if matches!(sprite.source.texture_key(), Some("tap0" | "tap1" | "mine")) =>
                {
                    Some(sprite)
                }
                _ => None,
            })
            .collect();
        for local_col in 0..2 {
            let effect = gameplay_visual_effect_params(&request.visual.visual, local_col);
            let base_zoom = visual_arrow_effect_zoom(0.0, effect);
            let tap_zoom = base_zoom * prepared.column_zooms[local_col];
            let tap_size = crate::scale_effect_size([64.0; 2], prepared.field_zoom, tap_zoom.abs());
            let mine_size =
                crate::scale_effect_size([64.0; 2], prepared.field_zoom, base_zoom.abs());
            assert_eq!(
                explosions[local_col].size.map(f32::to_bits),
                tap_size.map(f32::to_bits)
            );
            assert_eq!(
                explosions[local_col + 2].size.map(f32::to_bits),
                mine_size.map(f32::to_bits)
            );
            let rotation = visual_confusion_rotation_deg(prepared.current_beat, effect)
                + prepared.column_rotations_deg[local_col];
            assert_eq!(
                explosions[local_col].rot_z_deg.to_bits(),
                rotation.to_bits()
            );
        }
        assert!(hud.is_empty());
    }

    #[test]
    fn dark_columns_combine_with_global_dark_before_clamping() {
        let mut ns = noteskin();
        ns.receptor_idle_glow = ReceptorIdleGlow::ActorEffect;
        ns.receptor_idle_glow_layers =
            vec![Some(TestSlot::new("idle0")), Some(TestSlot::new("idle1"))];
        let timing = TimingData::default();
        let hides = SongLuaNoteHideWindows::default();
        let notes = [note(0), note(1)];
        let hold = active_hold(0);
        let taps = [tap(), tap()];
        let mut frame = spline_feedback(&taps);
        frame.lanes[0].active_hold = Some(&hold);
        for lane in &mut frame.lanes[..2] {
            lane.receptor_press_visual = Some((1.0, 1.0));
        }
        // Include negative levels and global values above 1: clamping either
        // component first would lose ITGmania's cancellation behavior.
        for (dark, columns, alphas) in [
            (0.25, [0.25, -0.5], [0.5, 1.0]),
            (1.5, [-1.0, -0.75], [0.5, 0.25]),
            (0.0, [1.0, 0.0], [0.0, 1.0]),
            (1.0, [0.0, 1.0], [0.0, 0.0]),
        ] {
            // P2 uses player-local column indices, even with a global offset.
            let mut request = request(&ns, &timing, &notes, &hides, FieldPlacement::P2, 1, 2, 2, 4);
            request.visual.visibility.dark = dark;
            request.visual.visibility.dark_cols[..2].copy_from_slice(&columns);
            let prepared = prepare_notefield(&request).unwrap();
            let mut draws = Vec::new();
            compose_notefield_feedback(
                &mut draws,
                &mut Vec::new(),
                &mut ModelMeshCache::default(),
                &request,
                &prepared,
                &frame,
                &source,
            );
            let alpha_for = |key| {
                draws.iter().find_map(|draw| match draw {
                    FlatDraw::Sprite(sprite) if sprite.source.texture_key() == Some(key) => {
                        Some(sprite.tint[3])
                    }
                    _ => None,
                })
            };
            for (keys, alpha) in [
                ["target0", "idle0", "press0"],
                ["target1", "idle1", "press1"],
            ]
            .into_iter()
            .zip(alphas)
            {
                for key in keys {
                    assert_eq!(
                        alpha_for(key),
                        (alpha > 0.0).then_some(alpha),
                        "{key}, global={dark}, columns={columns:?}"
                    );
                }
            }
            assert_eq!(
                alpha_for("hold0"),
                Some(1.0),
                "dark must preserve hold feedback"
            );
            assert_eq!(
                alpha_for("tap0"),
                Some(2.0 / 3.0),
                "dark must preserve tap feedback"
            );
            assert_eq!(
                alpha_for("tap1"),
                Some(2.0 / 3.0),
                "dark must preserve tap feedback"
            );
        }
    }

    #[test]
    fn receptor_beat_effects_preserve_lane_tints_and_delay_boundaries() {
        let mut ns = noteskin();
        ns.receptor_pulse = ReceptorPulse {
            effect_color1: [0.25, 0.5, 0.75, 0.6],
            effect_color2: [0.9, 0.7, 0.3, 0.2],
            ramp_to_half: 0.25,
            hold_at_half: 0.125,
            ramp_to_full: 0.5,
            hold_at_full: 0.125,
            ..ReceptorPulse::default()
        };
        ns.receptor_idle_glow_layers =
            vec![Some(TestSlot::new("idle0")), Some(TestSlot::new("idle1"))];
        let timing = TimingData::default();
        let hides = SongLuaNoteHideWindows::default();
        let frame = spline_feedback(&[]);
        for idle in [
            ReceptorIdleGlow::None,
            ReceptorIdleGlow::BeatFade,
            ReceptorIdleGlow::ActorEffect,
        ] {
            ns.receptor_idle_glow = idle;
            for (beat, in_delay, hide_targets) in [
                (-0.25, false, false),
                (0.0, false, false),
                (0.25, false, false),
                (0.5, false, false),
                (0.75, false, false),
                (4.0, true, false),
                (0.25, false, true),
                (f32::NAN, false, false),
            ] {
                let mut request =
                    request(&ns, &timing, &[], &hides, FieldPlacement::P1, 0, 1, 2, 2);
                request.chart.visible_beat = beat;
                request.chart.is_in_delay = in_delay;
                request.visual.visibility.dark = 0.25;
                request.options.hide_targets = hide_targets;
                let prepared = prepare_notefield(&request).unwrap();
                let mut draws = Vec::new();
                compose_notefield_feedback(
                    &mut draws,
                    &mut Vec::new(),
                    &mut ModelMeshCache::default(),
                    &request,
                    &prepared,
                    &frame,
                    &source,
                );
                let mut expected = Vec::new();
                if !hide_targets {
                    for (local_col, (target_key, idle_key)) in
                        [("target0", "idle0"), ("target1", "idle1")]
                            .into_iter()
                            .enumerate()
                    {
                        let color = ns.receptor_pulse.color_for_beat(beat);
                        let alpha = color[3] * 1.0 * prepared.receptor_alphas[local_col];
                        if alpha > f32::EPSILON {
                            expected.push((
                                target_key,
                                [color[0], color[1], color[2], alpha].map(f32::to_bits),
                            ));
                        }
                        let alpha =
                            idle.alpha(beat, in_delay) * 1.0 * prepared.receptor_alphas[local_col];
                        if idle.is_visible() && alpha > f32::EPSILON {
                            expected.push((idle_key, [1.0, 1.0, 1.0, alpha].map(f32::to_bits)));
                        }
                    }
                }
                let actual: Vec<_> = draws
                    .iter()
                    .map(|draw| {
                        let FlatDraw::Sprite(sprite) = draw else {
                            panic!("receptor sprite")
                        };
                        (
                            sprite.source.texture_key().unwrap(),
                            sprite.tint.map(f32::to_bits),
                        )
                    })
                    .collect();
                assert_eq!(
                    actual, expected,
                    "beat={beat}, delay={in_delay}, idle={idle:?}"
                );
            }
        }
    }

    #[test]
    fn square_z_moves_receptors_and_live_explosions_at_zero_travel() {
        let ns = noteskin();
        let timing = TimingData::default();
        let notes = [note(0)];
        let hides = SongLuaNoteHideWindows::default();
        let hold = active_hold(0);
        let taps = [tap(), None];
        let mines = [mine(), None];
        let mut feedback = spline_feedback(&taps);
        feedback.lanes[0].active_hold = Some(&hold);
        feedback.mine_explosions = Some(&mines);
        // Native RageSquare vectors at the receptor, with independent X/Z phases.
        for (offset, period, expected_z) in
            [(0.0, 0.0, -32.0), (32.0, 0.0, 32.0), (64.0, 1.0, 32.0)]
        {
            let mut request = request(&ns, &timing, &notes, &hides, FieldPlacement::P1, 0, 1, 2, 2);
            request.visual.visual.square = 0.5;
            request.visual.visual.square_z = 1.0;
            request.visual.visual.square_z_offset = offset;
            request.visual.visual.square_z_period = period;
            let prepared = prepare_notefield(&request).expect("prepared field");
            let mut draws = Vec::new();
            compose_notefield_feedback(
                &mut draws,
                &mut Vec::new(),
                &mut ModelMeshCache::default(),
                &request,
                &prepared,
                &feedback,
                &source,
            );
            for key in ["target0", "hold0", "tap0", "mine"] {
                let sprite = draws
                    .iter()
                    .find_map(|draw| match draw {
                        FlatDraw::Sprite(sprite)
                            if matches!(&sprite.source,
                        SpriteSource::TextureHandle { key: name, .. } if name.as_ref() == key) =>
                        {
                            Some(sprite)
                        }
                        _ => None,
                    })
                    .expect("rendered receptor or explosion");
                assert_eq!(sprite.world_z, expected_z, "{key}");
                assert_eq!(
                    sprite.center[0],
                    prepared.field.playfield_center_x - 32.0 - 16.0
                );
            }
        }
    }

    #[test]
    fn riddle_note_and_feedback_rotation_match_native_vertices() {
        use deadlib_present::compose::{ActorSegment, ComposeScratch};
        struct Textures;
        impl deadlib_present::texture::TextureContext for Textures {
            fn texture_registry_generation(&self) -> u64 {
                1
            }
            fn texture_dims(&self, _: &str) -> Option<deadlib_present::texture::TextureMeta> {
                Some(deadlib_present::texture::TextureMeta { w: 64, h: 64 })
            }
            fn sprite_sheet_dims(&self, _: &str) -> (u32, u32) {
                (1, 1)
            }
            fn texture_handle(&self, _: &str) -> u64 {
                1
            }
        }
        let native: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/itgmania-actors/riddle-rotation.json"
        )))
        .expect("native Riddle rotation fixture");
        let cases = [
            (103.125, 0.8, 0.0, 0.0),
            (103.25, -0.4, 0.0, 0.0),
            (200.0, std::f32::consts::FRAC_PI_2, 0.0, 0.0),
            (3.5, 0.4, 1.5, 0.0),
            (3.5, -0.4, -1.5, 0.0),
            (68.0, 0.0, 0.0, -0.5),
        ];
        let ns = noteskin();
        let timing = TimingData::default();
        let notes = [note(0)];
        let hides = SongLuaNoteHideWindows::default();
        let hold = active_hold(0);
        let taps = [tap(), None];
        let mut feedback = spline_feedback(&taps);
        feedback.lanes[0].active_hold = Some(&hold);
        let metrics = deadlib_present::space::Metrics::centered(640.0, 480.0);
        let mut checked = 0;
        for (index, (beat, offset, confusion, dizzy)) in cases.into_iter().enumerate() {
            let mut request = request(&ns, &timing, &notes, &hides, FieldPlacement::P1, 0, 1, 2, 2);
            request.visual.current_display_beat = beat;
            request.chart.visible_beat = beat;
            request.visual.visual.confusion_offset = offset;
            request.visual.visual.confusion = confusion;
            request.visual.visual.dizzy = dizzy;
            let prepared = prepare_notefield(&request).expect("prepared field");
            let mut draws = Vec::new();
            compose_notefield_feedback(
                &mut draws,
                &mut Vec::new(),
                &mut ModelMeshCache::default(),
                &request,
                &prepared,
                &feedback,
                &source,
            );
            let find = |key| {
                draws
                    .iter()
                    .find_map(|draw| match draw {
                        FlatDraw::Sprite(sprite)
                            if matches!(&sprite.source,
                    SpriteSource::TextureHandle { key: name, .. } if name.as_ref() == key) =>
                        {
                            Some(sprite.clone())
                        }
                        _ => None,
                    })
                    .expect("rendered feedback source")
            };
            let receptor = find("target0");
            let mut note = receptor.clone();
            note.rot_z_deg = crate::visual_note_rotation_z_cached(
                beat + 2.0,
                crate::lane_note_transform_cache(
                    beat,
                    gameplay_visual_effect_params(&request.visual.visual, 0),
                ),
            );
            for (role, mut sprite) in [
                ("receptor", receptor),
                ("hold", find("hold0")),
                ("flash", find("tap0")),
                ("note", note),
            ] {
                // Isolate the shared rotation contract from noteskin pulses and layout.
                sprite.center = [320.0, 240.0];
                sprite.size = [64.0; 2];
                let draws = [FlatDraw::Sprite(sprite)];
                let frame = deadlib_present::compose::build_screen_segments_cached_with_scratch_and_texture_context_and_actor_resources(
                    &[ActorSegment::new(&[]).with_flat_draws(&draws, None)], [0.0; 4], &metrics,
                    &deadlib_present::font::FontMap::default(), 0.0,
                    &mut deadlib_present::compose::TextLayoutCache::default(), &mut ComposeScratch::default(),
                    &Textures, &deadlib_present::actors::ActorResourceArena::new(0),
                );
                assert_eq!(frame.sprite_instances.len(), 1, "{role} must draw");
                assert_eq!(frame.ops.len(), 1, "{role} must submit geometry");
                let deadlib_render_core::DrawOp::Sprite(run) = &frame.ops[0] else {
                    panic!("sprite draw")
                };
                let sprite = &frame.sprite_instances[0];
                let camera = frame.cameras[run.camera as usize];
                let name = format!("case_{index}_{role}");
                let actor = native["samples"][0]["actors"]
                    .as_array()
                    .expect("native actors")
                    .iter()
                    .find(|a| a["name"] == name)
                    .expect("native role");
                let vertices = actor["draws"][0]["vertices"]
                    .as_array()
                    .expect("native vertices");
                for vertex in vertices {
                    let u = vertex["uv"][0].as_f64().expect("u") as f32;
                    let v = vertex["uv"][1].as_f64().expect("v") as f32;
                    let x = (u - 0.5) * sprite.size[0];
                    let y = (0.5 - v) * sprite.size[1];
                    let [sin, cos] = sprite.rot_sin_cos;
                    let clip = camera
                        * glam::Vec4::new(
                            sprite.center[0] + x * cos - y * sin,
                            sprite.center[1] + x * sin + y * cos,
                            sprite.center[2],
                            1.0,
                        );
                    for axis in 0..4 {
                        let expected = vertex["clip"][axis].as_f64().expect("native clip") as f32;
                        assert!(
                            (clip[axis] - expected).abs() < 0.000002,
                            "{name}: {clip:?} != {}",
                            vertex["clip"]
                        );
                    }
                    checked += 1;
                }
            }
        }
        assert_eq!(
            checked, 96,
            "four textured corners for each role and rotation"
        );
    }

    #[test]
    fn hidden_idle_receptors_emit_no_feedback_actors() {
        let noteskin = noteskin();
        let timing = TimingData::default();
        let notes = [note(0)];
        let note_hides = SongLuaNoteHideWindows::default();
        let mut request = request(
            &noteskin,
            &timing,
            &notes,
            &note_hides,
            FieldPlacement::P1,
            0,
            1,
            2,
            2,
        );
        request.options.hide_targets = true;
        request.options.hold_explosion_enabled = false;
        let prepared = prepare_notefield(&request).expect("test notefield should prepare");
        let inactive_taps = [None; 2];
        let inactive_mines = [const { None }; 2];
        let frame = NotefieldFeedbackFrameView {
            column_cues: None,
            column_cue_cursor: None,
            crossover_cues: None,
            crossover_cue_entries: None,
            crossover_cue_cursor: None,
            column_flashes: None,
            tap_explosions: Some(&inactive_taps),
            mine_explosions: Some(&inactive_mines),
            lanes: [NotefieldLaneFeedback::default(); MAX_COLS],
            countdown_font: "test",
            countdown_text_slot: 17,
        };
        let mut actors = Vec::new();

        compose_notefield_feedback(
            &mut actors,
            &mut Vec::new(),
            &mut ModelMeshCache::default(),
            &request,
            &prepared,
            &frame,
            &source,
        );

        assert!(actors.is_empty());
    }

    #[test]
    fn unavailable_explosion_assets_skip_retained_feedback_state() {
        let noteskin = noteskin();
        let timing = TimingData::default();
        let notes = [note(0)];
        let note_hides = SongLuaNoteHideWindows::default();
        let request = request(
            &noteskin,
            &timing,
            &notes,
            &note_hides,
            FieldPlacement::P1,
            0,
            1,
            2,
            2,
        );
        let prepared = prepare_notefield(&request).expect("test notefield should prepare");
        let frame = NotefieldFeedbackFrameView {
            column_cues: None,
            column_cue_cursor: None,
            crossover_cues: None,
            crossover_cue_entries: None,
            crossover_cue_cursor: None,
            column_flashes: None,
            tap_explosions: None,
            mine_explosions: None,
            lanes: std::array::from_fn(|lane| {
                (lane < 2)
                    .then_some(NotefieldLaneFeedback {
                        receptor_bop_zoom: 1.0,
                        ..NotefieldLaneFeedback::default()
                    })
                    .unwrap_or_default()
            }),
            countdown_font: "test",
            countdown_text_slot: 17,
        };
        let mut actors = Vec::new();

        compose_notefield_feedback(
            &mut actors,
            &mut Vec::new(),
            &mut ModelMeshCache::default(),
            &request,
            &prepared,
            &frame,
            &source,
        );

        assert_eq!(sprite_keys(&actors), ["target0", "target1"]);
    }

    #[test]
    fn pump_receptors_emit_center_to_outer_draw_order() {
        let mut noteskin = noteskin();
        noteskin.receptor_off = (0..5)
            .map(|lane| TestSlot::new(format!("target{lane}")))
            .collect();
        noteskin.receptor_glow = vec![None; 5];
        noteskin.receptor_off_reverse = vec![ReceptorReverseBehavior::default(); 5];
        noteskin.receptor_glow_reverse = vec![ReceptorReverseBehavior::default(); 5];
        noteskin.column_xs = vec![-96, -48, 0, 48, 96];
        let timing = TimingData::default();
        let note_hides = SongLuaNoteHideWindows::default();
        let request = request(
            &noteskin,
            &timing,
            &[],
            &note_hides,
            FieldPlacement::P1,
            0,
            1,
            5,
            5,
        );
        let prepared = prepare_notefield(&request).expect("Pump notefield should prepare");
        let frame = NotefieldFeedbackFrameView {
            column_cues: None,
            column_cue_cursor: None,
            crossover_cues: None,
            crossover_cue_entries: None,
            crossover_cue_cursor: None,
            column_flashes: None,
            tap_explosions: None,
            mine_explosions: None,
            lanes: std::array::from_fn(|lane| NotefieldLaneFeedback {
                receptor_bop_zoom: if lane < 5 { 1.0 } else { 0.0 },
                ..NotefieldLaneFeedback::default()
            }),
            countdown_font: "test",
            countdown_text_slot: 17,
        };
        let mut actors = Vec::new();

        compose_notefield_feedback(
            &mut actors,
            &mut Vec::new(),
            &mut ModelMeshCache::default(),
            &request,
            &prepared,
            &frame,
            &source,
        );

        assert_eq!(
            sprite_keys(&actors),
            ["target2", "target1", "target3", "target0", "target4"]
        );
    }

    #[test]
    fn p2_local_lanes_honor_song_lua_hiding_except_mines() {
        let noteskin = noteskin();
        let timing = TimingData::default();
        let notes = [note(2), note(3)];
        let hides = SongLuaNoteHideWindows::new(vec![SongLuaNoteHideWindowRuntime {
            column: 0,
            start_beat: 0.0,
            end_beat: 2.0,
        }]);
        let request = request(
            &noteskin,
            &timing,
            &notes,
            &hides,
            FieldPlacement::P2,
            1,
            2,
            2,
            4,
        );
        let prepared = prepare_notefield(&request).expect("P2 notefield should prepare");
        assert_eq!(prepared.frame_plan.col_start, 2);
        let holds = [active_hold(0), active_hold(1)];
        let taps = [tap(), tap()];
        let mines = [mine(), mine()];
        let cues = [ColumnCue {
            start_time: 0.0,
            duration: 1.0,
            columns: [deadsync_gameplay::ColumnCueColumn {
                column: 3,
                is_mine: false,
            }]
            .into(),
        }];
        let frame = NotefieldFeedbackFrameView {
            column_cues: Some(&cues),
            column_cue_cursor: None,
            crossover_cues: None,
            crossover_cue_entries: None,
            crossover_cue_cursor: None,
            column_flashes: None,
            tap_explosions: Some(&taps),
            mine_explosions: Some(&mines),
            lanes: std::array::from_fn(|lane| {
                if lane < 2 {
                    NotefieldLaneFeedback {
                        active_hold: Some(&holds[lane]),
                        receptor_bop_zoom: 1.0,
                        receptor_press_visual: Some((1.0, 1.0)),
                        ..NotefieldLaneFeedback::default()
                    }
                } else {
                    NotefieldLaneFeedback::default()
                }
            }),
            countdown_font: "test",
            countdown_text_slot: 17,
        };
        let mut actors = Vec::new();

        compose_notefield_feedback(
            &mut actors,
            &mut Vec::new(),
            &mut ModelMeshCache::default(),
            &request,
            &prepared,
            &frame,
            &source,
        );

        let FlatDraw::Sprite(FlatSprite {
            source: SpriteSource::Solid,
            center,
            ..
        }) = &actors[0]
        else {
            panic!("P2 global cue should emit first");
        };
        let expected_x = prepared.field.playfield_center_x + 32.0;
        assert!((center[0] - expected_x).abs() <= 0.001);
        assert_eq!(
            sprite_keys(&actors),
            ["target1", "hold1", "press1", "tap1", "mine", "mine"]
        );
    }

    fn spline_feedback(taps: &[Option<ActiveTapExplosion>]) -> NotefieldFeedbackFrameView<'_> {
        NotefieldFeedbackFrameView {
            column_cues: None,
            column_cue_cursor: None,
            crossover_cues: None,
            crossover_cue_entries: None,
            crossover_cue_cursor: None,
            column_flashes: None,
            tap_explosions: Some(taps),
            mine_explosions: None,
            lanes: [NotefieldLaneFeedback {
                receptor_bop_zoom: 1.0,
                ..NotefieldLaneFeedback::default()
            }; MAX_COLS],
            countdown_font: "test",
            countdown_text_slot: 0,
        }
    }

    #[test]
    fn returning_receptor_and_live_flash_share_spline_zoom() {
        let ns = noteskin();
        let timing = TimingData::default();
        let notes = [note(0)];
        let mut hides = SongLuaNoteHideWindows::new(vec![SongLuaNoteHideWindowRuntime {
            column: 0,
            start_beat: 1.0,
            end_beat: 2.0,
        }]);
        hides.set_zoom_spline(0, 1.0 / 48.0, 98);
        let taps = [tap(), None];
        let frame = spline_feedback(&taps);
        let mut prior_width = 0.0;
        for (beat, returning) in [
            (1.0 + 0.5 / 48.0, false),
            (2.0, true),
            (2.0 + 0.25 / 48.0, true),
            (2.0 + 0.5 / 48.0, true),
            (2.0 + 1.0 / 48.0, true),
        ] {
            let mut request = request(&ns, &timing, &notes, &hides, FieldPlacement::P1, 0, 1, 2, 2);
            request.chart.visible_beat = beat;
            request.visual.current_display_beat = beat;
            let prepared = prepare_notefield(&request).expect("prepared feedback");
            let mut draws = Vec::new();
            compose_notefield_feedback(
                &mut draws,
                &mut Vec::new(),
                &mut ModelMeshCache::default(),
                &request,
                &prepared,
                &frame,
                &source,
            );
            let target = draws.iter().find_map(|draw| match draw {
                FlatDraw::Sprite(sprite) if matches!(&sprite.source, SpriteSource::TextureHandle { key, .. } if key.as_ref() == "target0") => Some(sprite),
                _ => None,
            });
            if beat == 2.0 {
                assert!(target.is_none());
                assert!(!sprite_keys(&draws).contains(&"tap0"));
            } else {
                let target = target.expect("receptor grows back");
                if returning {
                    assert!(target.size[0] > prior_width && target.size[0] <= 64.001);
                    prior_width = target.size[0];
                } else {
                    assert!(
                        target.flip_x && target.flip_y,
                        "native negative spline overshoot mirrors the receptor"
                    );
                }
                let positions = sprite_positions(&draws);
                let flash = positions
                    .iter()
                    .find(|(key, _)| *key == "tap0")
                    .expect("live final-hit flash returns");
                assert_eq!(target.center, flash.1);
            }
        }
    }

    #[test]
    fn holds_without_hide_windows_match_zero_offset_sampling() {
        use crate::{
            CapturedActorScratch, HoldMeshScratch, NotefieldCameraCache, NotefieldFieldFrameView,
            compose_notefield_field,
        };
        use deadsync_rules::note::HoldData;

        let mut ns = noteskin();
        ns.hold_columns[0].head_inactive = Some(TestSlot::new("head"));
        ns.hold_columns[0].body_inactive = Some(TestSlot::new("body"));
        ns.hold_columns[0].head_active = Some(TestSlot::new("head"));
        ns.hold_columns[0].body_active = Some(TestSlot::new("body"));
        ns.roll_columns = ns.hold_columns.clone();
        let timing = TimingData::default();
        let mut hold = note(0);
        hold.beat = 8.0;
        hold.row_index = 384;
        hold.hold = Some(HoldData {
            end_row_index: 576,
            end_beat: 12.0,
            result: None,
            life: 1.0,
            let_go_started_at: None,
            let_go_starting_life: 1.0,
            last_held_row_index: 384,
            last_held_beat: 8.0,
        });
        let lanes = [
            vec![deadsync_gameplay::ChartNoteIndex::try_from_usize(0).unwrap()],
            vec![],
        ];
        // This column still takes the hide-window sampling path, but the window
        // cannot affect the hold. It is the reference for the empty-column path.
        let distant = SongLuaNoteHideWindows::new(vec![SongLuaNoteHideWindowRuntime {
            column: 0,
            start_beat: 1000.0,
            end_beat: 1001.0,
        }]);
        for configured_empty_spline in [false, true] {
            let mut empty = SongLuaNoteHideWindows::default();
            if configured_empty_spline {
                empty.set_zoom_spline(0, 1.0 / 48.0, 482);
            }
            assert!(!empty.has_column_hides(0));
            for beat in [
                -f32::MAX,
                -1.0,
                -0.0,
                0.0,
                8.5,
                f32::MAX,
                f32::NAN,
                f32::INFINITY,
            ] {
                assert_eq!(empty.zoom_offset(0, beat).to_bits(), 0.0_f32.to_bits());
            }
            for (note_type, beat) in [
                (NoteType::Hold, 6.0),
                (NoteType::Hold, 9.0),
                (NoteType::Roll, 6.0),
                (NoteType::Roll, 9.0),
            ] {
                hold.note_type = note_type;
                let notes = [hold.clone()];
                let render = |hides: &SongLuaNoteHideWindows| {
                    let mut request =
                        request(&ns, &timing, &notes, hides, FieldPlacement::P1, 0, 1, 2, 2);
                    request.chart.visible_beat = beat;
                    request.chart.search_beat = beat;
                    request.chart.lane_note_row_indices = &lanes;
                    request.chart.lane_hold_indices = &lanes;
                    request.chart.note_itg_rows = &[384];
                    request.visual.current_display_beat = beat;
                    // Both cases use sliced geometry, independently of hide windows.
                    request.visual.visual.bumpy = 0.3;
                    request.visual.visual.tiny = -0.2;
                    request.visual.visual.pulse_outer = 0.25;
                    let prepared = prepare_notefield(&request).unwrap();
                    let mut active = active_hold(0);
                    active.note_type = note_type;
                    let mut feedback = spline_feedback(&[]);
                    if beat > 8.0 {
                        feedback.lanes[0].active_hold = Some(&active);
                    }
                    let frame = NotefieldFieldFrameView {
                        feedback,
                        completed_rows: Default::default(),
                    };
                    let mut draws = Vec::new();
                    compose_notefield_field(
                        &mut Vec::new(),
                        &mut draws,
                        &mut Vec::new(),
                        &mut ModelMeshCache::default(),
                        &mut HoldMeshScratch::default(),
                        &mut CapturedActorScratch::with_capacities(32, 0),
                        &mut NotefieldCameraCache::default(),
                        &request,
                        &prepared,
                        &frame,
                        &source,
                    );
                    assert!(draws.iter().any(|draw| matches!(draw, FlatDraw::TexturedMesh(mesh) if mesh.texture.as_ref() == "body")));
                    format!("{draws:?}")
                };
                assert_eq!(render(&empty), render(&distant));
            }
        }
    }

    #[test]
    fn mixed_hold_heads_sample_the_selected_arrow_layout() {
        use crate::{
            CapturedActorScratch, HoldMeshScratch, NotefieldCameraCache, NotefieldFieldFrameView,
            compose_notefield_field,
        };
        use deadsync_noteskin::{NUM_QUANTIZATIONS, NoteAnimPart, runtime::SkinPart};
        use deadsync_rules::note::HoldData;

        let mut base = noteskin();
        base.notes = vec![TestSlot::new("tap-head"); 2 * NUM_QUANTIZATIONS];
        // HURG Dev TD Dark selects colors vertically; bundled Cel/Metal heads
        // select them horizontally. The latter splits a HURG sprite cell in half.
        base.note_display_metrics.part_texture_translate[NoteAnimPart::Tap as usize]
            .note_color_spacing = [0.0, 0.125];
        for part in [NoteAnimPart::HoldHead, NoteAnimPart::RollHead] {
            base.note_display_metrics.part_texture_translate[part as usize].note_color_spacing =
                [0.03125, 0.0];
        }
        base.hold_columns[0].body_inactive = Some(TestSlot::new("body"));
        base.hold_columns[0].body_active = Some(TestSlot::new("body"));
        base.roll_columns = base.hold_columns.clone();
        let timing = TimingData::default();
        let hides = SongLuaNoteHideWindows::default();
        let lanes = [
            vec![deadsync_gameplay::ChartNoteIndex::try_from_usize(0).unwrap()],
            vec![],
        ];

        for layered in [false, true] {
            for (mixed, explicit) in [(false, false), (true, false), (true, true)] {
                let mut ns = base.clone();
                if layered {
                    ns.note_layers =
                        vec![
                            Arc::from([TestSlot::new("tap-head"), TestSlot::new("outline")]);
                            2 * NUM_QUANTIZATIONS
                        ];
                }
                if mixed {
                    let arrows = ns.clone();
                    ns.apply_part(&arrows, SkinPart::Arrows);
                    // Hold selections are applied after arrows during song load.
                    for part in [
                        SkinPart::HoldActive,
                        SkinPart::HoldInactive,
                        SkinPart::RollActive,
                        SkinPart::RollInactive,
                    ] {
                        ns.apply_part(&base, part);
                    }
                }
                if explicit {
                    for visuals in [&mut ns.hold_columns[0], &mut ns.roll_columns[0]] {
                        visuals.head_active = Some(TestSlot::new("head"));
                        visuals.head_inactive = Some(TestSlot::new("head"));
                        if layered {
                            let layers =
                                Arc::from([TestSlot::new("head"), TestSlot::new("outline")]);
                            visuals.head_active_layers = Some(Arc::clone(&layers));
                            visuals.head_inactive_layers = Some(layers);
                        }
                    }
                }
                for (note_type, beat) in [
                    (NoteType::Hold, 6.0),
                    (NoteType::Hold, 9.0),
                    (NoteType::Roll, 6.0),
                    (NoteType::Roll, 9.0),
                ] {
                    let mut hold = note(0);
                    hold.note_type = note_type;
                    hold.beat = 8.5;
                    hold.row_index = 408;
                    hold.quantization_idx = 1;
                    hold.hold = Some(HoldData {
                        end_row_index: 576,
                        end_beat: 12.0,
                        result: None,
                        life: 1.0,
                        let_go_started_at: None,
                        let_go_starting_life: 1.0,
                        last_held_row_index: 408,
                        last_held_beat: 8.5,
                    });
                    let notes = [hold];
                    let mut request =
                        request(&ns, &timing, &notes, &hides, FieldPlacement::P1, 0, 1, 2, 2);
                    request.chart.visible_beat = beat;
                    request.chart.search_beat = beat;
                    request.chart.lane_note_row_indices = &lanes;
                    request.chart.lane_hold_indices = &lanes;
                    request.chart.note_itg_rows = &[408];
                    request.visual.current_display_beat = beat;
                    request.visual.visual.roll = 15.0;
                    let prepared = prepare_notefield(&request).unwrap();
                    let mut active = active_hold(0);
                    active.note_type = note_type;
                    let mut feedback = spline_feedback(&[]);
                    if beat > 8.5 {
                        feedback.lanes[0].active_hold = Some(&active);
                    }
                    let frame = NotefieldFieldFrameView {
                        feedback,
                        completed_rows: Default::default(),
                    };
                    let mut draws = Vec::new();
                    compose_notefield_field(
                        &mut Vec::new(),
                        &mut draws,
                        &mut Vec::new(),
                        &mut ModelMeshCache::default(),
                        &mut HoldMeshScratch::default(),
                        &mut CapturedActorScratch::with_capacities(32, 0),
                        &mut NotefieldCameraCache::default(),
                        &request,
                        &prepared,
                        &frame,
                        &source,
                    );
                    let heads: Vec<_> = draws.iter().filter_map(|draw| match draw {
                        FlatDraw::Sprite(sprite) if matches!(&sprite.source, SpriteSource::TextureHandle { key, .. } if matches!(key.as_ref(), "head" | "tap-head" | "outline")) => Some(sprite),
                        _ => None,
                    }).collect();
                    assert_eq!(heads.len(), if layered { 2 } else { 1 });
                    for head in heads {
                        assert_eq!(head.rot_x_deg, 0.0, "Roll must exclude hold heads");
                        assert_eq!(
                            head.uv_rect,
                            if mixed && !explicit {
                                [0.0, 0.125, 1.0, 1.125]
                            } else {
                                [0.03125, 0.0, 1.03125, 1.0]
                            },
                            "{note_type:?}, beat={beat}, layered={layered}, mixed={mixed}, explicit={explicit}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn hidden_hold_head_keeps_visible_body_sections() {
        use crate::{
            CapturedActorScratch, HoldMeshScratch, NotefieldCameraCache, NotefieldFieldFrameView,
            compose_notefield_field,
        };
        use deadlib_present::actors::FlatMeshVertices;
        use deadsync_rules::note::HoldData;
        let mut ns = noteskin();
        ns.notes = (0..2 * deadsync_noteskin::NUM_QUANTIZATIONS)
            .map(|_| TestSlot::new("tap-head"))
            .collect();
        ns.hold_columns[0].head_inactive = Some(TestSlot::new("head"));
        ns.hold_columns[0].body_inactive = Some(TestSlot::new("body"));
        ns.hold_columns[0].head_active = Some(TestSlot::new("head"));
        ns.hold_columns[0].body_active = Some(TestSlot::new("body"));
        ns.roll_columns = ns.hold_columns.clone();
        let timing = TimingData::default();
        let mut hold_note = note(0);
        hold_note.beat = 8.0;
        hold_note.row_index = 384;
        hold_note.hold = Some(HoldData {
            end_row_index: 576,
            end_beat: 12.0,
            result: None,
            life: 1.0,
            let_go_started_at: None,
            let_go_starting_life: 1.0,
            last_held_row_index: 384,
            last_held_beat: 8.0,
        });
        for (note_type, beat) in [
            (NoteType::Hold, 6.0),
            (NoteType::Hold, 9.0),
            (NoteType::Roll, 6.0),
            (NoteType::Roll, 9.0),
        ] {
            let mut drawn_note = hold_note.clone();
            drawn_note.note_type = note_type;
            let notes = [drawn_note];
            let lanes = [
                vec![deadsync_gameplay::ChartNoteIndex::try_from_usize(0).expect("index")],
                vec![],
            ];
            let mut hides = SongLuaNoteHideWindows::new(vec![SongLuaNoteHideWindowRuntime {
                column: 0,
                start_beat: 8.0,
                end_beat: 10.0,
            }]);
            hides.set_zoom_spline(0, 1.0 / 48.0, 482);
            let mut request = request(&ns, &timing, &notes, &hides, FieldPlacement::P1, 0, 1, 2, 2);
            request.chart.visible_beat = beat;
            request.chart.search_beat = beat;
            request.chart.lane_note_row_indices = &lanes;
            request.chart.lane_hold_indices = &lanes;
            request.chart.note_itg_rows = &[384];
            request.visual.current_display_beat = beat;
            let prepared = prepare_notefield(&request).expect("prepared hold field");
            let mut active = active_hold(0);
            active.note_type = note_type;
            let mut feedback = spline_feedback(&[]);
            if beat > 8.0 {
                feedback.lanes[0].active_hold = Some(&active);
            }
            let frame = NotefieldFieldFrameView {
                feedback,
                completed_rows: Default::default(),
            };
            let mut draws = Vec::new();
            compose_notefield_field(
                &mut Vec::new(),
                &mut draws,
                &mut Vec::new(),
                &mut ModelMeshCache::default(),
                &mut HoldMeshScratch::default(),
                &mut CapturedActorScratch::with_capacities(32, 0),
                &mut NotefieldCameraCache::default(),
                &request,
                &prepared,
                &frame,
                &source,
            );
            assert!(!sprite_keys(&draws).contains(&"head"));
            let vertices: Vec<_> = draws
                .iter()
                .filter_map(|draw| match draw {
                    FlatDraw::TexturedMesh(mesh) if mesh.texture.as_ref() == "body" => {
                        Some(match &mesh.vertices {
                            FlatMeshVertices::Shared(v) => v.as_ref(),
                            FlatMeshVertices::Reusable(v) => v.as_slice(),
                        })
                    }
                    _ => None,
                })
                .flatten()
                .collect();
            assert!(
                !vertices.is_empty(),
                "body must render despite its hidden head"
            );
            let center_x = prepared.field.playfield_center_x - 32.0;
            assert!(
                vertices.iter().any(|v| (v.pos[0] - center_x).abs() < 0.001),
                "hidden body section collapses"
            );
            assert!(
                vertices.iter().any(|v| (v.pos[0] - center_x).abs() > 30.0),
                "tail section keeps full width"
            );
        }
    }

    #[test]
    fn draw_size_matches_native_pixel_ranges() {
        use crate::compose::NoteDrawRange;
        let ns = noteskin();
        let timing = TimingData::default();
        let hides = SongLuaNoteHideWindows::default();
        let mut request = request(&ns, &timing, &[], &hides, FieldPlacement::P1, 0, 1, 2, 2);
        request.geometry.draw_distance_before_targets = 720.0;
        request.geometry.draw_distance_after_targets = 130.0;
        // These constants come from the verbatim checked-out C++
        // NoteField::CalcPixelsBeforeAndAfterTargets, compiled with MSVC.
        for (size, back, tilt, mini, centered, boomerang, expected) in [
            (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, [-130.0, 720.0]),
            (1.0, -1.0, 0.0, 0.0, 0.0, 0.0, [0.0, 1440.0]),
            (-0.75, 0.5, 0.0, 0.0, 0.0, 0.0, [-195.0, 180.0]),
            (-1.0, -1.0, 0.0, 0.0, 0.0, 0.0, [0.0, 0.0]),
            (-1.2, -1.5, 0.0, 0.0, 0.0, 0.0, [65.0, -144.0]),
            (0.0, -1.0, 0.0, 0.0, 1.0, 1.0, [-240.0, 720.0]),
            (0.333, -0.37, -1.5, 0.8, 0.5, 0.75, [-541.0, 3023.0]),
            (-0.4, -1.0, 2.0, -0.5, -1.0, 1.5, [1080.0, 1296.0]),
            (0.25, -1.0, 0.0, 0.0, 0.003, 1.0, [0.0, 900.0]),
            (0.25, -1.0, 0.0, 0.0, -0.003, 1.0, [0.0, 900.0]),
            (-2.0, -3.0, 1.0, -2.0, 1.0, 1.0, [90.0, -3240.0]),
            (
                0.12345,
                -0.6789,
                0.4321,
                0.3456,
                0.2345,
                0.789,
                [-140.0, 1323.0],
            ),
        ] {
            request.visual.visual.draw_size = size;
            request.visual.visual.draw_size_back = back;
            request.visual.perspective.tilt = tilt;
            request.visual.mini_percent = mini * 100.0;
            request.visual.scroll.centered = centered;
            request.visual.accel.boomerang = boomerang;
            for zoom in [-0.5, 0.5, 1.0, 1.5] {
                let prepared = prepare_notefield(&request).expect("prepare native range");
                let range = NoteDrawRange::new(request.geometry, request.visual, mini, zoom);
                let valid = expected[0] <= expected[1];
                assert_eq!(
                    range.bounds(),
                    valid.then(|| {
                        let a = expected[0] * zoom;
                        let b = expected[1] * zoom;
                        [a.min(b), a.max(b)]
                    })
                );
                for travel in [
                    expected[0] - 0.01,
                    expected[0],
                    expected[1],
                    expected[1] + 0.01,
                ] {
                    let visible = expected[0] <= travel && travel <= expected[1];
                    assert_eq!(range.contains(travel * zoom), visible);
                    assert_eq!(prepared.draw_range.contains(travel), visible);
                }
            }
        }
    }

    #[test]
    fn draw_range_keeps_native_hold_direction_and_boomerang_peak() {
        let ns = noteskin();
        let timing = TimingData::default();
        let hides = SongLuaNoteHideWindows::default();
        let mut request = request(&ns, &timing, &[], &hides, FieldPlacement::P1, 0, 1, 2, 2);
        request.geometry.draw_distance_before_targets = 256.0;
        request.geometry.draw_distance_after_targets = 128.0;
        for zoom in [-0.5, 0.5, 1.0, 1.5] {
            request.geometry.field_zoom = zoom;
            let prepared = prepare_notefield(&request).expect("prepare hold range");
            // NoteDisplay::DrawHoldsInRange uses the directed original span,
            // and independently keeps a hold straddling the Boomerang peak.
            for (head, tail, head_peak, tail_peak, visible) in [
                (512.0, 768.0, true, true, false),
                (-192.0, -160.0, true, true, false),
                (-192.0, 320.0, true, true, true),
                (320.0, -192.0, false, false, false),
                (-192.0, -160.0, true, false, true),
                (256.0, 400.0, false, false, true),
                (-160.0, -128.0, true, true, true),
            ] {
                assert_eq!(
                    prepared.draw_range.hold_visible(
                        head * zoom,
                        tail * zoom,
                        head_peak,
                        tail_peak
                    ),
                    visible
                );
            }
        }
    }

    #[test]
    fn draw_size_culls_composed_taps_and_mines_before_lane_transforms() {
        use crate::{
            CapturedActorScratch, HoldMeshScratch, NotefieldCameraCache, NotefieldFieldFrameView,
            compose_notefield_field,
        };
        let mut ns = noteskin();
        ns.notes = (0..2 * deadsync_noteskin::NUM_QUANTIZATIONS)
            .map(|_| TestSlot::new("note"))
            .collect();
        ns.mine_layers = (0..2).map(|_| vec![TestSlot::new("note")].into()).collect();
        let timing = TimingData::default();
        let hides = SongLuaNoteHideWindows::default();
        let index = deadsync_gameplay::ChartNoteIndex::try_from_usize(0).expect("index");
        let lanes = [vec![index], vec![]];
        // Expected masks correspond to native travels -192,-128,-64,0,64,128,256,320.
        for (size, back, visible) in [
            (0.0, 0.0, [false, true, true, true, true, true, true, false]),
            (
                -0.5,
                0.0,
                [false, true, true, true, true, true, false, false],
            ),
            (1.0, 0.0, [false, true, true, true, true, true, true, true]),
            (
                0.0,
                -1.0,
                [false, false, false, true, true, true, true, false],
            ),
            (
                -1.0,
                -1.0,
                [false, false, false, true, false, false, false, false],
            ),
            (-1.5, -1.5, [false; 8]),
        ] {
            for note_type in [NoteType::Tap, NoteType::Mine] {
                for dir in [-1.0, 1.0] {
                    for zoom in [0.5, 1.0, 1.5] {
                        for (beat, visible) in [5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 12.0, 13.0]
                            .into_iter()
                            .zip(visible)
                        {
                            let mut n = note(0);
                            n.beat = beat;
                            n.note_type = note_type;
                            let notes = [n];
                            let rows = [deadsync_core::timing::beat_to_note_row(beat)];
                            let mut request = request(
                                &ns,
                                &timing,
                                &notes,
                                &hides,
                                FieldPlacement::P1,
                                0,
                                1,
                                2,
                                2,
                            );
                            request.geometry.field_zoom = zoom;
                            request.geometry.column_dirs = [dir; MAX_COLS];
                            request.geometry.draw_distance_before_targets = 256.0;
                            request.geometry.draw_distance_after_targets = 128.0;
                            request.chart.visible_beat = 8.0;
                            request.chart.search_beat = 8.0;
                            request.chart.lane_note_row_indices = &lanes;
                            request.chart.note_itg_rows = &rows;
                            request.visual.visual.draw_size = size;
                            request.visual.visual.draw_size_back = back;
                            request.visual.visual.move_y_cols[0] = 2.0;
                            request.visual.visual.tiny = 0.5;
                            request.visual.visual.tipsy = 0.75;
                            let prepared =
                                prepare_notefield(&request).expect("prepare DrawSize field");
                            let frame = NotefieldFieldFrameView {
                                feedback: spline_feedback(&[]),
                                completed_rows: Default::default(),
                            };
                            let mut draws = Vec::new();
                            compose_notefield_field(
                                &mut Vec::new(),
                                &mut draws,
                                &mut Vec::new(),
                                &mut ModelMeshCache::default(),
                                &mut HoldMeshScratch::default(),
                                &mut CapturedActorScratch::with_capacities(32, 0),
                                &mut NotefieldCameraCache::default(),
                                &request,
                                &prepared,
                                &frame,
                                &source,
                            );
                            let keys = sprite_keys(&draws);
                            assert_eq!(
                                keys.contains(&"note"),
                                visible,
                                "{note_type:?}, beat {beat}, size {size}, back {back}, zoom {zoom}, dir {dir}"
                            );
                            assert!(
                                keys.contains(&"target0"),
                                "draw range leaves receptors visible"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn draw_size_clips_composed_hold_and_roll_geometry() {
        use crate::{
            CapturedActorScratch, HoldMeshScratch, NotefieldCameraCache, NotefieldFieldFrameView,
            compose_notefield_field,
        };
        use deadlib_present::actors::FlatMeshVertices;
        use deadsync_rules::note::HoldData;
        let mut ns = noteskin();
        ns.notes = (0..2 * deadsync_noteskin::NUM_QUANTIZATIONS)
            .map(|_| TestSlot::new("head"))
            .collect();
        ns.hold_columns[0].head_inactive = Some(TestSlot::new("head"));
        ns.hold_columns[0].body_inactive = Some(TestSlot::new("body"));
        ns.hold_columns[0].topcap_inactive = Some(TestSlot::new("top"));
        ns.hold_columns[0].bottomcap_inactive = Some(TestSlot::new("bottom"));
        ns.roll_columns = ns.hold_columns.clone();
        let timing = TimingData::default();
        let hides = SongLuaNoteHideWindows::default();
        let index = deadsync_gameplay::ChartNoteIndex::try_from_usize(0).expect("index");
        let lanes = [vec![index], vec![]];
        for kind in [NoteType::Hold, NoteType::Roll] {
            for (head, tail) in [(5.0, 14.0), (6.5, 10.0), (5.0, 5.5)] {
                for dir in [-1.0, 1.0] {
                    for zoom in [0.5, 1.0, 1.5] {
                        for (size, back, low, high) in [
                            (0.0, 0.0, -128.0, 256.0),
                            (-0.5, -1.0, 0.0, 128.0),
                            (1.0, 1.0, -256.0, 512.0),
                        ] {
                            let mut n = note(0);
                            n.note_type = kind;
                            n.beat = head;
                            n.row_index = deadsync_core::timing::beat_to_note_row(head) as usize;
                            n.hold = Some(HoldData {
                                end_row_index: (tail * 48.0) as usize,
                                end_beat: tail,
                                result: None,
                                life: 1.0,
                                let_go_started_at: None,
                                let_go_starting_life: 1.0,
                                last_held_row_index: n.row_index,
                                last_held_beat: head,
                            });
                            let notes = [n];
                            let rows = [deadsync_core::timing::beat_to_note_row(head)];
                            let mut request = request(
                                &ns,
                                &timing,
                                &notes,
                                &hides,
                                FieldPlacement::P1,
                                0,
                                1,
                                2,
                                2,
                            );
                            request.geometry.field_zoom = zoom;
                            request.geometry.column_dirs = [dir; MAX_COLS];
                            request.geometry.draw_distance_before_targets = 256.0;
                            request.geometry.draw_distance_after_targets = 128.0;
                            request.chart.visible_beat = 8.0;
                            request.chart.search_beat = 8.0;
                            request.chart.lane_hold_indices = &lanes;
                            request.chart.note_itg_rows = &rows;
                            request.visual.visual.draw_size = size;
                            request.visual.visual.draw_size_back = back;
                            request.visual.visual.move_y_cols[0] = 0.5;
                            request.visual.visual.square_z = 0.25;
                            let prepared = prepare_notefield(&request).expect("prepare hold range");
                            let frame = NotefieldFieldFrameView {
                                feedback: spline_feedback(&[]),
                                completed_rows: Default::default(),
                            };
                            let mut draws = Vec::new();
                            compose_notefield_field(
                                &mut Vec::new(),
                                &mut draws,
                                &mut Vec::new(),
                                &mut ModelMeshCache::default(),
                                &mut HoldMeshScratch::default(),
                                &mut CapturedActorScratch::with_capacities(128, 0),
                                &mut NotefieldCameraCache::default(),
                                &request,
                                &prepared,
                                &frame,
                                &source,
                            );
                            let endpoints = [
                                prepared.field.column_receptor_ys[0] + dir * low * zoom + 32.0,
                                prepared.field.column_receptor_ys[0] + dir * high * zoom + 32.0,
                            ];
                            let bounds = [
                                endpoints[0].min(endpoints[1]),
                                endpoints[0].max(endpoints[1]),
                            ];
                            let mut vertices = 0;
                            for draw in &draws {
                                let FlatDraw::TexturedMesh(mesh) = draw else {
                                    continue;
                                };
                                let v = match &mesh.vertices {
                                    FlatMeshVertices::Shared(v) => v.as_ref(),
                                    FlatMeshVertices::Reusable(v) => v.as_slice(),
                                };
                                for vertex in v {
                                    assert!(
                                        vertex.pos[1] >= bounds[0] - 0.001
                                            && vertex.pos[1] <= bounds[1] + 0.001,
                                        "{kind:?} {head}..{tail} size {size} back {back} dir {dir} zoom {zoom}: {} outside {bounds:?}",
                                        vertex.pos[1]
                                    );
                                }
                                vertices += v.len();
                            }
                            assert_eq!(
                                vertices > 0,
                                tail > 6.0 || back == 1.0,
                                "hold range intersection"
                            );
                            assert!(sprite_keys(&draws).contains(&"target0"));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn lane_stealth_hides_only_its_notes_in_composed_field() {
        use crate::{
            CapturedActorScratch, HoldMeshScratch, NotefieldCameraCache, NotefieldFieldFrameView,
            compose_notefield_field,
        };
        let mut ns = noteskin();
        ns.notes = (0..2 * deadsync_noteskin::NUM_QUANTIZATIONS)
            .map(|index| {
                TestSlot::new(if index < deadsync_noteskin::NUM_QUANTIZATIONS {
                    "lane0"
                } else {
                    "lane1"
                })
            })
            .collect();
        let timing = TimingData::default();
        let hides = SongLuaNoteHideWindows::default();
        let lanes = [
            vec![deadsync_gameplay::ChartNoteIndex::try_from_usize(0).expect("index")],
            vec![deadsync_gameplay::ChartNoteIndex::try_from_usize(1).expect("index")],
        ];
        for beat in [0.5, 3.0] {
            let mut notes = [note(0), note(1)];
            for note in &mut notes {
                note.note_type = NoteType::Tap;
                note.beat = beat;
                note.row_index = usize::try_from(deadsync_core::timing::beat_to_note_row(beat))
                    .expect("positive fixture beat");
            }
            for past in [false, true] {
                let mut request =
                    request(&ns, &timing, &notes, &hides, FieldPlacement::P1, 0, 1, 2, 2);
                request.chart.lane_note_row_indices = &lanes;
                let rows = [deadsync_core::timing::beat_to_note_row(beat); 2];
                request.chart.note_itg_rows = &rows;
                request.visual.appearance.stealth = 0.25;
                request.visual.appearance.stealth_cols[0] = 0.75;
                request.visual.appearance.stealth_past_receptors = past;
                let prepared = prepare_notefield(&request).expect("prepared field");
                let frame = NotefieldFieldFrameView {
                    feedback: spline_feedback(&[]),
                    completed_rows: Default::default(),
                };
                let mut draws = Vec::new();
                compose_notefield_field(
                    &mut Vec::new(),
                    &mut draws,
                    &mut Vec::new(),
                    &mut ModelMeshCache::default(),
                    &mut HoldMeshScratch::default(),
                    &mut CapturedActorScratch::with_capacities(32, 0),
                    &mut NotefieldCameraCache::default(),
                    &request,
                    &prepared,
                    &frame,
                    &source,
                );
                let keys = sprite_keys(&draws);
                assert!(keys.contains(&"lane1"), "neighbor lane stays visible");
                assert_eq!(
                    keys.contains(&"lane0"),
                    beat < 1.0 && !past,
                    "beat {beat}, past {past}"
                );
                assert!(
                    keys.contains(&"target0"),
                    "Stealth leaves receptors visible"
                );
            }
        }
    }

    #[test]
    fn dizzy_holds_rotates_composed_heads_from_original_note_beat() {
        use crate::{
            CapturedActorScratch, HoldMeshScratch, NotefieldCameraCache, NotefieldFieldFrameView,
            compose_notefield_field,
        };
        use deadsync_rules::note::HoldData;
        let timing = TimingData::default();
        let hides = SongLuaNoteHideWindows::default();
        let lanes = [
            vec![deadsync_gameplay::ChartNoteIndex::try_from_usize(0).expect("index")],
            vec![],
        ];
        // Keep the render-state and noteskin-path matrix together: each variation
        // must preserve original-beat rotation while moving the held/let-go head.
        for placement in [FieldPlacement::P1, FieldPlacement::P2] {
            for kind in [NoteType::Hold, NoteType::Roll] {
                for state in 0..3 {
                    let beat = if state == 0 { 1.0 } else { 3.0 };
                    let native_rotation = if state == 0 { -57.2957764 } else { 57.2957764 };
                    let mut n = note(0);
                    n.note_type = kind;
                    n.beat = 2.0;
                    n.row_index = 96;
                    n.hold = Some(HoldData {
                        end_row_index: 240,
                        end_beat: 5.0,
                        result: None,
                        life: 1.0,
                        let_go_started_at: (state == 2).then_some(0),
                        let_go_starting_life: 1.0,
                        last_held_row_index: if state == 2 { 144 } else { 96 },
                        last_held_beat: if state == 2 { 3.0 } else { 2.0 },
                    });
                    let notes = [n];
                    let mut held = active_hold(0);
                    held.note_type = kind;
                    for direction in [-1.0, 1.0] {
                        for path in 0..3 {
                            let mut ns = noteskin();
                            ns.note_display_metrics.flip_head_and_tail_when_reverse = true;
                            ns.notes = (0..2 * deadsync_noteskin::NUM_QUANTIZATIONS)
                                .map(|_| TestSlot::new("head"))
                                .collect();
                            ns.hold_columns[0].body_inactive = Some(TestSlot::new("body"));
                            ns.hold_columns[0].topcap_inactive = Some(TestSlot::new("top"));
                            ns.hold_columns[0].bottomcap_inactive = Some(TestSlot::new("bottom"));
                            if path == 0 {
                                ns.hold_columns[0].head_inactive = Some(TestSlot::new("head"));
                                ns.hold_columns[0].head_active = Some(TestSlot::new("head"));
                            } else if path == 1 {
                                ns.hold_columns[0].head_inactive_layers =
                                    Some(Arc::from([TestSlot::new("head")]));
                                ns.hold_columns[0].head_active_layers =
                                    Some(Arc::from([TestSlot::new("head")]));
                            }
                            ns.roll_columns = ns.hold_columns.clone();
                            let mut body_before = Vec::new();
                            for enabled in [false, true] {
                                let mut request =
                                    request(&ns, &timing, &notes, &hides, placement, 0, 1, 2, 2);
                                request.geometry.column_dirs.fill(direction);
                                request.chart.lane_note_row_indices = &lanes;
                                request.chart.lane_hold_indices = &lanes;
                                request.chart.note_itg_rows = &[96];
                                request.chart.visible_beat = beat;
                                request.chart.search_beat = beat;
                                request.visual.current_display_beat = beat;
                                request.visual.visual.dizzy = 1.0;
                                request.visual.visual.dizzy_holds = enabled;
                                let prepared = prepare_notefield(&request).expect("prepared field");
                                let mut frame = NotefieldFieldFrameView {
                                    feedback: spline_feedback(&[]),
                                    completed_rows: Default::default(),
                                };
                                if state == 1 {
                                    frame.feedback.lanes[0].active_hold = Some(&held);
                                }
                                let mut draws = Vec::new();
                                let mut scratch = HoldMeshScratch::with_columns(2);
                                compose_notefield_field(
                                    &mut Vec::new(),
                                    &mut draws,
                                    &mut Vec::new(),
                                    &mut ModelMeshCache::default(),
                                    &mut scratch,
                                    &mut CapturedActorScratch::with_capacities(32, 0),
                                    &mut NotefieldCameraCache::default(),
                                    &request,
                                    &prepared,
                                    &frame,
                                    &source,
                                );
                                let mut head_count = 0;
                                let mut body = Vec::new();
                                for draw in &draws {
                                    match draw {
                                        FlatDraw::Sprite(sprite)
                                            if sprite.source.texture_key() == Some("head") =>
                                        {
                                            head_count += 1;
                                            let expected =
                                                if enabled { native_rotation } else { 0.0 };
                                            assert!(
                                                (sprite.rot_z_deg - expected).abs() < 0.0001,
                                                "{placement:?}, {kind:?}, state {state}, path {path}, {enabled}: {}",
                                                sprite.rot_z_deg
                                            );
                                            if state == 1 {
                                                let target = sprite_positions(&draws)
                                                    .into_iter()
                                                    .find(|(key, _)| *key == "target0")
                                                    .expect("receptor")
                                                    .1;
                                                assert_eq!(
                                                    sprite.center, target,
                                                    "held head stays at receptor while rotating"
                                                );
                                            }
                                        }
                                        FlatDraw::Sprite(sprite)
                                            if matches!(
                                                sprite.source.texture_key(),
                                                Some("body" | "top" | "bottom")
                                            ) =>
                                        {
                                            let geometry = sprite
                                                .center
                                                .into_iter()
                                                .chain(sprite.size)
                                                .chain([
                                                    sprite.world_z,
                                                    sprite.rot_x_deg,
                                                    sprite.rot_y_deg,
                                                    sprite.rot_z_deg,
                                                ])
                                                .chain(sprite.tint)
                                                .map(f32::to_bits)
                                                .collect::<Vec<_>>();
                                            body.push((
                                                sprite
                                                    .source
                                                    .texture_key()
                                                    .expect("body texture")
                                                    .to_owned(),
                                                geometry,
                                            ));
                                        }
                                        FlatDraw::TexturedMesh(mesh)
                                            if matches!(
                                                mesh.texture.as_ref(),
                                                "body" | "top" | "bottom"
                                            ) =>
                                        {
                                            let vertices = match &mesh.vertices {
                                                deadlib_present::actors::FlatMeshVertices::Shared(v) => v.as_ref(),
                                                deadlib_present::actors::FlatMeshVertices::Reusable(v) => v.as_slice(),
                                            };
                                            let geometry = mesh
                                                .offset
                                                .into_iter()
                                                .chain([mesh.world_z])
                                                .chain(mesh.tint)
                                                .chain(vertices.iter().flat_map(|v| {
                                                    v.pos.into_iter().chain(v.uv).chain(v.color)
                                                }))
                                                .map(f32::to_bits)
                                                .collect::<Vec<_>>();
                                            body.push((mesh.texture.to_string(), geometry));
                                        }
                                        _ => {}
                                    }
                                }
                                assert_eq!(head_count, 1);
                                assert!(!body.is_empty());
                                if enabled {
                                    assert_eq!(
                                        body, body_before,
                                        "DizzyHolds does not twist the body or its strip caps"
                                    );
                                } else {
                                    body_before = body;
                                }
                                assert_eq!(scratch.stats().buffer_grows, 0);
                                assert_eq!(scratch.stats().capacity_grows, 0);
                                assert_eq!(scratch.stats().saturated_pairs, 0);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn drunk_variants_move_composed_notes_and_hold_meshes() {
        use crate::{
            CapturedActorScratch, HoldMeshScratch, NotefieldCameraCache, NotefieldFieldFrameView,
            compose_notefield_field,
        };
        use deadlib_present::actors::FlatMeshVertices;
        use deadsync_rules::note::HoldData;
        let mut ns = noteskin();
        ns.notes = (0..2 * deadsync_noteskin::NUM_QUANTIZATIONS)
            .map(|i| TestSlot::new(format!("note{}", i / deadsync_noteskin::NUM_QUANTIZATIONS)))
            .collect();
        ns.mine_layers = (0..2)
            .map(|i| vec![TestSlot::new(format!("note{i}"))].into())
            .collect();
        for col in 0..2 {
            ns.hold_columns[col].head_inactive = Some(TestSlot::new(format!("note{col}")));
            ns.hold_columns[col].body_inactive = Some(TestSlot::new(format!("body{col}")));
            ns.hold_columns[col].topcap_inactive = Some(TestSlot::new(format!("top{col}")));
            ns.hold_columns[col].bottomcap_inactive = Some(TestSlot::new(format!("bottom{col}")));
        }
        ns.roll_columns = ns.hold_columns.clone();
        let timing = TimingData::default();
        let hides = SongLuaNoteHideWindows::default();
        let lanes = [
            vec![deadsync_gameplay::ChartNoteIndex::try_from_usize(0).expect("index")],
            vec![deadsync_gameplay::ChartNoteIndex::try_from_usize(1).expect("index")],
        ];
        for placement in [FieldPlacement::P1, FieldPlacement::P2] {
            for kind in [
                NoteType::Tap,
                NoteType::Mine,
                NoteType::Hold,
                NoteType::Roll,
            ] {
                let notes: Vec<_> = (0..2)
                    .map(|col| {
                        let mut n = note(col);
                        n.note_type = kind;
                        n.beat = 2.0 + col as f32;
                        n.row_index = 96 + col * 48;
                        if matches!(kind, NoteType::Hold | NoteType::Roll) {
                            n.hold = Some(HoldData {
                                end_row_index: n.row_index + 48,
                                end_beat: n.beat + 1.0,
                                result: None,
                                life: 1.0,
                                let_go_started_at: None,
                                let_go_starting_life: 1.0,
                                last_held_row_index: n.row_index,
                                last_held_beat: n.beat,
                            });
                        }
                        n
                    })
                    .collect();
                for direction in [-1.0, 1.0] {
                    for amount in [-0.75, 0.75] {
                        for cosecant in [false, true] {
                            let mut request =
                                request(&ns, &timing, &notes, &hides, placement, 0, 1, 2, 2);
                            request.geometry.column_dirs.fill(direction);
                            request.chart.lane_note_row_indices = &lanes;
                            request.chart.lane_hold_indices = &lanes;
                            request.chart.note_itg_rows = &[96, 144];
                            request.arrow_effect_time_s = 1.25;
                            request.visual.visual = VisualEffects {
                                tan_drunk: amount,
                                tan_drunk_offset: 0.75,
                                tan_drunk_speed: -0.5,
                                tan_drunk_period: -0.25,
                                drunk_z: amount,
                                drunk_z_offset: -0.5,
                                drunk_z_speed: 0.5,
                                drunk_z_period: 0.25,
                                tan_drunk_z: -amount / 2.0,
                                tan_drunk_z_offset: 0.25,
                                tan_drunk_z_speed: -0.5,
                                tan_drunk_z_period: -0.5,
                                cosecant,
                                tiny: 1.0,
                                tipsy: 0.25,
                                move_y_cols: [0.5; MAX_COLS],
                                ..VisualEffects::default()
                            };
                            request.visual.accel.wave = 0.5;
                            let prepared = prepare_notefield(&request).expect("prepared field");
                            let frame = NotefieldFieldFrameView {
                                feedback: spline_feedback(&[]),
                                completed_rows: Default::default(),
                            };
                            let mut draws = Vec::new();
                            let mut scratch = HoldMeshScratch::with_columns(2);
                            compose_notefield_field(
                                &mut Vec::new(),
                                &mut draws,
                                &mut Vec::new(),
                                &mut ModelMeshCache::default(),
                                &mut scratch,
                                &mut CapturedActorScratch::with_capacities(32, 0),
                                &mut NotefieldCameraCache::default(),
                                &request,
                                &prepared,
                                &frame,
                                &source,
                            );
                            let positions = sprite_positions(&draws);
                            let mut meshes = [0; 2];
                            for col in 0..2 {
                                let receptor = positions
                                    .iter()
                                    .find(|(key, _)| *key == format!("target{col}"))
                                    .expect("receptor")
                                    .1;
                                let wave = |angle: f32| {
                                    if cosecant {
                                        1.0 / angle.sin()
                                    } else {
                                        angle.tan()
                                    }
                                };
                                let x_at = |travel: f32| {
                                    amount
                                        * (wave(0.625 + col as f32 * 0.35 + travel * 7.5 / 480.0)
                                            * 32.0)
                                };
                                let z_at = |travel: f32| {
                                    amount
                                        * ((1.875 + col as f32 * 0.1 + travel * 12.5 / 480.0).cos()
                                            * 32.0)
                                        - amount / 2.0
                                            * (wave(
                                                0.625 + col as f32 * 0.25 + travel * 5.0 / 480.0,
                                            ) * 32.0)
                                };
                                let x_at_y = |y: f32| {
                                    receptor[0]
                                        + (x_at((y - receptor[1]) / direction) - x_at(0.0)) * 0.5
                                };
                                let expected_row_x = prepared.field.playfield_center_x
                                    + prepared.column_x_offsets[col]
                                    + (prepared
                                        .notes
                                        .as_ref()
                                        .expect("prepared notes")
                                        .col_offsets[col]
                                        + x_at(0.0))
                                        * 0.5;
                                assert!((receptor[0] - expected_row_x).abs() < 0.0001);
                                let arrow = positions
                                    .iter()
                                    .find(|(key, _)| *key == format!("note{col}"))
                                    .expect("arrow")
                                    .1;
                                assert!((arrow[0] - x_at_y(arrow[1])).abs() < 0.001);
                                for draw in &draws {
                                    if let FlatDraw::Sprite(sprite) = draw {
                                        if let SpriteSource::TextureHandle { key, .. } =
                                            &sprite.source
                                        {
                                            if [format!("target{col}"), format!("note{col}")]
                                                .contains(&key.to_string())
                                            {
                                                let expected_z = z_at(
                                                    (sprite.center[1] - receptor[1]) / direction,
                                                );
                                                assert!(
                                                    (sprite.world_z - expected_z).abs() < 0.001,
                                                    "{placement:?}, {kind:?}, col {col}"
                                                );
                                            }
                                        }
                                    }
                                    let FlatDraw::TexturedMesh(mesh) = draw else {
                                        continue;
                                    };
                                    if ![
                                        format!("body{col}"),
                                        format!("top{col}"),
                                        format!("bottom{col}"),
                                    ]
                                    .contains(&mesh.texture.to_string())
                                    {
                                        continue;
                                    }
                                    meshes[col] += 1;
                                    assert!(
                                        !mesh.depth_test,
                                        "DrunkZ alone does not request native depth testing"
                                    );
                                    let vertices = match &mesh.vertices {
                                        FlatMeshVertices::Shared(v) => v.as_ref(),
                                        FlatMeshVertices::Reusable(v) => v.as_slice(),
                                    };
                                    for quad in vertices.chunks_exact(6) {
                                        for (left, right) in
                                            [(&quad[0], &quad[1]), (&quad[5], &quad[4])]
                                        {
                                            let x = (left.pos[0] + right.pos[0]) * 0.5;
                                            let expected_x = x_at_y(left.pos[1]);
                                            assert!(
                                                (x - expected_x).abs()
                                                    < 0.002 * expected_x.abs().max(1.0)
                                            );
                                            let expected_z =
                                                z_at((left.pos[1] - receptor[1]) / direction);
                                            assert!(
                                                (left.pos[2] - expected_z).abs()
                                                    < 0.001 * expected_z.abs().max(1.0)
                                            );
                                            assert_eq!(right.pos[2], left.pos[2]);
                                        }
                                    }
                                }
                            }
                            if matches!(kind, NoteType::Hold | NoteType::Roll) {
                                assert!(meshes.into_iter().all(|n| n >= 3), "body and both caps");
                            }
                            assert_eq!(scratch.stats().capacity_grows, 0);
                            assert_eq!(scratch.stats().saturated_pairs, 0);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn drunk_z_moves_each_receptor_and_its_live_explosions() {
        let ns = noteskin();
        let timing = TimingData::default();
        let notes = [note(0), note(1)];
        let hides = SongLuaNoteHideWindows::default();
        let holds = [active_hold(0), active_hold(1)];
        let taps = [tap(), tap()];
        let mines = [mine(), mine()];
        let mut feedback = spline_feedback(&taps);
        feedback.lanes[0].active_hold = Some(&holds[0]);
        feedback.lanes[1].active_hold = Some(&holds[1]);
        feedback.mine_explosions = Some(&mines);
        let mut request = request(&ns, &timing, &notes, &hides, FieldPlacement::P1, 0, 1, 2, 2);
        request.arrow_effect_time_s = 1.25;
        request.visual.visual.drunk_z = 0.75;
        request.visual.visual.drunk_z_offset = -0.5;
        request.visual.visual.drunk_z_speed = 0.5;
        let prepared = prepare_notefield(&request).expect("prepared field");
        let mut draws = Vec::new();
        compose_notefield_feedback(
            &mut draws,
            &mut Vec::new(),
            &mut ModelMeshCache::default(),
            &request,
            &prepared,
            &feedback,
            &source,
        );
        let mut checked = [0; 2];
        let mut mine_depths = Vec::new();
        for draw in draws {
            let FlatDraw::Sprite(sprite) = draw else {
                continue;
            };
            let SpriteSource::TextureHandle { key, .. } = &sprite.source else {
                continue;
            };
            if key.as_ref() == "mine" {
                mine_depths.push(sprite.world_z);
            }
            for col in 0..2 {
                if [
                    format!("target{col}"),
                    format!("hold{col}"),
                    format!("tap{col}"),
                ]
                .contains(&key.to_string())
                {
                    assert!(
                        (sprite.world_z - 24.0 * (1.875 + col as f32 * 0.1).cos()).abs() < 0.00002
                    );
                    checked[col] += 1;
                }
            }
        }
        assert_eq!(checked, [3, 3]);
        assert_eq!(mine_depths.len(), 2);
        mine_depths.sort_by(f32::total_cmp);
        let mut expected = [24.0 * 1.875f32.cos(), 24.0 * 1.975f32.cos()];
        expected.sort_by(f32::total_cmp);
        for (actual, expected) in mine_depths.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 0.00002);
        }
    }

    #[test]
    fn travel_mods_move_composed_notes_receptors_and_hold_meshes() {
        use crate::{
            CapturedActorScratch, HoldMeshScratch, NotefieldCameraCache, NotefieldFieldFrameView,
            compose_notefield_field,
        };
        use deadlib_present::actors::FlatMeshVertices;
        use deadsync_rules::note::HoldData;
        let mut ns = noteskin();
        ns.notes = (0..2 * deadsync_noteskin::NUM_QUANTIZATIONS)
            .map(|i| TestSlot::new(format!("note{}", i / deadsync_noteskin::NUM_QUANTIZATIONS)))
            .collect();
        ns.mine_layers = (0..2)
            .map(|i| vec![TestSlot::new(format!("note{i}"))].into())
            .collect();
        for col in 0..2 {
            ns.hold_columns[col].head_inactive = Some(TestSlot::new(format!("note{col}")));
            ns.hold_columns[col].body_inactive = Some(TestSlot::new(format!("body{col}")));
            ns.hold_columns[col].topcap_inactive = Some(TestSlot::new(format!("top{col}")));
            ns.hold_columns[col].bottomcap_inactive = Some(TestSlot::new(format!("bottom{col}")));
        }
        ns.roll_columns = ns.hold_columns.clone();
        let timing = TimingData::default();
        let hides = SongLuaNoteHideWindows::default();
        // P2 can be the only field and still uses the native P2 direction.
        for ((placement, player_idx, players, double, signs), square) in [
            (FieldPlacement::P1, 0, 1, false, [1.0, 1.0]),
            (FieldPlacement::P2, 0, 1, false, [-1.0, -1.0]),
            (FieldPlacement::P1, 0, 2, false, [1.0, 1.0]),
            (FieldPlacement::P2, 1, 2, false, [-1.0, -1.0]),
            (FieldPlacement::P1, 0, 1, true, [1.0, -1.0]),
            (FieldPlacement::P2, 0, 1, true, [1.0, -1.0]),
        ]
        .into_iter()
        .flat_map(|case| [(case, false), (case, true)])
        {
            let col_start = player_idx * 2;
            let mut lanes = vec![vec![]; players * 2];
            for col in 0..2 {
                lanes[col_start + col]
                    .push(deadsync_gameplay::ChartNoteIndex::try_from_usize(col).expect("index"));
            }
            for kind in [
                NoteType::Tap,
                NoteType::Mine,
                NoteType::Hold,
                NoteType::Roll,
            ] {
                let notes: Vec<_> = (0..2)
                    .map(|col| {
                        let mut n = note(col_start + col);
                        n.note_type = kind;
                        n.beat = 2.0 + col as f32;
                        n.row_index = 96 + col * 48;
                        if matches!(kind, NoteType::Hold | NoteType::Roll) {
                            n.hold = Some(HoldData {
                                end_row_index: n.row_index + 48,
                                end_beat: n.beat + 1.0,
                                result: None,
                                life: 1.0,
                                let_go_started_at: None,
                                let_go_starting_life: 1.0,
                                last_held_row_index: n.row_index,
                                last_held_beat: n.beat,
                            });
                        }
                        n
                    })
                    .collect();
                for direction in [-1.0, 1.0] {
                    for amount in [-2.5, 2.5] {
                        let mut request = request(
                            &ns,
                            &timing,
                            &notes,
                            &hides,
                            placement,
                            player_idx,
                            players,
                            2,
                            players * 2,
                        );
                        request.geometry.double_style = double;
                        request.geometry.single_style = !double;
                        request.geometry.column_dirs.fill(direction);
                        request.chart.lane_note_row_indices = &lanes;
                        request.chart.lane_hold_indices = &lanes;
                        request.chart.note_itg_rows = &[96, 144];
                        if square {
                            request.visual.visual.square = amount;
                            request.visual.visual.square_period = 0.5;
                            request.visual.visual.square_z = -amount;
                            request.visual.visual.square_z_offset = 32.0;
                            request.visual.visual.square_z_period = 1.0;
                        } else {
                            request.visual.visual.xmode = amount;
                        }
                        request.visual.visual.tiny = 1.0;
                        request.visual.visual.tipsy = 0.25;
                        request.visual.visual.move_y_cols = [0.5; MAX_COLS];
                        request.visual.accel.wave = 0.5;
                        let prepared = prepare_notefield(&request).expect("prepared field");
                        let frame = NotefieldFieldFrameView {
                            feedback: spline_feedback(&[]),
                            completed_rows: Default::default(),
                        };
                        let mut draws = Vec::new();
                        let mut scratch = HoldMeshScratch::with_columns(2);
                        compose_notefield_field(
                            &mut Vec::new(),
                            &mut draws,
                            &mut Vec::new(),
                            &mut ModelMeshCache::default(),
                            &mut scratch,
                            &mut CapturedActorScratch::with_capacities(32, 0),
                            &mut NotefieldCameraCache::default(),
                            &request,
                            &prepared,
                            &frame,
                            &source,
                        );
                        let positions = sprite_positions(&draws);
                        let mut meshes = [0; 2];
                        for col in 0..2 {
                            let receptor = positions
                                .iter()
                                .find(|(key, _)| *key == format!("target{col}"))
                                .expect("receptor")
                                .1;
                            let arrow = positions
                                .iter()
                                .find(|(key, _)| *key == format!("note{col}"))
                                .expect("arrow")
                                .1;
                            let native_x = |y: f32| {
                                let travel = (y - receptor[1]) / direction;
                                let extra = if square {
                                    crate::transforms::square_wave_offset(travel, amount, 0.0, 0.5)
                                        - crate::transforms::square_wave_offset(
                                            0.0, amount, 0.0, 0.5,
                                        )
                                } else {
                                    amount * signs[col] * travel
                                };
                                receptor[0] + extra * 0.5
                            };
                            assert!(
                                (arrow[0] - native_x(arrow[1])).abs() < 0.0001,
                                "{placement:?}, {kind:?}, direction {direction}, amount {amount}, col {col}"
                            );
                            let native_z = |y: f32| {
                                if square {
                                    crate::transforms::square_wave_offset(
                                        (y - receptor[1]) / direction,
                                        -amount,
                                        32.0,
                                        1.0,
                                    )
                                } else {
                                    0.0
                                }
                            };
                            let row_extra = if square { -amount * 32.0 } else { 0.0 };
                            let expected_receptor_x = prepared.field.playfield_center_x
                                + prepared.column_x_offsets[col]
                                + (prepared.notes.as_ref().expect("prepared notes").col_offsets
                                    [col]
                                    + row_extra)
                                    * 0.5;
                            assert!((receptor[0] - expected_receptor_x).abs() < 0.0001);
                            for draw in &draws {
                                if let FlatDraw::Sprite(sprite) = draw {
                                    if let SpriteSource::TextureHandle { key, .. } = &sprite.source
                                    {
                                        if [format!("target{col}"), format!("note{col}")]
                                            .contains(&key.to_string())
                                        {
                                            assert_eq!(sprite.world_z, native_z(sprite.center[1]));
                                        }
                                    }
                                }
                                let FlatDraw::TexturedMesh(mesh) = draw else {
                                    continue;
                                };
                                if ![
                                    format!("body{col}"),
                                    format!("top{col}"),
                                    format!("bottom{col}"),
                                ]
                                .contains(&mesh.texture.to_string())
                                {
                                    continue;
                                }
                                meshes[col] += 1;
                                assert_eq!(mesh.depth_test, square);
                                let vertices = match &mesh.vertices {
                                    FlatMeshVertices::Shared(v) => v.as_ref(),
                                    FlatMeshVertices::Reusable(v) => v.as_slice(),
                                };
                                for quad in vertices.chunks_exact(6) {
                                    for (left, right) in
                                        [(&quad[0], &quad[1]), (&quad[5], &quad[4])]
                                    {
                                        let center = (left.pos[0] + right.pos[0]) * 0.5;
                                        assert!((center - native_x(left.pos[1])).abs() < 0.0002);
                                        assert_eq!(left.pos[2], native_z(left.pos[1]));
                                        assert_eq!(right.pos[2], left.pos[2]);
                                    }
                                }
                            }
                        }
                        if matches!(kind, NoteType::Hold | NoteType::Roll) {
                            assert!(
                                meshes.into_iter().all(|count| count >= 3),
                                "body and both caps must follow travel modifiers"
                            );
                        }
                        assert_eq!(scratch.stats().capacity_grows, 0);
                        assert_eq!(scratch.stats().saturated_pairs, 0);
                    }
                }
            }
        }
    }

    #[test]
    fn parabola_bends_composed_taps_and_mines_before_reverse_and_move_y() {
        use crate::{
            CapturedActorScratch, HoldMeshScratch, NotefieldCameraCache, NotefieldFieldFrameView,
            compose_notefield_field,
        };
        let mut ns = noteskin();
        ns.notes = (0..2 * deadsync_noteskin::NUM_QUANTIZATIONS)
            .map(|i| {
                TestSlot::new(if i < deadsync_noteskin::NUM_QUANTIZATIONS {
                    "note0"
                } else {
                    "note1"
                })
            })
            .collect();
        ns.mine_layers = vec![
            vec![TestSlot::new("note0")].into(),
            vec![TestSlot::new("note1")].into(),
        ];
        let timing = TimingData::default();
        let hides = SongLuaNoteHideWindows::default();
        let lanes = [
            vec![deadsync_gameplay::ChartNoteIndex::try_from_usize(0).expect("index")],
            vec![deadsync_gameplay::ChartNoteIndex::try_from_usize(1).expect("index")],
        ];
        for kind in [NoteType::Tap, NoteType::Mine] {
            let mut notes = [note(0), note(1)];
            for (index, note) in notes.iter_mut().enumerate() {
                note.note_type = kind;
                note.beat = 2.0 + index as f32;
                note.row_index =
                    usize::try_from(deadsync_core::timing::beat_to_note_row(note.beat))
                        .expect("positive fixture beat");
            }
            for direction in [-1.0, 1.0] {
                for amount in [-2.5, 2.5] {
                    let mut request =
                        request(&ns, &timing, &notes, &hides, FieldPlacement::P1, 0, 1, 2, 2);
                    request.chart.lane_note_row_indices = &lanes;
                    request.chart.note_itg_rows = &[96, 144];
                    request.geometry.column_dirs.fill(direction);
                    request.visual.visual.parabola_x = amount;
                    request.visual.visual.parabola_z = -amount;
                    request.visual.visual.tiny = 1.0;
                    request.visual.visual.move_y_cols = [0.5; MAX_COLS];
                    request.visual.accel.wave = 0.5;
                    let prepared = prepare_notefield(&request).expect("prepared field");
                    let frame = NotefieldFieldFrameView {
                        feedback: spline_feedback(&[]),
                        completed_rows: Default::default(),
                    };
                    let mut draws = Vec::new();
                    compose_notefield_field(
                        &mut Vec::new(),
                        &mut draws,
                        &mut Vec::new(),
                        &mut ModelMeshCache::default(),
                        &mut HoldMeshScratch::default(),
                        &mut CapturedActorScratch::with_capacities(32, 0),
                        &mut NotefieldCameraCache::default(),
                        &request,
                        &prepared,
                        &frame,
                        &source,
                    );
                    let mut arrows = [None; 2];
                    let mut receptors = [None; 2];
                    for draw in &draws {
                        let FlatDraw::Sprite(sprite) = draw else {
                            continue;
                        };
                        let SpriteSource::TextureHandle { key, .. } = &sprite.source else {
                            continue;
                        };
                        for col in 0..2 {
                            if key.as_ref() == ["note0", "note1"][col] {
                                arrows[col] = Some((sprite.center, sprite.world_z));
                            } else if key.as_ref() == ["target0", "target1"][col] {
                                receptors[col] = Some(sprite.center);
                                assert_eq!(sprite.world_z, 0.0);
                            }
                        }
                    }
                    for col in 0..2 {
                        let (arrow, z) = arrows[col].expect("visible arrow");
                        let receptor = receptors[col].expect("visible receptor");
                        let travel = (arrow[1] - receptor[1]) / direction;
                        let curve = amount * (travel / 64.0) * (travel / 64.0);
                        assert!((arrow[0] - receptor[0] - curve * 0.5).abs() < 0.0001);
                        assert!((z + curve).abs() < 0.0001);
                        assert!(curve.abs() > 0.5);
                    }
                }
            }
        }
    }

    #[test]
    fn twirl_rotates_composed_taps_and_mines_without_rotating_receptors() {
        use crate::{
            CapturedActorScratch, HoldMeshScratch, NotefieldCameraCache, NotefieldFieldFrameView,
            compose_notefield_field,
        };
        let mut ns = noteskin();
        ns.notes = (0..2 * deadsync_noteskin::NUM_QUANTIZATIONS)
            .map(|index| {
                TestSlot::new(if index < deadsync_noteskin::NUM_QUANTIZATIONS {
                    "note0"
                } else {
                    "note1"
                })
            })
            .collect();
        ns.mine_layers = vec![
            vec![TestSlot::new("note0")].into(),
            vec![TestSlot::new("note1")].into(),
        ];
        let timing = TimingData::default();
        let hides = SongLuaNoteHideWindows::default();
        let lanes = [
            vec![deadsync_gameplay::ChartNoteIndex::try_from_usize(0).expect("index")],
            vec![deadsync_gameplay::ChartNoteIndex::try_from_usize(1).expect("index")],
        ];
        for kind in [NoteType::Tap, NoteType::Mine] {
            let mut notes = [note(0), note(1)];
            for (index, note) in notes.iter_mut().enumerate() {
                note.note_type = kind;
                note.beat = 2.0 + index as f32;
                note.row_index =
                    usize::try_from(deadsync_core::timing::beat_to_note_row(note.beat))
                        .expect("positive fixture beat");
            }
            let rows = [96, 144];
            for direction in [-1.0, 1.0] {
                for twirl in [-2.5, 2.5] {
                    let mut request =
                        request(&ns, &timing, &notes, &hides, FieldPlacement::P1, 0, 1, 2, 2);
                    request.chart.lane_note_row_indices = &lanes;
                    request.chart.note_itg_rows = &rows;
                    request.geometry.column_dirs.fill(direction);
                    request.visual.visual.twirl = twirl;
                    request.visual.visual.move_y_cols = [0.5; MAX_COLS];
                    let prepared = prepare_notefield(&request).expect("prepared field");
                    let frame = NotefieldFieldFrameView {
                        feedback: spline_feedback(&[]),
                        completed_rows: Default::default(),
                    };
                    let mut draws = Vec::new();
                    compose_notefield_field(
                        &mut Vec::new(),
                        &mut draws,
                        &mut Vec::new(),
                        &mut ModelMeshCache::default(),
                        &mut HoldMeshScratch::default(),
                        &mut CapturedActorScratch::with_capacities(32, 0),
                        &mut NotefieldCameraCache::default(),
                        &request,
                        &prepared,
                        &frame,
                        &source,
                    );
                    let mut checked = [false; 2];
                    for draw in &draws {
                        let FlatDraw::Sprite(sprite) = draw else {
                            continue;
                        };
                        let SpriteSource::TextureHandle { key, .. } = &sprite.source else {
                            continue;
                        };
                        if let Some(col) = ["note0", "note1"]
                            .iter()
                            .position(|name| *name == key.as_ref())
                        {
                            let y_offset =
                                (sprite.center[1] - prepared.field.column_receptor_ys[col] - 32.0)
                                    / prepared.field.column_dirs[col];
                            assert!((sprite.rot_y_deg - twirl * y_offset / 2.0).abs() < 0.0001);
                            assert!(sprite.rot_y_deg.abs() > 1.0);
                            checked[col] = true;
                        } else if key.starts_with("target") {
                            assert_eq!(sprite.rot_y_deg, 0.0);
                        }
                    }
                    assert_eq!(
                        checked, [true; 2],
                        "{kind:?}, direction {direction}, Twirl {twirl}"
                    );
                }
            }
        }
    }
    #[test]
    fn roll_rotates_composed_taps_and_mines_without_rotating_receptors() {
        use crate::{
            CapturedActorScratch, HoldMeshScratch, NotefieldCameraCache, NotefieldFieldFrameView,
            compose_notefield_field,
        };
        let mut ns = noteskin();
        ns.notes = (0..2 * deadsync_noteskin::NUM_QUANTIZATIONS)
            .map(|index| {
                TestSlot::new(if index < deadsync_noteskin::NUM_QUANTIZATIONS {
                    "note0"
                } else {
                    "note1"
                })
            })
            .collect();
        ns.mine_layers = vec![
            vec![TestSlot::new("note0")].into(),
            vec![TestSlot::new("note1")].into(),
        ];
        let timing = TimingData::default();
        let hides = SongLuaNoteHideWindows::default();
        let lanes = [
            vec![deadsync_gameplay::ChartNoteIndex::try_from_usize(0).expect("index")],
            vec![deadsync_gameplay::ChartNoteIndex::try_from_usize(1).expect("index")],
        ];
        for kind in [NoteType::Tap, NoteType::Mine] {
            let mut notes = [note(0), note(1)];
            for (index, note) in notes.iter_mut().enumerate() {
                note.note_type = kind;
                note.beat = 2.0 + index as f32;
                note.row_index =
                    usize::try_from(deadsync_core::timing::beat_to_note_row(note.beat))
                        .expect("positive fixture beat");
            }
            let rows = [96, 144];
            for direction in [-1.0, 1.0] {
                for roll in [-2.5, 2.5] {
                    let mut request =
                        request(&ns, &timing, &notes, &hides, FieldPlacement::P1, 0, 1, 2, 2);
                    request.chart.lane_note_row_indices = &lanes;
                    request.chart.note_itg_rows = &rows;
                    request.geometry.column_dirs.fill(direction);
                    request.visual.visual.roll = roll;
                    request.visual.accel.wave = 0.5;
                    request.visual.visual.move_y_cols = [0.5; MAX_COLS];
                    let prepared = prepare_notefield(&request).expect("prepared field");
                    let frame = NotefieldFieldFrameView {
                        feedback: spline_feedback(&[]),
                        completed_rows: Default::default(),
                    };
                    let mut draws = Vec::new();
                    compose_notefield_field(
                        &mut Vec::new(),
                        &mut draws,
                        &mut Vec::new(),
                        &mut ModelMeshCache::default(),
                        &mut HoldMeshScratch::default(),
                        &mut CapturedActorScratch::with_capacities(32, 0),
                        &mut NotefieldCameraCache::default(),
                        &request,
                        &prepared,
                        &frame,
                        &source,
                    );
                    let mut checked = [false; 2];
                    for draw in &draws {
                        let FlatDraw::Sprite(sprite) = draw else {
                            continue;
                        };
                        let SpriteSource::TextureHandle { key, .. } = &sprite.source else {
                            continue;
                        };
                        if let Some(col) = ["note0", "note1"]
                            .iter()
                            .position(|name| *name == key.as_ref())
                        {
                            let y_offset =
                                (sprite.center[1] - prepared.field.column_receptor_ys[col] - 32.0)
                                    / prepared.field.column_dirs[col];
                            assert!((sprite.rot_x_deg - roll * y_offset / 2.0).abs() < 0.0001);
                            assert!(sprite.rot_x_deg.abs() > 1.0);
                            let deadlib_present::actors::Actor::Sprite { rot_x_deg, .. } =
                                crate::actor_from_flat_draw(draw.clone())
                            else {
                                panic!("proxy capture must retain the sprite")
                            };
                            assert_eq!(rot_x_deg, sprite.rot_x_deg);
                            checked[col] = true;
                        } else if key.starts_with("target") {
                            assert_eq!(sprite.rot_x_deg, 0.0);
                        }
                    }
                    assert_eq!(
                        checked, [true; 2],
                        "{kind:?}, direction {direction}, Roll {roll}"
                    );
                }
            }
        }
    }
}
