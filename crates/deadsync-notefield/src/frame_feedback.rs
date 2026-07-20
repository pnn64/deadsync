use crate::explosions::{ExplosionComposeRequest, ExplosionRotation, compose_explosion_layers};
use crate::{
    ColumnFeedbackRequest, ModelMeshCache, NoteXParams, NotefieldComposeRequest, PreparedNotefield,
    ReceptorActorsRequest, ReceptorPress, compose_column_feedback, compose_receptor_actors,
    gameplay_visual_effect_params, receptor_row_center, visual_arrow_effect_zoom,
    visual_confusion_rotation_deg,
};
use deadlib_present::actors::{Actor, SpriteSource};
use deadsync_core::input::MAX_COLS;
use deadsync_core::note::NoteType;
use deadsync_gameplay::{
    ActiveColumnFlash, ActiveHold, ActiveMineExplosion, ActiveTapExplosion, ColumnCue,
    hold_explosion_active, song_lua_note_hidden,
};
use deadsync_noteskin::NoteskinSlot;
use std::sync::Arc;

/// Dynamic receptor state for one local notefield lane.
#[derive(Clone, Copy, Debug, Default)]
pub struct NotefieldLaneFeedback<'a> {
    pub active_hold: Option<&'a ActiveHold>,
    pub receptor_bop_zoom: f32,
    pub receptor_press_visual: Option<(f32, f32)>,
}

/// Borrowed per-frame feedback emitted around the receptor row.
///
/// The concrete theme prepares this view from gameplay state; canonical
/// notefield code owns actor selection, placement, animation, and ordering.
#[derive(Clone, Copy, Debug)]
pub struct NotefieldFeedbackFrameView<'a> {
    /// Cue columns use chart-global column indices.
    pub column_cues: Option<&'a [ColumnCue]>,
    /// Crossover cue columns use chart-global column indices.
    pub crossover_cues: Option<&'a [ColumnCue]>,
    /// Per-cue fade-in anchor times parallel to `crossover_cues`.
    pub crossover_cue_entries: Option<&'a [Option<f32>]>,
    /// Column flashes are ordered by local lane within the prepared player span.
    pub column_flashes: Option<&'a [Option<ActiveColumnFlash>]>,
    /// Tap explosions are ordered by local lane within the prepared player span.
    pub tap_explosions: &'a [Option<ActiveTapExplosion>],
    /// Mine explosions are ordered by local lane within the prepared player span.
    pub mine_explosions: &'a [Option<ActiveMineExplosion>],
    /// Lane feedback is ordered by local lane within the prepared player span.
    pub lanes: [NotefieldLaneFeedback<'a>; MAX_COLS],
    pub countdown_font: &'static str,
    pub countdown_text: fn(i32) -> Arc<str>,
}

