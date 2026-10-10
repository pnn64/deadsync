use crate::{
    ErrorBarModes, FieldLayout, FieldLayoutRequest, FieldPlacement, HudLayoutOffsets,
    HudLayoutParams, LayoutMiniIndicatorPosition, NoteDepthFrameCache, NotePartPhaseCache,
    NotefieldFrameFeatures, NotefieldFramePlan, NotefieldFramePlanRequest, ProxyCaptureRequests,
    ScrollTravel, ScrollTravelRequest, TornadoBounds, TornadoLaneCache, ViewOverride,
    ZmodLayoutParams, beat_factor, compute_active_note_geometry, compute_tornado_lane_caches,
    effective_mini_value, field_effect_height, field_layout, fill_lane_col_offsets,
    fill_move_col_extras, note_depth_frame_cache, note_part_phase_cache, notefield_frame_plan,
    scroll_travel, song_time_ns_to_seconds, tiny_spacing_scale,
};
use deadsync_core::{input::MAX_COLS, song_time::song_time_ns_invalid};
use deadsync_gameplay::{
    AccelEffects, AppearanceEffects, ChartNoteIndex, PerspectiveEffects, ScrollEffects,
    SongLuaColumnOffsetWindowRuntime, SongLuaNoteHideWindows, VisibilityEffects, VisualEffects,
    song_lua_column_transforms,
};
#[cfg(test)]
use deadsync_gameplay::{SongLuaColumnTransformTarget, song_lua_column_offset_window_value};
use deadsync_noteskin::{NOTE_ANIM_PART_COUNT, NoteAnimPart, NoteskinRuntime};
use deadsync_rules::note::{Note, NoteCountStat};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::timing::{
    DelaySegment, ScrollSegment, StopSegment, TimeSignatureSegment, TimingData,
};
use deadsync_theme::NotefieldHudStyle;

/// Screen and player geometry supplied by the gameplay presentation boundary.
#[derive(Clone, Copy, Debug)]
pub struct NotefieldGeometry {
    pub player_idx: usize,
    pub stage_seed: u32,
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
    pub field_zoom: f32,
    pub scroll_speed: ScrollSpeedSetting,
    /// Unscaled theme draw metrics; the back distance is a positive magnitude.
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
    /// Use a constant-time horizon for active Sudden appearance effects.
    /// When fixed Sudden is also selected, this mode takes precedence.
    pub dynamic_sudden: bool,
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
    pub lane_note_row_indices: &'a [Vec<ChartNoteIndex>],
    pub lane_hold_indices: &'a [Vec<ChartNoteIndex>],
    pub note_itg_rows: &'a [i32],
    /// Song-load timing caches aligned one-to-one with `notes`. Empty or short
    /// slices are valid and fall back to canonical timing queries.
    pub note_time_cache_ns: &'a [i64],
    pub hold_end_time_cache_ns: &'a [i64],
    pub note_displayed_beat_cache: &'a [[f32; 2]],
    pub decaying_hold_indices: &'a [usize],
    /// Packed gameplay row metadata. The upper two bits contain the row's
    /// hold/roll flags; the lower bits belong to the gameplay row index.
    pub note_row_metadata: &'a [u32],
    pub visible_music_time_ns: i64,
    pub visible_beat: f32,
    pub is_in_delay: bool,
    pub search_beat: f32,
    pub scroll_reference_bpm: f32,
    pub music_rate: f32,
    pub note_count_stats: &'a [NoteCountStat],
    pub time_signatures: &'a [TimeSignatureSegment],
    pub bpms: &'a [(f32, f32)],
    pub stops: &'a [StopSegment],
    pub delays: &'a [DelaySegment],
    pub scrolls: &'a [ScrollSegment],
    /// Song-load proof that displayed beat is safe to invert as one monotonic
    /// visible range. False keeps timing cues on the legacy full scan.
    pub displayed_beat_monotonic: bool,
}

