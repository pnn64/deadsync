use crate::act;
use crate::assets;
use crate::notefield_style::notefield_style;
use crate::screens::gameplay::{GameplayCoreState as State, GameplayNoteskinAssets};
use deadlib_present::actors::{Actor, IntoTextureKey};
use deadlib_present::color;
use deadlib_present::space::*;
use deadlib_render::{BlendMode, TexturedMeshVertex};
use deadsync_assets::noteskin::SpriteSlot;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_core::note::NoteType;
use deadsync_gameplay::{
    AppearanceEffects, FantasticWindowOptions, GameplayErrorBarTrim, TapExplosionOptions,
    VisualEffects, blue_fantastic_window_ms, gameplay_error_bar_trim_max_window_ix,
    hold_explosion_active, hold_explosion_enabled_for_options, hold_head_render_flags,
    song_lua_note_hidden,
};
use deadsync_notefield::{
    BuiltNotefield, ColumnFeedbackRequest, ComboFeedbackRequest, ComboMilestoneAssets,
    CounterHudRequest, ErrorBarComposeRequest, ErrorBarModes, ErrorBarState, HoldEntryPlanRequest,
    IndicatorSprite, JudgmentFeedbackRequest, JudgmentTiltParams, LayoutMiniIndicatorPosition,
    MeasureComposeRequest, MeasureCounterOptions, MeasureLineMode, MineLayerRequest,
    MiniIndicatorRequest, ModelMeshCache, NoteAlphaParams, NoteLayerRequest, NoteXParams,
    NotefieldChartView, NotefieldComposeRequest, NotefieldFrameFeatures, NotefieldGeometry,
    NotefieldNoteskinView, NotefieldOptions, NotefieldSongLuaView, NotefieldVisualState,
    ReceptorActorsRequest, ReceptorPress, TapJudgmentFeedback, TapJudgmentRowsParams,
    TapJudgmentSprite, TornadoBounds, VisualEffectParams, ZmodLayoutParams, actor_with_world_z,
    appearance_needs_rows, appearance_note_actor_alpha, appearance_note_glow, bottom_cap_uv_window,
    clipped_hold_body_bounds, compose_column_feedback, compose_combo_feedback, compose_counter_hud,
    compose_error_bar, compose_judgment_feedback, compose_measure_lines, compose_mine_layers,
    compose_mini_indicator, compose_note_layer, compose_receptor_actors,
    for_each_visible_hold_index, for_each_visible_note_index,
    gameplay_visual_effect_params as visual_effect_params, hold_body_bottom_for_tail_cap,
    hold_body_segment_budget, hold_entry_head_beat, hold_entry_plan, hold_glow_color,
    hold_overlaps_visible_window, hold_parts_for_note_type, hold_segment_pose, hold_strip_actor,
    hold_strip_glow_actor, hold_strip_quad, hold_strip_row_3d, hold_strip_row_from_positions,
    hold_tail_cap_bounds, itg_actor_glow_alpha, judgment_actor_zoom,
    judgment_tilt_rotation_deg as crate_judgment_tilt_rotation_deg, maybe_flip_uv_vert,
    maybe_mirror_uv_horiz_for_reverse_flipped, mine_hides_after_resolution, mine_part,
    note_world_z_for_bumpy, note_x_offset as crate_note_x_offset, notefield_view_proj,
    offset_center, prepare_notefield, receptor_row_center as crate_receptor_row_center,
    scale_cap_to_arrow, scale_effect_size, scale_sprite_to_arrow, share_actor_range,
    song_lua_note_model_draw, tap_judgment_rows as crate_tap_judgment_rows, tap_part_for_note_type,
    tap_replacement_head, top_cap_rotation_deg, translated_uv_rect, visual_arrow_effect_zoom,
    visual_confusion_rotation_deg, visual_hold_body_needs_z_buffer, visual_note_rotation_z,
    visual_pulse_zoom_for_y, visual_tiny_zoom, visual_use_legacy_hold_sprites, zmod_broken_run_end,
};
use deadsync_notefield::{FieldPlacement, ProxyCaptureRequests, ViewOverride};
use deadsync_noteskin::NUM_QUANTIZATIONS;
use deadsync_profile as profile_data;
use deadsync_rules::judgment::Judgment;
use deadsync_rules::note::HoldResult;
use deadsync_theme::NotefieldStyle;
use std::array::from_fn;
use std::cell::RefCell;
use std::sync::Arc;

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

