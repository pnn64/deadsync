use crate::{
    ErrorBarModes, FieldLayout, FieldLayoutRequest, FieldPlacement, HudLayoutOffsets,
    HudLayoutParams, LayoutMiniIndicatorPosition, NotefieldFrameFeatures, NotefieldFramePlan,
    NotefieldFramePlanRequest, ProxyCaptureRequests, ScrollTravel, ScrollTravelRequest,
    TornadoBounds, ViewOverride, ZmodLayoutParams, beat_factor, compute_invert_distances,
    compute_tornado_bounds, effective_mini_value, field_effect_height, field_layout,
    fill_lane_col_offsets, notefield_frame_plan, scroll_travel, song_time_ns_to_seconds,
};
use deadsync_core::input::MAX_COLS;
use deadsync_gameplay::{
    AccelEffects, AppearanceEffects, PerspectiveEffects, ScrollEffects,
    SongLuaColumnOffsetWindowRuntime, SongLuaNoteHideWindowRuntime, VisibilityEffects,
    VisualEffects, song_lua_column_y_offset,
};
use deadsync_noteskin::NoteskinRuntime;
use deadsync_rules::note::{Note, NoteCountStat};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::timing::{
    DelaySegment, ScrollSegment, StopSegment, TimeSignatureSegment, TimingData,
};
use deadsync_theme::NotefieldStyle;

/// Screen and player geometry supplied by the gameplay presentation boundary.
#[derive(Clone, Copy, Debug)]
pub struct NotefieldGeometry {
    pub player_idx: usize,
    pub num_players: usize,
    pub cols_per_player: usize,
    pub total_cols: usize,
    pub single_style: bool,
    pub double_style: bool,
    pub center_one_player: bool,
    pub screen_width: f32,
    pub screen_height: f32,
    pub screen_center_x: f32,
    pub screen_center_y: f32,
    pub target_arrow_pixel_size: f32,
    pub field_zoom: f32,
    pub scroll_speed: ScrollSpeedSetting,
    pub draw_distance_before_targets: f32,
    pub draw_distance_after_targets: f32,
    pub column_dirs: [f32; MAX_COLS],
    pub reverse_scroll: bool,
}

/// Per-frame gameplay visual values, already resolved from profile and attacks.
#[derive(Clone, Copy, Debug)]
pub struct NotefieldVisualState {
    pub elapsed_screen_s: f32,
    pub current_display_beat: f32,
    pub accel: AccelEffects,
    pub scroll: ScrollEffects,
    pub perspective: PerspectiveEffects,
    pub visual: VisualEffects,
    pub appearance: AppearanceEffects,
    pub visibility: VisibilityEffects,
    pub mini_percent: f32,
    pub spacing_multiplier: f32,
}

/// Borrowed timing and chart inputs used to plan the visible notefield.
#[derive(Clone, Copy, Debug)]
pub struct NotefieldChartView<'a> {
    pub timing: Option<&'a TimingData>,
    pub notes: &'a [Note],
    pub note_range: (usize, usize),
    pub lane_note_row_indices: [&'a [usize]; MAX_COLS],
    pub lane_hold_indices: [&'a [usize]; MAX_COLS],
    pub decaying_hold_indices: &'a [usize],
    pub tap_row_hold_roll_flags: &'a [u8],
    pub current_music_time_ns: i64,
    pub visible_music_time_ns: i64,
    pub visible_beat: f32,
    pub scroll_reference_bpm: f32,
    pub music_rate: f32,
    pub note_count_stats: &'a [NoteCountStat],
    pub time_signatures: &'a [TimeSignatureSegment],
    pub bpms: &'a [(f32, f32)],
    pub stops: &'a [StopSegment],
    pub delays: &'a [DelaySegment],
    pub scrolls: &'a [ScrollSegment],
}

impl NotefieldChartView<'_> {
    #[inline(always)]
    pub fn tap_row_flags(&self, note_index: usize) -> u8 {
        self.tap_row_hold_roll_flags
            .get(note_index)
            .copied()
            .unwrap_or_default()
    }
}

/// Concrete noteskin storage viewed through the renderer-neutral slot contract.
#[derive(Clone, Copy, Debug)]
pub struct NotefieldNoteskinView<'a, S> {
    pub base: Option<&'a NoteskinRuntime<S>>,
    pub mine: Option<&'a NoteskinRuntime<S>>,
    pub receptor: Option<&'a NoteskinRuntime<S>>,
    pub tap_explosion: Option<&'a NoteskinRuntime<S>>,
}