impl NotefieldChartView<'_> {
    #[inline(always)]
    pub(crate) fn lane_note_rows(&self, col: usize) -> &[ChartNoteIndex] {
        self.lane_note_row_indices
            .get(col)
            .map_or(&[], Vec::as_slice)
    }

    #[inline(always)]
    pub(crate) fn lane_holds(&self, col: usize) -> &[ChartNoteIndex] {
        self.lane_hold_indices.get(col).map_or(&[], Vec::as_slice)
    }

    #[inline(always)]
    #[must_use]
    pub fn tap_row_flags(&self, note_index: usize) -> u8 {
        self.note_row_metadata
            .get(note_index)
            .map_or(0, |&metadata| (metadata >> 30) as u8)
    }

    #[inline(always)]
    pub(crate) fn cached_note_time_ns(&self, note_index: usize, use_hold_end: bool) -> Option<i64> {
        if use_hold_end {
            self.hold_end_time_cache_ns
                .get(note_index)
                .copied()
                .filter(|&time_ns| !song_time_ns_invalid(time_ns))
        } else {
            self.note_time_cache_ns.get(note_index).copied()
        }
    }

    #[inline(always)]
    pub(crate) fn cached_displayed_beat(
        &self,
        note_index: usize,
        use_hold_end: bool,
    ) -> Option<f32> {
        self.note_displayed_beat_cache
            .get(note_index)
            .map(|pair| pair[usize::from(use_hold_end)])
            .filter(|beat| beat.is_finite())
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
    pub note_hides: &'a SongLuaNoteHideWindows,
    pub column_offsets: &'a [SongLuaColumnOffsetWindowRuntime],
    pub column_splines: &'a [deadsync_gameplay::SongLuaColumnSplineTrack],
    /// Local native Actor wrapper matrix, outside the NoteField's own tilt.
    pub wrapper: glam::Mat4,
    pub wrapper_visible: bool,
}

/// Profile-derived behavior and resolved asset availability in canonical terms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NotefieldOptions {
    pub mine_size_scale: f32,
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

#[derive(Clone, Copy, Debug, PartialEq)]
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
    pub hud_style: NotefieldHudStyle,
    /// Empty in regular gameplay; Practice supplies annotations in both views.
    pub timing_labels: &'a [crate::TimingSegmentLabel],
    pub placement: FieldPlacement,
    pub view: ViewOverride,
    pub geometry: NotefieldGeometry,
    pub visual: NotefieldVisualState,
    pub chart: NotefieldChartView<'a>,
    pub noteskin: NotefieldNoteskinView<'a, S>,
    pub song_lua: NotefieldSongLuaView<'a>,
    pub options: NotefieldOptions,
    pub capture_requests: ProxyCaptureRequests,
    pub edit_measure_text_slot_base: u8,
    /// Global game clock, kept in native double precision until selection.
    pub arrow_effect_time_s: f64,
    /// Global music seconds, before player-specific visual delay/split timing.
    pub music_time_s: f32,
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
    pub(crate) tornado_lane_caches: [TornadoLaneCache; MAX_COLS],
    pub(crate) move_x_offsets: [f32; MAX_COLS],
    pub(crate) note_depth_frame_cache: NoteDepthFrameCache,
    pub(crate) tiny_spacing_scale: f32,
    pub(crate) part_phase_caches: [NotePartPhaseCache; NOTE_ANIM_PART_COUNT],
    pub(crate) mine_phase_cache: NotePartPhaseCache,
    pub measure_column_xs: [f32; MAX_COLS],
    pub note_display_time_scale: f32,
    pub travel: ScrollTravel<'a>,
}

/// Native NoteField limits are measured before its outer zoom and Reverse.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NoteDrawRange {
    after: f32,
    before: f32,
    zoom: f32,
}

impl NoteDrawRange {
    pub(crate) fn new(
        geometry: NotefieldGeometry,
        visual: NotefieldVisualState,
        mini: f32,
        zoom: f32,
    ) -> Self {
        // NoteField::CalcPixelsBeforeAndAfterTargets: signed, unrestricted
        // amounts, a separately truncated Centered/Boomerang extension, and
        // whole-pixel truncation only after Tilt and Mini scale the range.
        let scale = (1.0 + 0.5 * visual.perspective.tilt.abs()) * (1.0 + mini.abs());
        let centered_boomerang = visual.scroll.centered * visual.accel.boomerang;
        let extension = (centered_boomerang * (-geometry.screen_height / 2.0)) as i32;
        let after = -(geometry.draw_distance_after_targets as i32 as f32)
            * (1.0 + visual.visual.draw_size_back)
            + extension as f32;
        let before =
            geometry.draw_distance_before_targets as i32 as f32 * (1.0 + visual.visual.draw_size);
        Self {
            after: (after * scale) as i32 as f32,
            before: (before * scale) as i32 as f32,
            zoom,
        }
    }

