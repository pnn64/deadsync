use crate::assets;
use crate::notefield_style::notefield_style;
use crate::screens::gameplay::{GameplayCoreState as State, GameplayNoteskinAssets};
use deadlib_present::actors::{Actor, ActorResourceArena, IntoTextureKey};
use deadlib_present::color;
use deadlib_present::space::*;
use deadsync_assets::noteskin::SpriteSlot;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::{
    FantasticWindowOptions, GameplayErrorBarTrim, TapExplosionOptions, blue_fantastic_window_ms,
    gameplay_error_bar_trim_max_window_ix, hold_explosion_enabled_for_options,
};
use deadsync_notefield::{
    BuiltNotefield, ComboHudFrame, ComboMilestoneAssets, CounterHudFrame, ErrorBarHudFrame,
    ErrorBarModes, HoldMeshScratch, IndicatorSprite, JudgmentHudFrame, LayoutMiniIndicatorPosition,
    MeasureCounterOptions, MeasureLineMode, MiniHudFrame, ModelMeshCache, NotefieldChartView,
    NotefieldComposeRequest, NotefieldFeedbackFrameView, NotefieldFieldFrameView,
    NotefieldFrameFeatures, NotefieldGeometry, NotefieldHudFrameView, NotefieldLaneFeedback,
    NotefieldNoteskinView, NotefieldOptions, NotefieldPlacementScratch, NotefieldSongLuaView,
    NotefieldVisualState, TapJudgmentHudFrame, TapJudgmentSprite, ZmodLayoutParams,
    compose_notefield_field, compose_notefield_hud, prepare_notefield,
};
use deadsync_notefield::{FieldPlacement, ProxyCaptureRequests, ViewOverride};
use deadsync_profile as profile_data;
use deadsync_theme::NotefieldStyle;
use std::array::from_fn;
use std::cell::RefCell;

use super::display_mods::{self, DisplayModsFrame};

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

// Gameplay Layout & Feel
const TARGET_ARROW_PIXEL_SIZE: f32 = 64.0; // Dance lane width for hold bodies and square fallback visuals

const TEXT_CACHE_LIMIT: usize = 8192;
const COMBO_PREWARM_CAP: u32 = 2048;
const MEASURE_PREWARM_CAP: i32 = 64;
const COLUMN_COUNTDOWN_PREWARM_CAP: i32 = 64;
const RUN_TIMER_PREWARM_CAP_S: i32 = 600;

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