/// Song Lua note visibility and per-column placement inputs for one player.
#[derive(Clone, Copy, Debug)]
pub struct NotefieldSongLuaView<'a> {
    pub note_hides: &'a [SongLuaNoteHideWindowRuntime],
    pub column_offsets: &'a [SongLuaColumnOffsetWindowRuntime],
}

/// Profile-derived behavior and resolved asset availability in canonical terms.
#[derive(Clone, Copy, Debug)]
pub struct NotefieldOptions {
    pub frame_features: NotefieldFrameFeatures,
    pub notefield_offset: [f32; 2],
    pub judgment_offset: [f32; 2],
    pub combo_offset: [f32; 2],
    pub error_bar_offset: [f32; 2],
    pub zmod_layout: ZmodLayoutParams,
    pub has_judgment_texture: bool,
    pub error_bar_up: bool,
    pub fallback_mini_percent: f32,
    pub column_flash_compact: bool,
    pub column_flash_dimmed: bool,
    pub hide_targets: bool,
    pub hold_explosion_enabled: bool,
    pub hide_combo_explosions: bool,
    pub judgment_back: bool,
    pub show_fa_plus_window: bool,
    pub fa_plus_10ms_blue_window: bool,
    pub split_15_10ms: bool,
    pub custom_fantastic_window: bool,
    pub judgment_tilt_enabled: bool,
    pub judgment_tilt_min_ms: f32,
    pub judgment_tilt_max_ms: f32,
    pub judgment_tilt_multiplier: f32,
    pub blue_fantastic_window_s: f32,
    pub error_bar_modes: ErrorBarModes,
    pub error_bar_max_window_ix: usize,
    pub monochrome_background: bool,
    pub error_bar_multi_tick: bool,
    pub short_average_error_bar: bool,
    pub center_tick: bool,
    pub error_ms_display: bool,
    pub long_error_bar_enabled: bool,
    pub long_error_bar_intensity: f32,
    pub measure_counter: Option<MeasureCounterOptions>,
    pub mini_indicator_position: LayoutMiniIndicatorPosition,
    pub mini_indicator_zoom: f32,
    pub counter_left: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct MeasureCounterOptions {
    pub lookahead: u8,
    pub multiplier: f32,
    pub vertical: bool,
    pub left: bool,
    pub broken_run: bool,
    pub run_timer: bool,
}

/// Canonical inputs for one player notefield composition pass.
pub struct NotefieldComposeRequest<'a, S> {
    pub style: NotefieldStyle,
    pub placement: FieldPlacement,
    pub view: ViewOverride,
    pub geometry: NotefieldGeometry,
    pub visual: NotefieldVisualState,
    pub chart: NotefieldChartView<'a>,
    pub noteskin: NotefieldNoteskinView<'a, S>,
    pub song_lua: NotefieldSongLuaView<'a>,
    pub options: NotefieldOptions,
    pub capture_requests: ProxyCaptureRequests,
    pub arrow_effect_time_s: f32,
}

/// Noteskin-dependent values shared by all note, hold, receptor, and cue passes.
pub struct PreparedNotefieldNotes<'a, S> {
    pub base: &'a NoteskinRuntime<S>,
    pub mine: &'a NoteskinRuntime<S>,
    pub receptor: &'a NoteskinRuntime<S>,
    pub tap_explosion: Option<&'a NoteskinRuntime<S>>,
    pub target_arrow_px: f32,
    pub beat_factor: f32,
    pub col_offsets: [f32; MAX_COLS],
    pub invert_distances: [f32; MAX_COLS],
    pub tornado_bounds: [TornadoBounds; MAX_COLS],
    pub measure_column_xs: [f32; MAX_COLS],
    pub note_display_time_scale: f32,
    pub travel: ScrollTravel<'a>,
}

/// Purely prepared composition state consumed by actor emission.
pub struct PreparedNotefield<'a, S> {
    pub frame_plan: NotefieldFramePlan,
    pub field: FieldLayout,
    pub field_zoom: f32,
    pub scroll_speed: ScrollSpeedSetting,
    pub current_time_s: f32,
    pub current_beat: f32,
    pub mini: f32,
    pub receptor_alpha: f32,
    pub blind_active: bool,
    pub notes: Option<PreparedNotefieldNotes<'a, S>>,
}