    pub(crate) fn contains(self, travel: f32) -> bool {
        let native_travel = travel / self.zoom;
        self.after <= native_travel && native_travel <= self.before
    }

    pub(crate) fn hold_visible(
        self,
        head: f32,
        tail: f32,
        head_peak: bool,
        tail_peak: bool,
    ) -> bool {
        self.contains(head)
            || self.contains(tail)
            || (head / self.zoom <= self.after && self.before <= tail / self.zoom)
            || (head_peak && !tail_peak)
    }

    pub(crate) fn bounds(self) -> Option<[f32; 2]> {
        if self.after > self.before || !self.zoom.is_finite() || self.zoom == 0.0 {
            return None;
        }
        let a = self.after * self.zoom;
        let b = self.before * self.zoom;
        Some([a.min(b), a.max(b)])
    }

    pub(crate) fn lane_bounds(self, receptor_y: f32, dir: f32, offset: f32) -> [f32; 2] {
        let a = receptor_y + self.after * self.zoom * dir + offset;
        let b = receptor_y + self.before * self.zoom * dir + offset;
        [a.min(b), a.max(b)]
    }
}

/// Purely prepared composition state consumed by actor emission.
pub struct PreparedNotefield<'a, S> {
    pub frame_plan: NotefieldFramePlan,
    pub field: FieldLayout,
    pub field_zoom: f32,
    pub scroll_speed: ScrollSpeedSetting,
    pub arrow_effect_time_s: f32,
    pub current_time_s: f32,
    pub current_beat: f32,
    pub is_in_delay: bool,
    pub mini: f32,
    pub(crate) draw_range: NoteDrawRange,
    pub receptor_alphas: [f32; MAX_COLS],
    pub blind_active: bool,
    pub column_x_offsets: [f32; MAX_COLS],
    pub column_position_splines: [deadsync_gameplay::SongLuaPositionSpline<'a>; MAX_COLS],
    pub column_zoom_splines: [deadsync_gameplay::SongLuaPositionSpline<'a>; MAX_COLS],
    // Resolved once for each lane by prepare_notefield, valid for this frame.
    pub(crate) column_spline_receptors: [[f32; 3]; MAX_COLS],
    pub spline_origin_y: f32,
    pub column_zooms: [f32; MAX_COLS],
    pub column_rotations_deg: [f32; MAX_COLS],
    pub notes: Option<PreparedNotefieldNotes<'a, S>>,
}

impl<S> PreparedNotefield<'_, S> {
    pub(crate) fn spline_position(&self, col: usize, beat: f32, base: [f32; 3]) -> [f32; 3] {
        let spline = self.column_position_splines[col];
        if spline.enabled && spline.absolute {
            let position = spline.sample(self.current_beat, beat).0;
            [
                self.field.playfield_center_x + position[0] * self.field_zoom,
                self.spline_origin_y + position[1] * self.field_zoom,
                position[2] * self.field_zoom,
            ]
        } else {
            let offset = self.spline_offsets(col, beat).0;
            std::array::from_fn(|axis| base[axis] + offset[axis])
        }
    }

    /// Resolve hold position and direction from a single spline evaluation.
    pub(crate) fn spline_path(
        &self,
        col: usize,
        beat: f32,
        base: [f32; 3],
    ) -> ([f32; 3], [f32; 3]) {
        let spline = self.column_position_splines[col];
        if spline.enabled && spline.absolute {
            let (position, derivative) = spline.sample(self.current_beat, beat);
            (
                [
                    self.field.playfield_center_x + position[0] * self.field_zoom,
                    self.spline_origin_y + position[1] * self.field_zoom,
                    position[2] * self.field_zoom,
                ],
                derivative,
            )
        } else {
            let (offset, derivative) = self.spline_offsets(col, beat);
            (
                std::array::from_fn(|axis| base[axis] + offset[axis]),
                derivative,
            )
        }
    }

    pub(crate) fn spline_zoom(&self, col: usize, beat: f32, base: f32) -> f32 {
        let spline = self.column_zoom_splines[col];
        if spline.enabled {
            spline.sample(self.current_beat, beat).0[0]
        } else {
            base
        }
    }

    pub(crate) fn spline_offsets(&self, col: usize, beat: f32) -> ([f32; 3], [f32; 3]) {
        let spline = self.column_position_splines[col];
        let (position, derivative) = spline.sample(self.current_beat, beat);
        let receptor = self.column_spline_receptors[col];
        (
            std::array::from_fn(|axis| {
                (position[axis] - if axis < 2 { receptor[axis] } else { 0.0 }) * self.field_zoom
            }),
            derivative,
        )
    }
}

