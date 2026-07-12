use crate::act;
use crate::assets;
use crate::notefield_style::notefield_style;
use crate::screens::gameplay::{GameplayCoreState as State, GameplayNoteskinAssets};
use deadlib_present::actors::{Actor, IntoTextureKey};
use deadlib_present::color;
use deadlib_present::space::*;
use deadlib_render::BlendMode;
use deadsync_assets::noteskin::SpriteSlot;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_core::note::NoteType;
use deadsync_gameplay::{
    AppearanceEffects, FantasticWindowOptions, GameplayErrorBarTrim, TapExplosionOptions,
    VisualEffects, blue_fantastic_window_ms, gameplay_error_bar_trim_max_window_ix,
    hold_explosion_enabled_for_options, hold_head_render_flags, song_lua_note_hidden,
};
use deadsync_notefield::{
    BuiltNotefield, ComboFeedbackRequest, ComboMilestoneAssets, CounterHudRequest,
    ErrorBarComposeRequest, ErrorBarModes, ErrorBarState, HoldBodyCapRequest, HoldComposeControl,
    HoldEntryPlanRequest, HoldPathSample, IndicatorSprite, JudgmentFeedbackRequest,
    JudgmentTiltParams, LayoutMiniIndicatorPosition, MeasureComposeRequest, MeasureCounterOptions,
    MeasureLineMode, MineLayerRequest, MiniIndicatorRequest, ModelMeshCache, NoteAlphaParams,
    NoteLayerRequest, NoteXParams, NotefieldChartView, NotefieldComposeRequest,
    NotefieldFeedbackFrameView, NotefieldFrameFeatures, NotefieldGeometry, NotefieldLaneFeedback,
    NotefieldNoteskinView, NotefieldOptions, NotefieldSongLuaView, NotefieldVisualState,
    TapJudgmentFeedback, TapJudgmentRowsParams, TapJudgmentSprite, TornadoBounds,
    VisualEffectParams, ZmodLayoutParams, appearance_note_actor_alpha, appearance_note_glow,
    compose_combo_feedback, compose_counter_hud, compose_error_bar, compose_hold_body_caps,
    compose_judgment_feedback, compose_measure_lines, compose_mine_layers, compose_mini_indicator,
    compose_note_layer, compose_notefield_feedback, for_each_visible_hold_index,
    for_each_visible_note_index, gameplay_visual_effect_params as visual_effect_params,
    hold_entry_head_beat, hold_entry_plan, hold_overlaps_visible_window, hold_parts_for_note_type,
    judgment_actor_zoom, judgment_tilt_rotation_deg as crate_judgment_tilt_rotation_deg,
    mine_hides_after_resolution, mine_part, note_world_z_for_bumpy,
    note_x_offset as crate_note_x_offset, notefield_view_proj, offset_center, prepare_notefield,
    receptor_row_center as crate_receptor_row_center, scale_sprite_to_arrow, share_actor_range,
    song_lua_note_model_draw, tap_judgment_rows as crate_tap_judgment_rows, tap_part_for_note_type,
    tap_replacement_head, translated_uv_rect, visual_arrow_effect_zoom,
    visual_hold_body_needs_z_buffer, visual_note_rotation_z, visual_pulse_zoom_for_y,
    visual_tiny_zoom, visual_use_legacy_hold_sprites, zmod_broken_run_end,
};
use deadsync_notefield::{FieldPlacement, ProxyCaptureRequests, ViewOverride};
use deadsync_noteskin::NUM_QUANTIZATIONS;
use deadsync_profile as profile_data;
use deadsync_rules::judgment::Judgment;
use deadsync_rules::note::HoldResult;
use deadsync_theme::NotefieldStyle;
use std::array::from_fn;
use std::cell::RefCell;

mod prewarm;
mod text;
mod zmod;
pub use prewarm::prewarm_text_layout;
pub(crate) use text::gameplay_mods_text;
use text::{
    cached_error_bar_text_label, cached_int_i32, cached_int_u32, cached_offset_ms,
    cached_zmod_measure_counter_text, effective_accel_effects_for_player,
    effective_mini_percent_for_player, effective_perspective_effects_for_player,
    effective_scroll_effects_for_player, effective_spacing_multiplier_for_player,
    effective_visual_effects_for_player, zmod_run_timer_fmt,
};
use zmod::{
    zmod_combo_font_name, zmod_indicator_mode, zmod_mini_indicator_text, zmod_mini_indicator_zoom,
    zmod_resolved_combo_color, zmod_small_combo_font,
};

#[inline(always)]
fn player_blue_window_ms(state: &State, player_idx: usize) -> f32 {
    let base = state.default_fa_plus_window_s();
    let Some(profile) = state.profiles().get(player_idx) else {
        return base * 1000.0;
    };
    blue_fantastic_window_ms(FantasticWindowOptions {
        base_fa_plus_s: base,
        custom_fantastic_window_s: profile.custom_fantastic_window.then(|| {
            f32::from(profile_data::clamp_custom_fantastic_window_ms(
                profile.custom_fantastic_window_ms,
            )) / 1000.0
        }),
        fa_plus_10ms_blue_window: profile.fa_plus_10ms_blue_window,
    })
}

// --- CONSTANTS ---

// Simply Love ScreenGameplay in/default.lua keeps intro cover actors alive for 2.0s.
const TRANSITION_IN_DURATION: f32 = 2.0;

// Gameplay Layout & Feel
const TARGET_ARROW_PIXEL_SIZE: f32 = 64.0; // Dance lane width for hold bodies and square fallback visuals

const DISPLAY_MODS_ZOOM: f32 = 0.8;
const DISPLAY_MODS_WRAP_WIDTH_PX: f32 = 125.0;
const DISPLAY_MODS_LINE_STEP: f32 = 15.0;
const DISPLAY_MODS_WARNING_W: f32 = 90.0;
const DISPLAY_MODS_WARNING_H: f32 = 30.0;
const DISPLAY_MODS_WARNING_ZOOM: f32 = 1.5;

const TEXT_CACHE_LIMIT: usize = 8192;
const COMBO_PREWARM_CAP: u32 = 2048;
const MEASURE_PREWARM_CAP: i32 = 64;
const COLUMN_COUNTDOWN_PREWARM_CAP: i32 = 64;
const RUN_TIMER_PREWARM_CAP_S: i32 = 600;

#[inline(always)]
fn judgment_tilt_rotation_deg(options: &NotefieldOptions, judgment: &Judgment) -> f32 {
    crate_judgment_tilt_rotation_deg(JudgmentTiltParams {
        enabled: options.judgment_tilt_enabled,
        grade: judgment.grade,
        time_error_ms: judgment.time_error_ms,
        min_threshold_ms: options.judgment_tilt_min_ms,
        max_threshold_ms: options.judgment_tilt_max_ms,
        multiplier: options.judgment_tilt_multiplier,
    })
}

#[inline(always)]
fn note_slot_base_size(slot: &SpriteSlot, scale: f32) -> [f32; 2] {
    if let Some(model) = slot.model.as_ref() {
        let model_size = model.size();
        if model_size[0] > f32::EPSILON && model_size[1] > f32::EPSILON {
            return [model_size[0] * scale, model_size[1] * scale];
        }
    }
    let logical = slot.logical_size();
    [logical[0] * scale, logical[1] * scale]
}

#[inline(always)]
fn note_glow(y_no_reverse: f32, elapsed: f32, mini: f32, appearance: AppearanceEffects) -> f32 {
    appearance_note_glow(y_no_reverse, elapsed, mini, note_alpha_params(appearance))
}