/// Compose cues/flashes, receptor targets and feedback, then tap and mine
/// explosions in the canonical field ordering.
pub(crate) fn compose_notefield_feedback<S, F>(
    actors: &mut Vec<Actor>,
    hud_actors: &mut Vec<Actor>,
    model_cache: &mut ModelMeshCache,
    request: &NotefieldComposeRequest<'_, S>,
    prepared: &PreparedNotefield<'_, S>,
    frame: &NotefieldFeedbackFrameView<'_>,
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
        actors,
        hud_actors,
        ColumnFeedbackRequest {
            style: request.style,
            column_cues: frame.column_cues,
            crossover_cues: frame.crossover_cues,
            crossover_cue_entries: frame.crossover_cue_entries,
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
            countdown_text: frame.countdown_text,
        },
    );

    let receptor = notes.receptor;
    let tap_explosion = notes.tap_explosion;
    let col_offsets = notes.col_offsets;
    let invert_distances = notes.invert_distances;
    let tornado_bounds = notes.tornado_bounds;
    let beat_factor = notes.beat_factor;

    for (local_col, &receptor_y) in field.column_receptor_ys.iter().take(num_cols).enumerate() {
        let lane = frame.lanes[local_col];
        let hidden = song_lua_note_hidden(request.song_lua.note_hides, local_col, current_beat);
        let effect = gameplay_visual_effect_params(&visual, local_col);
        let confusion_rotation_deg = visual_confusion_rotation_deg(current_beat, effect);
        let center = receptor_row_center(
            field.playfield_center_x,
            local_col,
            receptor_y,
            beat_factor,
            request.arrow_effect_time_s,
            &col_offsets[..num_cols],
            &invert_distances[..num_cols],
            &tornado_bounds[..num_cols],
            &visual.move_x_cols,
            &visual.move_y_cols,
            NoteXParams {
                screen_height: request.geometry.screen_height,
                tornado: visual.tornado,
                drunk: visual.drunk,
                flip: visual.flip,
                invert: visual.invert,
                beat: visual.beat,
            },
            visual.tiny,
            visual.tipsy,
        );
        let effect_zoom = visual_arrow_effect_zoom(0.0, effect);
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
        let targets_visible =
            !hidden && !options.hide_targets && prepared.receptor_alpha > f32::EPSILON;
        let target_slot = targets_visible.then(|| &receptor.receptor_off[local_col]);
        let target_reverse = targets_visible
            .then(|| receptor.receptor_off_reverse.get(local_col).copied())
            .flatten();
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
        compose_receptor_actors(
            actors,
            model_cache,
            ReceptorActorsRequest {
                target_slot,
                target_reverse,
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
                receptor_alpha: prepared.receptor_alpha,
                field_zoom,
                rotation_y_deg: 0.0,
                pulse: &receptor.receptor_pulse,
                press_behavior: receptor.receptor_glow_behavior,
                style: request.style.receptor,
            },
            resolve_press,
            sprite_source,
        );
    }

    // Tap explosions are independent of the concrete "Hide Combo
    // Explosions" option, which applies only to combo milestone art.
    for (local_col, active) in frame.tap_explosions.iter().take(num_cols).enumerate() {
        if song_lua_note_hidden(request.song_lua.note_hides, local_col, current_beat) {
            continue;
        }
        let Some(active) = active.as_ref() else {
            continue;
        };
        let Some(explosion) = tap_explosion.and_then(|noteskin| {
            noteskin.tap_explosion_for_col_with_bright(local_col, active.window, active.bright)
        }) else {
            continue;
        };
        let effect = gameplay_visual_effect_params(&visual, local_col);
        let center = feedback_center(request, prepared, local_col, notes);
        compose_explosion_layers(
            actors,
            ExplosionComposeRequest {
                layers: explosion.layers.as_ref(),
                elapsed_s: active.elapsed,
                current_frame_beat: request.visual.current_display_beat,
                relative_frame_beat: Some(
                    (request.visual.current_display_beat - active.start_beat).max(0.0),
                ),
                uv_elapsed_s: elapsed_screen,
                center,
                field_zoom,
                effect_zoom: visual_arrow_effect_zoom(0.0, effect),
                rotation: ExplosionRotation::Tap {
                    rotation_y_deg: 0.0,
                    extra_z_deg: visual_confusion_rotation_deg(current_beat, effect),
                },
                z: request.style.actors.tap_explosion_z,
            },
            sprite_source,
        );
    }

    for (local_col, active) in frame.mine_explosions.iter().take(num_cols).enumerate() {
        let Some(active) = active.as_ref() else {
            continue;
        };
        let Some(explosion) = notes.mine.mine_hit_explosion.as_ref() else {
            continue;
        };
        let effect = gameplay_visual_effect_params(&visual, local_col);
        compose_explosion_layers(
            actors,
            ExplosionComposeRequest {
                layers: explosion.layers.as_ref(),
                elapsed_s: active.elapsed,
                current_frame_beat: current_beat,
                relative_frame_beat: None,
                uv_elapsed_s: elapsed_screen,
                center: feedback_center(request, prepared, local_col, notes),
                field_zoom,
                effect_zoom: visual_arrow_effect_zoom(0.0, effect),
                rotation: ExplosionRotation::Mine,
                z: request.style.actors.mine_explosion_z,
            },
            sprite_source,
        );
    }
}