/// Resolve canonical layout and travel inputs without reading clocks or globals.
#[must_use]
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
    let (
        [
            mut column_x_offsets,
            mut column_y_offsets,
            column_zooms,
            column_rotations_deg,
        ],
        mut column_position_splines,
    ) = song_lua_column_transforms(
        request.song_lua.column_offsets,
        frame_plan.num_cols,
        current_time_s,
    );
    let mut column_zoom_splines = [deadsync_gameplay::SongLuaPositionSpline::default(); MAX_COLS];
    for track in request.song_lua.column_splines {
        if track.column >= frame_plan.num_cols {
            continue;
        }
        if let Some(frame) = track.at_second(current_time_s) {
            if let Some(position) = &frame.position {
                column_position_splines[track.column] = position.view();
            }
            column_zoom_splines[track.column] = frame
                .zoom
                .as_ref()
                .map_or_else(Default::default, |zoom| zoom.view());
        }
    }
    let mut column_spline_receptors = [[0.0; 3]; MAX_COLS];
    for col in 0..frame_plan.num_cols {
        let receptor = column_position_splines[col].receptor(request.chart.visible_beat);
        column_spline_receptors[col] = receptor;
        if !column_position_splines[col].absolute {
            column_x_offsets[col] += receptor[0] * field_zoom;
            column_y_offsets[col] += receptor[1];
        }
    }
    let mut field = prepare_field(request, frame_plan, field_zoom, column_y_offsets);
    let spline_origin_y = request.geometry.screen_center_y + request.options.notefield_offset[1];
    for (col, spline) in column_position_splines
        .iter()
        .enumerate()
        .take(frame_plan.num_cols)
    {
        if spline.enabled && spline.absolute {
            field.column_receptor_ys[col] =
                spline_origin_y + column_spline_receptors[col][1] * field_zoom;
        }
    }
    let mini = effective_mini_value(
        request.visual.mini_percent,
        request.options.fallback_mini_percent,
        request.visual.visual.big,
    );
    let effect_height = field_effect_height(
        request.geometry.screen_height,
        request.visual.perspective.tilt,
    );
    let draw_range = NoteDrawRange::new(request.geometry, request.visual, mini, field_zoom);
    let arrow_effect_time_s = mod_timer_time(
        request.visual.visual,
        request.arrow_effect_time_s,
        request.visual.current_display_beat,
        request.music_time_s,
    );
    let notes = prepare_notes(
        request,
        frame_plan,
        field_zoom,
        scroll_speed,
        effect_height,
        draw_range,
        arrow_effect_time_s,
    )?;
    Some(PreparedNotefield {
        frame_plan,
        field,
        field_zoom,
        scroll_speed,
        current_time_s,
        arrow_effect_time_s,
        current_beat: request.chart.visible_beat,
        is_in_delay: request.chart.is_in_delay,
        mini,
        draw_range,
        // ITGmania adds global and column Dark before clamping receptor opacity.
        receptor_alphas: request
            .visual
            .visibility
            .dark_cols
            .map(|dark| (1.0 - request.visual.visibility.dark - dark).clamp(0.0, 1.0)),
        blind_active: request.visual.visibility.blind > f32::EPSILON,
        column_x_offsets,
        column_position_splines,
        column_zoom_splines,
        column_spline_receptors,
        spline_origin_y,
        column_zooms,
        column_rotations_deg,
        notes,
    })
}