/// Resolve canonical layout and travel inputs without reading clocks or globals.
pub fn prepare_notefield<'a, S>(
    request: &'a NotefieldComposeRequest<'a, S>,
) -> Option<PreparedNotefield<'a, S>> {
    let features = resolved_frame_features(request.options.frame_features, request.view);
    let frame_plan = notefield_frame_plan(NotefieldFramePlanRequest {
        placement: request.placement,
        num_players: request.geometry.num_players,
        cols_per_player: request.geometry.cols_per_player,
        total_cols: request.geometry.total_cols,
        features,
    })?;
    if frame_plan.player_idx != request.geometry.player_idx {
        return None;
    }
    let field_zoom = request
        .view
        .field_zoom
        .unwrap_or(request.geometry.field_zoom);
    let scroll_speed = request
        .view
        .scroll_speed
        .unwrap_or(request.geometry.scroll_speed);
    let current_time_s = song_time_ns_to_seconds(request.chart.visible_music_time_ns);
    let field = prepare_field(request, frame_plan, field_zoom, current_time_s);
    let mini = effective_mini_value(
        request.visual.mini_percent,
        request.options.fallback_mini_percent,
        request.visual.visual.big,
    );
    let effect_height = field_effect_height(
        request.geometry.screen_height,
        request.visual.perspective.tilt,
    );
    let notes = prepare_notes(request, frame_plan, field_zoom, scroll_speed, effect_height)?;
    Some(PreparedNotefield {
        frame_plan,
        field,
        field_zoom,
        scroll_speed,
        current_time_s,
        current_beat: request.chart.visible_beat,
        mini,
        receptor_alpha: (1.0 - request.visual.visibility.dark).clamp(0.0, 1.0),
        blind_active: request.visual.visibility.blind > f32::EPSILON,
        notes,
    })
}

fn prepare_field<S>(
    request: &NotefieldComposeRequest<'_, S>,
    frame_plan: NotefieldFramePlan,
    field_zoom: f32,
    current_time_s: f32,
) -> FieldLayout {
    let num_cols = frame_plan.num_cols;
    let column_reverse_percent = std::array::from_fn(|i| {
        (i < num_cols)
            .then(|| {
                request
                    .visual
                    .scroll
                    .reverse_percent_for_column(i, num_cols)
            })
            .unwrap_or_default()
    });
    let song_lua_column_y_offsets = std::array::from_fn(|i| {
        (i < num_cols)
            .then(|| song_lua_column_y_offset(request.song_lua.column_offsets, i, current_time_s))
            .unwrap_or_default()
    });
    field_layout(FieldLayoutRequest {
        style: request.style,
        placement: request.placement,
        num_players: request.geometry.num_players,
        single_style: request.geometry.single_style,
        double_style: request.geometry.double_style,
        center_one_player: request.geometry.center_one_player,
        screen_width: request.geometry.screen_width,
        screen_center_x: request.geometry.screen_center_x,
        screen_center_y: request.geometry.screen_center_y,
        num_cols,
        field_zoom,
        notefield_offset_x: request.options.notefield_offset[0],
        notefield_offset_y: request.options.notefield_offset[1],
        receptor_y_override: request.view.receptor_y,
        center_receptors_y: request.view.center_receptors_y,
        centered_scroll: request.visual.scroll.centered,
        column_reverse_percent,
        column_dirs: request.geometry.column_dirs,
        song_lua_column_y_offsets,
        judgment_offset_x: request.options.judgment_offset[0],
        combo_offset_x: request.options.combo_offset[0],
        error_bar_offset_x: request.options.error_bar_offset[0],
        hud_offsets: HudLayoutOffsets {
            judgment_extra_y: request.options.judgment_offset[1],
            combo_extra_y: request.options.combo_offset[1],
            error_bar_extra_y: request.options.error_bar_offset[1],
        },
        hud_params: HudLayoutParams {
            zmod: request.options.zmod_layout,
            has_judgment_texture: request.options.has_judgment_texture,
            error_bar_up: request.options.error_bar_up,
            error_bar_offset: request.style.error_bar_offset_y,
        },
    })
}