fn feedback_center<S>(
    request: &NotefieldComposeRequest<'_, S>,
    prepared: &PreparedNotefield<'_, S>,
    local_col: usize,
    notes: &crate::PreparedNotefieldNotes<'_, S>,
) -> [f32; 2] {
    let visual = request.visual.visual;
    receptor_row_center(
        prepared.field.playfield_center_x,
        local_col,
        prepared.field.column_receptor_ys[local_col],
        notes.beat_factor,
        request.arrow_effect_time_s,
        &notes.col_offsets[..prepared.frame_plan.num_cols],
        &notes.invert_distances[..prepared.frame_plan.num_cols],
        &notes.tornado_bounds[..prepared.frame_plan.num_cols],
        &visual.move_x_cols,
        &visual.move_y_cols,
        NoteXParams {
            screen_height: request.geometry.screen_height,
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
        SongLuaNoteHideWindowRuntime, VisibilityEffects, VisualEffects,
    };
    use deadsync_noteskin::{
        ExplosionAnimation, HoldVisuals, ModelDrawState, ModelMesh, NoteDisplayMetrics,
        NoteskinRuntime, ReceptorGlowBehavior, ReceptorPulse, SpriteDefinition, TapExplosion,
    };
    use deadsync_rules::judgment::JudgeGrade;
    use deadsync_rules::note::Note;
    use deadsync_rules::scroll::ScrollSpeedSetting;
    use deadsync_rules::timing::TimingData;
    use deadsync_theme::{
        ColumnCueStyle, ColumnFlashLayoutStyle, ColumnFlashStyle, ComboFeedbackStyle,
        CounterHudStyle, ErrorBarLayers, ErrorBarPalette, ErrorBarStyle, JudgmentFeedbackStyle,
        MiniIndicatorStyle, NotefieldActorStyle, NotefieldStyle, ReceptorStyle,
    };
    use std::collections::HashMap;

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

    fn explosion(key: &str) -> TapExplosion<TestSlot> {
        TapExplosion::from_single(TestSlot::new(key), ExplosionAnimation::default())
    }

    fn noteskin() -> NoteskinRuntime<TestSlot> {
        let tap_explosions_by_col = (0..2)
            .map(|lane| HashMap::from([("W1".to_string(), explosion(&format!("tap{lane}")))]))
            .collect();
        NoteskinRuntime {
            notes: Vec::new(),
            note_layers: Vec::new(),
            lift_note_layers: Vec::new(),
            receptor_off: vec![TestSlot::new("target0"), TestSlot::new("target1")],
            receptor_glow: vec![Some(TestSlot::new("press0")), Some(TestSlot::new("press1"))],
            receptor_off_reverse: vec![Default::default(); 2],
            receptor_glow_reverse: vec![Default::default(); 2],
            receptor_step_behaviors: Vec::new(),
            mines: Vec::new(),
            mine_fill_slots: Vec::new(),
            mine_frames: Vec::new(),
            column_xs: vec![-32, 32],
            tap_explosions: HashMap::new(),
            tap_explosions_by_col,
            mine_hit_explosion: Some(explosion("mine")),
            receptor_glow_behavior: ReceptorGlowBehavior::default(),
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
            animation_is_beat_based: false,
            note_display_metrics: NoteDisplayMetrics::default(),
        }
    }

    fn style() -> NotefieldStyle {
        NotefieldStyle {
            layout_width_min: 640.0,
            layout_width_max: 854.0,
            side_center_x_ratio: 0.25,
            receptor_normal_y: -125.0,
            receptor_reverse_y: 145.0,
            receptor: ReceptorStyle {
                target_z: 100,
                press_glow_z: 105,
                hold_explosion_z: 145,
            },
            actors: NotefieldActorStyle {
                hold_body_z: 110,
                hold_cap_z: 110,
                hold_glow_z: 111,
                tap_explosion_z: 150,
                mine_explosion_z: 101,
                note_z: 140,
                mine_core_size_ratio: 0.45,
            },
            judgment_normal_y: -30.0,
            judgment_reverse_y: 30.0,
            judgment_centered_y: 95.0,
            combo_normal_y: 30.0,
            combo_reverse_y: -30.0,
            combo_centered_y: 155.0,
            judgment_height: 40.0,
            error_bar_offset_y: 25.0,
            measure_line_overscan_y: 400.0,
            measure_line_z: 80,
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
            frame_features: NotefieldFrameFeatures {
                measure_line_mode: MeasureLineMode::Off,
                measure_cues: false,
                column_cues: true,
                crossover_cues: false,
                crossover_countdown: false,
                column_flash: true,
                error_bar: false,
                error_bar_text: false,
                held_miss_asset: false,
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
        note_hides: &'a [SongLuaNoteHideWindowRuntime],
        placement: FieldPlacement,
        player_idx: usize,
        num_players: usize,
        cols_per_player: usize,
        total_cols: usize,
    ) -> NotefieldComposeRequest<'a, TestSlot> {
        NotefieldComposeRequest {
            style: style(),
            placement,
            view: ViewOverride::default(),
            geometry: NotefieldGeometry {
                player_idx,
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
                target_arrow_pixel_size: 64.0,
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
                visibility: VisibilityEffects::default(),
                mini_percent: 0.0,
                spacing_multiplier: 1.0,
            },
            chart: NotefieldChartView {
                timing: Some(timing),
                notes,
                note_range: (0, notes.len()),
                lane_note_row_indices: [&[]; MAX_COLS],
                lane_hold_indices: [&[]; MAX_COLS],
                decaying_hold_indices: &[],
                tap_row_hold_roll_flags: &[],
                current_music_time_ns: 100_000_000,
                visible_music_time_ns: 100_000_000,
                visible_beat: 1.0,
                scroll_reference_bpm: 120.0,
                music_rate: 1.0,
                note_count_stats: &[],
                time_signatures: &[],
                bpms: &[],
                stops: &[],
                delays: &[],
                scrolls: &[],
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
        })
    }

    fn mine() -> Option<ActiveMineExplosion> {
        Some(ActiveMineExplosion {
            elapsed: 0.1,
            duration: 1.0,
            started_at_screen_s: 0.0,
        })
    }

    fn countdown_text(value: i32) -> Arc<str> {
        Arc::from(value.to_string())
    }

    fn source(slot: &TestSlot) -> SpriteSource {
        SpriteSource::TextureHandle {
            key: Arc::clone(&slot.key),
            handle: 1,
            generation: 1,
        }
    }

    fn sprite_keys(actors: &[Actor]) -> Vec<&str> {
        actors
            .iter()
            .filter_map(|actor| match actor {
                Actor::Sprite {
                    source: SpriteSource::TextureHandle { key, .. },
                    ..
                } => Some(key.as_ref()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn feedback_frame_preserves_phase_and_lane_order() {
        let noteskin = noteskin();
        let timing = TimingData::default();
        let notes = [note(0)];
        let request = request(
            &noteskin,
            &timing,
            &notes,
            &[],
            FieldPlacement::P1,
            0,
            1,
            2,
            2,
        );
        let prepared = prepare_notefield(&request).expect("test notefield should prepare");
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
            crossover_cues: None,
            crossover_cue_entries: None,
            column_flashes: Some(&flashes),
            tap_explosions: &taps,
            mine_explosions: &mines,
            lanes: std::array::from_fn(|lane| match lane {
                0 => NotefieldLaneFeedback {
                    active_hold: Some(&hold),
                    receptor_bop_zoom: 1.0,
                    receptor_press_visual: Some((1.0, 1.0)),
                },
                1 => NotefieldLaneFeedback {
                    receptor_bop_zoom: 1.0,
                    receptor_press_visual: Some((1.0, 1.0)),
                    ..NotefieldLaneFeedback::default()
                },
                _ => NotefieldLaneFeedback::default(),
            }),
            countdown_font: "test",
            countdown_text,
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
            Some(Actor::Sprite {
                source: SpriteSource::Solid,
                ..
            })
        ));
        assert_eq!(
            sprite_keys(&actors),
            [
                "target0", "hold0", "press0", "target1", "press1", "tap0", "tap1", "mine", "mine",
            ]
        );
        assert!(hud.is_empty());
    }

    #[test]
    fn p2_local_lanes_honor_song_lua_hiding_except_mines() {
        let noteskin = noteskin();
        let timing = TimingData::default();
        let notes = [note(2), note(3)];
        let hides = [SongLuaNoteHideWindowRuntime {
            column: 0,
            start_beat: 0.0,
            end_beat: 2.0,
        }];
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
            columns: vec![deadsync_gameplay::ColumnCueColumn {
                column: 3,
                is_mine: false,
            }],
        }];
        let frame = NotefieldFeedbackFrameView {
            column_cues: Some(&cues),
            crossover_cues: None,
            crossover_cue_entries: None,
            column_flashes: None,
            tap_explosions: &taps,
            mine_explosions: &mines,
            lanes: std::array::from_fn(|lane| {
                if lane < 2 {
                    NotefieldLaneFeedback {
                        active_hold: Some(&holds[lane]),
                        receptor_bop_zoom: 1.0,
                        receptor_press_visual: Some((1.0, 1.0)),
                    }
                } else {
                    NotefieldLaneFeedback::default()
                }
            }),
            countdown_font: "test",
            countdown_text,
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

        let Actor::Sprite {
            source: SpriteSource::Solid,
            offset,
            ..
        } = &actors[0]
        else {
            panic!("P2 global cue should emit first");
        };
        let expected_x = prepared.field.playfield_center_x + 32.0;
        assert!((offset[0] - expected_x).abs() <= 0.001);
        assert_eq!(
            sprite_keys(&actors),
            ["target1", "hold1", "press1", "tap1", "mine", "mine"]
        );
    }
}