// Z-order layers for key gameplay visuals (higher draws on top)
const Z_HOLD_BODY: i32 = 110;
const Z_HOLD_CAP: i32 = 110;
const Z_HOLD_GLOW: i32 = 111;
// ITG's Explosion actor declares hold/roll children before tap judgments, so taps render on top.
const Z_TAP_EXPLOSION: i32 = 150;
const Z_MINE_EXPLOSION: i32 = 101;
const Z_TAP_NOTE: i32 = 140;
const MINE_CORE_SIZE_RATIO: f32 = 0.45;
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
fn hold_alpha_needs_rows(appearance: AppearanceEffects) -> bool {
    appearance_needs_rows(note_alpha_params(appearance))
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
fn confusion_rotation_deg(song_beat: f32, visual: VisualEffects, local_col: usize) -> f32 {
    visual_confusion_rotation_deg(song_beat, visual_effect_params(&visual, local_col))
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
    let current_time = prepared.current_time_s;
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
    let receptor_alpha = prepared.receptor_alpha;
    let blind_active = prepared.blind_active;

    if let Some(note_inputs) = prepared.notes.as_ref() {
        let ns = note_inputs.base;
        let mine_ns = note_inputs.mine;
        let receptor_ns = note_inputs.receptor;
        let tap_explosion_ns = note_inputs.tap_explosion;
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
        let logical_slot_size = |slot: &SpriteSlot| -> [f32; 2] { slot.logical_size() };
        let scale_explosion = |logical_size: [f32; 2], effect_zoom: f32| -> [f32; 2] {
            scale_effect_size(logical_size, field_zoom, effect_zoom)
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

        compose_column_feedback(
            actors,
            hud_actors,
            ColumnFeedbackRequest {
                style,
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
                // Preserve the existing regular-cue behavior: its countdown is
                // independent of the crossover-only profile toggle.
                regular_countdown: true,
                crossover_countdown: options.frame_features.crossover_countdown,
                current_music_time: current_time,
                current_screen_time: elapsed_screen,
                music_rate: request.chart.music_rate,
                col_start,
                num_cols,
                column_xs: &measure_column_xs,
                column_dirs: &column_dirs,
                spacing_multiplier: spacing_mult,
                field_zoom,
                playfield_center_x,
                field_center_y: notefield_offset_y,
                screen_height: screen_height(),
                compact_flashes: options.column_flash_compact,
                dim_flashes: options.column_flash_dimmed,
                countdown_font: mc_font_name,
                countdown_text: cached_int_i32,
            },
        );

        // Receptors + glow
        let noteskin_sprite_source =
            |slot: &SpriteSlot| slot.texture_key_handle().into_sprite_source();
        for (i, &receptor_y_lane) in column_receptor_ys.iter().take(num_cols).enumerate() {
            let col = col_start + i;
            let receptor_hidden_by_song_lua =
                song_lua_note_hidden(request.song_lua.note_hides, i, current_beat);
            let confusion_receptor_rot = confusion_rotation_deg(current_beat, visual, i);
            let receptor_center = receptor_row_center(
                playfield_center_x,
                i,
                receptor_y_lane,
                request.arrow_effect_time_s,
                beat_push,
                visual,
                &col_offsets[..num_cols],
                &invert_distances[..num_cols],
                &tornado_bounds[..num_cols],
            );
            let bop_zoom = state.receptor_bop_zoom(col);
            let receptor_effect_zoom = arrow_effect_zoom(&visual, i, 0.0);
            let hold_slot = if receptor_hidden_by_song_lua || !options.hold_explosion_enabled {
                None
            } else {
                state.active_hold(col).and_then(|active| {
                    let note = request.chart.notes.get(active.note_index)?;
                    if !hold_explosion_active(Some(active), current_beat, note.beat) {
                        return None;
                    }
                    tap_explosion_ns.and_then(|ns| {
                        ns.hold_explosion_for_col(i, matches!(note.note_type, NoteType::Roll))
                    })
                })
            };
            let targets_visible = !receptor_hidden_by_song_lua
                && !options.hide_targets
                && receptor_alpha > f32::EPSILON;
            let target_slot = targets_visible.then(|| &receptor_ns.receptor_off[i]);
            let target_reverse = targets_visible
                .then(|| receptor_ns.receptor_off_reverse.get(i).copied())
                .flatten();
            let resolve_receptor_press = || {
                let visual = state.receptor_glow_visual_for_col(col)?;
                let slot = receptor_ns
                    .receptor_glow
                    .get(i)
                    .and_then(|slot| slot.as_ref())?;
                Some(ReceptorPress {
                    slot,
                    reverse: receptor_ns.receptor_glow_reverse.get(i).copied(),
                    visual,
                })
            };
            compose_receptor_actors(
                actors,
                &mut model_cache,
                ReceptorActorsRequest {
                    target_slot,
                    target_reverse,
                    hold_slot,
                    center: receptor_center,
                    hidden: receptor_hidden_by_song_lua,
                    hide_targets: options.hide_targets,
                    reverse: column_reverse_percent[i] > 0.5,
                    bop_zoom,
                    effect_zoom: receptor_effect_zoom,
                    confusion_rotation_deg: confusion_receptor_rot,
                    elapsed: elapsed_screen,
                    beat: current_beat,
                    receptor_alpha,
                    field_zoom,
                    rotation_y_deg: note_rotation_y,
                    pulse: &receptor_ns.receptor_pulse,
                    press_behavior: receptor_ns.receptor_glow_behavior,
                    style: style.receptor,
                },
                resolve_receptor_press,
                &noteskin_sprite_source,
            );
        }
        // Tap explosions (receptor noteflash / GhostArrow) are independent of
        // the "Hide Combo Explosions" UI option, which only affects combo splodes.
        for (i, active_opt) in state
            .tap_explosions_for_columns(col_start, num_cols)
            .iter()
            .enumerate()
        {
            if song_lua_note_hidden(request.song_lua.note_hides, i, current_beat) {
                continue;
            }
            if let Some(active) = active_opt.as_ref()
                && let Some(tap_explosion_ns) = tap_explosion_ns
                && let Some(explosion) = tap_explosion_ns.tap_explosion_for_col_with_bright(
                    i,
                    active.window,
                    active.bright,
                )
            {
                let receptor_y_lane = column_receptor_ys[i];
                let receptor_center = receptor_row_center(
                    playfield_center_x,
                    i,
                    receptor_y_lane,
                    request.arrow_effect_time_s,
                    beat_push,
                    visual,
                    &col_offsets[..num_cols],
                    &invert_distances[..num_cols],
                    &tornado_bounds[..num_cols],
                );
                let confusion_receptor_rot = confusion_rotation_deg(current_beat, visual, i);
                let explosion_effect_zoom = arrow_effect_zoom(&visual, i, 0.0);
                for layer in explosion.layers.iter() {
                    let anim_time = active.elapsed;
                    let slot = &layer.slot;
                    let beat_for_anim = if slot.source.is_beat_based() {
                        (request.visual.current_display_beat - active.start_beat).max(0.0)
                    } else {
                        request.visual.current_display_beat
                    };
                    let frame = slot.frame_index(anim_time, beat_for_anim);
                    let uv = slot.uv_for_frame_at(frame, elapsed_screen);
                    let size = scale_explosion(logical_slot_size(slot), explosion_effect_zoom);
                    let explosion_visual = layer.animation.state_at(active.elapsed);
                    if !explosion_visual.visible {
                        continue;
                    }
                    let glow = explosion_visual.glow;
                    let glow_strength =
                        glow[0].abs() + glow[1].abs() + glow[2].abs() + glow[3].abs();
                    if layer.animation.blend_add {
                        actors.push(act!(sprite(slot.texture_key_handle()):
                            align(0.5, 0.5):
                            xy(receptor_center[0], receptor_center[1]):
                            setsize(size[0], size[1]):
                            zoom(explosion_visual.zoom):
                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                            diffuse(
                                explosion_visual.diffuse[0],
                                explosion_visual.diffuse[1],
                                explosion_visual.diffuse[2],
                                explosion_visual.diffuse[3]
                            ):
                            rotationy(flat_tap_face_rotation_y):
                            rotationz(explosion_visual.rotation_z - slot.def.rotation_deg as f32 + confusion_receptor_rot):
                            blend(add):
                            z(Z_TAP_EXPLOSION)
                        ));
                        if glow_strength > f32::EPSILON {
                            actors.push(act!(sprite(slot.texture_key_handle()):
                                align(0.5, 0.5):
                                xy(receptor_center[0], receptor_center[1]):
                                setsize(size[0], size[1]):
                                zoom(explosion_visual.zoom):
                                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                diffuse(glow[0], glow[1], glow[2], glow[3]):
                                rotationy(flat_tap_face_rotation_y):
                                rotationz(explosion_visual.rotation_z - slot.def.rotation_deg as f32 + confusion_receptor_rot):
                                blend(add):
                                z(Z_TAP_EXPLOSION)
                            ));
                        }
                    } else {
                        actors.push(act!(sprite(slot.texture_key_handle()):
                            align(0.5, 0.5):
                            xy(receptor_center[0], receptor_center[1]):
                            setsize(size[0], size[1]):
                            zoom(explosion_visual.zoom):
                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                            diffuse(
                                explosion_visual.diffuse[0],
                                explosion_visual.diffuse[1],
                                explosion_visual.diffuse[2],
                                explosion_visual.diffuse[3]
                            ):
                            rotationy(flat_tap_face_rotation_y):
                            rotationz(explosion_visual.rotation_z - slot.def.rotation_deg as f32 + confusion_receptor_rot):
                            blend(normal):
                            z(Z_TAP_EXPLOSION)
                        ));
                        if glow_strength > f32::EPSILON {
                            actors.push(act!(sprite(slot.texture_key_handle()):
                                align(0.5, 0.5):
                                xy(receptor_center[0], receptor_center[1]):
                                setsize(size[0], size[1]):
                                zoom(explosion_visual.zoom):
                                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                diffuse(glow[0], glow[1], glow[2], glow[3]):
                                rotationy(flat_tap_face_rotation_y):
                                rotationz(explosion_visual.rotation_z - slot.def.rotation_deg as f32 + confusion_receptor_rot):
                                blend(normal):
                                z(Z_TAP_EXPLOSION)
                            ));
                        }
                    }
                }
            }
        }
        // Mine explosions
        for (i, active_opt) in state
            .mine_explosions_for_columns(col_start, num_cols)
            .iter()
            .enumerate()
        {
            let Some(active) = active_opt.as_ref() else {
                continue;
            };
            let Some(explosion) = mine_ns.mine_hit_explosion.as_ref() else {
                continue;
            };
            let receptor_y_lane = column_receptor_ys[i];
            let receptor_center = receptor_row_center(
                playfield_center_x,
                i,
                receptor_y_lane,
                request.arrow_effect_time_s,
                beat_push,
                visual,
                &col_offsets[..num_cols],
                &invert_distances[..num_cols],
                &tornado_bounds[..num_cols],
            );
            let explosion_effect_zoom = arrow_effect_zoom(&visual, i, 0.0);
            for layer in explosion.layers.iter() {
                let slot = &layer.slot;
                let explosion_visual = layer.animation.state_at(active.elapsed);
                if !explosion_visual.visible {
                    continue;
                }
                let frame = slot.frame_index(active.elapsed, current_beat);
                let uv = slot.uv_for_frame_at(frame, elapsed_screen);
                let size = scale_explosion(logical_slot_size(slot), explosion_effect_zoom);
                let glow = explosion_visual.glow;
                let glow_strength = glow[0].abs() + glow[1].abs() + glow[2].abs() + glow[3].abs();
                if layer.animation.blend_add {
                    actors.push(act!(sprite(slot.texture_key_handle()):
                        align(0.5, 0.5):
                        xy(receptor_center[0], receptor_center[1]):
                        setsize(size[0], size[1]):
                        zoom(explosion_visual.zoom):
                        customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                        rotationz(-explosion_visual.rotation_z):
                        diffuse(
                            explosion_visual.diffuse[0],
                            explosion_visual.diffuse[1],
                            explosion_visual.diffuse[2],
                            explosion_visual.diffuse[3]
                        ):
                        blend(add):
                        z(Z_MINE_EXPLOSION)
                    ));
                    if glow_strength > f32::EPSILON {
                        actors.push(act!(sprite(slot.texture_key_handle()):
                            align(0.5, 0.5):
                            xy(receptor_center[0], receptor_center[1]):
                            setsize(size[0], size[1]):
                            zoom(explosion_visual.zoom):
                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                            rotationz(-explosion_visual.rotation_z):
                            diffuse(glow[0], glow[1], glow[2], glow[3]):
                            blend(add):
                            z(Z_MINE_EXPLOSION)
                        ));
                    }
                } else {
                    actors.push(act!(sprite(slot.texture_key_handle()):
                        align(0.5, 0.5):
                        xy(receptor_center[0], receptor_center[1]):
                        setsize(size[0], size[1]):
                        zoom(explosion_visual.zoom):
                        customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                        rotationz(-explosion_visual.rotation_z):
                        diffuse(
                            explosion_visual.diffuse[0],
                            explosion_visual.diffuse[1],
                            explosion_visual.diffuse[2],
                            explosion_visual.diffuse[3]
                        ):
                        blend(normal):
                        z(Z_MINE_EXPLOSION)
                    ));
                    if glow_strength > f32::EPSILON {
                        actors.push(act!(sprite(slot.texture_key_handle()):
                            align(0.5, 0.5):
                            xy(receptor_center[0], receptor_center[1]):
                            setsize(size[0], size[1]):
                            zoom(explosion_visual.zoom):
                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                            rotationz(-explosion_visual.rotation_z):
                            diffuse(glow[0], glow[1], glow[2], glow[3]):
                            blend(normal):
                            z(Z_MINE_EXPLOSION)
                        ));
                    }
                }
            }
        }
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
            let (top, bottom, draw_body_or_cap) = hold_plan
                .draw_span
                .map_or((0.0, 0.0, false), |(top, bottom)| (top, bottom, true));
            let hold_diffuse = hold_plan.diffuse;
            let head_anchor_y = hold_plan.head_anchor_y;
            let head_anchor_travel = hold_plan.head_anchor_travel;
            let hold_parts = hold_plan.parts;
            let hold_part_phase = hold_plan.head_phase;
            let hold_body_phase = hold_plan.body_phase;
            let hold_topcap_phase = hold_plan.top_cap_phase;
            let hold_bottomcap_phase = hold_plan.bottom_cap_phase;
            let top_cap_slot = hold_plan.top_cap_slot;
            let bottom_cap_slot = hold_plan.bottom_cap_slot;
            let body_slot = hold_plan.body_slot;
            let head_layers = hold_plan.head_layers;
            let head_slot = hold_plan.head_slot;
            // Prepare clipped body extents. ITG DrawHoldBodyInternal always
            // draws the bottom cap downward from y_tail, so we keep body clipping
            // anchored to that same tail-side join.
            let body_top = top;
            let mut body_bottom = bottom;
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
            if let Some(cap_slot) = bottom_cap_slot {
                let cap_size = scale_cap_to_arrow(cap_slot.size(), hold_target_arrow_px);
                // ITGmania joins hold body to cap at the tail edge (with a tiny overlap),
                // not at the cap midpoint. Keep the body clipped to that join line.
                body_bottom = hold_body_bottom_for_tail_cap(body_bottom, y_tail, cap_size[1]);
            }
            // Track rendered body extents so the tail cap can attach cleanly when
            // body segments are visible.
            let mut rendered_body_top: Option<f32> = None;
            let mut rendered_body_bottom: Option<f32> = None;
            let mut body_head_row: Option<[[f32; 3]; 2]> = None;
            let mut body_tail_row: Option<[[f32; 3]; 2]> = None;
            let col_bumpy = visual_effect_params(&visual, local_col).bumpy;
            let hold_depth_test = hold_body_needs_z_buffer(&visual);
            let use_legacy_hold_sprites = visual_use_legacy_hold_sprites(
                col_bumpy,
                visual.drunk,
                visual.tornado,
                visual.beat,
                visual.pulse_outer,
            );
            let hold_y_rotation_active = note_rotation_y.abs() > f32::EPSILON;
            // ITG draws hold bodies from y_head to y_tail (top-to-bottom in screen space).
            // If noteskin offsets invert the interval for ultra-short holds, skip body draw
            // and rely on tail-cap clipping.
            let body_direction_invalid = y_tail <= y_head;
            if draw_body_or_cap
                && !body_direction_invalid
                && body_bottom > body_top
                && let Some(body_slot) = body_slot
            {
                let texture_size = body_slot.size();
                let texture_width = texture_size[0].max(1) as f32;
                let texture_height = texture_size[1].max(1) as f32;
                if texture_width > f32::EPSILON && texture_height > f32::EPSILON {
                    let body_frame = body_slot.frame_index_from_phase(hold_body_phase);
                    let body_width = hold_target_arrow_px;
                    let scale = body_width / texture_width;
                    let segment_height = (texture_height * scale).max(f32::EPSILON);
                    let body_uv_elapsed = if body_slot.model.is_some() {
                        hold_body_phase
                    } else {
                        elapsed_screen
                    };
                    let body_uv = maybe_flip_uv_vert(
                        translated_uv_rect(
                            body_slot.uv_for_frame_at(body_frame, body_uv_elapsed),
                            ns.part_uv_translation(hold_parts.body, note.beat, false),
                        ),
                        body_flipped,
                    );
                    let u0 = body_uv[0];
                    let u1 = body_uv[2];
                    let v_top = body_uv[1];
                    let v_bottom = body_uv[3];
                    let v_range = v_bottom - v_top;
                    let natural_top = y_head;
                    let natural_bottom = y_tail;
                    let hold_length = natural_bottom - natural_top;
                    const SEGMENT_PHASE_EPS: f32 = 1e-4;
                    if hold_length > f32::EPSILON
                        && let Some((clipped_top, clipped_bottom)) = clipped_hold_body_bounds(
                            body_top,
                            body_bottom,
                            natural_top,
                            natural_bottom,
                        )
                    {
                        let visible_top_distance = clipped_top - natural_top;
                        let visible_bottom_distance = clipped_bottom - natural_top;
                        let visible_span = visible_bottom_distance - visible_top_distance;
                        let (max_segments, allow_legacy_sprites) =
                            hold_body_segment_budget(visible_span, segment_height);
                        let anchor_to_top =
                            lane_reverse && note_display.top_hold_anchor_when_reverse;
                        let phase_offset = if anchor_to_top {
                            0.0
                        } else {
                            let total_phase = hold_length / segment_height;
                            if total_phase >= 1.0 + SEGMENT_PHASE_EPS {
                                let fractional = total_phase.fract();
                                if fractional > SEGMENT_PHASE_EPS
                                    && (1.0 - fractional) > SEGMENT_PHASE_EPS
                                {
                                    1.0 - fractional
                                } else {
                                    0.0
                                }
                            } else {
                                0.0
                            }
                        };

                        let mut phase = visible_top_distance / segment_height + phase_offset;
                        let phase_end_adjusted =
                            visible_bottom_distance / segment_height + phase_offset;
                        let mut emitted = 0;

                        let hold_alpha_rows = hold_alpha_needs_rows(appearance);
                        if use_legacy_hold_sprites && allow_legacy_sprites && !hold_alpha_rows {
                            while phase + SEGMENT_PHASE_EPS < phase_end_adjusted
                                && emitted < max_segments
                            {
                                let mut next_phase = (phase.floor() + 1.0).min(phase_end_adjusted);
                                if next_phase - phase < SEGMENT_PHASE_EPS {
                                    next_phase = phase_end_adjusted;
                                }
                                if next_phase - phase < SEGMENT_PHASE_EPS {
                                    break;
                                }

                                let distance_start = (phase - phase_offset) * segment_height;
                                let distance_end = (next_phase - phase_offset) * segment_height;
                                let y_start = natural_top + distance_start;
                                let y_end = natural_top + distance_end;
                                let segment_top = y_start.max(body_top);
                                let segment_bottom = y_end.min(body_bottom);

                                if segment_bottom - segment_top <= f32::EPSILON {
                                    phase = next_phase;
                                    continue;
                                }

                                let base_floor = phase.floor();
                                let start_fraction = (phase - base_floor).clamp(0.0, 1.0);
                                let end_fraction = (next_phase - base_floor).clamp(0.0, 1.0);
                                let mut v0 = v_top + v_range * start_fraction;
                                let mut v1 = v_top + v_range * end_fraction;

                                let segment_size = segment_bottom - segment_top;
                                let portion = (segment_size / segment_height).clamp(0.0, 1.0);
                                let tail_gap = (natural_bottom - body_bottom).max(0.0);
                                let body_reaches_tail = tail_gap <= segment_height + 1.0;
                                let is_last_visible_segment = (body_bottom - segment_bottom).abs()
                                    <= 0.5
                                    || next_phase >= phase_end_adjusted - SEGMENT_PHASE_EPS;

                                if body_reaches_tail && is_last_visible_segment {
                                    if v_range >= 0.0 {
                                        v1 = v_bottom;
                                        v0 = v_bottom - v_range.abs() * portion;
                                    } else {
                                        v1 = v_bottom;
                                        v0 = v_bottom + v_range.abs() * portion;
                                    }
                                }

                                let segment_center_screen = (segment_top + segment_bottom) * 0.5;
                                let segment_center_travel = travel.adjusted_from_screen_y(
                                    local_col,
                                    lane_receptor_y,
                                    dir,
                                    segment_center_screen,
                                );
                                let segment_alpha = note_actor_alpha(
                                    segment_center_travel + travel.lane_offset(local_col),
                                    elapsed_screen,
                                    mini,
                                    appearance,
                                );
                                let segment_glow = itg_actor_glow_alpha(note_glow(
                                    segment_center_travel + travel.lane_offset(local_col),
                                    elapsed_screen,
                                    mini,
                                    appearance,
                                ));
                                if segment_alpha > f32::EPSILON || segment_glow > f32::EPSILON {
                                    let segment_center_x = lane_center_x_from_adjusted_travel(
                                        local_col,
                                        segment_center_travel,
                                    );
                                    rendered_body_top = Some(match rendered_body_top {
                                        None => segment_top,
                                        Some(v) => v.min(segment_top),
                                    });
                                    rendered_body_bottom = Some(match rendered_body_bottom {
                                        None => segment_bottom,
                                        Some(v) => v.max(segment_bottom),
                                    });
                                    if segment_alpha > f32::EPSILON {
                                        actors.push(actor_with_world_z(
                                            act!(sprite(body_slot.texture_key_handle()):
                                                align(0.5, 0.5):
                                                xy(segment_center_x, segment_center_screen):
                                                setsize(body_width, segment_size):
                                                rotationy(note_rotation_y):
                                                rotationz(0.0):
                                                customtexturerect(u0, v0, u1, v1):
                                                diffuse(
                                                    hold_diffuse[0],
                                                    hold_diffuse[1],
                                                    hold_diffuse[2],
                                                    hold_diffuse[3] * segment_alpha
                                                ):
                                                z(Z_HOLD_BODY)
                                            ),
                                            world_z_for_adjusted_travel(
                                                local_col,
                                                segment_center_travel,
                                            ),
                                        ));
                                    }
                                    if segment_glow > f32::EPSILON {
                                        actors.push(actor_with_world_z(
                                            act!(sprite(body_slot.texture_key_handle()):
                                                align(0.5, 0.5):
                                                xy(segment_center_x, segment_center_screen):
                                                setsize(body_width, segment_size):
                                                rotationy(note_rotation_y):
                                                rotationz(0.0):
                                                customtexturerect(u0, v0, u1, v1):
                                                diffuse(1.0, 1.0, 1.0, 0.0):
                                                glow(1.0, 1.0, 1.0, segment_glow):
                                                z(Z_HOLD_GLOW)
                                            ),
                                            world_z_for_adjusted_travel(
                                                local_col,
                                                segment_center_travel,
                                            ),
                                        ));
                                    }
                                }

                                phase = next_phase;
                                emitted += 1;
                            }
                        } else {
                            let body_slice_step = if hold_depth_test { 4.0 } else { 16.0 };
                            let use_body_mesh =
                                body_slot.model.is_none() && !hold_y_rotation_active;
                            let mut body_mesh_vertices: Option<Vec<TexturedMeshVertex>> = None;
                            let mut body_glow_vertices: Option<Vec<TexturedMeshVertex>> = None;
                            let mut prev_body_row: Option<[[f32; 3]; 2]> = None;

                            while phase + SEGMENT_PHASE_EPS < phase_end_adjusted
                                && emitted < max_segments
                            {
                                let mut next_phase = (phase.floor() + 1.0).min(phase_end_adjusted);
                                if next_phase - phase < SEGMENT_PHASE_EPS {
                                    next_phase = phase_end_adjusted;
                                }
                                if next_phase - phase < SEGMENT_PHASE_EPS {
                                    break;
                                }

                                let distance_start = (phase - phase_offset) * segment_height;
                                let distance_end = (next_phase - phase_offset) * segment_height;
                                let y_start = natural_top + distance_start;
                                let y_end = natural_top + distance_end;
                                let segment_top = y_start.max(body_top);
                                let segment_bottom = y_end.min(body_bottom);

                                if segment_bottom - segment_top <= f32::EPSILON {
                                    phase = next_phase;
                                    continue;
                                }

                                let base_floor = phase.floor();
                                let start_fraction = (phase - base_floor).clamp(0.0, 1.0);
                                let end_fraction = (next_phase - base_floor).clamp(0.0, 1.0);
                                let mut v0 = v_top + v_range * start_fraction;
                                let mut v1 = v_top + v_range * end_fraction;

                                let segment_size = segment_bottom - segment_top;
                                let portion = (segment_size / segment_height).clamp(0.0, 1.0);

                                let tail_gap = (natural_bottom - body_bottom).max(0.0);
                                let body_reaches_tail = tail_gap <= segment_height + 1.0;
                                let is_last_visible_segment = (body_bottom - segment_bottom).abs()
                                    <= 0.5
                                    || next_phase >= phase_end_adjusted - SEGMENT_PHASE_EPS;

                                if body_reaches_tail && is_last_visible_segment {
                                    if v_range >= 0.0 {
                                        v1 = v_bottom;
                                        v0 = v_bottom - v_range.abs() * portion;
                                    } else {
                                        v1 = v_bottom;
                                        v0 = v_bottom + v_range.abs() * portion;
                                    }
                                }
                                let mut slice_top = segment_top;
                                while slice_top + f32::EPSILON < segment_bottom {
                                    let slice_bottom =
                                        (slice_top + body_slice_step).min(segment_bottom);
                                    let slice_size = slice_bottom - slice_top;
                                    if slice_size <= f32::EPSILON {
                                        break;
                                    }
                                    let slice_t0 =
                                        ((slice_top - segment_top) / segment_size).clamp(0.0, 1.0);
                                    let slice_t1 = ((slice_bottom - segment_top) / segment_size)
                                        .clamp(0.0, 1.0);
                                    let slice_v0 = (v1 - v0).mul_add(slice_t0, v0);
                                    let slice_v1 = (v1 - v0).mul_add(slice_t1, v0);
                                    let slice_center_screen = (slice_top + slice_bottom) * 0.5;
                                    let slice_center_travel = travel.adjusted_from_screen_y(
                                        local_col,
                                        lane_receptor_y,
                                        dir,
                                        slice_center_screen,
                                    );
                                    let slice_alpha = note_actor_alpha(
                                        slice_center_travel + travel.lane_offset(local_col),
                                        elapsed_screen,
                                        mini,
                                        appearance,
                                    );
                                    let slice_glow = itg_actor_glow_alpha(note_glow(
                                        slice_center_travel + travel.lane_offset(local_col),
                                        elapsed_screen,
                                        mini,
                                        appearance,
                                    ));
                                    if slice_alpha <= f32::EPSILON && slice_glow <= f32::EPSILON {
                                        prev_body_row = None;
                                        slice_top = slice_bottom;
                                        continue;
                                    }
                                    let slice_top_travel = travel.adjusted_from_screen_y(
                                        local_col,
                                        lane_receptor_y,
                                        dir,
                                        slice_top,
                                    );
                                    let slice_bottom_travel = travel.adjusted_from_screen_y(
                                        local_col,
                                        lane_receptor_y,
                                        dir,
                                        slice_bottom,
                                    );
                                    let slice_top_x = lane_center_x_from_adjusted_travel(
                                        local_col,
                                        slice_top_travel,
                                    );
                                    let slice_bottom_x = lane_center_x_from_adjusted_travel(
                                        local_col,
                                        slice_bottom_travel,
                                    );
                                    let (slice_center, slice_height, slice_rotation) =
                                        hold_segment_pose(
                                            [slice_top_x, slice_top],
                                            [slice_bottom_x, slice_bottom],
                                        );
                                    if slice_height <= f32::EPSILON {
                                        slice_top = slice_bottom;
                                        continue;
                                    }
                                    let slice_world_z =
                                        world_z_for_adjusted_travel(local_col, slice_center_travel);

                                    rendered_body_top = Some(match rendered_body_top {
                                        None => slice_top,
                                        Some(v) => v.min(slice_top),
                                    });
                                    rendered_body_bottom = Some(match rendered_body_bottom {
                                        None => slice_bottom,
                                        Some(v) => v.max(slice_bottom),
                                    });

                                    if use_body_mesh {
                                        let top_alpha = note_actor_alpha(
                                            slice_top_travel + travel.lane_offset(local_col),
                                            elapsed_screen,
                                            mini,
                                            appearance,
                                        );
                                        let bottom_alpha = note_actor_alpha(
                                            slice_bottom_travel + travel.lane_offset(local_col),
                                            elapsed_screen,
                                            mini,
                                            appearance,
                                        );
                                        let top_glow = itg_actor_glow_alpha(note_glow(
                                            slice_top_travel + travel.lane_offset(local_col),
                                            elapsed_screen,
                                            mini,
                                            appearance,
                                        ));
                                        let bottom_glow = itg_actor_glow_alpha(note_glow(
                                            slice_bottom_travel + travel.lane_offset(local_col),
                                            elapsed_screen,
                                            mini,
                                            appearance,
                                        ));
                                        let slice_forward = [
                                            slice_bottom_x - slice_top_x,
                                            slice_bottom - slice_top,
                                        ];
                                        let top_half_width =
                                            hold_arrow_px_for_adjusted_travel(slice_top_travel)
                                                * 0.5;
                                        let bottom_half_width =
                                            hold_arrow_px_for_adjusted_travel(slice_bottom_travel)
                                                * 0.5;
                                        let slice_top_z = world_z_for_adjusted_travel(
                                            local_col,
                                            slice_top_travel,
                                        );
                                        let slice_bottom_z = world_z_for_adjusted_travel(
                                            local_col,
                                            slice_bottom_travel,
                                        );
                                        let top_row = prev_body_row.unwrap_or_else(|| {
                                            let row = hold_strip_row_3d(
                                                [slice_top_x, slice_top, slice_top_z],
                                                slice_forward,
                                                top_half_width,
                                                u0,
                                                u1,
                                                slice_v0,
                                                [
                                                    hold_diffuse[0],
                                                    hold_diffuse[1],
                                                    hold_diffuse[2],
                                                    hold_diffuse[3] * top_alpha,
                                                ],
                                            );
                                            [row[0].pos, row[1].pos]
                                        });
                                        let top_row = hold_strip_row_from_positions(
                                            top_row[0],
                                            top_row[1],
                                            u0,
                                            u1,
                                            slice_v0,
                                            [
                                                hold_diffuse[0],
                                                hold_diffuse[1],
                                                hold_diffuse[2],
                                                hold_diffuse[3] * top_alpha,
                                            ],
                                        );
                                        if body_head_row.is_none() {
                                            body_head_row = Some([top_row[0].pos, top_row[1].pos]);
                                        }
                                        let bottom_row = hold_strip_row_3d(
                                            [slice_bottom_x, slice_bottom, slice_bottom_z],
                                            slice_forward,
                                            bottom_half_width,
                                            u0,
                                            u1,
                                            slice_v1,
                                            [
                                                hold_diffuse[0],
                                                hold_diffuse[1],
                                                hold_diffuse[2],
                                                hold_diffuse[3] * bottom_alpha,
                                            ],
                                        );
                                        if top_alpha > f32::EPSILON || bottom_alpha > f32::EPSILON {
                                            let mesh_vertices = body_mesh_vertices
                                                .get_or_insert_with(|| Vec::with_capacity(96));
                                            mesh_vertices.extend_from_slice(&hold_strip_quad(
                                                top_row, bottom_row,
                                            ));
                                        }
                                        if top_glow > f32::EPSILON || bottom_glow > f32::EPSILON {
                                            let top_glow_row = hold_strip_row_from_positions(
                                                top_row[0].pos,
                                                top_row[1].pos,
                                                u0,
                                                u1,
                                                slice_v0,
                                                hold_glow_color(top_glow),
                                            );
                                            let bottom_glow_row = hold_strip_row_from_positions(
                                                bottom_row[0].pos,
                                                bottom_row[1].pos,
                                                u0,
                                                u1,
                                                slice_v1,
                                                hold_glow_color(bottom_glow),
                                            );
                                            let glow_vertices = body_glow_vertices
                                                .get_or_insert_with(|| Vec::with_capacity(96));
                                            glow_vertices.extend_from_slice(&hold_strip_quad(
                                                top_glow_row,
                                                bottom_glow_row,
                                            ));
                                        }
                                        body_tail_row =
                                            Some([bottom_row[0].pos, bottom_row[1].pos]);
                                        prev_body_row =
                                            Some([bottom_row[0].pos, bottom_row[1].pos]);
                                    } else {
                                        if slice_alpha > f32::EPSILON {
                                            actors.push(actor_with_world_z(
                                                act!(sprite(body_slot.texture_key_handle()):
                                                    align(0.5, 0.5):
                                                    xy(slice_center[0], slice_center[1]):
                                                    setsize(body_width, slice_height):
                                                    rotationy(note_rotation_y):
                                                    rotationz(slice_rotation):
                                                    customtexturerect(u0, slice_v0, u1, slice_v1):
                                                    diffuse(
                                                        hold_diffuse[0],
                                                        hold_diffuse[1],
                                                        hold_diffuse[2],
                                                        hold_diffuse[3] * slice_alpha
                                                    ):
                                                    z(Z_HOLD_BODY)
                                                ),
                                                slice_world_z,
                                            ));
                                        }
                                        if slice_glow > f32::EPSILON {
                                            actors.push(actor_with_world_z(
                                                act!(sprite(body_slot.texture_key_handle()):
                                                    align(0.5, 0.5):
                                                    xy(slice_center[0], slice_center[1]):
                                                    setsize(body_width, slice_height):
                                                    rotationy(note_rotation_y):
                                                    rotationz(slice_rotation):
                                                    customtexturerect(u0, slice_v0, u1, slice_v1):
                                                    diffuse(1.0, 1.0, 1.0, 0.0):
                                                    glow(1.0, 1.0, 1.0, slice_glow):
                                                    z(Z_HOLD_GLOW)
                                                ),
                                                slice_world_z,
                                            ));
                                        }
                                    }
                                    slice_top = slice_bottom;
                                }

                                phase = next_phase;
                                emitted += 1;
                            }

                            if let Some(vertices) = body_mesh_vertices
                                && !vertices.is_empty()
                            {
                                actors.push(hold_strip_actor(
                                    body_slot.texture_key_shared(),
                                    Arc::from(vertices),
                                    BlendMode::Alpha,
                                    hold_depth_test,
                                    Z_HOLD_BODY as i16,
                                ));
                            }
                            if let Some(vertices) = body_glow_vertices
                                && !vertices.is_empty()
                            {
                                actors.push(hold_strip_glow_actor(
                                    body_slot.texture_key_shared(),
                                    Arc::from(vertices),
                                    hold_depth_test,
                                    Z_HOLD_GLOW as i16,
                                ));
                            }
                        }
                    }
                }
            }
            if draw_body_or_cap && let Some(cap_slot) = top_cap_slot {
                let head_position = y_head;
                if head_position > -400.0 && head_position < screen_height() + 400.0 {
                    let cap_frame = cap_slot.frame_index_from_phase(hold_topcap_phase);
                    let cap_uv_elapsed = if cap_slot.model.is_some() {
                        hold_topcap_phase
                    } else {
                        elapsed_screen
                    };
                    let cap_uv = maybe_flip_uv_vert(
                        translated_uv_rect(
                            cap_slot.uv_for_frame_at(cap_frame, cap_uv_elapsed),
                            ns.part_uv_translation(hold_parts.topcap, note.beat, false),
                        ),
                        body_flipped,
                    );
                    let cap_uv = maybe_mirror_uv_horiz_for_reverse_flipped(
                        cap_uv,
                        lane_reverse,
                        body_flipped,
                    );
                    let cap_size = scale_cap_to_arrow(cap_slot.size(), hold_target_arrow_px);
                    let cap_width = cap_size[0];
                    let mut cap_height = cap_size[1];
                    let u0 = cap_uv[0];
                    let u1 = cap_uv[2];
                    let v0 = cap_uv[1];
                    let mut v1 = cap_uv[3];
                    let cap_top = y_head - cap_height;
                    let mut cap_bottom = y_head;
                    if cap_height > f32::EPSILON {
                        let v_span = v1 - v0;
                        if y_tail < cap_bottom {
                            let trimmed = (cap_bottom - y_tail).clamp(0.0, cap_height);
                            if trimmed >= cap_height - f32::EPSILON {
                                cap_height = 0.0;
                            } else if trimmed > f32::EPSILON {
                                let fraction = trimmed / cap_height;
                                v1 -= v_span * fraction;
                                cap_bottom -= trimmed;
                                cap_height = cap_bottom - cap_top;
                            }
                        }
                    }
                    if cap_height > f32::EPSILON {
                        let cap_center = (cap_top + cap_bottom) * 0.5;
                        let cap_center_travel = travel.adjusted_from_screen_y(
                            local_col,
                            lane_receptor_y,
                            dir,
                            cap_center,
                        );
                        let cap_alpha = note_actor_alpha(
                            cap_center_travel + travel.lane_offset(local_col),
                            elapsed_screen,
                            mini,
                            appearance,
                        );
                        let cap_glow = itg_actor_glow_alpha(note_glow(
                            cap_center_travel + travel.lane_offset(local_col),
                            elapsed_screen,
                            mini,
                            appearance,
                        ));
                        if cap_alpha <= f32::EPSILON && cap_glow <= f32::EPSILON {
                            return;
                        }
                        let cap_top_travel =
                            travel.adjusted_from_screen_y(local_col, lane_receptor_y, dir, cap_top);
                        let cap_bottom_travel = travel.adjusted_from_screen_y(
                            local_col,
                            lane_receptor_y,
                            dir,
                            cap_bottom,
                        );
                        let cap_top_x =
                            lane_center_x_from_adjusted_travel(local_col, cap_top_travel);
                        let cap_bottom_x =
                            lane_center_x_from_adjusted_travel(local_col, cap_bottom_travel);
                        let (cap_center_xy, cap_draw_height, cap_path_rotation) =
                            hold_segment_pose([cap_top_x, cap_top], [cap_bottom_x, cap_bottom]);
                        if cap_draw_height <= f32::EPSILON {
                            return;
                        }
                        let use_top_cap_mesh = !use_legacy_hold_sprites
                            && cap_slot.model.is_none()
                            && !hold_y_rotation_active;
                        if use_top_cap_mesh {
                            let top_alpha = note_actor_alpha(
                                cap_top_travel + travel.lane_offset(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            );
                            let bottom_alpha = note_actor_alpha(
                                cap_bottom_travel + travel.lane_offset(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            );
                            let top_glow = itg_actor_glow_alpha(note_glow(
                                cap_top_travel + travel.lane_offset(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            ));
                            let bottom_glow = itg_actor_glow_alpha(note_glow(
                                cap_bottom_travel + travel.lane_offset(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            ));
                            let cap_forward = [cap_bottom_x - cap_top_x, cap_bottom - cap_top];
                            let top_half_width =
                                hold_arrow_px_for_adjusted_travel(cap_top_travel) * 0.5;
                            let bottom_half_width =
                                hold_arrow_px_for_adjusted_travel(cap_bottom_travel) * 0.5;
                            let cap_top_z = world_z_for_adjusted_travel(local_col, cap_top_travel);
                            let cap_bottom_z =
                                world_z_for_adjusted_travel(local_col, cap_bottom_travel);
                            let top_row = hold_strip_row_3d(
                                [cap_top_x, cap_top, cap_top_z],
                                cap_forward,
                                top_half_width,
                                u0,
                                u1,
                                v0,
                                [
                                    hold_diffuse[0],
                                    hold_diffuse[1],
                                    hold_diffuse[2],
                                    hold_diffuse[3] * top_alpha,
                                ],
                            );
                            let bottom_row = if let Some(body_head_row) = body_head_row
                                && rendered_body_top
                                    .is_some_and(|body_top| (body_top - cap_bottom).abs() <= 2.0)
                            {
                                hold_strip_row_from_positions(
                                    body_head_row[0],
                                    body_head_row[1],
                                    u0,
                                    u1,
                                    v1,
                                    [
                                        hold_diffuse[0],
                                        hold_diffuse[1],
                                        hold_diffuse[2],
                                        hold_diffuse[3] * bottom_alpha,
                                    ],
                                )
                            } else {
                                hold_strip_row_3d(
                                    [cap_bottom_x, cap_bottom, cap_bottom_z],
                                    cap_forward,
                                    bottom_half_width,
                                    u0,
                                    u1,
                                    v1,
                                    [
                                        hold_diffuse[0],
                                        hold_diffuse[1],
                                        hold_diffuse[2],
                                        hold_diffuse[3] * bottom_alpha,
                                    ],
                                )
                            };
                            if top_alpha > f32::EPSILON || bottom_alpha > f32::EPSILON {
                                actors.push(hold_strip_actor(
                                    cap_slot.texture_key_shared(),
                                    Arc::new(hold_strip_quad(top_row, bottom_row)),
                                    BlendMode::Alpha,
                                    hold_depth_test,
                                    Z_HOLD_CAP as i16,
                                ));
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
                                actors.push(hold_strip_glow_actor(
                                    cap_slot.texture_key_shared(),
                                    Arc::new(hold_strip_quad(top_glow_row, bottom_glow_row)),
                                    hold_depth_test,
                                    Z_HOLD_GLOW as i16,
                                ));
                            }
                        } else {
                            let cap_world_z =
                                world_z_for_adjusted_travel(local_col, cap_center_travel);
                            let cap_rotation = cap_path_rotation
                                + top_cap_rotation_deg(lane_reverse, body_flipped);
                            if cap_alpha > f32::EPSILON {
                                actors.push(actor_with_world_z(
                                    act!(sprite(cap_slot.texture_key_handle()):
                                        align(0.5, 0.5):
                                        xy(cap_center_xy[0], cap_center_xy[1]):
                                        setsize(cap_width, cap_draw_height):
                                        customtexturerect(u0, v0, u1, v1):
                                        diffuse(
                                            hold_diffuse[0],
                                            hold_diffuse[1],
                                            hold_diffuse[2],
                                            hold_diffuse[3] * cap_alpha
                                        ):
                                        rotationy(note_rotation_y):
                                        rotationz(cap_rotation):
                                        z(Z_HOLD_CAP)
                                    ),
                                    cap_world_z,
                                ));
                            }
                            if cap_glow > f32::EPSILON {
                                actors.push(actor_with_world_z(
                                    act!(sprite(cap_slot.texture_key_handle()):
                                        align(0.5, 0.5):
                                        xy(cap_center_xy[0], cap_center_xy[1]):
                                        setsize(cap_width, cap_draw_height):
                                        customtexturerect(u0, v0, u1, v1):
                                        diffuse(1.0, 1.0, 1.0, 0.0):
                                        glow(1.0, 1.0, 1.0, cap_glow):
                                        rotationy(note_rotation_y):
                                        rotationz(cap_rotation):
                                        z(Z_HOLD_GLOW)
                                    ),
                                    cap_world_z,
                                ));
                            }
                        }
                    }
                }
            }
            if draw_body_or_cap && let Some(cap_slot) = bottom_cap_slot {
                let tail_position = y_tail + 1.0;
                if tail_position > -400.0 && tail_position < screen_height() + 400.0 {
                    let cap_frame = cap_slot.frame_index_from_phase(hold_bottomcap_phase);
                    let cap_uv_elapsed = if cap_slot.model.is_some() {
                        hold_bottomcap_phase
                    } else {
                        elapsed_screen
                    };
                    let cap_uv = maybe_flip_uv_vert(
                        translated_uv_rect(
                            cap_slot.uv_for_frame_at(cap_frame, cap_uv_elapsed),
                            ns.part_uv_translation(hold_parts.bottomcap, note.beat, false),
                        ),
                        body_flipped,
                    );
                    let cap_uv = maybe_mirror_uv_horiz_for_reverse_flipped(
                        cap_uv,
                        lane_reverse,
                        body_flipped,
                    );
                    let cap_size = scale_cap_to_arrow(cap_slot.size(), hold_target_arrow_px);
                    let cap_width = cap_size[0];
                    let cap_span = cap_size[1];
                    let u0 = cap_uv[0];
                    let u1 = cap_uv[2];
                    let v_base0 = cap_uv[1];
                    let v_base1 = cap_uv[3];
                    // Prefer attaching to rendered body edge when available; fall
                    // back to native tail anchoring for collapsed micro-holds.
                    let Some((raw_top, raw_bottom)) = hold_tail_cap_bounds(
                        y_tail + 1.0,
                        cap_span,
                        rendered_body_top,
                        rendered_body_bottom,
                    ) else {
                        return;
                    };
                    if cap_span <= f32::EPSILON {
                        return;
                    }

                    // ITG DrawHoldPart bottom-cap UV progression:
                    // add_to_tex_coord = (frame_h - visible_h / zoom) / frame_h, clamped at 0.
                    // In our renderer cap_span is already zoomed size, so this reduces to
                    // add_to_tex_coord = 1 - visible_h / cap_span.
                    let mut draw_top = raw_top;
                    let draw_bottom = raw_bottom;
                    if y_head > draw_top {
                        draw_top = y_head.min(draw_bottom);
                    }
                    let draw_height = draw_bottom - draw_top;
                    let anchor_to_top = lane_reverse && note_display.top_hold_anchor_when_reverse;
                    let Some((v0, v1)) = bottom_cap_uv_window(
                        v_base0,
                        v_base1,
                        draw_height,
                        cap_span,
                        anchor_to_top,
                    ) else {
                        return;
                    };
                    let cap_center = (draw_top + draw_bottom) * 0.5;
                    if draw_height > f32::EPSILON {
                        let cap_center_travel = travel.adjusted_from_screen_y(
                            local_col,
                            lane_receptor_y,
                            dir,
                            cap_center,
                        );
                        let cap_alpha = note_actor_alpha(
                            cap_center_travel + travel.lane_offset(local_col),
                            elapsed_screen,
                            mini,
                            appearance,
                        );
                        let cap_glow = itg_actor_glow_alpha(note_glow(
                            cap_center_travel + travel.lane_offset(local_col),
                            elapsed_screen,
                            mini,
                            appearance,
                        ));
                        if cap_alpha <= f32::EPSILON && cap_glow <= f32::EPSILON {
                            return;
                        }
                        let cap_top_travel = travel.adjusted_from_screen_y(
                            local_col,
                            lane_receptor_y,
                            dir,
                            draw_top,
                        );
                        let cap_bottom_travel = travel.adjusted_from_screen_y(
                            local_col,
                            lane_receptor_y,
                            dir,
                            draw_bottom,
                        );
                        let cap_top_x =
                            lane_center_x_from_adjusted_travel(local_col, cap_top_travel);
                        let cap_bottom_x =
                            lane_center_x_from_adjusted_travel(local_col, cap_bottom_travel);
                        let (cap_center_xy, cap_draw_height, cap_rotation) =
                            hold_segment_pose([cap_top_x, draw_top], [cap_bottom_x, draw_bottom]);
                        if cap_draw_height <= f32::EPSILON {
                            return;
                        }
                        let use_bottom_cap_mesh = !use_legacy_hold_sprites
                            && cap_slot.model.is_none()
                            && !lane_reverse
                            && !hold_y_rotation_active;
                        if use_bottom_cap_mesh {
                            let top_alpha = note_actor_alpha(
                                cap_top_travel + travel.lane_offset(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            );
                            let bottom_alpha = note_actor_alpha(
                                cap_bottom_travel + travel.lane_offset(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            );
                            let top_glow = itg_actor_glow_alpha(note_glow(
                                cap_top_travel + travel.lane_offset(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            ));
                            let bottom_glow = itg_actor_glow_alpha(note_glow(
                                cap_bottom_travel + travel.lane_offset(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            ));
                            let cap_forward = [cap_bottom_x - cap_top_x, draw_bottom - draw_top];
                            let top_half_width =
                                hold_arrow_px_for_adjusted_travel(cap_top_travel) * 0.5;
                            let bottom_half_width =
                                hold_arrow_px_for_adjusted_travel(cap_bottom_travel) * 0.5;
                            let cap_top_z = world_z_for_adjusted_travel(local_col, cap_top_travel);
                            let cap_bottom_z =
                                world_z_for_adjusted_travel(local_col, cap_bottom_travel);
                            let top_row = if let Some(body_tail_row) = body_tail_row {
                                hold_strip_row_from_positions(
                                    body_tail_row[0],
                                    body_tail_row[1],
                                    u0,
                                    u1,
                                    v0,
                                    [
                                        hold_diffuse[0],
                                        hold_diffuse[1],
                                        hold_diffuse[2],
                                        hold_diffuse[3] * top_alpha,
                                    ],
                                )
                            } else {
                                hold_strip_row_3d(
                                    [cap_top_x, draw_top, cap_top_z],
                                    cap_forward,
                                    top_half_width,
                                    u0,
                                    u1,
                                    v0,
                                    [
                                        hold_diffuse[0],
                                        hold_diffuse[1],
                                        hold_diffuse[2],
                                        hold_diffuse[3] * top_alpha,
                                    ],
                                )
                            };
                            let bottom_row = hold_strip_row_3d(
                                [cap_bottom_x, draw_bottom, cap_bottom_z],
                                cap_forward,
                                bottom_half_width,
                                u0,
                                u1,
                                v1,
                                [
                                    hold_diffuse[0],
                                    hold_diffuse[1],
                                    hold_diffuse[2],
                                    hold_diffuse[3] * bottom_alpha,
                                ],
                            );
                            if top_alpha > f32::EPSILON || bottom_alpha > f32::EPSILON {
                                actors.push(hold_strip_actor(
                                    cap_slot.texture_key_shared(),
                                    Arc::new(hold_strip_quad(top_row, bottom_row)),
                                    BlendMode::Alpha,
                                    hold_depth_test,
                                    Z_HOLD_CAP as i16,
                                ));
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
                                actors.push(hold_strip_glow_actor(
                                    cap_slot.texture_key_shared(),
                                    Arc::new(hold_strip_quad(top_glow_row, bottom_glow_row)),
                                    hold_depth_test,
                                    Z_HOLD_GLOW as i16,
                                ));
                            }
                        } else {
                            let cap_world_z =
                                world_z_for_adjusted_travel(local_col, cap_center_travel);
                            if cap_alpha > f32::EPSILON {
                                actors.push(actor_with_world_z(
                                    act!(sprite(cap_slot.texture_key_handle()):
                                        align(0.5, 0.5):
                                        xy(cap_center_xy[0], cap_center_xy[1]):
                                        setsize(cap_width, cap_draw_height):
                                        customtexturerect(u0, v0, u1, v1):
                                        diffuse(
                                            hold_diffuse[0],
                                            hold_diffuse[1],
                                            hold_diffuse[2],
                                            hold_diffuse[3] * cap_alpha
                                        ):
                                        rotationy(note_rotation_y):
                                        rotationz(cap_rotation):
                                        z(Z_HOLD_CAP)
                                    ),
                                    cap_world_z,
                                ));
                            }
                            if cap_glow > f32::EPSILON {
                                actors.push(actor_with_world_z(
                                    act!(sprite(cap_slot.texture_key_handle()):
                                        align(0.5, 0.5):
                                        xy(cap_center_xy[0], cap_center_xy[1]):
                                        setsize(cap_width, cap_draw_height):
                                        customtexturerect(u0, v0, u1, v1):
                                        diffuse(1.0, 1.0, 1.0, 0.0):
                                        glow(1.0, 1.0, 1.0, cap_glow):
                                        rotationy(note_rotation_y):
                                        rotationz(cap_rotation):
                                        z(Z_HOLD_GLOW)
                                    ),
                                    cap_world_z,
                                ));
                            }
                        }
                    }
                }
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
                            z: Z_TAP_NOTE as i16,
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
                        let layer_z = Z_TAP_NOTE;
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
                            z: Z_TAP_NOTE as i16,
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
                                    circle_reference[0] * MINE_CORE_SIZE_RATIO,
                                    circle_reference[1] * MINE_CORE_SIZE_RATIO,
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
                                note_z: Z_TAP_NOTE as i16,
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
                                        z: Z_TAP_NOTE as i16,
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
                                    z: Z_TAP_NOTE as i16,
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
                            let layer_z = Z_TAP_NOTE;
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
                                z: Z_TAP_NOTE as i16,
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