fn prepare_notes<'a, S>(
    request: &'a NotefieldComposeRequest<'a, S>,
    frame_plan: NotefieldFramePlan,
    field_zoom: f32,
    scroll_speed: ScrollSpeedSetting,
    effect_height: f32,
) -> Option<Option<PreparedNotefieldNotes<'a, S>>> {
    let Some(base) = request.noteskin.base else {
        return Some(None);
    };
    let timing = request.chart.timing?;
    let num_cols = frame_plan.num_cols;
    let mut col_offsets = [0.0; MAX_COLS];
    fill_lane_col_offsets(
        &mut col_offsets,
        Some(base.column_xs.as_slice()),
        num_cols,
        request.visual.spacing_multiplier,
        field_zoom,
    );
    let mut invert_distances = [0.0; MAX_COLS];
    compute_invert_distances(&col_offsets[..num_cols], &mut invert_distances[..num_cols]);
    let mut tornado_bounds = [TornadoBounds::default(); MAX_COLS];
    compute_tornado_bounds(&col_offsets[..num_cols], &mut tornado_bounds[..num_cols]);
    let current_search_beat = timing.get_beat_for_time_ns(request.chart.current_music_time_ns);
    let travel = scroll_travel(ScrollTravelRequest {
        timing,
        accel: crate::AccelYParams {
            boost: request.visual.accel.boost,
            brake: request.visual.accel.brake,
            wave: request.visual.accel.wave,
            boomerang: request.visual.accel.boomerang,
            expand: request.visual.accel.expand,
        },
        scroll_speed,
        current_time_ns: request.chart.visible_music_time_ns,
        visible_beat: request.chart.visible_beat,
        search_beat: current_search_beat,
        scroll_reference_bpm: request.chart.scroll_reference_bpm,
        music_rate: request.chart.music_rate,
        edit_beat_spacing: request.view.edit_beat_bars,
        draw_distance_after_targets: request.geometry.draw_distance_after_targets,
        draw_distance_before_targets: request.geometry.draw_distance_before_targets,
        field_zoom,
        elapsed_screen_s: request.visual.elapsed_screen_s,
        effect_height,
        screen_height: request.geometry.screen_height,
        note_count_stats: request.chart.note_count_stats,
        arrow_effect_time_s: request.arrow_effect_time_s,
        lane_tipsy: request.visual.visual.tipsy,
        lane_move_y: &request.visual.visual.move_y_cols,
    });
    let measure_column_xs =
        std::array::from_fn(|i| base.column_xs.get(i).copied().unwrap_or_default() as f32);
    Some(Some(PreparedNotefieldNotes {
        base,
        mine: request.noteskin.mine.unwrap_or(base),
        receptor: request.noteskin.receptor.unwrap_or(base),
        tap_explosion: request.noteskin.tap_explosion,
        target_arrow_px: request.geometry.target_arrow_pixel_size * field_zoom,
        beat_factor: beat_factor(request.chart.visible_beat),
        col_offsets,
        invert_distances,
        tornado_bounds,
        measure_column_xs,
        note_display_time_scale: request.geometry.num_players as f32 + 1.0,
        travel,
    }))
}

fn resolved_frame_features(
    mut features: NotefieldFrameFeatures,
    view: ViewOverride,
) -> NotefieldFrameFeatures {
    if view.edit_beat_bars {
        features.measure_line_mode = crate::MeasureLineMode::Edit;
    }
    features.combo_visible &= !view.hide_combo;
    features
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MeasureLineMode;

    fn features() -> NotefieldFrameFeatures {
        NotefieldFrameFeatures {
            measure_line_mode: MeasureLineMode::Quarter,
            measure_cues: true,
            column_cues: true,
            crossover_cues: true,
            crossover_countdown: true,
            column_flash: true,
            error_bar: true,
            error_bar_text: true,
            held_miss_asset: true,
            combo_visible: true,
        }
    }

    #[test]
    fn view_overrides_resolve_inside_canonical_preparation() {
        let resolved = resolved_frame_features(
            features(),
            ViewOverride {
                edit_beat_bars: true,
                hide_combo: true,
                ..ViewOverride::default()
            },
        );

        assert_eq!(resolved.measure_line_mode, MeasureLineMode::Edit);
        assert!(!resolved.combo_visible);
        assert!(resolved.measure_cues);
        assert!(resolved.column_cues);
    }

    #[test]
    fn default_view_preserves_profile_derived_features() {
        assert_eq!(
            resolved_frame_features(features(), ViewOverride::default()),
            features()
        );
    }
}