// ArrowEffects::GetTime uses global GAMESTATE clocks, adds the offset first,
// computes in double precision, and rounds once on return to float.
fn mod_timer_time(visual: VisualEffects, game: f64, beat: f32, song: f32) -> f32 {
    let time = match visual.mod_timer_type {
        deadsync_gameplay::ModTimerType::Game | deadsync_gameplay::ModTimerType::Default => game,
        deadsync_gameplay::ModTimerType::Beat => f64::from(beat),
        deadsync_gameplay::ModTimerType::Song => f64::from(song),
    };
    ((time + f64::from(visual.mod_timer_offset)) * (1.0 + f64::from(visual.mod_timer_mult))) as f32
}

fn prepare_field<S>(
    request: &NotefieldComposeRequest<'_, S>,
    frame_plan: NotefieldFramePlan,
    field_zoom: f32,
    column_y_offsets: [f32; MAX_COLS],
) -> FieldLayout {
    let num_cols = frame_plan.num_cols;
    let column_reverse_percent = column_reverse_percents(request.visual.scroll, num_cols);
    field_layout(FieldLayoutRequest {
        hud_style: request.hud_style,
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
        song_lua_column_y_offsets: column_y_offsets,
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
            error_bar_offset: request.hud_style.error_bar_offset_y,
        },
    })
}

fn prepare_notes<'a, S>(
    request: &'a NotefieldComposeRequest<'a, S>,
    frame_plan: NotefieldFramePlan,
    field_zoom: f32,
    scroll_speed: ScrollSpeedSetting,
    effect_height: f32,
    draw_range: NoteDrawRange,
    arrow_effect_time_s: f32,
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
    let mut tornado_bounds = [TornadoBounds::default(); MAX_COLS];
    compute_active_note_geometry(
        &request.visual.visual,
        &col_offsets[..num_cols],
        &mut invert_distances[..num_cols],
        &mut tornado_bounds[..num_cols],
    );
    let mut tornado_lane_caches = [TornadoLaneCache::default(); MAX_COLS];
    compute_tornado_lane_caches(
        &col_offsets[..num_cols],
        &tornado_bounds[..num_cols],
        request.visual.visual.tornado,
        &mut tornado_lane_caches[..num_cols],
    );
    let mut move_x_offsets = [0.0; MAX_COLS];
    fill_move_col_extras(
        &request.visual.visual.move_x_cols,
        &mut move_x_offsets[..num_cols],
    );
    let note_depth_frame_cache = note_depth_frame_cache(
        request.visual.visual.bumpy_offset,
        request.visual.visual.bumpy_period,
        arrow_effect_time_s,
        request.geometry.screen_height,
    );
    let tiny_spacing_scale = tiny_spacing_scale(request.visual.visual.tiny);
    let mine = request.noteskin.mine.unwrap_or(base);
    let part_phase_caches = NoteAnimPart::ALL.map(|part| {
        note_part_phase_cache(
            request.visual.elapsed_screen_s,
            request.chart.visible_beat,
            base.note_display_metrics.part_animation[part as usize],
            base.part_animation_is_beat_based[part as usize],
        )
    });
    let mine_phase_cache = note_part_phase_cache(
        request.visual.elapsed_screen_s,
        request.chart.visible_beat,
        mine.note_display_metrics.part_animation[NoteAnimPart::Mine as usize],
        mine.part_animation_is_beat_based[NoteAnimPart::Mine as usize],
    );
    let [after, before] = draw_range.bounds().unwrap_or([0.0; 2]);
    let travel = scroll_travel(ScrollTravelRequest {
        timing,
        accel: crate::AccelYParams {
            boost: request.visual.accel.boost,
            brake: request.visual.accel.brake,
            wave: request.visual.accel.wave,
            boomerang: request.visual.accel.boomerang,
            expand: request.visual.accel.expand,
        },
        random_speed: request.visual.visual.random_speed,
        stage_seed: request.geometry.stage_seed,
        scroll_speed,
        current_time_ns: request.chart.visible_music_time_ns,
        visible_beat: request.chart.visible_beat,
        search_beat: request.chart.search_beat,
        scroll_reference_bpm: request.chart.scroll_reference_bpm,
        music_rate: request.chart.music_rate,
        edit_beat_spacing: request.view.edit_beat_bars,
        draw_distance_after_targets: -after,
        draw_distance_before_targets: before,
        field_zoom,
        elapsed_screen_s: request.visual.elapsed_screen_s,
        effect_height,
        screen_height: request.geometry.screen_height,
        note_count_stats: request.chart.note_count_stats,
        arrow_effect_time_s,
        lane_tipsy: request.visual.visual.tipsy,
        lane_tipsy_offset: request.visual.visual.tipsy_offset,
        lane_tipsy_speed: request.visual.visual.tipsy_speed,
        lane_move_y: &request.visual.visual.move_y_cols,
    });
    let measure_column_xs =
        std::array::from_fn(|i| base.column_xs.get(i).copied().unwrap_or_default() as f32);
    Some(Some(PreparedNotefieldNotes {
        base,
        mine,
        receptor: request.noteskin.receptor.unwrap_or(base),
        tap_explosion: request.noteskin.tap_explosion,
        target_arrow_px: ScrollSpeedSetting::ARROW_SPACING * field_zoom,
        beat_factor: beat_factor(request.chart.visible_beat),
        col_offsets,
        invert_distances,
        tornado_bounds,
        tornado_lane_caches,
        move_x_offsets,
        note_depth_frame_cache,
        tiny_spacing_scale,
        part_phase_caches,
        mine_phase_cache,
        measure_column_xs,
        note_display_time_scale: request.geometry.num_players as f32 + 1.0,
        travel,
    }))
}