#[inline(always)]
fn note_actor_alpha(
    y_no_reverse: f32,
    elapsed: f32,
    mini: f32,
    appearance: AppearanceEffects,
) -> f32 {
    appearance_note_actor_alpha(y_no_reverse, elapsed, mini, note_alpha_params(appearance))
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

#[inline(always)]
fn note_x_offset(
    local_col: usize,
    y: f32,
    arrow_effect_time_s: f32,
    beat_factor: f32,
    visual: VisualEffects,
    col_offsets: &[f32],
    invert_distances: &[f32],
    tornado_bounds: &[TornadoBounds],
) -> f32 {
    crate_note_x_offset(
        local_col,
        y,
        beat_factor,
        arrow_effect_time_s,
        col_offsets,
        invert_distances,
        tornado_bounds,
        &visual.move_x_cols,
        NoteXParams {
            screen_height: screen_height(),
            tornado: visual.tornado,
            drunk: visual.drunk,
            flip: visual.flip,
            invert: visual.invert,
            beat: visual.beat,
        },
        visual.tiny,
    )
}

#[inline(always)]
fn receptor_row_center(
    playfield_center_x: f32,
    local_col: usize,
    receptor_y_lane: f32,
    arrow_effect_time_s: f32,
    beat_factor: f32,
    visual: VisualEffects,
    col_offsets: &[f32],
    invert_distances: &[f32],
    tornado_bounds: &[TornadoBounds],
) -> [f32; 2] {
    crate_receptor_row_center(
        playfield_center_x,
        local_col,
        receptor_y_lane,
        beat_factor,
        arrow_effect_time_s,
        col_offsets,
        invert_distances,
        tornado_bounds,
        &visual.move_x_cols,
        &visual.move_y_cols,
        NoteXParams {
            screen_height: screen_height(),
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
    visual_tiny_zoom(visual_effect_params(visual, local_col))
}

#[inline(always)]
fn pulse_zoom_for_y(y: f32, visual: &VisualEffects) -> f32 {
    visual_pulse_zoom_for_y(y, visual_effect_params(visual, 0))
}

#[inline(always)]
fn arrow_effect_zoom(visual: &VisualEffects, local_col: usize, y: f32) -> f32 {
    visual_arrow_effect_zoom(y, visual_effect_params(visual, local_col))
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
        visual_effect_params(&visual, local_col),
    )
}

#[inline(always)]
fn column_flash_dimmed(brightness: profile_data::ColumnFlashBrightness) -> bool {
    matches!(brightness, profile_data::ColumnFlashBrightness::Dimmed)
}

#[inline(always)]
fn resolved_judgment_texture(
    profile: &profile_data::Profile,
) -> Option<&'static assets::TextureChoice> {
    assets::resolve_texture_choice_entry(
        profile.judgment_graphic.texture_key(),
        assets::judgment_texture_choices(),
    )
}

#[inline(always)]
fn resolved_hold_judgment_texture(
    profile: &profile_data::Profile,
) -> Option<&'static assets::TextureChoice> {
    assets::resolve_texture_choice_entry(
        profile.hold_judgment_graphic.texture_key(),
        assets::hold_judgment_texture_choices(),
    )
}

#[inline(always)]
fn resolved_held_miss_texture(
    profile: &profile_data::Profile,
) -> Option<&'static assets::TextureChoice> {
    assets::resolve_texture_choice_entry(
        profile.held_miss_graphic.texture_key(),
        assets::held_miss_texture_choices(),
    )
}

#[inline(always)]
fn judgment_frame_size(texture_key: &str) -> [f32; 2] {
    let Some(meta) = assets::texture_dims(texture_key) else {
        return [0.0, 76.0];
    };
    let (w, h) = assets::texture_source_frame_dims_from_real(texture_key, meta.w, meta.h);
    [w as f32, h as f32]
}

#[inline(always)]
fn tap_judgment_rows(
    options: &NotefieldOptions,
    judgment: &Judgment,
    frame_rows: usize,
) -> (usize, Option<usize>) {
    crate_tap_judgment_rows(TapJudgmentRowsParams {
        grade: judgment.grade,
        window: judgment.window,
        time_error_ms: judgment.time_error_ms,
        frame_rows,
        show_fa_plus_window: options.show_fa_plus_window,
        fa_plus_10ms_blue_window: options.fa_plus_10ms_blue_window,
        split_15_10ms: options.split_15_10ms,
        custom_fantastic_window: options.custom_fantastic_window,
    })
}

#[inline(always)]
fn gameplay_error_bar_trim(trim: profile_data::ErrorBarTrim) -> GameplayErrorBarTrim {
    match trim {
        profile_data::ErrorBarTrim::Off => GameplayErrorBarTrim::Off,
        profile_data::ErrorBarTrim::Fantastic => GameplayErrorBarTrim::Fantastic,
        profile_data::ErrorBarTrim::Excellent => GameplayErrorBarTrim::Excellent,
        profile_data::ErrorBarTrim::Great => GameplayErrorBarTrim::Great,
    }
}

#[inline(always)]
fn error_bar_trim_max_window_ix(trim: profile_data::ErrorBarTrim) -> usize {
    gameplay_error_bar_trim_max_window_ix(gameplay_error_bar_trim(trim))
}

#[inline(always)]
fn zmod_layout_params(
    profile: &profile_data::Profile,
    style: NotefieldStyle,
    has_judgment_texture: bool,
) -> ZmodLayoutParams {
    // Zmod SL-Layout.lua: hasErrorBar checks multiple flags.
    let mut error_bar_mask = profile.error_bar_active_mask;
    if error_bar_mask.is_empty() {
        error_bar_mask =
            profile_data::error_bar_mask_from_style(profile.error_bar, profile.error_bar_text);
    }
    let has_error_bar = !error_bar_mask.is_empty();
    let mini_indicator_position = match profile.mini_indicator_position {
        profile_data::MiniIndicatorPosition::Default => LayoutMiniIndicatorPosition::Default,
        profile_data::MiniIndicatorPosition::UnderUpArrow => {
            LayoutMiniIndicatorPosition::UnderUpArrow
        }
    };
    ZmodLayoutParams {
        judgment_height: style.judgment_height,
        has_error_bar,
        has_judgment_texture,
        error_bar_up: profile.error_bar_up,
        has_measure_counter: profile.measure_counter != profile_data::MeasureCounter::None,
        measure_counter_up: profile.measure_counter_up,
        broken_run: profile.broken_run,
        mini_indicator_position,
    }
}

#[inline(always)]
fn hold_explosion_enabled(profile: &profile_data::Profile) -> bool {
    let mask = profile.tap_explosion_active_mask;
    hold_explosion_enabled_for_options(TapExplosionOptions {
        fantastic: mask.contains(profile_data::TapExplosionMask::FANTASTIC),
        excellent: mask.contains(profile_data::TapExplosionMask::EXCELLENT),
        great: mask.contains(profile_data::TapExplosionMask::GREAT),
        decent: mask.contains(profile_data::TapExplosionMask::DECENT),
        way_off: mask.contains(profile_data::TapExplosionMask::WAY_OFF),
        miss: mask.contains(profile_data::TapExplosionMask::MISS),
        held: mask.contains(profile_data::TapExplosionMask::HELD),
        holding: mask.contains(profile_data::TapExplosionMask::HOLDING),
    })
}