pub(crate) fn compose_frame(
    state: &State,
    player_idx: usize,
    arrow_effect_time_s: f32,
    noteskin_assets: &GameplayNoteskinAssets,
    visual_effects: &crate::visual_styles::EffectAssets,
    actor_resources: &ActorResourceArena,
    model_caches: &[RefCell<ModelMeshCache>; MAX_PLAYERS],
    hold_mesh_scratch: &[RefCell<HoldMeshScratch>; MAX_PLAYERS],
    placement_scratch: &[RefCell<NotefieldPlacementScratch>; MAX_PLAYERS],
    profile: &profile_data::Profile,
    placement: FieldPlacement,
    play_style: profile_data::PlayStyle,
    center_1player_notefield: bool,
    capture_requests: ProxyCaptureRequests,
    warn_cmod_for_itl_chart: bool,
    view: ViewOverride,
    actors: &mut Vec<Actor>,
    hud_actors: &mut Vec<Actor>,
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
    let frame_plan = prepared.frame_plan;
    let col_start = frame_plan.col_start;
    let num_cols = frame_plan.num_cols;
    let field = prepared.field;
    let playfield_center_x = field.playfield_center_x;
    let layout_center_x = field.layout_center_x;
    let notefield_offset_y = field.notefield_offset_y;
    let mc_font_name = zmod_small_combo_font(profile.combo_font);
    let blind_active = prepared.blind_active;

    let noteskin_sprite_source = |slot: &SpriteSlot| slot.actor_texture_source(actor_resources);
    let feedback_frame = NotefieldFeedbackFrameView {
        column_cues: options
            .frame_features
            .column_cues
            .then(|| state.column_cues(player_idx)),
        crossover_cues: options
            .frame_features
            .crossover_cues
            .then(|| state.crossover_cues(player_idx)),
        crossover_cue_entries: options
            .frame_features
            .crossover_cues
            .then(|| state.crossover_cue_entries(player_idx)),
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
    let field_frame = NotefieldFieldFrameView {
        feedback: feedback_frame,
        completed_rows: state.completed_row_visibility(player_idx),
    };
    let field_result = compose_notefield_field(
        actors,
        hud_actors,
        &mut model_cache,
        &mut hold_mesh_scratch[player_idx].borrow_mut(),
        &mut placement_scratch[player_idx].borrow_mut(),
        &request,
        &prepared,
        &field_frame,
        &noteskin_sprite_source,
    );
    display_mods::compose(
        hud_actors,
        state,
        player_idx,
        DisplayModsFrame {
            hidden: request.view.hide_display_mods,
            warn_cmod_for_itl_chart,
            elapsed_screen_s: elapsed_screen,
            playfield_center_x,
            notefield_offset_y,
        },
    );

    let show_combo =
        !request.view.hide_combo && !blind_active && options.frame_features.combo_visible;
    let milestone_assets = (show_combo
        && !options.hide_combo_explosions
        && !p.combo_milestones.is_empty())
    .then(|| {
        let combo_splode_tex = visual_effects.combo_100milestone_splode;
        let combo_minisplode_tex = visual_effects.combo_100milestone_minisplode;
        let combo_swoosh_tex = visual_effects.combo_1000milestone_swoosh;
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
    let combo_frame = ComboHudFrame {
        milestones: &p.combo_milestones,
        milestone_assets,
        combo: p.combo,
        miss_combo: p.miss_combo,
        player_color,
        combo_color,
        font: zmod_combo_font_name(profile.combo_font),
        number_text: cached_int_u32,
    };

    let timing_windows_s = state.timing_profile_windows_s();
    let error_bar_frame = ErrorBarHudFrame {
        mono_ticks: &p.error_bar_mono_ticks,
        color_ticks: &p.error_bar_color_ticks,
        average_ticks: &p.error_bar_avg_ticks,
        color_bar_started_at: p.error_bar_color_bar_started_at,
        average_bar_started_at: p.error_bar_avg_bar_started_at,
        flash_early: &p.error_bar_color_flash_early,
        flash_late: &p.error_bar_color_flash_late,
        timing_windows_s,
        offset_indicator: p.offset_indicator_text,
        long_average_tick: p.error_bar_long_avg_tick,
        long_average_active: p.error_bar_long_avg_visible,
        text: p.error_bar_text,
        offset_text: cached_offset_ms,
        text_label: cached_error_bar_text_label,
    };

    let display_beat = request.visual.current_display_beat;
    let counter_frame = options.measure_counter.map(|_| CounterHudFrame {
        segments: state.measure_counter_segments(player_idx),
        current_bpm: state.timing().get_bpm_for_beat(display_beat),
        font: mc_font_name,
        counter_text: cached_zmod_measure_counter_text,
        timer_text: zmod_run_timer_fmt,
    });
    let mini_frame =
        zmod_mini_indicator_text(state, p, profile, player_idx).map(|(text, color)| MiniHudFrame {
            text,
            color,
            failed: p.is_failing || p.life <= 0.0,
            font: mc_font_name,
        });

    let held_misses = state.held_miss_judgments_for_columns(col_start, num_cols);
    let hold_judgments = state.hold_judgments_for_columns(col_start, num_cols);
    let tap = if !blind_active
        && let Some(render) = p.last_judgment.as_ref()
        && let Some(texture) = judgment_texture
    {
        let (frame_cols, frame_rows) = assets::parse_sprite_sheet_dims(texture.key.as_ref());
        Some(TapJudgmentHudFrame {
            render,
            sprite: TapJudgmentSprite {
                source: texture.actor_texture_source(actor_resources),
                frame_size: judgment_frame_size(texture.key.as_ref()),
                frame_cols: frame_cols as usize,
            },
            frame_rows: frame_rows as usize,
        })
    } else {
        None
    };
    let held_miss_sprite = (!blind_active && held_misses.iter().any(Option::is_some))
        .then(|| {
            held_miss_texture.map(|texture| IndicatorSprite {
                source: texture.actor_texture_source(actor_resources),
                scale: if assets::parse_texture_hints(texture.key.as_ref()).doubleres {
                    0.5
                } else {
                    1.0
                },
            })
        })
        .flatten();
    let hold_sprite = (!blind_active && hold_judgments.iter().any(Option::is_some))
        .then(|| hold_judgment_texture.map(|texture| texture.actor_texture_source(actor_resources)))
        .flatten();
    let hud_frame = NotefieldHudFrameView {
        combo: combo_frame,
        error_bar: error_bar_frame,
        counter: counter_frame,
        mini: mini_frame,
        judgment: JudgmentHudFrame {
            tap,
            held_misses,
            held_miss_sprite,
            hold_judgments,
            hold_sprite,
        },
    };
    let hud_result = compose_notefield_hud(hud_actors, &request, &prepared, &hud_frame);

    BuiltNotefield {
        layout_center_x,
        field_actors: field_result.captured_actors,
        judgment_actors: hud_result.judgment_actors,
        combo_actors: hud_result.combo_actors,
    }
}

pub(crate) fn prewarm_actor_resources(
    arena: &ActorResourceArena,
    noteskin_assets: &GameplayNoteskinAssets,
    profiles: &[profile_data::Profile; MAX_PLAYERS],
    num_players: usize,
) {
    for noteskins in [
        &noteskin_assets.noteskin,
        &noteskin_assets.mine_noteskin,
        &noteskin_assets.receptor_noteskin,
        &noteskin_assets.tap_explosion_noteskin,
    ] {
        for noteskin in noteskins.iter().take(num_players).flatten() {
            noteskin.for_each_slot(|slot| {
                let _ = slot.actor_texture_source(arena);
            });
        }
    }

    for profile in profiles.iter().take(num_players) {
        for texture in [
            resolved_judgment_texture(profile),
            resolved_hold_judgment_texture(profile),
            resolved_held_miss_texture(profile),
        ]
        .into_iter()
        .flatten()
        {
            let _ = texture.actor_texture_source(arena);
        }
    }
    arena.lock_growth();
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