#[inline(always)]
fn column_reverse_percents(scroll: ScrollEffects, num_cols: usize) -> [f32; MAX_COLS] {
    let active_cols = num_cols.min(MAX_COLS);
    let mut out = [0.0; MAX_COLS];
    if active_cols == 0 {
        return out;
    }
    if scroll.split == 0.0 && scroll.alternate == 0.0 && scroll.cross == 0.0 {
        out[..active_cols].fill(scroll.reverse_percent_for_column(0, num_cols));
        return out;
    }
    for (local_col, percent) in out.iter_mut().take(active_cols).enumerate() {
        *percent = scroll.reverse_percent_for_column(local_col, num_cols);
    }
    out
}

const fn resolved_frame_features(
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

    #[test]
    fn mod_timer_matches_native_get_time_bits() {
        // Golden bits from the unmodified ArrowEffects::GetTime compiled with
        // MSVC /O2. Large game times detect premature float conversion.
        let cases: [(u8, f64, f32, f32, f32, f32, u32); 20] = [
            (0, 17.125, 3.25, -0.5, 0.0, 0.0, 0x41890000u32),
            (1, 17.125, 3.25, -0.5, 0.0, 0.0, 0x40500000u32),
            (2, 17.125, 3.25, -0.5, 0.0, 0.0, 0xbf000000u32),
            (3, 17.125, 3.25, -0.5, 0.0, 0.0, 0x41890000u32),
            (
                0,
                1234.1234567890001,
                17.123455,
                9.76543236,
                0.333333343,
                -0.125,
                0x44cdaa9au32,
            ),
            (
                1,
                1234.1234567890001,
                17.123455,
                9.76543236,
                0.333333343,
                -0.125,
                0x41b5511du32,
            ),
            (
                2,
                1234.1234567890001,
                17.123455,
                9.76543236,
                0.333333343,
                -0.125,
                0x414da99du32,
            ),
            (
                3,
                1234.1234567890001,
                17.123455,
                9.76543236,
                0.333333343,
                -0.125,
                0x44cdaa9au32,
            ),
            (
                0,
                16777217.25,
                12345.75,
                -3.5,
                -0.99999994,
                16777216.0,
                0x40000000u32,
            ),
            (
                1,
                16777217.25,
                12345.75,
                -3.5,
                -0.99999994,
                16777216.0,
                0x3f80181du32,
            ),
            (
                2,
                16777217.25,
                12345.75,
                -3.5,
                -0.99999994,
                16777216.0,
                0x3f7ffffcu32,
            ),
            (
                3,
                16777217.25,
                12345.75,
                -3.5,
                -0.99999994,
                16777216.0,
                0x40000000u32,
            ),
            (0, 987654321.125, -123.125, 1.25, -2.5, -0.75, 0xceb09b3au32),
            (1, 987654321.125, -123.125, 1.25, -2.5, -0.75, 0x4339d000u32),
            (2, 987654321.125, -123.125, 1.25, -2.5, -0.75, 0xbf400000u32),
            (3, 987654321.125, -123.125, 1.25, -2.5, -0.75, 0xceb09b3au32),
            (0, 99999.75, 0.0, -1.5, -1.0, 123.0, 0x00000000u32),
            (1, 99999.75, 0.0, -1.5, -1.0, 123.0, 0x00000000u32),
            (2, 99999.75, 0.0, -1.5, -1.0, 123.0, 0x00000000u32),
            (3, 99999.75, 0.0, -1.5, -1.0, 123.0, 0x00000000u32),
        ];
        for (mode, game, beat, song, mult, offset, bits) in cases {
            let visual = VisualEffects {
                mod_timer_type: deadsync_gameplay::ModTimerType::from_value(f32::from(mode))
                    .expect("valid native mode"),
                mod_timer_mult: mult,
                mod_timer_offset: offset,
                ..Default::default()
            };
            assert_eq!(
                mod_timer_time(visual, game, beat, song).to_bits(),
                bits,
                "mode {mode}, game {game}"
            );
        }
    }

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
            combo_visible: true,
        }
    }

    #[test]
    fn borrowed_lane_storage_preserves_lane_views() {
        let index = |value| ChartNoteIndex::try_from_usize(value).expect("test index fits u32");
        let note_rows = [vec![index(2), index(9)], vec![index(4)], vec![]];
        let holds = [vec![], vec![index(7), index(11)]];
        let view = NotefieldChartView {
            timing: None,
            notes: &[],
            note_range: (0, 0),
            lane_note_row_indices: &note_rows,
            lane_hold_indices: &holds,
            note_itg_rows: &[],
            note_time_cache_ns: &[],
            hold_end_time_cache_ns: &[],
            note_displayed_beat_cache: &[],
            decaying_hold_indices: &[],
            note_row_metadata: &[],
            visible_music_time_ns: 0,
            visible_beat: 0.0,
            is_in_delay: false,
            search_beat: 0.0,
            scroll_reference_bpm: 120.0,
            music_rate: 1.0,
            note_count_stats: &[],
            time_signatures: &[],
            bpms: &[],
            stops: &[],
            delays: &[],
            scrolls: &[],
            displayed_beat_monotonic: true,
        };

        assert_eq!(view.lane_note_rows(0), &[2, 9]);
        assert_eq!(view.lane_note_rows(1), &[4]);
        assert!(view.lane_note_rows(3).is_empty());
        assert_eq!(view.lane_holds(1), &[7, 11]);
        assert!(view.lane_holds(2).is_empty());
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

    #[test]
    fn song_lua_column_transforms_resolve_x_zoom_and_rotation() {
        let window = |target, from_y, to_y| {
            deadsync_gameplay::build_song_lua_column_offset_window_runtime(
                1, target, 1.0, 3.0, 3.0, from_y, to_y, None, None, None,
            )
        };
        let windows = [
            window(SongLuaColumnTransformTarget::OffsetX, 0.0, 64.0),
            window(SongLuaColumnTransformTarget::OffsetY, 0.0, 80.0),
            window(SongLuaColumnTransformTarget::Zoom, 1.0, 0.0),
            window(
                SongLuaColumnTransformTarget::RotationZ,
                0.0,
                std::f32::consts::TAU,
            ),
        ];

        let ([x_offsets, y_offsets, zooms, rotations], _) =
            song_lua_column_transforms(&windows, 4, 2.0);
        assert!((x_offsets[1] - 32.0).abs() <= f32::EPSILON);
        assert!((zooms[1] - 0.5).abs() <= f32::EPSILON);
        assert!((rotations[1] - 180.0).abs() <= 0.001);
        assert_eq!(y_offsets[1], 40.0);
    }

    #[test]
    fn column_transforms_preserve_last_active_window_per_target() {
        use SongLuaColumnTransformTarget::{OffsetX, OffsetY, RotationZ, Zoom};
        let targets = [OffsetX, OffsetY, Zoom, RotationZ];
        let mut windows = Vec::new();
        for column in [0, 1, MAX_COLS - 1, MAX_COLS] {
            for target in targets {
                for (start, end, sustain, from, to, easing) in [
                    (0.0, 2.0, 5.0, -2.0, 3.0, Some("inoutsine")),
                    (1.0, 3.0, 4.0, 9.0, -0.0, Some("outbounce")),
                    (2.0, 2.0, 2.5, 0.0, -4.0, None),
                ] {
                    windows.push(
                        deadsync_gameplay::build_song_lua_column_offset_window_runtime(
                            column, target, start, end, sustain, from, to, easing, None, None,
                        ),
                    );
                }
            }
        }
        for num_cols in [0, 1, 4, MAX_COLS, MAX_COLS + 1] {
            for time in [
                -1.0,
                0.0,
                0.5,
                1.0,
                1.75,
                2.0,
                2.5,
                3.0,
                4.0001,
                5.1,
                f32::NAN,
            ] {
                let (actual, _) = song_lua_column_transforms(&windows, num_cols, time);
                for (target_index, target) in targets.into_iter().enumerate() {
                    for column in 0..MAX_COLS {
                        let expected = windows
                            .iter()
                            .rev()
                            .filter(|window| {
                                column < num_cols.min(MAX_COLS)
                                    && window.column == column
                                    && window.target == target
                            })
                            .find_map(|window| song_lua_column_offset_window_value(window, time))
                            .map(|value| match target {
                                Zoom => value.max(0.0),
                                RotationZ => value.to_degrees(),
                                _ => value,
                            })
                            .unwrap_or(if target == Zoom { 1.0 } else { 0.0 });
                        let actual = actual[target_index][column];
                        assert!(
                            actual.to_bits() == expected.to_bits()
                                || (actual.is_nan() && expected.is_nan()),
                            "columns={num_cols}, time={time}, column={column}, target={target:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn active_note_geometry_matches_unconditional_calculation() {
        let cols = [-224.0, -160.0, -96.0, -32.0, 32.0, 96.0, 160.0, 224.0];
        let mut expected_invert = [0.0; MAX_COLS];
        crate::compute_invert_distances(&cols, &mut expected_invert);
        let mut expected_tornado = [TornadoBounds::default(); MAX_COLS];
        crate::compute_tornado_bounds(&cols, &mut expected_tornado);
        let mut actual_invert = [0.0; MAX_COLS];
        let mut actual_tornado = [TornadoBounds::default(); MAX_COLS];
        compute_active_note_geometry(
            &VisualEffects {
                invert: -0.5,
                tornado: 0.75,
                ..VisualEffects::default()
            },
            &cols,
            &mut actual_invert,
            &mut actual_tornado,
        );
        assert_eq!(actual_invert, expected_invert);
        assert_eq!(actual_tornado, expected_tornado);
    }

    #[test]
    fn inactive_note_geometry_does_not_change_note_positions() {
        let cols = [-224.0, -160.0, -96.0, -32.0, 32.0, 96.0, 160.0, 224.0];
        let mut populated_invert = [0.0; MAX_COLS];
        crate::compute_invert_distances(&cols, &mut populated_invert);
        let mut populated_tornado = [TornadoBounds::default(); MAX_COLS];
        crate::compute_tornado_bounds(&cols, &mut populated_tornado);
        let empty_invert = [0.0; MAX_COLS];
        let empty_tornado = [TornadoBounds::default(); MAX_COLS];
        let move_x = [0.0; MAX_COLS];
        for local_col in 0..MAX_COLS {
            let expected = crate::note_x_offset(
                local_col,
                180.0,
                0.25,
                4.0,
                &cols,
                &populated_invert,
                &populated_tornado,
                &move_x,
                crate::NoteXParams::default(),
                0.0,
            );
            let actual = crate::note_x_offset(
                local_col,
                180.0,
                0.25,
                4.0,
                &cols,
                &empty_invert,
                &empty_tornado,
                &move_x,
                crate::NoteXParams::default(),
                0.0,
            );
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
    }
}