pub(crate) fn build_bundles(
    state: &State,
    player_idx: usize,
    arrow_effect_time_s: f32,
    noteskin_assets: &GameplayNoteskinAssets,
    model_caches: &[RefCell<ModelMeshCache>; MAX_PLAYERS],
    profile: &profile_data::Profile,
    placement: FieldPlacement,
    play_style: profile_data::PlayStyle,
    center_1player_notefield: bool,
    capture_requests: ProxyCaptureRequests,
    warn_cmod_for_itl_chart: bool,
    view: ViewOverride,
    mut actors: &mut Vec<Actor>,
    mut hud_actors: &mut Vec<Actor>,
) -> BuiltNotefield {
    actors.clear();
    hud_actors.clear();
    let hold_judgment_texture = resolved_hold_judgment_texture(profile);
    let held_miss_texture = resolved_held_miss_texture(profile);

    let measure_line_mode = match profile.measure_lines {
        profile_data::MeasureLines::Off => MeasureLineMode::Off,
        profile_data::MeasureLines::Measure => MeasureLineMode::Measure,
        profile_data::MeasureLines::Quarter => MeasureLineMode::Quarter,
        profile_data::MeasureLines::Eighth => MeasureLineMode::Eighth,
    };
    let error_bar_mask = {
        let mut mask = profile.error_bar_active_mask;
        if mask.is_empty() {
            mask =
                profile_data::error_bar_mask_from_style(profile.error_bar, profile.error_bar_text);
        }
        mask
    };
    let p = &state.players()[player_idx];
    let mut model_cache = model_caches[player_idx].borrow_mut();

    // Collect concrete profile/runtime inputs here; canonical placement stays
    // independent of Profile, gameplay state, config, and theme globals.
    let style = notefield_style();
    let notefield_offset_x = profile.note_field_offset_x.clamp(0, 50) as f32;
    let notefield_offset_y = profile.note_field_offset_y.clamp(-50, 50) as f32;
    let judgment_offset_x = profile
        .judgment_offset_x
        .clamp(profile_data::HUD_OFFSET_MIN, profile_data::HUD_OFFSET_MAX)
        as f32;
    let judgment_offset_y = profile
        .judgment_offset_y
        .clamp(profile_data::HUD_OFFSET_MIN, profile_data::HUD_OFFSET_MAX)
        as f32;
    let combo_offset_x = profile
        .combo_offset_x
        .clamp(profile_data::HUD_OFFSET_MIN, profile_data::HUD_OFFSET_MAX)
        as f32;
    let combo_offset_y = profile
        .combo_offset_y
        .clamp(profile_data::HUD_OFFSET_MIN, profile_data::HUD_OFFSET_MAX)
        as f32;
    let error_bar_offset_x = profile
        .error_bar_offset_x
        .clamp(profile_data::HUD_OFFSET_MIN, profile_data::HUD_OFFSET_MAX)
        as f32;
    let error_bar_offset_y = profile
        .error_bar_offset_y
        .clamp(profile_data::HUD_OFFSET_MIN, profile_data::HUD_OFFSET_MAX)
        as f32;
    let judgment_texture = resolved_judgment_texture(profile);
    let has_judgment_texture = judgment_texture.is_some();
    let elapsed_screen = state.total_elapsed_in_screen();
    let accel = effective_accel_effects_for_player(state, player_idx);
    let scroll = effective_scroll_effects_for_player(state, player_idx);
    let perspective = effective_perspective_effects_for_player(state, player_idx);
    let visual = effective_visual_effects_for_player(state, player_idx);
    let appearance = state.effective_appearance_effects_for_player(player_idx);
    let visibility = state.effective_visibility_effects_for_player(player_idx);
    let mini_percent = effective_mini_percent_for_player(state, player_idx);
    let spacing_mult = effective_spacing_multiplier_for_player(state, player_idx);
    let player_col_start = player_idx.saturating_mul(state.cols_per_player());
    let column_dirs = from_fn(|local_col| {
        let col = player_col_start + local_col;
        if local_col >= state.cols_per_player() || col >= state.num_cols() {
            1.0
        } else {
            state.notefield_column_scroll_dir(col)
        }
    });
    let (time_signatures, bpms, stops, delays, scrolls) = state
        .gameplay_chart(player_idx)
        .map(|chart| {
            let timing = &chart.timing_segments;
            (
                timing.time_signatures.as_slice(),
                timing.bpms.as_slice(),
                timing.stops.as_slice(),
                timing.delays.as_slice(),
                timing.scrolls.as_slice(),
            )
        })
        .unwrap_or((&[], &[], &[], &[], &[]));
    let base_noteskin = noteskin_assets.noteskin[player_idx].as_deref();
    let tap_explosion_noteskin = if profile.tap_explosion_noteskin_hidden() {
        None
    } else {
        noteskin_assets.tap_explosion_noteskin[player_idx]
            .as_deref()
            .or(base_noteskin)
    };
    let request = NotefieldComposeRequest {
        style,
        placement,
        view,
        geometry: NotefieldGeometry {
            player_idx,
            num_players: state.num_players(),
            cols_per_player: state.cols_per_player(),
            total_cols: state.num_cols(),
            single_style: play_style == profile_data::PlayStyle::Single,
            double_style: play_style == profile_data::PlayStyle::Double,
            center_one_player: center_1player_notefield,
            screen_width: screen_width(),
            screen_height: screen_height(),
            screen_center_x: screen_center_x(),
            screen_center_y: screen_center_y(),
            target_arrow_pixel_size: TARGET_ARROW_PIXEL_SIZE,
            field_zoom: state.field_zoom_for_player(player_idx),
            scroll_speed: state.effective_scroll_speed_for_player(player_idx),
            draw_distance_before_targets: state.notefield_draw_distance_before_targets(player_idx),
            draw_distance_after_targets: state.notefield_draw_distance_after_targets(player_idx),
            column_dirs,
            reverse_scroll: state.notefield_reverse_scroll(player_idx),
        },
        visual: NotefieldVisualState {
            elapsed_screen_s: elapsed_screen,
            current_display_beat: state.current_beat_display(),
            accel,
            scroll,
            perspective,
            visual,
            appearance,
            visibility,
            mini_percent,
            spacing_multiplier: spacing_mult,
        },
        chart: NotefieldChartView {
            timing: state.timing_for_player(player_idx),
            notes: state.notes(),
            note_range: state.note_range_for_player(player_idx),
            lane_note_row_indices: from_fn(|col| state.lane_note_row_indices(col)),
            lane_hold_indices: from_fn(|col| state.lane_hold_indices(col)),
            decaying_hold_indices: state.decaying_hold_indices(),
            tap_row_hold_roll_flags: &state.chart_runtime.lane_indices.tap_row_hold_roll_flags,
            current_music_time_ns: state.current_music_time_ns(),
            visible_music_time_ns: state.visible_music_time_ns(player_idx),
            visible_beat: state.visible_beat(player_idx),
            scroll_reference_bpm: state.scroll_reference_bpm(),
            music_rate: state.music_rate(),
            note_count_stats: state.note_count_stats(player_idx),
            time_signatures,
            bpms,
            stops,
            delays,
            scrolls,
        },
        noteskin: NotefieldNoteskinView {
            base: base_noteskin,
            mine: noteskin_assets.mine_noteskin[player_idx].as_deref(),
            receptor: noteskin_assets.receptor_noteskin[player_idx].as_deref(),
            tap_explosion: tap_explosion_noteskin,
        },
        song_lua: NotefieldSongLuaView {
            note_hides: &state.song_lua_visuals().note_hides[player_idx],
            column_offsets: &state.song_lua_visuals().column_offsets[player_idx],
        },
        options: NotefieldOptions {
            frame_features: NotefieldFrameFeatures {
                measure_line_mode,
                measure_cues: profile.measure_cues,
                column_cues: profile.column_cues,
                crossover_cues: profile.crossover_cues,
                crossover_countdown: profile.column_countdown,
                column_flash: profile.column_flash_on_miss,
                error_bar: !error_bar_mask.is_empty(),
                error_bar_text: error_bar_mask.contains(profile_data::ErrorBarMask::TEXT),
                held_miss_asset: held_miss_texture.is_some(),
                combo_visible: !profile.hide_combo,
            },
            notefield_offset: [notefield_offset_x, notefield_offset_y],
            judgment_offset: [judgment_offset_x, judgment_offset_y],
            combo_offset: [combo_offset_x, combo_offset_y],
            error_bar_offset: [error_bar_offset_x, error_bar_offset_y],
            zmod_layout: zmod_layout_params(profile, style, has_judgment_texture),
            has_judgment_texture,
            error_bar_up: profile.error_bar_up,
            fallback_mini_percent: profile.mini_percent as f32,
            column_flash_compact: profile.column_flash_size
                == profile_data::ColumnFlashSize::Compact,
            column_flash_dimmed: column_flash_dimmed(profile.column_flash_brightness),
            hide_targets: profile.hide_targets,
            hold_explosion_enabled: hold_explosion_enabled(profile),
            hide_combo_explosions: profile.hide_combo_explosions,
            judgment_back: profile.judgment_back,
            show_fa_plus_window: profile.show_fa_plus_window,
            fa_plus_10ms_blue_window: profile.fa_plus_10ms_blue_window,
            split_15_10ms: profile.split_15_10ms,
            custom_fantastic_window: profile.custom_fantastic_window,
            judgment_tilt_enabled: profile.judgment_tilt,
            judgment_tilt_min_ms: profile.tilt_min_threshold_ms as f32,
            judgment_tilt_max_ms: profile.tilt_max_threshold_ms as f32,
            judgment_tilt_multiplier: profile.tilt_multiplier,
            blue_fantastic_window_s: player_blue_window_ms(state, player_idx) / 1000.0,
            error_bar_modes: ErrorBarModes {
                colorful: error_bar_mask.contains(profile_data::ErrorBarMask::COLORFUL),
                monochrome: error_bar_mask.contains(profile_data::ErrorBarMask::MONOCHROME),
                highlight: error_bar_mask.contains(profile_data::ErrorBarMask::HIGHLIGHT),
                average: error_bar_mask.contains(profile_data::ErrorBarMask::AVERAGE),
            },
            error_bar_max_window_ix: error_bar_trim_max_window_ix(profile.error_bar_trim),
            monochrome_background: profile.background_filter.is_off(),
            error_bar_multi_tick: profile.error_bar_multi_tick,
            short_average_error_bar: profile.short_average_error_bar_enabled,
            center_tick: profile.center_tick,
            error_ms_display: profile.error_ms_display,
            long_error_bar_enabled: profile.long_error_bar_enabled,
            long_error_bar_intensity: profile_data::clamp_long_error_bar_intensity(
                profile.long_error_bar_intensity,
            ),
            measure_counter: (profile.measure_counter != profile_data::MeasureCounter::None)
                .then_some(MeasureCounterOptions {
                    lookahead: profile.measure_counter_lookahead.min(4),
                    multiplier: profile.measure_counter.multiplier(),
                    vertical: profile.measure_counter_vert,
                    left: profile.measure_counter_left,
                    broken_run: profile.broken_run,
                    run_timer: profile.run_timer,
                }),
            mini_indicator_position: match profile.mini_indicator_position {
                profile_data::MiniIndicatorPosition::Default => {
                    LayoutMiniIndicatorPosition::Default
                }
                profile_data::MiniIndicatorPosition::UnderUpArrow => {
                    LayoutMiniIndicatorPosition::UnderUpArrow
                }
            },
            mini_indicator_zoom: zmod_mini_indicator_zoom(profile.mini_indicator_size),
            counter_left: profile.measure_counter_left,
        },
        capture_requests,
        arrow_effect_time_s,
    };
    let Some(prepared) = prepare_notefield(&request) else {
        return BuiltNotefield::empty(request.geometry.screen_center_x);
    };
    let options = &request.options;
    let elapsed_screen = request.visual.elapsed_screen_s;
    let perspective = request.visual.perspective;
    let visual = request.visual.visual;
    let appearance = request.visual.appearance;
    let spacing_mult = request.visual.spacing_multiplier;
    let frame_plan = prepared.frame_plan;
    let col_start = frame_plan.col_start;
    let num_cols = frame_plan.num_cols;
    let col_end = col_start + num_cols;
    actors.reserve(frame_plan.field_actor_reserve);
    hud_actors.reserve(frame_plan.hud_actor_reserve);
    let field_zoom = prepared.field_zoom;
    let scroll_speed = prepared.scroll_speed;
    let draw_distance_before_targets = request.geometry.draw_distance_before_targets;
    let draw_distance_after_targets = request.geometry.draw_distance_after_targets;
    let current_beat = prepared.current_beat;
    let measure_line_mode = if request.view.edit_beat_bars {
        MeasureLineMode::Edit
    } else {
        measure_line_mode
    };
    let field = prepared.field;
    let playfield_center_x = field.playfield_center_x;
    let layout_center_x = field.layout_center_x;
    let notefield_offset_y = field.notefield_offset_y;
    let receptor_y_normal = field.receptor_y_normal;
    let receptor_y_reverse = field.receptor_y_reverse;
    let column_reverse_percent = field.column_reverse_percent;
    let column_dirs = field.column_dirs;
    let column_receptor_ys = field.column_receptor_ys;
    let hud_layout = field.hud_layout;
    let judgment_y = hud_layout.judgment_y;
    let zmod_layout = hud_layout.zmod_layout;
    let judgment_x = field.judgment_x;
    let combo_x = field.combo_x;
    let error_bar_x = field.error_bar_x;

    let mini = prepared.mini;
    let reverse_scroll = request.geometry.reverse_scroll;
    let mc_font_name = zmod_small_combo_font(profile.combo_font);
    let judgment_zoom_mod = judgment_actor_zoom(
        mini,
        options.judgment_back,
        perspective.tilt,
        perspective.skew,
    );
    let blind_active = prepared.blind_active;

    if let Some(note_inputs) = prepared.notes.as_ref() {
        let ns = note_inputs.base;
        let mine_ns = note_inputs.mine;
        let target_arrow_px = note_inputs.target_arrow_px;
        let scale_sprite =
            |size: [i32; 2]| -> [f32; 2] { scale_sprite_to_arrow(size, target_arrow_px) };
        let scale_mine_slot = |slot: &SpriteSlot| -> [f32; 2] {
            // ITG NoteDisplay::DrawTap uses SetPRZForActor zoom for TapMine and does not
            // normalize Def.Model mine meshes to an arrow texture target size. Preserve
            // native model geometry scale here; keep sprite mines on texture-size scaling.
            if let Some(model) = slot.model.as_ref() {
                let model_size = model.size();
                if model_size[0] > f32::EPSILON && model_size[1] > f32::EPSILON {
                    return [model_size[0] * field_zoom, model_size[1] * field_zoom];
                }
            }
            scale_sprite(slot.size())
        };
        // The column swap for Step's hold-turn section is handled at the player bundle
        // level. Keep the actual note/receptor/ghost visuals on the normal noteskin
        // path here; applying an extra local Y turn breaks model-backed arrows and hit
        // effects.
        let note_rotation_y = 0.0_f32;
        let prefer_sprite_note_path = false;
        let flat_tap_face_rotation_y = 0.0_f32;
        let beat_push = note_inputs.beat_factor;
        let col_offsets = note_inputs.col_offsets;
        let invert_distances = note_inputs.invert_distances;
        let tornado_bounds = note_inputs.tornado_bounds;
        let note_display_time_scale = note_inputs.note_display_time_scale;
        let travel = &note_inputs.travel;
        let visible_row_range = travel.visible_row_range();
        let (note_start, note_end) = request.chart.note_range;
        let lane_center_x_from_travel = |local_col: usize, travel_offset: f32| -> f32 {
            playfield_center_x
                + note_x_offset(
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
                visual_effect_params(&visual, local_col).bumpy,
                visual.bumpy_offset,
                visual.bumpy_period,
            )
        };
        let world_z_for_adjusted_travel = |local_col: usize, travel_offset: f32| -> f32 {
            note_world_z_for_bumpy(
                travel_offset,
                visual_effect_params(&visual, local_col).bumpy,
                visual.bumpy_offset,
                visual.bumpy_period,
            )
        };
        let measure_column_xs = note_inputs.measure_column_xs;
        compose_measure_lines(
            actors,
            MeasureComposeRequest {
                mode: measure_line_mode,
                show_cues: options.frame_features.measure_cues,
                style,
                column_xs: &measure_column_xs,
                column_dirs: &column_dirs,
                column_receptor_ys: &column_receptor_ys,
                num_cols,
                spacing_multiplier: spacing_mult,
                field_zoom,
                playfield_center_x,
                screen_height: screen_height(),
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

        let noteskin_sprite_source =
            |slot: &SpriteSlot| slot.texture_key_handle().into_sprite_source();
        let feedback_frame = NotefieldFeedbackFrameView {
            column_cues: options
                .frame_features
                .column_cues
                .then(|| state.column_cues(player_idx)),
            crossover_cues: options
                .frame_features
                .crossover_cues
                .then(|| state.crossover_cues(player_idx)),
            column_flashes: options
                .frame_features
                .column_flash
                .then(|| state.column_flashes_for_columns(col_start, num_cols)),
            tap_explosions: state.tap_explosions_for_columns(col_start, num_cols),
            mine_explosions: state.mine_explosions_for_columns(col_start, num_cols),
            lanes: from_fn(|local_col| {
                if local_col >= num_cols {
                    return NotefieldLaneFeedback::default();
                }
                let col = col_start + local_col;
                NotefieldLaneFeedback {
                    active_hold: state.active_hold(col),
                    receptor_bop_zoom: state.receptor_bop_zoom(col),
                    receptor_press_visual: state.receptor_glow_visual_for_col(col),
                }
            }),
            countdown_font: mc_font_name,
            countdown_text: cached_int_i32,
        };
        compose_notefield_feedback(
            actors,
            hud_actors,
            &mut model_cache,
            &request,
            &prepared,
            &feedback_frame,
            &noteskin_sprite_source,
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

            // Keep gameplay-state eligibility in the adapter while the canonical
            // planner owns the resulting dynamic/static head beat.
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
            // ITG gates reverse noteskin metrics by lane reverse state, not by
            // temporary visual inversion from scroll gimmicks.
            let lane_reverse = col_dir < 0.0;
            let active_state = state
                .active_hold(note.column)
                .filter(|h| h.note_index == note_index);
            // ITG keeps early-hit hold heads scrolling as inactive until the head
            // reaches the receptor row; only then does hold-active rendering clamp.
            let (engaged, use_active) =
                hold_head_render_flags(active_state, current_beat, note.beat);
            let visuals =
                ns.hold_visuals_for_col(local_col, matches!(note.note_type, NoteType::Roll));
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
                screen_height: screen_height(),
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
            let col_bumpy = visual_effect_params(&visual, local_col).bumpy;
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
                &mut actors,
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
                    top_cap_uv_translation: ns.part_uv_translation(
                        hold_parts.topcap,
                        note.beat,
                        false,
                    ),
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
                    screen_height: screen_height(),
                    body_z: style.actors.hold_body_z,
                    cap_z: style.actors.hold_cap_z,
                    glow_z: style.actors.hold_glow_z,
                },
                &sample_hold_path,
                &noteskin_sprite_source,
            ) == HoldComposeControl::AbortHold
            {
                return;
            }
            let should_draw_hold_head = true;
            let head_draw_y = head_anchor_y;
            let head_draw_delta = (head_draw_y - receptor_draw_y) * dir;
            if should_draw_hold_head
                && head_draw_delta >= -draw_distance_after_targets
                && head_draw_delta <= draw_distance_before_targets
            {
                let head_alpha = actor_alpha_for_travel(local_col, head_anchor_travel);
                let head_glow = glow_for_travel(local_col, head_anchor_travel);
                if head_alpha <= f32::EPSILON && head_glow <= f32::EPSILON {
                    return;
                }
                let hold_head_rot =
                    calc_note_rotation_z(visual, note.beat, current_beat, true, local_col);
                let note_idx = local_col * NUM_QUANTIZATIONS + note.quantization_idx as usize;
                let head_center_x = if (head_draw_y - receptor_draw_y).abs() <= 0.5 {
                    receptor_center_x
                } else {
                    lane_center_x_from_travel(local_col, head_anchor_travel)
                };
                let head_center = [head_center_x, head_draw_y];
                let head_world_z = world_z_for_raw_travel(local_col, head_anchor_travel);
                let elapsed = elapsed_screen;
                let hold_head_translation =
                    ns.part_uv_translation(hold_parts.head, note.beat, false);
                let head_slot = head_slot.and_then(|slot| {
                    let draw = song_lua_note_model_draw(
                        slot.model_draw_at(elapsed, current_beat),
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
                    let frame = head_slot.frame_index_from_phase(hold_part_phase);
                    let uv_elapsed = if head_slot.model.is_some() {
                        hold_part_phase
                    } else {
                        elapsed
                    };
                    let uv = translated_uv_rect(
                        head_slot.uv_for_frame_at(frame, uv_elapsed),
                        hold_head_translation,
                    );
                    let local_offset = [draw.pos[0] * note_scale, draw.pos[1] * note_scale];
                    let local_offset_rot_sin_cos = head_slot.base_rot_sin_cos();
                    let model_center = if head_slot.model.is_some() {
                        let [sin_r, cos_r] = local_offset_rot_sin_cos;
                        let offset = [
                            local_offset[0] * cos_r - local_offset[1] * sin_r,
                            local_offset[0] * sin_r + local_offset[1] * cos_r,
                        ];
                        [head_center[0] + offset[0], head_center[1] + offset[1]]
                    } else {
                        head_center
                    };
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
                    compose_note_layer(
                        &mut actors,
                        &mut model_cache,
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
                            model_rotation_z_deg: -head_slot.def.rotation_deg as f32
                                + hold_head_rot,
                            sprite_rotation_z_deg: draw.rot[2] - head_slot.def.rotation_deg as f32
                                + hold_head_rot,
                            tint: color,
                            glow_alpha: head_glow,
                            blend,
                            z: style.actors.note_z,
                            world_z: head_world_z,
                            prefer_sprite: prefer_sprite_note_path,
                        },
                        &noteskin_sprite_source,
                    );
                } else if let Some(note_slots) = head_layers
                    .or_else(|| ns.note_layers.get(note_idx).map(|layers| layers.as_ref()))
                {
                    let note_scale = hold_note_scale;
                    for note_slot in note_slots.iter() {
                        let draw = song_lua_note_model_draw(
                            note_slot.model_draw_at(elapsed, current_beat),
                            note_rotation_y,
                        );
                        if !draw.visible {
                            continue;
                        }
                        let frame = note_slot.frame_index_from_phase(hold_part_phase);
                        let uv_elapsed = if note_slot.model.is_some() {
                            hold_part_phase
                        } else {
                            elapsed
                        };
                        let uv = translated_uv_rect(
                            note_slot.uv_for_frame_at(frame, uv_elapsed),
                            hold_head_translation,
                        );
                        let base_size = note_slot_base_size(note_slot, note_scale);
                        let offset_scale = note_scale;
                        let local_offset = [draw.pos[0] * offset_scale, draw.pos[1] * offset_scale];
                        let local_offset_rot_sin_cos = note_slot.base_rot_sin_cos();
                        let model_center = if note_slot.model.is_some() {
                            let [sin_r, cos_r] = local_offset_rot_sin_cos;
                            let offset = [
                                local_offset[0] * cos_r - local_offset[1] * sin_r,
                                local_offset[0] * sin_r + local_offset[1] * cos_r,
                            ];
                            [head_center[0] + offset[0], head_center[1] + offset[1]]
                        } else {
                            head_center
                        };
                        let size = [
                            base_size[0] * draw.zoom[0].max(0.0),
                            base_size[1] * draw.zoom[1].max(0.0),
                        ];
                        if size[0] <= f32::EPSILON || size[1] <= f32::EPSILON {
                            continue;
                        }
                        let color = [
                            draw.tint[0] * hold_diffuse[0],
                            draw.tint[1] * hold_diffuse[1],
                            draw.tint[2] * hold_diffuse[2],
                            draw.tint[3] * hold_diffuse[3] * head_alpha,
                        ];
                        let layer_z = i32::from(style.actors.note_z);
                        let blend = if draw.blend_add {
                            BlendMode::Add
                        } else {
                            BlendMode::Alpha
                        };
                        compose_note_layer(
                            &mut actors,
                            &mut model_cache,
                            NoteLayerRequest {
                                slot: note_slot,
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
                                model_rotation_z_deg: -note_slot.def.rotation_deg as f32
                                    + hold_head_rot,
                                sprite_rotation_z_deg: draw.rot[2]
                                    - note_slot.def.rotation_deg as f32
                                    + hold_head_rot,
                                tint: color,
                                glow_alpha: head_glow,
                                blend,
                                z: layer_z as i16,
                                world_z: head_world_z,
                                prefer_sprite: prefer_sprite_note_path,
                            },
                            &noteskin_sprite_source,
                        );
                    }
                } else if let Some(note_slot) = ns.notes.get(note_idx) {
                    let frame = note_slot.frame_index_from_phase(hold_part_phase);
                    let uv_elapsed = if note_slot.model.is_some() {
                        hold_part_phase
                    } else {
                        elapsed
                    };
                    let uv = translated_uv_rect(
                        note_slot.uv_for_frame_at(frame, uv_elapsed),
                        hold_head_translation,
                    );
                    let size = scale_sprite_to_arrow(note_slot.size(), hold_head_target_arrow_px);
                    let draw = song_lua_note_model_draw(
                        note_slot.model_draw_at(elapsed, current_beat),
                        note_rotation_y,
                    );
                    compose_note_layer(
                        &mut actors,
                        &mut model_cache,
                        NoteLayerRequest {
                            slot: note_slot,
                            draw,
                            model_center: head_center,
                            sprite_center: head_center,
                            size,
                            uv,
                            rotation_y_deg: flat_tap_face_rotation_y,
                            model_rotation_z_deg: -note_slot.def.rotation_deg as f32
                                + hold_head_rot,
                            sprite_rotation_z_deg: -note_slot.def.rotation_deg as f32
                                + hold_head_rot,
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
                        &noteskin_sprite_source,
                    );
                }
            }
        };
        for local_col in 0..num_cols {
            let col = col_start + local_col;
            for_each_visible_hold_index(
                request.chart.lane_hold_indices[col],
                request.chart.notes,
                visible_row_range,
                |note_index| render_hold(note_index),
            );
        }
        let extra_hold_indices = state
            .active_hold_note_indices()
            .chain(request.chart.decaying_hold_indices.iter().copied())
            .filter(|&idx| {
                idx >= note_start
                    && idx < note_end
                    && !hold_overlaps_visible_window(idx, request.chart.notes, visible_row_range)
            });
        for note_index in extra_hold_indices {
            render_hold(note_index);
        }
        let elapsed = elapsed_screen;
        let note_display_time = elapsed * note_display_time_scale;
        let mine_fill_phase = current_beat.rem_euclid(1.0);
        let draw_hold_same_row = ns.note_display_metrics.draw_hold_head_for_taps_on_same_row;
        let draw_roll_same_row = ns.note_display_metrics.draw_roll_head_for_taps_on_same_row;
        let tap_same_row_means_hold = ns.note_display_metrics.tap_hold_roll_on_row_means_hold;
        // Visible tap and mine notes
        for col_idx in 0..num_cols {
            let col = col_start + col_idx;
            let column_note_indices = request.chart.lane_note_row_indices[col];
            let dir = column_dirs[col_idx];
            let receptor_y_lane = column_receptor_ys[col_idx];
            let fill_slot = mine_ns.mines.get(col_idx).and_then(|slot| slot.as_ref());
            let fill_gradient_slot = mine_ns
                .mine_fill_slots
                .get(col_idx)
                .and_then(|slot| slot.as_ref());
            let frame_slot = mine_ns
                .mine_frames
                .get(col_idx)
                .and_then(|slot| slot.as_ref());
            for_each_visible_note_index(
                column_note_indices,
                request.chart.notes,
                // ITGmania gets tap candidates from a row-keyed TrackMap via
                // GetTapNoteRangeInclusive, then NoteDisplay::IsOnScreen
                // performs the exact ArrowEffects visibility check below.
                visible_row_range,
                |note_index| {
                    let note = &request.chart.notes[note_index];
                    if matches!(note.note_type, NoteType::Hold | NoteType::Roll) {
                        return;
                    }
                    if song_lua_note_hidden(request.song_lua.note_hides, col_idx, note.beat) {
                        return;
                    }
                    if !note.is_fake {
                        if matches!(note.note_type, NoteType::Mine) {
                            if mine_hides_after_resolution(note.mine_result) {
                                return;
                            }
                        } else if note.result.is_some()
                            && state.row_hides_completed_note(player_idx, note.row_index)
                        {
                            return;
                        }
                    }
                    let raw_travel_offset = travel.raw_note(note, false);
                    let travel_offset = travel.adjusted(raw_travel_offset);
                    let y_pos = travel.lane_y(col_idx, receptor_y_lane, dir, raw_travel_offset);
                    let delta = travel_offset;
                    if delta < -draw_distance_after_targets || delta > draw_distance_before_targets
                    {
                        return;
                    }
                    let note_alpha = actor_alpha_for_travel(col_idx, raw_travel_offset);
                    let note_glow = glow_for_travel(col_idx, raw_travel_offset);
                    if note_alpha <= f32::EPSILON && note_glow <= f32::EPSILON {
                        return;
                    }
                    let column_center_x = lane_center_x_from_travel(col_idx, raw_travel_offset);
                    let note_world_z = world_z_for_adjusted_travel(col_idx, travel_offset);
                    let col_effect_zoom = arrow_effect_zoom(&visual, col_idx, travel_offset);
                    let col_note_scale = field_zoom * col_effect_zoom;
                    let col_target_arrow_px = target_arrow_px * col_effect_zoom;
                    let scale_mine_slot_for_note = |slot: &SpriteSlot| -> [f32; 2] {
                        let size = scale_mine_slot(slot);
                        [size[0] * col_effect_zoom, size[1] * col_effect_zoom]
                    };
                    let note_rot =
                        calc_note_rotation_z(visual, note.beat, current_beat, false, col_idx);
                    if matches!(note.note_type, NoteType::Mine) {
                        if fill_slot.is_none() && frame_slot.is_none() {
                            return;
                        }
                        let mine_note_beat = note.beat;
                        let mine_uv_phase =
                            mine_ns.tap_mine_uv_phase(elapsed, current_beat, mine_note_beat);
                        let mine_translation =
                            mine_ns.part_uv_translation(mine_part(), mine_note_beat, false);
                        let circle_reference = frame_slot
                            .map(|slot| scale_mine_slot_for_note(slot))
                            .or_else(|| fill_slot.map(|slot| scale_mine_slot_for_note(slot)))
                            .unwrap_or([
                                TARGET_ARROW_PIXEL_SIZE * col_note_scale,
                                TARGET_ARROW_PIXEL_SIZE * col_note_scale,
                            ]);
                        compose_mine_layers(
                            &mut actors,
                            &mut model_cache,
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
                                note_rotation_z_deg: note_rot,
                                alpha: note_alpha,
                                glow_alpha: note_glow,
                                note_z: style.actors.note_z,
                                world_z: note_world_z,
                                prefer_sprite: prefer_sprite_note_path,
                            },
                            &scale_mine_slot_for_note,
                            &noteskin_sprite_source,
                        );
                        return;
                    }
                    let tap_note_part = tap_part_for_note_type(note.note_type);
                    let tap_row_flags = request.chart.tap_row_flags(note_index);
                    if let Some(replacement_head) = tap_replacement_head(
                        note.note_type,
                        tap_row_flags & 0b01 != 0,
                        tap_row_flags & 0b10 != 0,
                        draw_hold_same_row,
                        draw_roll_same_row,
                        tap_same_row_means_hold,
                    ) {
                        let visuals = ns.hold_visuals_for_col(col_idx, replacement_head.is_roll);
                        let part = replacement_head.part;
                        if let Some(head_slots) = visuals
                            .head_inactive_layers
                            .as_deref()
                            .or(visuals.head_active_layers.as_deref())
                        {
                            let head_phase =
                                ns.part_uv_phase(part, elapsed, current_beat, note.beat);
                            let head_translation = ns.part_uv_translation(part, note.beat, false);
                            let note_scale = col_note_scale;
                            let center = [column_center_x, y_pos];
                            for head_slot in head_slots.iter() {
                                let draw = song_lua_note_model_draw(
                                    head_slot.model_draw_at(elapsed, current_beat),
                                    note_rotation_y,
                                );
                                if !draw.visible {
                                    continue;
                                }
                                let note_frame = head_slot.frame_index_from_phase(head_phase);
                                let uv_elapsed = if head_slot.model.is_some() {
                                    head_phase
                                } else {
                                    elapsed
                                };
                                let note_uv = translated_uv_rect(
                                    head_slot.uv_for_frame_at(note_frame, uv_elapsed),
                                    head_translation,
                                );
                                let base_size = note_slot_base_size(head_slot, note_scale);
                                let local_offset =
                                    [draw.pos[0] * note_scale, draw.pos[1] * note_scale];
                                let local_offset_rot_sin_cos = head_slot.base_rot_sin_cos();
                                let model_center = if head_slot.model.is_some() {
                                    let [sin_r, cos_r] = local_offset_rot_sin_cos;
                                    let offset = [
                                        local_offset[0] * cos_r - local_offset[1] * sin_r,
                                        local_offset[0] * sin_r + local_offset[1] * cos_r,
                                    ];
                                    [center[0] + offset[0], center[1] + offset[1]]
                                } else {
                                    center
                                };
                                let note_size = [
                                    base_size[0] * draw.zoom[0].max(0.0),
                                    base_size[1] * draw.zoom[1].max(0.0),
                                ];
                                if note_size[0] <= f32::EPSILON || note_size[1] <= f32::EPSILON {
                                    continue;
                                }
                                let blend = if draw.blend_add {
                                    BlendMode::Add
                                } else {
                                    BlendMode::Alpha
                                };
                                let color = [
                                    draw.tint[0],
                                    draw.tint[1],
                                    draw.tint[2],
                                    draw.tint[3] * note_alpha,
                                ];
                                compose_note_layer(
                                    &mut actors,
                                    &mut model_cache,
                                    NoteLayerRequest {
                                        slot: head_slot,
                                        draw,
                                        model_center,
                                        sprite_center: offset_center(
                                            center,
                                            local_offset,
                                            local_offset_rot_sin_cos,
                                        ),
                                        size: note_size,
                                        uv: note_uv,
                                        rotation_y_deg: flat_tap_face_rotation_y,
                                        model_rotation_z_deg: -head_slot.def.rotation_deg as f32
                                            + note_rot,
                                        sprite_rotation_z_deg: draw.rot[2]
                                            - head_slot.def.rotation_deg as f32
                                            + note_rot,
                                        tint: color,
                                        glow_alpha: note_glow,
                                        blend,
                                        z: style.actors.note_z,
                                        world_z: note_world_z,
                                        prefer_sprite: prefer_sprite_note_path,
                                    },
                                    &noteskin_sprite_source,
                                );
                            }
                            return;
                        }
                        if let Some(head_slot) = visuals
                            .head_inactive
                            .as_ref()
                            .or(visuals.head_active.as_ref())
                        {
                            let part = replacement_head.part;
                            let head_phase =
                                ns.part_uv_phase(part, elapsed, current_beat, note.beat);
                            let head_translation = ns.part_uv_translation(part, note.beat, false);
                            let note_frame = head_slot.frame_index_from_phase(head_phase);
                            let uv_elapsed = if head_slot.model.is_some() {
                                head_phase
                            } else {
                                elapsed
                            };
                            let note_uv = translated_uv_rect(
                                head_slot.uv_for_frame_at(note_frame, uv_elapsed),
                                head_translation,
                            );
                            let note_scale = col_note_scale;
                            let note_size = note_slot_base_size(head_slot, note_scale);
                            let center = [column_center_x, y_pos];
                            let draw = song_lua_note_model_draw(
                                head_slot.model_draw_at(elapsed, current_beat),
                                note_rotation_y,
                            );
                            compose_note_layer(
                                &mut actors,
                                &mut model_cache,
                                NoteLayerRequest {
                                    slot: head_slot,
                                    draw,
                                    model_center: center,
                                    sprite_center: center,
                                    size: note_size,
                                    uv: note_uv,
                                    rotation_y_deg: flat_tap_face_rotation_y,
                                    model_rotation_z_deg: -head_slot.def.rotation_deg as f32
                                        + note_rot,
                                    sprite_rotation_z_deg: -head_slot.def.rotation_deg as f32
                                        + note_rot,
                                    tint: [1.0, 1.0, 1.0, note_alpha],
                                    glow_alpha: note_glow,
                                    blend: BlendMode::Alpha,
                                    z: style.actors.note_z,
                                    world_z: note_world_z,
                                    prefer_sprite: prefer_sprite_note_path,
                                },
                                &noteskin_sprite_source,
                            );
                            return;
                        }
                    }
                    let note_idx = col_idx * NUM_QUANTIZATIONS + note.quantization_idx as usize;
                    let tap_note_translation =
                        ns.part_uv_translation(tap_note_part, note.beat, false);
                    let lift_layers = if note.note_type == NoteType::Lift {
                        ns.lift_note_layers.get(note_idx)
                    } else {
                        None
                    };
                    if let Some(note_slots) = lift_layers.or_else(|| ns.note_layers.get(note_idx)) {
                        let note_center = [column_center_x, y_pos];
                        let note_uv_phase =
                            ns.part_uv_phase(tap_note_part, elapsed, current_beat, note.beat);
                        let note_scale = col_note_scale;
                        for note_slot in note_slots.iter() {
                            let draw = song_lua_note_model_draw(
                                note_slot.model_draw_at(elapsed, current_beat),
                                note_rotation_y,
                            );
                            if !draw.visible {
                                continue;
                            }
                            let note_frame = note_slot.frame_index_from_phase(note_uv_phase);
                            let uv_elapsed = if note_slot.model.is_some() {
                                note_uv_phase
                            } else {
                                elapsed
                            };
                            let note_uv = translated_uv_rect(
                                note_slot.uv_for_frame_at(note_frame, uv_elapsed),
                                tap_note_translation,
                            );
                            let base_size = note_slot_base_size(note_slot, note_scale);
                            let offset_scale = note_scale;
                            let local_offset =
                                [draw.pos[0] * offset_scale, draw.pos[1] * offset_scale];
                            let local_offset_rot_sin_cos = note_slot.base_rot_sin_cos();
                            let model_center = if note_slot.model.is_some() {
                                let [sin_r, cos_r] = local_offset_rot_sin_cos;
                                let offset = [
                                    local_offset[0] * cos_r - local_offset[1] * sin_r,
                                    local_offset[0] * sin_r + local_offset[1] * cos_r,
                                ];
                                [note_center[0] + offset[0], note_center[1] + offset[1]]
                            } else {
                                note_center
                            };
                            let note_size = [
                                base_size[0] * draw.zoom[0].max(0.0),
                                base_size[1] * draw.zoom[1].max(0.0),
                            ];
                            if note_size[0] <= f32::EPSILON || note_size[1] <= f32::EPSILON {
                                continue;
                            }
                            let layer_z = i32::from(style.actors.note_z);
                            let blend = if draw.blend_add {
                                BlendMode::Add
                            } else {
                                BlendMode::Alpha
                            };
                            let color = [
                                draw.tint[0],
                                draw.tint[1],
                                draw.tint[2],
                                draw.tint[3] * note_alpha,
                            ];
                            compose_note_layer(
                                &mut actors,
                                &mut model_cache,
                                NoteLayerRequest {
                                    slot: note_slot,
                                    draw,
                                    model_center,
                                    sprite_center: offset_center(
                                        note_center,
                                        local_offset,
                                        local_offset_rot_sin_cos,
                                    ),
                                    size: note_size,
                                    uv: note_uv,
                                    rotation_y_deg: flat_tap_face_rotation_y,
                                    model_rotation_z_deg: -note_slot.def.rotation_deg as f32
                                        + note_rot,
                                    sprite_rotation_z_deg: draw.rot[2]
                                        - note_slot.def.rotation_deg as f32
                                        + note_rot,
                                    tint: color,
                                    glow_alpha: note_glow,
                                    blend,
                                    z: layer_z as i16,
                                    world_z: note_world_z,
                                    prefer_sprite: prefer_sprite_note_path,
                                },
                                &noteskin_sprite_source,
                            );
                        }
                    } else if let Some(note_slot) = ns.notes.get(note_idx) {
                        let note_uv_phase =
                            ns.part_uv_phase(tap_note_part, elapsed, current_beat, note.beat);
                        let note_frame = note_slot.frame_index_from_phase(note_uv_phase);
                        let uv_elapsed = if note_slot.model.is_some() {
                            note_uv_phase
                        } else {
                            elapsed
                        };
                        let note_uv = translated_uv_rect(
                            note_slot.uv_for_frame_at(note_frame, uv_elapsed),
                            tap_note_translation,
                        );
                        let note_size =
                            scale_sprite_to_arrow(note_slot.size(), col_target_arrow_px);
                        let center = [column_center_x, y_pos];
                        let draw = song_lua_note_model_draw(
                            note_slot.model_draw_at(elapsed, current_beat),
                            note_rotation_y,
                        );
                        compose_note_layer(
                            &mut actors,
                            &mut model_cache,
                            NoteLayerRequest {
                                slot: note_slot,
                                draw,
                                model_center: center,
                                sprite_center: center,
                                size: note_size,
                                uv: note_uv,
                                rotation_y_deg: flat_tap_face_rotation_y,
                                model_rotation_z_deg: -note_slot.def.rotation_deg as f32 + note_rot,
                                sprite_rotation_z_deg: -note_slot.def.rotation_deg as f32
                                    + note_rot,
                                tint: [1.0, 1.0, 1.0, note_alpha],
                                glow_alpha: note_glow,
                                blend: BlendMode::Alpha,
                                z: style.actors.note_z,
                                world_z: note_world_z,
                                prefer_sprite: prefer_sprite_note_path,
                            },
                            &noteskin_sprite_source,
                        );
                    }
                },
            );
        }
    }
    // Simply Love: ScreenGameplay underlay/PerPlayer/NoteField/DisplayMods.lua
    // shows the current mod string for 5s, then decelerates out over 0.5s.
    // Arrow Cloud/zmod add a CMod warning below this block for ITL no-CMod charts.
    if !request.view.hide_display_mods {
        // Simply Love DisplayMods.lua uses sleep(5), but ScreenGameplay in/default.lua
        // keeps a full-screen intro cover up for 2.0s. Since deadsync's gameplay
        // in-transition cover is shorter, subtract the exact missing cover time so
        // the *visible* mods duration matches ITG/SL.
        const SL_DISPLAY_MODS_HOLD_S: f32 = 5.0;
        const SL_GAMEPLAY_IN_COVER_S: f32 = 2.0;
        const MODS_FADE_S: f32 = 0.5;
        let hold_adjust = (SL_GAMEPLAY_IN_COVER_S - TRANSITION_IN_DURATION).max(0.0);
        let mods_hold_s = (SL_DISPLAY_MODS_HOLD_S - hold_adjust).max(0.0);

        let alpha = if elapsed_screen <= mods_hold_s {
            1.0
        } else if elapsed_screen < mods_hold_s + MODS_FADE_S {
            let t = ((elapsed_screen - mods_hold_s) / MODS_FADE_S).clamp(0.0, 1.0);
            let decelerate = 1.0 - (1.0 - t) * (1.0 - t);
            1.0 - decelerate
        } else {
            0.0
        };

        if alpha > 0.0 {
            let mods_text = gameplay_mods_text(state, player_idx);
            let mods_line_y = screen_height() * 0.25 * 1.3 + notefield_offset_y;
            let mods_line_count = mods_text
                .split(", ")
                .filter(|part| !part.is_empty())
                .count()
                .max(1) as f32;
            if !mods_text.is_empty() {
                hud_actors.push(act!(text:
                    font("miso"): settext(mods_text):
                    align(0.5, 0.0): xy(playfield_center_x, mods_line_y):
                    zoom(DISPLAY_MODS_ZOOM): wrapwidthpixels(DISPLAY_MODS_WRAP_WIDTH_PX): horizalign(center):
                    shadowcolor(0.0, 0.0, 0.0, 1.0):
                    shadowlength(1.0):
                    diffuse(1.0, 1.0, 1.0, alpha):
                    z(84)
                ));
            }
            if warn_cmod_for_itl_chart {
                let warning_y = mods_line_y + DISPLAY_MODS_LINE_STEP * mods_line_count;
                hud_actors.push(act!(quad:
                    align(0.5, 0.5):
                    xy(playfield_center_x, warning_y):
                    setsize(DISPLAY_MODS_WARNING_W, DISPLAY_MODS_WARNING_H):
                    diffuse(0.0, 0.0, 0.0, 0.8 * alpha):
                    z(84)
                ));
                hud_actors.push(act!(text:
                    font("miso"): settext("CMod On"):
                    align(0.5, 0.5): xy(playfield_center_x, warning_y):
                    zoom(DISPLAY_MODS_WARNING_ZOOM):
                    diffuse(1.0, 0.0, 0.0, alpha):
                    z(85)
                ));
            }
        }
    }

    let combo_capture_start = hud_actors.len();
    let show_combo =
        !request.view.hide_combo && !blind_active && options.frame_features.combo_visible;
    let milestone_assets = (show_combo
        && !options.hide_combo_explosions
        && !p.combo_milestones.is_empty())
    .then(|| {
        let combo_splode_tex = assets::visual_styles::combo_100milestone_splode_texture_key();
        let combo_minisplode_tex =
            assets::visual_styles::combo_100milestone_minisplode_texture_key();
        let combo_swoosh_tex = assets::visual_styles::combo_1000milestone_swoosh_texture_key();
        ComboMilestoneAssets {
            burst: "combo_explosion.png".into_sprite_source(),
            hundred: combo_splode_tex.into_sprite_source(),
            hundred_mini: combo_minisplode_tex.into_sprite_source(),
            thousand: combo_swoosh_tex.into_sprite_source(),
            hundred_zoom_scale: assets::visual_styles::effect_zoom_scale(combo_splode_tex),
            hundred_mini_zoom_scale: assets::visual_styles::effect_zoom_scale(combo_minisplode_tex),
            thousand_zoom_scale: assets::visual_styles::effect_zoom_scale(combo_swoosh_tex),
        }
    });
    let player_color = milestone_assets
        .is_some()
        .then(|| color::decorative_rgba(state.player_color_index()))
        .unwrap_or([1.0; 4]);
    let combo_color = (show_combo
        && p.miss_combo < style.combo_feedback.threshold
        && p.combo >= style.combo_feedback.threshold)
        .then(|| zmod_resolved_combo_color(state, p, profile, player_idx))
        .unwrap_or([1.0; 4]);
    compose_combo_feedback(
        &mut hud_actors,
        ComboFeedbackRequest {
            style: style.combo_feedback,
            show: show_combo,
            milestone_assets: milestone_assets.as_ref(),
            milestones: &p.combo_milestones,
            combo: p.combo,
            miss_combo: p.miss_combo,
            number_xy: [combo_x, zmod_layout.combo_y],
            milestone_xy: [playfield_center_x, zmod_layout.combo_y],
            mini,
            player_color,
            combo_color,
            font: zmod_combo_font_name(profile.combo_font),
            number_text: cached_int_u32,
        },
    );
    let combo_actors = request
        .capture_requests
        .combo
        .then(|| share_actor_range(&mut hud_actors, combo_capture_start))
        .flatten();

    let show_error_bar = options.frame_features.error_bar;
    let error_bar_y = hud_layout.error_bar_y;
    let error_bar_max_h = hud_layout.error_bar_max_h;
    let mut average_bar_y = 0.0_f32;
    for y in column_receptor_ys.iter().take(num_cols) {
        average_bar_y += *y;
    }
    if num_cols > 0 {
        average_bar_y /= num_cols as f32;
    }
    let error_bar_style = style.error_bar;
    let timing_windows_s = state.timing_profile_windows_s();
    compose_error_bar(
        &mut hud_actors,
        ErrorBarComposeRequest {
            style: error_bar_style,
            modes: options.error_bar_modes,
            state: ErrorBarState {
                mono_ticks: &p.error_bar_mono_ticks,
                color_ticks: &p.error_bar_color_ticks,
                average_ticks: &p.error_bar_avg_ticks,
                color_bar_started_at: p.error_bar_color_bar_started_at,
                average_bar_started_at: p.error_bar_avg_bar_started_at,
                flash_early: &p.error_bar_color_flash_early,
                flash_late: &p.error_bar_color_flash_late,
            },
            visible: !blind_active && show_error_bar,
            elapsed_s: elapsed_screen,
            position: [error_bar_x, error_bar_y],
            average_y: average_bar_y,
            max_height: error_bar_max_h,
            mini,
            timing_windows_s,
            blue_fantastic_window_s: Some(options.blue_fantastic_window_s),
            max_window_ix: options.error_bar_max_window_ix,
            show_fa_plus: options.show_fa_plus_window,
            judgment_back: options.judgment_back,
            monochrome_background: options.monochrome_background,
            multi_tick: options.error_bar_multi_tick,
            short_average: options.short_average_error_bar,
            center_tick: options.center_tick,
            has_error_bar: show_error_bar,
            offset_indicator: p.offset_indicator_text,
            offset_indicator_visible: !blind_active && options.error_ms_display,
            offset_indicator_position: [playfield_center_x, screen_center_y() + notefield_offset_y],
            offset_text: cached_offset_ms,
            long_average_tick: p.error_bar_long_avg_tick,
            long_average_visible: !blind_active
                && show_error_bar
                && options.long_error_bar_enabled
                && p.error_bar_long_avg_visible,
            long_average_intensity: options.long_error_bar_intensity,
            text: p.error_bar_text,
            text_visible: !blind_active && show_error_bar && options.frame_features.error_bar_text,
            text_label: cached_error_bar_text_label,
        },
    );

    if let Some(counter) = options.measure_counter {
        let display_beat = request.visual.current_display_beat;
        compose_counter_hud(
            hud_actors,
            CounterHudRequest {
                style: style.counter_hud,
                segments: state.measure_counter_segments(player_idx),
                current_beat,
                current_display_beat: display_beat,
                current_bpm: state.timing().get_bpm_for_beat(display_beat),
                music_rate: request.chart.music_rate,
                lookahead: counter.lookahead,
                multiplier: counter.multiplier,
                vertical: counter.vertical,
                left: counter.left,
                broken_run: counter.broken_run,
                run_timer: counter.run_timer,
                measure_counter_y: zmod_layout.measure_counter_y,
                subtractive_scoring_y: zmod_layout.subtractive_scoring_y,
                playfield_center_x,
                field_zoom,
                font: mc_font_name,
                counter_text: cached_zmod_measure_counter_text,
                timer_text: zmod_run_timer_fmt,
            },
        );
    }

    if let Some((text, color)) = zmod_mini_indicator_text(state, p, profile, player_idx) {
        compose_mini_indicator(
            hud_actors,
            MiniIndicatorRequest {
                style: style.mini_indicator,
                text,
                color,
                failed: p.is_failing || p.life <= 0.0,
                position: options.mini_indicator_position,
                counter_left: options.counter_left,
                playfield_center_x,
                field_zoom,
                layout_add_x: zmod_layout.subtractive_scoring_addx,
                y: zmod_layout.subtractive_scoring_y,
                zoom: options.mini_indicator_zoom,
                font: mc_font_name,
            },
        );
    }

    let judgment_capture_start = hud_actors.len();
    let held_misses = state.held_miss_judgments_for_columns(col_start, num_cols);
    let hold_judgments = state.hold_judgments_for_columns(col_start, num_cols);
    let mut tap = None;
    let mut tap_sprite = None;
    if !blind_active
        && let Some(render) = p.last_judgment.as_ref()
        && let Some(texture) = judgment_texture
    {
        let (frame_cols, frame_rows) = assets::parse_sprite_sheet_dims(texture.key.as_ref());
        let (frame_row, overlay_row) =
            tap_judgment_rows(options, &render.judgment, frame_rows as usize);
        tap = Some(TapJudgmentFeedback {
            render,
            frame_row,
            overlay_row,
            rotation_deg: judgment_tilt_rotation_deg(options, &render.judgment),
        });
        tap_sprite = Some(TapJudgmentSprite {
            source: texture.texture_key_handle().into_sprite_source(),
            frame_size: judgment_frame_size(texture.key.as_ref()),
            frame_cols: frame_cols as usize,
        });
    }
    let held_miss_sprite = (!blind_active && held_misses.iter().any(Option::is_some))
        .then(|| {
            held_miss_texture.map(|texture| IndicatorSprite {
                source: texture.texture_key_handle().into_sprite_source(),
                scale: if assets::parse_texture_hints(texture.key.as_ref()).doubleres {
                    0.5
                } else {
                    1.0
                },
            })
        })
        .flatten();
    let hold_sprite = (!blind_active && hold_judgments.iter().any(Option::is_some))
        .then(|| {
            hold_judgment_texture.map(|texture| texture.texture_key_handle().into_sprite_source())
        })
        .flatten();
    compose_judgment_feedback(
        hud_actors,
        JudgmentFeedbackRequest {
            style: style.judgment_feedback,
            blind: blind_active,
            elapsed_screen,
            tap,
            tap_sprite,
            tap_xy: [judgment_x, judgment_y],
            judgment_back: options.judgment_back,
            judgment_zoom: judgment_zoom_mod,
            held_misses,
            held_miss_sprite,
            hold_judgments,
            hold_sprite,
            current_beat,
            arrow_effect_time: request.arrow_effect_time_s,
            mini,
            visual,
            noteskin_column_xs: noteskin_assets.noteskin[player_idx]
                .as_ref()
                .map(|noteskin| noteskin.column_xs.as_slice()),
            num_cols,
            spacing_multiplier: spacing_mult,
            field_zoom,
            playfield_center_x,
            screen_center_y: screen_center_y(),
            screen_height: screen_height(),
            field_center_y: notefield_offset_y,
            column_reverse_percent: &column_reverse_percent[..num_cols],
        },
    );
    let judgment_actors = request
        .capture_requests
        .judgment
        .then(|| share_actor_range(&mut hud_actors, judgment_capture_start))
        .flatten();

    let (tilt, skew) = (perspective.tilt, perspective.skew);
    if !actors.is_empty() {
        let center_y = 0.5 * (receptor_y_normal + receptor_y_reverse);
        if let Some(view_proj) = notefield_view_proj(
            screen_width(),
            screen_height(),
            playfield_center_x,
            center_y,
            tilt,
            skew,
            reverse_scroll,
        ) {
            actors.reserve(2);
            actors.insert(0, Actor::CameraPush { view_proj });
            actors.push(Actor::CameraPop);
        }
    }

    let field_actors = request
        .capture_requests
        .note_field
        .then(|| share_actor_range(&mut actors, 0))
        .flatten()
        .unwrap_or_default();
    BuiltNotefield {
        layout_center_x,
        field_actors,
        judgment_actors,
        combo_actors,
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
